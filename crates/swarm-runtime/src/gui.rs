//! Private Rust host bridge for human GUI operations.
//!
//! The capability in this module only authenticates the local GUI host path.
//! Human identity, authorization, and credential handling remain in the core
//! service and `SessionBridge`.

use serde_json::{Value, json};
use std::fmt;
use std::path::{Path, PathBuf};
use swarm_core::{
    Service, WindowGeometry,
    protocol::{Fault, Request},
};
use swarm_ipc::{RpcResponse, read_response, write_request};
use swarm_platform::{CredentialStore, pipe};

const CAPABILITY_KEY: &str = "transport-capability";

fn invalid() -> Fault {
    Fault::new("INVALID_INPUT", 2, "Invalid GUI request")
}

fn unavailable() -> Fault {
    Fault::new("CONNECTION_ERROR", 8, "Local Swarm connection failed")
}

fn storage() -> Fault {
    Fault::new(
        "CREDENTIAL_STORAGE",
        8,
        "Private GUI capability storage failed",
    )
}

fn wrong_capability() -> Fault {
    Fault::new(
        "AUTHENTICATION_REQUIRED",
        3,
        "A valid GUI capability is required",
    )
}

fn denied() -> Fault {
    Fault::new(
        "PERMISSION_DENIED",
        4,
        "Operation is unavailable on the GUI transport",
    )
}

fn valid_hex(bytes: &[u8]) -> bool {
    bytes.len() == 64 && bytes.iter().all(u8::is_ascii_hexdigit)
}

fn equal_capability(expected: &[u8; 64], supplied: Option<&str>) -> bool {
    let supplied = supplied.unwrap_or("").as_bytes();
    let mut difference = (supplied.len() != expected.len()) as u8;
    let mut candidate = [0u8; 64];
    let copied = supplied.len().min(candidate.len());
    candidate[..copied].copy_from_slice(&supplied[..copied]);
    for (left, right) in expected.iter().zip(candidate.iter()) {
        difference |= left ^ right;
    }
    difference == 0 && valid_hex(supplied)
}

fn alias_valid(alias: &str) -> bool {
    (1..=128).contains(&alias.len())
        && alias
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn exact_keys(args: &Value, expected: &[&str]) -> bool {
    let Some(object) = args.as_object() else {
        return false;
    };
    object.len() == expected.len() && object.keys().all(|key| expected.contains(&key.as_str()))
}

fn string_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, Fault> {
    args.get(key).and_then(Value::as_str).ok_or_else(invalid)
}

fn read_capability(store: &CredentialStore) -> Result<Option<[u8; 64]>, Fault> {
    let Some(bytes) = store.get(CAPABILITY_KEY).map_err(|_| storage())? else {
        return Ok(None);
    };
    if !valid_hex(&bytes) {
        return Err(storage());
    }
    let mut capability = [0u8; 64];
    capability.copy_from_slice(&bytes);
    Ok(Some(capability))
}

/// Authenticated dispatcher used only by the Rust GUI host.
pub struct GuiAccess {
    capability: [u8; 64],
    root: PathBuf,
}

impl fmt::Debug for GuiAccess {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GuiAccess { capability: [redacted] }")
    }
}

impl GuiAccess {
    pub(crate) fn authorize_stop(
        &self,
        service: &mut Service,
        sessions: &mut crate::session::SessionBridge,
        request: Request,
    ) -> Result<(), Fault> {
        if !equal_capability(&self.capability, request.auth.as_deref()) {
            return Err(wrong_capability());
        }
        let _selection_gate = sessions.notification_selection_gate()?;
        sessions.synchronize_notification_recipient_locked(service)?;
        if request.operation != "gui.runtime_stop"
            || request.context != json!({})
            || request.request_id.is_some()
            || !exact_keys(&request.args, &["alias"])
        {
            return Err(invalid());
        }
        let alias = string_arg(&request.args, "alias")?;
        if !alias_valid(alias) {
            return Err(invalid());
        }
        sessions.gui_request(
            service,
            alias,
            Request {
                operation: "account.status".into(),
                auth: None,
                args: json!({}),
                context: json!({}),
                request_id: None,
            },
        )?;
        Ok(())
    }
    /// Opens or initializes the per-user DPAPI-protected GUI transport secret.
    pub fn open(root: &Path) -> Result<Self, Fault> {
        let store = CredentialStore::open(&root.join("gui-host")).map_err(|_| storage())?;
        let capability = match read_capability(&store)? {
            Some(capability) => capability,
            None => {
                let generated = format!(
                    "{}{}",
                    uuid::Uuid::new_v4().simple(),
                    uuid::Uuid::new_v4().simple()
                );
                let bytes = generated.as_bytes();
                if !valid_hex(bytes) {
                    return Err(storage());
                }
                store.put(CAPABILITY_KEY, bytes).map_err(|_| storage())?;
                let mut capability = [0u8; 64];
                capability.copy_from_slice(bytes);
                capability
            }
        };
        Ok(Self {
            capability,
            root: root.to_path_buf(),
        })
    }

    /// Dispatches one capability-authenticated outer GUI request.
    pub fn dispatch(
        &self,
        service: &mut Service,
        sessions: &mut crate::session::SessionBridge,
        request: Request,
    ) -> RpcResponse {
        if !equal_capability(&self.capability, request.auth.as_deref()) {
            return RpcResponse::from_result(Err(wrong_capability()));
        }
        let result = (|| {
            #[cfg(windows)]
            crate::restore_apply::blocked(&self.root)?;
            let _selection_gate = sessions.notification_selection_gate()?;
            sessions.synchronize_notification_recipient_locked(service)?;
            self.dispatch_result(service, sessions, request)
        })();
        RpcResponse::from_result(result)
    }

    fn dispatch_result(
        &self,
        service: &mut Service,
        sessions: &mut crate::session::SessionBridge,
        request: Request,
    ) -> Result<Value, Fault> {
        // Authenticate before inspecting operation or request contents.
        if !equal_capability(&self.capability, request.auth.as_deref()) {
            return Err(wrong_capability());
        }
        if request.request_id.is_some() || request.context != json!({}) {
            return Err(invalid());
        }

        match request.operation.as_str() {
            "gui.google_begin_recovery" => {
                crate::lifecycle::check_running_allowed(&self.root)?;
                if !exact_keys(&request.args, &["name"]) {
                    return Err(invalid());
                }
                let name = string_arg(&request.args, "name")?;
                if name.trim().is_empty() || name.len() > 4096 {
                    return Err(invalid());
                }
                service.request(Request {
                    operation: "account.google.begin".into(),
                    auth: None,
                    args: json!({"name":name,"purpose":"recover","current_password":""}),
                    context: json!({}),
                    request_id: None,
                })
            }
            "gui.backup" => {
                if !exact_keys(&request.args, &["alias", "account_id", "operation", "args"]) {
                    return Err(invalid());
                }
                let alias = string_arg(&request.args, "alias")?;
                let expected = string_arg(&request.args, "account_id")?;
                if !alias_valid(alias) || expected.is_empty() || expected.len() > 128 {
                    return Err(invalid());
                }
                let account = sessions.gui_request(
                    service,
                    alias,
                    Request {
                        operation: "account.status".into(),
                        auth: None,
                        args: json!({}),
                        context: json!({}),
                        request_id: None,
                    },
                )?;
                if account["id"].as_str() != Some(expected) {
                    return Err(Fault::new(
                        "ACCOUNT_CHANGED",
                        6,
                        "The selected account has changed",
                    ));
                }
                if account["is_admin"].as_bool() != Some(true) {
                    return Err(Fault::new(
                        "PERMISSION_DENIED",
                        4,
                        "Backup management requires an administrator",
                    ));
                }
                crate::lifecycle::check_running_allowed(&self.root)?;
                crate::backup::control(
                    service,
                    &self.root,
                    expected,
                    string_arg(&request.args, "operation")?,
                    &request.args["args"],
                )
            }
            "gui.setup_status" => {
                if !exact_keys(&request.args, &[]) {
                    return Err(invalid());
                }
                Ok(json!({"setup_required": service.setup_required()?}))
            }
            "gui.window_geometry_get" => {
                if !exact_keys(&request.args, &[]) {
                    return Err(invalid());
                }
                Ok(json!({"geometry":service.window_geometry_get()?}))
            }
            "gui.window_geometry_set" => {
                if !exact_keys(&request.args, &["geometry"]) {
                    return Err(invalid());
                }
                let geometry: WindowGeometry =
                    serde_json::from_value(request.args["geometry"].clone())
                        .map_err(|_| invalid())?;
                service.window_geometry_set(&geometry)?;
                Ok(json!({"saved":true}))
            }
            "gui.startup_preference_get" => {
                if !exact_keys(&request.args, &[]) {
                    return Err(invalid());
                }
                Ok(json!({"enabled":service.startup_preference()?}))
            }
            "gui.startup_preference_set" => {
                if !exact_keys(&request.args, &["enabled"]) {
                    return Err(invalid());
                }
                let enabled = request.args["enabled"].as_bool().ok_or_else(invalid)?;
                service.set_startup_preference(enabled)?;
                Ok(json!({"saved":true}))
            }
            "gui.issue" => {
                if !exact_keys(&request.args, &["alias", "intent", "request"]) {
                    return Err(invalid());
                }
                let alias = string_arg(&request.args, "alias")?;
                if !alias_valid(alias) {
                    return Err(invalid());
                }
                let intent = string_arg(&request.args, "intent")?;
                if !valid_hex(intent.as_bytes()) {
                    return Err(invalid());
                }
                let inner: Request = serde_json::from_value(
                    request.args.get("request").cloned().ok_or_else(invalid)?,
                )
                .map_err(|_| invalid())?;
                sessions.gui_issue(service, alias, intent, inner)
            }
            "gui.request" => {
                if !exact_keys(&request.args, &["alias", "request"]) {
                    return Err(invalid());
                }
                let alias = string_arg(&request.args, "alias")?;
                if !alias_valid(alias) {
                    return Err(invalid());
                }
                let inner: Request = serde_json::from_value(
                    request.args.get("request").cloned().ok_or_else(invalid)?,
                )
                .map_err(|_| invalid())?;
                sessions.gui_request(service, alias, inner)
            }
            "gui.reveal_recovery_code" => {
                if !exact_keys(&request.args, &["alias"]) {
                    return Err(invalid());
                }
                let alias = string_arg(&request.args, "alias")?;
                if !alias_valid(alias) {
                    return Err(invalid());
                }
                sessions.gui_reveal_recovery_code(service, alias)
            }
            _ => Err(denied()),
        }
    }
}

/// Private transport client for the Rust GUI host.
pub struct GuiClient {
    root: PathBuf,
    capability: [u8; 64],
}

impl fmt::Debug for GuiClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GuiClient { root: [redacted], capability: [redacted] }")
    }
}

impl GuiClient {
    pub fn stop(&self, alias: &str) -> Result<Value, Fault> {
        if !alias_valid(alias) {
            return Err(invalid());
        }
        self.call("gui.runtime_stop", json!({"alias":alias}))
    }
    /// Reads the existing host capability without creating a capability.
    pub fn open(root: &Path) -> Result<Self, Fault> {
        let path = root.join("gui-host");
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(unavailable());
            }
            Err(_) => return Err(storage()),
            Ok(_) => {}
        }
        let store = CredentialStore::open(&path).map_err(|_| storage())?;
        let capability = read_capability(&store)?.ok_or_else(unavailable)?;
        Ok(Self {
            root: root.to_path_buf(),
            capability,
        })
    }

    pub fn setup_status(&self) -> Result<Value, Fault> {
        self.call("gui.setup_status", json!({}))
    }

    pub fn window_geometry_get(&self) -> Result<Option<WindowGeometry>, Fault> {
        match self.call("gui.window_geometry_get", json!({})) {
            Ok(value) => serde_json::from_value(value["geometry"].clone()).map_err(|_| invalid()),
            Err(error) if error.code == "CONNECTION_ERROR" => {
                crate::window_geometry_get(&self.root)
            }
            Err(error) => Err(error),
        }
    }

    pub fn window_geometry_set(&self, geometry: &WindowGeometry) -> Result<(), Fault> {
        match self.call("gui.window_geometry_set", json!({"geometry":geometry})) {
            Ok(_) => Ok(()),
            Err(error) if error.code == "CONNECTION_ERROR" => {
                crate::window_geometry_set(&self.root, geometry)
            }
            Err(error) => Err(error),
        }
    }

    pub fn startup_preference_get(&self) -> Result<Option<bool>, Fault> {
        match self.call("gui.startup_preference_get", json!({})) {
            Ok(value) => serde_json::from_value(value["enabled"].clone()).map_err(|_| invalid()),
            Err(error) if error.code == "CONNECTION_ERROR" => {
                crate::startup_preference_get(&self.root)
            }
            Err(error) => Err(error),
        }
    }

    pub fn startup_preference_set(&self, enabled: bool) -> Result<(), Fault> {
        match self.call("gui.startup_preference_set", json!({"enabled":enabled})) {
            Ok(_) => Ok(()),
            Err(error) if error.code == "CONNECTION_ERROR" => {
                crate::startup_preference_set(&self.root, enabled)
            }
            Err(error) => Err(error),
        }
    }

    pub fn google_begin_recovery(&self, name: &str) -> Result<Value, Fault> {
        self.call("gui.google_begin_recovery", json!({"name":name}))
    }

    pub fn issue(&self, alias: &str, intent: &str, request: Request) -> Result<Value, Fault> {
        if request.auth.is_some() {
            return Err(invalid());
        }
        self.call(
            "gui.issue",
            json!({"alias":alias,"intent":intent,"request":request}),
        )
    }

    pub fn request(&self, alias: &str, request: Request) -> Result<Value, Fault> {
        if request.auth.is_some() {
            return Err(invalid());
        }
        if matches!(
            request.operation.as_str(),
            "ui.preferences.get" | "ui.preferences.set"
        ) {
            let endpoint = crate::gui_pipe_name(&self.root).map_err(|_| unavailable())?;
            let connection = match pipe::connect(&endpoint) {
                Ok(connection) => connection,
                // The fallback is intentionally limited to failure before a
                // pipe is connected, so a sent request is never replayed.
                Err(_) => return crate::ui_preferences(&self.root, alias, request),
            };
            return self.call_connected(
                "gui.request",
                json!({"alias":alias,"request":request}),
                connection,
            );
        }
        self.call("gui.request", json!({"alias":alias,"request":request}))
    }

    /// Administrator-only backup controls, authenticated with saved human credentials.
    pub fn backup(
        &self,
        alias: &str,
        account_id: &str,
        operation: &str,
        args: Value,
    ) -> Result<Value, Fault> {
        self.call(
            "gui.backup",
            json!({"alias":alias,"account_id":account_id,"operation":operation,"args":args}),
        )
    }

    pub fn reveal_recovery_code(&self, alias: &str) -> Result<Value, Fault> {
        if !alias_valid(alias) {
            return Err(invalid());
        }
        self.call("gui.reveal_recovery_code", json!({"alias":alias}))
    }

    fn call(&self, operation: &str, args: Value) -> Result<Value, Fault> {
        let endpoint = crate::gui_pipe_name(&self.root).map_err(|_| unavailable())?;
        let connection = pipe::connect(&endpoint).map_err(|_| unavailable())?;
        self.call_connected(operation, args, connection)
    }

    fn call_connected(
        &self,
        operation: &str,
        args: Value,
        mut connection: pipe::Connection,
    ) -> Result<Value, Fault> {
        if operation == "gui.backup" {
            connection.set_timeout(std::time::Duration::from_secs(300));
        }
        if (operation == "gui.request" && args["request"]["operation"] == "account.google.link")
            || (operation == "gui.issue"
                && args["request"]["operation"] == "account.google.recover")
        {
            connection.set_timeout(std::time::Duration::from_secs(30));
        }
        if operation == "gui.request" && args["request"]["operation"] == "changes.wait" {
            let timeout = args["request"]["args"]
                .get("timeout")
                .and_then(Value::as_u64)
                .unwrap_or(60);
            if (1..=300).contains(&timeout) {
                connection.set_timeout(std::time::Duration::from_secs(timeout + 10));
            }
        }
        let request = Request {
            operation: operation.to_owned(),
            auth: Some(
                std::str::from_utf8(&self.capability)
                    .map_err(|_| storage())?
                    .to_owned(),
            ),
            args,
            context: json!({}),
            request_id: None,
        };
        write_request(&mut connection, &request).map_err(|_| unavailable())?;
        let response = read_response(&mut connection).map_err(|_| unavailable())?;
        response_result(response)
    }
}

fn response_result(response: RpcResponse) -> Result<Value, Fault> {
    if response.schema_version != swarm_ipc::SCHEMA_VERSION {
        return Err(invalid());
    }
    match (response.ok, response.data, response.error) {
        (true, Some(data), None) => Ok(data),
        (false, None, Some(error)) => Err(error),
        _ => Err(invalid()),
    }
}
