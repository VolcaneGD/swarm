//! Owned desktop toast identity and bounded protocol activation.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[cfg(windows)]
mod native;
#[cfg(windows)]
pub use native::Registration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationError {
    InvalidInput,
    Conflict,
    Storage,
    Disabled,
    WindowsApi,
    Unsupported,
}
impl std::fmt::Display for NotificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "notification operation failed: {self:?}")
    }
}
impl std::error::Error for NotificationError {}

#[derive(Clone, PartialEq, Eq)]
pub struct Identity {
    pub(crate) root: PathBuf,
    pub(crate) executable: PathBuf,
    pub(crate) suffix: String,
}
impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationIdentity([redacted])")
    }
}
impl Identity {
    pub fn new(root: &Path, executable: &Path) -> Result<Self, NotificationError> {
        if !root.is_absolute() || !executable.is_absolute() {
            return Err(NotificationError::InvalidInput);
        }
        let root = std::fs::canonicalize(root).map_err(|_| NotificationError::Storage)?;
        let executable =
            std::fs::canonicalize(executable).map_err(|_| NotificationError::Storage)?;
        if !root.is_dir() || !executable.is_file() {
            return Err(NotificationError::InvalidInput);
        }
        #[cfg(windows)]
        let bytes = {
            use std::os::windows::ffi::OsStrExt;
            root.as_os_str()
                .encode_wide()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
        };
        #[cfg(not(windows))]
        let bytes = root.as_os_str().as_encoded_bytes().to_vec();
        Ok(Self {
            root,
            executable,
            suffix: format!("{:x}", Sha256::digest(bytes)),
        })
    }
    pub fn app_id(&self) -> String {
        format!("dev.swarm.local.{}", self.suffix)
    }
    pub fn scheme(&self) -> String {
        format!("swarm-{}", self.suffix)
    }
    pub fn activation_uri(&self, target: &ActivationTarget) -> String {
        format!(
            "{}://thread/{}/{}",
            self.scheme(),
            target.account_id,
            target.thread_id
        )
    }
    pub fn parse_activation(&self, uri: &str) -> Result<ActivationTarget, NotificationError> {
        let suffix = uri
            .strip_prefix(&format!("{}://thread/", self.scheme()))
            .ok_or(NotificationError::InvalidInput)?;
        let (account, thread) = suffix
            .split_once('/')
            .ok_or(NotificationError::InvalidInput)?;
        ActivationTarget::new(account, thread)
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ActivationTarget {
    account_id: String,
    thread_id: String,
}
impl std::fmt::Debug for ActivationTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationTarget([redacted])")
    }
}
impl ActivationTarget {
    pub fn new(account: &str, thread: &str) -> Result<Self, NotificationError> {
        let canonical = |text: &str| {
            uuid::Uuid::parse_str(text).is_ok_and(|id| id.hyphenated().to_string() == text)
        };
        if !canonical(account) || !canonical(thread) {
            return Err(NotificationError::InvalidInput);
        }
        Ok(Self {
            account_id: account.into(),
            thread_id: thread.into(),
        })
    }
    pub fn account_id(&self) -> &str {
        &self.account_id
    }
    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }
}

pub struct ToastContent {
    pub(crate) app_id: String,
    pub(crate) tag: String,
    pub(crate) xml: String,
    pub(crate) suppress_popup: bool,
}
impl std::fmt::Debug for ToastContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ToastContent([redacted])")
    }
}
impl ToastContent {
    /// Explicit diagnostics may verify history without showing a popup banner.
    pub fn without_popup(mut self) -> Self {
        self.suppress_popup = true;
        self
    }

    pub fn new(
        identity: &Identity,
        target: &ActivationTarget,
        batch_id: &str,
        posts: usize,
        responses: usize,
        excerpt: Option<&str>,
        japanese: bool,
    ) -> Result<Self, NotificationError> {
        let id = batch_id
            .strip_prefix("batch:")
            .ok_or(NotificationError::InvalidInput)?;
        if uuid::Uuid::parse_str(id).is_err()
            || batch_id.len() != 42
            || posts.saturating_add(responses) == 0
            || posts.saturating_add(responses) > 200
        {
            return Err(NotificationError::InvalidInput);
        }
        if excerpt.is_some_and(|text| text.chars().count() > 241) {
            return Err(NotificationError::InvalidInput);
        }
        let summary = if japanese {
            format!("新着投稿: {posts}件・返信: {responses}件")
        } else {
            format!("New posts: {posts} · Responses: {responses}")
        };
        let excerpt = excerpt
            .map(|text| format!("<text>{}</text>", escape_xml(text)))
            .unwrap_or_default();
        Ok(Self {
            app_id: identity.app_id(),
            tag: batch_id.into(),
            suppress_popup: false,
            xml: format!(
                "<toast activationType=\"protocol\" launch=\"{}\"><visual><binding template=\"ToastGeneric\"><text>Swarm</text><text>{}</text>{excerpt}</binding></visual></toast>",
                identity.activation_uri(target),
                escape_xml(&summary)
            ),
        })
    }
}
fn escape_xml(text: &str) -> String {
    let mut output = String::new();
    for ch in text.chars().filter(|&ch| {
        ch == '\t'
            || ch == '\n'
            || ch == '\r'
            || ('\u{20}'..='\u{d7ff}').contains(&ch)
            || ('\u{e000}'..='\u{fffd}').contains(&ch)
            || ch >= '\u{10000}'
    }) {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            _ => output.push(ch),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_activation_cannot_change_root_or_inject_arguments() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let exe = std::env::current_exe().unwrap();
        let identity = Identity::new(root.path(), &exe).unwrap();
        let other = Identity::new(other.path(), &exe).unwrap();
        let target = ActivationTarget::new(
            &uuid::Uuid::new_v4().to_string(),
            &uuid::Uuid::new_v4().to_string(),
        )
        .unwrap();
        let uri = identity.activation_uri(&target);
        assert_eq!(identity.parse_activation(&uri).unwrap(), target);
        for invalid in [
            format!("{uri}?--data-dir=C:/other"),
            format!("{uri}/extra"),
            format!("{uri}#fragment"),
            other.activation_uri(&target),
        ] {
            assert_eq!(
                identity.parse_activation(&invalid).unwrap_err(),
                NotificationError::InvalidInput
            );
        }
    }
    #[test]
    fn content_is_private_bounded_and_escapes_untrusted_excerpt() {
        let root = tempfile::tempdir().unwrap();
        let identity = Identity::new(root.path(), &std::env::current_exe().unwrap()).unwrap();
        let target = ActivationTarget::new(
            &uuid::Uuid::new_v4().to_string(),
            &uuid::Uuid::new_v4().to_string(),
        )
        .unwrap();
        let id = format!("batch:{}", uuid::Uuid::new_v4());
        let plain = ToastContent::new(&identity, &target, &id, 2, 1, None, false).unwrap();
        assert_eq!(plain.tag, id);
        assert!(plain.xml.contains("New posts: 2 · Responses: 1"));
        let japanese = ToastContent::new(&identity, &target, &id, 2, 1, None, true).unwrap();
        assert!(japanese.xml.contains("新着投稿: 2件・返信: 1件"));
        assert!(!plain.xml.contains("secret"));
        let content = ToastContent::new(
            &identity,
            &target,
            &id,
            2,
            1,
            Some("secret<&\"\u{0}"),
            false,
        )
        .unwrap();
        assert!(content.xml.contains("secret&lt;&amp;&quot;"));
        assert!(!content.xml.contains('\0'));
        assert!(!format!("{content:?}").contains("secret"));
        assert!(ToastContent::new(&identity, &target, &id, 201, 0, None, false).is_err());
        assert!(ToastContent::new(&identity, &target, &id, 0, 0, None, false).is_err());
    }
}
