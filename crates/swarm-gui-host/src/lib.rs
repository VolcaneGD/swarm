//! Private desktop state. The renderer receives only public account metadata.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use swarm_core::protocol::{Fault, Request};
use swarm_platform::CredentialStore;
use swarm_runtime::{gui::GuiClient, lifecycle};

mod account_names;
mod bug_reports;
mod google;
mod updates;
pub use updates::{fetch_update_manifest, normalize_update_url};
mod notification_activation;
mod restore;
mod retention;
pub use notification_activation::{pending_notification_event, queue_notification_activation};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    version: u32,
    intent: String,
    alias: String,
    request: Request,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    alias: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingBackup {
    version: u32,
    account_id: String,
    alias: String,
    intent: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingCreate {
    version: u32,
    alias: String,
    operation: String,
    args: Value,
    request_id: String,
    created_at: Value,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateManifest {
    version: u32,
    group: String,
    chunks: usize,
    bytes: usize,
    sha256: String,
    #[serde(default)]
    completed: bool,
}

const CREATE_KEY: &str = "pending-create";
const CREATE_CHUNK_PREFIX: &str = "pending-create-";
const CREATE_CHUNK_BYTES: usize = 60 * 1024;
const CREATE_MAX_BYTES: usize = 8 * 1024 * 1024;
const CREATE_MAX_CHUNKS: usize = CREATE_MAX_BYTES.div_ceil(CREATE_CHUNK_BYTES);
const CREATE_MAX_BODY_BYTES: usize = 1024 * 1024;
const RULES_MAX_BYTES: usize = 1024 * 1024;
const ACCOUNT_MANAGE_PASSWORD_MAX_BYTES: usize = 1024 * 1024;
const AUTHORIZATION_TEXT_MAX_BYTES: usize = 4096;
const AUTHORIZATION_OPERATION_MAX_BYTES: usize = 128;
const CREATE_MAX_AGE_SECS: u64 = 7 * 24 * 60 * 60;

pub struct GuiHost {
    root: PathBuf,
    store: CredentialStore,
    _lease: File,
}
impl std::fmt::Debug for GuiHost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GuiHost([redacted])")
    }
}
fn storage() -> Fault {
    Fault::new(
        "GUI_STATE",
        8,
        "Private GUI state could not be saved or loaded",
    )
}
fn invalid() -> Fault {
    Fault::new("INVALID_INPUT", 2, "Invalid account input")
}
fn invalid_read() -> Fault {
    Fault::new(
        "INVALID_INPUT",
        2,
        "Operation is unavailable on the GUI read endpoint",
    )
}
fn internal() -> Fault {
    Fault::new("INTERNAL_ERROR", 8, "Storage or internal operation failed")
}
fn authentication() -> Fault {
    Fault::new(
        "AUTHENTICATION_REQUIRED",
        3,
        "A valid human login is required",
    )
}
fn conflict() -> Fault {
    Fault::new(
        "CONFLICT",
        6,
        "Resolve the pending request before changing the account or starting another operation",
    )
}
fn alias(name: &str) -> String {
    format!("account-{:x}", Sha256::digest(name.as_bytes()))
}
fn request(operation: &str, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: None,
        args,
        context: json!({}),
        request_id: None,
    }
}
fn valid_alias(value: &str) -> bool {
    value.len() == 72
        && value.starts_with("account-")
        && value[8..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Starts the runtime for an explicit human desktop launch without exposing a
/// renderer-controlled executable or path.
pub fn start_runtime(root: &Path) -> Result<(), Fault> {
    lifecycle::start_human(root)
}

fn startup_executable() -> Result<std::path::PathBuf, Fault> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("swarm-runtime.exe")))
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            Fault::new(
                "RUNTIME_STARTUP_STATE",
                8,
                "Windows startup executable is unavailable",
            )
        })
}

/// Explicit native desktop initialization, never called by ordinary host reads.
#[cfg(windows)]
pub fn initialize_startup(root: &Path) -> Result<(), Fault> {
    swarm_runtime::startup::initialize(root, &startup_executable()?)
}

/// Trusted native launch registration; renderer cannot choose the executable.
#[cfg(windows)]
pub fn initialize_notifications(root: &Path) -> Result<(), Fault> {
    let executable = std::env::current_exe().map_err(|_| storage())?;
    swarm_runtime::notifications::register(root, &executable)
}

fn create_chunk_key(group: &str, index: usize) -> String {
    format!("{CREATE_CHUNK_PREFIX}{group}-{index:03}")
}

fn create_pending(value: &PendingCreate) -> Result<Vec<u8>, Fault> {
    struct Capped(Vec<u8>);
    impl Write for Capped {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.0.len().saturating_add(bytes.len()) > CREATE_MAX_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "pending create exceeds storage limit",
                ));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut output = Capped(Vec::new());
    serde_json::to_writer(&mut output, value).map_err(|_| invalid())?;
    Ok(output.0)
}

fn create_manifest(bytes: &[u8], group: &str, chunks: usize) -> Result<Vec<u8>, Fault> {
    let value = CreateManifest {
        version: 1,
        group: group.to_owned(),
        chunks,
        bytes: bytes.len(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
        completed: false,
    };
    serde_json::to_vec(&value).map_err(|_| storage())
}

fn create_is_expired(created_at: u64) -> bool {
    let Ok(now) = SystemTime::now().duration_since(UNIX_EPOCH) else {
        return true;
    };
    created_at > now.as_secs() || now.as_secs().saturating_sub(created_at) >= CREATE_MAX_AGE_SECS
}

fn create_time_is_expired(created_at: &Value) -> bool {
    created_at.as_u64().is_none_or(create_is_expired)
}

fn valid_create_args(operation: &str, args: &Value) -> bool {
    let Some(object) = args.as_object() else {
        return false;
    };
    let allowed: &[&str] = match operation {
        "board.create" => &["name", "description"],
        "thread.create" => &["board_id", "name", "description"],
        "post.create" => &["thread_id", "body"],
        "response.create" => &["thread_id", "body", "to"],
        _ => return false,
    };
    if object.is_empty() || object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return false;
    }
    let required: &[&str] = match operation {
        "board.create" => &["name"],
        "thread.create" => &["board_id", "name"],
        "post.create" => &["thread_id", "body"],
        "response.create" => &["thread_id", "body", "to"],
        _ => return false,
    };
    if required.iter().any(|key| {
        object
            .get(*key)
            .and_then(Value::as_str)
            .is_some_and(|text| text.trim().is_empty())
            || object.get(*key).is_none()
    }) {
        return false;
    }
    for key in ["name", "description", "board_id", "thread_id"] {
        if let Some(value) = object.get(key)
            && !value.is_string()
        {
            return false;
        }
    }
    if let Some(body) = object.get("body") {
        let Some(text) = body.as_str() else {
            return false;
        };
        if text.len() > CREATE_MAX_BODY_BYTES {
            return false;
        }
    }
    if let Some(to) = object.get("to")
        && to.as_i64().is_none_or(|number| number <= 0)
    {
        return false;
    }
    true
}

fn valid_edit_args(args: &Value) -> bool {
    let Some(object) = args.as_object() else {
        return false;
    };
    if object.len() != 4
        || object
            .keys()
            .any(|key| !["thread_id", "no", "body", "if_revision"].contains(&key.as_str()))
    {
        return false;
    }
    let Some(thread_id) = object.get("thread_id").and_then(Value::as_str) else {
        return false;
    };
    let Some(no) = object.get("no").and_then(Value::as_i64) else {
        return false;
    };
    let Some(body) = object.get("body").and_then(Value::as_str) else {
        return false;
    };
    let Some(if_revision) = object.get("if_revision").and_then(Value::as_i64) else {
        return false;
    };
    !thread_id.trim().is_empty()
        && no > 0
        && !body.trim().is_empty()
        && body.len() <= CREATE_MAX_BODY_BYTES
        && if_revision > 0
}

fn valid_manage_args(operation: &str, args: &Value) -> bool {
    let Some(object) = args.as_object() else {
        return false;
    };
    let fields: &[&str] = match operation {
        "thread.pin" => &["id", "pinned"],
        "thread.subscribe" => &["id", "subscribed"],
        "notification.settings.set" => &["enabled", "include_excerpt"],
        "display.settings.set" => &["language", "theme"],
        "post.read.mark" => &["thread_id", "post_ids"],
        "board.delete" | "board.restore" | "thread.delete" | "thread.restore" => &["id"],
        "board.purge" | "thread.purge" => &["id", "confirmed"],
        "conversation.batch" => &["action", "items", "confirmed"],
        "post.delete" | "post.restore" => &["thread_id", "no"],
        _ => return false,
    };
    if object.len() != fields.len() || object.keys().any(|key| !fields.contains(&key.as_str())) {
        return false;
    }
    match operation {
        "thread.pin" => {
            object
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.trim().is_empty())
                && object.get("pinned").and_then(Value::as_bool).is_some()
        }
        "thread.subscribe" => {
            object
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.trim().is_empty() && id.len() <= 4096)
                && object.get("subscribed").and_then(Value::as_bool).is_some()
        }
        "notification.settings.set" => {
            object.get("enabled").and_then(Value::as_bool).is_some()
                && object
                    .get("include_excerpt")
                    .and_then(Value::as_bool)
                    .is_some()
        }
        "display.settings.set" => {
            serde_json::from_value::<swarm_core::DisplaySettings>(args.clone()).is_ok()
        }
        "post.read.mark" => {
            let valid_id = |key: &str| {
                object
                    .get(key)
                    .and_then(Value::as_str)
                    .is_some_and(|id| !id.trim().is_empty() && id.len() <= 4096)
            };
            let Some(post_ids) = object.get("post_ids").and_then(Value::as_array) else {
                return false;
            };
            valid_id("thread_id")
                && !post_ids.is_empty()
                && post_ids.len() <= 200
                && post_ids.iter().all(|value| {
                    value
                        .as_str()
                        .is_some_and(|id| !id.trim().is_empty() && id.len() <= 4096)
                })
        }
        "board.purge" | "thread.purge" => {
            object.get("confirmed") == Some(&json!(true))
                && object
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| !id.trim().is_empty() && id.len() <= 4096)
        }
        "conversation.batch" => {
            object.get("confirmed") == Some(&json!(true))
                && matches!(
                    object.get("action").and_then(Value::as_str),
                    Some("restore" | "purge")
                )
                && object
                    .get("items")
                    .and_then(Value::as_array)
                    .is_some_and(|items| {
                        !items.is_empty()
                            && items.iter().all(|item| {
                                item.as_object().is_some_and(|fields| fields.len() == 2)
                                    && matches!(
                                        item.get("kind").and_then(Value::as_str),
                                        Some("board" | "thread")
                                    )
                                    && item
                                        .get("id")
                                        .and_then(Value::as_str)
                                        .is_some_and(|id| !id.trim().is_empty() && id.len() <= 4096)
                            })
                    })
        }
        "board.delete" | "board.restore" | "thread.delete" | "thread.restore" => object
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| !id.trim().is_empty()),
        "post.delete" | "post.restore" => {
            object
                .get("thread_id")
                .and_then(Value::as_str)
                .is_some_and(|id| !id.trim().is_empty())
                && object
                    .get("no")
                    .and_then(Value::as_i64)
                    .is_some_and(|no| no > 0)
        }
        _ => false,
    }
}

fn valid_notification_read_args(operation: &str, args: &Value) -> bool {
    matches!(
        operation,
        "conversation.subscriptions"
            | "conversation.unread"
            | "notification.settings.get"
            | "display.settings.get"
    ) && args.as_object().is_some_and(serde_json::Map::is_empty)
}

pub fn display_defaults() -> Result<swarm_core::DisplaySettings, Fault> {
    Ok(swarm_core::DisplaySettings {
        language: if swarm_platform::display::japanese() {
            swarm_core::DisplayLanguage::Ja
        } else {
            swarm_core::DisplayLanguage::En
        },
        theme: if swarm_platform::display::dark().map_err(|_| {
            Fault::new(
                "DISPLAY_DEFAULTS",
                8,
                "Windows display defaults could not be read",
            )
        })? {
            swarm_core::DisplayTheme::Dark
        } else {
            swarm_core::DisplayTheme::Light
        },
    })
}

fn valid_account_manage_args(operation: &str, args: &Value) -> bool {
    let Some(object) = args.as_object() else {
        return false;
    };
    let fields: &[&str] = match operation {
        "account.create" => &["name", "password"],
        "account.disable" => &["id", "disabled"],
        "account.reset" => &["id", "password"],
        _ => return false,
    };
    if object.keys().any(|key| !fields.contains(&key.as_str())) {
        return false;
    }
    let bounded_text = |key: &str, max_bytes: usize| {
        object
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty() && value.len() <= max_bytes)
    };
    match operation {
        "account.create" => {
            bounded_text("name", 4096).is_some()
                && args
                    .get("password")
                    .and_then(Value::as_str)
                    .is_some_and(|value| {
                        value.len() <= ACCOUNT_MANAGE_PASSWORD_MAX_BYTES
                            && (value.is_empty() || !value.trim().is_empty())
                    })
        }
        "account.disable" => {
            bounded_text("id", 4096).is_some()
                && object.get("disabled").and_then(Value::as_bool).is_some()
        }
        "account.reset" => {
            bounded_text("id", 4096).is_some()
                && bounded_text("password", ACCOUNT_MANAGE_PASSWORD_MAX_BYTES).is_some()
        }
        _ => false,
    }
}

fn valid_authorization_args(operation: &str, args: &Value) -> bool {
    let Some(object) = args.as_object() else {
        return false;
    };
    let fields: &[&str] = match operation {
        "role.create" => &["name"],
        "role.assign" => &["account_id", "role_id", "assigned"],
        "permission.set" => &[
            "subject_type",
            "subject_id",
            "operation",
            "scope",
            "target_id",
            "decision",
        ],
        _ => return false,
    };
    if object.len() != fields.len() || object.keys().any(|key| !fields.contains(&key.as_str())) {
        return false;
    }
    let bounded_text = |key: &str, max_bytes: usize| {
        object
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty() && value.len() <= max_bytes)
    };
    match operation {
        "role.create" => bounded_text("name", AUTHORIZATION_TEXT_MAX_BYTES).is_some(),
        "role.assign" => {
            bounded_text("account_id", AUTHORIZATION_TEXT_MAX_BYTES).is_some()
                && bounded_text("role_id", AUTHORIZATION_TEXT_MAX_BYTES).is_some()
                && object.get("assigned").and_then(Value::as_bool).is_some()
        }
        "permission.set" => {
            let subject_type = bounded_text("subject_type", 16);
            let scope = bounded_text("scope", 16);
            let decision = bounded_text("decision", 16);
            let target = object
                .get("target_id")
                .and_then(Value::as_str)
                .filter(|value| value.len() <= AUTHORIZATION_TEXT_MAX_BYTES);
            subject_type.is_some_and(|value| matches!(value, "account" | "role"))
                && bounded_text("subject_id", AUTHORIZATION_TEXT_MAX_BYTES).is_some()
                && bounded_text("operation", AUTHORIZATION_OPERATION_MAX_BYTES).is_some()
                && scope.is_some_and(|value| matches!(value, "global" | "board" | "thread"))
                && target.is_some_and(|value| match scope {
                    Some("global") => value.is_empty(),
                    Some("board" | "thread") => !value.trim().is_empty(),
                    _ => false,
                })
                && decision.is_some_and(|value| matches!(value, "allow" | "deny" | "inherit"))
        }
        _ => false,
    }
}

fn valid_rules_save_args(args: &Value) -> bool {
    let Some(object) = args.as_object() else {
        return false;
    };
    if object.keys().any(|key| {
        ![
            "project_id",
            "source",
            "if_revision",
            "recovery_id",
            "recovery_revision",
        ]
        .contains(&key.as_str())
    }) {
        return false;
    }
    let Some(source) = object.get("source").and_then(Value::as_str) else {
        return false;
    };
    let Some(revision) = object.get("if_revision").and_then(Value::as_str) else {
        return false;
    };
    if source.len() > RULES_MAX_BYTES || revision.is_empty() || revision.len() > 128 {
        return false;
    }
    match (object.get("recovery_id"), object.get("recovery_revision")) {
        (None, None) => {}
        (Some(id), Some(revision))
            if valid_recovery_id(id)
                && revision
                    .as_str()
                    .is_some_and(|value| !value.is_empty() && value.len() <= 128) => {}
        _ => return false,
    }
    match object.get("project_id") {
        None => true,
        Some(Value::String(id)) => !id.trim().is_empty() && id.len() <= 4096,
        _ => false,
    }
}

fn valid_recovery_id(value: &Value) -> bool {
    value.as_str().is_some_and(|id| {
        id.len() == 32
            && id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn valid_rules_recovery_args(args: &Value) -> bool {
    let Some(object) = args.as_object() else {
        return false;
    };
    object
        .keys()
        .all(|key| matches!(key.as_str(), "project_id" | "recovery_id"))
        && object.get("recovery_id").is_some_and(valid_recovery_id)
        && match object.get("project_id") {
            None => true,
            Some(Value::String(id)) => !id.trim().is_empty() && id.len() <= 4096,
            _ => false,
        }
}

fn valid_rules_validate_args(args: &Value) -> bool {
    let Some(object) = args.as_object() else {
        return false;
    };
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "project_id" | "source"))
    {
        return false;
    }
    let Some(source) = object.get("source").and_then(Value::as_str) else {
        return false;
    };
    if source.len() > RULES_MAX_BYTES {
        return false;
    }
    match object.get("project_id") {
        None => true,
        Some(Value::String(id)) => !id.trim().is_empty() && id.len() <= 4096,
        _ => false,
    }
}

fn public_role(value: &Value) -> Result<Value, Fault> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(internal)?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(internal)?;
    match value.get("created_at").and_then(Value::as_str) {
        Some(created_at) => Ok(json!({"id":id,"name":name,"created_at":created_at})),
        None => Ok(json!({"id":id,"name":name})),
    }
}

fn public_role_list(value: &Value) -> Result<Value, Fault> {
    let roles = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(internal)?
        .iter()
        .map(public_role)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"items":roles}))
}

fn public_permission_list(value: &Value) -> Result<Value, Fault> {
    let permissions = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(internal)?
        .iter()
        .map(|item| {
            Ok(json!({
                "operation":item.get("operation").and_then(Value::as_str).ok_or_else(internal)?,
                "scope":item.get("scope").and_then(Value::as_str).ok_or_else(internal)?,
                "target_id":item.get("target_id").and_then(Value::as_str).ok_or_else(internal)?,
                "decision":item.get("decision").and_then(Value::as_str).ok_or_else(internal)?
            }))
        })
        .collect::<Result<Vec<_>, Fault>>()?;
    Ok(json!({"items":permissions}))
}

fn public_account(value: &Value) -> Result<Value, Fault> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(internal)?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(internal)?;
    let is_admin = value
        .get("is_admin")
        .and_then(Value::as_bool)
        .ok_or_else(internal)?;
    let disabled = value
        .get("disabled")
        .and_then(Value::as_bool)
        .ok_or_else(internal)?;
    let role_ids = value
        .get("role_ids")
        .and_then(Value::as_array)
        .ok_or_else(internal)?
        .iter()
        .map(|role| role.as_str().map(str::to_owned).ok_or_else(internal))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"id":id,"name":name,"is_admin":is_admin,"disabled":disabled,"role_ids":role_ids}))
}

fn public_account_list(value: &Value) -> Result<Value, Fault> {
    let accounts = value
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(internal)?
        .iter()
        .map(public_account)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"items":accounts}))
}

#[cfg(windows)]
fn lease(store: &CredentialStore) -> Result<File, Fault> {
    store.private_lease(".host.lock").map_err(|error| {
        if error.raw_os_error() == Some(32) {
            Fault::new("GUI_ALREADY_RUNNING", 6, "The GUI is already running")
        } else {
            storage()
        }
    })
}
#[cfg(not(windows))]
fn lease(_store: &CredentialStore) -> Result<File, Fault> {
    Err(storage())
}

impl GuiHost {
    pub fn open(root: &Path) -> Result<Self, Fault> {
        let state_root = root.join("gui-state");
        let store = CredentialStore::open(&state_root).map_err(|_| storage())?;
        let held = lease(&store)?;
        Ok(Self {
            root: root.to_owned(),
            store,
            _lease: held,
        })
    }
    fn selection(&self) -> Result<Option<Selection>, Fault> {
        let selected = self
            .store
            .get("selection")
            .map_err(|_| storage())?
            .map(|bytes| serde_json::from_slice::<Selection>(&bytes).map_err(|_| storage()))
            .transpose()?;
        if selected.as_ref().is_some_and(|v| !valid_alias(&v.alias)) {
            return Err(storage());
        }
        Ok(selected)
    }
    fn notification_selection_gate(&self) -> Result<File, Fault> {
        self.store
            .lock("notification-selection-gate")
            .map_err(|_| storage())
    }
    fn pending(&self) -> Result<Option<Pending>, Fault> {
        let Some(bytes) = self.store.get("pending-auth").map_err(|_| storage())? else {
            return Ok(None);
        };
        let value: Pending = serde_json::from_slice(&bytes).map_err(|_| storage())?;
        let name = value
            .request
            .args
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(storage)?;
        let password = value
            .request
            .args
            .get("password")
            .and_then(Value::as_str)
            .ok_or_else(storage)?;
        let args = value.request.args.as_object().ok_or_else(storage)?;
        let recovery_code = if value.request.operation == "account.recover" {
            Some(
                args.get("recovery_code")
                    .and_then(Value::as_str)
                    .filter(|code| !code.trim().is_empty() && code.len() <= 4096)
                    .ok_or_else(storage)?,
            )
        } else {
            None
        };
        let password_change = value.request.operation == "account.password";
        let google_recovery = value.request.operation == "account.google.recover";
        if google_recovery
            && (args
                .get("challenge")
                .and_then(Value::as_str)
                .is_none_or(|v| v.len() != 64 || !v.bytes().all(|b| b.is_ascii_hexdigit()))
                || args
                    .get("id_token")
                    .and_then(Value::as_str)
                    .is_none_or(|v| v.is_empty() || v.len() > 16384))
        {
            return Err(storage());
        }
        if password_change
            && args
                .get("current_password")
                .and_then(Value::as_str)
                .is_none_or(|text| {
                    text.len() > 4096 || (!text.is_empty() && text.trim().is_empty())
                })
        {
            return Err(storage());
        }
        let expected_arg_count = if google_recovery {
            4
        } else if recovery_code.is_some() || password_change {
            3
        } else {
            2
        };
        if value.version != 1
            || value.intent.len() != 64
            || !value.intent.bytes().all(|b| b.is_ascii_hexdigit())
            || value.alias != alias(name)
            || name.trim().is_empty()
            || name.len() > 4096
            || (password.trim().is_empty()
                && (!password.is_empty() || recovery_code.is_some() || google_recovery))
            || password.len() > 4096
            || !matches!(
                value.request.operation.as_str(),
                "account.setup"
                    | "account.login"
                    | "account.recover"
                    | "account.password"
                    | "account.google.recover"
            )
            || value.request.auth.is_some()
            || value.request.request_id.is_some()
            || value.request.context != json!({})
            || args.len() != expected_arg_count
            || (recovery_code.is_none() && args.contains_key("recovery_code"))
        {
            return Err(storage());
        }
        Ok(Some(value))
    }
    fn pending_create(&self) -> Result<Option<(PendingCreate, CreateManifest)>, Fault> {
        let Some(bytes) = self.store.get(CREATE_KEY).map_err(|_| storage())? else {
            return Ok(None);
        };
        let manifest: CreateManifest = serde_json::from_slice(&bytes).map_err(|_| storage())?;
        if manifest.version != 1
            || manifest.bytes == 0
            || manifest.bytes > CREATE_MAX_BYTES
            || manifest.chunks == 0
            || manifest.chunks > CREATE_MAX_CHUNKS
            || manifest.chunks != manifest.bytes.div_ceil(CREATE_CHUNK_BYTES)
            || manifest.group.len() != 32
            || !manifest.group.bytes().all(|b| b.is_ascii_hexdigit())
            || manifest.sha256.len() != 64
            || !manifest.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(storage());
        }
        let mut encoded = Vec::with_capacity(manifest.bytes);
        for index in 0..manifest.chunks {
            let chunk = self
                .store
                .get(&create_chunk_key(&manifest.group, index))
                .map_err(|_| storage())?
                .ok_or_else(storage)?;
            let expected = if index + 1 == manifest.chunks {
                manifest.bytes - index * CREATE_CHUNK_BYTES
            } else {
                CREATE_CHUNK_BYTES
            };
            if chunk.len() != expected {
                return Err(storage());
            }
            encoded.extend_from_slice(&chunk);
        }
        if encoded.len() != manifest.bytes
            || format!("{:x}", Sha256::digest(&encoded)) != manifest.sha256.to_ascii_lowercase()
        {
            return Err(storage());
        }
        let value: PendingCreate = serde_json::from_slice(&encoded).map_err(|_| storage())?;
        let request_id_valid = uuid::Uuid::parse_str(&value.request_id)
            .is_ok_and(|id| id.to_string() == value.request_id);
        if value.version != 1
            || !valid_alias(&value.alias)
            || !matches!(
                value.operation.as_str(),
                "board.create" | "thread.create" | "post.create" | "response.create"
            )
            || !valid_create_args(&value.operation, &value.args)
            || !request_id_valid
        {
            return Err(storage());
        }
        Ok(Some((value, manifest)))
    }

    fn save_pending_create(&self, value: &PendingCreate) -> Result<(), Fault> {
        let encoded = create_pending(value)?;
        if encoded.is_empty() || encoded.len() > CREATE_MAX_BYTES {
            return Err(invalid());
        }
        let group = uuid::Uuid::new_v4().simple().to_string();
        let chunks = encoded.len().div_ceil(CREATE_CHUNK_BYTES);
        if chunks == 0 || chunks > CREATE_MAX_CHUNKS {
            return Err(invalid());
        }
        for (index, chunk) in encoded.chunks(CREATE_CHUNK_BYTES).enumerate() {
            self.store
                .put(&create_chunk_key(&group, index), chunk)
                .map_err(|_| storage())?;
        }
        let manifest = create_manifest(&encoded, &group, chunks)?;
        self.store.put(CREATE_KEY, &manifest).map_err(|_| storage())
    }

    fn mark_pending_create_completed(&self, group: &str) -> Result<(), Fault> {
        let Some((_, mut manifest)) = self.pending_create()? else {
            return Err(storage());
        };
        if manifest.group != group {
            return Err(storage());
        }
        manifest.completed = true;
        let bytes = serde_json::to_vec(&manifest).map_err(|_| storage())?;
        self.store.put(CREATE_KEY, &bytes).map_err(|_| storage())
    }

    fn remove_pending_create(&self) -> Result<(), Fault> {
        let Some((_, manifest)) = self.pending_create()? else {
            return Ok(());
        };
        self.store.remove(CREATE_KEY).map_err(|_| storage())?;
        // The manifest removal is the durable acknowledgement. Orphan chunk cleanup is best effort.
        for index in 0..CREATE_MAX_CHUNKS {
            let _ = self.store.remove(&create_chunk_key(&manifest.group, index));
        }
        Ok(())
    }

    fn acknowledge_pending_create(&self) -> Result<(), Fault> {
        let Some((_, manifest)) = self.pending_create()? else {
            return Ok(());
        };
        if !manifest.completed {
            return Err(conflict());
        }
        self.remove_pending_create()
    }

    fn creation_status_value(&self) -> Result<Value, Fault> {
        match self.pending_create()? {
            Some((value, _)) => Ok(json!({
                "pending": true,
                "operation": value.operation,
                "expired": create_time_is_expired(&value.created_at)
            })),
            None => Ok(json!({"pending":false,"operation":null,"expired":false})),
        }
    }
    pub fn snapshot(&mut self) -> Result<Value, Fault> {
        let client = GuiClient::open(&self.root)?;
        let required = client
            .setup_status()?
            .get("setup_required")
            .and_then(Value::as_bool)
            .ok_or_else(storage)?;
        let account = match self.selection()? {
            Some(selected) => {
                match client.request(&selected.alias, request("account.status", json!({}))) {
                    Ok(account) => Some(account),
                    Err(error) if error.exit_code == 3 => None,
                    Err(error) => return Err(error),
                }
            }
            None => None,
        };
        let pending = self.pending()?.is_some();
        if !pending && let Some(name) = account.as_ref().and_then(|value| value["name"].as_str()) {
            account_names::remember(&self.store, name);
        }
        Ok(
            json!({"setupRequired":required,"account":account,"pendingIssue":pending,
            "accountNameHistory":account_names::read(&self.store)}),
        )
    }

    /// Reports whether the background runtime is available without launching it.
    pub fn runtime_status(&self) -> Result<Value, Fault> {
        lifecycle::status(&self.root)
    }

    #[cfg(windows)]
    pub fn startup_status(&self) -> Result<Value, Fault> {
        swarm_runtime::startup::status(&self.root, &startup_executable()?)
    }

    #[cfg(windows)]
    pub fn set_startup(&self, enabled: bool) -> Result<Value, Fault> {
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        GuiClient::open(&self.root)?
            .request(&selected.alias, request("account.status", json!({})))?;
        swarm_runtime::startup::configure(&self.root, &startup_executable()?, enabled)
    }

    /// Starts the runtime only after an explicit human desktop action.
    pub fn start_runtime(&self) -> Result<Value, Fault> {
        start_runtime(&self.root)?;
        lifecycle::status(&self.root)
    }

    /// Requests a complete stop through the selected authenticated human session.
    pub fn stop_runtime(&self) -> Result<Value, Fault> {
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        client.stop(&selected.alias)?;
        lifecycle::status(&self.root)
    }

    pub fn authenticate(&mut self, mode: &str, name: &str, password: &str) -> Result<Value, Fault> {
        let operation = match mode {
            "setup" => "account.setup",
            "login" => "account.login",
            _ => return Err(invalid()),
        };
        if name.trim().is_empty()
            || name.len() > 4096
            || (!password.is_empty() && password.trim().is_empty())
            || password.len() > 4096
        {
            return Err(invalid());
        }
        if let Some((pending, _)) = self.pending_create()?
            && (operation != "account.login" || alias(name) != pending.alias)
        {
            return Err(conflict());
        }
        let args = json!({"name":name,"password":password});
        match self.pending()? {
            Some(value) if value.request.operation != operation || value.request.args != args => {
                return Err(conflict());
            }
            Some(_) => {}
            None => {
                let value = Pending {
                    version: 1,
                    intent: format!(
                        "{}{}",
                        uuid::Uuid::new_v4().simple(),
                        uuid::Uuid::new_v4().simple()
                    ),
                    alias: alias(name),
                    request: request(operation, args),
                };
                let encoded = serde_json::to_vec(&value).map_err(|_| storage())?;
                let _gate = self.notification_selection_gate()?;
                self.store
                    .put("pending-auth", &encoded)
                    .map_err(|_| storage())?;
            }
        }
        self.retry_pending()
    }

    /// Recovers the main Admin through the same durable, DPAPI-protected issuance
    /// path as setup and login. The public result is only the account snapshot.
    pub fn recover_account(
        &mut self,
        name: &str,
        recovery_code: &str,
        password: &str,
    ) -> Result<Value, Fault> {
        if name.trim().is_empty()
            || name.len() > 4096
            || recovery_code.trim().is_empty()
            || recovery_code.len() > 4096
            || password.trim().is_empty()
            || password.len() > 4096
        {
            return Err(invalid());
        }
        let pending_alias = alias(name);
        if let Some((pending, _)) = self.pending_create()?
            && pending_alias != pending.alias
        {
            return Err(conflict());
        }
        let args = json!({"name":name,"recovery_code":recovery_code,"password":password});
        match self.pending()? {
            Some(value)
                if value.request.operation != "account.recover"
                    || value.alias != pending_alias
                    || value.request.args != args =>
            {
                return Err(conflict());
            }
            Some(_) => {}
            None => {
                let value = Pending {
                    version: 1,
                    intent: format!(
                        "{}{}",
                        uuid::Uuid::new_v4().simple(),
                        uuid::Uuid::new_v4().simple()
                    ),
                    alias: pending_alias,
                    request: request("account.recover", args),
                };
                let encoded = serde_json::to_vec(&value).map_err(|_| storage())?;
                let _gate = self.notification_selection_gate()?;
                self.store
                    .put("pending-auth", &encoded)
                    .map_err(|_| storage())?;
            }
        }
        self.retry_pending()
    }

    /// Cancels only the outstanding credential issuance intent, then refreshes
    /// the currently selected account snapshot without rolling back any service commit.
    pub fn cancel_pending(&mut self) -> Result<Value, Fault> {
        self.pending()?;
        let _gate = self.notification_selection_gate()?;
        self.store.remove("pending-auth").map_err(|_| storage())?;
        drop(_gate);
        self.snapshot()
    }

    /// Explicitly reveals the current recovery code through the authenticated
    /// private GUI transport. It is never included in the general snapshot.
    pub fn reveal_recovery_code(&mut self) -> Result<Value, Fault> {
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        let result = client.reveal_recovery_code(&selected.alias)?;
        let recovery_code = result
            .get("recovery_code")
            .and_then(Value::as_str)
            .filter(|code| !code.trim().is_empty() && code.len() <= 4096)
            .ok_or_else(internal)?;
        Ok(json!({"recovery_code":recovery_code}))
    }

    pub fn retry_pending(&mut self) -> Result<Value, Fault> {
        let value = self.pending()?.ok_or_else(invalid)?;
        let client = GuiClient::open(&self.root)?;
        match client.issue(&value.alias, &value.intent, value.request) {
            Ok(_) => {}
            Err(error) => {
                // Definitive input/authentication failures did not create a usable grant.
                // Transport/storage/conflict errors retain the original intent for replay.
                if matches!(error.exit_code, 2..=5) || error.code == "ALREADY_CONFIGURED" {
                    let _gate = self.notification_selection_gate()?;
                    self.store.remove("pending-auth").map_err(|_| storage())?;
                }
                return Err(error);
            }
        }
        let selection =
            serde_json::to_vec(&Selection { alias: value.alias }).map_err(|_| storage())?;
        let _gate = self.notification_selection_gate()?;
        self.store
            .put("selection", &selection)
            .map_err(|_| storage())?;
        self.store.remove("pending-auth").map_err(|_| storage())?;
        drop(_gate);
        self.snapshot()
    }

    pub fn set_password(
        &mut self,
        expected_account_id: &str,
        current_password: &str,
        password: &str,
    ) -> Result<Value, Fault> {
        if expected_account_id.is_empty()
            || password.len() > 4096
            || current_password.len() > 4096
            || (!password.is_empty() && password.trim().is_empty())
            || (!current_password.is_empty() && current_password.trim().is_empty())
        {
            return Err(invalid());
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let account = GuiClient::open(&self.root)?
            .request(&selected.alias, request("account.status", json!({})))?;
        if account["id"].as_str() != Some(expected_account_id) {
            return Err(authentication());
        }
        let name = account["name"].as_str().ok_or_else(storage)?;
        if alias(name) != selected.alias {
            return Err(authentication());
        }
        let value = Pending {
            version: 1,
            intent: format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
            alias: selected.alias,
            request: request(
                "account.password",
                json!({"name":name,"password":password,"current_password":current_password}),
            ),
        };
        let encoded = serde_json::to_vec(&value).map_err(|_| storage())?;
        let gate = self.notification_selection_gate()?;
        self.store
            .put("pending-auth", &encoded)
            .map_err(|_| storage())?;
        drop(gate);
        self.retry_pending()
    }
    pub fn logout(&mut self) -> Result<Value, Fault> {
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let client = GuiClient::open(&self.root)?;
        if let Some(selected) = self.selection()? {
            match client.request(&selected.alias, request("account.logout", json!({}))) {
                Ok(_) => {}
                Err(error) if error.exit_code == 3 => {}
                Err(error) => return Err(error),
            }
            let _gate = self.notification_selection_gate()?;
            self.store.remove("selection").map_err(|_| storage())?;
        }
        self.snapshot()
    }

    /// Read conversation data through the authenticated human operation path.
    /// The renderer supplies only an operation and its core-validated arguments;
    /// identity and request metadata are constructed privately here.
    pub fn read(&mut self, operation: &str, args: Value) -> Result<Value, Fault> {
        if !matches!(
            operation,
            "management.capabilities"
                | "authorization.capabilities"
                | "rules.capabilities"
                | "rules.get"
                | "rules.subjects"
                | "account.list"
                | "account.get"
                | "role.list"
                | "permission.list"
                | "search"
                | "board.list"
                | "thread.list"
                | "thread.get"
                | "post.list"
                | "post.page"
                | "post.get"
                | "post.history"
                | "response.list"
                | "conversation.capabilities"
                | "conversation.deleted"
                | "conversation.pins"
                | "conversation.activity"
                | "conversation.subscriptions"
                | "conversation.unread"
                | "notification.settings.get"
                | "display.settings.get"
        ) {
            return Err(invalid_read());
        }
        if !valid_notification_read_args(operation, &args)
            && matches!(
                operation,
                "conversation.subscriptions"
                    | "conversation.unread"
                    | "notification.settings.get"
                    | "display.settings.get"
            )
        {
            return Err(invalid());
        }
        if self.pending()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        let result = client.request(&selected.alias, request(operation, args))?;
        match operation {
            "account.list" => public_account_list(&result),
            "account.get" => public_account(&result),
            "role.list" => public_role_list(&result),
            "permission.list" => public_permission_list(&result),
            _ => Ok(result),
        }
    }

    /// Runs an authenticated human RULES diagnostic. The renderer supplies
    /// only scoped identifiers and context; the selected human session is
    /// attached privately by the host.
    pub fn check_rules(&mut self, args: Value, context: Value) -> Result<Value, Fault> {
        let object = args
            .as_object()
            .filter(|object| {
                object.keys().all(|key| {
                    ["project_id", "actor_id", "operation", "target"].contains(&key.as_str())
                })
            })
            .ok_or_else(invalid)?;
        for field in ["project_id", "actor_id", "operation"] {
            if !object
                .get(field)
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty() && value.len() <= 512)
            {
                return Err(invalid());
            }
        }
        if let Some(target) = object.get("target") {
            let target = target
                .as_object()
                .filter(|target| {
                    target.keys().all(|key| {
                        ["id", "board_id", "thread_id", "no", "to"].contains(&key.as_str())
                    })
                })
                .ok_or_else(invalid)?;
            if target.values().any(|value| {
                !value
                    .as_str()
                    .is_some_and(|text| !text.trim().is_empty() && text.len() <= 512)
                    && value.as_i64().is_none_or(|number| number <= 0)
            }) {
                return Err(invalid());
            }
        }
        if context.as_object().is_none()
            || serde_json::to_vec(&context).map_or(true, |bytes| bytes.len() > 64 * 1024)
        {
            return Err(invalid());
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        let mut request = request("rules.check", args);
        request.context = context;
        client.request(&selected.alias, request)
    }

    /// Validates or saves RULES.md through the current human session. The
    /// renderer can provide only a registered project ID and source/revision
    /// data, never a filesystem path or authority token.
    pub fn manage_rules(&mut self, operation: &str, args: Value) -> Result<Value, Fault> {
        match operation {
            "rules.recovery.inspect" => {
                if !valid_rules_recovery_args(&args) {
                    return Err(invalid());
                }
                if self.pending()?.is_some() || self.pending_create()?.is_some() {
                    return Err(conflict());
                }
                let selected = self.selection()?.ok_or_else(authentication)?;
                let client = GuiClient::open(&self.root)?;
                let result = client.request(&selected.alias, request("rules.get", args.clone()))?;
                let current = &result["current"];
                let candidate = &result["candidate"];
                let bounded_source = |value: &Value| {
                    value
                        .as_str()
                        .is_some_and(|source| source.len() <= RULES_MAX_BYTES)
                };
                let bounded_revision = |value: &Value| {
                    value
                        .as_str()
                        .is_some_and(|revision| !revision.is_empty() && revision.len() <= 128)
                };
                if result["recovery_id"] != args["recovery_id"]
                    || result["project_id"]
                        != args.get("project_id").cloned().unwrap_or(Value::Null)
                    || result["scope"]
                        != if args.get("project_id").is_some() {
                            "project"
                        } else {
                            "global"
                        }
                    || !(current["source"].is_null() || bounded_source(&current["source"]))
                    || !bounded_revision(&current["revision"])
                    || current["utf8"].as_bool().is_none()
                    || current["exists"].as_bool().is_none()
                    || !bounded_source(&candidate["source"])
                    || !bounded_revision(&candidate["revision"])
                    || candidate["valid"].as_bool().is_none()
                    || candidate["message"]
                        .as_str()
                        .is_none_or(|message| message.len() > 4096)
                {
                    return Err(internal());
                }
                Ok(
                    json!({"scope":result["scope"],"project_id":result["project_id"],"recovery_id":result["recovery_id"],
                    "current":{"source":current["source"],"revision":current["revision"],"utf8":current["utf8"],"exists":current["exists"]},
                    "candidate":{"source":candidate["source"],"revision":candidate["revision"],"valid":candidate["valid"],"message":candidate["message"]}}),
                )
            }
            "rules.recovery.apply" => {
                if args.get("recovery_id").is_none() || args.get("recovery_revision").is_none() {
                    return Err(invalid());
                }
                self.save_rules(args)
            }
            "rules.validate" => {
                if !valid_rules_validate_args(&args) {
                    return Err(invalid());
                }
                if self.pending()?.is_some() || self.pending_create()?.is_some() {
                    return Err(conflict());
                }
                let selected = self.selection()?.ok_or_else(authentication)?;
                let client = GuiClient::open(&self.root)?;
                let result = client.request(&selected.alias, request("rules.validate", args))?;
                let valid = result
                    .get("valid")
                    .and_then(Value::as_bool)
                    .ok_or_else(internal)?;
                let message = result
                    .get("message")
                    .and_then(Value::as_str)
                    .ok_or_else(internal)?;
                let actions = result
                    .get("actions")
                    .and_then(Value::as_array)
                    .ok_or_else(internal)?
                    .iter()
                    .map(|action| action.as_str().map(str::to_owned).ok_or_else(internal))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(json!({"valid":valid,"message":message,"actions":actions}))
            }
            "rules.save" => self.save_rules(args),
            _ => Err(invalid()),
        }
    }

    fn save_rules(&mut self, args: Value) -> Result<Value, Fault> {
        if !valid_rules_save_args(&args) {
            return Err(invalid());
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        let result = client.request(&selected.alias, request("rules.save", args))?;
        let source = result
            .get("source")
            .and_then(Value::as_str)
            .filter(|source| source.len() <= RULES_MAX_BYTES)
            .ok_or_else(internal)?;
        let revision = result
            .get("revision")
            .and_then(Value::as_str)
            .ok_or_else(internal)?;
        let scope = result
            .get("scope")
            .and_then(Value::as_str)
            .filter(|scope| matches!(*scope, "global" | "project"))
            .ok_or_else(internal)?;
        let project_id = result
            .get("project_id")
            .filter(|id| id.is_null() || id.as_str().is_some())
            .ok_or_else(internal)?;
        Ok(json!({"source":source,"revision":revision,"scope":scope,"project_id":project_id}))
    }

    /// Creates a Board, Thread, Post, or Response through the saved human identity.
    /// The durable request id is reused until the result is received or explicitly discarded.
    pub fn create(&mut self, operation: &str, args: Value) -> Result<Value, Fault> {
        if !valid_create_args(operation, &args) {
            return Err(invalid());
        }
        if self.pending()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        if self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| storage())?
            .as_secs();
        if created_at == 0 {
            return Err(storage());
        }
        let pending = PendingCreate {
            version: 1,
            alias: selected.alias,
            operation: operation.to_owned(),
            args,
            request_id: uuid::Uuid::new_v4().to_string(),
            created_at: json!(created_at),
        };
        self.save_pending_create(&pending)?;
        self.retry_create()
    }

    /// Edits a post through the authenticated human operation path. The renderer
    /// cannot choose the operation, identity, request id, or request context.
    pub fn edit(&mut self, args: Value) -> Result<Value, Fault> {
        if !valid_edit_args(&args) {
            return Err(invalid());
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        client.request(&selected.alias, request("post.edit", args))
    }

    /// Deletes or restores one conversation item using the selected saved
    /// human identity. The core rechecks its current view and mutation rights.
    pub fn manage(&mut self, operation: &str, args: Value) -> Result<Value, Fault> {
        if !valid_manage_args(operation, &args) {
            return Err(invalid());
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        client.request(&selected.alias, request(operation, args))
    }

    /// Admin-only project bootstrap. Never accepts renderer credentials or metadata.
    pub fn project_control(
        &mut self,
        expected_account_id: &str,
        operation: &str,
        args: Value,
    ) -> Result<Value, Fault> {
        let fields: &[&str] = match operation {
            "project.list" => &[],
            "project.register" => &["name", "root"],
            _ => return Err(invalid()),
        };
        let object = args.as_object().ok_or_else(invalid)?;
        if expected_account_id.is_empty()
            || expected_account_id.len() > 128
            || object.len() != fields.len()
            || object.keys().any(|key| !fields.contains(&key.as_str()))
        {
            return Err(invalid());
        }
        if operation == "project.register" {
            for (field, limit) in [("name", 512), ("root", 32768)] {
                if !object[field]
                    .as_str()
                    .is_some_and(|value| !value.trim().is_empty() && value.len() <= limit)
                {
                    return Err(invalid());
                }
            }
            if !Path::new(object["root"].as_str().ok_or_else(invalid)?).is_absolute() {
                return Err(invalid());
            }
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        let account = client.request(&selected.alias, request("account.status", json!({})))?;
        if account["id"].as_str() != Some(expected_account_id) {
            return Err(Fault::new(
                "ACCOUNT_CHANGED",
                6,
                "The selected account changed",
            ));
        }
        if account["is_admin"].as_bool() != Some(true) {
            return Err(Fault::new(
                "PERMISSION_DENIED",
                4,
                "Project setup requires Admin",
            ));
        }
        client.request(&selected.alias, request(operation, args))
    }

    /// Native-only window state; never exposed as a renderer operation.
    pub fn window_geometry_get(&mut self) -> Result<Option<swarm_core::WindowGeometry>, Fault> {
        match GuiClient::open(&self.root) {
            Ok(client) => client.window_geometry_get(),
            Err(error) if error.code == "CONNECTION_ERROR" => {
                swarm_runtime::window_geometry_get(&self.root)
            }
            Err(error) => Err(error),
        }
    }

    pub fn window_geometry_set(
        &mut self,
        geometry: &swarm_core::WindowGeometry,
    ) -> Result<(), Fault> {
        match GuiClient::open(&self.root) {
            Ok(client) => client.window_geometry_set(geometry),
            Err(error) if error.code == "CONNECTION_ERROR" => {
                swarm_runtime::window_geometry_set(&self.root, geometry)
            }
            Err(error) => Err(error),
        }
    }

    /// Keep account layout settings in the common operation layer and backups.
    pub fn ui_preferences(
        &mut self,
        expected_account_id: &str,
        operation: &str,
        mut args: Value,
    ) -> Result<Value, Fault> {
        if !matches!(operation, "ui.preferences.get" | "ui.preferences.set")
            || expected_account_id.is_empty()
            || expected_account_id.len() > 128
            || args.to_string().len() > 4096
        {
            return Err(invalid());
        }
        let object = args.as_object_mut().ok_or_else(invalid)?;
        let fields = if operation == "ui.preferences.get" {
            &["scope", "key"][..]
        } else {
            &["scope", "key", "value"][..]
        };
        if object.keys().any(|key| !fields.contains(&key.as_str())) {
            return Err(invalid());
        }
        object.insert("expected_account_id".into(), json!(expected_account_id));
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        GuiClient::open(&self.root)?.request(&selected.alias, request(operation, args))
    }

    /// A delayed notification UI mutation must not apply to a newly selected account.
    /// Tauri holds the same host mutex as authentication/selection through this call.
    pub fn manage_notifications(
        &mut self,
        expected_account_id: &str,
        operation: &str,
        args: Value,
    ) -> Result<Value, Fault> {
        if !matches!(
            operation,
            "thread.subscribe"
                | "post.read.mark"
                | "notification.settings.set"
                | "display.settings.set"
        ) || !valid_manage_args(operation, &args)
            || expected_account_id.is_empty()
            || expected_account_id.len() > 128
        {
            return Err(invalid());
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        let account = client.request(&selected.alias, request("account.status", json!({})))?;
        if account.get("id").and_then(Value::as_str) != Some(expected_account_id) {
            return Err(Fault::new(
                "ACCOUNT_CHANGED",
                6,
                "The selected account changed; refresh before continuing",
            ));
        }
        client.request(&selected.alias, request(operation, args))
    }

    /// Tauri holds the selection mutex through the runtime's administrator check.
    pub fn backup_control(
        &mut self,
        expected_account_id: &str,
        operation: &str,
        args: Value,
    ) -> Result<Value, Fault> {
        if !matches!(
            operation,
            "settings.get"
                | "settings.set"
                | "status"
                | "manual.create"
                | "manual.ack"
                | "archives.list"
                | "archives.delete"
                | "restore.cleanup"
        ) || expected_account_id.is_empty()
            || expected_account_id.len() > 128
        {
            return Err(invalid());
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        // Authorization precedes even creation of a local retry intent.
        client.backup(
            &selected.alias,
            expected_account_id,
            "settings.get",
            json!({}),
        )?;
        if matches!(
            operation,
            "settings.get"
                | "settings.set"
                | "archives.list"
                | "archives.delete"
                | "restore.cleanup"
        ) {
            let result = client.backup(&selected.alias, expected_account_id, operation, args)?;
            if operation == "restore.cleanup" {
                self.clear_restore_selection(expected_account_id)?;
            }
            return Ok(result);
        }
        let key = format!(
            "backup-pending-{:x}",
            Sha256::digest(expected_account_id.as_bytes())
        );
        let pending = self
            .store
            .get(&key)
            .map_err(|_| storage())?
            .map(|raw| {
                let p: PendingBackup = serde_json::from_slice(&raw).map_err(|_| storage())?;
                if p.version != 1
                    || p.account_id != expected_account_id
                    || !valid_alias(&p.alias)
                    || !swarm_core::valid_backup_intent(&p.intent)
                {
                    return Err(storage());
                }
                Ok(p)
            })
            .transpose()?;
        match operation {
            "manual.create" => {
                if args.as_object().is_none_or(|o| !o.is_empty()) {
                    return Err(invalid());
                }
                let p = match pending {
                    Some(p) => p,
                    None => {
                        let p = PendingBackup {
                            version: 1,
                            account_id: expected_account_id.into(),
                            alias: selected.alias.clone(),
                            intent: uuid::Uuid::new_v4().to_string(),
                        };
                        self.store
                            .put(&key, &serde_json::to_vec(&p).map_err(|_| storage())?)
                            .map_err(|_| storage())?;
                        p
                    }
                };
                client.backup(
                    &selected.alias,
                    expected_account_id,
                    "manual.create",
                    json!({"intent":p.intent}),
                )
            }
            "manual.ack" => {
                if args.as_object().is_none_or(|o| o.len() != 1) {
                    return Err(invalid());
                }
                let p = pending.ok_or_else(conflict)?;
                if args["intent"].as_str() != Some(p.intent.as_str()) {
                    return Err(conflict());
                }
                let result = client.backup(
                    &selected.alias,
                    expected_account_id,
                    "manual.status",
                    json!({"intent":p.intent}),
                )?;
                if result["state"] != "completed" {
                    return Err(conflict());
                }
                self.store.remove(&key).map_err(|_| storage())?;
                Ok(json!({"acknowledged":true}))
            }
            "status" => {
                let mut result =
                    client.backup(&selected.alias, expected_account_id, operation, args)?;
                result["manual"] = match pending {
                    None => Value::Null,
                    Some(p) => client
                        .backup(
                            &selected.alias,
                            expected_account_id,
                            "manual.status",
                            json!({"intent":p.intent}),
                        )
                        .unwrap_or_else(|_| json!({"intent":p.intent,"state":"unavailable"})),
                };
                if result["manual"]["state"] == "not_started" {
                    // Another administrator may have deliberately removed the
                    // completed archive and its receipt. Do not strand the GUI
                    // behind an acknowledgement for a result that no longer exists.
                    self.store.remove(&key).map_err(|_| storage())?;
                    result["manual"] = Value::Null;
                }
                Ok(result)
            }
            _ => client.backup(&selected.alias, expected_account_id, operation, args),
        }
    }

    /// Performs a one-shot account-management operation through the selected human
    /// identity. Password arguments are validated in memory and never added to GUI state.
    pub fn manage_account(&mut self, operation: &str, args: Value) -> Result<Value, Fault> {
        if !valid_account_manage_args(operation, &args) {
            return Err(invalid());
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        let result = client.request(&selected.alias, request(operation, args))?;
        let account = public_account(&result)?;
        if operation == "account.create"
            && let Some(name) = account["name"].as_str()
        {
            account_names::remember(&self.store, name);
        }
        Ok(account)
    }

    /// Performs one role or permission mutation through the selected human
    /// identity. The core remains authoritative for both visibility and rights.
    pub fn manage_authorization(&mut self, operation: &str, args: Value) -> Result<Value, Fault> {
        if !valid_authorization_args(operation, &args) {
            return Err(invalid());
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        if operation == "permission.set" {
            let capabilities = client.request(
                &selected.alias,
                request("authorization.capabilities", json!({})),
            )?;
            let requested_operation = args["operation"].as_str().ok_or_else(invalid)?;
            let requested_scope = args["scope"].as_str().ok_or_else(invalid)?;
            let supported = capabilities
                .get("operations")
                .and_then(Value::as_array)
                .is_some_and(|operations| {
                    operations.iter().any(|entry| {
                        entry.get("operation").and_then(Value::as_str) == Some(requested_operation)
                            && entry
                                .get("scopes")
                                .and_then(Value::as_array)
                                .is_some_and(|scopes| {
                                    scopes
                                        .iter()
                                        .any(|scope| scope.as_str() == Some(requested_scope))
                                })
                    })
                });
            if !supported {
                return Err(invalid());
            }
        }
        let result = client.request(&selected.alias, request(operation, args))?;
        match operation {
            "role.create" => public_role(&result),
            "role.assign" => Ok(json!({
                "account_id":result.get("account_id").and_then(Value::as_str).ok_or_else(internal)?,
                "role_id":result.get("role_id").and_then(Value::as_str).ok_or_else(internal)?,
                "assigned":result.get("assigned").and_then(Value::as_bool).ok_or_else(internal)?
            })),
            "permission.set" => Ok(json!({
                "subject_type":result.get("subject_type").and_then(Value::as_str).ok_or_else(internal)?,
                "subject_id":result.get("subject_id").and_then(Value::as_str).ok_or_else(internal)?,
                "operation":result.get("operation").and_then(Value::as_str).ok_or_else(internal)?,
                "scope":result.get("scope").and_then(Value::as_str).ok_or_else(internal)?,
                "target_id":result.get("target_id").and_then(Value::as_str).ok_or_else(internal)?,
                "decision":result.get("decision").and_then(Value::as_str).ok_or_else(internal)?
            })),
            _ => Err(invalid()),
        }
    }

    pub fn retry_create(&mut self) -> Result<Value, Fault> {
        let (pending, manifest) = self.pending_create()?.ok_or_else(invalid)?;
        if create_time_is_expired(&pending.created_at) {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(conflict)?;
        if selected.alias != pending.alias || self.pending()?.is_some() {
            return Err(conflict());
        }
        let request_id = Some(pending.request_id.clone());
        let client = GuiClient::open(&self.root)?;
        let result = client.request(
            &pending.alias,
            Request {
                operation: pending.operation,
                auth: None,
                args: pending.args,
                context: json!({}),
                request_id,
            },
        );
        match result {
            Ok(value) => {
                self.mark_pending_create_completed(&manifest.group)?;
                Ok(value)
            }
            Err(error) => {
                // Only core input rejection is definitive. Permission, target, auth,
                // conflict, and transport failures retain the durable request for review.
                if error.exit_code == 2 {
                    self.remove_pending_create()?;
                }
                Err(error)
            }
        }
    }

    pub fn creation_status(&self) -> Result<Value, Fault> {
        self.creation_status_value()
    }

    pub fn discard_create(&self) -> Result<Value, Fault> {
        self.remove_pending_create()?;
        self.creation_status_value()
    }

    pub fn acknowledge_create(&self) -> Result<Value, Fault> {
        self.acknowledge_pending_create()?;
        self.creation_status_value()
    }
}
