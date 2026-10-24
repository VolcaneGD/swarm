#![cfg(windows)]

use serde_json::Value;
use std::time::{Duration, Instant};
use swarm_gui_host::GuiHost;
use swarm_platform::CredentialStore;

fn registration(root: &std::path::Path) -> swarm_platform::startup::Registration {
    swarm_platform::startup::Registration::new(&std::env::current_exe().unwrap(), root).unwrap()
}

fn wait_until_ready(root: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    for endpoint in [
        swarm_runtime::pipe_name(root).unwrap(),
        swarm_runtime::gui_pipe_name(root).unwrap(),
    ] {
        loop {
            if let Ok(connection) = swarm_platform::pipe::connect(&endpoint) {
                drop(connection);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "runtime endpoint did not become ready"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn status_is_read_only_and_reports_an_unlaunched_runtime() {
    let root = tempfile::tempdir().unwrap();
    let host = GuiHost::open(root.path()).unwrap();

    assert_eq!(
        host.runtime_status().unwrap(),
        serde_json::json!({"state":"not_running"})
    );
    assert!(
        CredentialStore::open(&root.path().join("gui-state"))
            .unwrap()
            .get("pending-auth")
            .unwrap()
            .is_none()
    );
}

#[test]
fn windows_startup_status_is_read_only_and_default_enabled() {
    let root = tempfile::tempdir().unwrap();
    let registration = registration(root.path());
    assert!(!registration.is_registered().unwrap());

    let status =
        swarm_runtime::startup::status(root.path(), &std::env::current_exe().unwrap()).unwrap();
    assert_eq!(
        status,
        serde_json::json!({"enabled":true,"registered":false,"configured":false})
    );
    assert!(!root.path().join("lifecycle").exists());
    assert!(!registration.is_registered().unwrap());
}

#[test]
fn startup_configuration_without_selection_is_rejected_before_registration() {
    let root = tempfile::tempdir().unwrap();
    let host = GuiHost::open(root.path()).unwrap();
    let registration = registration(root.path());
    assert!(!registration.is_registered().unwrap());

    let error = host.set_startup(true).unwrap_err();
    assert_eq!(error.exit_code, 3);
    assert_eq!(error.code, "AUTHENTICATION_REQUIRED");
    assert!(!root.path().join("lifecycle").exists());
    assert!(!registration.is_registered().unwrap());
}

#[test]
fn startup_configuration_is_rejected_before_registration_while_auth_is_pending() {
    let root = tempfile::tempdir().unwrap();
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.authenticate("setup", "StartupPendingAdmin", "fixture-password")
            .unwrap_err()
            .exit_code,
        8
    );
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    let pending_before = store.get("pending-auth").unwrap().unwrap();
    let registration = registration(root.path());
    assert!(!registration.is_registered().unwrap());

    let error = host.set_startup(false).unwrap_err();
    assert_eq!(error.exit_code, 6);
    assert_eq!(error.code, "CONFLICT");
    assert_eq!(store.get("pending-auth").unwrap().unwrap(), pending_before);
    assert!(!root.path().join("lifecycle").exists());
    assert!(!registration.is_registered().unwrap());
}

#[test]
fn startup_configuration_is_rejected_before_registration_while_create_is_pending() {
    let root = tempfile::tempdir().unwrap();
    let mut host = GuiHost::open(root.path()).unwrap();
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    store
        .put(
            "selection",
            br#"{"alias":"account-0000000000000000000000000000000000000000000000000000000000000000"}"#,
        )
        .unwrap();

    assert_eq!(
        host.create(
            "board.create",
            serde_json::json!({"name":"Startup pending"})
        )
        .unwrap_err()
        .exit_code,
        8
    );
    let pending_before = store.get("pending-create").unwrap().unwrap();
    let registration = registration(root.path());
    assert!(!registration.is_registered().unwrap());

    let error = host.set_startup(true).unwrap_err();
    assert_eq!(error.exit_code, 6);
    assert_eq!(error.code, "CONFLICT");
    assert_eq!(
        store.get("pending-create").unwrap().unwrap(),
        pending_before
    );
    assert!(!root.path().join("lifecycle").exists());
    assert!(!registration.is_registered().unwrap());
}

#[test]
fn startup_configuration_requires_the_selected_account_to_remain_authorized() {
    let root = tempfile::tempdir().unwrap();
    let runtime_root = root.path().to_owned();
    let runtime = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&runtime_root, None, None).unwrap()
    });
    wait_until_ready(root.path());

    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("setup", "StartupPermissionAdmin", "fixture-password")
        .unwrap();
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    let selection: Value =
        serde_json::from_slice(&store.get("selection").unwrap().unwrap()).unwrap();
    let selected_alias = selection["alias"].as_str().unwrap();
    swarm_runtime::gui::GuiClient::open(root.path())
        .unwrap()
        .request(
            selected_alias,
            swarm_core::protocol::Request {
                operation: "account.logout".into(),
                auth: None,
                args: serde_json::json!({}),
                context: serde_json::json!({}),
                request_id: None,
            },
        )
        .unwrap();

    let registration = registration(root.path());
    assert!(!registration.is_registered().unwrap());
    let startup_store = CredentialStore::open(&root.path().join("lifecycle")).unwrap();
    assert!(startup_store.get("windows-startup").unwrap().is_none());
    let error = host.set_startup(true).unwrap_err();
    assert_eq!(error.exit_code, 3);
    assert_eq!(error.code, "AUTHENTICATION_REQUIRED");
    assert!(startup_store.get("windows-startup").unwrap().is_none());
    assert!(!registration.is_registered().unwrap());

    host.authenticate("login", "StartupPermissionAdmin", "fixture-password")
        .unwrap();
    host.stop_runtime().unwrap();
    runtime.join().unwrap();
}

#[test]
fn complete_stop_requires_a_selected_human_session() {
    let root = tempfile::tempdir().unwrap();
    let host = GuiHost::open(root.path()).unwrap();

    let error = host.stop_runtime().unwrap_err();
    assert_eq!(error.exit_code, 3);
    assert_eq!(error.code, "AUTHENTICATION_REQUIRED");
    assert_eq!(
        host.runtime_status().unwrap(),
        serde_json::json!({"state":"not_running"})
    );
}

#[test]
fn complete_stop_is_blocked_without_discarding_pending_authentication() {
    let root = tempfile::tempdir().unwrap();
    let mut host = GuiHost::open(root.path()).unwrap();

    // With no runtime, an attempted setup leaves its encrypted request pending.
    assert_eq!(
        host.authenticate("setup", "LifecycleAdmin", "fixture-password")
            .unwrap_err()
            .exit_code,
        8
    );
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    let pending_before = store.get("pending-auth").unwrap().unwrap();
    let error = host.stop_runtime().unwrap_err();

    assert_eq!(error.exit_code, 6);
    assert_eq!(error.code, "CONFLICT");
    assert_eq!(store.get("pending-auth").unwrap().unwrap(), pending_before);
    assert_eq!(
        host.runtime_status().unwrap(),
        serde_json::json!({"state":"not_running"})
    );
}

#[test]
fn complete_stop_is_blocked_without_discarding_pending_creation() {
    let root = tempfile::tempdir().unwrap();
    let mut host = GuiHost::open(root.path()).unwrap();
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    store
        .put(
            "selection",
            br#"{"alias":"account-0000000000000000000000000000000000000000000000000000000000000000"}"#,
        )
        .unwrap();

    // The valid local selection lets create persist its request before its IPC
    // attempt fails against the deliberately unlaunched runtime.
    assert_eq!(
        host.create("board.create", serde_json::json!({"name":"Pending"}))
            .unwrap_err()
            .exit_code,
        8
    );
    assert_eq!(host.creation_status().unwrap()["pending"], true);
    let pending_before = store.get("pending-create").unwrap().unwrap();
    let error = host.stop_runtime().unwrap_err();

    assert_eq!(error.exit_code, 6);
    assert_eq!(error.code, "CONFLICT");
    assert_eq!(
        store.get("pending-create").unwrap().unwrap(),
        pending_before
    );
    assert_eq!(
        host.runtime_status().unwrap(),
        serde_json::json!({"state":"not_running"})
    );
}

#[test]
fn status_shape_contains_no_runtime_path_or_credentials() {
    let root = tempfile::tempdir().unwrap();
    let host = GuiHost::open(root.path()).unwrap();
    let status: Value = host.runtime_status().unwrap();

    assert_eq!(status.as_object().unwrap().len(), 1);
    assert!(status["state"].as_str().is_some());
    assert!(
        !status
            .to_string()
            .contains(root.path().to_string_lossy().as_ref())
    );
    assert!(!status.to_string().contains("token"));
    assert!(!status.to_string().contains("password"));
}
