#![cfg(windows)]
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use swarm_gui_host::GuiHost;
use swarm_platform::CredentialStore;

fn ready(root: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(name) = swarm_runtime::pipe_name(root)
            && let Ok(pipe) = swarm_platform::pipe::connect(&name)
        {
            drop(pipe);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let name = swarm_runtime::gui_pipe_name(root).unwrap();
    loop {
        if let Ok(pipe) = swarm_platform::pipe::connect(&name) {
            drop(pipe);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[cfg(feature = "google-recovery")]
fn google_link_checks_private_account_and_password_before_opening_browser() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, None, None).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    let setup = host
        .authenticate("setup", "Google Admin", "current-password")
        .unwrap();
    let id = setup["account"]["id"].as_str().unwrap();
    assert!(
        host.google_link("other-account", "current-password", |_| panic!(
            "stale account opened browser"
        ))
        .is_err()
    );
    assert!(
        host.google_link(id, "wrong", |_| panic!("wrong password opened browser"))
            .is_err()
    );
    let mut opened = false;
    assert!(
        host.google_link(id, "current-password", |url| {
            opened = true;
            assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
            assert!(url.contains("code_challenge_method=S256"));
            assert!(!url.contains("current-password"));
            Err(swarm_core::protocol::Fault::new(
                "TEST_CANCEL",
                3,
                "Cancelled fixture",
            ))
        })
        .is_err()
    );
    assert!(opened);
    assert_eq!(host.snapshot().unwrap()["account"]["google_linked"], false);
    assert!(host.google_unlink(id, "wrong").is_err());
    assert_eq!(
        host.google_unlink(id, "current-password").unwrap()["account"]["google_linked"],
        false
    );
    assert!(
        host.google_recover("Google Admin", "replacement", |_| panic!(
            "unlinked account opened browser"
        ))
        .is_err()
    );
    host.stop_runtime().unwrap();
    server.join().unwrap();
}

#[test]
#[cfg(not(feature = "google-recovery"))]
fn beta_google_commands_never_open_a_browser_or_create_pending_auth() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, None, None).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    let snapshot = host
        .authenticate("setup", "Beta Admin", "fixture-password")
        .unwrap();
    let id = snapshot["account"]["id"].as_str().unwrap();
    let mut opened = false;
    let link = host.google_link(id, "fixture-password", |_| {
        opened = true;
        Err(swarm_core::protocol::Fault::new(
            "TEST_CANCEL",
            3,
            "Test cancelled",
        ))
    });
    let recover = host.google_recover("Beta Admin", "new-fixture-password", |_| {
        opened = true;
        Err(swarm_core::protocol::Fault::new(
            "TEST_CANCEL",
            3,
            "Test cancelled",
        ))
    });
    let unlink = host.google_unlink(id, "fixture-password");
    let after = host.snapshot().unwrap();
    host.stop_runtime().unwrap();
    server.join().unwrap();
    assert!(!opened, "Beta must not launch Google authentication");
    for result in [link, recover, unlink] {
        assert_eq!(result.unwrap_err().code, "FEATURE_UNAVAILABLE");
    }
    assert_eq!(after["account"], snapshot["account"]);
    assert!(
        CredentialStore::open(&root.path().join("gui-state"))
            .unwrap()
            .get("pending-auth")
            .unwrap()
            .is_none()
    );
}

#[test]
#[cfg(not(feature = "google-recovery"))]
fn beta_google_rejects_before_host_selection_or_runtime_access() {
    let root = tempfile::tempdir().unwrap();
    let mut host = GuiHost::open(root.path()).unwrap();
    let link = host.google_link("missing-account", "", |_| {
        panic!("Google browser must stay closed")
    });
    let recover = host.google_recover("", "", |_| panic!("Google browser must stay closed"));
    let unlink = host.google_unlink("missing-account", "");
    for result in [link, recover, unlink] {
        assert_eq!(result.unwrap_err().code, "FEATURE_UNAVAILABLE");
    }
    assert!(!root.path().join("swarm.sqlite3").exists());
}

#[test]
fn desktop_host_skips_password_through_private_pipe_and_can_sign_in_again() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, None, None).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    let setup = host.authenticate("setup", "Unlocked Admin", "").unwrap();
    assert_eq!(setup["account"]["is_admin"], true);
    assert_eq!(setup["account"]["lock_enabled"], false);
    assert!(!setup.to_string().contains("token"));
    let id = setup["account"]["id"].clone();
    drop(host);
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(host.snapshot().unwrap()["account"]["id"], id);
    assert!(host.logout().unwrap()["account"].is_null());
    assert_eq!(
        host.authenticate("login", "Unlocked Admin", "").unwrap()["account"]["id"],
        id
    );
    host.stop_runtime().unwrap();
    server.join().unwrap();
}

#[test]
fn password_lock_changes_recheck_account_and_replay_the_same_private_intent() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, None, None).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    let account = host
        .authenticate("setup", "Admin", "fixture-password")
        .unwrap();
    let id = account["account"]["id"].as_str().unwrap();
    assert!(
        host.set_password("stale-account", "fixture-password", "")
            .is_err()
    );
    assert_eq!(host.set_password(id, "wrong", "").unwrap_err().exit_code, 3);
    assert_eq!(host.snapshot().unwrap()["account"]["lock_enabled"], true);
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    let pending = serde_json::to_vec(&json!({"version":1,"intent":"b".repeat(64),"alias":format!("account-{:x}",Sha256::digest(b"Admin")),"request":{"operation":"account.password","auth":null,"args":{"name":"Admin","current_password":"fixture-password","password":""},"context":{},"request_id":null}})).unwrap();
    store.put("pending-auth", &pending).unwrap();
    assert_eq!(
        host.retry_pending().unwrap()["account"]["lock_enabled"],
        false
    );
    // Simulate a lost response by retaining the exact journal, then reopen.
    store.put("pending-auth", &pending).unwrap();
    drop(host);
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.retry_pending().unwrap()["account"]["lock_enabled"],
        false
    );
    assert_eq!(
        host.set_password(id, "", "new-password").unwrap()["account"]["lock_enabled"],
        true
    );
    host.stop_runtime().unwrap();
    server.join().unwrap();
}

#[test]
fn desktop_host_sets_up_preserves_login_and_requires_login_after_logout() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(14)).unwrap()
    });
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(host.snapshot().unwrap()["setupRequired"], true);
    let setup = host
        .authenticate("setup", "Admin", "fixture-password")
        .unwrap();
    assert_eq!(setup["account"]["name"], "Admin");
    let id = setup["account"]["id"].clone();
    assert!(!setup.to_string().contains("fixture-password"));
    assert!(!setup.to_string().contains("token"));
    drop(host);
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(host.snapshot().unwrap()["account"]["id"], id);
    assert!(host.logout().unwrap()["account"].is_null());
    assert!(host.snapshot().unwrap()["account"].is_null());
    assert_eq!(
        host.authenticate("login", "Admin", "wrong-password")
            .unwrap_err()
            .exit_code,
        3
    );
    assert!(
        CredentialStore::open(&root.path().join("gui-state"))
            .unwrap()
            .get("pending-auth")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        host.authenticate("login", "Admin", "fixture-password")
            .unwrap()["account"]["id"],
        id
    );
    server.join().unwrap();
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    let selected = store.get("selection").unwrap().unwrap();
    assert_eq!(host.logout().unwrap_err().exit_code, 8);
    assert_eq!(store.get("selection").unwrap().unwrap(), selected);
}

#[test]
fn logout_clears_selection_when_core_confirms_saved_session_is_revoked() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(8)).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("setup", "Admin", "fixture-password")
        .unwrap();
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    let selected: Value =
        serde_json::from_slice(&store.get("selection").unwrap().unwrap()).unwrap();
    let client = swarm_runtime::gui::GuiClient::open(root.path()).unwrap();
    client
        .request(
            selected["alias"].as_str().unwrap(),
            swarm_core::protocol::Request {
                operation: "account.logout".into(),
                auth: None,
                args: serde_json::json!({}),
                context: serde_json::json!({}),
                request_id: None,
            },
        )
        .unwrap();
    assert!(host.logout().unwrap()["account"].is_null());
    assert!(store.get("selection").unwrap().is_none());
    assert!(host.snapshot().unwrap()["account"].is_null());
    server.join().unwrap();
}

#[test]
fn unavailable_runtime_keeps_encrypted_pending_input_and_replays_after_restart() {
    let root = tempfile::tempdir().unwrap();
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.authenticate("setup", "Admin", "fixture-password")
            .unwrap_err()
            .exit_code,
        8
    );
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    let pending = store.get("pending-auth").unwrap().unwrap();
    assert_eq!(
        host.authenticate("setup", "Admin", "changed")
            .unwrap_err()
            .exit_code,
        6
    );
    assert_eq!(store.get("pending-auth").unwrap().unwrap(), pending);
    drop(host);
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(5)).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(host.snapshot().unwrap()["pendingIssue"], true);
    let restored = host.retry_pending().unwrap();
    assert_eq!(restored["account"]["name"], "Admin");
    assert_eq!(restored["pendingIssue"], false);
    assert!(store.get("pending-auth").unwrap().is_none());
    server.join().unwrap();
}

#[test]
fn desktop_state_is_exclusive_and_corrupt_pending_is_not_discarded() {
    let root = tempfile::tempdir().unwrap();
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(GuiHost::open(root.path()).unwrap_err().exit_code, 6);
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    store
        .put("pending-auth", br#"{"password":"private"}"#)
        .unwrap();
    assert_eq!(
        host.authenticate("setup", "Admin", "fixture-password")
            .unwrap_err()
            .exit_code,
        8
    );
    assert_eq!(
        store.get("pending-auth").unwrap().unwrap(),
        br#"{"password":"private"}"#
    );
    assert!(!format!("{host:?}").contains(root.path().to_string_lossy().as_ref()));
    drop(host);
    GuiHost::open(root.path()).unwrap();
}

#[test]
fn pending_credentials_are_ciphertext_and_reject_unknown_modes_without_state_changes() {
    let root = tempfile::tempdir().unwrap();
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.authenticate("raw-operation", "Admin", "secret-password")
            .unwrap_err()
            .exit_code,
        2
    );
    assert_eq!(
        host.authenticate("setup", "Admin", "secret-password")
            .unwrap_err()
            .exit_code,
        8
    );
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    let pending: Value =
        serde_json::from_slice(&store.get("pending-auth").unwrap().unwrap()).unwrap();
    assert_eq!(pending["request"]["args"]["password"], "secret-password");
    for entry in std::fs::read_dir(root.path().join("gui-state")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "bin") {
            let bytes = std::fs::read(path).unwrap();
            assert!(
                !bytes
                    .windows(b"secret-password".len())
                    .any(|v| v == b"secret-password")
            );
        }
    }
}
