use serde_json::{Value, json};
use swarm_core::{Service, protocol::Request};
fn call(
    service: &mut Service,
    operation: &str,
    auth: Option<&str>,
    args: Value,
) -> Result<Value, swarm_core::protocol::Fault> {
    service.request(Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    })
}
#[test]
fn repeated_database_restore_expires_all_sessions_resume_secrets_and_cursor_epochs() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let initial = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"fixture-password"}),
    )
    .unwrap();
    let token = initial["token"].as_str().unwrap();
    let board = call(
        &mut service,
        "board.create",
        Some(token),
        json!({"name":"Retained"}),
    )
    .unwrap();
    let cursor =
        call(&mut service, "changes.get", Some(token), json!({})).unwrap()["change_cursor"]
            .as_str()
            .unwrap()
            .to_owned();
    let db = rusqlite::Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    db.execute_batch("INSERT INTO projects VALUES('project','Project','C:/isolated/project','now'); INSERT INTO actors VALUES('ai','ai'); INSERT INTO display_ids VALUES('display','ai'); INSERT INTO ai_identities VALUES('ai','project','model','declared','display','2099-01-01','OLD_RESUME_HASH','now'); INSERT INTO ai_sessions VALUES('OLD_AI_TOKEN_HASH','ai','session');").unwrap();
    // Use the SQLite primitive so the project fixture needs no real directory.
    let snapshot = service.credential_free_snapshot().unwrap();
    let snapshot_root = tempfile::tempdir().unwrap();
    let mut snapshot_db =
        rusqlite::Connection::open(snapshot_root.path().join("swarm.sqlite3")).unwrap();
    rusqlite::backup::Backup::new(&snapshot, &mut snapshot_db)
        .unwrap()
        .run_to_completion(100, std::time::Duration::from_millis(1), None)
        .unwrap();
    // Register the actual isolated project root for manifest validation.
    let project = tempfile::tempdir().unwrap();
    snapshot_db
        .execute(
            "UPDATE projects SET canonical_root=?1",
            [project.path().canonicalize().unwrap().to_str().unwrap()],
        )
        .unwrap();
    drop(snapshot_db);
    std::fs::copy(
        root.path().join("RULES.md"),
        snapshot_root.path().join("RULES.md"),
    )
    .unwrap();
    let source = Service::open(snapshot_root.path()).unwrap();
    let mut archive = Vec::new();
    source
        .prepare_backup()
        .unwrap()
        .write_archive(&mut archive, swarm_core::BackupKind::Manual)
        .unwrap();
    let input = swarm_core::inspect_restore_archive(&mut std::io::Cursor::new(&archive)).unwrap();
    let epoch = || {
        db.query_row(
            "SELECT value FROM app_metadata WHERE key='changes.cursor_epoch.v1'",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap()
    };
    let old_epoch = epoch();
    service
        .install_restore(&input, &Default::default())
        .unwrap();
    assert_ne!(epoch(), old_epoch);
    assert!(call(&mut service, "board.list", Some(token), json!({})).is_err());
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM human_sessions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM ai_sessions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT resume_hash FROM ai_identities", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        ""
    );
    let fresh = call(
        &mut service,
        "account.login",
        None,
        json!({"name":"Admin","password":"fixture-password"}),
    )
    .unwrap();
    let fresh_token = fresh["token"].as_str().unwrap();
    assert_eq!(
        call(
            &mut service,
            "changes.get",
            Some(fresh_token),
            json!({"after":cursor})
        )
        .unwrap_err()
        .code,
        "RESYNC_REQUIRED"
    );
    assert_eq!(
        call(&mut service, "board.list", Some(fresh_token), json!({})).unwrap()["items"][0]["id"],
        board["id"]
    );
    let previous_epoch = epoch();
    service
        .install_restore(&input, &Default::default())
        .unwrap();
    assert_ne!(epoch(), previous_epoch);
    assert!(call(&mut service, "board.list", Some(fresh_token), json!({})).is_err());
}
