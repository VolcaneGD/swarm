//! Private issuance intents. Public replies contain only receipt metadata.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use swarm_adapter::CredentialBroker;
use swarm_adapter::human::HumanBridge;
use swarm_core::{
    CredentialCandidate, NotificationBatch, NotificationDelivery, Service,
    protocol::{Fault, Request},
};
use swarm_platform::CredentialStore;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Memo {
    version: u32,
    fingerprint: String,
    alias: String,
    candidate: CredentialCandidate,
}

pub struct SessionBridge {
    root: PathBuf,
    broker: CredentialBroker,
    human: HumanBridge,
    memos: CredentialStore,
    clients: CredentialStore,
    gui_state: CredentialStore,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GuiSelection {
    alias: String,
}
fn storage() -> Fault {
    Fault::new("CREDENTIAL_STORAGE", 8, "Private credential handoff failed")
}
fn invalid() -> Fault {
    Fault::new("INVALID_INPUT", 2, "Invalid session request")
}
fn conflict() -> Fault {
    Fault::new(
        "SESSION_CONFLICT",
        6,
        "Session alias or issuance request conflicts",
    )
}
fn authentication() -> Fault {
    Fault::new(
        "AUTHENTICATION_REQUIRED",
        3,
        "A valid session credential is required",
    )
}
fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str, Fault> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty() && v.len() <= 4096)
        .ok_or_else(invalid)
}
fn alias_valid(alias: &str) -> bool {
    (1..=128).contains(&alias.len())
        && alias
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl SessionBridge {
    pub fn open(root: &Path, service: &mut Service) -> Result<Self, Fault> {
        let mut broker = CredentialBroker::open(&root.join("credentials"))?;
        broker.recover(service)?;
        let memos = CredentialStore::open(&root.join("credentials")).map_err(|_| storage())?;
        let clients =
            CredentialStore::open(&root.join("client-credentials")).map_err(|_| storage())?;
        let mut sessions = Self {
            root: root.to_owned(),
            broker,
            human: HumanBridge::open(root)?,
            memos,
            clients,
            gui_state: CredentialStore::open(&root.join("gui-state")).map_err(|_| storage())?,
        };
        {
            let _gate = sessions.notification_selection_gate()?;
            sessions.synchronize_notification_recipient_locked(service)?;
        }
        Ok(sessions)
    }

    /// Acquires the shared short-lived gate used by GuiHost selection writes.
    pub(crate) fn notification_selection_gate(&self) -> Result<std::fs::File, Fault> {
        self.gui_state
            .lock("notification-selection-gate")
            .map_err(|_| storage())
    }

    fn selected_alias(&self) -> Result<Option<String>, Fault> {
        // Any pending intent, including malformed bytes, suppresses delivery.
        if self
            .gui_state
            .get("pending-auth")
            .map_err(|_| storage())?
            .is_some()
        {
            return Ok(None);
        }
        let Some(bytes) = self.gui_state.get("selection").map_err(|_| storage())? else {
            return Ok(None);
        };
        let selected: GuiSelection = serde_json::from_slice(&bytes).map_err(|_| storage())?;
        if !alias_valid(&selected.alias) {
            return Err(storage());
        }
        Ok(Some(selected.alias))
    }

    fn resolve_notification_candidate(
        &mut self,
        service: &mut Service,
    ) -> Result<Option<CredentialCandidate>, Fault> {
        let Some(alias) = self.selected_alias()? else {
            return Ok(None);
        };
        self.human
            .notification_candidate(service, &mut self.broker, &alias)
    }

    /// Must be called while `notification_selection_gate` is held.
    pub(crate) fn synchronize_notification_recipient_locked(
        &mut self,
        service: &mut Service,
    ) -> Result<Option<CredentialCandidate>, Fault> {
        if let Some(candidate) = self.resolve_notification_candidate(service)? {
            match service.select_notification_recipient(&candidate) {
                Ok(()) => return Ok(Some(candidate)),
                Err(error)
                    if matches!(
                        error.code.as_str(),
                        "AUTHENTICATION_REQUIRED" | "PERMISSION_DENIED"
                    ) =>
                {
                    service.clear_notification_recipient()?;
                    return Ok(None);
                }
                Err(error) => return Err(error),
            }
        }
        // Verified absence, pending authentication, stale credentials, disabled
        // accounts, and AI candidates revoke old recipient authority. Storage
        // failures propagate above without deleting durable pending work.
        service.clear_notification_recipient()?;
        Ok(None)
    }

    /// Trusted runtime worker hook. No renderer or AI RPC exposes this path.
    /// Caller must retain the runtime SharedService mutex through this call.
    /// The sink must not call back into runtime IPC. A successful return means
    /// OS submission only, never proof that the user saw the notification.
    pub fn deliver_notification_batch(
        &mut self,
        service: &mut Service,
        now_ms: i64,
        window_ms: u32,
        sink: &mut dyn FnMut(&NotificationBatch) -> Result<(), Fault>,
    ) -> Result<bool, Fault> {
        crate::lifecycle::check_running_allowed(&self.root)?;
        let _gate = self.notification_selection_gate()?;
        crate::lifecycle::check_running_allowed(&self.root)?;
        let Some(candidate) = self.synchronize_notification_recipient_locked(service)? else {
            return Ok(false);
        };
        let Some(batch) = service.prepare_notification_batch(&candidate, now_ms, window_ms)? else {
            return Ok(false);
        };
        // Selection gate and the caller's service guard remain held across
        // submission and receipt. Account changes and accepted full stop wait.
        crate::lifecycle::check_running_allowed(&self.root)?;
        match sink(&batch) {
            Ok(()) => {
                service.complete_notification_batch(&candidate, &batch.id, now_ms, true)?;
                Ok(true)
            }
            Err(error) => {
                service.complete_notification_batch(&candidate, &batch.id, now_ms, false)?;
                Err(error)
            }
        }
    }

    /// Trusted runtime worker hook. No renderer or AI RPC exposes this path.
    pub fn prepare_notifications(
        &mut self,
        service: &mut Service,
        limit: usize,
    ) -> Result<Vec<NotificationDelivery>, Fault> {
        crate::lifecycle::check_running_allowed(&self.root)?;
        let _gate = self.notification_selection_gate()?;
        let Some(candidate) = self.synchronize_notification_recipient_locked(service)? else {
            return Ok(Vec::new());
        };
        service.prepare_notifications(&candidate, limit)
    }

    /// Trusted runtime worker hook. A stale delivery cannot be acknowledged
    /// under a newly selected account because the core checks identity/generation.
    pub fn acknowledge_notification(
        &mut self,
        service: &mut Service,
        delivery_id: &str,
    ) -> Result<(), Fault> {
        crate::lifecycle::check_running_allowed(&self.root)?;
        let _gate = self.notification_selection_gate()?;
        let Some(candidate) = self.synchronize_notification_recipient_locked(service)? else {
            return Err(authentication());
        };
        service.acknowledge_notification(&candidate, delivery_id)
    }

    /// Trusted Rust GUI-host entry point; never dispatched by the AI pipe.
    pub fn gui_issue(
        &mut self,
        service: &mut Service,
        alias: &str,
        intent: &str,
        request: Request,
    ) -> Result<Value, Fault> {
        self.human
            .issue(service, &mut self.broker, alias, intent, request)
    }

    /// Selects a saved human account inside the runtime's single broker lease.
    pub fn gui_request(
        &mut self,
        service: &mut Service,
        alias: &str,
        request: Request,
    ) -> Result<Value, Fault> {
        self.human
            .request(service, &mut self.broker, alias, request)
    }

    /// Trusted Rust GUI-host action; returns no session or credential metadata.
    pub fn gui_reveal_recovery_code(
        &mut self,
        service: &mut Service,
        alias: &str,
    ) -> Result<Value, Fault> {
        self.human
            .reveal_recovery_code(service, &mut self.broker, alias)
    }

    pub fn dispatch(&mut self, service: &mut Service, request: Request) -> Result<Value, Fault> {
        let operation = match request.operation.as_str() {
            "session.start" => "identity.start",
            "session.resume" => "identity.resume",
            _ => return Err(invalid()),
        };
        let intent = request
            .auth
            .as_deref()
            .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(authentication)?;
        let alias = text(&request.args, "alias")?.to_owned();
        if !alias_valid(&alias)
            || request.request_id.is_some()
            || (!request.context.is_null() && request.context != json!({}))
        {
            return Err(invalid());
        }
        let allowed = if operation == "identity.start" {
            &[
                "alias",
                "project_id",
                "model_name",
                "model_report",
                "model_client",
                "model_local",
            ][..]
        } else {
            &[
                "alias",
                "actor_id",
                "project_id",
                "model_name",
                "model_report",
                "model_client",
                "model_local",
                "resume_secret",
            ][..]
        };
        if request
            .args
            .as_object()
            .ok_or_else(invalid)?
            .keys()
            .any(|key| !allowed.contains(&key.as_str()))
        {
            return Err(invalid());
        }
        let project = text(&request.args, "project_id")?.to_owned();
        let mut args = request.args.clone();
        args.as_object_mut().ok_or_else(invalid)?.remove("alias");
        let receipt = hash(intent.as_bytes());
        let key = format!("issuance-{receipt}");
        let fingerprint = hash(
            &serde_json::to_vec(&json!({"operation":operation,"args":args,"alias":alias}))
                .map_err(|_| invalid())?,
        );
        let prior = self
            .memos
            .get(&key)
            .map_err(|_| storage())?
            .map(|bytes| serde_json::from_slice::<Memo>(&bytes).map_err(|_| storage()))
            .transpose()?;
        if let Some(memo) = &prior
            && (memo.version != 1 || memo.alias != alias || memo.fingerprint != fingerprint)
        {
            return Err(conflict());
        }
        if let Some(public) = service.credential_receipt(&receipt)? {
            let memo = prior.ok_or_else(storage)?;
            self.finish(service, &alias, &memo.candidate)?;
            return Ok(public);
        }

        let slot = format!("client-{}", hash(alias.as_bytes()));
        let existing = match self.broker.credential(service, &slot)? {
            Some(candidate) => Some(candidate),
            None => self
                .clients
                .get(&alias)
                .map_err(|_| storage())?
                .map(|bytes| {
                    serde_json::from_slice::<CredentialCandidate>(&bytes).map_err(|_| storage())
                })
                .transpose()?,
        };
        if let Some(candidate) = existing {
            match candidate {
                CredentialCandidate::Ai { id, project_id, .. }
                    if operation == "identity.resume"
                        && id == text(&args, "actor_id")?
                        && project_id == project => {}
                _ => return Err(conflict()),
            }
        }
        if operation == "identity.resume" {
            text(&args, "actor_id")?;
            text(&args, "resume_secret")?;
        }
        let memos = &self.memos;
        let public = self.broker.issue_with_receipt(
            service,
            &slot,
            Request {
                operation: operation.into(),
                auth: None,
                args,
                context: json!({}),
                request_id: None,
            },
            &receipt,
            |candidate| {
                let memo = Memo {
                    version: 1,
                    fingerprint: fingerprint.clone(),
                    alias: alias.clone(),
                    candidate: candidate.clone(),
                };
                let bytes = serde_json::to_vec(&memo).map_err(|_| storage())?;
                memos.put(&key, &bytes).map_err(|_| storage())
            },
        )?;
        let bytes = self
            .memos
            .get(&key)
            .map_err(|_| storage())?
            .ok_or_else(storage)?;
        let memo: Memo = serde_json::from_slice(&bytes).map_err(|_| storage())?;
        self.finish(service, &alias, &memo.candidate)?;
        Ok(public)
    }
    fn finish(
        &self,
        service: &mut Service,
        alias: &str,
        candidate: &CredentialCandidate,
    ) -> Result<(), Fault> {
        if !matches!(candidate, CredentialCandidate::Ai { .. })
            || !service.validate_credential_candidate(candidate)?.session
        {
            return Err(authentication());
        }
        let bytes = serde_json::to_vec(candidate).map_err(|_| storage())?;
        self.clients.put(alias, &bytes).map_err(|_| storage())
    }
}
