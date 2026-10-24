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
    call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread["id"],"body":"Visible post"}),
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

fn member(f: &mut Fixture, name: &str) -> (String, String) {
    let account = call(
        &mut f.service,
        "account.create",
        Some(&f.admin),
        json!({"name":name,"password":format!("{name}-password")}),
    )
    .unwrap();
    let login = call(
        &mut f.service,
        "account.login",
        None,
        json!({"name":name,"password":format!("{name}-password")}),
    )
    .unwrap();
    (
        account["id"].as_str().unwrap().to_owned(),
        login["token"].as_str().unwrap().to_owned(),
    )
}

fn permission(
    f: &mut Fixture,
    subject_type: &str,
    subject_id: &str,
    operation: &str,
    scope: &str,
    target_id: &str,
    decision: &str,
) {
    call(
        &mut f.service,
        "permission.set",
        Some(&f.admin),
        json!({"subject_type":subject_type,"subject_id":subject_id,"operation":operation,
            "scope":scope,"target_id":target_id,"decision":decision}),
    )
    .unwrap();
}

fn capabilities(f: &mut Fixture, token: &str) -> Value {
    call(
        &mut f.service,
        "conversation.capabilities",
        Some(token),
        json!({}),
    )
    .unwrap()
}

#[test]
fn member_capabilities_reflect_defaults_and_admin_has_all_create_rights() {
    let mut f = fixture();
    let (_, token) = member(&mut f, "Member");
    let result = capabilities(&mut f, &token);
    assert_eq!(result["board_create"], false);
    assert_eq!(
        result["boards"],
        json!([{"id":f.board["id"],"thread_create":false,"board_delete":false,"search":true}])
    );
    assert_eq!(
        result["threads"],
        json!([{"id":f.thread["id"],"post_read":true,"search":true,"response_read":true,"post_create":true,"response_create":true,
            "post_edit":false,"post_history":true,"thread_delete":false,
            "post_delete":false,"post_restore":false}])
    );

    let admin = f.admin.clone();
    let result = capabilities(&mut f, &admin);
    assert_eq!(result["board_create"], true);
    assert_eq!(result["boards"][0]["thread_create"], true);
    assert_eq!(result["boards"][0]["board_delete"], true);
    assert_eq!(result["threads"][0]["post_create"], true);
    assert_eq!(result["threads"][0]["response_create"], true);
    assert_eq!(result["threads"][0]["post_edit"], true);
    assert_eq!(result["threads"][0]["post_history"], true);
    assert_eq!(result["threads"][0]["thread_delete"], true);
    assert_eq!(result["threads"][0]["post_delete"], true);
    assert_eq!(result["threads"][0]["post_restore"], true);
}

#[test]
fn independent_search_and_response_read_denials_preserve_post_read_but_revoke_their_capability() {
    for (operation, field) in [("search", "search"), ("response.list", "response_read")] {
        let mut f = fixture();
        let (account, token) = member(&mut f, "ScopedReader");
        let thread = f.thread["id"].as_str().unwrap().to_owned();
        let before = capabilities(&mut f, &token);
        assert_eq!(before["threads"][0][field], true);
        permission(
            &mut f, "account", &account, operation, "thread", &thread, "deny",
        );
        let after = capabilities(&mut f, &token);
        assert_eq!(after["threads"][0][field], false);
        assert_eq!(after["threads"][0]["post_read"], true);
        assert_eq!(after["threads"][0]["post_history"], true);
        assert_eq!(after["threads"][0]["post_create"], true);
        assert_eq!(after["threads"][0]["response_create"], true);
    }
}

#[test]
fn board_search_denial_has_its_own_capability_without_hiding_readable_board() {
    let mut f = fixture();
    let (account, token) = member(&mut f, "BoardSearchReader");
    let board = f.board["id"].as_str().unwrap().to_owned();
    assert_eq!(capabilities(&mut f, &token)["boards"][0]["search"], true);
    permission(
        &mut f, "account", &account, "search", "board", &board, "deny",
    );
    let after = capabilities(&mut f, &token);
    assert_eq!(after["boards"][0]["id"], board);
    assert_eq!(after["boards"][0]["search"], false);
    assert_eq!(after["threads"][0]["search"], false);
    assert_eq!(after["threads"][0]["post_read"], true);
}

#[test]
fn role_grant_and_same_tier_deny_are_reflected() {
    let mut f = fixture();
    let (account_id, token) = member(&mut f, "RoleMember");
    let role = call(
        &mut f.service,
        "role.create",
        Some(&f.admin),
        json!({"name":"Writer"}),
    )
    .unwrap();
    let role_id = role["id"].as_str().unwrap();
    call(
        &mut f.service,
        "role.assign",
        Some(&f.admin),
        json!({"account_id":account_id,"role_id":role_id,"assigned":true}),
    )
    .unwrap();
    permission(
        &mut f,
        "role",
        role_id,
        "board.create",
        "global",
        "",
        "allow",
    );
    assert_eq!(capabilities(&mut f, &token)["board_create"], true);

    permission(
        &mut f,
        "account",
        &account_id,
        "board.create",
        "global",
        "",
        "deny",
    );
    assert_eq!(capabilities(&mut f, &token)["board_create"], false);
}

#[test]
fn create_capabilities_require_the_result_to_be_readable() {
    let mut f = fixture();
    let (board_account, board_token) = member(&mut f, "BoardCreator");
    permission(
        &mut f,
        "account",
        &board_account,
        "board.create",
        "global",
        "",
        "allow",
    );
    permission(
        &mut f,
        "account",
        &board_account,
        "board.get",
        "global",
        "",
        "deny",
    );
    assert_eq!(capabilities(&mut f, &board_token)["board_create"], false);

    let (thread_account, thread_token) = member(&mut f, "ThreadCreator");
    let board_id = f.board["id"].as_str().unwrap().to_owned();
    permission(
        &mut f,
        "account",
        &thread_account,
        "thread.create",
        "global",
        "",
        "allow",
    );
    permission(
        &mut f,
        "account",
        &thread_account,
        "thread.get",
        "board",
        &board_id,
        "deny",
    );
    let result = capabilities(&mut f, &thread_token);
    assert_eq!(result["boards"][0]["thread_create"], false);
}

#[test]
fn thread_overrides_parent_and_hidden_parent_id_is_not_returned() {
    let mut f = fixture();
    let (account_id, token) = member(&mut f, "Scoped");
    let board_id = f.board["id"].as_str().unwrap().to_owned();
    let thread_id = f.thread["id"].as_str().unwrap().to_owned();
    permission(
        &mut f,
        "account",
        &account_id,
        "read",
        "board",
        &board_id,
        "deny",
    );
    permission(
        &mut f,
        "account",
        &account_id,
        "read",
        "thread",
        &thread_id,
        "allow",
    );
    permission(
        &mut f,
        "account",
        &account_id,
        "post.create",
        "board",
        &board_id,
        "allow",
    );
    permission(
        &mut f,
        "account",
        &account_id,
        "post.create",
        "thread",
        &thread_id,
        "deny",
    );

    let result = capabilities(&mut f, &token);
    assert!(result["boards"].as_array().unwrap().is_empty());
    assert_eq!(result["threads"][0]["id"], thread_id);
    assert!(result["threads"][0].get("board_id").is_none());
    assert_eq!(result["threads"][0]["post_create"], false);
    assert_eq!(result["threads"][0]["response_create"], true);
}

#[test]
fn unreadable_posts_disable_both_creation_actions_and_deleted_items_are_excluded() {
    let mut f = fixture();
    let (account_id, token) = member(&mut f, "Reader");
    let thread_id = f.thread["id"].as_str().unwrap().to_owned();
    permission(
        &mut f,
        "account",
        &account_id,
        "post.get",
        "thread",
        &thread_id,
        "deny",
    );
    let result = capabilities(&mut f, &token);
    assert_eq!(result["threads"][0]["post_create"], false);
    assert_eq!(result["threads"][0]["response_create"], false);
    assert_eq!(result["threads"][0]["post_edit"], false);
    assert_eq!(result["threads"][0]["post_history"], false);

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
        json!({"id":f.board["id"]}),
    )
    .unwrap();
    let result = capabilities(&mut f, &token);
    assert!(result["boards"].as_array().unwrap().is_empty());
    assert!(result["threads"].as_array().unwrap().is_empty());
}

#[test]
fn capabilities_recompute_after_permission_changes_and_reject_unknown_arguments() {
    let mut f = fixture();
    let (account_id, token) = member(&mut f, "Changing");
    assert_eq!(
        capabilities(&mut f, &token)["threads"][0]["post_create"],
        true
    );
    assert_eq!(
        capabilities(&mut f, &token)["threads"][0]["post_edit"],
        false
    );
    assert_eq!(
        capabilities(&mut f, &token)["threads"][0]["post_history"],
        true
    );
    permission(
        &mut f,
        "account",
        &account_id,
        "post.history",
        "global",
        "",
        "deny",
    );
    assert_eq!(
        capabilities(&mut f, &token)["threads"][0]["post_history"],
        false
    );
    permission(
        &mut f,
        "account",
        &account_id,
        "post.history",
        "global",
        "",
        "allow",
    );
    assert_eq!(
        capabilities(&mut f, &token)["threads"][0]["post_history"],
        true
    );
    permission(
        &mut f,
        "account",
        &account_id,
        "post.edit",
        "global",
        "",
        "allow",
    );
    assert_eq!(
        capabilities(&mut f, &token)["threads"][0]["post_edit"],
        true
    );
    permission(
        &mut f,
        "account",
        &account_id,
        "post.edit",
        "global",
        "",
        "deny",
    );
    assert_eq!(
        capabilities(&mut f, &token)["threads"][0]["post_edit"],
        false
    );
    permission(
        &mut f,
        "account",
        &account_id,
        "post.create",
        "global",
        "",
        "deny",
    );
    assert_eq!(
        capabilities(&mut f, &token)["threads"][0]["post_create"],
        false
    );
    assert_eq!(
        call(
            &mut f.service,
            "conversation.capabilities",
            Some(&token),
            json!({"account_id":account_id})
        )
        .unwrap_err()
        .exit_code,
        2
    );
}

#[test]
fn ai_identity_is_explicitly_denied_from_human_capabilities() {
    let mut f = fixture();
    let root = tempfile::tempdir().unwrap();
    let project = call(
        &mut f.service,
        "project.register",
        Some(&f.admin),
        json!({"name":"Test","root":root.path()}),
    )
    .unwrap();
    let identity = call(
        &mut f.service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    let error = call(
        &mut f.service,
        "conversation.capabilities",
        Some(identity["session_token"].as_str().unwrap()),
        json!({}),
    )
    .unwrap_err();
    assert_eq!(error.exit_code, 4);
    assert!(error.message.contains("AI identity"));
}

#[test]
fn read_capability_distinguishes_post_access_from_thread_metadata() {
    let mut f = fixture();
    let (account, token) = member(&mut f, "ReadRevocation");
    assert_eq!(
        capabilities(&mut f, &token)["threads"][0]["post_read"],
        true
    );
    let thread = f.thread["id"].as_str().unwrap().to_owned();
    permission(
        &mut f, "account", &account, "post.get", "thread", &thread, "deny",
    );
    let result = capabilities(&mut f, &token);
    assert_eq!(result["threads"][0]["id"], thread);
    assert_eq!(result["threads"][0]["post_read"], false);
    assert_eq!(result["threads"][0]["post_history"], false);
    permission(
        &mut f, "account", &account, "post.get", "thread", &thread, "inherit",
    );
    assert_eq!(
        capabilities(&mut f, &token)["threads"][0]["post_read"],
        true
    );
}
