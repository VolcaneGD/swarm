#![cfg(windows)]
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::Path,
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
use swarm_gui_host::GuiHost;
use swarm_ipc::RpcResponse;
use swarm_platform::{CredentialStore, pipe};
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
            if let Ok(connection) = pipe::connect(&name) {
                drop(connection);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn assert_no_secret_keys(value: &Value) {
    match value {
        Value::Object(object) => {
            for key in ["token", "recovery_code", "recovery_hash", "password_hash"] {
                assert!(!object.contains_key(key), "secret field {key} in {value}");
            }
            for child in object.values() {
                assert_no_secret_keys(child);
            }
        }
        Value::Array(values) => {
            for child in values {
                assert_no_secret_keys(child);
            }
        }
        _ => {}
    }
}

fn read_pending_auth(root: &Path) -> (String, String, Request) {
    let store = CredentialStore::open(&root.join("gui-state")).unwrap();
    let bytes = store.get("pending-auth").unwrap().unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let args = value["request"]["args"].as_object().unwrap();
    assert_eq!(args.len(), 3);
    assert!(args.contains_key("name"));
    assert!(args.contains_key("recovery_code"));
    assert!(args.contains_key("password"));
    (
        value["alias"].as_str().unwrap().to_owned(),
        value["intent"].as_str().unwrap().to_owned(),
        serde_json::from_value(value["request"].clone()).unwrap(),
    )
}

fn assert_only_key(value: &Value, expected: &str) {
    let keys = value
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    assert_eq!(keys, [expected.to_owned()].into_iter().collect());
}

#[test]
fn name_history_remembers_only_successful_accounts_and_survives_logout_and_reopen() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, None, None).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    let first = host
        .authenticate("setup", "HistoryAdmin", "history-admin-secret")
        .unwrap();
    assert_eq!(first["accountNameHistory"], json!(["HistoryAdmin"]));
    host.manage_account(
        "account.create",
        json!({"name":"履歴Member","password":"history-member-secret"}),
    )
    .unwrap();
    assert_eq!(
        host.authenticate("login", "FailedName", "wrong")
            .unwrap_err()
            .exit_code,
        3
    );
    let logged_out = host.logout().unwrap();
    assert!(logged_out["account"].is_null());
    assert_eq!(
        logged_out["accountNameHistory"],
        json!(["履歴Member", "HistoryAdmin"])
    );
    assert!(
        !serde_json::to_string(&logged_out)
            .unwrap()
            .contains("secret")
    );
    drop(host);
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.snapshot().unwrap()["accountNameHistory"],
        logged_out["accountNameHistory"]
    );
    let member = host
        .authenticate("login", "履歴Member", "history-member-secret")
        .unwrap();
    assert_eq!(member["accountNameHistory"][0], "履歴Member");
    assert_no_secret_keys(&member);
    host.authenticate("login", "HistoryAdmin", "history-admin-secret")
        .unwrap();
    host.stop_runtime().unwrap();
    drop(host);
    server.join().unwrap();
}

#[test]
fn recovery_replays_after_host_reopen_and_rotates_the_code_and_password() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let first_server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(5)).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    let setup = host
        .authenticate("setup", "Admin", "initial-admin-password")
        .unwrap();
    assert_no_secret_keys(&setup);
    let first_reveal = host.reveal_recovery_code().unwrap();
    assert_only_key(&first_reveal, "recovery_code");
    let old_code = first_reveal["recovery_code"].as_str().unwrap().to_owned();
    assert!(!old_code.is_empty());
    drop(host);
    first_server.join().unwrap();

    // Leave the encrypted recovery request pending while the runtime is stopped.
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.recover_account("Admin", &old_code, "recovered-admin-password")
            .unwrap_err()
            .exit_code,
        8
    );
    drop(host);
    let (alias, intent, request) = read_pending_auth(root.path());

    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(7)).unwrap());
    ready(root.path());
    let client = GuiClient::open(root.path()).unwrap();
    // Simulate a committed recovery whose reply was lost to the pending GUI host.
    let issue_result = client.issue(&alias, &intent, request).unwrap();
    assert_only_key(&issue_result, "id");
    let serialized_issue = serde_json::to_string(&issue_result).unwrap();
    assert!(!serialized_issue.contains(&old_code));
    assert!(!serialized_issue.contains("recovered-admin-password"));

    let mut host = GuiHost::open(root.path()).unwrap();
    let replayed = host.retry_pending().unwrap();
    assert_no_secret_keys(&replayed);
    assert_eq!(replayed["account"]["name"], "Admin");
    assert_eq!(replayed["pendingIssue"], false);
    let second_reveal = host.reveal_recovery_code().unwrap();
    assert_only_key(&second_reveal, "recovery_code");
    let new_code = second_reveal["recovery_code"].as_str().unwrap().to_owned();
    assert_ne!(new_code, old_code);
    assert!(
        !serde_json::to_string(&replayed)
            .unwrap()
            .contains(&new_code)
    );
    assert_eq!(
        host.recover_account("Admin", &old_code, "another-password")
            .unwrap_err()
            .exit_code,
        3
    );

    let mut service = Service::open(root.path()).unwrap();
    assert!(
        call(
            &mut service,
            None,
            "account.login",
            json!({"name":"Admin","password":"recovered-admin-password"}),
        )
        .is_ok()
    );
    assert_eq!(
        call(
            &mut service,
            None,
            "account.login",
            json!({"name":"Admin","password":"initial-admin-password"}),
        )
        .unwrap_err()
        .exit_code,
        3
    );
    drop(service);
    drop(host);
    server.join().unwrap();
}

#[test]
fn cancellation_keeps_selection_and_conversation_pending_without_rolling_back_recovery() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let first_server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(5)).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("setup", "Admin", "initial-admin-password")
        .unwrap();
    let old_code = host.reveal_recovery_code().unwrap()["recovery_code"]
        .as_str()
        .unwrap()
        .to_owned();
    drop(host);
    first_server.join().unwrap();

    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.create("board.create", json!({"name":"Pending board"}))
            .unwrap_err()
            .exit_code,
        8
    );
    assert_eq!(host.creation_status().unwrap()["pending"], true);
    assert_eq!(
        host.recover_account("Other account", &old_code, "unused-password")
            .unwrap_err()
            .exit_code,
        6,
        "a pending conversation create blocks switching account"
    );
    assert_eq!(
        host.recover_account("Admin", &old_code, "recovered-admin-password")
            .unwrap_err()
            .exit_code,
        8
    );
    assert_eq!(
        host.reveal_recovery_code().unwrap_err().exit_code,
        6,
        "explicit reveal is blocked while issuance is pending"
    );
    drop(host);
    let (alias, intent, request) = read_pending_auth(root.path());

    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(4)).unwrap());
    ready(root.path());
    let client = GuiClient::open(root.path()).unwrap();
    let committed = client.issue(&alias, &intent, request).unwrap();
    assert_only_key(&committed, "id");

    let mut host = GuiHost::open(root.path()).unwrap();
    let canceled = host.cancel_pending().unwrap();
    assert_no_secret_keys(&canceled);
    assert_eq!(canceled["account"]["name"], "Admin");
    assert_eq!(canceled["pendingIssue"], false);
    assert_eq!(host.creation_status().unwrap()["pending"], true);
    assert_eq!(
        host.reveal_recovery_code().unwrap_err().exit_code,
        6,
        "canceling authentication must leave the conversation request pending"
    );
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    assert!(store.get("pending-auth").unwrap().is_none());
    assert!(store.get("pending-create").unwrap().is_some());

    let mut service = Service::open(root.path()).unwrap();
    assert!(
        call(
            &mut service,
            None,
            "account.login",
            json!({"name":"Admin","password":"recovered-admin-password"}),
        )
        .is_ok()
    );
    assert_eq!(
        call(
            &mut service,
            None,
            "account.login",
            json!({"name":"Admin","password":"initial-admin-password"}),
        )
        .unwrap_err()
        .exit_code,
        3,
        "cancel did not roll back the committed account recovery"
    );
    drop(service);
    drop(host);
    server.join().unwrap();
}

#[test]
fn recovery_code_reveal_requires_current_admin_session_and_is_unavailable_on_ai_pipe() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(2), Some(12)).unwrap()
    });
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("setup", "Admin", "admin-password")
        .unwrap();
    let revealed = host.reveal_recovery_code().unwrap();
    assert_only_key(&revealed, "recovery_code");
    let recovery_code = revealed["recovery_code"].as_str().unwrap().to_owned();

    let child = host
        .manage_account(
            "account.create",
            json!({"name":"Member","password":"member-password"}),
        )
        .unwrap();
    assert_no_secret_keys(&child);

    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.login",
        json!({"name":"Admin","password":"admin-password"}),
    )
    .unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let project = call(
        &mut service,
        admin["token"].as_str(),
        "project.register",
        json!({"name":"AI account access test","root":project_root.path()}),
    )
    .unwrap();
    let ai = call(
        &mut service,
        None,
        "identity.start",
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    let ai_token = ai["session_token"].as_str().unwrap().to_owned();

    let admin_alias = format!("account-{:x}", Sha256::digest(b"Admin"));
    let gui_client = GuiClient::open(root.path()).unwrap();
    gui_client
        .request(
            &admin_alias,
            Request {
                operation: "account.logout".into(),
                auth: None,
                args: json!({}),
                context: json!({}),
                request_id: None,
            },
        )
        .unwrap();
    let invalid_session = host.reveal_recovery_code().unwrap_err();
    assert_eq!(invalid_session.exit_code, 3);
    assert!(!invalid_session.message.contains(&recovery_code));

    host.authenticate("login", "Member", "member-password")
        .unwrap();
    let non_admin = host.reveal_recovery_code().unwrap_err();
    assert_eq!(non_admin.exit_code, 4);
    assert!(!non_admin.message.contains(&recovery_code));

    let ai_pipe = swarm_runtime::pipe_name(root.path()).unwrap();
    let mut connection = pipe::connect(&ai_pipe).unwrap();
    let mut ai_request = request("gui.reveal_recovery_code", json!({"alias":admin_alias}));
    ai_request.auth = Some(ai_token.clone());
    swarm_ipc::write_request(&mut connection, &ai_request).unwrap();
    let ai_response: RpcResponse = swarm_ipc::read_response(&mut connection).unwrap();
    let ai_error = ai_response.error.unwrap();
    assert_eq!(ai_error.exit_code, 4);
    assert!(!ai_error.message.contains(&ai_token));
    assert!(!ai_error.message.contains(&recovery_code));

    drop(service);
    drop(host);
    server.join().unwrap();
}
