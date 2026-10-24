//! Opt-in structured bug reports; never inspect conversation databases or credentials.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path, time::Duration};
use swarm_platform::CredentialStore;

pub const ENDPOINT: &str = "https://swarm-bug-reports.volvcane.workers.dev/v1/reports";
pub const CATEGORIES: &[&str] = &[
    "permissions",
    "accounts",
    "gui",
    "cli",
    "runtime",
    "data",
    "backup_restore",
    "notifications",
    "updates",
    "installation",
    "ai_integration",
    "security",
    "other",
];
pub const SEVERITIES: &[&str] = &["critical", "high", "medium", "low"];
type Result<T> = std::result::Result<T, &'static str>;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema_version: u32,
    pub report_id: String,
    pub app_version: String,
    pub category: String,
    pub severity: String,
    pub summary: String,
    pub steps: Vec<String>,
    pub expected: String,
    pub actual: String,
    pub severity_reason: String,
    pub error_code: String,
    pub reporter: String,
    pub redacted: bool,
    pub consent_version: u32,
}
impl Report {
    pub fn validate(&self) -> Result<()> {
        let core = self.app_version.split(['-', '+']).next().unwrap_or("");
        let version_parts: Vec<_> = core.split('.').collect();
        if self.schema_version != 1
            || version_parts.len() != 3
            || version_parts
                .iter()
                .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
            || self.consent_version != 1
            || !self.redacted
            || uuid::Uuid::parse_str(&self.report_id)
                .ok()
                .is_none_or(|id| id.get_version_num() != 4 || id.to_string() != self.report_id)
            || !CATEGORIES.contains(&self.category.as_str())
            || !SEVERITIES.contains(&self.severity.as_str())
            || !["ai", "human"].contains(&self.reporter.as_str())
            || self.steps.is_empty()
            || self.steps.len() > 12
            || self.error_code.len() > 64
            || !self
                .error_code
                .bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
            || self.app_version.is_empty()
            || self.app_version.len() > 64
            || !self
                .app_version
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b".-+".contains(&c))
        {
            return Err("INVALID_REPORT");
        }
        for (value, limit) in [
            (&self.summary, 160),
            (&self.expected, 2000),
            (&self.actual, 2000),
            (&self.severity_reason, 500),
        ] {
            if value.trim().is_empty() || value.encode_utf16().count() > limit || sensitive(value) {
                return Err("INVALID_OR_SENSITIVE_REPORT");
            }
        }
        for step in &self.steps {
            if step.trim().is_empty() || step.encode_utf16().count() > 500 || sensitive(step) {
                return Err("INVALID_OR_SENSITIVE_REPORT");
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| "INVALID_REPORT")?
            .len()
            > 16384
        {
            return Err("REPORT_TOO_LARGE");
        }
        Ok(())
    }
}
fn sensitive(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    let compact: String = lower.chars().filter(|c| !c.is_whitespace()).collect();
    if raw
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
        || lower.contains("private key-----")
        || [
            "bearer ",
            "sk-proj-",
            "sk-ant-",
            "ghp_",
            "gho_",
            "password=",
            "password:",
            "recovery_code=",
            "access_token=",
            "client_secret=",
            "/users/",
            "/home/",
        ]
        .iter()
        .any(|p| lower.contains(p))
        || [
            "password=",
            "password:",
            "recovery_code=",
            "recovery-code=",
            "recoverycode=",
            "access_token=",
            "access-token=",
            "accesstoken=",
            "client_secret=",
            "client-secret=",
            "clientsecret=",
            "recovery_code:",
            "access_token:",
            "client_secret:",
        ]
        .iter()
        .any(|p| compact.contains(p))
        || raw
            .as_bytes()
            .windows(3)
            .any(|w| w[0].is_ascii_alphabetic() && w[1] == b':' && [b'/', b'\\'].contains(&w[2]))
        || raw.contains("\\\\")
        || raw
            .split_whitespace()
            .any(|word| word.contains('@') && word.contains('.'))
    {
        return true;
    }
    false
}
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    consent_version: u32,
    #[serde(default)]
    pending: Vec<String>,
    #[serde(default)]
    sent: Vec<(String, String)>,
}
fn open(root: &Path) -> Result<(CredentialStore, std::fs::File, State)> {
    let store = CredentialStore::open(&root.join("bug-reports")).map_err(|_| "REPORT_STORAGE")?;
    let lease = store
        .private_lease("reports.lease")
        .map_err(|_| "REPORT_BUSY")?;
    let state = match store.get("state").map_err(|_| "REPORT_STORAGE")? {
        Some(bytes) => serde_json::from_slice(&bytes).map_err(|_| "REPORT_STORAGE")?,
        None => State::default(),
    };
    Ok((store, lease, state))
}
fn save(store: &CredentialStore, state: &State) -> Result<()> {
    store
        .put(
            "state",
            &serde_json::to_vec(state).map_err(|_| "REPORT_STORAGE")?,
        )
        .map_err(|_| "REPORT_STORAGE")
}
fn snapshot(state: &State) -> Value {
    json!({"enabled":state.enabled && state.consent_version == 1,"pending_count":state.pending.len(),"pending_ids":state.pending,"recent_sent":state.sent.iter().rev().take(20).map(|(id,_)|id).collect::<Vec<_>>(),"endpoint":ENDPOINT,"consent_version":1})
}
pub fn status(root: &Path) -> Result<Value> {
    let (_, _, state) = open(root)?;
    Ok(snapshot(&state))
}
/// Caller must be an authenticated human Admin; no AI CLI consent command exists.
pub fn set_consent(root: &Path, enabled: bool) -> Result<Value> {
    let (store, _lease, mut state) = open(root)?;
    state.enabled = enabled;
    state.consent_version = if enabled { 1 } else { 0 };
    save(&store, &state)?;
    Ok(snapshot(&state))
}
pub fn submit(root: &Path, report: Report) -> Result<Value> {
    report.validate()?;
    let bytes = serde_json::to_vec(&report).map_err(|_| "INVALID_REPORT")?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let (store, lease, mut state) = open(root)?;
    if let Some((_, old)) = state.sent.iter().find(|(id, _)| id == &report.report_id) {
        return if old == &digest {
            Ok(json!({"report_id":report.report_id,"delivery":"sent"}))
        } else {
            Err("REPORT_ID_CONFLICT")
        };
    }
    let key = format!("report-{}", report.report_id);
    if let Some(old) = store.get(&key).map_err(|_| "REPORT_STORAGE")?
        && old != bytes
    {
        return Err("REPORT_ID_CONFLICT");
    }
    if !state.pending.contains(&report.report_id) {
        if state.pending.len() >= 200 {
            return Err("REPORT_QUEUE_FULL");
        }
        // Index first: a partial write remains visible and can be repaired by resubmitting the same report.
        state.pending.push(report.report_id.clone());
        save(&store, &state)?;
    }
    store.put(&key, &bytes).map_err(|_| "REPORT_STORAGE")?;
    drop(lease);
    drop(store);
    let _ = flush(root);
    let (_, _, state) = open(root)?;
    Ok(
        json!({"report_id":report.report_id,"delivery":if state.pending.contains(&report.report_id){"queued"}else{"sent"},"consent_enabled":state.enabled}),
    )
}
pub fn flush(root: &Path) -> Result<Value> {
    let (store, _lease, mut state) = open(root)?;
    if !state.enabled || state.consent_version != 1 || state.pending.is_empty() {
        return Ok(snapshot(&state));
    }
    // Read only: never launch the runtime or clear a human full stop.
    swarm_runtime::lifecycle::check_running_allowed(root).map_err(|_| "REPORT_UPLOAD_PAUSED")?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Swarm-bug-report/1")
        .build()
        .map_err(|_| "REPORT_NETWORK")?;
    for id in state.pending.clone().into_iter().take(3) {
        let key = format!("report-{id}");
        let Some(bytes) = store.get(&key).map_err(|_| "REPORT_STORAGE")? else {
            return Err("REPORT_STORAGE");
        };
        let report: Report = serde_json::from_slice(&bytes).map_err(|_| "REPORT_STORAGE")?;
        report.validate()?;
        swarm_runtime::lifecycle::check_running_allowed(root)
            .map_err(|_| "REPORT_UPLOAD_PAUSED")?;
        let response = match client
            .post(ENDPOINT)
            .header("content-type", "application/json")
            .body(bytes.clone())
            .send()
        {
            Ok(r) if r.status().is_success() => r,
            _ => break,
        };
        let mut result = Vec::new();
        response
            .take(2049)
            .read_to_end(&mut result)
            .map_err(|_| "REPORT_NETWORK")?;
        if result.len() > 2048 {
            break;
        }
        let Ok(ack) = serde_json::from_slice::<Value>(&result) else {
            break;
        };
        if ack["accepted"] != true || ack["report_id"] != id {
            break;
        }
        state.pending.retain(|pending| pending != &id);
        state
            .sent
            .push((id, format!("{:x}", Sha256::digest(&bytes))));
        if state.sent.len() > 100 {
            state.sent.remove(0);
        }
        // Save the acknowledgement before clearing the body; retry after any crash remains idempotent.
        save(&store, &state)?;
        store.remove(&key).map_err(|_| "REPORT_STORAGE")?;
    }
    Ok(snapshot(&state))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    fn sample() -> Report {
        Report {
            schema_version: 1,
            report_id: uuid::Uuid::new_v4().to_string(),
            app_version: "0.1.0".into(),
            category: "gui".into(),
            severity: "high".into(),
            summary: "Sample GUI issue".into(),
            steps: vec!["Open sample".into()],
            expected: "Visible controls".into(),
            actual: "Controls overlap".into(),
            severity_reason: "Primary action is blocked".into(),
            error_code: String::new(),
            reporter: "ai".into(),
            redacted: true,
            consent_version: 1,
        }
    }
    #[test]
    fn consent_defaults_off_and_report_survives_without_network_or_runtime() {
        let root = tempfile::tempdir().unwrap();
        let report = sample();
        let id = report.report_id.clone();
        assert_eq!(
            submit(root.path(), report.clone()).unwrap()["delivery"],
            "queued"
        );
        assert_eq!(submit(root.path(), report).unwrap()["report_id"], id);
        assert_eq!(status(root.path()).unwrap()["pending_count"], 1);
        assert_eq!(flush(root.path()).unwrap()["enabled"], false);
        let mut wrong = sample();
        wrong.report_id = id;
        wrong.actual = "Different observation".into();
        assert_eq!(
            submit(root.path(), wrong).unwrap_err(),
            "REPORT_ID_CONFLICT"
        );
        assert!(!root.path().join("swarm.db").exists());
    }
    #[test]
    fn credentials_paths_and_invalid_severity_are_rejected() {
        for text in [
            "password=secret",
            "password = secret",
            "C:/private/project",
            "Bearer abc123",
            "owner@example.test",
        ] {
            let mut r = sample();
            r.actual = text.into();
            assert!(r.validate().is_err());
        }
        let mut r = sample();
        r.severity = "urgent".into();
        assert!(r.validate().is_err());
    }
    #[test]
    fn full_stop_preserves_queue_and_versioned_consent() {
        let root = tempfile::tempdir().unwrap();
        submit(root.path(), sample()).unwrap();
        set_consent(root.path(), true).unwrap();
        let lifecycle = CredentialStore::open(&root.path().join("lifecycle")).unwrap();
        lifecycle
            .put(
                "full-stop",
                format!("2:{}", uuid::Uuid::new_v4()).as_bytes(),
            )
            .unwrap();
        assert_eq!(flush(root.path()).unwrap_err(), "REPORT_UPLOAD_PAUSED");
        assert_eq!(status(root.path()).unwrap()["pending_count"], 1);
        assert!(swarm_runtime::lifecycle::check_running_allowed(root.path()).is_err());
        set_consent(root.path(), false).unwrap();
        assert_eq!(flush(root.path()).unwrap()["enabled"], false);
    }
}
