use serde_json::{Value, json};
use std::path::Path;
use swarm_core::{
    Service,
    protocol::{Fault, Request},
};

fn request(
    service: &mut Service,
    operation: &str,
    auth: Option<&str>,
    args: Value,
    request_id: Option<&str>,
) -> Result<Value, Fault> {
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

fn write_rules(data_root: &Path, rules: &str, access: &str) {
    std::fs::write(
        data_root.join("RULES.md"),
        format!(
            "# Swarm rules\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n{rules}\naccess:\n{access}\n~~~\n"
        ),
    )
    .unwrap();
}

fn allow_all_writes() -> &'static str {
    "  - id: allow-tests\n    effect: allow\n    actions: [board.create, board.edit, thread.create, thread.edit, post.create, post.edit, post.history]"
}

struct Fixture {
    data: tempfile::TempDir,
    project_roots: Vec<tempfile::TempDir>,
    service: Service,
    admin: String,
    project_id: String,
    token: String,
}

fn fixture() -> Fixture {
    let data = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    write_rules(
        data.path(),
        allow_all_writes(),
        "  boards: []\n  threads: []",
    );
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
        json!({"name":"Policy edge project","root":project_root.path()}),
    );
    let started = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    );
    Fixture {
        data,
        project_roots: vec![project_root],
        service,
        admin,
        project_id: project["id"].as_str().unwrap().to_owned(),
        token: started["session_token"].as_str().unwrap().to_owned(),
    }
}

fn admin_board(f: &mut Fixture) -> String {
    call(
        &mut f.service,
        "board.create",
        Some(&f.admin),
        json!({"name":"Admin board"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn create_receipt_replay_hides_soft_deleted_board_and_thread() {
    let mut f = fixture();
    let board_args = json!({"name":"AI board receipt"});
    let board = request(
        &mut f.service,
        "board.create",
        Some(&f.token),
        board_args.clone(),
        Some("board-create-1"),
    )
    .unwrap();
    call(
        &mut f.service,
        "board.delete",
        Some(&f.admin),
        json!({"id":board["id"]}),
    );
    let replay = request(
        &mut f.service,
        "board.create",
        Some(&f.token),
        board_args,
        Some("board-create-1"),
    )
    .unwrap_err();
    assert_eq!(replay.exit_code, 5);
    assert!(!replay.message.contains("AI board receipt"));

    let parent = admin_board(&mut f);
    let thread_args = json!({"board_id":parent,"name":"AI thread receipt"});
    let thread = request(
        &mut f.service,
        "thread.create",
        Some(&f.token),
        thread_args.clone(),
        Some("thread-create-1"),
    )
    .unwrap();
    call(
        &mut f.service,
        "thread.delete",
        Some(&f.admin),
        json!({"id":thread["id"]}),
    );
    let replay = request(
        &mut f.service,
        "thread.create",
        Some(&f.token),
        thread_args,
        Some("thread-create-1"),
    )
    .unwrap_err();
    assert_eq!(replay.exit_code, 5);
    assert!(!replay.message.contains("AI thread receipt"));
}

#[test]
fn create_receipt_replay_rechecks_current_board_and_thread_read_access() {
    let mut f = fixture();
    let board_args = json!({"name":"ACL-hidden board"});
    let board = request(
        &mut f.service,
        "board.create",
        Some(&f.token),
        board_args.clone(),
        Some("acl-board-create"),
    )
    .unwrap();
    let board_access = format!(
        "  boards:\n    - id: {}\n      projects:\n        {}:\n          read: deny\n  threads: []",
        board["id"], f.project_id
    );
    write_rules(f.data.path(), allow_all_writes(), &board_access);
    let replay = request(
        &mut f.service,
        "board.create",
        Some(&f.token),
        board_args,
        Some("acl-board-create"),
    )
    .unwrap_err();
    assert_eq!(replay.exit_code, 4);
    assert!(!replay.message.contains("ACL-hidden board"));

    let parent = admin_board(&mut f);
    let thread_args = json!({"board_id":parent,"name":"ACL-hidden thread"});
    let thread = request(
        &mut f.service,
        "thread.create",
        Some(&f.token),
        thread_args.clone(),
        Some("acl-thread-create"),
    )
    .unwrap();
    let thread_access = format!(
        "  boards: []\n  threads:\n    - id: {}\n      projects:\n        {}:\n          read: deny",
        thread["id"], f.project_id
    );
    write_rules(f.data.path(), allow_all_writes(), &thread_access);
    let replay = request(
        &mut f.service,
        "thread.create",
        Some(&f.token),
        thread_args,
        Some("acl-thread-create"),
    )
    .unwrap_err();
    assert_eq!(replay.exit_code, 4);
    assert!(!replay.message.contains("ACL-hidden thread"));
}

#[test]
fn missing_global_rules_remains_missing_and_denies_ai_after_reopen() {
    let f = fixture();
    std::fs::remove_file(f.data.path().join("RULES.md")).unwrap();
    drop(f.service);

    let mut reopened = Service::open(f.data.path()).unwrap();
    assert!(!f.data.path().join("RULES.md").exists());
    let denied = request(
        &mut reopened,
        "identity.status",
        Some(&f.token),
        json!({}),
        None,
    )
    .unwrap_err();
    assert_eq!(denied.code, "RULES_INVALID");
    assert!(!f.data.path().join("RULES.md").exists());
}

#[test]
fn unavailable_canonical_root_blocks_only_its_project_identity() {
    let mut f = fixture();
    let other_root = tempfile::tempdir().unwrap();
    let other_project = call(
        &mut f.service,
        "project.register",
        Some(&f.admin),
        json!({"name":"Other project","root":other_root.path()}),
    );
    let other_started = call(
        &mut f.service,
        "identity.start",
        None,
        json!({"project_id":other_project["id"]}),
    );
    let other_token = other_started["session_token"].as_str().unwrap();

    f.project_roots.pop().unwrap().close().unwrap();
    let unavailable = request(
        &mut f.service,
        "identity.status",
        Some(&f.token),
        json!({}),
        None,
    )
    .unwrap_err();
    assert_eq!(unavailable.code, "PROJECT_ROOT_UNAVAILABLE");
    let available = call(
        &mut f.service,
        "identity.status",
        Some(other_token),
        json!({}),
    );
    assert_eq!(available["project_id"], other_project["id"]);
}

#[test]
fn post_edit_keeps_original_author_model_and_records_editor_model_snapshot() {
    let mut f = fixture();
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
    let created = call(
        &mut f.service,
        "post.create",
        Some(&f.token),
        json!({"thread_id":thread["id"],"body":"Original"}),
    );
    assert_eq!(created["author"]["model_name"], "不明");
    call(
        &mut f.service,
        "identity.model",
        Some(&f.token),
        json!({"model_name":"DeclaredModel"}),
    );
    let edited = call(
        &mut f.service,
        "post.edit",
        Some(&f.token),
        json!({"thread_id":thread["id"],"no":1,"body":"Edited","if_revision":1}),
    );
    assert_eq!(edited["author"]["model_name"], "不明");
    assert_eq!(edited["editor_model_name"], "DeclaredModel");

    let history = call(
        &mut f.service,
        "post.history",
        Some(&f.token),
        json!({"thread_id":thread["id"],"no":1}),
    );
    assert_eq!(history["items"][0]["editor_model_name"], "DeclaredModel");
}

#[test]
fn ai_cannot_edit_board_or_thread_created_by_another_identity() {
    let mut f = fixture();
    let board = call(
        &mut f.service,
        "board.create",
        Some(&f.token),
        json!({"name":"Owned board"}),
    );
    let thread = call(
        &mut f.service,
        "thread.create",
        Some(&f.token),
        json!({"board_id":board["id"],"name":"Owned thread"}),
    );
    let second = call(
        &mut f.service,
        "identity.start",
        None,
        json!({"project_id":f.project_id}),
    );
    let other_token = second["session_token"].as_str().unwrap();
    let board_edit = request(
        &mut f.service,
        "board.edit",
        Some(other_token),
        json!({"id":board["id"],"name":"Stolen board","if_revision":1}),
        None,
    )
    .unwrap_err();
    assert_eq!(board_edit.exit_code, 4);
    let thread_edit = request(
        &mut f.service,
        "thread.edit",
        Some(other_token),
        json!({"id":thread["id"],"name":"Stolen thread","if_revision":1}),
        None,
    )
    .unwrap_err();
    assert_eq!(thread_edit.exit_code, 4);
}
