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
fn selected_human_reads_real_conversation_pagination_replies_and_tombstones() {
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
        json!({"name":"研究","description":"日本語のBoard"}),
    );
    let thread = call(
        &mut service,
        token,
        "thread.create",
        json!({"board_id":board["id"],"name":"引き継ぎ"}),
    );
    for no in 1..=55 {
        call(
            &mut service,
            token,
            "post.create",
            json!({"thread_id":thread["id"],"body":format!("記録 {no}")}),
        );
    }
    call(
        &mut service,
        token,
        "response.create",
        json!({"thread_id":thread["id"],"to":1,"body":"直接のResponse"}),
    );
    call(
        &mut service,
        token,
        "response.create",
        json!({"thread_id":thread["id"],"to":56,"body":"ResponseへのResponse"}),
    );
    call(
        &mut service,
        token,
        "post.delete",
        json!({"thread_id":thread["id"],"no":2}),
    );
    drop(service);
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(16)).unwrap()
    });
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(host.read("board.list", json!({})).unwrap_err().exit_code, 3);
    host.authenticate("login", "Admin", "fixture-password")
        .unwrap();
    assert_eq!(
        host.read("board.list", json!({})).unwrap()["items"][0]["name"],
        "研究"
    );
    assert_eq!(
        host.read("thread.list", json!({})).unwrap()["items"][0]["id"],
        thread["id"]
    );
    assert_eq!(
        host.read("thread.get", json!({"id":thread["id"]})).unwrap()["name"],
        "引き継ぎ"
    );
    let latest = host
        .read("post.list", json!({"thread_id":thread["id"]}))
        .unwrap();
    assert_eq!(latest["items"].as_array().unwrap().len(), 50);
    assert_eq!(latest["items"][0]["no"], 8);
    assert_eq!(latest["items"][49]["no"], 57);
    assert_eq!(latest["next_before_no"], 8);
    let numbered_first = host
        .read(
            "post.page",
            json!({"thread_id":thread["id"],"page":1,"page_size":50}),
        )
        .unwrap();
    assert_eq!(numbered_first["page"], 1);
    assert_eq!(numbered_first["page_size"], 50);
    assert_eq!(numbered_first["total_posts"], 57);
    assert_eq!(numbered_first["total_pages"], 2);
    assert_eq!(numbered_first["items"][0]["no"], 1);
    assert_eq!(numbered_first["items"][1]["deleted"], true);
    assert!(numbered_first["items"][1]["body"].is_null());
    assert_eq!(numbered_first["items"][49]["no"], 50);
    let numbered_last = host
        .read(
            "post.page",
            json!({"thread_id":thread["id"],"page":2,"page_size":50}),
        )
        .unwrap();
    assert_eq!(numbered_last["items"][0]["no"], 51);
    assert_eq!(numbered_last["items"][6]["no"], 57);
    let older = host
        .read("post.list", json!({"thread_id":thread["id"],"before_no":8}))
        .unwrap();
    assert_eq!(older["items"].as_array().unwrap().len(), 7);
    assert_eq!(older["items"][0]["no"], 1);
    assert_eq!(older["items"][1]["deleted"], true);
    assert!(older["items"][1]["body"].is_null());
    let parent = host
        .read("post.get", json!({"thread_id":thread["id"],"no":1}))
        .unwrap();
    assert_eq!(parent["author"]["model_name"], "USER");
    assert_eq!(parent["response_count"], 1);
    let replies = host
        .read("response.list", json!({"thread_id":thread["id"],"to":1}))
        .unwrap();
    assert_eq!(replies["items"].as_array().unwrap().len(), 1);
    assert_eq!(replies["items"][0]["no"], 56);
    for operation in [
        "post.create",
        "post.deleted_body",
        "account.create",
        "identity.start",
        "account.logout",
    ] {
        assert_eq!(host.read(operation, json!({})).unwrap_err().exit_code, 2);
    }
    assert_eq!(
        host.read(
            "post.list",
            json!({"thread_id":thread["id"],"auth":"forged"})
        )
        .unwrap_err()
        .exit_code,
        2
    );
    host.logout().unwrap();
    assert_eq!(
        host.read("post.get", json!({"thread_id":thread["id"],"no":1}))
            .unwrap_err()
            .exit_code,
        3
    );
    server.join().unwrap();
}

#[test]
fn thread_access_does_not_reveal_denied_parent_and_live_revocation_blocks_reads() {
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
        json!({"name":"private-board-name"}),
    );
    let thread = call(
        &mut service,
        token,
        "thread.create",
        json!({"board_id":board["id"],"name":"shared-thread"}),
    );
    call(
        &mut service,
        token,
        "post.create",
        json!({"thread_id":thread["id"],"body":"permitted-body"}),
    );
    let reader = call(
        &mut service,
        token,
        "account.create",
        json!({"name":"Reader","password":"reader-password"}),
    );
    call(
        &mut service,
        token,
        "permission.set",
        json!({"subject_type":"account","subject_id":reader["id"],"operation":"read","scope":"board","target_id":board["id"],"decision":"deny"}),
    );
    let permission = json!({"subject_type":"account","subject_id":reader["id"],"operation":"read","scope":"thread","target_id":thread["id"],"decision":"allow"});
    call(&mut service, token, "permission.set", permission.clone());
    drop(service);
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(12)).unwrap()
    });
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Reader", "reader-password")
        .unwrap();
    assert!(
        host.read("board.list", json!({})).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let threads = host.read("thread.list", json!({})).unwrap();
    assert_eq!(threads["items"][0]["name"], "shared-thread");
    assert!(threads["items"][0]["board_id"].is_null());
    assert!(!threads.to_string().contains("private-board-name"));
    assert!(!threads.to_string().contains(board["id"].as_str().unwrap()));
    assert!(host.read("thread.get", json!({"id":thread["id"]})).unwrap()["board_id"].is_null());
    assert_eq!(
        host.read("post.list", json!({"thread_id":thread["id"]}))
            .unwrap()["items"][0]["body"],
        "permitted-body"
    );
    let client = GuiClient::open(root.path()).unwrap();
    client
        .issue(
            "admin-fixture",
            &"d".repeat(64),
            req(
                "account.login",
                json!({"name":"Admin","password":"fixture-password"}),
            ),
        )
        .unwrap();
    let mut revoked = permission;
    revoked["decision"] = json!("deny");
    client
        .request("admin-fixture", req("permission.set", revoked))
        .unwrap();
    assert_eq!(
        host.read("post.list", json!({"thread_id":thread["id"]}))
            .unwrap_err()
            .exit_code,
        4
    );
    assert!(
        host.read("thread.list", json!({})).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    server.join().unwrap();
}

#[test]
fn pending_authentication_blocks_conversation_read_without_losing_pending_state() {
    let root = tempfile::tempdir().unwrap();
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.authenticate("setup", "Admin", "fixture-password")
            .unwrap_err()
            .exit_code,
        8
    );
    assert_eq!(host.read("board.list", json!({})).unwrap_err().exit_code, 6);
    let store = swarm_platform::CredentialStore::open(&root.path().join("gui-state")).unwrap();
    assert!(store.get("pending-auth").unwrap().is_some());
}
