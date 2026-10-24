#![cfg(windows)]
use serde_json::{Value, json};
use swarm_core::{Service, protocol::Request};
use swarm_runtime::{gui::GuiAccess, session::SessionBridge};

#[test]
fn manual_publication_recovers_same_intent_after_receipt_failure_and_reopen() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut sessions = SessionBridge::open(root.path(), &mut service).unwrap();
    let access = GuiAccess::open(root.path()).unwrap();
    let cap = String::from_utf8(
        swarm_platform::CredentialStore::open(&root.path().join("gui-host"))
            .unwrap()
            .get("transport-capability")
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let req = |op: &str, args: Value| Request {
        operation: op.into(),
        auth: Some(cap.clone()),
        args,
        context: json!({}),
        request_id: None,
    };
    let admin=access.dispatch(&mut service,&mut sessions,req("gui.issue",json!({"alias":"admin","intent":"a".repeat(64),"request":{"operation":"account.setup","args":{"name":"Admin","password":"fixture-password"},"context":{}}}))).data.unwrap();
    let intent = uuid::Uuid::new_v4().to_string();
    let manual = |op: &str| {
        req(
            "gui.backup",
            json!({"alias":"admin","account_id":admin["id"],"operation":op,"args":{"intent":intent}}),
        )
    };
    let db = rusqlite::Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_manual_receipt BEFORE UPDATE ON app_metadata WHEN NEW.key LIKE 'backup-manual-v1:%' AND json_extract(NEW.value,'$.completed')=1 BEGIN SELECT RAISE(ABORT,'fixture receipt'); END;").unwrap();
    let failed = access.dispatch(&mut service, &mut sessions, manual("manual.create"));
    assert!(!failed.ok);
    let key = format!("manual-{intent}");
    assert!(
        root.path()
            .join("backups")
            .join(format!("{key}.backup"))
            .exists()
    );
    db.execute_batch("DROP TRIGGER fail_manual_receipt;")
        .unwrap();
    drop(sessions);
    drop(service);
    let mut service = Service::open(root.path()).unwrap();
    let mut sessions = SessionBridge::open(root.path(), &mut service).unwrap();
    let recovered = access
        .dispatch(&mut service, &mut sessions, manual("manual.create"))
        .data
        .unwrap();
    assert_eq!(recovered["key"], key);
    assert_eq!(recovered["state"], "completed");
    assert_eq!(
        access
            .dispatch(&mut service, &mut sessions, manual("manual.create"))
            .data
            .unwrap(),
        recovered
    );
    assert_eq!(
        std::fs::read_dir(root.path().join("backups"))
            .unwrap()
            .count(),
        1
    );
    assert!(service.backup_schedule().unwrap().generations.is_empty());
    let snapshot = service.credential_free_snapshot().unwrap();
    assert!(
        !snapshot
            .prepare("SELECT 1 FROM app_metadata WHERE key LIKE 'backup-manual-v1:%'")
            .unwrap()
            .exists([])
            .unwrap()
    );
    let mut settings = service.backup_settings().unwrap();
    settings.destination = Some(root.path().join("changed-destination"));
    service.set_backup_settings(&settings).unwrap();
    assert_eq!(
        access
            .dispatch(&mut service, &mut sessions, manual("manual.create"))
            .data
            .unwrap(),
        recovered
    );
    assert!(!root.path().join("changed-destination").exists());
    let archive = root.path().join("backups").join(format!("{key}.backup"));
    std::fs::write(&archive, b"corrupt fixture").unwrap();
    assert!(
        !access
            .dispatch(&mut service, &mut sessions, manual("manual.create"))
            .ok
    );
    assert_eq!(std::fs::read(&archive).unwrap(), b"corrupt fixture");
    let protected = swarm_platform::CredentialStore::open(&root.path().join("lifecycle")).unwrap();
    protected
        .put(
            "full-stop",
            format!("1:{}", swarm_platform::lifecycle::boot_id().unwrap()).as_bytes(),
        )
        .unwrap();
    assert_eq!(
        access
            .dispatch(&mut service, &mut sessions, manual("manual.create"))
            .error
            .unwrap()
            .code,
        "RUNTIME_STOPPED"
    );
}
