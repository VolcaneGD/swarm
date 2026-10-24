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
    thread: Value,
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
        thread,
    }
}

fn create_reader(f: &mut Fixture, name: &str) -> (String, String) {
    let account = call(
        &mut f.service,
        "account.create",
        Some(&f.admin),
        json!({"name":name,"password":format!("{name}-password")}),
    );
    let login = call(
        &mut f.service,
        "account.login",
        None,
        json!({"name":name,"password":format!("{name}-password")}),
    );
    (
        account["id"].as_str().unwrap().to_owned(),
        login["token"].as_str().unwrap().to_owned(),
    )
}

fn permission(
    f: &mut Fixture,
    account: &str,
    operation: &str,
    scope: &str,
    target: &str,
    decision: &str,
) {
    call(
        &mut f.service,
        "permission.set",
        Some(&f.admin),
        json!({"subject_type":"account","subject_id":account,"operation":operation,
            "scope":scope,"target_id":target,"decision":decision}),
    );
}

fn grant_conversation(f: &mut Fixture, account: &str) {
    for operation in [
        "board.list",
        "board.get",
        "thread.list",
        "thread.get",
        "post.list",
        "post.get",
    ] {
        permission(f, account, operation, "global", "", "allow");
    }
}

#[test]
fn pins_persist_per_account_are_idempotent_and_are_filtered_by_current_view_permissions() {
    let mut f = fixture();
    let thread_id = f.thread["id"].as_str().unwrap().to_owned();
    let (reader_id, reader) = create_reader(&mut f, "Reader");
    let (_, other) = create_reader(&mut f, "OtherReader");
    grant_conversation(&mut f, &reader_id);

    for _ in 0..2 {
        assert_eq!(
            call(
                &mut f.service,
                "thread.pin",
                Some(&reader),
                json!({"id":thread_id,"pinned":true}),
            ),
            json!({"id":thread_id,"pinned":true})
        );
    }
    assert_eq!(
        call(
            &mut f.service,
            "conversation.pins",
            Some(&reader),
            json!({})
        )["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        call(&mut f.service, "conversation.pins", Some(&other), json!({}))["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    drop(f.service);
    let mut reopened = Service::open(f.data.path()).unwrap();
    assert_eq!(
        call(&mut reopened, "conversation.pins", Some(&reader), json!({}))["items"][0]["id"],
        thread_id
    );
    assert_eq!(
        request(
            &mut reopened,
            "thread.pin",
            Some(&reader),
            json!({"id":thread_id,"pinned":false}),
        )
        .unwrap()["pinned"],
        false
    );
    call(
        &mut reopened,
        "thread.pin",
        Some(&reader),
        json!({"id":thread_id,"pinned":true}),
    );
    call(
        &mut reopened,
        "permission.set",
        Some(&f.admin),
        json!({"subject_type":"account","subject_id":reader_id,
            "operation":"post.get","scope":"global","target_id":"","decision":"deny"}),
    );
    assert!(
        call(&mut reopened, "conversation.pins", Some(&reader), json!({}))["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        request(
            &mut reopened,
            "thread.pin",
            Some(&reader),
            json!({"id":thread_id,"pinned":false}),
        )
        .unwrap_err()
        .exit_code,
        4
    );
}

#[test]
fn activity_returns_body_free_post_and_response_metadata_with_exclusive_paging() {
    let mut f = fixture();
    let thread_id = f.thread["id"].as_str().unwrap().to_owned();
    let post = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":thread_id,"body":"BODY_SECRET_ORIGINAL"}),
    );
    call(
        &mut f.service,
        "post.edit",
        Some(&f.admin),
        json!({"thread_id":thread_id,"no":post["no"],"body":"BODY_SECRET_EDITED","if_revision":1}),
    );
    call(
        &mut f.service,
        "response.create",
        Some(&f.admin),
        json!({"thread_id":thread_id,"to":post["no"],"body":"BODY_SECRET_RESPONSE"}),
    );
    call(
        &mut f.service,
        "post.delete",
        Some(&f.admin),
        json!({"thread_id":thread_id,"no":post["no"]}),
    );

    let (reader_id, reader) = create_reader(&mut f, "ActivityReader");
    grant_conversation(&mut f, &reader_id);
    let first = call(
        &mut f.service,
        "conversation.activity",
        Some(&reader),
        json!({"limit":2}),
    );
    let first_items = first["items"].as_array().unwrap();
    assert_eq!(first_items.len(), 2);
    assert!(first["next_before_id"].as_i64().is_some());
    assert!(first_items[0]["id"].as_i64().unwrap() > first_items[1]["id"].as_i64().unwrap());
    let second = call(
        &mut f.service,
        "conversation.activity",
        Some(&reader),
        json!({"limit":100,"before_id":first["next_before_id"]}),
    );
    let items = second["items"].as_array().unwrap();
    assert!(!items.is_empty());
    let mut all = first_items.to_vec();
    all.extend(items.iter().cloned());
    let actions = all
        .iter()
        .filter_map(|item| item["action"].as_str())
        .collect::<Vec<_>>();
    for expected in ["post.create", "post.edit", "response.create", "post.delete"] {
        assert!(
            actions.contains(&expected),
            "missing {expected} in {actions:?}"
        );
    }
    let response = all
        .iter()
        .find(|item| item["action"] == "response.create")
        .unwrap();
    assert_eq!(response["post_no"], 2);
    assert_eq!(response["thread_id"], thread_id);
    let serialized = serde_json::to_string(&all).unwrap();
    for secret in [
        "BODY_SECRET_ORIGINAL",
        "BODY_SECRET_EDITED",
        "BODY_SECRET_RESPONSE",
    ] {
        assert!(!serialized.contains(secret));
    }
    for item in &all {
        assert!(item.get("body").is_none());
        assert!(item.get("actor").is_none());
        assert!(item.get("actor_id").is_none());
    }
}

#[test]
fn activity_cursor_advances_over_raw_rows_even_when_visible_results_are_sparse() {
    let mut f = fixture();
    let mut board_ids = Vec::new();
    for n in 0..6 {
        board_ids.push(
            call(
                &mut f.service,
                "board.create",
                Some(&f.admin),
                json!({"name":format!("Board {n}")}),
            )["id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    let (reader_id, reader) = create_reader(&mut f, "SparseReader");
    permission(&mut f, &reader_id, "read", "global", "", "deny");
    permission(&mut f, &reader_id, "read", "board", &board_ids[0], "allow");
    for operation in ["board.list", "board.get"] {
        permission(
            &mut f,
            &reader_id,
            operation,
            "board",
            &board_ids[0],
            "allow",
        );
    }
    let mut before = None;
    let mut visible = Vec::new();
    for page in 0..4 {
        let args = match before {
            Some(cursor) => json!({"limit":2,"before_id":cursor}),
            None => json!({"limit":2}),
        };
        let result = call(&mut f.service, "conversation.activity", Some(&reader), args);
        visible.extend(result["items"].as_array().unwrap().iter().cloned());
        before = result["next_before_id"].as_i64();
        if before.is_none() {
            break;
        }
        assert!(
            page < 3,
            "cursor did not terminate after scanning the change rows"
        );
    }
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0]["board_id"], board_ids[0]);
    assert!(visible[0]["board_name"].as_str().is_some());
}

#[test]
fn activity_rechecks_current_permissions_and_redacts_hidden_parent_metadata() {
    let mut f = fixture();
    let thread_id = f.thread["id"].as_str().unwrap().to_owned();
    let post = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":thread_id,"body":"not included in activity"}),
    );
    let (reader_id, reader) = create_reader(&mut f, "ScopedReader");
    grant_conversation(&mut f, &reader_id);

    let before = call(
        &mut f.service,
        "conversation.activity",
        Some(&reader),
        json!({"limit":100}),
    );
    assert!(
        before["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "post.create")
    );

    permission(&mut f, &reader_id, "board.get", "global", "", "deny");
    let hidden_parent = call(
        &mut f.service,
        "conversation.activity",
        Some(&reader),
        json!({"limit":100}),
    );
    let thread_item = hidden_parent["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["action"] == "thread.create")
        .unwrap();
    assert!(thread_item["board_id"].is_null());
    assert!(thread_item["board_name"].is_null());
    assert!(
        !hidden_parent["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "board.create")
    );

    permission(&mut f, &reader_id, "post.get", "global", "", "deny");
    let revoked = call(
        &mut f.service,
        "conversation.activity",
        Some(&reader),
        json!({"limit":100}),
    );
    assert!(
        !revoked["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "post.create")
    );

    permission(&mut f, &reader_id, "post.get", "global", "", "allow");
    call(
        &mut f.service,
        "post.delete",
        Some(&f.admin),
        json!({"thread_id":thread_id,"no":post["no"]}),
    );
    permission(&mut f, &reader_id, "thread.restore", "global", "", "deny");
    call(
        &mut f.service,
        "thread.delete",
        Some(&f.admin),
        json!({"id":thread_id}),
    );
    let deleted_hidden = call(
        &mut f.service,
        "conversation.activity",
        Some(&reader),
        json!({"limit":100}),
    );
    assert!(
        !deleted_hidden["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "thread.delete")
    );

    permission(&mut f, &reader_id, "thread.restore", "global", "", "allow");
    let deleted_visible = call(
        &mut f.service,
        "conversation.activity",
        Some(&reader),
        json!({"limit":100}),
    );
    assert!(
        deleted_visible["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "thread.delete")
    );
}

#[test]
fn pin_activity_inputs_and_ai_access_are_rejected() {
    let mut f = fixture();
    let thread_id = f.thread["id"].as_str().unwrap().to_owned();
    for args in [
        json!({"id":thread_id,"pinned":"yes"}),
        json!({"id":thread_id,"pinned":true,"extra":1}),
        json!({"id":" ","pinned":true}),
    ] {
        assert_eq!(
            request(&mut f.service, "thread.pin", Some(&f.admin), args)
                .unwrap_err()
                .exit_code,
            2
        );
    }
    for args in [
        json!({"limit":0}),
        json!({"limit":101}),
        json!({"before_id":-1}),
        json!({"extra":1}),
    ] {
        assert_eq!(
            request(
                &mut f.service,
                "conversation.activity",
                Some(&f.admin),
                args
            )
            .unwrap_err()
            .exit_code,
            2
        );
    }

    let project_root = tempfile::tempdir().unwrap();
    let project = call(
        &mut f.service,
        "project.register",
        Some(&f.admin),
        json!({"name":"AI Project","root":project_root.path()}),
    );
    let started = call(
        &mut f.service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    );
    let ai = started["session_token"].as_str().unwrap();
    for (operation, args) in [
        ("thread.pin", json!({"id":thread_id,"pinned":true})),
        ("conversation.pins", json!({})),
        ("conversation.activity", json!({})),
    ] {
        assert_eq!(
            request(&mut f.service, operation, Some(ai), args)
                .unwrap_err()
                .exit_code,
            4,
            "{operation}"
        );
    }
}

#[test]
fn v4_database_migrates_to_current_schema_without_losing_existing_conversation_data() {
    let mut f = fixture();
    let thread_id = f.thread["id"].as_str().unwrap().to_owned();
    let post = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":thread_id,"body":"Keep this post"}),
    );
    drop(f.service);
    let db = Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    db.execute_batch(
        "DROP TABLE notification_outbox;
         DROP TABLE notification_selection;
         DROP TABLE notification_post_reads;
         DROP TABLE notification_settings;
         DROP TABLE thread_subscriptions;
         DROP TABLE thread_pins;
         DROP TRIGGER posts_purged_body_immutable;
         DROP INDEX post_revisions_saved_at;
         ALTER TABLE posts DROP COLUMN body_purged_at;
         DROP TABLE retention_settings;
         ALTER TABLE threads DROP COLUMN archived_at;
         PRAGMA user_version=4;",
    )
    .unwrap();
    drop(db);

    let mut reopened = Service::open(f.data.path()).unwrap();
    assert_eq!(
        call(
            &mut reopened,
            "post.get",
            Some(&f.admin),
            json!({"thread_id":thread_id,"no":post["no"]}),
        )["body"],
        "Keep this post"
    );
    assert_eq!(
        call(
            &mut reopened,
            "thread.get",
            Some(&f.admin),
            json!({"id":thread_id})
        )["name"],
        "Topic"
    );
    let db = Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        9
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
}
