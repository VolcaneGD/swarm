use rusqlite::Connection;
use serde_json::{Value, json};
use swarm_core::{Service, changes_wait_timeout, protocol::Request};

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
    board: String,
    thread: String,
}

fn fixture() -> Fixture {
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Changes Admin","password":"changes-admin-fixture"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let board = call(
        &mut service,
        "board.create",
        Some(&admin),
        json!({"name":"Changes board"}),
    );
    let thread = call(
        &mut service,
        "thread.create",
        Some(&admin),
        json!({"board_id":board["id"],"name":"Changes thread"}),
    );
    Fixture {
        data,
        service,
        admin,
        board: board["id"].as_str().unwrap().to_owned(),
        thread: thread["id"].as_str().unwrap().to_owned(),
    }
}

fn bootstrap(f: &mut Fixture, auth: &str, scope: Value) -> String {
    let result = call(&mut f.service, "changes.get", Some(auth), scope);
    assert_eq!(result["items"], json!([]));
    result["change_cursor"].as_str().unwrap().to_owned()
}

fn account(f: &mut Fixture, name: &str) -> (String, String) {
    let created = call(
        &mut f.service,
        "account.create",
        Some(&f.admin),
        json!({"name":name,"password":format!("{name}-fixture-password")}),
    );
    let login = call(
        &mut f.service,
        "account.login",
        None,
        json!({"name":name,"password":format!("{name}-fixture-password")}),
    );
    (
        created["id"].as_str().unwrap().to_owned(),
        login["token"].as_str().unwrap().to_owned(),
    )
}

fn grant(
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

fn request_changes(
    f: &mut Fixture,
    auth: &str,
    cursor: &str,
    scope: Value,
) -> Result<Value, swarm_core::protocol::Fault> {
    let mut args = scope.as_object().cloned().unwrap_or_default();
    args.insert("after".into(), json!(cursor));
    request(
        &mut f.service,
        "changes.get",
        Some(auth),
        Value::Object(args),
    )
}

#[test]
fn changes_bootstrap_returns_an_opaque_stable_cursor_and_polls_body_free_events() {
    let mut f = fixture();
    let admin = f.admin.clone();
    let initial = bootstrap(&mut f, &admin, json!({}));
    assert_eq!(initial.len(), 64);
    assert!(initial.bytes().all(|byte| byte.is_ascii_hexdigit()));

    let post = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"private body must never appear in changes"}),
    );
    let fetched = request_changes(&mut f, &admin, &initial, json!({})).unwrap();
    assert_eq!(fetched["items"].as_array().unwrap().len(), 1);
    let item = &fetched["items"][0];
    assert_eq!(item["operation"], "post.create");
    assert_eq!(item["entity_id"], post["id"]);
    assert_eq!(item["board_id"], f.board);
    assert_eq!(item["thread_id"], f.thread);
    assert_eq!(item["no"], 1);
    assert!(item.get("body").is_none());
    assert!(item.get("name").is_none());
    assert_eq!(fetched["has_more"], false);

    let stable = request_changes(
        &mut f,
        &admin,
        fetched["change_cursor"].as_str().unwrap(),
        json!({}),
    )
    .unwrap();
    assert_eq!(stable["items"], json!([]));
    assert_eq!(stable["change_cursor"], fetched["change_cursor"]);
}

#[test]
fn changes_events_include_edit_delete_restore_and_pagination() {
    let mut f = fixture();
    let admin = f.admin.clone();
    let post = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"initial"}),
    );
    let cursor = bootstrap(&mut f, &admin, json!({}));
    call(
        &mut f.service,
        "post.edit",
        Some(&f.admin),
        json!({"thread_id":f.thread,"no":post["no"],"body":"edited","if_revision":1}),
    );
    call(
        &mut f.service,
        "post.delete",
        Some(&f.admin),
        json!({"thread_id":f.thread,"no":post["no"]}),
    );
    call(
        &mut f.service,
        "post.restore",
        Some(&f.admin),
        json!({"thread_id":f.thread,"no":post["no"]}),
    );
    let first = request_changes(&mut f, &admin, &cursor, json!({"limit":1})).unwrap();
    assert_eq!(first["items"][0]["operation"], "post.edit");
    assert_eq!(first["has_more"], true);
    let second = request_changes(
        &mut f,
        &admin,
        first["change_cursor"].as_str().unwrap(),
        json!({"limit":1}),
    )
    .unwrap();
    assert_eq!(second["items"][0]["operation"], "post.delete");
    let third = request_changes(
        &mut f,
        &admin,
        second["change_cursor"].as_str().unwrap(),
        json!({"limit":1}),
    )
    .unwrap();
    assert_eq!(third["items"][0]["operation"], "post.restore");
    assert_eq!(third["has_more"], false);
}

#[test]
fn post_tombstone_events_are_body_free_and_visible_with_live_thread_read_access() {
    let mut f = fixture();
    let admin = f.admin.clone();
    let thread_id = f.thread.clone();
    let post = call(
        &mut f.service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread_id,"body":"secret deleted body"}),
    );
    let cursor = bootstrap(&mut f, &admin, json!({"thread_id":thread_id}));
    call(
        &mut f.service,
        "post.delete",
        Some(&admin),
        json!({"thread_id":thread_id,"no":post["no"]}),
    );

    let deleted = request_changes(&mut f, &admin, &cursor, json!({"thread_id":thread_id})).unwrap();
    assert_eq!(deleted["items"].as_array().unwrap().len(), 1);
    assert_eq!(deleted["items"][0]["operation"], "post.delete");
    assert_eq!(deleted["items"][0]["entity_id"], post["id"]);
    assert_eq!(deleted["items"][0]["no"], post["no"]);
    assert!(deleted["items"][0].get("body").is_none());
}

#[test]
fn changes_filter_rechecks_visibility_and_hides_private_backlog() {
    let mut f = fixture();
    let hidden_board = call(
        &mut f.service,
        "board.create",
        Some(&f.admin),
        json!({"name":"Private changes board"}),
    );
    let hidden_thread = call(
        &mut f.service,
        "thread.create",
        Some(&f.admin),
        json!({"board_id":hidden_board["id"],"name":"Private changes thread"}),
    );
    let (reader_id, reader) = account(&mut f, "Changes Reader");
    for operation in [
        "changes.get",
        "board.list",
        "board.get",
        "thread.list",
        "thread.get",
        "post.list",
        "post.get",
    ] {
        grant(&mut f, &reader_id, operation, "global", "", "allow");
    }
    grant(
        &mut f,
        &reader_id,
        "read",
        "board",
        hidden_board["id"].as_str().unwrap(),
        "deny",
    );
    let cursor = bootstrap(&mut f, &reader, json!({}));

    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"visible"}),
    );
    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":hidden_thread["id"],"body":"hidden"}),
    );
    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"visible two"}),
    );
    let first = request_changes(&mut f, &reader, &cursor, json!({"limit":1})).unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(first["items"][0]["thread_id"], f.thread);
    assert_eq!(first["has_more"], true);
    let second = request_changes(
        &mut f,
        &reader,
        first["change_cursor"].as_str().unwrap(),
        json!({"limit":1}),
    )
    .unwrap();
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["items"][0]["thread_id"], f.thread);
    assert_eq!(second["has_more"], false);

    let inaccessible = request(
        &mut f.service,
        "changes.get",
        Some(&reader),
        json!({"thread_id":hidden_thread["id"]}),
    )
    .unwrap_err();
    assert_eq!(inaccessible.exit_code, 5);
    assert_eq!(
        request(
            &mut f.service,
            "changes.get",
            Some(&reader),
            json!({"thread_id":"missing-thread"}),
        )
        .unwrap_err()
        .exit_code,
        5
    );
}

#[test]
fn changes_cursors_reject_malformed_tampered_foreign_and_changed_access_states() {
    let mut f = fixture();
    let (reader_id, reader) = account(&mut f, "Cursor Reader");
    for operation in [
        "changes.get",
        "board.list",
        "board.get",
        "thread.list",
        "thread.get",
    ] {
        grant(&mut f, &reader_id, operation, "global", "", "allow");
    }
    let cursor = bootstrap(&mut f, &reader, json!({}));
    assert_eq!(
        request(
            &mut f.service,
            "changes.get",
            Some(&reader),
            json!({"after":"not-a-token"}),
        )
        .unwrap_err()
        .exit_code,
        2
    );
    let mut tampered = cursor.clone().into_bytes();
    tampered[0] = if tampered[0] == b'0' { b'1' } else { b'0' };
    let tampered = String::from_utf8(tampered).unwrap();
    assert_eq!(
        request_changes(&mut f, &reader, &tampered, json!({}))
            .unwrap_err()
            .code,
        "RESYNC_REQUIRED"
    );
    let (_, other) = account(&mut f, "Different Cursor Principal");
    assert_eq!(
        request_changes(&mut f, &other, &cursor, json!({}))
            .unwrap_err()
            .code,
        "RESYNC_REQUIRED"
    );

    grant(&mut f, &reader_id, "post.get", "global", "", "allow");
    assert_eq!(
        request_changes(&mut f, &reader, &cursor, json!({}))
            .unwrap_err()
            .code,
        "RESYNC_REQUIRED",
        "permission changes invalidate the old visibility snapshot"
    );
}

#[test]
fn changes_cursor_from_another_database_and_explicit_invalidation_require_resync() {
    let mut first = fixture();
    let first_admin = first.admin.clone();
    let cursor = bootstrap(&mut first, &first_admin, json!({}));
    let mut second = fixture();
    let second_admin = second.admin.clone();
    assert_eq!(
        request_changes(&mut second, &second_admin, &cursor, json!({}))
            .unwrap_err()
            .code,
        "RESYNC_REQUIRED"
    );
    first.service.invalidate_change_cursors().unwrap();
    assert_eq!(
        request_changes(&mut first, &first_admin, &cursor, json!({}))
            .unwrap_err()
            .code,
        "RESYNC_REQUIRED"
    );
}

#[test]
fn changes_wait_timeout_is_bounded_and_reads_do_not_renew_ai_identity() {
    let mut f = fixture();
    let project_root = tempfile::tempdir().unwrap();
    let project = call(
        &mut f.service,
        "project.register",
        Some(&f.admin),
        json!({"name":"Changes project","root":project_root.path()}),
    );
    let grant = call(
        &mut f.service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    );
    let token = grant["session_token"].as_str().unwrap();
    let ai_id = grant["identity"]["id"].as_str().unwrap();
    let before: String = Connection::open(f.data.path().join("swarm.sqlite3"))
        .unwrap()
        .query_row(
            "SELECT expires_at FROM ai_identities WHERE id=?1",
            [ai_id],
            |row| row.get(0),
        )
        .unwrap();
    let listed_boards = call(
        &mut f.service,
        "board.list",
        Some(token),
        json!({"limit":1}),
    );
    assert_eq!(listed_boards["items"].as_array().unwrap().len(), 1);
    let listed_threads = call(
        &mut f.service,
        "thread.list",
        Some(token),
        json!({"board_id":f.board,"limit":1}),
    );
    assert_eq!(listed_threads["items"].as_array().unwrap().len(), 1);
    let get_bootstrap = call(&mut f.service, "changes.get", Some(token), json!({}));
    let bootstrap = call(
        &mut f.service,
        "changes.wait",
        Some(token),
        json!({"after":get_bootstrap["change_cursor"],"timeout":300}),
    );
    assert_eq!(bootstrap["items"], json!([]));
    assert_eq!(
        bootstrap["change_cursor"], get_bootstrap["change_cursor"],
        "get and wait share the same opaque position"
    );
    let get_after_wait = call(
        &mut f.service,
        "changes.get",
        Some(token),
        json!({"after":bootstrap["change_cursor"]}),
    );
    assert_eq!(get_after_wait["items"], json!([]));
    assert_eq!(get_after_wait["change_cursor"], bootstrap["change_cursor"]);
    assert!(
        !bootstrap["change_cursor"]
            .as_str()
            .unwrap()
            .contains(&before)
    );
    let stable = call(
        &mut f.service,
        "changes.wait",
        Some(token),
        json!({"after":bootstrap["change_cursor"],"timeout":1}),
    );
    assert_eq!(stable["items"], json!([]));
    assert_eq!(stable["change_cursor"], bootstrap["change_cursor"]);
    let after: String = Connection::open(f.data.path().join("swarm.sqlite3"))
        .unwrap()
        .query_row(
            "SELECT expires_at FROM ai_identities WHERE id=?1",
            [ai_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(before, after, "polling does not renew the identity TTL");

    for (timeout, expected) in [(0, false), (301, false), (60, true)] {
        let request = Request {
            operation: "changes.wait".into(),
            auth: Some(token.to_owned()),
            args: json!({"timeout":timeout}),
            context: json!({}),
            request_id: None,
        };
        assert_eq!(changes_wait_timeout(&request).is_ok(), expected);
    }
}

#[test]
fn changes_rejects_conflicting_filters_and_limit_out_of_range() {
    let mut f = fixture();
    for args in [
        json!({"board_id":f.board,"thread_id":f.thread}),
        json!({"limit":0}),
        json!({"limit":201}),
    ] {
        assert_eq!(
            request(&mut f.service, "changes.get", Some(&f.admin), args)
                .unwrap_err()
                .exit_code,
            2
        );
    }
    let scoped = call(
        &mut f.service,
        "changes.get",
        Some(&f.admin),
        json!({"board_id":f.board,"limit":200}),
    );
    assert_eq!(scoped["items"], json!([]));
}

#[test]
fn changes_scans_past_multiple_batches_of_hidden_rows_without_losing_visible_events() {
    let mut f = fixture();
    let hidden_board = call(
        &mut f.service,
        "board.create",
        Some(&f.admin),
        json!({"name":"Hidden scan board"}),
    );
    let (reader_id, reader) = account(&mut f, "Long Scan Reader");
    for operation in [
        "changes.get",
        "board.list",
        "board.get",
        "thread.list",
        "thread.get",
        "post.list",
        "post.get",
    ] {
        grant(&mut f, &reader_id, operation, "global", "", "allow");
    }
    grant(
        &mut f,
        &reader_id,
        "read",
        "board",
        hidden_board["id"].as_str().unwrap(),
        "deny",
    );
    let cursor = bootstrap(&mut f, &reader, json!({}));

    let db = Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    let actor_id: String = db
        .query_row(
            "SELECT id FROM accounts WHERE name='Changes Admin'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    for _ in 0..1001 {
        db.execute(
            "INSERT INTO changes(operation,entity_id,actor_id,created_at) VALUES('board.create',?1,?2,'2026-10-09T00:00:00Z')",
            rusqlite::params![hidden_board["id"].as_str().unwrap(), actor_id],
        )
        .unwrap();
    }
    drop(db);
    let visible_post = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"visible after hidden batches"}),
    );

    let result = request_changes(&mut f, &reader, &cursor, json!({"limit":1})).unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    assert_eq!(result["items"][0]["entity_id"], visible_post["id"]);
    assert_eq!(result["has_more"], false);
}

#[test]
fn changes_wait_requires_its_own_grant_even_when_no_events_exist() {
    let mut f = fixture();
    let (reader_id, reader) = account(&mut f, "Get Only Reader");
    for operation in [
        "changes.get",
        "board.list",
        "board.get",
        "thread.list",
        "thread.get",
    ] {
        grant(&mut f, &reader_id, operation, "global", "", "allow");
    }
    assert!(request(&mut f.service, "changes.get", Some(&reader), json!({})).is_ok());
    let wait = request(
        &mut f.service,
        "changes.wait",
        Some(&reader),
        json!({"timeout":1}),
    )
    .unwrap_err();
    assert_eq!(wait.exit_code, 4);
}

#[test]
fn human_change_feed_exposes_deleted_board_and_thread_metadata_only_with_restore_access() {
    let mut f = fixture();
    let admin = f.admin.clone();
    let second_board = call(
        &mut f.service,
        "board.create",
        Some(&admin),
        json!({"name":"Deleted metadata board"}),
    );
    let second_thread = call(
        &mut f.service,
        "thread.create",
        Some(&admin),
        json!({"board_id":second_board["id"],"name":"Deleted metadata thread"}),
    );
    let primary_board = f.board.clone();
    let (reader_id, reader) = account(&mut f, "Tombstone Reader");
    for operation in [
        "changes.get",
        "board.list",
        "board.get",
        "thread.list",
        "thread.get",
    ] {
        grant(&mut f, &reader_id, operation, "global", "", "allow");
    }
    for operation in ["board.restore", "thread.restore"] {
        grant(&mut f, &reader_id, operation, "global", "", "deny");
    }
    let reader_cursor = bootstrap(&mut f, &reader, json!({}));
    let reader_board_cursor = bootstrap(&mut f, &reader, json!({"board_id":primary_board}));
    let reader_thread_cursor = bootstrap(&mut f, &reader, json!({"thread_id":second_thread["id"]}));
    let board_cursor = bootstrap(&mut f, &admin, json!({"board_id":primary_board}));
    let thread_cursor = bootstrap(&mut f, &admin, json!({"thread_id":second_thread["id"]}));
    let all_cursor = bootstrap(&mut f, &admin, json!({}));

    call(
        &mut f.service,
        "thread.delete",
        Some(&admin),
        json!({"id":second_thread["id"]}),
    );
    call(
        &mut f.service,
        "board.delete",
        Some(&admin),
        json!({"id":primary_board}),
    );

    let thread_event = request_changes(
        &mut f,
        &admin,
        &thread_cursor,
        json!({"thread_id":second_thread["id"]}),
    )
    .unwrap();
    assert_eq!(thread_event["items"][0]["operation"], "thread.delete");
    assert_eq!(thread_event["items"][0]["thread_id"], second_thread["id"]);

    let board_event = request_changes(
        &mut f,
        &admin,
        &board_cursor,
        json!({"board_id":primary_board}),
    )
    .unwrap();
    assert_eq!(board_event["items"][0]["operation"], "board.delete");
    assert_eq!(board_event["items"][0]["board_id"], primary_board);

    let all_events = request_changes(&mut f, &admin, &all_cursor, json!({})).unwrap();
    let hidden = request_changes(&mut f, &reader, &reader_cursor, json!({})).unwrap();
    assert_eq!(hidden["items"], json!([]));
    assert_eq!(hidden["has_more"], false);
    for (cursor, scope) in [
        (reader_board_cursor, json!({"board_id":primary_board})),
        (
            reader_thread_cursor,
            json!({"thread_id":second_thread["id"]}),
        ),
    ] {
        assert_eq!(
            request_changes(&mut f, &reader, &cursor, scope)
                .unwrap_err()
                .code,
            "NOT_FOUND"
        );
    }
    assert!(all_events["items"].as_array().unwrap().iter().any(|item| {
        item["operation"] == "thread.delete" && item["thread_id"] == second_thread["id"]
    }));
    assert!(
        all_events["items"].as_array().unwrap().iter().any(|item| {
            item["operation"] == "board.delete" && item["board_id"] == primary_board
        })
    );
    assert!(
        all_events["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| { item.get("body").is_none() && item.get("name").is_none() })
    );
}

#[test]
fn ai_change_wait_and_rules_check_require_an_underlying_readable_scope() {
    const RULES: &str = "# AI changes fixture\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: false\nrules:\n  - id: setup\n    effect: allow\n    actions: [project.register, rules.check, changes.wait]\naccess:\n  boards: []\n  threads: []\n~~~\n";
    let data = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    std::fs::write(data.path().join("RULES.md"), RULES).unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"AI Changes Admin","password":"ai-changes-admin-fixture"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let _board = call(
        &mut service,
        "board.create",
        Some(&admin),
        json!({"name":"AI changes board"}),
    );
    let project = call(
        &mut service,
        "project.register",
        Some(&admin),
        json!({"name":"AI changes project","root":project_root.path()}),
    );
    let started = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    );
    let token = started["session_token"].as_str().unwrap();

    let actual = request(
        &mut service,
        "changes.wait",
        Some(token),
        json!({"timeout":1}),
    )
    .unwrap_err();
    assert_eq!(actual.exit_code, 4);
    let diagnostic = call(
        &mut service,
        "rules.check",
        Some(token),
        json!({"operation":"changes.wait"}),
    );
    assert_eq!(diagnostic["allowed"], false);
    assert_eq!(diagnostic["reason"], "not_allowed");
}

#[test]
fn ai_change_feed_does_not_expose_deleted_threads() {
    const RULES: &str = "# AI tombstone fixture\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: registration\n    effect: allow\n    actions: [project.register]\naccess:\n  boards: []\n  threads: []\n~~~\n";
    let data = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    std::fs::write(data.path().join("RULES.md"), RULES).unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"AI Tombstone Admin","password":"ai-tombstone-admin-fixture"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let board = call(
        &mut service,
        "board.create",
        Some(&admin),
        json!({"name":"AI tombstone board"}),
    );
    let thread = call(
        &mut service,
        "thread.create",
        Some(&admin),
        json!({"board_id":board["id"],"name":"AI tombstone thread"}),
    );
    let project = call(
        &mut service,
        "project.register",
        Some(&admin),
        json!({"name":"AI tombstone project","root":project_root.path()}),
    );
    let started = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    );
    let token = started["session_token"].as_str().unwrap();
    let thread_cursor = call(
        &mut service,
        "changes.get",
        Some(token),
        json!({"thread_id":thread["id"]}),
    )["change_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    let all_cursor = call(&mut service, "changes.get", Some(token), json!({}))["change_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    call(
        &mut service,
        "thread.delete",
        Some(&admin),
        json!({"id":thread["id"]}),
    );
    assert_eq!(
        request(
            &mut service,
            "changes.get",
            Some(token),
            json!({"thread_id":thread["id"],"after":thread_cursor}),
        )
        .unwrap_err()
        .exit_code,
        5
    );
    let global = call(
        &mut service,
        "changes.get",
        Some(token),
        json!({"after":all_cursor}),
    );
    assert!(global["items"].as_array().unwrap().is_empty());
}
