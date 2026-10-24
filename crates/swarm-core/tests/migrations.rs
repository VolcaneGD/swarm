use rusqlite::{Connection, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use swarm_core::{Service, protocol::Request};

fn call(service: &mut Service, operation: &str, args: Value) -> Value {
    service
        .request(Request {
            operation: operation.into(),
            auth: Some("legacy-token".into()),
            args,
            context: json!({}),
            request_id: None,
        })
        .unwrap()
}
fn legacy(root: &std::path::Path) -> Connection {
    let db = Connection::open(root.join("swarm.sqlite3")).unwrap();
    db.execute_batch(include_str!("../src/schema.sql")).unwrap();
    db.execute_batch("PRAGMA user_version=1;").unwrap();
    db.execute("INSERT INTO accounts VALUES('admin','Admin','unused','unused',1,'2026-01-01T00:00:00.000Z')",[]).unwrap();
    let hash = format!("{:x}", Sha256::digest(b"legacy-token"));
    db.execute("INSERT INTO human_sessions VALUES(?1,'admin')", [hash])
        .unwrap();
    db.execute(
        "INSERT INTO human_display_ids VALUES('admin','2026-01-01','legacy-display')",
        [],
    )
    .unwrap();
    db
}
#[test]
fn v1_migration_preserves_posts_replies_history_receipts_and_login() {
    let root = tempfile::tempdir().unwrap();
    let db = legacy(root.path());
    db.execute_batch("
        INSERT INTO boards VALUES('board','Board','description','admin',1,'2026-01-01','2026-01-01',NULL);
        INSERT INTO threads VALUES('thread','board','Topic','','admin',1,3,'2026-01-01','2026-01-01',NULL);
        INSERT INTO posts VALUES('post','thread',1,'Current','admin','USER','legacy-display','2026-01-01','2026-01-02',2,NULL,'admin','legacy-display',NULL);
        INSERT INTO posts VALUES('reply','thread',2,'Reply','admin','USER','legacy-display','2026-01-01','2026-01-01',1,1,NULL,NULL,'2026-01-03');
        INSERT INTO post_revisions VALUES('post',1,'Original','2026-01-02','admin','legacy-display');
        INSERT INTO changes VALUES(42,'post.edit','post','admin','2026-01-02');
        UPDATE sqlite_sequence SET seq=100 WHERE name='changes';
        INSERT INTO create_requests VALUES('admin','receipt','fingerprint','post.create','post','2026-01-01');
    ").unwrap();
    drop(db);
    let mut service = Service::open(root.path()).unwrap();
    let fetched = call(
        &mut service,
        "post.get",
        json!({"thread_id":"thread","no":1}),
    );
    assert_eq!(fetched["body"], "Current");
    assert_eq!(fetched["revision"], 2);
    assert_eq!(fetched["author"]["display_id"], "legacy-display");
    assert_eq!(fetched["response_count"], 0);
    assert_eq!(
        call(
            &mut service,
            "post.history",
            json!({"thread_id":"thread","no":1})
        )["items"][0]["body"],
        "Original"
    );
    assert_eq!(
        call(
            &mut service,
            "post.restore",
            json!({"thread_id":"thread","no":2})
        )["reply_to"],
        1
    );
    assert_eq!(
        call(
            &mut service,
            "post.create",
            json!({"thread_id":"thread","body":"Third"})
        )["no"],
        3
    );
    let db = Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        9
    );
    assert_eq!(
        db.query_row("SELECT kind FROM actors WHERE id='admin'", [], |row| row
            .get::<_, String>(
            0
        ))
        .unwrap(),
        "human"
    );
    assert!(
        db.prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none()
    );
    assert_eq!(
        db.query_row(
            "SELECT min(sequence) FROM changes WHERE sequence>42",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        101
    );
    assert_eq!(
        db.query_row(
            "SELECT fingerprint FROM create_requests WHERE request_id='receipt'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "fingerprint"
    );
    assert!(
        db.execute("UPDATE posts SET author_id='other'", [])
            .is_err()
    );
    assert!(
        db.execute("UPDATE post_revisions SET body='Tampered'", [])
            .is_err()
    );
}
#[test]
fn v1_migration_preserves_change_sequence_when_all_rows_were_deleted() {
    let root = tempfile::tempdir().unwrap();
    let db = legacy(root.path());
    db.execute(
        "INSERT INTO changes VALUES(42,'post.edit','post','admin','2026-01-02')",
        [],
    )
    .unwrap();
    db.execute(
        "UPDATE sqlite_sequence SET seq=100 WHERE name='changes'",
        [],
    )
    .unwrap();
    db.execute("DELETE FROM changes", []).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT seq FROM sqlite_sequence WHERE name='changes'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        100
    );
    drop(db);

    let mut service = Service::open(root.path()).unwrap();
    call(
        &mut service,
        "board.create",
        json!({"name":"After migration"}),
    );

    let db = Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    let next_sequence: i64 = db
        .query_row("SELECT sequence FROM changes", [], |row| row.get(0))
        .unwrap();
    assert!(next_sequence > 100, "got sequence {next_sequence}");
}
#[test]
fn corrupt_legacy_foreign_keys_roll_back_the_whole_migration() {
    let root = tempfile::tempdir().unwrap();
    let db = legacy(root.path());
    // Deliberately simulate a legacy file written while FK enforcement was disabled.
    db.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    db.execute(
        "INSERT INTO boards VALUES('bad','Board','','missing',1,?1,?1,NULL)",
        params!["2026-01-01"],
    )
    .unwrap();
    drop(db);
    assert_eq!(
        Service::open(root.path()).err().unwrap().code,
        "MIGRATION_INTEGRITY"
    );
    let db = Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("SELECT creator_id FROM boards WHERE id='bad'", [], |row| {
            row.get::<_, String>(0)
        })
        .unwrap(),
        "missing"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='actors'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
