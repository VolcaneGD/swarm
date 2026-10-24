#![cfg(windows)]
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
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
fn prepare(root: &Path) -> (String, Value, Value) {
    let mut service = Service::open(root).unwrap();
    let admin = service
        .request(req(
            "account.setup",
            json!({"name":"Admin","password":"fixture-password"}),
        ))
        .unwrap();
    let token = admin["token"].as_str().unwrap().to_owned();
    let board = call(
        &mut service,
        &token,
        "board.create",
        json!({"name":"Management"}),
    );
    let thread = call(
        &mut service,
        &token,
        "thread.create",
        json!({"board_id":board["id"],"name":"Retained children"}),
    );
    call(
        &mut service,
        &token,
        "post.create",
        json!({"thread_id":thread["id"],"body":"Preserved original"}),
    );
    call(
        &mut service,
        &token,
        "response.create",
        json!({"thread_id":thread["id"],"to":1,"body":"Preserved reply"}),
    );
    (token, board, thread)
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
fn post_args(thread: &Value, no: u64) -> Value {
    json!({"thread_id":thread["id"],"no":no})
}

#[test]
fn gui_management_retains_tombstones_reply_counts_and_individual_child_deletions() {
    let root = tempfile::tempdir().unwrap();
    let (_, board, thread) = prepare(root.path());
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(21)).unwrap()
    });
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.manage("post.delete", post_args(&thread, 1))
            .unwrap_err()
            .exit_code,
        3
    );
    host.authenticate("login", "Admin", "fixture-password")
        .unwrap();
    for (operation, args) in [
        ("permission.set", json!({})),
        (
            "post.delete",
            json!({"thread_id":thread["id"],"no":1,"auth":"forged"}),
        ),
        ("post.restore", json!({"thread_id":thread["id"],"no":0})),
        (
            "board.delete",
            json!({"id":board["id"],"request_id":"forged"}),
        ),
    ] {
        assert_eq!(host.manage(operation, args).unwrap_err().exit_code, 2);
    }
    let caps = host.read("conversation.capabilities", json!({})).unwrap();
    assert_eq!(caps["boards"][0]["board_delete"], true);
    assert_eq!(caps["threads"][0]["post_restore"], true);
    host.manage("post.delete", post_args(&thread, 2)).unwrap();
    let parent = host.read("post.get", post_args(&thread, 1)).unwrap();
    assert_eq!(parent["response_count"], 0);
    let tombstone = host.read("post.get", post_args(&thread, 2)).unwrap();
    assert_eq!(tombstone["body"], Value::Null);
    assert_eq!(tombstone["no"], 2);
    assert_eq!(tombstone["reply_to"], 1);
    host.manage("post.restore", post_args(&thread, 2)).unwrap();
    assert_eq!(
        host.read("post.get", post_args(&thread, 1)).unwrap()["response_count"],
        1
    );
    host.manage("post.delete", post_args(&thread, 1)).unwrap();
    host.manage("thread.delete", json!({"id":thread["id"]}))
        .unwrap();
    assert_eq!(
        host.read("conversation.deleted", json!({})).unwrap()["threads"][0]["id"],
        thread["id"]
    );
    host.manage("board.delete", json!({"id":board["id"]}))
        .unwrap();
    let hidden = host.read("conversation.deleted", json!({})).unwrap();
    assert_eq!(hidden["boards"][0]["id"], board["id"]);
    assert_eq!(hidden["threads"][0]["id"], thread["id"]);
    assert_eq!(hidden["threads"][0]["parent_deleted"], true);
    host.manage("board.restore", json!({"id":board["id"]}))
        .unwrap();
    assert_eq!(
        host.read("conversation.deleted", json!({})).unwrap()["threads"][0]["id"],
        thread["id"]
    );
    host.manage("thread.restore", json!({"id":thread["id"]}))
        .unwrap();
    assert_eq!(
        host.read("post.get", post_args(&thread, 1)).unwrap()["body"],
        Value::Null
    );
    host.manage("post.restore", post_args(&thread, 1)).unwrap();
    let restored = host.read("post.get", post_args(&thread, 1)).unwrap();
    assert_eq!(restored["body"], "Preserved original");
    assert_eq!(restored["response_count"], 1);
    server.join().unwrap();
    assert_eq!(
        host.create("board.create", json!({"name":"Unconfirmed"}))
            .unwrap_err()
            .exit_code,
        8
    );
    assert_eq!(
        host.manage("post.delete", post_args(&thread, 1))
            .unwrap_err()
            .exit_code,
        6
    );
    host.discard_create().unwrap();
    assert_eq!(
        host.authenticate("login", "Admin", "fixture-password")
            .unwrap_err()
            .exit_code,
        8
    );
    assert_eq!(
        host.manage("post.restore", post_args(&thread, 1))
            .unwrap_err()
            .exit_code,
        6
    );
}

#[test]
fn gui_management_uses_current_grants_of_the_saved_human_account() {
    let root = tempfile::tempdir().unwrap();
    let (token, _, thread) = prepare(root.path());
    let mut service = Service::open(root.path()).unwrap();
    let member = call(
        &mut service,
        &token,
        "account.create",
        json!({"name":"Member","password":"member-password"}),
    );
    drop(service);
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(15)).unwrap()
    });
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Member", "member-password")
        .unwrap();
    assert_eq!(
        host.manage("post.delete", post_args(&thread, 1))
            .unwrap_err()
            .exit_code,
        4
    );
    let client = GuiClient::open(root.path()).unwrap();
    client
        .issue(
            "admin-fixture",
            &"e".repeat(64),
            req(
                "account.login",
                json!({"name":"Admin","password":"fixture-password"}),
            ),
        )
        .unwrap();
    let grant = |operation: &str, decision: &str| {
        req(
            "permission.set",
            json!({"subject_type":"account","subject_id":member["id"],"operation":operation,"scope":"thread","target_id":thread["id"],"decision":decision}),
        )
    };
    client
        .request("admin-fixture", grant("post.delete", "allow"))
        .unwrap();
    assert_eq!(
        host.read("conversation.capabilities", json!({})).unwrap()["threads"][0]["post_delete"],
        true
    );
    host.manage("post.delete", post_args(&thread, 1)).unwrap();
    client
        .request("admin-fixture", grant("post.delete", "deny"))
        .unwrap();
    assert_eq!(
        host.manage("post.delete", post_args(&thread, 2))
            .unwrap_err()
            .exit_code,
        4
    );
    let tombstone = host.read("post.get", post_args(&thread, 1)).unwrap();
    assert_eq!(tombstone["body"], Value::Null);
    client
        .request("admin-fixture", grant("post.restore", "allow"))
        .unwrap();
    client
        .request("admin-fixture", grant("post.get", "deny"))
        .unwrap();
    let refused = host
        .manage("post.restore", post_args(&thread, 1))
        .unwrap_err();
    assert_eq!(refused.exit_code, 4);
    assert!(!refused.message.contains("Preserved original"));
    server.join().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    assert_eq!(
        call(&mut service, &token, "post.get", post_args(&thread, 1))["deleted"],
        true
    );
}

#[test]
fn gui_can_refresh_after_a_delete_commits_without_a_response() {
    let root = tempfile::tempdir().unwrap();
    let (token, _, thread) = prepare(root.path());
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(8)).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Admin", "fixture-password")
        .unwrap();
    let store = swarm_platform::CredentialStore::open(&root.path().join("gui-host")).unwrap();
    let capability =
        String::from_utf8(store.get("transport-capability").unwrap().unwrap()).unwrap();
    let alias = format!("account-{:x}", Sha256::digest(b"Admin"));
    let mut outer = req(
        "gui.request",
        json!({"alias":alias,"request":req("post.delete", post_args(&thread, 1))}),
    );
    outer.auth = Some(capability);
    let mut pipe =
        swarm_platform::pipe::connect(&swarm_runtime::gui_pipe_name(root.path()).unwrap()).unwrap();
    swarm_ipc::write_request(&mut pipe, &outer).unwrap();
    drop(pipe);
    let mut reader = Service::open(root.path()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if call(&mut reader, &token, "post.get", post_args(&thread, 1))["deleted"] == true {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let latest = host.read("post.get", post_args(&thread, 1)).unwrap();
    assert_eq!(latest["deleted"], true);
    assert_eq!(latest["body"], Value::Null);
    assert_eq!(latest["revision"], 2);
    let restored = host.manage("post.restore", post_args(&thread, 1)).unwrap();
    assert_eq!(restored["revision"], 3);
    assert_eq!(
        host.read("post.get", post_args(&thread, 1)).unwrap()["body"],
        "Preserved original"
    );
    server.join().unwrap();
}
