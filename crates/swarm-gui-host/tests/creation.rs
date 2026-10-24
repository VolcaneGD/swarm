#![cfg(windows)]
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
use swarm_gui_host::GuiHost;

fn req(operation: &str, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: None,
        args,
        context: json!({}),
        request_id: None,
    }
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
fn prepare(root: &Path) {
    Service::open(root)
        .unwrap()
        .request(req(
            "account.setup",
            json!({"name":"Admin","password":"fixture-password"}),
        ))
        .unwrap();
}

fn created(host: &mut GuiHost, operation: &str, args: Value) -> Value {
    let value = host.create(operation, args).unwrap();
    assert_eq!(host.creation_status().unwrap()["pending"], true);
    assert_eq!(host.acknowledge_create().unwrap()["pending"], false);
    value
}

#[test]
fn gui_creates_boards_threads_posts_and_responses_as_selected_human() {
    let root = tempfile::tempdir().unwrap();
    prepare(root.path());
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(12)).unwrap()
    });
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.create("board.create", json!({"name":"未認証"}))
            .unwrap_err()
            .exit_code,
        3
    );
    host.authenticate("login", "Admin", "fixture-password")
        .unwrap();
    let board = created(
        &mut host,
        "board.create",
        json!({"name":"連絡","description":"人間から作成"}),
    );
    let thread = created(
        &mut host,
        "thread.create",
        json!({"board_id":board["id"],"name":"レビュー"}),
    );
    let post = created(
        &mut host,
        "post.create",
        json!({"thread_id":thread["id"],"body":"本文と **Markdown**"}),
    );
    assert_eq!(post["no"], 1);
    assert_eq!(post["author"]["model_name"], "USER");
    let response = created(
        &mut host,
        "response.create",
        json!({"thread_id":thread["id"],"to":1,"body":"返信"}),
    );
    assert_eq!(response["no"], 2);
    assert_eq!(response["reply_to"], 1);
    let posts = host
        .read("post.list", json!({"thread_id":thread["id"]}))
        .unwrap();
    assert_eq!(posts["items"][0]["response_count"], 1);
    assert_eq!(posts["items"].as_array().unwrap().len(), 2);
    assert_eq!(
        host.read("board.list", json!({})).unwrap()["items"][0]["name"],
        "連絡"
    );
    assert_eq!(host.creation_status().unwrap()["pending"], false);
    for operation in ["post.delete", "account.create", "identity.start"] {
        assert_eq!(host.create(operation, json!({})).unwrap_err().exit_code, 2);
    }
    assert_eq!(
        host.create(
            "post.create",
            json!({"thread_id":thread["id"],"body":"forged","auth":"secret"})
        )
        .unwrap_err()
        .exit_code,
        2
    );
    host.logout().unwrap();
    server.join().unwrap();
}

#[test]
fn maximum_scalar_body_with_json_escapes_is_durable_and_keeps_the_platform_store_limit() {
    let root = tempfile::tempdir().unwrap();
    prepare(root.path());
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(8)).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Admin", "fixture-password")
        .unwrap();
    let board = created(&mut host, "board.create", json!({"name":"Boundary"}));
    let thread = created(
        &mut host,
        "thread.create",
        json!({"board_id":board["id"],"name":"Body"}),
    );
    let body = "\u{1}".repeat(100_000);
    let post = created(
        &mut host,
        "post.create",
        json!({"thread_id":thread["id"],"body":body}),
    );
    assert_eq!(post["body"].as_str().unwrap().len(), 100_000);
    assert_eq!(
        host.read("post.get", json!({"thread_id":thread["id"],"no":1}))
            .unwrap()["body"],
        post["body"]
    );
    assert_eq!(
        host.create(
            "post.create",
            json!({"thread_id":thread["id"],"body":"a".repeat(1024*1024+1)})
        )
        .unwrap_err()
        .exit_code,
        2
    );
    assert_eq!(host.creation_status().unwrap()["pending"], false);
    server.join().unwrap();
}

#[test]
fn lost_transport_and_renderer_results_replay_one_post_after_host_reopen() {
    let root = tempfile::tempdir().unwrap();
    prepare(root.path());
    let mut service = Service::open(root.path()).unwrap();
    let login = service
        .request(req(
            "account.login",
            json!({"name":"Admin","password":"fixture-password"}),
        ))
        .unwrap();
    let mut board_request = req("board.create", json!({"name":"Recovery"}));
    board_request.auth = Some(login["token"].as_str().unwrap().into());
    let board = service.request(board_request).unwrap();
    let mut thread_request = req(
        "thread.create",
        json!({"board_id":board["id"],"name":"Replay"}),
    );
    thread_request.auth = Some(login["token"].as_str().unwrap().into());
    let thread = service.request(thread_request).unwrap();
    drop(service);
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(4)).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Admin", "fixture-password")
        .unwrap();
    server.join().unwrap();
    let args = json!({"thread_id":thread["id"],"body":"durable-replay-sentinel"});
    assert_eq!(
        host.create("post.create", args.clone())
            .unwrap_err()
            .exit_code,
        8
    );
    assert_eq!(
        host.create("post.create", args.clone())
            .unwrap_err()
            .exit_code,
        6
    );
    assert_eq!(host.logout().unwrap_err().exit_code, 6);
    assert_eq!(host.acknowledge_create().unwrap_err().exit_code, 6);
    let status = host.creation_status().unwrap();
    assert_eq!(status["pending"], true);
    assert!(status.get("args").is_none());
    assert!(status.get("request_id").is_none());
    let store = swarm_platform::CredentialStore::open(&root.path().join("gui-state")).unwrap();
    let manifest: Value =
        serde_json::from_slice(&store.get("pending-create").unwrap().unwrap()).unwrap();
    let mut bytes = Vec::new();
    for index in 0..manifest["chunks"].as_u64().unwrap() {
        let key = format!(
            "pending-create-{}-{index:03}",
            manifest["group"].as_str().unwrap()
        );
        bytes.extend(store.get(&key).unwrap().unwrap());
    }
    let pending: Value = serde_json::from_slice(&bytes).unwrap();
    drop(host);
    let mut host = GuiHost::open(root.path()).unwrap();
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(5)).unwrap());
    ready(root.path());
    let capability_store =
        swarm_platform::CredentialStore::open(&root.path().join("gui-host")).unwrap();
    let capability = String::from_utf8(
        capability_store
            .get("transport-capability")
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let mut inner = req("post.create", args);
    inner.request_id = Some(pending["request_id"].as_str().unwrap().into());
    let mut outer = req(
        "gui.request",
        json!({"alias":pending["alias"],"request":inner}),
    );
    outer.auth = Some(capability);
    let mut pipe =
        swarm_platform::pipe::connect(&swarm_runtime::gui_pipe_name(root.path()).unwrap()).unwrap();
    swarm_ipc::write_request(&mut pipe, &outer).unwrap();
    drop(pipe); // The server may commit, but no response is received.
    assert_eq!(host.retry_create().unwrap()["no"], 1);
    assert_eq!(host.creation_status().unwrap()["pending"], true);
    drop(host); // Also lose the renderer result before it acknowledges completion.
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(host.retry_create().unwrap()["no"], 1);
    let posts = host
        .read("post.list", json!({"thread_id":thread["id"]}))
        .unwrap();
    assert_eq!(posts["items"].as_array().unwrap().len(), 1);
    assert_eq!(posts["items"][0]["body"], "durable-replay-sentinel");
    assert_eq!(host.acknowledge_create().unwrap()["pending"], false);
    server.join().unwrap();
}

#[test]
fn gui_creation_rechecks_human_permissions_and_discard_does_not_create_data() {
    let root = tempfile::tempdir().unwrap();
    prepare(root.path());
    let mut service = Service::open(root.path()).unwrap();
    let login = service
        .request(req(
            "account.login",
            json!({"name":"Admin","password":"fixture-password"}),
        ))
        .unwrap();
    let mut user = req(
        "account.create",
        json!({"name":"Member","password":"member-password"}),
    );
    user.auth = Some(login["token"].as_str().unwrap().into());
    service.request(user).unwrap();
    drop(service);
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(6)).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Member", "member-password")
        .unwrap();
    assert_eq!(
        host.create("board.create", json!({"name":"Denied"}))
            .unwrap_err()
            .exit_code,
        4
    );
    assert_eq!(host.creation_status().unwrap()["pending"], true);
    assert_eq!(host.logout().unwrap_err().exit_code, 6);
    assert_eq!(host.discard_create().unwrap()["pending"], false);
    assert!(
        host.read("board.list", json!({})).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    server.join().unwrap();
}
