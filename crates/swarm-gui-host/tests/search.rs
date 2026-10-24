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
fn gui_search_pages_current_content_and_enforces_private_parent_filters_and_revocation() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = service
        .request(req(
            "account.setup",
            json!({"name":"Admin","password":"search-host-admin"}),
        ))
        .unwrap();
    let admin_token = admin["token"].as_str().unwrap();
    let board = call(
        &mut service,
        admin_token,
        "board.create",
        json!({"name":"Private parent","description":""}),
    );
    let thread = call(
        &mut service,
        admin_token,
        "thread.create",
        json!({"board_id":board["id"],"name":"Individually shared","description":""}),
    );
    call(
        &mut service,
        admin_token,
        "post.create",
        json!({"thread_id":thread["id"],"body":"日本語 Current needle one"}),
    );
    call(
        &mut service,
        admin_token,
        "post.create",
        json!({"thread_id":thread["id"],"body":"日本語 Current needle two"}),
    );
    call(
        &mut service,
        admin_token,
        "post.create",
        json!({"thread_id":thread["id"],"body":"oldneedle retired"}),
    );
    call(
        &mut service,
        admin_token,
        "post.edit",
        json!({"thread_id":thread["id"],"no":3,"body":"deleted needle","if_revision":1}),
    );
    call(
        &mut service,
        admin_token,
        "post.delete",
        json!({"thread_id":thread["id"],"no":3}),
    );
    let reader = call(
        &mut service,
        admin_token,
        "account.create",
        json!({"name":"Reader","password":"search-host-reader"}),
    );
    call(
        &mut service,
        admin_token,
        "permission.set",
        json!({"subject_type":"account","subject_id":reader["id"],"operation":"board.get","scope":"board","target_id":board["id"],"decision":"deny"}),
    );
    call(
        &mut service,
        admin_token,
        "permission.set",
        json!({"subject_type":"account","subject_id":reader["id"],"operation":"search","scope":"board","target_id":board["id"],"decision":"deny"}),
    );
    call(
        &mut service,
        admin_token,
        "permission.set",
        json!({"subject_type":"account","subject_id":reader["id"],"operation":"search","scope":"thread","target_id":thread["id"],"decision":"allow"}),
    );
    drop(service);

    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(12)).unwrap()
    });
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Reader", "search-host-reader")
        .unwrap();
    let first = host
        .read(
            "search",
            json!({"query":"日本語 Current","scope":"post","limit":1}),
        )
        .unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert!(first["items"][0]["board_id"].is_null());
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();
    let hidden_filter = host
        .read(
            "search",
            json!({"query":"日本語 Current","scope":"post","limit":1,"board_id":board["id"]}),
        )
        .unwrap();
    assert_eq!(hidden_filter["items"], json!([]));
    let names = host
        .read("search", json!({"query":"Private","scope":"board_name"}))
        .unwrap();
    assert_eq!(names["items"], json!([]));
    let names = host
        .read(
            "search",
            json!({"query":"Individually","scope":"thread_name"}),
        )
        .unwrap();
    assert_eq!(names["items"].as_array().unwrap().len(), 1);
    assert_eq!(names["items"][0]["kind"], "thread");
    assert!(names["items"][0]["board_id"].is_null());
    let bodies = host
        .read("search", json!({"query":"日本語 Current","scope":"body"}))
        .unwrap();
    assert_eq!(bodies["items"].as_array().unwrap().len(), 2);
    assert!(
        bodies["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["kind"] == "post")
    );

    let client = GuiClient::open(root.path()).unwrap();
    client
        .issue(
            "admin-fixture",
            &"d".repeat(64),
            req(
                "account.login",
                json!({"name":"Admin","password":"search-host-admin"}),
            ),
        )
        .unwrap();
    client.request("admin-fixture", req("permission.set", json!({"subject_type":"account","subject_id":reader["id"],"operation":"search","scope":"thread","target_id":thread["id"],"decision":"deny"}))).unwrap();
    let revoked = host
        .read(
            "search",
            json!({"query":"日本語 Current","scope":"post","limit":1,"cursor":cursor}),
        )
        .unwrap();
    assert_eq!(revoked["items"], json!([]));
    assert!(revoked["next_cursor"].is_null());
    assert!(!revoked.to_string().contains("Private parent"));
    server.join().unwrap();
}
