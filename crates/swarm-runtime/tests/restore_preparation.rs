#![cfg(windows)]
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use swarm_core::{BackupKind, Service, protocol::Request};
use swarm_runtime::{gui::GuiAccess, session::SessionBridge};

struct Fixture {
    root: tempfile::TempDir,
    source: tempfile::TempDir,
    service: Service,
    sessions: SessionBridge,
    access: GuiAccess,
    capability: String,
    admin: String,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        let original = tempfile::tempdir().unwrap();
        let original_service = Service::open(original.path()).unwrap();
        let snapshot = original_service.prepare_backup().unwrap();
        swarm_platform::BackupStore::open(&source.path().join("archives"))
            .unwrap()
            .write_new("selected", |f| {
                snapshot
                    .write_archive(f, BackupKind::Manual)
                    .map_err(|_| swarm_platform::PlatformError::Io)
            })
            .unwrap();
        let mut service = Service::open(root.path()).unwrap();
        let mut sessions = SessionBridge::open(root.path(), &mut service).unwrap();
        let access = GuiAccess::open(root.path()).unwrap();
        let capability = String::from_utf8(
            swarm_platform::CredentialStore::open(&root.path().join("gui-host"))
                .unwrap()
                .get("transport-capability")
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        let setup = access.dispatch(&mut service, &mut sessions, Request { operation:"gui.issue".into(),auth:Some(capability.clone()),args:json!({"alias":"admin","intent":"a".repeat(64),"request":{"operation":"account.setup","args":{"name":"Admin","password":"fixture-password"},"context":{}}}),context:json!({}),request_id:None }).data.unwrap();
        Self {
            root,
            source,
            service,
            sessions,
            access,
            capability,
            admin: setup["id"].as_str().unwrap().into(),
        }
    }
    fn call(&mut self, operation: &str, args: Value) -> swarm_ipc::RpcResponse {
        self.access.dispatch(&mut self.service,&mut self.sessions,Request {operation:"gui.backup".into(),auth:Some(self.capability.clone()),args:json!({"alias":"admin","account_id":self.admin,"operation":operation,"args":args}),context:json!({}),request_id:None})
    }
    fn preview(&mut self) -> Value {
        let reply = self.call(
            "restore.preview",
            json!({"directory":self.source.path().join("archives"),"key":"selected"}),
        );
        assert!(reply.ok, "{:?}", reply.error);
        reply.data.unwrap()
    }
}

#[test]
fn applying_requires_final_confirmation_and_invalidates_current_human_credentials() {
    let mut f = Fixture::new();
    let preview = f.preview();
    let intent = uuid::Uuid::new_v4().to_string();
    let args = json!({"directory":f.source.path().join("archives"),"key":"selected","input_sha256":preview["input_sha256"],"intent":intent,"targets":preview["targets"]});
    assert!(f.call("restore.prepare", args).ok);
    std::fs::write(f.root.path().join("RULES.md"), "# latest live state\n").unwrap();
    let denied = f.call(
        "restore.apply",
        json!({"intent":intent,"input_sha256":preview["input_sha256"],"confirmed":false}),
    );
    assert!(!denied.ok);
    assert!(!f.service.setup_required().unwrap());
    let applied = f.call(
        "restore.apply",
        json!({"intent":intent,"input_sha256":preview["input_sha256"],"confirmed":true}),
    );
    assert!(applied.ok, "{:?}", applied.error);
    assert_eq!(applied.data.unwrap()["state"], "applied");
    assert!(f.service.setup_required().unwrap());
    assert!(!f.call("settings.get", json!({})).ok);
    let store = swarm_platform::BackupStore::open(&f.root.path().join("restore-safety")).unwrap();
    let latest = swarm_core::inspect_restore_archive(
        &mut store.read(&format!("apply-safety-{intent}")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        latest.manifest().rules[0].content.as_deref(),
        Some("# latest live state\n")
    );
    assert!(
        swarm_platform::CredentialStore::open(&f.root.path().join("credentials"))
            .unwrap()
            .get("candidate-journal")
            .unwrap()
            .is_none()
    );
}

#[test]
fn restore_preparation_requires_exact_confirmation_saves_safety_and_never_applies_input() {
    let mut f = Fixture::new();
    let preview = f.preview();
    assert_eq!(
        preview["targets"][0]["target_path"],
        json!(f.root.path().canonicalize().unwrap().join("RULES.md"))
    );
    assert!(!f.root.path().join("restore-safety").exists());
    let intent = uuid::Uuid::new_v4().to_string();
    let args = json!({"directory":f.source.path().join("archives"),"key":"selected","input_sha256":preview["input_sha256"],"intent":intent,"targets":preview["targets"]});
    let mut bad = args.clone();
    bad["targets"][0]["target_path"] = json!(f.source.path().join("RULES.md"));
    assert_eq!(
        f.call("restore.prepare", bad).error.unwrap().code,
        "RESTORE_CONFIRMATION"
    );
    assert!(!f.root.path().join("restore-safety").exists());
    let before = std::fs::read(f.root.path().join("RULES.md")).unwrap();
    let prepared = f.call("restore.prepare", args.clone());
    assert!(prepared.ok, "{:?}", prepared.error);
    let value = prepared.data.unwrap();
    assert_eq!(value["state"], "prepared");
    assert!(value.get("live_ledger").is_none());
    let safety = swarm_platform::BackupStore::open(&f.root.path().join("restore-safety")).unwrap();
    let key = format!("safety-{intent}");
    let manifest = swarm_core::inspect_restore_archive(&mut safety.read(&key).unwrap()).unwrap();
    assert!(matches!(manifest.manifest().kind, BackupKind::Safety));
    assert_eq!(
        std::fs::read(f.root.path().join("RULES.md")).unwrap(),
        before
    );
    let status = f
        .call("restore.status", json!({"intent":intent}))
        .data
        .unwrap();
    assert_eq!(status["safety_sha256"], value["safety_sha256"]);
    assert!(f.call("restore.prepare", args).ok);
    assert_eq!(
        std::fs::read_dir(f.root.path().join("restore-safety"))
            .unwrap()
            .count(),
        1
    );
    assert!(!f.service.setup_required().unwrap()); // selected input had no account; never applied
    safety.remove(&key).unwrap();
    assert!(!f.call("restore.status", json!({"intent":intent})).ok);
}

#[test]
fn publication_receipt_recovery_retains_original_safety_and_live_ledger() {
    let mut f = Fixture::new();
    let manual_intent = uuid::Uuid::new_v4().to_string();
    f.service
        .set_manual_backup(&swarm_core::ManualBackup {
            version: 1,
            account_id: f.admin.clone(),
            intent: manual_intent.clone(),
            destination: f.root.path().canonicalize().unwrap().join("backups"),
            completed: false,
        })
        .unwrap();
    f.service
        .set_backup_schedule(&swarm_core::BackupScheduleState::default())
        .unwrap();
    let expected = f.service.backup_live_ledger().unwrap();
    let preview = f.preview();
    let intent = uuid::Uuid::new_v4().to_string();
    let args = json!({"directory":f.source.path().join("archives"),"key":"selected","input_sha256":preview["input_sha256"],"intent":intent,"targets":preview["targets"]});
    assert!(f.call("restore.prepare", args.clone()).ok);
    let safety_path = f
        .root
        .path()
        .join("restore-safety")
        .join(format!("safety-{intent}.backup"));
    let before = std::fs::read(&safety_path).unwrap();
    let journal =
        swarm_platform::CredentialStore::open(&f.root.path().join("restore-preparation")).unwrap();
    let key = format!("restore-{:x}-{intent}", Sha256::digest(f.admin.as_bytes()));
    let mut record: Value = serde_json::from_slice(&journal.get(&key).unwrap().unwrap()).unwrap();
    assert_eq!(record["live_ledger"], json!(expected));
    record["safety_sha256"] = Value::Null; // file published, receipt not persisted
    journal
        .put(&key, &serde_json::to_vec(&record).unwrap())
        .unwrap();
    // New live state must never silently replace the original safety snapshot.
    std::fs::write(f.root.path().join("RULES.md"), "# newer live state\n").unwrap();
    f.service
        .set_manual_backup(&swarm_core::ManualBackup {
            version: 1,
            account_id: f.admin.clone(),
            intent: manual_intent,
            destination: f.root.path().canonicalize().unwrap().join("backups"),
            completed: true,
        })
        .unwrap();
    assert!(f.call("restore.prepare", args.clone()).ok);
    assert_eq!(std::fs::read(&safety_path).unwrap(), before);
    let recovered: Value = serde_json::from_slice(&journal.get(&key).unwrap().unwrap()).unwrap();
    assert_eq!(recovered["live_ledger"], json!(expected));
    assert!(recovered["safety_sha256"].is_string());
    let mut changed = args.clone();
    changed["input_sha256"] = json!("0".repeat(64));
    assert_eq!(
        f.call("restore.prepare", changed).error.unwrap().code,
        "RESTORE_CHANGED"
    );
    // Corrupt safety is rejected and never overwritten, even on explicit retry.
    let mut corrupt = before;
    corrupt[0] ^= 1;
    std::fs::write(&safety_path, &corrupt).unwrap();
    assert!(!f.call("restore.prepare", args).ok);
    assert_eq!(std::fs::read(safety_path).unwrap(), corrupt);
}

#[test]
fn stale_identity_non_admin_stopped_and_corrupt_history_never_prepare_restore() {
    let mut f = Fixture::new();
    let args = json!({"directory":f.source.path().join("archives"),"key":"selected"});
    let original = f.admin.clone();
    f.admin = "stale".into();
    assert_eq!(
        f.call("restore.preview", args.clone()).error.unwrap().code,
        "ACCOUNT_CHANGED"
    );
    f.admin = original;
    assert!(
        f.service
            .request(Request {
                operation: "restore.prepare".into(),
                auth: None,
                args: json!({}),
                context: json!({}),
                request_id: None
            })
            .is_err()
    );
    assert!(!f.root.path().join("restore-preparation").exists());
    let member = f.access.dispatch(&mut f.service,&mut f.sessions,Request {operation:"gui.request".into(),auth:Some(f.capability.clone()),args:json!({"alias":"admin","request":{"operation":"account.create","args":{"name":"Member","password":""},"context":{}}}),context:json!({}),request_id:None}).data.unwrap();
    assert!(f.access.dispatch(&mut f.service,&mut f.sessions,Request {operation:"gui.issue".into(),auth:Some(f.capability.clone()),args:json!({"alias":"member","intent":"b".repeat(64),"request":{"operation":"account.login","args":{"name":"Member","password":""},"context":{}}}),context:json!({}),request_id:None}).ok);
    let denied = f.access.dispatch(&mut f.service,&mut f.sessions,Request {operation:"gui.backup".into(),auth:Some(f.capability.clone()),args:json!({"alias":"member","account_id":member["id"],"operation":"restore.preview","args":args}),context:json!({}),request_id:None});
    assert_eq!(denied.error.unwrap().code, "PERMISSION_DENIED");
    assert!(!f.root.path().join("restore-preparation").exists());
    let preview = f.preview();
    let prepare = json!({"directory":f.source.path().join("archives"),"key":"selected","input_sha256":preview["input_sha256"],"intent":uuid::Uuid::new_v4().to_string(),"targets":preview["targets"]});
    let db = rusqlite::Connection::open(f.root.path().join("swarm.sqlite3")).unwrap();
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES('backup-manual-v1:invalid','{}')",
        [],
    )
    .unwrap();
    assert_eq!(
        f.call("restore.prepare", prepare).error.unwrap().code,
        "BACKUP_STATE"
    );
    assert!(!f.root.path().join("restore-safety").exists());
    db.execute(
        "DELETE FROM app_metadata WHERE key='backup-manual-v1:invalid'",
        [],
    )
    .unwrap();
    let lifecycle =
        swarm_platform::CredentialStore::open(&f.root.path().join("lifecycle")).unwrap();
    lifecycle
        .put(
            "full-stop",
            format!("1:{}", uuid::Uuid::new_v4()).as_bytes(),
        )
        .unwrap();
    assert_eq!(
        f.call("restore.preview", args).error.unwrap().code,
        "RUNTIME_STOPPED"
    );
}

#[test]
fn interrupted_unpublished_safety_requires_new_intent_instead_of_capturing_new_state() {
    let mut f = Fixture::new();
    let preview = f.preview();
    let intent = uuid::Uuid::new_v4().to_string();
    let args = json!({"directory":f.source.path().join("archives"),"key":"selected","input_sha256":preview["input_sha256"],"intent":intent,"targets":preview["targets"]});
    assert!(f.call("restore.prepare", args.clone()).ok);
    let journal =
        swarm_platform::CredentialStore::open(&f.root.path().join("restore-preparation")).unwrap();
    let key = format!("restore-{:x}-{intent}", Sha256::digest(f.admin.as_bytes()));
    let mut record: Value = serde_json::from_slice(&journal.get(&key).unwrap().unwrap()).unwrap();
    record["safety_sha256"] = Value::Null;
    journal
        .put(&key, &serde_json::to_vec(&record).unwrap())
        .unwrap();
    let safety = swarm_platform::BackupStore::open(&f.root.path().join("restore-safety")).unwrap();
    safety.remove(&format!("safety-{intent}")).unwrap();
    std::fs::write(
        f.root.path().join("RULES.md"),
        "# changed after interruption\n",
    )
    .unwrap();
    assert_eq!(
        f.call("restore.prepare", args.clone()).error.unwrap().code,
        "RESTORE_RESTART_REQUIRED"
    );
    assert!(!safety.contains(&format!("safety-{intent}")).unwrap());
    journal.put(&key, b"not-json").unwrap();
    assert_eq!(
        f.call("restore.prepare", args).error.unwrap().code,
        "RESTORE_INVALID"
    );
    assert_eq!(journal.get(&key).unwrap().unwrap(), b"not-json");
}
