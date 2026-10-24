#![cfg(windows)]
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
use swarm_gui_host::GuiHost;
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
fn call(service: &mut Service, token: &str, operation: &str, args: Value) -> Value {
    let mut request = req(operation, args);
    request.auth = Some(token.into());
    service.request(request).unwrap()
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

#[test]
fn selected_human_capabilities_follow_live_grants_without_authorizing_stale_writes() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = service
        .request(req(
            "account.setup",
            json!({"name":"Admin","password":"fixture-password"}),
        ))
        .unwrap();
    let token = admin["token"].as_str().unwrap();
    let board = call(
        &mut service,
        token,
        "board.create",
        json!({"name":"Visible"}),
    );
    let thread = call(
        &mut service,
        token,
        "thread.create",
        json!({"board_id":board["id"],"name":"Conversation"}),
    );
    let member = call(
        &mut service,
        token,
        "account.create",
        json!({"name":"Member","password":"member-password"}),
    );
    drop(service);
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(13)).unwrap()
    });
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.read("conversation.capabilities", json!({}))
            .unwrap_err()
            .exit_code,
        3
    );
    host.authenticate("login", "Member", "member-password")
        .unwrap();
    let before = host.read("conversation.capabilities", json!({})).unwrap();
    assert_eq!(before["board_create"], false);
    assert_eq!(
        before["boards"][0],
        json!({"id":board["id"],"thread_create":false,"board_delete":false,"search":true})
    );
    assert_eq!(
        before["threads"][0],
        json!({"id":thread["id"],"post_read":true,"search":true,"response_read":true,"post_create":true,"response_create":true,"post_edit":false,"post_history":true,"thread_delete":false,"post_delete":false,"post_restore":false})
    );
    assert!(!before.to_string().contains("member-password"));
    assert_eq!(
        host.read(
            "conversation.capabilities",
            json!({"subject_id":admin["id"]})
        )
        .unwrap_err()
        .exit_code,
        2
    );
    let client = GuiClient::open(root.path()).unwrap();
    client
        .issue(
            "admin-fixture",
            &"c".repeat(64),
            req(
                "account.login",
                json!({"name":"Admin","password":"fixture-password"}),
            ),
        )
        .unwrap();
    let mut permission = json!({"subject_type":"account","subject_id":member["id"],"operation":"board.create","scope":"global","target_id":"","decision":"allow"});
    client
        .request("admin-fixture", req("permission.set", permission.clone()))
        .unwrap();
    assert_eq!(
        host.read("conversation.capabilities", json!({})).unwrap()["board_create"],
        true
    );
    permission["decision"] = json!("deny");
    client
        .request("admin-fixture", req("permission.set", permission))
        .unwrap();
    assert_eq!(
        host.read("conversation.capabilities", json!({})).unwrap()["board_create"],
        false
    );
    assert_eq!(
        host.create("board.create", json!({"name":"Stale UI"}))
            .unwrap_err()
            .exit_code,
        4
    );
    assert_eq!(host.creation_status().unwrap()["pending"], true);
    assert_eq!(
        host.read("conversation.capabilities", json!({})).unwrap()["board_create"],
        false
    );
    assert_eq!(host.discard_create().unwrap()["pending"], false);
    server.join().unwrap();
}
