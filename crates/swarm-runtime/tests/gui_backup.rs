#![cfg(windows)]
use serde_json::{Value, json};
use swarm_core::{Service, protocol::Request};
use swarm_runtime::{gui::GuiAccess, session::SessionBridge};

fn request(operation: &str, args: Value, cap: &str) -> Request {
    Request {
        operation: operation.into(),
        auth: Some(cap.into()),
        args,
        context: json!({}),
        request_id: None,
    }
}
fn call(
    access: &GuiAccess,
    service: &mut Service,
    sessions: &mut SessionBridge,
    cap: &str,
    identity: (&str, &str),
    operation: &str,
    args: Value,
) -> swarm_ipc::RpcResponse {
    let (alias, id) = identity;
    access.dispatch(
        service,
        sessions,
        request(
            "gui.backup",
            json!({"alias":alias,"account_id":id,"operation":operation,"args":args}),
            cap,
        ),
    )
}
#[test]
fn backup_controls_recheck_admin_identity_and_never_enable_public_management() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let access = GuiAccess::open(root.path()).unwrap();
    let mut sessions = SessionBridge::open(root.path(), &mut service).unwrap();
    let store = swarm_platform::CredentialStore::open(&root.path().join("gui-host")).unwrap();
    let cap = String::from_utf8(store.get("transport-capability").unwrap().unwrap()).unwrap();
    let setup = access.dispatch(&mut service, &mut sessions, request("gui.issue", json!({"alias":"admin","intent":"a".repeat(64),"request":{"operation":"account.setup","args":{"name":"Admin","password":"fixture-password"},"context":{}}}), &cap)).data.unwrap();
    let id = setup["id"].as_str().unwrap();
    let defaults = call(
        &access,
        &mut service,
        &mut sessions,
        &cap,
        ("admin", id),
        "settings.get",
        json!({}),
    );
    assert!(defaults.ok, "{:?}", defaults.error);
    assert_eq!(defaults.data.unwrap()["retention"], 7);
    assert_eq!(
        call(
            &access,
            &mut service,
            &mut sessions,
            &cap,
            ("admin", "stale-account"),
            "settings.get",
            json!({})
        )
        .error
        .unwrap()
        .code,
        "ACCOUNT_CHANGED"
    );
    let public = service.request(Request {
        operation: "backup.settings.get".into(),
        auth: None,
        args: json!({}),
        context: json!({}),
        request_id: None,
    });
    assert!(public.is_err());
    let member = access.dispatch(&mut service, &mut sessions, request("gui.request", json!({"alias":"admin","request":{"operation":"account.create","args":{"name":"Member","password":"member-password"},"context":{}}}), &cap)).data.unwrap();
    access.dispatch(&mut service, &mut sessions, request("gui.issue", json!({"alias":"member","intent":"b".repeat(64),"request":{"operation":"account.login","args":{"name":"Member","password":"member-password"},"context":{}}}), &cap)).data.unwrap();
    assert_eq!(
        call(
            &access,
            &mut service,
            &mut sessions,
            &cap,
            ("member", member["id"].as_str().unwrap()),
            "settings.get",
            json!({})
        )
        .error
        .unwrap()
        .code,
        "PERMISSION_DENIED"
    );
    access.dispatch(&mut service, &mut sessions, request("gui.request", json!({"alias":"admin","request":{"operation":"account.logout","args":{},"context":{}}}), &cap));
    assert_eq!(
        call(
            &access,
            &mut service,
            &mut sessions,
            &cap,
            ("admin", id),
            "status",
            json!({})
        )
        .error
        .unwrap()
        .exit_code,
        3
    );
}

#[test]
fn settings_validate_before_storage_and_disable_without_touching_unsafe_destination() {
    let root = tempfile::tempdir().unwrap();
    let unsafe_root = tempfile::tempdir().unwrap();
    std::fs::write(unsafe_root.path().join("unrelated.txt"), "keep").unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let access = GuiAccess::open(root.path()).unwrap();
    let mut sessions = SessionBridge::open(root.path(), &mut service).unwrap();
    let store = swarm_platform::CredentialStore::open(&root.path().join("gui-host")).unwrap();
    let cap = String::from_utf8(store.get("transport-capability").unwrap().unwrap()).unwrap();
    let setup = access.dispatch(&mut service, &mut sessions, request("gui.issue", json!({"alias":"admin","intent":"a".repeat(64),"request":{"operation":"account.setup","args":{"name":"Admin","password":"fixture-password"},"context":{}}}), &cap)).data.unwrap();
    let id = setup["id"].as_str().unwrap();
    let settings = json!({"enabled":true,"interval_seconds":60,"retention":3,"destination":unsafe_root.path()});
    assert_eq!(
        call(
            &access,
            &mut service,
            &mut sessions,
            &cap,
            ("admin", id),
            "settings.set",
            settings.clone()
        )
        .error
        .unwrap()
        .code,
        "BACKUP_STORAGE"
    );
    assert_eq!(service.backup_settings().unwrap().retention, 7);
    let mut disabled = settings.clone();
    disabled["enabled"] = json!(false);
    assert!(
        call(
            &access,
            &mut service,
            &mut sessions,
            &cap,
            ("admin", id),
            "settings.set",
            disabled
        )
        .ok
    );
    assert_eq!(service.backup_settings().unwrap().retention, 3);
    let mut invalid = settings;
    invalid["destination"] = json!(root.path().join("never-create"));
    invalid["retention"] = json!(101);
    assert_eq!(
        call(
            &access,
            &mut service,
            &mut sessions,
            &cap,
            ("admin", id),
            "settings.set",
            invalid
        )
        .error
        .unwrap()
        .code,
        "INVALID_INPUT"
    );
    assert!(!root.path().join("never-create").exists());
    assert_eq!(
        std::fs::read_to_string(unsafe_root.path().join("unrelated.txt")).unwrap(),
        "keep"
    );
    let status = call(
        &access,
        &mut service,
        &mut sessions,
        &cap,
        ("admin", id),
        "status",
        json!({}),
    );
    assert!(status.ok);
    assert_eq!(status.data.unwrap()["settings"]["enabled"], false);
    let destination = root.path().join("published");
    let generation = swarm_core::BackupGeneration {
        key: format!("scheduled-{}", uuid::Uuid::new_v4()),
        destination: destination.clone(),
    };
    let mut state = service.backup_schedule().unwrap();
    state.pending = Some(swarm_core::BackupIntent {
        generation: generation.clone(),
        due_at: 100,
    });
    service.set_backup_schedule(&state).unwrap();
    let waiting = call(
        &access,
        &mut service,
        &mut sessions,
        &cap,
        ("admin", id),
        "status",
        json!({}),
    )
    .data
    .unwrap();
    assert_eq!(waiting["pending_stage"], "publication_pending");
    assert!(
        !destination.exists(),
        "status must not create a destination"
    );
    let store = swarm_platform::BackupStore::open(&destination).unwrap();
    let prepared = service.prepare_backup().unwrap();
    store
        .write_new(&generation.key, |file| {
            prepared
                .write_archive(file, swarm_core::BackupKind::Scheduled)
                .map_err(|_| swarm_platform::PlatformError::Io)
        })
        .unwrap();
    let published = call(
        &access,
        &mut service,
        &mut sessions,
        &cap,
        ("admin", id),
        "status",
        json!({}),
    )
    .data
    .unwrap();
    assert_eq!(published["pending_stage"], "receipt_pending");
    assert!(
        service.backup_schedule().unwrap().pending.is_some(),
        "inspection must not acknowledge publication"
    );
    let db = rusqlite::Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    db.execute(
        "UPDATE app_metadata SET value='broken' WHERE key='backup-schedule-v1'",
        [],
    )
    .unwrap();
    let corrupt = call(
        &access,
        &mut service,
        &mut sessions,
        &cap,
        ("admin", id),
        "status",
        json!({}),
    )
    .data
    .unwrap();
    assert!(corrupt["schedule"].is_null());
    assert_eq!(corrupt["journal_error"], "BACKUP_STATE");
    assert!(call(&access, &mut service, &mut sessions, &cap, ("admin", id), "settings.set", json!({"enabled":false,"interval_seconds":60,"retention":3,"destination":unsafe_root.path()})).ok);
    assert_eq!(
        db.query_row(
            "SELECT value FROM app_metadata WHERE key='backup-schedule-v1'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "broken"
    );
}
