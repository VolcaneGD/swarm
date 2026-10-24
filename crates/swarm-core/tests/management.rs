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
    _data: tempfile::TempDir,
    service: Service,
    admin: String,
    board: Value,
    thread: Value,
}

fn fixture() -> Fixture {
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"admin-password"}),
    )
    .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let board = call(
        &mut service,
        "board.create",
        Some(&admin),
        json!({"name":"Board"}),
    )
    .unwrap();
    let thread = call(
        &mut service,
        "thread.create",
        Some(&admin),
        json!({"board_id":board["id"],"name":"Thread"}),
    )
    .unwrap();
    Fixture {
        _data: data,
        service,
        admin,
        board,
        thread,
    }
}

fn deleted(f: &mut Fixture, token: &str) -> Value {
    call(
        &mut f.service,
        "conversation.deleted",
        Some(token),
        json!({}),
    )
    .unwrap()
}

fn permission(
    f: &mut Fixture,
    account_id: &str,
    operation: &str,
    scope: &str,
    target: &str,
    decision: &str,
) {
    call(
        &mut f.service,
        "permission.set",
        Some(&f.admin),
        json!({"subject_type":"account","subject_id":account_id,"operation":operation,
            "scope":scope,"target_id":target,"decision":decision}),
    )
    .unwrap();
}

#[test]
fn deleted_listing_distinguishes_individual_deletion_from_deleted_parent() {
    let mut f = fixture();
    let thread_id = f.thread["id"].as_str().unwrap().to_owned();
    let board_id = f.board["id"].as_str().unwrap().to_owned();
    call(
        &mut f.service,
        "thread.delete",
        Some(&f.admin),
        json!({"id":thread_id}),
    )
    .unwrap();
    call(
        &mut f.service,
        "board.delete",
        Some(&f.admin),
        json!({"id":board_id}),
    )
    .unwrap();

    let admin = f.admin.clone();
    let result = deleted(&mut f, &admin);
    assert_eq!(result["boards"].as_array().unwrap().len(), 1);
    assert_eq!(result["boards"][0]["id"], board_id);
    assert_eq!(result["threads"].as_array().unwrap().len(), 1);
    assert_eq!(result["threads"][0]["id"], thread_id);
    assert_eq!(result["threads"][0]["parent_deleted"], true);
    assert_eq!(result["threads"][0]["can_restore"], true);
    assert!(result["boards"][0]["deleted_at"].as_str().is_some());

    call(
        &mut f.service,
        "board.restore",
        Some(&f.admin),
        json!({"id":board_id}),
    )
    .unwrap();
    let result = deleted(&mut f, &admin);
    assert!(result["boards"].as_array().unwrap().is_empty());
    assert_eq!(result["threads"].as_array().unwrap().len(), 1);
    assert_eq!(result["threads"][0]["id"], thread_id);
    assert_eq!(result["threads"][0]["board_id"], board_id);
}

#[test]
fn deleted_listing_requires_current_view_and_restore_permissions() {
    let mut f = fixture();
    let account = call(
        &mut f.service,
        "account.create",
        Some(&f.admin),
        json!({"name":"Member","password":"member-password"}),
    )
    .unwrap();
    let member = call(
        &mut f.service,
        "account.login",
        None,
        json!({"name":"Member","password":"member-password"}),
    )
    .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let board_id = f.board["id"].as_str().unwrap().to_owned();
    let thread_id = f.thread["id"].as_str().unwrap().to_owned();
    call(
        &mut f.service,
        "board.delete",
        Some(&f.admin),
        json!({"id":board_id}),
    )
    .unwrap();
    assert!(
        deleted(&mut f, &member)["boards"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    permission(
        &mut f,
        account["id"].as_str().unwrap(),
        "board.restore",
        "global",
        "",
        "allow",
    );
    assert_eq!(
        deleted(&mut f, &member)["boards"].as_array().unwrap().len(),
        1
    );
    permission(
        &mut f,
        account["id"].as_str().unwrap(),
        "board.restore",
        "global",
        "",
        "deny",
    );
    assert!(
        deleted(&mut f, &member)["boards"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    call(
        &mut f.service,
        "board.restore",
        Some(&f.admin),
        json!({"id":board_id}),
    )
    .unwrap();
    call(
        &mut f.service,
        "thread.delete",
        Some(&f.admin),
        json!({"id":thread_id}),
    )
    .unwrap();
    permission(
        &mut f,
        account["id"].as_str().unwrap(),
        "thread.restore",
        "thread",
        &thread_id,
        "allow",
    );
    permission(
        &mut f,
        account["id"].as_str().unwrap(),
        "read",
        "board",
        &board_id,
        "deny",
    );
    permission(
        &mut f,
        account["id"].as_str().unwrap(),
        "read",
        "thread",
        &thread_id,
        "allow",
    );
    let visible = deleted(&mut f, &member);
    assert_eq!(visible["threads"].as_array().unwrap().len(), 1);
    assert!(visible["threads"][0]["board_id"].is_null());
    permission(
        &mut f,
        account["id"].as_str().unwrap(),
        "thread.restore",
        "thread",
        &thread_id,
        "deny",
    );
    assert!(
        deleted(&mut f, &member)["threads"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn capabilities_add_delete_and_restore_actions_and_recompute_revocation() {
    let mut f = fixture();
    let account = call(
        &mut f.service,
        "account.create",
        Some(&f.admin),
        json!({"name":"Member","password":"member-password"}),
    )
    .unwrap();
    let member = call(
        &mut f.service,
        "account.login",
        None,
        json!({"name":"Member","password":"member-password"}),
    )
    .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let board_id = f.board["id"].as_str().unwrap().to_owned();
    let thread_id = f.thread["id"].as_str().unwrap().to_owned();
    let result = call(
        &mut f.service,
        "conversation.capabilities",
        Some(&member),
        json!({}),
    )
    .unwrap();
    assert_eq!(result["boards"][0]["board_delete"], false);
    assert_eq!(result["threads"][0]["thread_delete"], false);
    assert_eq!(result["threads"][0]["post_delete"], false);
    assert_eq!(result["threads"][0]["post_restore"], false);

    permission(
        &mut f,
        account["id"].as_str().unwrap(),
        "board.delete",
        "board",
        &board_id,
        "allow",
    );
    permission(
        &mut f,
        account["id"].as_str().unwrap(),
        "thread.delete",
        "thread",
        &thread_id,
        "allow",
    );
    permission(
        &mut f,
        account["id"].as_str().unwrap(),
        "post.delete",
        "thread",
        &thread_id,
        "allow",
    );
    permission(
        &mut f,
        account["id"].as_str().unwrap(),
        "post.restore",
        "thread",
        &thread_id,
        "allow",
    );
    let result = call(
        &mut f.service,
        "conversation.capabilities",
        Some(&member),
        json!({}),
    )
    .unwrap();
    assert_eq!(result["boards"][0]["board_delete"], true);
    assert_eq!(result["threads"][0]["thread_delete"], true);
    assert_eq!(result["threads"][0]["post_delete"], true);
    assert_eq!(result["threads"][0]["post_restore"], true);
    permission(
        &mut f,
        account["id"].as_str().unwrap(),
        "post.restore",
        "thread",
        &thread_id,
        "deny",
    );
    let result = call(
        &mut f.service,
        "conversation.capabilities",
        Some(&member),
        json!({}),
    )
    .unwrap();
    assert_eq!(result["threads"][0]["post_restore"], false);
}

#[test]
fn post_restore_rechecks_post_view_permission_and_never_returns_hidden_body() {
    let mut f = fixture();
    let account = call(
        &mut f.service,
        "account.create",
        Some(&f.admin),
        json!({"name":"Member","password":"member-password"}),
    )
    .unwrap();
    let member = call(
        &mut f.service,
        "account.login",
        None,
        json!({"name":"Member","password":"member-password"}),
    )
    .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let thread_id = f.thread["id"].as_str().unwrap().to_owned();
    let post = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":thread_id,"body":"Secret body"}),
    )
    .unwrap();
    call(
        &mut f.service,
        "post.delete",
        Some(&f.admin),
        json!({"thread_id":thread_id,"no":post["no"]}),
    )
    .unwrap();
    permission(
        &mut f,
        account["id"].as_str().unwrap(),
        "post.restore",
        "global",
        "",
        "allow",
    );
    let capabilities = call(
        &mut f.service,
        "conversation.capabilities",
        Some(&member),
        json!({}),
    )
    .unwrap();
    assert_eq!(capabilities["threads"][0]["post_restore"], true);

    permission(
        &mut f,
        account["id"].as_str().unwrap(),
        "post.get",
        "global",
        "",
        "deny",
    );
    let error = call(
        &mut f.service,
        "post.restore",
        Some(&member),
        json!({"thread_id":thread_id,"no":post["no"]}),
    )
    .unwrap_err();
    assert_eq!(error.exit_code, 4);
    assert!(!error.message.contains("Secret body"));

    let still_deleted = call(
        &mut f.service,
        "post.get",
        Some(&f.admin),
        json!({"thread_id":thread_id,"no":post["no"]}),
    )
    .unwrap();
    assert_eq!(still_deleted["deleted"], true);
    assert!(still_deleted["body"].is_null());
}

#[test]
fn deleted_endpoint_rejects_extra_arguments_and_ai_credentials() {
    let mut f = fixture();
    assert_eq!(
        call(
            &mut f.service,
            "conversation.deleted",
            Some(&f.admin),
            json!({"include_bodies":true})
        )
        .unwrap_err()
        .exit_code,
        2
    );
    let project_root = tempfile::tempdir().unwrap();
    let project = call(
        &mut f.service,
        "project.register",
        Some(&f.admin),
        json!({"name":"Test","root":project_root.path()}),
    )
    .unwrap();
    let identity = call(
        &mut f.service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    assert_eq!(
        call(
            &mut f.service,
            "conversation.deleted",
            Some(identity["session_token"].as_str().unwrap()),
            json!({}),
        )
        .unwrap_err()
        .exit_code,
        4
    );
}
