use chrono::{Duration, SecondsFormat, Utc};
use rusqlite::Connection;
use serde_json::{Value, json};
use std::path::Path;
use swarm_core::{Service, protocol::Request};

const RULES: &str = "# Swarm rules\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: activity-test\n    effect: allow\n    actions: [board.create, board.edit, thread.create, thread.edit, post.create, response.create, post.edit, post.history]\naccess:\n  boards: []\n  threads: []\n~~~\n";

#[test]
fn ai_confirmed_creates_replay_at_full_thread_without_bypassing_current_rules() {
    let mut f = fixture(None);
    let board = call(
        &mut f.service,
        "board.create",
        Some(&f.admin),
        json!({"name":"Boundary"}),
    );
    let thread = call(
        &mut f.service,
        "thread.create",
        Some(&f.admin),
        json!({"board_id":board["id"],"name":"Boundary"}),
    );
    let first_args = json!({"thread_id":thread["id"],"body":"First"});
    let first = request(
        &mut f.service,
        "post.create",
        Some(&f.token),
        first_args.clone(),
        Some("first-key"),
    )
    .unwrap();
    Connection::open(f.data.path().join("swarm.sqlite3"))
        .unwrap()
        .execute(
            "UPDATE threads SET next_no=1000 WHERE id=?1",
            [thread["id"].as_str().unwrap()],
        )
        .unwrap();
    let last_args = json!({"thread_id":thread["id"],"to":1,"body":"Last response"});
    let last = request(
        &mut f.service,
        "response.create",
        Some(&f.token),
        last_args.clone(),
        Some("last-key"),
    )
    .unwrap();
    assert_eq!(last["no"], 1000);
    for (operation, args, key, expected) in [
        ("post.create", first_args, "first-key", first),
        ("response.create", last_args, "last-key", last),
    ] {
        let replay = request(
            &mut f.service,
            operation,
            Some(&f.token),
            args.clone(),
            Some(key),
        )
        .unwrap();
        assert_eq!(replay["id"], expected["id"]);
        let mut changed = args;
        changed["body"] = json!("Different");
        assert_eq!(
            request(
                &mut f.service,
                operation,
                Some(&f.token),
                changed,
                Some(key)
            )
            .unwrap_err()
            .code,
            "REQUEST_CONFLICT"
        );
    }
    assert_eq!(
        request(
            &mut f.service,
            "post.create",
            Some(&f.token),
            json!({"thread_id":thread["id"],"body":"Overflow"}),
            Some("new-key")
        )
        .unwrap_err()
        .code,
        "THREAD_FULL"
    );
    std::fs::write(
        f.data.path().join("RULES.md"),
        RULES.replace("effect: allow", "effect: deny"),
    )
    .unwrap();
    assert_eq!(
        request(
            &mut f.service,
            "response.create",
            Some(&f.token),
            json!({"thread_id":thread["id"],"to":1,"body":"Last response"}),
            Some("last-key")
        )
        .unwrap_err()
        .code,
        "PERMISSION_DENIED"
    );
    let page = call(
        &mut f.service,
        "post.page",
        Some(&f.admin),
        json!({"thread_id":thread["id"],"page":1,"page_size":1000}),
    );
    // This isolated fixture allocated No.1000 directly; only two rows exist.
    assert_eq!(page["total_posts"], 2);
    let archived = call(
        &mut f.service,
        "thread.get",
        Some(&f.admin),
        json!({"id":thread["id"]}),
    );
    assert_eq!(archived["archived"], true);
}

fn request(
    service: &mut Service,
    operation: &str,
    auth: Option<&str>,
    args: Value,
    request_id: Option<&str>,
) -> Result<Value, swarm_core::protocol::Fault> {
    service.request(Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: request_id.map(str::to_owned),
    })
}

fn call(service: &mut Service, operation: &str, auth: Option<&str>, args: Value) -> Value {
    request(service, operation, auth, args, None).unwrap()
}

struct Fixture {
    data: tempfile::TempDir,
    _project_root: tempfile::TempDir,
    service: Service,
    admin: String,
    actor: String,
    token: String,
}

fn fixture(model: Option<&str>) -> Fixture {
    let data = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    std::fs::write(data.path().join("RULES.md"), RULES).unwrap();
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
    let project = call(
        &mut service,
        "project.register",
        Some(&admin),
        json!({"name":"Activity project","root":project_root.path()}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut args = json!({"project_id":project});
    if let Some(model) = model {
        args["model_name"] = json!(model);
    }
    let started = call(&mut service, "identity.start", None, args);
    Fixture {
        data,
        _project_root: project_root,
        service,
        admin,
        actor: started["identity"]["id"].as_str().unwrap().to_owned(),
        token: started["session_token"].as_str().unwrap().to_owned(),
    }
}

fn short_deadline(data_root: &Path, actor: &str) -> String {
    let value = (Utc::now() + Duration::hours(1)).to_rfc3339_opts(SecondsFormat::Millis, true);
    let db = Connection::open(data_root.join("swarm.sqlite3")).unwrap();
    db.execute(
        "UPDATE ai_identities SET expires_at=?1 WHERE id=?2",
        rusqlite::params![value, actor],
    )
    .unwrap();
    value
}

fn status(f: &mut Fixture) -> String {
    call(&mut f.service, "identity.status", Some(&f.token), json!({}))["expires_at"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn assert_renewed(f: &mut Fixture) {
    let expiry = chrono::DateTime::parse_from_rfc3339(&status(f)).unwrap();
    assert!(
        expiry > Utc::now() + Duration::days(6),
        "expiry was {expiry}"
    );
}

#[test]
fn successful_creation_and_own_post_edits_renew_the_seven_day_deadline() {
    let mut f = fixture(Some("TestModel"));

    short_deadline(f.data.path(), &f.actor);
    let board = call(
        &mut f.service,
        "board.create",
        Some(&f.token),
        json!({"name":"Board"}),
    );
    assert_renewed(&mut f);

    short_deadline(f.data.path(), &f.actor);
    let thread = call(
        &mut f.service,
        "thread.create",
        Some(&f.token),
        json!({"board_id":board["id"],"name":"Thread"}),
    );
    assert_renewed(&mut f);

    short_deadline(f.data.path(), &f.actor);
    let post = call(
        &mut f.service,
        "post.create",
        Some(&f.token),
        json!({"thread_id":thread["id"],"body":"Original"}),
    );
    assert_renewed(&mut f);

    short_deadline(f.data.path(), &f.actor);
    call(
        &mut f.service,
        "response.create",
        Some(&f.token),
        json!({"thread_id":thread["id"],"to":post["no"],"body":"Reply"}),
    );
    assert_renewed(&mut f);

    short_deadline(f.data.path(), &f.actor);
    call(
        &mut f.service,
        "post.edit",
        Some(&f.token),
        json!({"thread_id":thread["id"],"no":post["no"],"body":"Edited","if_revision":1}),
    );
    assert_renewed(&mut f);
}

#[test]
fn reads_model_supplementation_and_board_thread_edits_do_not_renew() {
    let mut f = fixture(None);
    let board = call(
        &mut f.service,
        "board.create",
        Some(&f.token),
        json!({"name":"Board"}),
    );
    let thread = call(
        &mut f.service,
        "thread.create",
        Some(&f.token),
        json!({"board_id":board["id"],"name":"Thread"}),
    );
    call(
        &mut f.service,
        "post.create",
        Some(&f.token),
        json!({"thread_id":thread["id"],"body":"Original"}),
    );
    call(
        &mut f.service,
        "post.edit",
        Some(&f.token),
        json!({"thread_id":thread["id"],"no":1,"body":"Edited","if_revision":1}),
    );

    let deadline = short_deadline(f.data.path(), &f.actor);
    call(
        &mut f.service,
        "identity.model",
        Some(&f.token),
        json!({"model_name":"TestModel"}),
    );
    call(
        &mut f.service,
        "board.edit",
        Some(&f.token),
        json!({"id":board["id"],"name":"Renamed board","if_revision":1}),
    );
    call(
        &mut f.service,
        "thread.edit",
        Some(&f.token),
        json!({"id":thread["id"],"name":"Renamed thread","if_revision":1}),
    );
    call(
        &mut f.service,
        "board.get",
        Some(&f.token),
        json!({"id":board["id"]}),
    );
    call(
        &mut f.service,
        "thread.get",
        Some(&f.token),
        json!({"id":thread["id"]}),
    );
    call(
        &mut f.service,
        "post.get",
        Some(&f.token),
        json!({"thread_id":thread["id"],"no":1}),
    );
    call(
        &mut f.service,
        "post.list",
        Some(&f.token),
        json!({"thread_id":thread["id"]}),
    );
    let history = call(
        &mut f.service,
        "post.history",
        Some(&f.token),
        json!({"thread_id":thread["id"],"no":1}),
    );
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
    assert_eq!(status(&mut f), deadline);
}

#[test]
fn failed_denied_and_human_edits_do_not_renew_the_ai_deadline() {
    let mut f = fixture(Some("TestModel"));
    let board = call(
        &mut f.service,
        "board.create",
        Some(&f.token),
        json!({"name":"Board"}),
    );
    let thread = call(
        &mut f.service,
        "thread.create",
        Some(&f.token),
        json!({"board_id":board["id"],"name":"Thread"}),
    );
    call(
        &mut f.service,
        "post.create",
        Some(&f.token),
        json!({"thread_id":thread["id"],"body":"Original"}),
    );
    call(
        &mut f.service,
        "post.edit",
        Some(&f.token),
        json!({"thread_id":thread["id"],"no":1,"body":"Version two","if_revision":1}),
    );

    let deadline = short_deadline(f.data.path(), &f.actor);
    let denied = request(
        &mut f.service,
        "board.delete",
        Some(&f.token),
        json!({"id":board["id"]}),
        None,
    )
    .unwrap_err();
    assert_eq!(denied.exit_code, 4);
    assert_eq!(status(&mut f), deadline);

    let invalid = request(
        &mut f.service,
        "post.create",
        Some(&f.token),
        json!({"thread_id":thread["id"],"body":"  "}),
        None,
    )
    .unwrap_err();
    assert_eq!(invalid.exit_code, 2);
    assert_eq!(status(&mut f), deadline);

    let conflict = request(
        &mut f.service,
        "post.edit",
        Some(&f.token),
        json!({"thread_id":thread["id"],"no":1,"body":"Stale edit","if_revision":1}),
        None,
    )
    .unwrap_err();
    assert_eq!(conflict.exit_code, 6);
    assert_eq!(status(&mut f), deadline);

    call(
        &mut f.service,
        "post.edit",
        Some(&f.admin),
        json!({"thread_id":thread["id"],"no":1,"body":"Human edit","if_revision":2}),
    );
    assert_eq!(status(&mut f), deadline);
}

#[test]
fn idempotent_create_replay_does_not_renew_or_emit_a_second_change() {
    let mut f = fixture(Some("TestModel"));
    let board = call(
        &mut f.service,
        "board.create",
        Some(&f.token),
        json!({"name":"Board"}),
    );
    let thread = call(
        &mut f.service,
        "thread.create",
        Some(&f.token),
        json!({"board_id":board["id"],"name":"Thread"}),
    );
    let args = json!({"thread_id":thread["id"],"body":"Once"});
    let first = request(
        &mut f.service,
        "post.create",
        Some(&f.token),
        args.clone(),
        Some("same-request"),
    )
    .unwrap();

    let db = Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    let before: i64 = db
        .query_row(
            "SELECT count(*) FROM changes WHERE operation='post.create'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(before, 1);
    let deadline = short_deadline(f.data.path(), &f.actor);
    let replay = request(
        &mut f.service,
        "post.create",
        Some(&f.token),
        args,
        Some("same-request"),
    )
    .unwrap();
    assert_eq!(replay["id"], first["id"]);
    assert_eq!(status(&mut f), deadline);
    let after: i64 = db
        .query_row(
            "SELECT count(*) FROM changes WHERE operation='post.create'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(after, before);
}
