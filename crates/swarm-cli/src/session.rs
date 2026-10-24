use crate::{RequestSpec, fault};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::fmt;
use std::fs::File;
use std::path::Path;
use swarm_core::{CredentialCandidate, protocol::Request};

const MAX_PENDING_BYTES: usize = 64 * 1024;

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct PendingIntent {
    intent: String,
    operation: String,
    args: Value,
}

impl fmt::Debug for PendingIntent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingIntent")
            .field("intent", &"[redacted]")
            .field("operation", &self.operation)
            .field("args", &"[redacted]")
            .finish()
    }
}

pub(super) fn valid_alias(alias: &str) -> bool {
    (1..=128).contains(&alias.len())
        && alias
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn internal_error() -> swarm_core::protocol::Fault {
    fault(
        "INTERNAL_ERROR",
        8,
        "Identity request could not be completed",
    )
}

fn conflict() -> swarm_core::protocol::Fault {
    fault(
        "CONFLICT",
        6,
        "An identity request conflicts for this session alias",
    )
}

#[cfg(windows)]
fn acquire_alias_lease(root: &Path, alias: &str) -> Result<File, swarm_core::protocol::Fault> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

    if !valid_alias(alias) {
        return Err(crate::invalid_input());
    }
    let path = root.join(format!(".identity-{alias}.lock"));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .custom_flags(0x00200000) // FILE_FLAG_OPEN_REPARSE_POINT
        .open(path)
        .map_err(|error| {
            if matches!(error.raw_os_error(), Some(32 | 33)) {
                conflict()
            } else {
                internal_error()
            }
        })?;
    let metadata = file.metadata().map_err(|_| internal_error())?;
    if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
        return Err(internal_error());
    }
    Ok(file)
}

#[cfg(not(windows))]
fn acquire_alias_lease(_root: &Path, _alias: &str) -> Result<File, swarm_core::protocol::Fault> {
    Err(internal_error())
}

fn new_intent() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}

fn decode_pending(bytes: &[u8]) -> Result<PendingIntent, swarm_core::protocol::Fault> {
    if bytes.len() > MAX_PENDING_BYTES {
        return Err(internal_error());
    }
    let pending: PendingIntent = serde_json::from_slice(bytes).map_err(|_| internal_error())?;
    if pending.intent.len() != 64
        || !pending.intent.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !matches!(
            pending.operation.as_str(),
            "identity.start" | "identity.resume"
        )
        || !pending.args.is_object()
    {
        return Err(internal_error());
    }
    Ok(pending)
}

fn encode_pending(pending: &PendingIntent) -> Result<Vec<u8>, swarm_core::protocol::Fault> {
    let bytes = serde_json::to_vec(pending).map_err(|_| internal_error())?;
    if bytes.len() > MAX_PENDING_BYTES {
        return Err(internal_error());
    }
    Ok(bytes)
}

fn same_requested_args(requested: &Value, stored: &Value) -> bool {
    [
        "actor_id",
        "project_id",
        "model_name",
        "model_report",
        "model_client",
        "model_local",
    ]
    .into_iter()
    .all(|key| requested.get(key) == stored.get(key))
}

fn wire_operation(operation: &str) -> &'static str {
    match operation {
        "identity.start" => "session.start",
        "identity.resume" => "session.resume",
        _ => unreachable!("pending operation was validated"),
    }
}

fn candidate_from_store(
    store: &swarm_platform::CredentialStore,
    alias: &str,
) -> Result<Option<CredentialCandidate>, swarm_core::protocol::Fault> {
    let bytes = credential_bytes(store, alias)?;
    bytes
        .map(|bytes| {
            crate::candidate_from_bytes(&bytes).map_err(|_| {
                fault(
                    "AUTHENTICATION_REQUIRED",
                    3,
                    "A valid AI session is required",
                )
            })
        })
        .transpose()
}

fn credential_bytes(
    store: &swarm_platform::CredentialStore,
    alias: &str,
) -> Result<Option<Vec<u8>>, swarm_core::protocol::Fault> {
    store.get(alias).map_err(|_| internal_error())
}

fn ai_candidate(
    candidate: CredentialCandidate,
) -> Result<CredentialCandidate, swarm_core::protocol::Fault> {
    if matches!(candidate, CredentialCandidate::Ai { .. }) {
        Ok(candidate)
    } else {
        Err(fault(
            "AUTHENTICATION_REQUIRED",
            3,
            "A valid AI session is required",
        ))
    }
}

fn make_pending(
    operation: &str,
    requested: &Value,
    credentials: &swarm_platform::CredentialStore,
    alias: &str,
) -> Result<PendingIntent, swarm_core::protocol::Fault> {
    let mut args = requested.clone();
    if operation == "identity.start" {
        if credential_bytes(credentials, alias)?.is_some() {
            return Err(conflict());
        }
    } else {
        let candidate = candidate_from_store(credentials, alias)?.ok_or_else(|| {
            fault(
                "AUTHENTICATION_REQUIRED",
                3,
                "A valid AI session is required",
            )
        })?;
        let CredentialCandidate::Ai {
            id,
            project_id,
            resume_secret,
            ..
        } = ai_candidate(candidate)?
        else {
            unreachable!()
        };
        if requested.get("actor_id").and_then(Value::as_str) != Some(id.as_str())
            || requested.get("project_id").and_then(Value::as_str) != Some(project_id.as_str())
        {
            return Err(fault(
                "AUTHENTICATION_REQUIRED",
                3,
                "A valid AI session is required",
            ));
        }
        args.as_object_mut()
            .ok_or_else(internal_error)?
            .insert("resume_secret".into(), Value::String(resume_secret));
    }
    args.as_object_mut()
        .ok_or_else(internal_error)?
        .insert("alias".into(), Value::String(alias.to_owned()));
    Ok(PendingIntent {
        intent: new_intent(),
        operation: operation.to_owned(),
        args,
    })
}

fn safe_metadata(data: Value) -> Result<Value, swarm_core::protocol::Fault> {
    let (identity, nested) = match data {
        Value::Object(mut object) if object.contains_key("identity") => {
            let identity = object.remove("identity").ok_or_else(internal_error)?;
            (identity, true)
        }
        value => (value, false),
    };
    let source = identity.as_object().ok_or_else(internal_error)?;
    const PUBLIC_FIELDS: &[&str] = &[
        "id",
        "project_id",
        "model_name",
        "model_source",
        "display_id",
        "expires_at",
        "created_at",
    ];
    let mut public = Map::new();
    for field in PUBLIC_FIELDS {
        if let Some(value) = source.get(*field) {
            if !value.is_string() {
                return Err(internal_error());
            }
            public.insert((*field).to_owned(), value.clone());
        }
    }
    if !public.contains_key("id") || !public.contains_key("project_id") {
        return Err(internal_error());
    }
    Ok(if nested {
        json!({"identity": Value::Object(public)})
    } else {
        Value::Object(public)
    })
}

pub(super) fn execute(spec: RequestSpec) -> Result<(String, Value), swarm_core::protocol::Fault> {
    if !valid_alias(&spec.alias) {
        return Err(crate::invalid_input());
    }
    swarm_runtime::lifecycle::check_running_allowed(&spec.data_dir)?;
    let intents = swarm_platform::CredentialStore::open(&spec.data_dir.join("client-intents"))
        .map_err(|_| internal_error())?;
    let intents_root = std::fs::canonicalize(spec.data_dir.join("client-intents"))
        .map_err(|_| internal_error())?;
    // Keep the exclusive file handle alive through receipt verification and pending removal.
    let _lease = acquire_alias_lease(&intents_root, &spec.alias)?;
    let credentials =
        swarm_platform::CredentialStore::open(&spec.data_dir.join("client-credentials"))
            .map_err(|_| internal_error())?;

    let pending = match intents.get(&spec.alias).map_err(|_| internal_error())? {
        Some(bytes) => {
            let pending = decode_pending(&bytes)?;
            if pending.operation != spec.operation
                || pending.args.get("alias").and_then(Value::as_str) != Some(spec.alias.as_str())
                || !same_requested_args(&spec.args, &pending.args)
            {
                return Err(conflict());
            }
            pending
        }
        None => {
            let pending = make_pending(&spec.operation, &spec.args, &credentials, &spec.alias)?;
            intents
                .put(&spec.alias, &encode_pending(&pending)?)
                .map_err(|_| internal_error())?;
            pending
        }
    };

    let mut stream = swarm_runtime::lifecycle::connect(&spec.data_dir)?;
    let request = Request {
        operation: wire_operation(&pending.operation).into(),
        auth: Some(pending.intent.clone()),
        args: pending.args.clone(),
        context: spec.context,
        request_id: None,
    };
    swarm_ipc::write_request(&mut stream, &request)?;
    let response = swarm_ipc::read_response(&mut stream).map_err(|_| {
        fault(
            "CONNECTION_ERROR",
            8,
            "Swarm runtime did not complete the request",
        )
    })?;
    if response.schema_version != 1 {
        return Err(fault(
            "INVALID_FRAME",
            2,
            "Invalid or unsupported IPC response",
        ));
    }
    if !response.ok {
        return Err(response
            .error
            .unwrap_or_else(|| fault("INTERNAL_ERROR", 8, "Request failed")));
    }
    let data = response
        .data
        .ok_or_else(|| fault("INVALID_FRAME", 2, "Invalid or unsupported IPC response"))?;

    let saved_bytes = credential_bytes(&credentials, &spec.alias)?.ok_or_else(internal_error)?;
    let saved = crate::candidate_from_bytes(&saved_bytes).map_err(|_| internal_error())?;
    let CredentialCandidate::Ai {
        id,
        project_id,
        session_token,
        resume_secret,
        ..
    } = saved
    else {
        return Err(internal_error());
    };
    if session_token.trim().is_empty() || resume_secret.trim().is_empty() {
        return Err(internal_error());
    }
    if pending.operation == "identity.resume"
        && (pending.args.get("actor_id").and_then(Value::as_str) != Some(id.as_str())
            || pending.args.get("project_id").and_then(Value::as_str) != Some(project_id.as_str()))
    {
        return Err(internal_error());
    }
    if pending.operation == "identity.start"
        && pending.args.get("project_id").and_then(Value::as_str) != Some(project_id.as_str())
    {
        return Err(internal_error());
    }
    let safe = safe_metadata(data)?;
    let public_identity = safe.get("identity").unwrap_or(&safe);
    if public_identity.get("id").and_then(Value::as_str) != Some(id.as_str())
        || public_identity.get("project_id").and_then(Value::as_str) != Some(project_id.as_str())
    {
        return Err(internal_error());
    }
    intents.remove(&spec.alias).map_err(|_| internal_error())?;
    Ok((pending.operation, safe))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_key_accepts_only_bounded_ascii_key_characters() {
        assert!(valid_alias("a"));
        assert!(valid_alias("Agent_01-test"));
        assert!(!valid_alias(""));
        assert!(!valid_alias("é"));
        assert!(!valid_alias("a/b"));
        assert!(!valid_alias(&"a".repeat(129)));
    }

    #[test]
    fn pending_is_strict_bounded_and_redacts_intent_and_private_args() {
        let pending = PendingIntent {
            intent: "a".repeat(64),
            operation: "identity.resume".into(),
            args: json!({"actor_id":"actor","project_id":"project","resume_secret":"private"}),
        };
        let debug = format!("{pending:?}");
        assert!(!debug.contains(&pending.intent));
        assert!(!debug.contains("private"));
        let bytes = encode_pending(&pending).unwrap();
        assert_eq!(decode_pending(&bytes).unwrap(), pending);
        assert!(
            decode_pending(
                br#"{"intent":"x","operation":"identity.start","args":{},"extra":true}"#
            )
            .is_err()
        );
        assert!(decode_pending(&vec![b'x'; MAX_PENDING_BYTES + 1]).is_err());
    }

    #[test]
    fn retry_requires_same_operation_and_user_supplied_arguments() {
        let original = json!({"actor_id":"actor","project_id":"project","model_name":"model","resume_secret":"old-secret"});
        assert!(same_requested_args(
            &json!({"actor_id":"actor","project_id":"project","model_name":"model"}),
            &original
        ));
        assert!(!same_requested_args(
            &json!({"actor_id":"other","project_id":"project","model_name":"model"}),
            &original
        ));
        let reported = json!({"project_id":"project","model_report":{"client":"codex","session_id":"one","model_name":"model"}});
        let mut changed = reported.clone();
        changed["model_report"]["session_id"] = json!("two");
        assert!(!same_requested_args(&changed, &reported));
        assert!(!same_requested_args(
            &json!({"project_id":"project","model_client":"codex"}),
            &json!({"project_id":"project","model_client":"antigravity"})
        ));
        assert!(!same_requested_args(
            &json!({"actor_id":"actor","project_id":"project"}),
            &original
        ));
    }

    #[test]
    fn resume_pending_preserves_the_original_secret_when_current_credential_changes() {
        let old = PendingIntent {
            intent: "f".repeat(64),
            operation: "identity.resume".into(),
            args: json!({"actor_id":"actor","project_id":"project","resume_secret":"old-secret"}),
        };
        let decoded = decode_pending(&encode_pending(&old).unwrap()).unwrap();
        assert_eq!(decoded.args["resume_secret"], "old-secret");
        assert!(!format!("{decoded:?}").contains("old-secret"));
    }

    #[test]
    fn public_metadata_filter_drops_any_private_grant_fields() {
        let result = safe_metadata(json!({
            "identity":{"id":"actor","project_id":"project","display_id":"A1"},
            "session_id":"private-session",
            "session_token":"private-token",
            "resume_secret":"private-resume"
        }))
        .unwrap();
        assert_eq!(
            result,
            json!({"identity":{"id":"actor","project_id":"project","display_id":"A1"}})
        );
        let rendered = result.to_string();
        for secret in ["private-session", "private-token", "private-resume"] {
            assert!(!rendered.contains(secret));
        }
    }

    #[cfg(windows)]
    #[test]
    fn alias_lease_contention_returns_conflict_without_changing_pending_intent() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("client-intents");
        let store = swarm_platform::CredentialStore::open(&root).unwrap();
        let alias = "agent_1";
        let pending = PendingIntent {
            intent: "b".repeat(64),
            operation: "identity.start".into(),
            args: json!({"alias":alias,"project_id":"project"}),
        };
        let bytes = encode_pending(&pending).unwrap();
        store.put(alias, &bytes).unwrap();
        let canonical_root = std::fs::canonicalize(&root).unwrap();

        let lease = acquire_alias_lease(&canonical_root, alias).unwrap();
        let contention = acquire_alias_lease(&canonical_root, alias).unwrap_err();
        assert_eq!(contention.code, "CONFLICT");
        assert_eq!(store.get(alias).unwrap().as_deref(), Some(bytes.as_slice()));

        drop(lease);
        assert!(acquire_alias_lease(&canonical_root, alias).is_ok());
    }
}
