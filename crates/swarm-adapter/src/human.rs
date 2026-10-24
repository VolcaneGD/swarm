//! Private credential and request bridge for a future local GUI host.
//!
//! This module is a Rust-only adapter. It does not define a GUI, IPC endpoint,
//! or public credential surface.
use crate::CredentialBroker;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
use swarm_core::{
    CredentialCandidate, Service,
    protocol::{Fault, Request},
};
use swarm_platform::CredentialStore;

const MEMO_KEY: &str = "human-bridge-memos";
const MAX_MEMO_BYTES: usize = 64 * 1024;

struct MemoBuffer(Vec<u8>);

impl std::io::Write for MemoBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_MEMO_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("Private memo limit exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct MemoFile {
    version: u32,
    entries: BTreeMap<String, HumanMemo>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HumanMemo {
    fingerprint: String,
    candidate: HumanCandidate,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HumanCandidate {
    id: String,
    token: String,
    recovery_code: Option<String>,
}

impl std::fmt::Debug for HumanMemo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HumanMemo { [redacted] }")
    }
}

impl std::fmt::Debug for HumanCandidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HumanCandidate([redacted])")
    }
}

impl HumanCandidate {
    fn from_candidate(candidate: &CredentialCandidate) -> Result<Self, Fault> {
        match candidate {
            CredentialCandidate::Human {
                id,
                token,
                recovery_code,
            } => Ok(Self {
                id: id.clone(),
                token: token.clone(),
                recovery_code: recovery_code.clone(),
            }),
            CredentialCandidate::Ai { .. } => Err(bridge_fault()),
        }
    }

    fn into_candidate(self) -> CredentialCandidate {
        CredentialCandidate::Human {
            id: self.id,
            token: self.token,
            recovery_code: self.recovery_code,
        }
    }
}

pub struct HumanBridge {
    store: CredentialStore,
}

impl std::fmt::Debug for HumanBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HumanBridge { [redacted] }")
    }
}

fn bridge_fault() -> Fault {
    Fault::new("HUMAN_BRIDGE_FAILED", 8, "Human operation failed")
}

fn invalid_fault() -> Fault {
    Fault::new("INVALID_INPUT", 2, "Human request is invalid")
}

fn authentication_fault() -> Fault {
    Fault::new(
        "AUTHENTICATION_REQUIRED",
        3,
        "A valid human login is required",
    )
}

fn recovery_permission_fault() -> Fault {
    Fault::new(
        "PERMISSION_DENIED",
        4,
        "Recovery codes are available only to the main Admin",
    )
}

fn conflict_fault() -> Fault {
    Fault::new("CONFLICT", 6, "Human operation conflicts with prior state")
}

fn sanitize_fault(fault: Fault) -> Fault {
    Fault::new(&fault.code, fault.exit_code, "Human operation failed")
}

fn valid_alias(alias: &str) -> bool {
    (1..=128).contains(&alias.len())
        && alias
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn intent_hash(intent: &str) -> Result<String, Fault> {
    if intent.len() != 64 || !intent.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid_fault());
    }
    Ok(hex_digest(intent.to_ascii_lowercase().as_bytes()))
}

fn slot(alias: &str) -> Result<String, Fault> {
    if !valid_alias(alias) {
        return Err(invalid_fault());
    }
    Ok(format!("human-{}", hex_digest(alias.as_bytes())))
}

fn operation_fingerprint(alias: &str, request: &Request) -> Result<String, Fault> {
    let canonical = serde_json::to_vec(&(alias, request.operation.as_str(), &request.args))
        .map_err(|_| bridge_fault())?;
    Ok(hex_digest(canonical))
}

fn load_memos(store: &CredentialStore) -> Result<MemoFile, Fault> {
    let Some(bytes) = store.get(MEMO_KEY).map_err(|_| bridge_fault())? else {
        return Ok(MemoFile {
            version: 1,
            entries: BTreeMap::new(),
        });
    };
    if bytes.len() > MAX_MEMO_BYTES {
        return Err(bridge_fault());
    }
    let file: MemoFile = serde_json::from_slice(&bytes).map_err(|_| bridge_fault())?;
    if file.version != 1
        || file.entries.iter().any(|(key, memo)| {
            key.len() != 64
                || !key.bytes().all(|byte| byte.is_ascii_hexdigit())
                || memo.fingerprint.len() != 64
                || !memo
                    .fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit())
                || memo.candidate.id.trim().is_empty()
                || memo.candidate.id.len() > 4096
                || memo.candidate.token.trim().is_empty()
                || memo.candidate.token.len() > 4096
                || memo
                    .candidate
                    .recovery_code
                    .as_ref()
                    .is_some_and(|code| code.trim().is_empty() || code.len() > 4096)
        })
    {
        return Err(bridge_fault());
    }
    Ok(file)
}

fn save_memos(store: &CredentialStore, file: &MemoFile) -> Result<(), Fault> {
    let mut bytes = MemoBuffer(Vec::new());
    serde_json::to_writer(&mut bytes, file).map_err(|_| bridge_fault())?;
    store.put(MEMO_KEY, &bytes.0).map_err(|_| bridge_fault())
}

fn issuance_operation(operation: &str) -> bool {
    matches!(
        operation,
        "account.setup"
            | "account.login"
            | "account.password"
            | "account.recover"
            | "account.google.recover"
    )
}

fn gui_operation(operation: &str) -> bool {
    matches!(
        operation,
        "account.status"
            | "account.google.begin"
            | "account.google.link"
            | "account.google.unlink"
            | "account.get"
            | "account.list"
            | "account.logout"
            | "account.create"
            | "account.reset"
            | "account.disable"
            | "role.create"
            | "role.list"
            | "role.assign"
            | "permission.set"
            | "permission.list"
            | "project.register"
            | "project.alias"
            | "project.root"
            | "project.list"
            | "project.get"
            | "board.create"
            | "board.edit"
            | "board.delete"
            | "board.restore"
            | "board.purge"
            | "board.list"
            | "board.get"
            | "thread.create"
            | "thread.edit"
            | "thread.delete"
            | "thread.restore"
            | "thread.purge"
            | "thread.list"
            | "thread.get"
            | "post.create"
            | "post.edit"
            | "post.delete"
            | "post.restore"
            | "post.list"
            | "post.page"
            | "post.get"
            | "post.history"
            | "post.deleted_body"
            | "response.create"
            | "response.list"
            | "search"
            | "changes.get"
            | "changes.wait"
            | "conversation.capabilities"
            | "conversation.deleted"
            | "conversation.batch"
            | "conversation.pins"
            | "conversation.activity"
            | "conversation.subscriptions"
            | "conversation.unread"
            | "notification.settings.get"
            | "thread.subscribe"
            | "post.read.mark"
            | "notification.settings.set"
            | "retention.settings.get"
            | "retention.settings.set"
            | "display.settings.get"
            | "display.settings.set"
            | "ui.preferences.get"
            | "ui.preferences.set"
            | "thread.pin"
            | "management.capabilities"
            | "authorization.capabilities"
            | "rules.capabilities"
            | "rules.get"
            | "rules.subjects"
            | "rules.check"
            | "rules.validate"
            | "rules.save"
    )
}

impl HumanBridge {
    /// Open the per-user DPAPI memo store at `<root>/gui-credentials`.
    pub fn open(root: &Path) -> Result<Self, Fault> {
        let path = root.join("gui-credentials");
        let store = CredentialStore::open(&path).map_err(|_| bridge_fault())?;
        Ok(Self { store })
    }

    /// Resolves a saved GUI human candidate for trusted notification selection.
    /// The credential stays inside the Rust adapter/runtime boundary.
    pub fn notification_candidate(
        &mut self,
        service: &mut Service,
        broker: &mut CredentialBroker,
        alias: &str,
    ) -> Result<Option<CredentialCandidate>, Fault> {
        let slot = slot(alias)?;
        let Some(candidate) = broker.credential(service, &slot).map_err(sanitize_fault)? else {
            return Ok(None);
        };
        match &candidate {
            CredentialCandidate::Human { .. }
                if service
                    .validate_credential_candidate(&candidate)
                    .map_err(sanitize_fault)?
                    .session =>
            {
                Ok(Some(candidate))
            }
            CredentialCandidate::Human { .. } | CredentialCandidate::Ai { .. } => Ok(None),
        }
    }

    /// Issue one human credential operation with a durable private replay memo.
    pub fn issue(
        &mut self,
        service: &mut Service,
        broker: &mut CredentialBroker,
        alias: &str,
        intent: &str,
        mut request: Request,
    ) -> Result<Value, Fault> {
        let slot = slot(alias)?;
        let key = intent_hash(intent)?;
        if !issuance_operation(&request.operation)
            || request.auth.is_some()
            || request.request_id.is_some()
            || request.context != json!({})
        {
            return Err(invalid_fault());
        }
        let fingerprint = operation_fingerprint(alias, &request)?;
        let receipt =
            hex_digest([b"Swarm.HumanBridge.Receipt.v1\0".as_slice(), key.as_bytes()].concat());
        let mut memos = load_memos(&self.store)?;

        if let Some(memo) = memos.entries.get(&key) {
            if memo.fingerprint != fingerprint {
                return Err(conflict_fault());
            }
            let candidate = memo.candidate.clone().into_candidate();
            let validity = service
                .validate_credential_candidate(&candidate)
                .map_err(sanitize_fault)?;
            match service
                .credential_receipt(&receipt)
                .map_err(sanitize_fault)?
            {
                Some(public) if validity.session => return Ok(public),
                Some(_) => return Err(authentication_fault()),
                None if validity.session => return Err(conflict_fault()),
                None => {}
            }
        }

        if !memos.entries.contains_key(&key)
            && service
                .credential_receipt(&receipt)
                .map_err(sanitize_fault)?
                .is_some()
        {
            return Err(conflict_fault());
        }

        if request.operation == "account.password" {
            let candidate = broker
                .credential(service, &slot)
                .map_err(sanitize_fault)?
                .ok_or_else(authentication_fault)?;
            let validity = service
                .validate_credential_candidate(&candidate)
                .map_err(sanitize_fault)?;
            if !validity.session {
                return Err(authentication_fault());
            }
            match candidate {
                CredentialCandidate::Human { token, .. } => request.auth = Some(token),
                CredentialCandidate::Ai { .. } => return Err(bridge_fault()),
            }
        }

        let public = broker
            .issue_with_receipt(service, &slot, request, &receipt, |candidate| {
                let candidate = HumanCandidate::from_candidate(candidate)?;
                memos.entries.insert(
                    key.clone(),
                    HumanMemo {
                        fingerprint: fingerprint.clone(),
                        candidate,
                    },
                );
                save_memos(&self.store, &memos)
            })
            .map_err(sanitize_fault)?;
        Ok(public)
    }

    /// Reveals a saved recovery code only after validating both the current
    /// human session and the code against the live main Admin account.
    pub fn reveal_recovery_code(
        &mut self,
        service: &mut Service,
        broker: &mut CredentialBroker,
        alias: &str,
    ) -> Result<Value, Fault> {
        let slot = slot(alias)?;
        let candidate = broker
            .credential(service, &slot)
            .map_err(sanitize_fault)?
            .ok_or_else(authentication_fault)?;
        let validity = service
            .validate_credential_candidate(&candidate)
            .map_err(sanitize_fault)?;
        if !validity.session {
            return Err(authentication_fault());
        }

        let id = match &candidate {
            CredentialCandidate::Human { id, .. } => id,
            CredentialCandidate::Ai { .. } => return Err(authentication_fault()),
        };
        let account = self.request(
            service,
            broker,
            alias,
            Request {
                operation: "account.status".into(),
                auth: None,
                args: json!({}),
                context: json!({}),
                request_id: None,
            },
        )?;
        if account.get("id").and_then(Value::as_str) != Some(id.as_str()) {
            return Err(authentication_fault());
        }
        if account.get("is_admin").and_then(Value::as_bool) != Some(true) {
            return Err(recovery_permission_fault());
        }

        let latest = service
            .validate_credential_candidate(&candidate)
            .map_err(sanitize_fault)?;
        let recovery_code = match &candidate {
            CredentialCandidate::Human { recovery_code, .. }
                if latest.session && latest.recovery =>
            {
                recovery_code
            }
            CredentialCandidate::Human { .. } | CredentialCandidate::Ai { .. } => {
                return Err(authentication_fault());
            }
        }
        .as_deref()
        .ok_or_else(authentication_fault)?;
        Ok(json!({"recovery_code":recovery_code}))
    }

    /// Execute one allowlisted GUI operation as a saved, currently valid human.
    pub fn request(
        &mut self,
        service: &mut Service,
        broker: &mut CredentialBroker,
        alias: &str,
        mut request: Request,
    ) -> Result<Value, Fault> {
        let slot = slot(alias)?;
        if !gui_operation(&request.operation) || request.auth.is_some() {
            return Err(invalid_fault());
        }
        let candidate = broker
            .credential(service, &slot)
            .map_err(sanitize_fault)?
            .ok_or_else(authentication_fault)?;
        let validity = service
            .validate_credential_candidate(&candidate)
            .map_err(sanitize_fault)?;
        match candidate {
            CredentialCandidate::Human { token, .. } if validity.session => {
                request.auth = Some(token);
            }
            CredentialCandidate::Human { .. } | CredentialCandidate::Ai { .. } => {
                return Err(authentication_fault());
            }
        }
        service.request(request).map_err(sanitize_fault)
    }
}
