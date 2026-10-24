use rusqlite::Connection;
use serde_json::{Value, json};
use swarm_core::{Service, protocol::Request};

fn request(
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

fn call(service: &mut Service, operation: &str, auth: Option<&str>, args: Value) -> Value {
    request(service, operation, auth, args).unwrap()
}

struct Fixture {
    data: tempfile::TempDir,
    service: Service,
    admin: String,
    thread: String,
}

fn fixture() -> Fixture {
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"correct horse battery staple"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let board = call(
        &mut service,
        "board.create",
        Some(&admin),
        json!({"name":"Board"}),
    );
    let thread = call(
        &mut service,
        "thread.create",
        Some(&admin),
        json!({"board_id":board["id"],"name":"Topic"}),
    );
    Fixture {
        data,
        service,
        admin,
        thread: thread["id"].as_str().unwrap().to_owned(),
    }
}

fn create_account(service: &mut Service, admin: &str, name: &str) -> (String, String) {
    let account = call(
        service,
        "account.create",
        Some(admin),
        json!({"name":name,"password":format!("{name}-password")} ),
    );
    let login = call(
        service,
        "account.login",
        None,
        json!({"name":name,"password":format!("{name}-password")} ),
    );
    (
        account["id"].as_str().unwrap().to_owned(),
        login["token"].as_str().unwrap().to_owned(),
    )
}

#[test]
fn display_settings_are_private_strict_and_persist_after_reopen() {
    let mut f = fixture();
    let (_, second) = create_account(&mut f.service, &f.admin, "DisplayReader");
    assert_eq!(
        call(
            &mut f.service,
            "display.settings.get",
            Some(&f.admin),
            json!({})
        ),
        json!({"settings":null})
    );
    let chosen = json!({"language":"ja","theme":"dark"});
    assert_eq!(
        call(
            &mut f.service,
            "display.settings.set",
            Some(&f.admin),
            chosen.clone()
        ),
        chosen
    );
    assert_eq!(
        call(
            &mut f.service,
            "display.settings.get",
            Some(&second),
            json!({})
        ),
        json!({"settings":null})
    );
    for args in [
        json!({"language":"fr","theme":"dark"}),
        json!({"language":"ja","theme":"system"}),
        json!({"language":"en"}),
        json!({"language":"en","theme":"light","account_id":"other"}),
    ] {
        assert_eq!(
            request(&mut f.service, "display.settings.set", Some(&f.admin), args)
                .unwrap_err()
                .exit_code,
            2
        );
    }
    assert!(request(&mut f.service, "display.settings.get", None, json!({})).is_err());
    drop(f.service);
    let mut reopened = Service::open(f.data.path()).unwrap();
    assert_eq!(
        call(
            &mut reopened,
            "display.settings.get",
            Some(&f.admin),
            json!({})
        ),
        json!({"settings":{"language":"ja","theme":"dark"}})
    );
}

#[test]
fn corrupt_display_preferences_are_not_replaced_with_defaults() {
    let mut f = fixture();
    let id = call(&mut f.service, "account.status", Some(&f.admin), json!({}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let db = Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,'broken')",
        [format!("display-settings-v1:{id}")],
    )
    .unwrap();
    assert_eq!(
        request(
            &mut f.service,
            "display.settings.get",
            Some(&f.admin),
            json!({})
        )
        .unwrap_err()
        .code,
        "DISPLAY_SETTINGS_STATE"
    );
    assert_eq!(
        db.query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [format!("display-settings-v1:{id}")],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "broken"
    );
}

fn grant_conversation(service: &mut Service, admin: &str, account_id: &str) {
    for operation in [
        "board.list",
        "board.get",
        "thread.list",
        "thread.get",
        "post.list",
        "post.get",
    ] {
        call(
            service,
            "permission.set",
            Some(admin),
            json!({"subject_type":"account","subject_id":account_id,"operation":operation,
                "scope":"global","target_id":"","decision":"allow"}),
        );
    }
}

#[test]
fn notification_preferences_and_subscriptions_are_account_scoped_and_persist() {
    let mut f = fixture();
    let (reader_id, reader) = create_account(&mut f.service, &f.admin, "Reader");
    let (_, other) = create_account(&mut f.service, &f.admin, "Other");
    grant_conversation(&mut f.service, &f.admin, &reader_id);

    assert_eq!(
        call(
            &mut f.service,
            "notification.settings.get",
            Some(&reader),
            json!({})
        ),
        json!({"enabled":true,"include_excerpt":false})
    );
    assert_eq!(
        call(
            &mut f.service,
            "notification.settings.set",
            Some(&reader),
            json!({"enabled":false,"include_excerpt":true}),
        ),
        json!({"enabled":false,"include_excerpt":true})
    );
    assert_eq!(
        call(
            &mut f.service,
            "thread.subscribe",
            Some(&reader),
            json!({"id":f.thread,"subscribed":true}),
        ),
        json!({"id":f.thread,"subscribed":true})
    );
    call(
        &mut f.service,
        "thread.subscribe",
        Some(&reader),
        json!({"id":f.thread,"subscribed":true}),
    );
    let existing = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"Read then unsubscribe"}),
    );
    call(
        &mut f.service,
        "post.read.mark",
        Some(&reader),
        json!({"thread_id":f.thread,"post_ids":[existing["id"]]}),
    );
    call(
        &mut f.service,
        "thread.subscribe",
        Some(&reader),
        json!({"id":f.thread,"subscribed":false}),
    );
    assert_eq!(
        call(
            &mut f.service,
            "conversation.unread",
            Some(&reader),
            json!({})
        )["total"],
        0
    );
    assert_eq!(
        Connection::open(f.data.path().join("swarm.sqlite3"))
            .unwrap()
            .query_row(
                "SELECT count(*) FROM notification_post_reads WHERE account_id=?1",
                [&reader_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
    call(
        &mut f.service,
        "thread.subscribe",
        Some(&reader),
        json!({"id":f.thread,"subscribed":true}),
    );
    assert_eq!(
        call(
            &mut f.service,
            "conversation.subscriptions",
            Some(&reader),
            json!({})
        )["items"],
        json!([{"id":f.thread}])
    );
    assert_eq!(
        call(
            &mut f.service,
            "conversation.subscriptions",
            Some(&other),
            json!({})
        )["items"],
        json!([])
    );

    drop(f.service);
    let mut reopened = Service::open(f.data.path()).unwrap();
    assert_eq!(
        call(
            &mut reopened,
            "conversation.subscriptions",
            Some(&reader),
            json!({})
        )["items"],
        json!([{"id":f.thread}])
    );
    assert_eq!(
        call(
            &mut reopened,
            "notification.settings.get",
            Some(&reader),
            json!({})
        ),
        json!({"enabled":false,"include_excerpt":true})
    );
    assert_eq!(
        call(
            &mut reopened,
            "notification.settings.get",
            Some(&other),
            json!({})
        ),
        json!({"enabled":true,"include_excerpt":false})
    );
    let db = Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT account_id FROM notification_settings WHERE account_id=?1",
            [&reader_id],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        reader_id
    );
}

#[test]
fn only_explicitly_marked_posts_are_read_and_unread_counts_are_per_account() {
    let mut f = fixture();
    let (reader_id, reader) = create_account(&mut f.service, &f.admin, "Reader");
    let (_, other) = create_account(&mut f.service, &f.admin, "Other");
    grant_conversation(&mut f.service, &f.admin, &reader_id);
    call(
        &mut f.service,
        "permission.set",
        Some(&f.admin),
        json!({"subject_type":"account","subject_id":reader_id,"operation":"changes.get",
            "scope":"global","target_id":"","decision":"allow"}),
    );
    let other_id = Connection::open(f.data.path().join("swarm.sqlite3"))
        .unwrap()
        .query_row("SELECT id FROM accounts WHERE name='Other'", [], |row| {
            row.get::<_, String>(0)
        })
        .unwrap();
    grant_conversation(&mut f.service, &f.admin, &other_id);
    let mut ids = Vec::new();
    for n in 1..=50 {
        ids.push(
            call(
                &mut f.service,
                "post.create",
                Some(&f.admin),
                json!({"thread_id":f.thread,"body":format!("Post {n}")}),
            )["id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    assert_eq!(
        call(
            &mut f.service,
            "conversation.unread",
            Some(&reader),
            json!({})
        ),
        json!({"items":[{"thread_id":f.thread,"unread_count":50}],"total":50})
    );
    assert_eq!(
        call(
            &mut f.service,
            "post.read.mark",
            Some(&reader),
            json!({"thread_id":f.thread,"post_ids":[ids[0],ids[49],ids[0]]}),
        ),
        json!({"marked":2})
    );
    assert_eq!(
        call(
            &mut f.service,
            "conversation.unread",
            Some(&reader),
            json!({})
        )["total"],
        48
    );
    assert_eq!(
        call(
            &mut f.service,
            "conversation.unread",
            Some(&other),
            json!({})
        )["total"],
        50
    );
    drop(f.service);
    let mut reopened = Service::open(f.data.path()).unwrap();
    assert_eq!(
        call(
            &mut reopened,
            "conversation.unread",
            Some(&reader),
            json!({})
        )["total"],
        48
    );
    let db = Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM notification_post_reads WHERE account_id=?1",
            [&reader_id],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
}

#[test]
fn notification_inputs_are_strict_and_read_marking_is_atomic_and_visibility_checked() {
    let mut f = fixture();
    let (_, reader) = create_account(&mut f.service, &f.admin, "Reader");
    let reader_id: String = Connection::open(f.data.path().join("swarm.sqlite3"))
        .unwrap()
        .query_row("SELECT id FROM accounts WHERE name='Reader'", [], |row| {
            row.get(0)
        })
        .unwrap();
    grant_conversation(&mut f.service, &f.admin, &reader_id);
    call(
        &mut f.service,
        "permission.set",
        Some(&f.admin),
        json!({"subject_type":"account","subject_id":reader_id,"operation":"changes.get",
            "scope":"global","target_id":"","decision":"allow"}),
    );
    let post = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"Visible"}),
    );
    let other_board = call(
        &mut f.service,
        "board.create",
        Some(&f.admin),
        json!({"name":"Other board"}),
    );
    let other_thread = call(
        &mut f.service,
        "thread.create",
        Some(&f.admin),
        json!({"board_id":other_board["id"],"name":"Other"}),
    );
    let other_post = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":other_thread["id"],"body":"Other thread"}),
    );

    for args in [
        json!({"thread_id":f.thread,"post_ids":[]}),
        json!({"thread_id":f.thread,"post_ids":[post["id"],other_post["id"]]}),
        json!({"thread_id":f.thread,"post_ids":[post["id"]],"account_id":"spoof"}),
        json!({"thread_id":f.thread,"post_ids":"wrong type"}),
    ] {
        assert!(request(&mut f.service, "post.read.mark", Some(&reader), args).is_err());
        assert_eq!(
            call(
                &mut f.service,
                "conversation.unread",
                Some(&reader),
                json!({})
            )["total"],
            2
        );
        let changes = request(&mut f.service, "changes.get", Some(&reader), json!({}));
        assert!(changes.is_ok(), "{changes:?}");
        assert_eq!(
            call(
                &mut f.service,
                "conversation.unread",
                Some(&reader),
                json!({})
            )["total"],
            2
        );
    }
    for oversized in ["x".repeat(4097), "日".repeat(1366)] {
        let error = request(
            &mut f.service,
            "post.read.mark",
            Some(&reader),
            json!({"thread_id":f.thread,"post_ids":[oversized]}),
        )
        .unwrap_err();
        assert_eq!(error.code, "INVALID_INPUT");
        assert_eq!(
            call(
                &mut f.service,
                "conversation.unread",
                Some(&reader),
                json!({})
            )["total"],
            2
        );
    }
    let too_many = json!({"thread_id":f.thread,"post_ids":(0..201).map(|i| format!("id-{i}")).collect::<Vec<_>>()});
    assert!(request(&mut f.service, "post.read.mark", Some(&reader), too_many).is_err());

    assert_eq!(
        call(
            &mut f.service,
            "post.read.mark",
            Some(&reader),
            json!({"thread_id":f.thread,"post_ids":[post["id"]]}),
        ),
        json!({"marked":1})
    );
    assert_eq!(
        call(
            &mut f.service,
            "post.read.mark",
            Some(&reader),
            json!({"thread_id":f.thread,"post_ids":vec![post["id"].clone(); 200]}),
        ),
        json!({"marked":1})
    );
    assert!(
        request(
            &mut f.service,
            "post.read.mark",
            Some(&reader),
            json!({"thread_id":f.thread,"post_ids":vec![post["id"].clone(); 201]}),
        )
        .is_err()
    );
    assert_eq!(
        call(
            &mut f.service,
            "conversation.unread",
            Some(&reader),
            json!({})
        )["total"],
        1
    );
    for invalid in [
        json!({"id":"","subscribed":true}),
        json!({"id":f.thread,"subscribed":"true"}),
        json!({"id":"x".repeat(4097),"subscribed":true}),
    ] {
        assert!(request(&mut f.service, "thread.subscribe", Some(&reader), invalid).is_err());
    }
    assert!(
        request(
            &mut f.service,
            "notification.settings.set",
            Some(&reader),
            json!({"enabled":"false","include_excerpt":false}),
        )
        .is_err()
    );
    call(
        &mut f.service,
        "permission.set",
        Some(&f.admin),
        json!({"subject_type":"account","subject_id":reader_id,"operation":"post.get","scope":"global","target_id":"","decision":"deny"}),
    );
    assert!(
        request(
            &mut f.service,
            "post.read.mark",
            Some(&reader),
            json!({"thread_id":f.thread,"post_ids":[other_post["id"]]}),
        )
        .is_err()
    );
    assert!(
        request(
            &mut f.service,
            "thread.subscribe",
            Some(&reader),
            json!({"id":f.thread,"subscribed":true,"account_id":"spoof"}),
        )
        .is_err()
    );
}

#[test]
fn ai_cannot_call_human_notification_operations() {
    let mut f = fixture();
    let project = call(
        &mut f.service,
        "project.register",
        Some(&f.admin),
        json!({"name":"Notification test","root":f.data.path()}),
    );
    let identity = call(
        &mut f.service,
        "identity.start",
        Some(&f.admin),
        json!({"project_id":project["id"],"model_name":"test"}),
    );
    let token = identity["session_token"].as_str().unwrap();
    for operation in [
        "conversation.subscriptions",
        "thread.subscribe",
        "conversation.unread",
        "post.read.mark",
        "notification.settings.get",
        "notification.settings.set",
        "display.settings.get",
        "display.settings.set",
    ] {
        let args = if operation == "thread.subscribe" {
            json!({"id":f.thread,"subscribed":true})
        } else if operation == "post.read.mark" {
            json!({"thread_id":f.thread,"post_ids":["unknown"]})
        } else if operation == "notification.settings.set" {
            json!({"enabled":false,"include_excerpt":true})
        } else if operation == "display.settings.set" {
            json!({"language":"ja","theme":"dark"})
        } else {
            json!({})
        };
        assert_eq!(
            request(&mut f.service, operation, Some(token), args)
                .unwrap_err()
                .exit_code,
            4
        );
    }
}

#[test]
fn v5_upgrade_adds_notification_tables_without_changing_pins_or_cursor_data() {
    let mut f = fixture();
    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"Existing post"}),
    );
    call(
        &mut f.service,
        "thread.pin",
        Some(&f.admin),
        json!({"id":f.thread,"pinned":true}),
    );
    let admin_id = call(&mut f.service, "account.status", Some(&f.admin), json!({}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    call(
        &mut f.service,
        "permission.set",
        Some(&f.admin),
        json!({"subject_type":"account","subject_id":admin_id,"operation":"changes.get",
            "scope":"global","target_id":"","decision":"allow"}),
    );
    let cursor = call(
        &mut f.service,
        "changes.get",
        Some(&f.admin),
        json!({"thread_id":f.thread}),
    )["change_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    drop(f.service);
    let db = Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES('changes.cursor.test','preserve')",
        [],
    )
    .unwrap();
    let last_change = db
        .query_row("SELECT MAX(sequence) FROM changes", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();
    db.execute_batch(
        "DROP TABLE IF EXISTS notification_outbox; DROP TABLE IF EXISTS notification_selection; \
         DROP TABLE IF EXISTS notification_post_reads; DROP TABLE IF EXISTS notification_settings; \
         DROP TABLE IF EXISTS thread_subscriptions; DROP TRIGGER posts_purged_body_immutable; \
         DROP INDEX post_revisions_saved_at; ALTER TABLE posts DROP COLUMN body_purged_at; \
         DROP TABLE retention_settings; ALTER TABLE threads DROP COLUMN archived_at; PRAGMA user_version=5;",
    )
    .unwrap();
    drop(db);

    let mut upgraded = Service::open(f.data.path()).unwrap();
    let resumed = call(
        &mut upgraded,
        "changes.get",
        Some(&f.admin),
        json!({"thread_id":f.thread,"after":cursor}),
    );
    assert_eq!(resumed["items"], json!([]));
    let db = Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        9
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM thread_pins WHERE thread_id=?1",
            [&f.thread],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT value FROM app_metadata WHERE key='changes.cursor.test'",
            [],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "preserve"
    );
    assert_eq!(
        db.query_row("SELECT MAX(sequence) FROM changes", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        last_change
    );
    assert_eq!(
        call(
            &mut upgraded,
            "conversation.unread",
            Some(&f.admin),
            json!({})
        )["total"],
        1
    );
}

#[test]
fn unread_and_subscribe_do_not_disclose_threads_without_current_visibility() {
    let mut f = fixture();
    let (reader_id, reader) = create_account(&mut f.service, &f.admin, "Reader");
    for operation in ["thread.get", "post.get"] {
        call(
            &mut f.service,
            "permission.set",
            Some(&f.admin),
            json!({"subject_type":"account","subject_id":reader_id,"operation":operation,
                "scope":"thread","target_id":f.thread,"decision":"deny"}),
        );
    }
    let hidden = request(
        &mut f.service,
        "thread.subscribe",
        Some(&reader),
        json!({"id":f.thread,"subscribed":true}),
    )
    .unwrap_err();
    let nonexistent = request(
        &mut f.service,
        "thread.subscribe",
        Some(&reader),
        json!({"id":"no-such-thread","subscribed":true}),
    )
    .unwrap_err();
    assert_eq!(hidden.code, "NOT_FOUND");
    assert_eq!(hidden.code, nonexistent.code);
    assert_eq!(
        call(
            &mut f.service,
            "conversation.unread",
            Some(&reader),
            json!({})
        ),
        json!({"items":[],"total":0})
    );
}

#[test]
fn live_post_and_thread_tombstones_leave_read_receipts_and_restore_unread_items() {
    let mut f = fixture();
    let (reader_id, reader) = create_account(&mut f.service, &f.admin, "Reader");
    grant_conversation(&mut f.service, &f.admin, &reader_id);
    let already_read = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"Read post"}),
    );
    call(
        &mut f.service,
        "post.read.mark",
        Some(&reader),
        json!({"thread_id":f.thread,"post_ids":[already_read["id"]]}),
    );
    call(
        &mut f.service,
        "post.delete",
        Some(&f.admin),
        json!({"thread_id":f.thread,"no":already_read["no"]}),
    );
    assert_eq!(
        call(
            &mut f.service,
            "conversation.unread",
            Some(&reader),
            json!({})
        )["total"],
        0
    );
    call(
        &mut f.service,
        "post.restore",
        Some(&f.admin),
        json!({"thread_id":f.thread,"no":already_read["no"]}),
    );
    assert_eq!(
        call(
            &mut f.service,
            "conversation.unread",
            Some(&reader),
            json!({})
        )["total"],
        0
    );

    let unread_post = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"Unread post"}),
    );
    call(
        &mut f.service,
        "thread.delete",
        Some(&f.admin),
        json!({"id":f.thread}),
    );
    assert_eq!(
        call(
            &mut f.service,
            "conversation.unread",
            Some(&reader),
            json!({})
        )["total"],
        0
    );
    call(
        &mut f.service,
        "thread.restore",
        Some(&f.admin),
        json!({"id":f.thread}),
    );
    assert_eq!(
        call(
            &mut f.service,
            "conversation.unread",
            Some(&reader),
            json!({})
        )["total"],
        1
    );
    call(
        &mut f.service,
        "post.delete",
        Some(&f.admin),
        json!({"thread_id":f.thread,"no":unread_post["no"]}),
    );
    assert_eq!(
        call(
            &mut f.service,
            "conversation.unread",
            Some(&reader),
            json!({})
        )["total"],
        0
    );
    call(
        &mut f.service,
        "post.restore",
        Some(&f.admin),
        json!({"thread_id":f.thread,"no":unread_post["no"]}),
    );
    assert_eq!(
        call(
            &mut f.service,
            "conversation.unread",
            Some(&reader),
            json!({})
        )["total"],
        1
    );
    assert!(
        request(
            &mut f.service,
            "post.read.mark",
            Some(&reader),
            json!({"thread_id":f.thread,"post_ids":[unread_post["id"]]}),
        )
        .is_ok()
    );
}

#[test]
fn failed_v5_migration_rolls_back_notification_schema_and_version() {
    let f = fixture();
    drop(f.service);
    let db = Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    db.execute_batch(
        "DROP TABLE notification_outbox; DROP TABLE notification_selection; \
         DROP TABLE notification_post_reads; DROP TABLE notification_settings; \
         DROP TABLE thread_subscriptions; DROP TRIGGER posts_purged_body_immutable; \
         DROP INDEX post_revisions_saved_at; ALTER TABLE posts DROP COLUMN body_purged_at; \
         DROP TABLE retention_settings; ALTER TABLE threads DROP COLUMN archived_at; PRAGMA user_version=5; PRAGMA foreign_keys=OFF;",
    )
    .unwrap();
    db.execute(
        "INSERT INTO thread_pins(account_id,thread_id,created_at) VALUES('missing-account',?1,'now')",
        [&f.thread],
    )
    .unwrap();
    drop(db);

    assert_eq!(
        Service::open(f.data.path()).err().unwrap().code,
        "MIGRATION_INTEGRITY"
    );
    let db = Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        5
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='notification_settings'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0
    );
}
