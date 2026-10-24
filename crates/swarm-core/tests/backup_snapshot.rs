use rusqlite::{Connection, backup::Backup};
use serde_json::json;
use swarm_core::{Service, protocol::Request};

fn call(
    service: &mut Service,
    operation: &str,
    auth: Option<&str>,
    args: serde_json::Value,
) -> serde_json::Value {
    service
        .request(Request {
            operation: operation.into(),
            auth: auth.map(str::to_owned),
            args,
            context: json!({}),
            request_id: None,
        })
        .unwrap()
}

#[test]
fn snapshot_preserves_data_but_excludes_sessions_resume_and_private_receipts() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let setup = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"backup-test-password"}),
    );
    let token = setup["token"].as_str().unwrap();
    let board = call(
        &mut service,
        "board.create",
        Some(token),
        json!({"name":"Kept board"}),
    );
    let thread = call(
        &mut service,
        "thread.create",
        Some(token),
        json!({"board_id":board["id"],"name":"Kept thread"}),
    );
    let post = call(
        &mut service,
        "post.create",
        Some(token),
        json!({"thread_id":thread["id"],"body":"original private body"}),
    );
    call(
        &mut service,
        "post.edit",
        Some(token),
        json!({"thread_id":thread["id"],"no":post["no"],"body":"edited private body","if_revision":1}),
    );
    call(
        &mut service,
        "post.delete",
        Some(token),
        json!({"thread_id":thread["id"],"no":post["no"]}),
    );
    call(
        &mut service,
        "display.settings.set",
        Some(token),
        json!({"language":"ja","theme":"dark"}),
    );
    let source = Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    source.execute_batch("INSERT INTO projects VALUES('project','Registered','C:/isolated/project','now');
        INSERT INTO actors VALUES('ai','ai'); INSERT INTO display_ids VALUES('ai-display','ai');
        INSERT INTO ai_identities VALUES('ai','project','test','declared','ai-display','2099-01-01','RESUME_SECRET_SENTINEL','now');
        INSERT INTO ai_sessions VALUES('AI_SESSION_SENTINEL','ai','session');
        INSERT INTO app_metadata VALUES('credential-receipt:sentinel','PRIVATE_RECEIPT_SENTINEL');
        INSERT INTO app_metadata VALUES('notification-batch-v1','PRIVATE_BATCH_SENTINEL');
        INSERT INTO app_metadata VALUES('google-recovery-link-v1:sentinel','GOOGLE_LINK_SENTINEL');
        INSERT INTO app_metadata VALUES('google-recovery-challenge-v1:sentinel','GOOGLE_CHALLENGE_SENTINEL');
        INSERT INTO notification_selection SELECT 1,id,printf('%064d',1),'generation' FROM accounts;
        INSERT INTO notification_outbox SELECT 'delivery','generation',(SELECT id FROM accounts),thread_id,id,'post','now' FROM posts;").unwrap();
    let old_epoch: String = source
        .query_row(
            "SELECT value FROM app_metadata WHERE key='changes.cursor_epoch.v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let snapshot = service.credential_free_snapshot().unwrap();
    for table in [
        "human_sessions",
        "ai_sessions",
        "notification_selection",
        "notification_outbox",
    ] {
        let count: i64 = snapshot
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
    assert_eq!(
        snapshot
            .query_row("SELECT resume_hash FROM ai_identities", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        ""
    );
    assert!(!snapshot.prepare("SELECT 1 FROM app_metadata WHERE key LIKE 'google-recovery-%' OR key LIKE 'credential-receipt:%' OR key='notification-batch-v1'").unwrap().exists([]).unwrap());
    assert_eq!(
        snapshot
            .query_row(
                "SELECT body FROM posts WHERE deleted_at IS NOT NULL",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "edited private body"
    );
    assert_eq!(
        snapshot
            .query_row(
                "SELECT body FROM post_revisions WHERE revision=1",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "original private body"
    );
    assert_eq!(
        snapshot
            .query_row("SELECT count(*) FROM projects", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(
        snapshot
            .prepare("SELECT 1 FROM app_metadata WHERE key LIKE 'display-settings-v1:%'")
            .unwrap()
            .exists([])
            .unwrap()
    );
    let new_epoch: String = snapshot
        .query_row(
            "SELECT value FROM app_metadata WHERE key='changes.cursor_epoch.v1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(old_epoch, new_epoch);
    assert_eq!(
        source
            .query_row("SELECT resume_hash FROM ai_identities", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "RESUME_SECRET_SENTINEL"
    );
    assert_eq!(
        source
            .query_row(
                "SELECT value FROM app_metadata WHERE key='changes.cursor_epoch.v1'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        old_epoch
    );
    call(&mut service, "account.status", Some(token), json!({}));
    let restored_root = tempfile::tempdir().unwrap();
    let target = restored_root.path().join("swarm.sqlite3");
    let mut saved = Connection::open(&target).unwrap();
    Backup::new(&snapshot, &mut saved)
        .unwrap()
        .step(-1)
        .unwrap();
    drop(saved);
    let bytes = std::fs::read(&target).unwrap();
    for secret in [
        "RESUME_SECRET_SENTINEL",
        "AI_SESSION_SENTINEL",
        "PRIVATE_RECEIPT_SENTINEL",
        "PRIVATE_BATCH_SENTINEL",
    ] {
        assert!(
            !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
            "secret remains in database pages"
        );
    }
    let mut reopened = Service::open(restored_root.path()).unwrap();
    let rejected = reopened
        .request(Request {
            operation: "account.status".into(),
            auth: Some(token.to_owned()),
            args: json!({}),
            context: json!({}),
            request_id: None,
        })
        .unwrap_err();
    assert_eq!(rejected.exit_code, 3);
    let login = call(
        &mut reopened,
        "account.login",
        None,
        json!({"name":"Admin","password":"backup-test-password"}),
    );
    let new_token = login["token"].as_str().unwrap();
    assert_ne!(new_token, token);
    let settings = call(
        &mut reopened,
        "display.settings.get",
        Some(new_token),
        json!({}),
    );
    assert_eq!(
        settings["settings"],
        json!({"language":"ja","theme":"dark"})
    );
    assert_eq!(
        source
            .query_row("SELECT count(*) FROM notification_outbox", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn snapshot_is_not_a_request_operation() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let error = service
        .request(Request {
            operation: "backup.snapshot".into(),
            auth: None,
            args: json!({}),
            context: json!({}),
            request_id: None,
        })
        .unwrap_err();
    assert_eq!(error.code, "UNKNOWN_OPERATION");
}

#[test]
fn snapshot_rejects_newer_schema_without_mutating_source() {
    let root = tempfile::tempdir().unwrap();
    let service = Service::open(root.path()).unwrap();
    let db = Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    db.execute_batch("PRAGMA user_version=10").unwrap();
    let error = service.credential_free_snapshot().err().unwrap();
    assert_eq!(error.code, "BACKUP_STATE");
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        10
    );
}

#[test]
fn snapshot_rejects_broken_relationships_without_publishing_a_copy() {
    let root = tempfile::tempdir().unwrap();
    let service = Service::open(root.path()).unwrap();
    let db = Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    db.execute_batch(
        "PRAGMA foreign_keys=OFF; INSERT INTO human_sessions VALUES('invalid','missing-account');",
    )
    .unwrap();
    assert_eq!(
        service.credential_free_snapshot().err().unwrap().code,
        "BACKUP_STATE"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM human_sessions", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
