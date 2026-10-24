#![cfg(windows)]

use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
use swarm_gui_host::GuiHost;
use swarm_runtime::gui::GuiClient;

fn request(operation: &str, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: None,
        args,
        context: json!({}),
        request_id: None,
    }
}

fn call(
    service: &mut Service,
    token: Option<&str>,
    operation: &str,
    args: Value,
) -> Result<Value, swarm_core::protocol::Fault> {
    let mut request = request(operation, args);
    request.auth = token.map(str::to_owned);
    service.request(request)
}

fn ready(root: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    for name in [
        swarm_runtime::pipe_name(root).unwrap(),
        swarm_runtime::gui_pipe_name(root).unwrap(),
    ] {
        loop {
            if let Ok(pipe) = swarm_platform::pipe::connect(&name) {
                drop(pipe);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn grant(client: &GuiClient, alias: &str, account_id: &str, operation: &str, decision: &str) {
    client
        .request(
            alias,
            request(
                "permission.set",
                json!({"subject_type":"account","subject_id":account_id,
                    "operation":operation,"scope":"global","target_id":"","decision":decision}),
            ),
        )
        .unwrap();
}

fn set_scope(
    host: &mut GuiHost,
    subject_type: &str,
    subject_id: &str,
    operation: &str,
    scope: &str,
    target_id: &str,
    decision: &str,
) -> Result<Value, swarm_core::protocol::Fault> {
    host.manage_authorization(
        "permission.set",
        json!({"subject_type":subject_type,"subject_id":subject_id,
            "operation":operation,"scope":scope,"target_id":target_id,"decision":decision}),
    )
}

fn drain_gui(host: &mut GuiHost) {
    loop {
        match host.read("authorization.capabilities", json!({})) {
            Ok(_) => {}
            Err(error) if error.exit_code == 8 => break,
            Err(error) => panic!("unexpected GUI drain error: {error:?}"),
        }
    }
}

#[test]
fn gui_authorization_management_tracks_roles_scope_inheritance_and_live_revocation() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"admin-authorization-fixture"}),
    )
    .unwrap();
    let admin_token = admin["token"].as_str().unwrap().to_owned();
    let member = call(
        &mut service,
        Some(&admin_token),
        "account.create",
        json!({"name":"Member","password":"member-authorization-fixture"}),
    )
    .unwrap();
    let board = call(
        &mut service,
        Some(&admin_token),
        "board.create",
        json!({"name":"Authorization board"}),
    )
    .unwrap();
    let thread = call(
        &mut service,
        Some(&admin_token),
        "thread.create",
        json!({"board_id":board["id"],"name":"Authorization thread"}),
    )
    .unwrap();
    drop(service);

    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(128)).unwrap()
    });
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Member", "member-authorization-fixture")
        .unwrap();

    let baseline = host.read("authorization.capabilities", json!({})).unwrap();
    for field in [
        "role_create",
        "role_list",
        "role_assign",
        "permission_set",
        "permission_list",
    ] {
        assert_eq!(baseline[field], false, "{field}");
    }
    let operations = baseline["operations"].as_array().unwrap();
    let post_create = operations
        .iter()
        .find(|entry| entry["operation"] == "post.create")
        .unwrap();
    assert_eq!(post_create["scopes"], json!(["global", "board", "thread"]));
    let role_create = operations
        .iter()
        .find(|entry| entry["operation"] == "role.create")
        .unwrap();
    assert_eq!(role_create["scopes"], json!(["global"]));
    assert_eq!(
        host.manage_authorization("role.create", json!({"name":"Not yet allowed"}))
            .unwrap_err()
            .exit_code,
        4
    );
    assert_eq!(
        host.manage_authorization(
            "permission.set",
            json!({
                "subject_type":"role","subject_id":"x","operation":"unknown.op",
                "scope":"global","target_id":"","decision":"allow","extra":true
            })
        )
        .unwrap_err()
        .exit_code,
        2
    );
    assert_eq!(
        host.manage_authorization(
            "role.assign",
            json!({"account_id":"x","role_id":"y","assigned":"yes"})
        )
        .unwrap_err()
        .exit_code,
        2
    );
    assert_eq!(
        host.manage_authorization(
            "permission.set",
            json!({"subject_type":"role","subject_id":"x","operation":"role.create",
                "scope":"board","target_id":"untrusted-id","decision":"allow"}),
        )
        .unwrap_err()
        .exit_code,
        2,
        "the GUI host must use the canonical operation-to-scope catalog"
    );

    let client = GuiClient::open(root.path()).unwrap();
    client
        .issue(
            "admin-authorization",
            &"a".repeat(64),
            request(
                "account.login",
                json!({"name":"Admin","password":"admin-authorization-fixture"}),
            ),
        )
        .unwrap();
    for operation in [
        "role.create",
        "role.list",
        "role.assign",
        "permission.set",
        "permission.list",
    ] {
        grant(
            &client,
            "admin-authorization",
            member["id"].as_str().unwrap(),
            operation,
            "allow",
        );
    }
    let delegated = host.read("authorization.capabilities", json!({})).unwrap();
    for field in [
        "role_create",
        "role_list",
        "role_assign",
        "permission_set",
        "permission_list",
    ] {
        assert_eq!(delegated[field], true, "{field}");
    }

    let role = host
        .manage_authorization("role.create", json!({"name":"Scoped writers"}))
        .unwrap();
    assert_eq!(
        role.as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>(),
        ["id", "name"].into_iter().map(str::to_owned).collect()
    );
    let account_id = member["id"].as_str().unwrap();
    let role_id = role["id"].as_str().unwrap();
    let assigned = host
        .manage_authorization(
            "role.assign",
            json!({"account_id":account_id,"role_id":role_id,"assigned":true}),
        )
        .unwrap();
    assert_eq!(assigned["assigned"], true);
    assert!(
        host.snapshot().unwrap()["account"]["role_ids"]
            .as_array()
            .unwrap()
            .contains(&json!(role_id))
    );
    assert!(
        host.read("role.list", json!({})).unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == role_id)
    );
    assert_eq!(
        host.read(
            "permission.list",
            json!({"subject_type":"role","subject_id":role_id})
        )
        .unwrap()["items"],
        json!([])
    );

    // Give the member sufficient parent visibility. The post.create rules below
    // then exercise global < board < thread precedence and same-tier deny wins.
    for operation in [
        "read",
        "board.list",
        "board.get",
        "thread.list",
        "thread.get",
        "post.list",
        "post.get",
    ] {
        grant(
            &client,
            "admin-authorization",
            account_id,
            operation,
            "allow",
        );
    }
    set_scope(
        &mut host,
        "role",
        role_id,
        "post.create",
        "global",
        "",
        "allow",
    )
    .unwrap();
    set_scope(
        &mut host,
        "role",
        role_id,
        "post.create",
        "board",
        board["id"].as_str().unwrap(),
        "allow",
    )
    .unwrap();
    set_scope(
        &mut host,
        "account",
        account_id,
        "post.create",
        "board",
        board["id"].as_str().unwrap(),
        "deny",
    )
    .unwrap();
    assert_eq!(
        host.create(
            "post.create",
            json!({"thread_id":thread["id"],"body":"Denied by board scope"})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    host.discard_create().unwrap();

    set_scope(
        &mut host,
        "role",
        role_id,
        "post.create",
        "thread",
        thread["id"].as_str().unwrap(),
        "allow",
    )
    .unwrap();
    set_scope(
        &mut host,
        "account",
        account_id,
        "post.create",
        "thread",
        thread["id"].as_str().unwrap(),
        "deny",
    )
    .unwrap();
    assert_eq!(
        host.create(
            "post.create",
            json!({"thread_id":thread["id"],"body":"Same tier deny"})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    host.discard_create().unwrap();
    set_scope(
        &mut host,
        "account",
        account_id,
        "post.create",
        "thread",
        thread["id"].as_str().unwrap(),
        "inherit",
    )
    .unwrap();
    let created = host
        .create(
            "post.create",
            json!({"thread_id":thread["id"],"body":"Thread scope overrides board deny"}),
        )
        .unwrap();
    assert_eq!(created["body"], "Thread scope overrides board deny");
    host.acknowledge_create().unwrap();

    set_scope(
        &mut host,
        "account",
        account_id,
        "board.get",
        "board",
        board["id"].as_str().unwrap(),
        "deny",
    )
    .unwrap();
    let hidden_parent = host.read("thread.get", json!({"id":thread["id"]})).unwrap();
    assert!(hidden_parent["board_id"].is_null());

    grant(
        &client,
        "admin-authorization",
        account_id,
        "permission.set",
        "deny",
    );
    assert_eq!(
        host.read("authorization.capabilities", json!({})).unwrap()["permission_set"],
        false
    );
    assert_eq!(
        host.manage_authorization(
            "permission.set",
            json!({"subject_type":"account","subject_id":account_id,"operation":"role.list",
                "scope":"global","target_id":"","decision":"allow"}),
        )
        .unwrap_err()
        .exit_code,
        4,
        "the core must reject a write after the displayed capability is revoked"
    );
    drain_gui(&mut host);
    server.join().unwrap();
}

#[test]
fn authorization_management_is_blocked_while_creation_is_pending() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"admin-pending-fixture"}),
    )
    .unwrap();
    call(
        &mut service,
        Some(admin["token"].as_str().unwrap()),
        "account.create",
        json!({"name":"Member","password":"member-pending-fixture"}),
    )
    .unwrap();
    drop(service);
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(4)).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Member", "member-pending-fixture")
        .unwrap();
    server.join().unwrap();
    assert_eq!(
        host.create("board.create", json!({"name":"Pending board"}))
            .unwrap_err()
            .exit_code,
        8
    );
    assert_eq!(host.creation_status().unwrap()["pending"], true);
    assert_eq!(
        host.manage_authorization("role.create", json!({"name":"Blocked"}))
            .unwrap_err()
            .exit_code,
        6
    );
}

#[test]
fn authorization_management_is_blocked_while_authentication_is_pending() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"admin-auth-pending-fixture"}),
    )
    .unwrap();
    call(
        &mut service,
        Some(admin["token"].as_str().unwrap()),
        "account.create",
        json!({"name":"Member","password":"member-auth-pending-fixture"}),
    )
    .unwrap();
    drop(service);
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(5)).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Member", "member-auth-pending-fixture")
        .unwrap();
    host.read("authorization.capabilities", json!({})).unwrap();
    server.join().unwrap();

    assert_eq!(
        host.authenticate("login", "Member", "member-auth-pending-fixture")
            .unwrap_err()
            .exit_code,
        8
    );
    assert_eq!(
        host.manage_authorization("role.create", json!({"name":"Blocked"}))
            .unwrap_err()
            .exit_code,
        6
    );
}
