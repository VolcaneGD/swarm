//! Private, root-scoped click intent. The renderer never supplies a target.
use super::*;
use swarm_platform::notifications::{ActivationTarget, Identity};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Activation {
    version: u32,
    event_id: String,
    account_id: String,
    thread_id: String,
}

fn open(root: &Path) -> Result<CredentialStore, Fault> {
    CredentialStore::open(&root.join("notification-state")).map_err(|_| storage())
}
fn load(store: &CredentialStore) -> Result<Option<Activation>, Fault> {
    let Some(bytes) = store.get("activation").map_err(|_| storage())? else {
        return Ok(None);
    };
    let value: Activation = serde_json::from_slice(&bytes).map_err(|_| storage())?;
    if value.version != 1
        || !uuid::Uuid::parse_str(&value.event_id)
            .is_ok_and(|id| !id.is_nil() && id.to_string() == value.event_id)
        || ActivationTarget::new(&value.account_id, &value.thread_id).is_err()
    {
        return Err(storage());
    }
    Ok(Some(value))
}

/// Trusted native startup only. Validates the URI before creating private state.
/// A later explicit click replaces the previous unconsumed intent.
pub fn queue_notification_activation(
    root: &Path,
    executable: &Path,
    uri: &str,
) -> Result<String, Fault> {
    let identity = Identity::new(root, executable).map_err(|_| invalid())?;
    let target = identity.parse_activation(uri).map_err(|_| invalid())?;
    let event = Activation {
        version: 1,
        event_id: uuid::Uuid::new_v4().to_string(),
        account_id: target.account_id().into(),
        thread_id: target.thread_id().into(),
    };
    let store = open(root)?;
    let _gate = store.lock("activation").map_err(|_| storage())?;
    store
        .put(
            "activation",
            &serde_json::to_vec(&event).map_err(|_| storage())?,
        )
        .map_err(|_| storage())?;
    Ok(event.event_id)
}

/// Native window wake-up only; exposes no target metadata or credentials.
pub fn pending_notification_event(root: &Path) -> Result<Option<String>, Fault> {
    let store = open(root)?;
    let _gate = store.lock("activation").map_err(|_| storage())?;
    Ok(load(&store)?.map(|value| value.event_id))
}

impl GuiHost {
    fn resolve_notification_activation(&self, event: &Activation) -> Result<Value, Fault> {
        let status = |state| json!({"state":state,"event_id":event.event_id});
        if self.pending()?.is_some() {
            return Ok(status("login_required"));
        }
        let Some(selected) = self.selection()? else {
            return Ok(status("login_required"));
        };
        let client = GuiClient::open(&self.root)?;
        let account = match client.request(&selected.alias, request("account.status", json!({}))) {
            Ok(account) => account,
            Err(error) if error.exit_code == 3 => return Ok(status("login_required")),
            Err(error) => return Err(error),
        };
        if account.get("id").and_then(Value::as_str) != Some(&event.account_id) {
            return Ok(status("account_required"));
        }
        let thread = match client.request(
            &selected.alias,
            request("thread.get", json!({"id":event.thread_id})),
        ) {
            Ok(thread) => thread,
            Err(error) if error.exit_code == 3 => return Ok(status("login_required")),
            Err(error) if error.exit_code == 4 || error.code == "NOT_FOUND" => {
                return Ok(status("unavailable"));
            }
            Err(error) => return Err(error),
        };
        if thread.get("id").and_then(Value::as_str) != Some(&event.thread_id) {
            return Err(internal());
        }
        Ok(
            json!({"state":"ready","event_id":event.event_id,"account_id":event.account_id,"thread":thread}),
        )
    }

    /// Revalidates the current human session and Thread permissions on every read.
    /// Holding this mailbox lock across IPC is safe: runtime never acquires it.
    pub fn notification_activation(&self) -> Result<Value, Fault> {
        let store = open(&self.root)?;
        let _gate = store.lock("activation").map_err(|_| storage())?;
        let Some(event) = load(&store)? else {
            return Ok(json!({"state":"none"}));
        };
        self.resolve_notification_activation(&event)
    }

    /// Removes only the exact intent successfully handled by this account.
    /// A new click while navigation is in flight cannot be acknowledged by an old ID.
    pub fn acknowledge_notification_activation(&self, event_id: &str) -> Result<bool, Fault> {
        if !uuid::Uuid::parse_str(event_id)
            .is_ok_and(|id| !id.is_nil() && id.to_string() == event_id)
        {
            return Err(invalid());
        }
        let store = open(&self.root)?;
        let _gate = store.lock("activation").map_err(|_| storage())?;
        let Some(event) = load(&store)? else {
            return Ok(false);
        };
        if event.event_id != event_id {
            return Ok(false);
        }
        let resolved = self.resolve_notification_activation(&event)?;
        if resolved["state"] != "ready" && resolved["state"] != "unavailable" {
            return Err(authentication());
        }
        store.remove("activation").map_err(|_| storage())?;
        Ok(true)
    }
}
