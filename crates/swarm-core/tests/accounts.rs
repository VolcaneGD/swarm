use serde_json::{Value, json};
use swarm_core::{
    Service,
    protocol::{Fault, Request},
};

fn call(
    service: &mut Service,
    operation: &str,
    auth: Option<&str>,
    args: Value,
) -> Result<Value, Fault> {
    service.request(Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    })
}

#[test]
fn skipped_initial_password_keeps_admin_authority_and_survives_service_reopen() {
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":""}),
    )
    .unwrap();
    let id = admin["id"].clone();
    assert!(!admin["token"].as_str().unwrap().is_empty());
    let status = call(
        &mut service,
        "account.status",
        admin["token"].as_str(),
        json!({}),
    )
    .unwrap();
    assert_eq!(status["is_admin"], true);
    assert_eq!(status["lock_enabled"], false);
    assert!(
        call(
            &mut service,
            "board.create",
            None,
            json!({"name":"Blocked"})
        )
        .is_err()
    );
    drop(service);
    let mut service = Service::open(data.path()).unwrap();
    let login = call(
        &mut service,
        "account.login",
        None,
        json!({"name":"Admin","password":""}),
    )
    .unwrap();
    assert_eq!(login["id"], id);
    call(
        &mut service,
        "account.password",
        login["token"].as_str(),
        json!({"password":"set-later"}),
    )
    .unwrap();
    assert!(
        call(
            &mut service,
            "account.login",
            None,
            json!({"name":"Admin","password":""})
        )
        .is_err()
    );
    assert!(
        call(
            &mut service,
            "account.login",
            None,
            json!({"name":"Admin","password":"set-later"})
        )
        .is_ok()
    );
}

struct Fixture {
    _data: tempfile::TempDir,
    service: Service,
    admin: Value,
    board: Value,
    thread: Value,
    post: Value,
}

#[test]
fn lock_changes_require_current_password_and_revoke_old_sessions() {
    let mut f = fixture();
    let token = f.admin["token"].as_str().unwrap();
    assert!(
        call(
            &mut f.service,
            "account.password",
            Some(token),
            json!({"password":"","current_password":"wrong"})
        )
        .is_err()
    );
    let unlocked = call(
        &mut f.service,
        "account.password",
        Some(token),
        json!({"password":"","current_password":"admin-password"}),
    )
    .unwrap();
    assert_eq!(unlocked["account"]["lock_enabled"], false);
    assert!(call(&mut f.service, "account.status", Some(token), json!({})).is_err());
    let relocked = call(
        &mut f.service,
        "account.password",
        unlocked["token"].as_str(),
        json!({"password":"new-password","current_password":""}),
    )
    .unwrap();
    assert_eq!(relocked["account"]["lock_enabled"], true);
    assert!(
        call(
            &mut f.service,
            "account.password",
            relocked["token"].as_str(),
            json!({"password":"bypass"})
        )
        .is_err()
    );
    assert!(
        call(
            &mut f.service,
            "account.login",
            None,
            json!({"name":"Admin","password":""})
        )
        .is_err()
    );
    assert!(
        call(
            &mut f.service,
            "account.login",
            None,
            json!({"name":"Admin","password":"new-password"})
        )
        .is_ok()
    );
    let member = call(
        &mut f.service,
        "account.create",
        relocked["token"].as_str(),
        json!({"name":"UnlockedMember","password":""}),
    )
    .unwrap();
    assert_eq!(member["lock_enabled"], false);
    let login = call(
        &mut f.service,
        "account.login",
        None,
        json!({"name":"UnlockedMember","password":""}),
    )
    .unwrap();
    assert!(
        call(
            &mut f.service,
            "account.list",
            login["token"].as_str(),
            json!({})
        )
        .is_err()
    );
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
    .unwrap();
    let board = call(
        &mut service,
        "board.create",
        admin["token"].as_str(),
        json!({"name":"Board"}),
    )
    .unwrap();
    let thread = call(
        &mut service,
        "thread.create",
        admin["token"].as_str(),
        json!({"board_id":board["id"],"name":"Thread"}),
    )
    .unwrap();
    let post = call(
        &mut service,
        "post.create",
        admin["token"].as_str(),
        json!({"thread_id":thread["id"],"body":"Admin post"}),
    )
    .unwrap();
    assert_eq!(post["author"]["user_name"], admin["name"]);
    assert_eq!(post["author"]["model_name"], "USER");
    Fixture {
        _data: data,
        service,
        admin,
        board,
        thread,
        post,
    }
}

fn admin_token(f: &Fixture) -> &str {
    f.admin["token"].as_str().unwrap()
}

fn admin_call(f: &mut Fixture, operation: &str, args: Value) -> Result<Value, Fault> {
    let token = admin_token(f).to_owned();
    call(&mut f.service, operation, Some(&token), args)
}

fn create_account(f: &mut Fixture, name: &str) -> Value {
    admin_call(
        f,
        "account.create",
        json!({"name":name,"password":format!("{name}-password")}),
    )
    .unwrap()
}

fn login(f: &mut Fixture, name: &str) -> Value {
    call(
        &mut f.service,
        "account.login",
        None,
        json!({"name":name,"password":format!("{name}-password")}),
    )
    .unwrap()
}

fn permission(
    f: &mut Fixture,
    subject_type: &str,
    subject_id: &str,
    operation: &str,
    scope: &str,
    target_id: &str,
    decision: &str,
) -> Result<Value, Fault> {
    admin_call(
        f,
        "permission.set",
        json!({"subject_type":subject_type,"subject_id":subject_id,"operation":operation,
            "scope":scope,"target_id":target_id,"decision":decision}),
    )
}

fn post_edit(f: &mut Fixture, token: &str, no: Value, body: &str) -> Result<Value, Fault> {
    call(
        &mut f.service,
        "post.edit",
        Some(token),
        json!({"thread_id":f.thread["id"],"no":no,"body":body,"if_revision":1}),
    )
}

#[test]
fn additional_account_gets_private_login_and_default_read_post_reply_rights() {
    let mut f = fixture();
    let created = create_account(&mut f, "Reviewer");
    let id = created["id"].as_str().unwrap();
    assert!(created.get("token").is_none());
    assert!(created.get("recovery_code").is_none());
    assert!(created.get("password").is_none());

    let login = login(&mut f, "Reviewer");
    assert_eq!(login["id"], id);
    let token = login["token"].as_str().unwrap();
    assert!(call(&mut f.service, "board.list", Some(token), json!({})).is_ok());
    assert!(
        call(
            &mut f.service,
            "board.get",
            Some(token),
            json!({"id":f.board["id"]})
        )
        .is_ok()
    );
    assert!(
        call(
            &mut f.service,
            "thread.get",
            Some(token),
            json!({"id":f.thread["id"]})
        )
        .is_ok()
    );
    assert!(
        call(
            &mut f.service,
            "post.get",
            Some(token),
            json!({"thread_id":f.thread["id"],"no":1})
        )
        .is_ok()
    );

    let own = call(
        &mut f.service,
        "post.create",
        Some(token),
        json!({"thread_id":f.thread["id"],"body":"Reviewer post"}),
    )
    .unwrap();
    let response = call(
        &mut f.service,
        "response.create",
        Some(token),
        json!({"thread_id":f.thread["id"],"to":own["no"],"body":"Reviewer response"}),
    )
    .unwrap();
    assert_eq!(response["reply_to"], own["no"]);
    assert!(
        call(
            &mut f.service,
            "post.get",
            Some(token),
            json!({"thread_id":f.thread["id"],"no":response["no"]})
        )
        .is_ok()
    );
}

#[test]
fn additional_account_defaults_deny_management_and_post_changes() {
    let mut f = fixture();
    let created = create_account(&mut f, "Reader");
    let token = login(&mut f, "Reader")["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let board_id = f.board["id"].as_str().unwrap();
    let thread_id = f.thread["id"].as_str().unwrap();
    let post_no = f.post["no"].clone();

    for (operation, args) in [
        ("board.create", json!({"name":"Unauthorized"})),
        (
            "thread.create",
            json!({"board_id":board_id,"name":"Unauthorized"}),
        ),
        (
            "board.edit",
            json!({"id":board_id,"name":"Changed","if_revision":1}),
        ),
        (
            "thread.edit",
            json!({"id":thread_id,"name":"Changed","if_revision":1}),
        ),
        ("board.delete", json!({"id":board_id})),
        ("thread.delete", json!({"id":thread_id})),
        (
            "post.edit",
            json!({"thread_id":thread_id,"no":post_no,"body":"Changed","if_revision":1}),
        ),
        ("post.delete", json!({"thread_id":thread_id,"no":post_no})),
        ("post.restore", json!({"thread_id":thread_id,"no":post_no})),
    ] {
        assert_eq!(
            call(&mut f.service, operation, Some(&token), args)
                .unwrap_err()
                .exit_code,
            4,
            "{operation}"
        );
    }
    assert!(created.get("token").is_none());
}

#[test]
fn role_grants_inherit_and_same_level_account_deny_wins() {
    let mut f = fixture();
    let created = create_account(&mut f, "Editor");
    let account_id = created["id"].as_str().unwrap().to_owned();
    let token = login(&mut f, "Editor")["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let role = admin_call(&mut f, "role.create", json!({"name":"Editors"})).unwrap();
    let role_id = role["id"].as_str().unwrap().to_owned();
    admin_call(
        &mut f,
        "role.assign",
        json!({"account_id":account_id,"role_id":role_id,"assigned":true}),
    )
    .unwrap();
    permission(
        &mut f,
        "role",
        &role_id,
        "board.create",
        "global",
        "",
        "allow",
    )
    .unwrap();
    permission(&mut f, "role", &role_id, "post.edit", "global", "", "allow").unwrap();

    let created_board = call(
        &mut f.service,
        "board.create",
        Some(&token),
        json!({"name":"Role Board"}),
    )
    .unwrap();
    assert_eq!(created_board["name"], "Role Board");
    let own_post = call(
        &mut f.service,
        "post.create",
        Some(&token),
        json!({"thread_id":f.thread["id"],"body":"Editable by role"}),
    )
    .unwrap();
    permission(
        &mut f,
        "account",
        &account_id,
        "post.edit",
        "global",
        "",
        "deny",
    )
    .unwrap();
    assert_eq!(
        post_edit(&mut f, &token, own_post["no"].clone(), "Denied")
            .unwrap_err()
            .exit_code,
        4
    );
    permission(
        &mut f,
        "account",
        &account_id,
        "post.edit",
        "global",
        "",
        "inherit",
    )
    .unwrap();
    assert!(post_edit(&mut f, &token, own_post["no"].clone(), "Inherited").is_ok());

    let roles = admin_call(&mut f, "role.list", json!({})).unwrap();
    assert_eq!(roles["items"].as_array().unwrap().len(), 1);
    let accounts = admin_call(&mut f, "account.list", json!({})).unwrap();
    let account = accounts["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == account_id)
        .unwrap();
    for secret in [
        "password",
        "password_hash",
        "token",
        "token_hash",
        "recovery_code",
        "recovery_hash",
    ] {
        assert!(
            account.get(secret).is_none(),
            "account list leaked {secret}"
        );
    }
}

#[test]
fn board_deny_can_be_overridden_by_explicit_thread_read_and_parent_is_redacted() {
    let mut f = fixture();
    let created = create_account(&mut f, "ThreadReader");
    let account_id = created["id"].as_str().unwrap().to_owned();
    let token = login(&mut f, "ThreadReader")["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let board_id = f.board["id"].as_str().unwrap().to_owned();
    let thread_id = f.thread["id"].as_str().unwrap().to_owned();

    permission(
        &mut f,
        "account",
        &account_id,
        "board.get",
        "board",
        &board_id,
        "deny",
    )
    .unwrap();
    permission(
        &mut f,
        "account",
        &account_id,
        "thread.get",
        "thread",
        &thread_id,
        "allow",
    )
    .unwrap();
    permission(
        &mut f,
        "account",
        &account_id,
        "thread.list",
        "thread",
        &thread_id,
        "allow",
    )
    .unwrap();
    assert_eq!(
        call(
            &mut f.service,
            "board.get",
            Some(&token),
            json!({"id":board_id})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    let thread = call(
        &mut f.service,
        "thread.get",
        Some(&token),
        json!({"id":thread_id}),
    )
    .unwrap();
    assert_eq!(thread["id"], thread_id);
    assert!(thread["board_id"].is_null());
    let listed = call(&mut f.service, "thread.list", Some(&token), json!({})).unwrap();
    assert_eq!(listed["items"].as_array().unwrap().len(), 1);
    assert_eq!(listed["items"][0]["id"], thread_id);
    assert!(listed["items"][0]["board_id"].is_null());
}

#[test]
fn disabling_then_reenabling_does_not_revive_old_sessions_and_admin_is_protected() {
    let mut f = fixture();
    let created = create_account(&mut f, "Disabled");
    let account_id = created["id"].as_str().unwrap().to_owned();
    let old = login(&mut f, "Disabled")["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let main_admin_id = f.admin["id"].clone();
    assert_eq!(
        admin_call(
            &mut f,
            "account.disable",
            json!({"id":main_admin_id,"disabled":true})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    admin_call(
        &mut f,
        "account.disable",
        json!({"id":account_id,"disabled":true}),
    )
    .unwrap();
    assert_eq!(
        call(&mut f.service, "board.list", Some(&old), json!({}))
            .unwrap_err()
            .exit_code,
        3
    );
    admin_call(
        &mut f,
        "account.disable",
        json!({"id":account_id,"disabled":false}),
    )
    .unwrap();
    assert_eq!(
        call(&mut f.service, "board.list", Some(&old), json!({}))
            .unwrap_err()
            .exit_code,
        3
    );
    let fresh = login(&mut f, "Disabled");
    assert_ne!(fresh["token"], old);
    assert!(
        call(
            &mut f.service,
            "board.list",
            fresh["token"].as_str(),
            json!({})
        )
        .is_ok()
    );
}

#[test]
fn password_change_and_admin_reset_rotate_tokens_and_clear_old_sessions() {
    let mut f = fixture();
    let created = create_account(&mut f, "PasswordUser");
    let account_id = created["id"].as_str().unwrap().to_owned();
    let first_token = login(&mut f, "PasswordUser")["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let changed = call(
        &mut f.service,
        "account.password",
        Some(&first_token),
        json!({"password":"new-user-password","current_password":"PasswordUser-password"}),
    )
    .unwrap();
    assert_eq!(
        call(&mut f.service, "board.list", Some(&first_token), json!({}))
            .unwrap_err()
            .exit_code,
        3
    );
    assert!(
        call(
            &mut f.service,
            "board.list",
            changed["token"].as_str(),
            json!({})
        )
        .is_ok()
    );
    assert_eq!(
        call(
            &mut f.service,
            "account.login",
            None,
            json!({"name":"PasswordUser","password":"PasswordUser-password"})
        )
        .unwrap_err()
        .exit_code,
        3
    );
    let main_admin_id = f.admin["id"].clone();
    assert_eq!(
        admin_call(
            &mut f,
            "account.reset",
            json!({"id":main_admin_id,"password":"must-not-reset"})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    assert!(
        admin_call(
            &mut f,
            "account.reset",
            json!({"id":account_id,"password":"reset-password"})
        )
        .is_ok()
    );
    assert_eq!(
        call(
            &mut f.service,
            "board.list",
            changed["token"].as_str(),
            json!({})
        )
        .unwrap_err()
        .exit_code,
        3
    );
    assert_eq!(
        call(
            &mut f.service,
            "account.login",
            None,
            json!({"name":"PasswordUser","password":"PasswordUser-password"})
        )
        .unwrap_err()
        .exit_code,
        3
    );
    let reset = call(
        &mut f.service,
        "account.login",
        None,
        json!({"name":"PasswordUser","password":"reset-password"}),
    )
    .unwrap();
    assert_ne!(reset["token"], changed["token"]);
    assert!(
        call(
            &mut f.service,
            "board.list",
            reset["token"].as_str(),
            json!({})
        )
        .is_ok()
    );
}

#[test]
fn recovery_is_generic_on_failure_rotates_admin_credentials_and_only_admin_can_use_it() {
    let mut f = fixture();
    let old_admin = admin_token(&f).to_owned();
    let old_code = f.admin["recovery_code"].as_str().unwrap().to_owned();
    let other = create_account(&mut f, "NotAdmin");
    assert_eq!(
        call(
            &mut f.service,
            "account.recover",
            None,
            json!({"name":"NotAdmin","recovery_code":"wrong","password":"x"})
        )
        .unwrap_err()
        .exit_code,
        3
    );
    assert_eq!(
        call(
            &mut f.service,
            "account.recover",
            None,
            json!({"name":"Admin","recovery_code":"wrong","password":"new-admin-password"})
        )
        .unwrap_err()
        .exit_code,
        3
    );
    assert!(call(&mut f.service, "board.list", Some(&old_admin), json!({})).is_ok());
    let recovered = call(
        &mut f.service,
        "account.recover",
        None,
        json!({"name":"Admin","recovery_code":old_code,"password":"new-admin-password"}),
    )
    .unwrap();
    assert!(recovered.get("token").is_some());
    assert!(recovered.get("recovery_code").is_some());
    assert_eq!(
        call(&mut f.service, "board.list", Some(&old_admin), json!({}))
            .unwrap_err()
            .exit_code,
        3
    );
    assert_eq!(
        call(
            &mut f.service,
            "account.recover",
            None,
            json!({"name":"Admin","recovery_code":old_code,"password":"another-password"})
        )
        .unwrap_err()
        .exit_code,
        3
    );
    assert!(
        call(
            &mut f.service,
            "board.list",
            recovered["token"].as_str(),
            json!({})
        )
        .is_ok()
    );
    assert!(other.get("token").is_none());
}

#[test]
fn delegated_management_requires_permission_and_ai_cannot_manage_accounts() {
    let mut f = fixture();
    let manager = create_account(&mut f, "Manager");
    let manager_id = manager["id"].as_str().unwrap().to_owned();
    let manager_token = login(&mut f, "Manager")["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        call(
            &mut f.service,
            "account.create",
            Some(&manager_token),
            json!({"name":"Child","password":"child-password"})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    permission(
        &mut f,
        "account",
        &manager_id,
        "account.create",
        "global",
        "",
        "allow",
    )
    .unwrap();
    let child = call(
        &mut f.service,
        "account.create",
        Some(&manager_token),
        json!({"name":"Child","password":"child-password"}),
    )
    .unwrap();
    assert!(child.get("token").is_none());

    let project_root = tempfile::tempdir().unwrap();
    let project = admin_call(
        &mut f,
        "project.register",
        json!({"name":"AI Project","root":project_root.path()}),
    )
    .unwrap();
    let started = call(
        &mut f.service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    let ai_token = started["session_token"].as_str().unwrap();
    for (operation, args) in [
        ("account.create", json!({"name":"AIChild","password":"p"})),
        ("account.disable", json!({"id":child["id"],"disabled":true})),
        (
            "permission.set",
            json!({"subject_type":"account","subject_id":child["id"],"operation":"board.create","scope":"global","target_id":"","decision":"allow"}),
        ),
        ("role.create", json!({"name":"AI Role"})),
    ] {
        assert_eq!(
            call(&mut f.service, operation, Some(ai_token), args)
                .unwrap_err()
                .exit_code,
            4,
            "{operation}"
        );
    }
}

#[test]
fn deleted_post_history_and_body_need_the_explicit_deleted_body_permission() {
    let mut f = fixture();
    let created = create_account(&mut f, "HistoryReader");
    let account_id = created["id"].as_str().unwrap().to_owned();
    let token = login(&mut f, "HistoryReader")["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let thread_id = f.thread["id"].as_str().unwrap().to_owned();
    let no = f.post["no"].clone();

    let before_edit = call(
        &mut f.service,
        "post.history",
        Some(&token),
        json!({"thread_id":thread_id,"no":no}),
    )
    .unwrap();
    assert!(before_edit["items"].as_array().unwrap().is_empty());
    admin_call(
        &mut f,
        "post.edit",
        json!({"thread_id":thread_id,"no":no,"body":"Admin revision","if_revision":1}),
    )
    .unwrap();
    let live_history = call(
        &mut f.service,
        "post.history",
        Some(&token),
        json!({"thread_id":thread_id,"no":no}),
    )
    .unwrap();
    assert_eq!(live_history["items"].as_array().unwrap().len(), 1);
    assert_eq!(live_history["items"][0]["body"], "Admin post");

    admin_call(
        &mut f,
        "post.delete",
        json!({"thread_id":thread_id,"no":no}),
    )
    .unwrap();
    let tombstone = call(
        &mut f.service,
        "post.get",
        Some(&token),
        json!({"thread_id":thread_id,"no":no}),
    )
    .unwrap();
    assert_eq!(tombstone["deleted"], true);
    assert!(tombstone["body"].is_null());
    assert_eq!(
        call(
            &mut f.service,
            "post.history",
            Some(&token),
            json!({"thread_id":thread_id,"no":no})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    assert_eq!(
        call(
            &mut f.service,
            "post.deleted_body",
            Some(&token),
            json!({"thread_id":thread_id,"no":no})
        )
        .unwrap_err()
        .exit_code,
        4
    );

    permission(
        &mut f,
        "account",
        &account_id,
        "post.deleted_body",
        "global",
        "",
        "allow",
    )
    .unwrap();
    let private_body = call(
        &mut f.service,
        "post.deleted_body",
        Some(&token),
        json!({"thread_id":thread_id,"no":no}),
    )
    .unwrap();
    assert_eq!(private_body["body"], "Admin revision");
    let authorized_history = call(
        &mut f.service,
        "post.history",
        Some(&token),
        json!({"thread_id":thread_id,"no":no}),
    )
    .unwrap();
    assert_eq!(authorized_history["items"][0]["body"], "Admin post");
}

#[test]
fn unassigning_role_revokes_grant_and_global_read_deny_keeps_self_actions_available() {
    let mut f = fixture();
    let created = create_account(&mut f, "RoleMember");
    let account_id = created["id"].as_str().unwrap().to_owned();
    let token = login(&mut f, "RoleMember")["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let role = admin_call(&mut f, "role.create", json!({"name":"Temporary"})).unwrap();
    let role_id = role["id"].as_str().unwrap().to_owned();
    admin_call(
        &mut f,
        "role.assign",
        json!({"account_id":account_id,"role_id":role_id,"assigned":true}),
    )
    .unwrap();
    permission(
        &mut f,
        "role",
        &role_id,
        "board.create",
        "global",
        "",
        "allow",
    )
    .unwrap();
    assert!(
        call(
            &mut f.service,
            "board.create",
            Some(&token),
            json!({"name":"Granted"})
        )
        .is_ok()
    );
    admin_call(
        &mut f,
        "role.assign",
        json!({"account_id":account_id,"role_id":role_id,"assigned":false}),
    )
    .unwrap();
    assert_eq!(
        call(
            &mut f.service,
            "board.create",
            Some(&token),
            json!({"name":"Revoked"})
        )
        .unwrap_err()
        .exit_code,
        4
    );

    permission(&mut f, "account", &account_id, "read", "global", "", "deny").unwrap();
    assert_eq!(
        call(
            &mut f.service,
            "board.get",
            Some(&token),
            json!({"id":f.board["id"]})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    let hidden = call(&mut f.service, "board.list", Some(&token), json!({})).unwrap();
    assert!(hidden["items"].as_array().unwrap().is_empty());
    assert_eq!(
        call(
            &mut f.service,
            "post.get",
            Some(&token),
            json!({"thread_id":f.thread["id"],"no":1})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    let status = call(&mut f.service, "account.status", Some(&token), json!({})).unwrap();
    assert_eq!(status["id"], account_id);
    assert!(call(&mut f.service, "account.logout", Some(&token), json!({})).is_ok());
    assert_eq!(
        call(&mut f.service, "account.status", Some(&token), json!({}))
            .unwrap_err()
            .exit_code,
        3
    );
}
