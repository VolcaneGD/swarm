use rusqlite::Connection;
use serde_json::json;
use sha2::{Digest, Sha256};
use swarm_core::{Service, protocol::Request};

fn version_three(path: &std::path::Path) -> Connection {
    let db = Connection::open(path.join("swarm.sqlite3")).unwrap();
    db.execute_batch(include_str!("../src/schema.sql")).unwrap();
    db.execute(
        "INSERT INTO accounts VALUES('admin','Admin','unused','unused',1,'2026-01-01')",
        [],
    )
    .unwrap();
    db.execute(
        "INSERT INTO human_sessions VALUES(?1,'admin')",
        [format!("{:x}", Sha256::digest(b"legacy-token"))],
    )
    .unwrap();
    db.execute_batch(include_str!("../src/migrations/002_actors.sql"))
        .unwrap();
    db.execute_batch(include_str!("../src/migrations/003_policy.sql"))
        .unwrap();
    db.execute_batch("PRAGMA user_version=3;").unwrap();
    db
}
#[test]
fn version_three_upgrade_preserves_sessions_and_enforces_main_admin_protection() {
    let root = tempfile::tempdir().unwrap();
    drop(version_three(root.path()));
    let mut service = Service::open(root.path()).unwrap();
    let status = service
        .request(Request {
            operation: "account.status".into(),
            auth: Some("legacy-token".into()),
            args: json!({}),
            context: json!({}),
            request_id: None,
        })
        .unwrap();
    assert_eq!(status["id"], "admin");
    assert_eq!(status["disabled"], false);
    let db = Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        9
    );
    assert!(
        db.execute("UPDATE accounts SET disabled=1 WHERE id='admin'", [])
            .is_err()
    );
    assert!(
        db.execute("UPDATE accounts SET is_admin=0 WHERE id='admin'", [])
            .is_err()
    );
    assert!(
        db.execute("DELETE FROM accounts WHERE id='admin'", [])
            .is_err()
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM roles", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn incompatible_multi_admin_legacy_database_rolls_back_version_four_migration() {
    let root = tempfile::tempdir().unwrap();
    let db = version_three(root.path());
    db.execute(
        "INSERT INTO accounts VALUES('second','Second','unused','unused',1,'2026-01-01')",
        [],
    )
    .unwrap();
    drop(db);
    assert!(Service::open(root.path()).is_err());
    let db = Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM accounts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='roles'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    let mut query = db.prepare("PRAGMA table_info(accounts)").unwrap();
    let columns = query
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert!(!columns.iter().any(|name| name == "disabled"));
}
