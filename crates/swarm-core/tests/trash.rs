use rusqlite::Connection;
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

struct Fixture {
    root: tempfile::TempDir,
    service: Service,
    admin: String,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Trash Admin","password":"trash-fixture-password"}),
    )
    .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    Fixture {
        root,
        service,
        admin,
    }
}

fn db(f: &Fixture) -> Connection {
    Connection::open(f.root.path().join("swarm.sqlite3")).unwrap()
}

#[test]
fn delayed_creation_retry_cannot_recreate_purged_conversations_and_receipts_expire() {
    let mut f = fixture();
    let parent = call(
        &mut f.service,
        "board.create",
        Some(&f.admin),
        json!({"name":"Surviving parent"}),
    )
    .unwrap();
    for (kind, args) in [
        ("board", json!({"name":"Purged once"})),
        (
            "thread",
            json!({"board_id":parent["id"],"name":"Purged once"}),
        ),
    ] {
        let request = || Request {
            operation: format!("{kind}.create"),
            auth: Some(f.admin.clone()),
            args: args.clone(),
            context: json!({}),
            request_id: Some(format!("purged-{kind}-intent")),
        };
        let target = f.service.request(request()).unwrap();
        call(
            &mut f.service,
            &format!("{kind}.delete"),
            Some(&f.admin),
            json!({"id":target["id"]}),
        )
        .unwrap();
        call(
            &mut f.service,
            &format!("{kind}.purge"),
            Some(&f.admin),
            json!({"id":target["id"],"confirmed":true}),
        )
        .unwrap();
        assert_eq!(f.service.request(request()).unwrap_err().exit_code, 5);
    }
    let database = db(&f);
    assert_eq!(
        database
            .query_row("SELECT count(*) FROM boards", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        database
            .query_row("SELECT count(*) FROM threads", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        database
            .query_row("SELECT count(*) FROM create_requests", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        2
    );
    database
        .execute(
            "UPDATE create_requests SET created_at='2000-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
    f.service.purge_expired_data().unwrap();
    assert_eq!(
        database
            .query_row("SELECT count(*) FROM create_requests", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn purge_is_human_only_confirmed_deleted_and_cascades_conversation_state() {
    let mut f = fixture();
    let board = call(
        &mut f.service,
        "board.create",
        Some(&f.admin),
        json!({"name":"Purge Board"}),
    )
    .unwrap();
    let thread = call(
        &mut f.service,
        "thread.create",
        Some(&f.admin),
        json!({"board_id":board["id"],"name":"Nested Replies"}),
    )
    .unwrap();
    let root = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":thread["id"],"body":format!("root {}", "x".repeat(80_000))}),
    )
    .unwrap();
    let child = call(
        &mut f.service,
        "response.create",
        Some(&f.admin),
        json!({"thread_id":thread["id"],"to":root["no"],"body":format!("child {}", "y".repeat(80_000))}),
    )
    .unwrap();
    let grandchild = call(
        &mut f.service,
        "response.create",
        Some(&f.admin),
        json!({"thread_id":thread["id"],"to":child["no"],"body":format!("grandchild {}", "z".repeat(80_000))}),
    )
    .unwrap();
    {
        let db = db(&f);
        let (author, display): (String, String) = db
            .query_row(
                "SELECT author_id,display_id FROM posts WHERE id=?1",
                [root["id"].as_str().unwrap()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let body = "x".repeat(90_000);
        for index in 0..40 {
            db.execute(
                "INSERT INTO posts(id,thread_id,no,body,author_id,model_name,display_id,created_at,updated_at) \
                 VALUES(?1,?2,?3,?4,?5,'human',?6,'now','now')",
                rusqlite::params![format!("bulk-{index}"), thread["id"].as_str().unwrap(), index + 4, body, author, display],
            )
            .unwrap();
        }
        db.execute(
            "UPDATE threads SET next_no=44 WHERE id=?1",
            [thread["id"].as_str().unwrap()],
        )
        .unwrap();
    }
    call(
        &mut f.service,
        "post.edit",
        Some(&f.admin),
        json!({"thread_id":thread["id"],"no":root["no"],"body":"edited root","if_revision":1}),
    )
    .unwrap();
    call(
        &mut f.service,
        "thread.pin",
        Some(&f.admin),
        json!({"id":thread["id"],"pinned":true}),
    )
    .unwrap();
    let account_id =
        call(&mut f.service, "account.status", Some(&f.admin), json!({})).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
    call(&mut f.service, "ui.preferences.set", Some(&f.admin), json!({
        "expected_account_id":account_id,"scope":"thread_scroll","key":thread["id"],
        "value":{"version":1,"postNo":grandchild["no"],"offset":0,"scrollTop":5,"atBottom":false}
    })).unwrap();
    {
        let db = db(&f);
        db.execute(
            "INSERT INTO thread_subscriptions(account_id,thread_id,created_at) VALUES(?1,?2,'now')",
            [account_id.as_str(), thread["id"].as_str().unwrap()],
        )
        .unwrap();
        db.execute(
            "INSERT INTO notification_post_reads(account_id,post_id,read_at) VALUES(?1,?2,'now')",
            [account_id.as_str(), root["id"].as_str().unwrap()],
        )
        .unwrap();
        db.execute("INSERT INTO notification_outbox(delivery_id,generation,account_id,thread_id,post_id,kind,created_at) VALUES('delivery','generation',?1,?2,?3,'post','now')", rusqlite::params![account_id,thread["id"].as_str().unwrap(),root["id"].as_str().unwrap()]).unwrap();
        db.execute("INSERT INTO create_requests(actor_id,request_id,fingerprint,operation,entity_id,created_at) VALUES(?1,'fixture','fingerprint','thread.create',?2,'now')", [account_id.as_str(), thread["id"].as_str().unwrap()]).unwrap();
    }
    let project_root = tempfile::tempdir().unwrap();
    let project = call(
        &mut f.service,
        "project.register",
        Some(&f.admin),
        json!({"name":"Trash AI","root":project_root.path()}),
    )
    .unwrap();
    let ai = call(
        &mut f.service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )
    .unwrap();

    let live = call(
        &mut f.service,
        "thread.purge",
        Some(&f.admin),
        json!({"id":thread["id"],"confirmed":true}),
    )
    .unwrap_err();
    assert_eq!(live.code, "STALE_SELECTION");
    let ai_purge = call(
        &mut f.service,
        "thread.purge",
        ai["session_token"].as_str(),
        json!({"id":thread["id"],"confirmed":true}),
    )
    .unwrap_err();
    assert_eq!(ai_purge.exit_code, 4);

    call(
        &mut f.service,
        "thread.delete",
        Some(&f.admin),
        json!({"id":thread["id"]}),
    )
    .unwrap();
    call(
        &mut f.service,
        "board.delete",
        Some(&f.admin),
        json!({"id":board["id"]}),
    )
    .unwrap();
    let list = call(
        &mut f.service,
        "conversation.deleted",
        Some(&f.admin),
        json!({}),
    )
    .unwrap();
    assert_eq!(list["threads"][0]["parent_deleted"], true);
    assert_eq!(list["threads"][0]["can_restore"], true);
    assert_eq!(list["threads"][0]["can_purge"], true);
    let unconfirmed = call(
        &mut f.service,
        "board.purge",
        Some(&f.admin),
        json!({"id":board["id"],"confirmed":false}),
    )
    .unwrap_err();
    assert_eq!(unconfirmed.code, "CONFIRMATION_REQUIRED");

    let before = std::fs::metadata(f.root.path().join("swarm.sqlite3"))
        .unwrap()
        .len();
    let result = call(
        &mut f.service,
        "board.purge",
        Some(&f.admin),
        json!({"id":board["id"],"confirmed":true}),
    )
    .unwrap();
    assert_eq!(result["removed_threads"], 1);
    assert_eq!(result["removed_posts"], 43);
    assert_eq!(result["removed_revisions"], 1);
    assert_eq!(result["storage_reclaimed"], true);
    let after = std::fs::metadata(f.root.path().join("swarm.sqlite3"))
        .unwrap()
        .len();
    assert!(
        after < before,
        "vacuum should contract the database after a large purge"
    );
    let db = db(&f);
    for table in [
        "boards",
        "threads",
        "posts",
        "post_revisions",
        "thread_pins",
        "thread_subscriptions",
        "notification_post_reads",
        "notification_outbox",
    ] {
        let count: i64 = db
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table} should have no conversation rows");
    }
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM app_metadata WHERE key LIKE 'ui-preference-v1:%:thread_scroll:%'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM post_revisions WHERE post_id=?1",
            [root["id"].as_str().unwrap()],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn batch_restore_orders_parent_and_child_and_rejects_stale_batch_atomically() {
    let mut f = fixture();
    let board = call(
        &mut f.service,
        "board.create",
        Some(&f.admin),
        json!({"name":"Restore Board"}),
    )
    .unwrap();
    let thread = call(
        &mut f.service,
        "thread.create",
        Some(&f.admin),
        json!({"board_id":board["id"],"name":"Restore Child"}),
    )
    .unwrap();
    call(
        &mut f.service,
        "thread.delete",
        Some(&f.admin),
        json!({"id":thread["id"]}),
    )
    .unwrap();
    call(
        &mut f.service,
        "board.delete",
        Some(&f.admin),
        json!({"id":board["id"]}),
    )
    .unwrap();
    let child_only = call(
        &mut f.service,
        "conversation.batch",
        Some(&f.admin),
        json!({"action":"restore","items":[{"kind":"thread","id":thread["id"]}],"confirmed":true}),
    )
    .unwrap_err();
    assert_eq!(child_only.code, "STALE_SELECTION");
    let restored = call(&mut f.service, "conversation.batch", Some(&f.admin), json!({
        "action":"restore","items":[{"kind":"thread","id":thread["id"]},{"kind":"board","id":board["id"]}],"confirmed":true
    })).unwrap();
    assert_eq!(restored["items"].as_array().unwrap().len(), 2);
    assert!(
        call(
            &mut f.service,
            "thread.get",
            Some(&f.admin),
            json!({"id":thread["id"]})
        )
        .is_ok()
    );

    call(
        &mut f.service,
        "board.delete",
        Some(&f.admin),
        json!({"id":board["id"]}),
    )
    .unwrap();
    let stale = call(&mut f.service, "conversation.batch", Some(&f.admin), json!({
        "action":"restore","items":[{"kind":"board","id":board["id"]},{"kind":"thread","id":thread["id"]}],"confirmed":true
    })).unwrap_err();
    assert_eq!(stale.code, "STALE_SELECTION");
    assert!(
        call(
            &mut f.service,
            "board.get",
            Some(&f.admin),
            json!({"id":board["id"]})
        )
        .is_err()
    );
}

#[test]
fn batch_purge_normalizes_descendant_and_denied_permission_changes_nothing() {
    let mut f = fixture();
    let board = call(
        &mut f.service,
        "board.create",
        Some(&f.admin),
        json!({"name":"Bulk Board"}),
    )
    .unwrap();
    let thread = call(
        &mut f.service,
        "thread.create",
        Some(&f.admin),
        json!({"board_id":board["id"],"name":"Bulk Thread"}),
    )
    .unwrap();
    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":thread["id"],"body":"body"}),
    )
    .unwrap();
    call(
        &mut f.service,
        "thread.delete",
        Some(&f.admin),
        json!({"id":thread["id"]}),
    )
    .unwrap();
    call(
        &mut f.service,
        "board.delete",
        Some(&f.admin),
        json!({"id":board["id"]}),
    )
    .unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let project = call(
        &mut f.service,
        "project.register",
        Some(&f.admin),
        json!({"name":"Batch AI","root":project_root.path()}),
    )
    .unwrap();
    let ai = call(
        &mut f.service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    let denied = call(
        &mut f.service,
        "conversation.batch",
        ai["session_token"].as_str(),
        json!({
            "action":"purge","items":[{"kind":"board","id":board["id"]}],"confirmed":true
        }),
    )
    .unwrap_err();
    assert_eq!(denied.exit_code, 4);
    assert_eq!(
        db(&f)
            .query_row(
                "SELECT count(*) FROM boards WHERE id=?1",
                [board["id"].as_str().unwrap()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    let result = call(&mut f.service, "conversation.batch", Some(&f.admin), json!({
        "action":"purge","items":[{"kind":"thread","id":thread["id"]},{"kind":"board","id":board["id"]}],"confirmed":true
    })).unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 2);
    assert_eq!(result["items"][0]["covered_by_board"], board["id"]);
    assert_eq!(
        db(&f)
            .query_row(
                "SELECT count(*) FROM boards WHERE id=?1",
                [board["id"].as_str().unwrap()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn individual_thread_purge_requires_its_new_human_grant() {
    let mut f = fixture();
    let board = call(
        &mut f.service,
        "board.create",
        Some(&f.admin),
        json!({"name":"Grant Board"}),
    )
    .unwrap();
    let thread = call(
        &mut f.service,
        "thread.create",
        Some(&f.admin),
        json!({"board_id":board["id"],"name":"Grant Thread"}),
    )
    .unwrap();
    call(
        &mut f.service,
        "thread.delete",
        Some(&f.admin),
        json!({"id":thread["id"]}),
    )
    .unwrap();
    let account = call(
        &mut f.service,
        "account.create",
        Some(&f.admin),
        json!({"name":"Purge User","password":"purge-user-password"}),
    )
    .unwrap();
    let user = call(
        &mut f.service,
        "account.login",
        None,
        json!({"name":"Purge User","password":"purge-user-password"}),
    )
    .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        call(
            &mut f.service,
            "thread.purge",
            Some(&user),
            json!({"id":thread["id"],"confirmed":true})
        )
        .unwrap_err()
        .code,
        "PERMISSION_DENIED"
    );
    for (operation, scope, target) in [
        ("thread.purge", "thread", thread["id"].as_str().unwrap()),
        ("thread.list", "thread", thread["id"].as_str().unwrap()),
        ("thread.get", "thread", thread["id"].as_str().unwrap()),
        ("board.get", "board", board["id"].as_str().unwrap()),
    ] {
        call(
            &mut f.service,
            "permission.set",
            Some(&f.admin),
            json!({
                "subject_type":"account","subject_id":account["id"],"operation":operation,
                "scope":scope,"target_id":target,"decision":"allow"
            }),
        )
        .unwrap();
    }
    let listed = call(
        &mut f.service,
        "conversation.deleted",
        Some(&user),
        json!({}),
    )
    .unwrap();
    assert_eq!(listed["threads"][0]["can_purge"], true);
    let purged = call(
        &mut f.service,
        "thread.purge",
        Some(&user),
        json!({"id":thread["id"],"confirmed":true}),
    )
    .unwrap();
    assert_eq!(purged["kind"], "thread");
    assert_eq!(
        db(&f)
            .query_row(
                "SELECT count(*) FROM threads WHERE id=?1",
                [thread["id"].as_str().unwrap()],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn failed_reclamation_remains_pending_for_later_maintenance_retry() {
    let mut f = fixture();
    let board = call(
        &mut f.service,
        "board.create",
        Some(&f.admin),
        json!({"name":"Retry Board"}),
    )
    .unwrap();
    call(
        &mut f.service,
        "board.delete",
        Some(&f.admin),
        json!({"id":board["id"]}),
    )
    .unwrap();
    let reader = db(&f);
    reader.execute_batch("BEGIN DEFERRED;").unwrap();
    let _: i64 = reader
        .query_row("SELECT count(*) FROM app_metadata", [], |row| row.get(0))
        .unwrap();

    let result = call(
        &mut f.service,
        "board.purge",
        Some(&f.admin),
        json!({"id":board["id"],"confirmed":true}),
    )
    .unwrap();
    assert_eq!(result["storage_reclaimed"], false);
    assert_eq!(
        db(&f)
            .query_row(
                "SELECT value FROM app_metadata WHERE key='storage.reclaim_pending'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "1"
    );

    reader.execute_batch("COMMIT;").unwrap();
    assert_eq!(
        f.service.purge_expired_data().unwrap()["storage_reclaimed"],
        true
    );
    assert_eq!(
        db(&f)
            .query_row(
                "SELECT count(*) FROM app_metadata WHERE key='storage.reclaim_pending'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn conversation_batch_accepts_more_than_five_hundred_items() {
    let mut f = fixture();
    let db = db(&f);
    let creator: String = db
        .query_row("SELECT id FROM accounts WHERE is_admin=1", [], |row| {
            row.get(0)
        })
        .unwrap();
    let mut items = Vec::with_capacity(501);
    for index in 0..501 {
        let id = format!("10000000-0000-4000-8000-{index:012x}");
        db.execute(
            "INSERT INTO boards(id,name,description,creator_id,created_at,updated_at,deleted_at) \
             VALUES(?1,'Bulk board','',?2,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','2026-01-02T00:00:00Z')",
            rusqlite::params![id, creator],
        )
        .unwrap();
        items.push(json!({"kind":"board","id":id}));
    }
    drop(db);
    let result = call(
        &mut f.service,
        "conversation.batch",
        Some(&f.admin),
        json!({"action":"purge","items":items,"confirmed":true}),
    )
    .unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 501);
}

#[test]
fn purge_invalidates_an_already_issued_listing_cursor() {
    let mut f = fixture();
    let mut target = None;
    for name in ["Cursor target", "Cursor survivor A", "Cursor survivor B"] {
        let board = call(
            &mut f.service,
            "board.create",
            Some(&f.admin),
            json!({"name":name}),
        )
        .unwrap();
        if name == "Cursor target" {
            target = board["id"].as_str().map(str::to_owned);
        }
    }
    let target = target.unwrap();
    call(
        &mut f.service,
        "board.delete",
        Some(&f.admin),
        json!({"id":target}),
    )
    .unwrap();
    let page = call(
        &mut f.service,
        "board.list",
        Some(&f.admin),
        json!({"limit":1}),
    )
    .unwrap();
    let cursor = page["next_cursor"].as_str().unwrap().to_owned();
    call(
        &mut f.service,
        "board.purge",
        Some(&f.admin),
        json!({"id":target,"confirmed":true}),
    )
    .unwrap();
    assert_eq!(
        call(
            &mut f.service,
            "board.list",
            Some(&f.admin),
            json!({"limit":1,"cursor":cursor}),
        )
        .unwrap_err()
        .code,
        "RESYNC_REQUIRED"
    );
}
