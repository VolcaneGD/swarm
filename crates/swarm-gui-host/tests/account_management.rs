#![cfg(windows)]
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
use swarm_gui_host::GuiHost;
use swarm_platform::CredentialStore;

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

fn setup_admin(root: &Path) -> (String, Value) {
    let mut service = Service::open(root).unwrap();
    let account = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"admin-fixture-password"}),
    )
    .unwrap();
    let token = account["token"].as_str().unwrap().to_owned();
    (token, account)
}

fn grant_account_management(service: &mut Service, admin: &str, account_id: &str) {
    for operation in [
        "account.list",
        "account.get",
        "account.create",
        "account.disable",
        "account.reset",
    ] {
        call(
            service,
            Some(admin),
            "permission.set",
            json!({"subject_type":"account","subject_id":account_id,"operation":operation,
                "scope":"global","target_id":"","decision":"allow"}),
        )
        .unwrap();
    }
}

fn assert_public_account(account: &Value) {
    let keys = account
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        keys,
        ["disabled", "id", "is_admin", "name", "role_ids"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    );
    assert!(account["id"].as_str().is_some());
    assert!(account["name"].as_str().is_some());
    assert!(account["is_admin"].as_bool().is_some());
    assert!(account["disabled"].as_bool().is_some());
    assert!(account["role_ids"].as_array().is_some());
}

#[test]
fn admin_account_management_returns_only_public_metadata_and_rotates_sessions() {
    let root = tempfile::tempdir().unwrap();
    let (_admin_token, admin_account) = setup_admin(root.path());
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(13)).unwrap()
    });
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Admin", "admin-fixture-password")
        .unwrap();

    let capabilities = host.read("management.capabilities", json!({})).unwrap();
    for field in [
        "account_list",
        "account_get",
        "account_create",
        "account_disable",
        "account_reset",
    ] {
        assert_eq!(capabilities[field], true, "{field}");
    }

    let created = host
        .manage_account(
            "account.create",
            json!({"name":"Managed","password":"initial-password"}),
        )
        .unwrap();
    assert_public_account(&created);
    assert_eq!(created["is_admin"], false);
    assert_eq!(created["disabled"], false);
    assert_eq!(created["role_ids"], json!([]));
    let account_id = created["id"].as_str().unwrap().to_owned();

    let listed = host.read("account.list", json!({})).unwrap();
    let list = listed["items"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    for account in list {
        assert_public_account(account);
    }
    let fetched = host.read("account.get", json!({"id":account_id})).unwrap();
    assert_public_account(&fetched);
    assert_eq!(fetched["name"], "Managed");
    for public_result in [&created, &listed, &fetched] {
        let serialized = serde_json::to_string(public_result).unwrap();
        for secret in [
            "initial-password",
            "admin-fixture-password",
            "recovery_code",
            "password_hash",
            "token",
        ] {
            assert!(!serialized.contains(secret));
        }
    }

    let mut service = Service::open(root.path()).unwrap();
    let old_token = call(
        &mut service,
        None,
        "account.login",
        json!({"name":"Managed","password":"initial-password"}),
    )
    .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        host.manage_account("account.disable", json!({"id":account_id,"disabled":true}),)
            .unwrap()["disabled"],
        true
    );
    assert_eq!(
        call(&mut service, Some(&old_token), "account.status", json!({}))
            .unwrap_err()
            .exit_code,
        3
    );
    assert_eq!(
        host.manage_account("account.disable", json!({"id":account_id,"disabled":false}),)
            .unwrap()["disabled"],
        false
    );
    let enabled_token = call(
        &mut service,
        None,
        "account.login",
        json!({"name":"Managed","password":"initial-password"}),
    )
    .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    host.manage_account(
        "account.reset",
        json!({"id":account_id,"password":"reset-password"}),
    )
    .unwrap();
    assert_eq!(
        call(
            &mut service,
            Some(&enabled_token),
            "account.status",
            json!({}),
        )
        .unwrap_err()
        .exit_code,
        3
    );
    assert_eq!(
        call(
            &mut service,
            None,
            "account.login",
            json!({"name":"Managed","password":"initial-password"}),
        )
        .unwrap_err()
        .exit_code,
        3
    );
    assert!(
        call(
            &mut service,
            None,
            "account.login",
            json!({"name":"Managed","password":"reset-password"}),
        )
        .is_ok()
    );

    assert_eq!(
        host.manage_account(
            "account.disable",
            json!({"id":admin_account["id"],"disabled":true}),
        )
        .unwrap_err()
        .exit_code,
        4
    );
    assert_eq!(
        host.manage_account(
            "account.reset",
            json!({"id":admin_account["id"],"password":"blocked-admin-reset"}),
        )
        .unwrap_err()
        .exit_code,
        4
    );

    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    assert!(store.get("pending-auth").unwrap().is_none());
    assert!(store.get("pending-create").unwrap().is_none());
    drop(service);
    drop(host);
    server.join().unwrap();
}

#[test]
fn delegated_account_grants_are_rechecked_after_revocation() {
    let root = tempfile::tempdir().unwrap();
    let (admin_token, _) = setup_admin(root.path());
    let mut service = Service::open(root.path()).unwrap();
    let member = call(
        &mut service,
        Some(&admin_token),
        "account.create",
        json!({"name":"Manager","password":"manager-password"}),
    )
    .unwrap();
    let member_id = member["id"].as_str().unwrap().to_owned();
    grant_account_management(&mut service, &admin_token, &member_id);
    drop(service);

    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(10)).unwrap()
    });
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Manager", "manager-password")
        .unwrap();
    assert_eq!(
        host.read("management.capabilities", json!({})).unwrap()["account_create"],
        true
    );
    let created = host
        .manage_account(
            "account.create",
            json!({"name":"Child","password":"child-password"}),
        )
        .unwrap();
    assert_public_account(&created);

    let mut service = Service::open(root.path()).unwrap();
    call(
        &mut service,
        Some(&admin_token),
        "permission.set",
        json!({"subject_type":"account","subject_id":member_id,"operation":"account.create",
            "scope":"global","target_id":"","decision":"deny"}),
    )
    .unwrap();
    assert_eq!(
        host.read("management.capabilities", json!({})).unwrap()["account_create"],
        false
    );
    assert_eq!(
        host.manage_account(
            "account.create",
            json!({"name":"DeniedChild","password":"child-password"}),
        )
        .unwrap_err()
        .exit_code,
        4
    );
    let listed = host.read("account.list", json!({})).unwrap();
    assert!(listed["items"].as_array().unwrap().len() >= 3);
    assert_public_account(
        &host
            .read("account.get", json!({"id":created["id"]}))
            .unwrap(),
    );
    drop(service);
    drop(host);
    server.join().unwrap();
}

#[test]
fn account_manage_validates_exact_arguments_and_bounds_passwords_before_transport() {
    let root = tempfile::tempdir().unwrap();
    let mut host = GuiHost::open(root.path()).unwrap();
    let too_long_password = "p".repeat(1024 * 1024 + 1);
    for (operation, args) in [
        ("account.create", json!({"name":"","password":"valid"})),
        (
            "account.create",
            json!({"name":"Valid","password":"valid","extra":true}),
        ),
        (
            "account.create",
            json!({"name":"Valid","password":too_long_password}),
        ),
        (
            "account.disable",
            json!({"id":"some-id","disabled":"false"}),
        ),
        ("account.reset", json!({"id":"some-id","password":"  "})),
        (
            "account.reset",
            json!({"id":"some-id","password":"valid","token":"forged"}),
        ),
        ("account.list", json!({})),
    ] {
        assert_eq!(
            host.manage_account(operation, args).unwrap_err().exit_code,
            2,
            "{operation}"
        );
    }
    assert_eq!(
        host.manage_account("account.create", json!({"name":"Valid","password":"valid"}),)
            .unwrap_err()
            .exit_code,
        3
    );
}

#[test]
fn account_management_is_blocked_while_authentication_is_pending() {
    let root = tempfile::tempdir().unwrap();
    let mut host = GuiHost::open(root.path()).unwrap();
    assert!(
        host.authenticate("login", "Unavailable", "not-stored-by-this-operation")
            .is_err()
    );
    assert_eq!(
        host.manage_account(
            "account.create",
            json!({"name":"Blocked","password":"blocked-password"}),
        )
        .unwrap_err()
        .exit_code,
        6
    );
}

#[test]
fn account_management_is_blocked_while_creation_result_is_pending() {
    let root = tempfile::tempdir().unwrap();
    let (_admin_token, _) = setup_admin(root.path());
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(4)).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Admin", "admin-fixture-password")
        .unwrap();
    server.join().unwrap();

    assert!(
        host.create("board.create", json!({"name":"Offline board"}))
            .is_err()
    );
    assert_eq!(host.creation_status().unwrap()["pending"], true);
    assert_eq!(
        host.manage_account(
            "account.create",
            json!({"name":"Blocked","password":"blocked-password"}),
        )
        .unwrap_err()
        .exit_code,
        6
    );
    // The pending creation is a Board request and contains no account credentials.
    assert!(
        CredentialStore::open(&root.path().join("gui-state"))
            .unwrap()
            .get("pending-auth")
            .unwrap()
            .is_none()
    );
    drop(host);
}
