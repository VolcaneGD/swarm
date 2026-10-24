#![cfg(windows)]
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
use swarm_gui_host::GuiHost;

fn request(operation: &str, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: None,
        args,
        context: json!({}),
        request_id: None,
    }
}

fn call(service: &mut Service, token: &str, operation: &str, args: Value) -> Value {
    let mut request = request(operation, args);
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
fn human_bridge_exposes_pin_mutation_and_conversation_metadata_reads() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = service
        .request(request(
            "account.setup",
            json!({"name":"Admin","password":"fixture-password"}),
        ))
        .unwrap();
    let token = admin["token"].as_str().unwrap();
    let board = call(
        &mut service,
        token,
        "board.create",
        json!({"name":"Bridge Board"}),
    );
    let thread = call(
        &mut service,
        token,
        "thread.create",
        json!({"board_id":board["id"],"name":"Bridge Thread"}),
    );
    call(
        &mut service,
        token,
        "post.create",
        json!({"thread_id":thread["id"],"body":"PRIVATE_BODY_NOT_IN_ACTIVITY"}),
    );
    drop(service);

    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(7)).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.read("conversation.pins", json!({}))
            .unwrap_err()
            .exit_code,
        3
    );
    host.authenticate("login", "Admin", "fixture-password")
        .unwrap();
    assert_eq!(
        host.manage("thread.pin", json!({"id":thread["id"],"pinned":true}),)
            .unwrap()["pinned"],
        true
    );
    let pins = host.read("conversation.pins", json!({})).unwrap();
    assert_eq!(pins["items"].as_array().unwrap().len(), 1);
    assert_eq!(pins["items"][0]["id"], thread["id"]);
    let activity = host
        .read("conversation.activity", json!({"limit":10}))
        .unwrap();
    assert!(
        activity["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["action"] == "post.create")
    );
    assert!(
        !serde_json::to_string(&activity)
            .unwrap()
            .contains("PRIVATE_BODY_NOT_IN_ACTIVITY")
    );
    drop(host);
    server.join().unwrap();
}
