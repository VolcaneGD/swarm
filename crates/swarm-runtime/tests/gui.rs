#![cfg(windows)]
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use swarm_core::protocol::Request;
use swarm_ipc::RpcResponse;
use swarm_runtime::gui::GuiClient;

fn req(operation: &str, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: None,
        args,
        context: json!({}),
        request_id: None,
    }
}
fn rpc(name: &str, request: Request) -> RpcResponse {
    let mut pipe = swarm_platform::pipe::connect(name).unwrap();
    swarm_ipc::write_request(&mut pipe, &request).unwrap();
    swarm_ipc::read_response(&mut pipe).unwrap()
}
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
            return;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn gui_capability(root: &std::path::Path) -> String {
    let store = swarm_platform::CredentialStore::open(&root.join("gui-host")).unwrap();
    String::from_utf8(store.get("transport-capability").unwrap().unwrap()).unwrap()
}

#[test]
fn real_gui_pipe_bootstraps_admin_and_shares_data_without_enabling_ai_management() {
    let root = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(5), Some(15)).unwrap()
    });
    ready(root.path());
    let gui_name = swarm_runtime::gui_pipe_name(root.path()).unwrap();
    let ai_name = swarm_runtime::pipe_name(root.path()).unwrap();
    let mut wrong = req("gui.issue", json!({"password":"do-not-reflect"}));
    wrong.auth = Some("f".repeat(64));
    let denied = rpc(&gui_name, wrong);
    assert_eq!(denied.error.unwrap().exit_code, 3);
    let client = GuiClient::open(root.path()).unwrap();
    let geometry = swarm_core::WindowGeometry {
        version: 1,
        x: -140,
        y: 28,
        width: 1360,
        height: 900,
        maximized: false,
    };
    client.window_geometry_set(&geometry).unwrap();
    assert_eq!(client.window_geometry_get().unwrap(), Some(geometry));
    client.startup_preference_set(false).unwrap();
    assert_eq!(client.startup_preference_get().unwrap(), Some(false));
    assert_eq!(
        client.setup_status().unwrap(),
        json!({"setup_required":true})
    );
    let public = client
        .issue(
            "admin",
            &"a".repeat(64),
            req(
                "account.setup",
                json!({"name":"Admin","password":"fixture-password"}),
            ),
        )
        .unwrap();
    assert!(public.get("token").is_none());
    assert!(public.get("recovery_code").is_none());
    let status = client
        .request("admin", req("account.status", json!({})))
        .unwrap();
    assert_eq!(status["id"], public["id"]);
    let board = client
        .request("admin", req("board.create", json!({"name":"Board"})))
        .unwrap();
    let project = client
        .request(
            "admin",
            req(
                "project.register",
                json!({"name":"Project","root":project_root.path()}),
            ),
        )
        .unwrap();
    let boards = client
        .request("admin", req("board.list", json!({})))
        .unwrap();
    assert_eq!(boards["items"][0]["id"], board["id"]);
    assert_eq!(
        client.setup_status().unwrap(),
        json!({"setup_required":false})
    );
    let cap = gui_capability(root.path());
    let mut bypass = req(
        "account.setup",
        json!({"name":"Second","password":"hidden"}),
    );
    bypass.auth = Some(cap.clone());
    assert_eq!(rpc(&gui_name, bypass).error.unwrap().exit_code, 4);
    let mut injected = req(
        "gui.request",
        json!({"alias":"admin","request":{"operation":"account.status","auth":"injected","args":{},"context":{}}}),
    );
    injected.auth = Some(cap.clone());
    assert_eq!(rpc(&gui_name, injected).error.unwrap().exit_code, 2);
    let mut start = req(
        "session.start",
        json!({"alias":"agent","project_id":project["id"]}),
    );
    start.auth = Some("b".repeat(64));
    assert!(rpc(&ai_name, start).ok);
    let store =
        swarm_platform::CredentialStore::open(&root.path().join("client-credentials")).unwrap();
    let ai: Value = serde_json::from_slice(&store.get("agent").unwrap().unwrap()).unwrap();
    let mut read = req("board.list", json!({}));
    read.auth = ai["session_token"].as_str().map(str::to_owned);
    let ai_boards = rpc(&ai_name, read);
    assert_eq!(ai_boards.data.unwrap()["items"][0]["id"], board["id"]);
    let mut gui_on_ai = req("gui.setup_status", json!({}));
    gui_on_ai.auth = Some(cap);
    assert_eq!(rpc(&ai_name, gui_on_ai).error.unwrap().exit_code, 4);
    let mut account_on_ai = req("account.list", json!({}));
    account_on_ai.auth = ai["session_token"].as_str().map(str::to_owned);
    assert_eq!(rpc(&ai_name, account_on_ai).error.unwrap().exit_code, 4);
    server.join().unwrap();
}

#[test]
fn ui_preferences_flush_offline_with_saved_human_without_clearing_full_stop() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(2)).unwrap());
    ready(root.path());
    let client = GuiClient::open(root.path()).unwrap();
    let public = client
        .issue(
            "admin",
            &"d".repeat(64),
            req(
                "account.setup",
                json!({"name":"Admin","password":"fixture-password"}),
            ),
        )
        .unwrap();
    let credentials =
        swarm_platform::CredentialStore::open(&root.path().join("credentials")).unwrap();
    let saved_human = credentials.get("candidate-journal").unwrap().unwrap();
    server.join().unwrap();

    let lifecycle = swarm_platform::CredentialStore::open(&root.path().join("lifecycle")).unwrap();
    lifecycle
        .put("full-stop", b"1:00000000-0000-4000-8000-000000000001")
        .unwrap();
    let expected_account_id = public["id"].as_str().unwrap();
    let layout = json!({"version":1,"treeWidth":360,"composerHeight":240});
    let set = client
        .request(
            "admin",
            req(
                "ui.preferences.set",
                json!({
                    "expected_account_id":expected_account_id,
                    "scope":"layout",
                    "value":layout
                }),
            ),
        )
        .unwrap();
    assert_eq!(set, json!({"saved":true}));
    let get = client
        .request(
            "admin",
            req(
                "ui.preferences.get",
                json!({"expected_account_id":expected_account_id,"scope":"layout"}),
            ),
        )
        .unwrap();
    assert_eq!(get["value"], layout);
    assert!(
        client
            .request(
                "missing",
                req(
                    "ui.preferences.get",
                    json!({"expected_account_id":expected_account_id,"scope":"layout"}),
                ),
            )
            .is_err()
    );
    assert!(
        client
            .request(
                "admin",
                req(
                    "ui.preferences.set",
                    json!({
                        "expected_account_id":"00000000-0000-4000-8000-000000000002",
                        "scope":"layout",
                        "value":{"version":1,"treeWidth":400,"composerHeight":260}
                    }),
                ),
            )
            .is_err()
    );
    assert_eq!(
        credentials.get("candidate-journal").unwrap().unwrap(),
        saved_human,
        "offline UI preference flush must not issue or replace a credential"
    );
    assert_eq!(
        lifecycle.get("full-stop").unwrap().unwrap(),
        b"1:00000000-0000-4000-8000-000000000001"
    );
    assert_eq!(
        swarm_runtime::lifecycle::check_running_allowed(root.path())
            .unwrap_err()
            .code,
        "RUNTIME_STOPPED"
    );
}

#[test]
fn gui_client_cannot_create_a_missing_transport_capability() {
    let root = tempfile::tempdir().unwrap();
    assert!(GuiClient::open(root.path()).is_err());
    assert!(!root.path().join("gui-host").exists());
}

#[test]
fn private_gui_capability_survives_reopen_and_corruption_is_not_reset() {
    use swarm_runtime::gui::GuiAccess;
    let root = tempfile::tempdir().unwrap();
    let first = GuiAccess::open(root.path()).unwrap();
    let saved = gui_capability(root.path());
    assert!(!format!("{first:?}").contains(&saved));
    let client = GuiClient::open(root.path()).unwrap();
    assert!(!format!("{client:?}").contains(&saved));
    GuiAccess::open(root.path()).unwrap();
    assert_eq!(gui_capability(root.path()), saved);
    let store = swarm_platform::CredentialStore::open(&root.path().join("gui-host")).unwrap();
    store.put("transport-capability", b"corrupted").unwrap();
    assert!(GuiAccess::open(root.path()).is_err());
    assert!(GuiClient::open(root.path()).is_err());
    assert_eq!(
        store.get("transport-capability").unwrap().unwrap(),
        b"corrupted"
    );
}

#[test]
fn dropped_gui_issue_reply_replays_once_and_released_login_stays_released() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(6)).unwrap());
    ready(root.path());
    let cap = gui_capability(root.path());
    let name = swarm_runtime::gui_pipe_name(root.path()).unwrap();
    let setup = req(
        "account.setup",
        json!({"name":"Admin","password":"fixture-password"}),
    );
    let mut issue = req(
        "gui.issue",
        json!({"alias":"admin","intent":"c".repeat(64),"request":setup.clone()}),
    );
    issue.auth = Some(cap);
    let mut pipe = swarm_platform::pipe::connect(&name).unwrap();
    swarm_ipc::write_request(&mut pipe, &issue).unwrap();
    drop(pipe);
    let client = GuiClient::open(root.path()).unwrap();
    let public = client
        .issue("admin", &"c".repeat(64), setup.clone())
        .unwrap();
    let again = client
        .issue("admin", &"c".repeat(64), setup.clone())
        .unwrap();
    assert_eq!(again, public);
    client
        .request("admin", req("account.logout", json!({})))
        .unwrap();
    assert_eq!(
        client
            .issue("admin", &"c".repeat(64), setup)
            .unwrap_err()
            .exit_code,
        3
    );
    server.join().unwrap();
}
