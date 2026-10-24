#![cfg(windows)]

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use swarm_core::{CredentialCandidate, Service, protocol::Request};
use swarm_platform::CredentialStore;
use swarm_runtime::{gui::GuiAccess, session::SessionBridge};

struct RuntimeFixture {
    root: tempfile::TempDir,
    service: Service,
    sessions: SessionBridge,
    access: GuiAccess,
    capability: String,
    thread: String,
}

impl RuntimeFixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut service = Service::open(root.path()).unwrap();
        let mut sessions = SessionBridge::open(root.path(), &mut service).unwrap();
        sessions
            .gui_issue(
                &mut service,
                "admin",
                &"a".repeat(64),
                request(
                    "account.setup",
                    json!({"name":"Admin","password":"password"}),
                ),
            )
            .unwrap();
        let access = GuiAccess::open(root.path()).unwrap();
        let capability = String::from_utf8(
            CredentialStore::open(&root.path().join("gui-host"))
                .unwrap()
                .get("transport-capability")
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let mut fixture = Self {
            root,
            service,
            sessions,
            access,
            capability,
            thread: String::new(),
        };
        fixture.select("admin");
        let board = fixture.gui("admin", "board.create", json!({"name":"Board"}));
        fixture.thread = fixture.gui(
            "admin",
            "thread.create",
            json!({"board_id":board["id"],"name":"Thread"}),
        )["id"]
            .as_str()
            .unwrap()
            .to_owned();
        fixture.gui(
            "admin",
            "thread.subscribe",
            json!({"id":fixture.thread,"subscribed":true}),
        );
        fixture
    }
    fn store(&self) -> CredentialStore {
        CredentialStore::open(&self.root.path().join("gui-state")).unwrap()
    }
    fn select(&mut self, alias: &str) {
        let store = self.store();
        let _gate = store.lock("notification-selection-gate").unwrap();
        store
            .put(
                "selection",
                &serde_json::to_vec(&json!({"alias":alias})).unwrap(),
            )
            .unwrap();
    }
    fn gui(&mut self, alias: &str, operation: &str, args: Value) -> Value {
        let mut outer = request(
            "gui.request",
            json!({"alias":alias,"request":request(operation,args)}),
        );
        outer.auth = Some(self.capability.clone());
        let response = self
            .access
            .dispatch(&mut self.service, &mut self.sessions, outer);
        assert!(response.ok, "{:?}", response.error);
        response.data.unwrap()
    }
    fn post(&mut self, body: &str) {
        self.gui(
            "admin",
            "post.create",
            json!({"thread_id":self.thread,"body":body}),
        );
    }
    fn pending(&mut self) -> Vec<swarm_core::NotificationDelivery> {
        self.sessions
            .prepare_notifications(&mut self.service, 20)
            .unwrap()
    }
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

#[test]
fn submission_holds_selection_gate_and_retries_same_batch_without_advancing_unread() {
    let mut f = RuntimeFixture::new();
    f.post("First");
    f.post("Second");
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
        + 3000;
    let state_root = f.root.path().join("gui-state");
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let mut worker = None;
    let mut batch_id = String::new();
    let error = f
        .sessions
        .deliver_notification_batch(&mut f.service, now, 2000, &mut |batch| {
            batch_id = batch.id.clone();
            assert_eq!(batch.deliveries.len(), 2);
            let state_root = state_root.clone();
            let entered_tx = entered_tx.clone();
            worker = Some(std::thread::spawn(move || {
                let store = CredentialStore::open(&state_root).unwrap();
                let _gate = store.lock("notification-selection-gate").unwrap();
                entered_tx.send(()).unwrap();
            }));
            assert!(
                entered_rx
                    .recv_timeout(std::time::Duration::from_millis(50))
                    .is_err()
            );
            Err(swarm_core::protocol::Fault::new(
                "NOTIFICATION_OS",
                8,
                "Controlled submission failure",
            ))
        })
        .unwrap_err();
    assert_eq!(error.code, "NOTIFICATION_OS");
    entered_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    worker.unwrap().join().unwrap();
    let mut calls = 0;
    assert!(
        !f.sessions
            .deliver_notification_batch(&mut f.service, now + 4999, 2000, &mut |_| {
                calls += 1;
                Ok(())
            })
            .unwrap()
    );
    assert_eq!(calls, 0);
    assert!(
        f.sessions
            .deliver_notification_batch(&mut f.service, now + 5000, 2000, &mut |batch| {
                assert_eq!(batch.id, batch_id);
                Ok(())
            })
            .unwrap()
    );
    assert!(f.pending().is_empty());
    assert_eq!(f.gui("admin", "conversation.unread", json!({}))["total"], 2);
}

#[test]
fn full_stop_and_pending_auth_never_call_submission_sink() {
    let mut f = RuntimeFixture::new();
    f.post("Pending");
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
        + 3000;
    f.store().put("pending-auth", b"pending").unwrap();
    assert!(
        !f.sessions
            .deliver_notification_batch(&mut f.service, now, 2000, &mut |_| panic!(
                "pending authentication must suppress sink"
            ))
            .unwrap()
    );
    f.store().remove("pending-auth").unwrap();
    f.post("New after selection recovery");
    CredentialStore::open(&f.root.path().join("lifecycle"))
        .unwrap()
        .put(
            "full-stop",
            format!("1:{}", uuid::Uuid::new_v4()).as_bytes(),
        )
        .unwrap();
    assert_eq!(
        f.sessions
            .deliver_notification_batch(&mut f.service, now, 2000, &mut |_| panic!(
                "full stop must suppress sink"
            ))
            .unwrap_err()
            .exit_code,
        7
    );
}

fn core_call(service: &mut Service, token: &str, operation: &str, args: Value) -> Value {
    let mut req = request(operation, args);
    req.auth = Some(token.to_owned());
    service.request(req).unwrap()
}

#[test]
fn unauthenticated_gui_request_cannot_revoke_durable_notification_work() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let grant = service
        .request(request(
            "account.setup",
            json!({"name":"Admin","password":"password"}),
        ))
        .unwrap();
    let token = grant["token"].as_str().unwrap();
    let candidate = CredentialCandidate::from_grant("account.setup", &grant).unwrap();
    let mut sessions = SessionBridge::open(root.path(), &mut service).unwrap();
    let access = GuiAccess::open(root.path()).unwrap();
    let board = core_call(&mut service, token, "board.create", json!({"name":"Board"}));
    let thread = core_call(
        &mut service,
        token,
        "thread.create",
        json!({"board_id":board["id"],"name":"Thread"}),
    );
    core_call(
        &mut service,
        token,
        "thread.subscribe",
        json!({"id":thread["id"],"subscribed":true}),
    );
    service.select_notification_recipient(&candidate).unwrap();
    core_call(
        &mut service,
        token,
        "post.create",
        json!({"thread_id":thread["id"],"body":"Keep pending"}),
    );
    CredentialStore::open(&root.path().join("gui-state"))
        .unwrap()
        .put("pending-auth", b"pending")
        .unwrap();
    let response = access.dispatch(
        &mut service,
        &mut sessions,
        request("gui.setup_status", json!({})),
    );
    assert!(!response.ok);
    assert_eq!(
        service.prepare_notifications(&candidate, 20).unwrap().len(),
        1
    );
}

#[test]
fn runtime_reacquires_only_selected_human_and_cancels_switch_pending_and_logout() {
    let mut f = RuntimeFixture::new();
    f.post("A new");
    let first = f.pending();
    assert_eq!(first.len(), 1);
    let old_id = first[0].id.clone();
    f.gui(
        "admin",
        "account.create",
        json!({"name":"Second","password":"second-password"}),
    );
    f.sessions
        .gui_issue(
            &mut f.service,
            "second",
            &"b".repeat(64),
            request(
                "account.login",
                json!({"name":"Second","password":"second-password"}),
            ),
        )
        .unwrap();
    // Saving another credential does not select it.
    assert_eq!(f.pending()[0].id, old_id);
    f.select("second");
    assert!(f.pending().is_empty());
    f.post("While B selected");
    f.select("admin");
    assert!(f.pending().is_empty());
    f.post("A new again");
    assert_eq!(f.pending().len(), 1);
    {
        let store = f.store();
        let _gate = store.lock("notification-selection-gate").unwrap();
        store.put("pending-auth", b"unconfirmed intent").unwrap();
    }
    f.post("While authentication pending");
    assert!(f.pending().is_empty());
    f.store().remove("pending-auth").unwrap();
    assert!(f.pending().is_empty());
    f.post("After cancel");
    assert_eq!(f.pending().len(), 1);
    f.gui("admin", "account.logout", json!({}));
    assert!(f.pending().is_empty());
    assert!(
        f.sessions
            .acknowledge_notification(&mut f.service, &old_id)
            .is_err()
    );
}

#[test]
fn damaged_pending_storage_blocks_preparation_without_erasing_pending_work() {
    let mut f = RuntimeFixture::new();
    f.post("Keep after storage recovery");
    let id = f.pending()[0].id.clone();
    let store = f.store();
    store.put("pending-auth", b"intent").unwrap();
    let path = f
        .root
        .path()
        .join("gui-state")
        .join(format!("{:x}.bin", Sha256::digest(b"pending-auth")));
    std::fs::write(path, b"corrupt DPAPI ciphertext").unwrap();
    assert_eq!(
        f.sessions
            .prepare_notifications(&mut f.service, 20)
            .unwrap_err()
            .code,
        "CREDENTIAL_STORAGE"
    );
    // Removing the damaged file is explicit recovery, not an implicit logout.
    store.remove("pending-auth").unwrap();
    assert_eq!(f.pending()[0].id, id);
    store.put("selection", b"not json").unwrap();
    assert!(
        f.sessions
            .prepare_notifications(&mut f.service, 20)
            .is_err()
    );
    f.select("admin");
    assert_eq!(f.pending()[0].id, id);
}

#[test]
fn runtime_restart_keeps_delivery_id_and_full_stop_blocks_preparation_and_ack() {
    let mut f = RuntimeFixture::new();
    f.post("Persist across runtime close");
    let id = f.pending()[0].id.clone();
    let RuntimeFixture {
        root,
        service,
        sessions,
        ..
    } = f;
    drop(sessions);
    drop(service);
    let mut service = Service::open(root.path()).unwrap();
    let mut reopened = SessionBridge::open(root.path(), &mut service).unwrap();
    assert_eq!(
        reopened.prepare_notifications(&mut service, 20).unwrap()[0].id,
        id
    );
    CredentialStore::open(&root.path().join("lifecycle"))
        .unwrap()
        .put(
            "full-stop",
            format!("1:{}", uuid::Uuid::new_v4()).as_bytes(),
        )
        .unwrap();
    assert_eq!(
        reopened
            .prepare_notifications(&mut service, 20)
            .unwrap_err()
            .exit_code,
        7
    );
    assert_eq!(
        reopened
            .acknowledge_notification(&mut service, &id)
            .unwrap_err()
            .exit_code,
        7
    );
}
