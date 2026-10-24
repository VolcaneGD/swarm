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
        json!({"name":"Editing"}),
    );
    let thread = call(
        &mut service,
        &token,
        "thread.create",
        json!({"board_id":board["id"],"name":"Versions"}),
    );
    let post = call(
        &mut service,
        &token,
        "post.create",
        json!({"thread_id":thread["id"],"body":"元の本文"}),
    );
    (token, thread, post)
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
fn edit_args(thread: &Value, no: u64, body: &str, revision: u64) -> Value {
    json!({"thread_id":thread["id"],"no":no,"body":body,"if_revision":revision})
}

#[test]
fn gui_edits_post_and_response_without_changing_author_number_or_reply_target() {
    let root = tempfile::tempdir().unwrap();
    let (token, thread, original) = prepare(root.path());
    let mut service = Service::open(root.path()).unwrap();
    let response = call(
        &mut service,
        &token,
        "response.create",
        json!({"thread_id":thread["id"],"to":1,"body":"返信の元本文"}),
    );
    drop(service);
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(10)).unwrap()
    });
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.edit(edit_args(&thread, 1, "Edited", 1))
            .unwrap_err()
            .exit_code,
        3
    );
    host.authenticate("login", "Admin", "fixture-password")
        .unwrap();
    let edited = host
        .edit(edit_args(&thread, 1, "編集後 **Markdown**", 1))
        .unwrap();
    assert_eq!(edited["revision"], 2);
    assert_eq!(edited["author"], original["author"]);
    assert_eq!(edited["no"], original["no"]);
    assert_eq!(edited["reply_to"], original["reply_to"]);
    let history = host
        .read("post.history", json!({"thread_id":thread["id"],"no":1}))
        .unwrap();
    assert_eq!(history["items"][0]["body"], "元の本文");
    assert_eq!(history["items"][0]["revision"], 1);
    assert_eq!(history["items"][0]["editor_id"], edited["edited_by"]);
    assert_eq!(
        host.edit(edit_args(&thread, 1, "Stale overwrite", 1))
            .unwrap_err()
            .exit_code,
        6
    );
    let unchanged = host
        .read("post.history", json!({"thread_id":thread["id"],"no":1}))
        .unwrap();
    assert_eq!(unchanged["items"].as_array().unwrap().len(), 1);
    let edited_response = host.edit(edit_args(&thread, 2, "返信を編集", 1)).unwrap();
    assert_eq!(edited_response["author"], response["author"]);
    assert_eq!(edited_response["reply_to"], 1);
    assert_eq!(
        host.read("post.get", json!({"thread_id":thread["id"],"no":1}))
            .unwrap()["body"],
        "編集後 **Markdown**"
    );
    for bad in [
        json!({"thread_id":thread["id"],"no":1,"body":"forged","if_revision":2,"auth":"private"}),
        edit_args(&thread, 1, " ", 2),
        edit_args(&thread, 1, &"a".repeat(1024 * 1024 + 1), 2),
    ] {
        assert_eq!(host.edit(bad).unwrap_err().exit_code, 2);
    }
    server.join().unwrap();
}

#[test]
fn gui_edit_and_history_require_current_grants_for_the_selected_account() {
    let root = tempfile::tempdir().unwrap();
    let (token, thread, _) = prepare(root.path());
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
        swarm_runtime::serve_with_gui(&path, Some(1), Some(11)).unwrap()
    });
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Member", "member-password")
        .unwrap();
    assert_eq!(
        host.edit(edit_args(&thread, 1, "Denied", 1))
            .unwrap_err()
            .exit_code,
        4
    );
    assert!(
        host.read("post.history", json!({"thread_id":thread["id"],"no":1}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
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
    client.request("admin-fixture", req("permission.set", json!({"subject_type":"account","subject_id":member["id"],"operation":"post.edit","scope":"thread","target_id":thread["id"],"decision":"allow"}))).unwrap();
    let edited = host
        .edit(edit_args(&thread, 1, "Delegated edit", 1))
        .unwrap();
    assert_eq!(edited["edited_by"], member["id"]);
    client.request("admin-fixture", req("permission.set", json!({"subject_type":"account","subject_id":member["id"],"operation":"post.history","scope":"thread","target_id":thread["id"],"decision":"deny"}))).unwrap();
    assert_eq!(
        host.read("post.history", json!({"thread_id":thread["id"],"no":1}))
            .unwrap_err()
            .exit_code,
        4
    );
    server.join().unwrap();
}

#[test]
fn lost_edit_response_cannot_apply_twice_and_history_has_one_prior_version() {
    let root = tempfile::tempdir().unwrap();
    let (token, thread, _) = prepare(root.path());
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(8)).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Admin", "fixture-password")
        .unwrap();
    let args = edit_args(&thread, 1, "Committed without a reply", 1);
    let store = swarm_platform::CredentialStore::open(&root.path().join("gui-host")).unwrap();
    let capability =
        String::from_utf8(store.get("transport-capability").unwrap().unwrap()).unwrap();
    let alias = format!("account-{:x}", Sha256::digest(b"Admin"));
    let mut outer = req(
        "gui.request",
        json!({"alias":alias,"request":req("post.edit",args.clone())}),
    );
    outer.auth = Some(capability);
    let mut pipe =
        swarm_platform::pipe::connect(&swarm_runtime::gui_pipe_name(root.path()).unwrap()).unwrap();
    swarm_ipc::write_request(&mut pipe, &outer).unwrap();
    drop(pipe);
    // Observe the committed version through a separate read before testing a stale retransmission.
    let mut reader = Service::open(root.path()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if call(
            &mut reader,
            &token,
            "post.get",
            json!({"thread_id":thread["id"],"no":1}),
        )["revision"]
            == 2
        {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(host.edit(args).unwrap_err().exit_code, 6);
    let current = host
        .read("post.get", json!({"thread_id":thread["id"],"no":1}))
        .unwrap();
    assert_eq!(current["revision"], 2);
    assert_eq!(current["body"], "Committed without a reply");
    let history = host
        .read("post.history", json!({"thread_id":thread["id"],"no":1}))
        .unwrap();
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
    assert_eq!(host.creation_status().unwrap()["pending"], false);
    server.join().unwrap();
    assert_eq!(
        host.create("board.create", json!({"name":"Unconfirmed creation"}))
            .unwrap_err()
            .exit_code,
        8
    );
    assert_eq!(
        host.edit(edit_args(
            &thread,
            1,
            "Blocked while creation is pending",
            2
        ))
        .unwrap_err()
        .exit_code,
        6
    );
    assert_eq!(host.discard_create().unwrap()["pending"], false);
}

#[test]
fn deleted_post_history_is_not_exposed_without_deleted_body_permission() {
    let root = tempfile::tempdir().unwrap();
    let (token, thread, _) = prepare(root.path());
    let mut service = Service::open(root.path()).unwrap();
    call(
        &mut service,
        &token,
        "post.edit",
        edit_args(&thread, 1, "Later private body", 1),
    );
    call(
        &mut service,
        &token,
        "post.delete",
        json!({"thread_id":thread["id"],"no":1}),
    );
    call(
        &mut service,
        &token,
        "account.create",
        json!({"name":"Member","password":"member-password"}),
    );
    drop(service);
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(5)).unwrap());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Member", "member-password")
        .unwrap();
    let error = host
        .read("post.history", json!({"thread_id":thread["id"],"no":1}))
        .unwrap_err();
    assert_eq!(error.exit_code, 4);
    assert!(!error.message.contains("元の本文"));
    assert!(!error.message.contains("Later private body"));
    server.join().unwrap();
}
