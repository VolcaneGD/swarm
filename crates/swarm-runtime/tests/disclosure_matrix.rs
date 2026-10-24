#![cfg(windows)]
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use swarm_core::{Service, protocol::Request};
use swarm_ipc::RpcResponse;

fn req(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}
fn call(service: &mut Service, operation: &str, auth: Option<&str>, args: Value) -> Value {
    service.request(req(operation, auth, args)).unwrap()
}
fn rpc(pipe: &str, request: Request) -> RpcResponse {
    let mut connection = swarm_platform::pipe::connect(pipe).unwrap();
    swarm_ipc::write_request(&mut connection, &request).unwrap();
    swarm_ipc::read_response(&mut connection).unwrap()
}
fn ready(pipe: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(connection) = swarm_platform::pipe::connect(pipe) {
            drop(connection);
            return;
        }
        assert!(
            Instant::now() < deadline,
            "isolated endpoint did not become ready"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn public_ai_pipe_filters_deleted_history_and_live_hidden_targets_across_read_routes() {
    let root = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"disclosure-fixture-password"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let project = call(
        &mut service,
        "project.register",
        Some(&admin),
        json!({"name":"Fixture","root":project_root.path()}),
    );
    let ai = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )["session_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let board = call(
        &mut service,
        "board.create",
        Some(&admin),
        json!({"name":"private-board-marker"}),
    );
    let thread = call(
        &mut service,
        "thread.create",
        Some(&admin),
        json!({"board_id":board["id"],"name":"private-thread-marker"}),
    );
    let cursor = call(&mut service, "changes.get", Some(&ai), json!({}))["change_cursor"].clone();
    call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread["id"],"body":"deleted-original-marker"}),
    );
    call(
        &mut service,
        "post.edit",
        Some(&admin),
        json!({"thread_id":thread["id"],"no":1,"body":"deleted-current-marker","if_revision":1}),
    );
    call(
        &mut service,
        "post.delete",
        Some(&admin),
        json!({"thread_id":thread["id"],"no":1}),
    );
    let mut routes = vec![
        ("board.list", json!({})),
        ("thread.list", json!({})),
        ("post.list", json!({"thread_id":thread["id"]})),
        ("post.history", json!({"thread_id":thread["id"],"no":1})),
        ("search", json!({"query":"marker","scope":"body"})),
        ("changes.get", json!({"after":cursor})),
    ];
    let rules = |deny: bool| {
        format!(
            "# Isolated disclosure fixture\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: history\n    effect: allow\n    actions: [post.history, rules.check]\naccess:\n  boards: {}\n  threads: []\n~~~\n",
            if deny {
                format!(
                    "\n    - id: {}\n      projects:\n        {}:\n          read: deny",
                    board["id"].as_str().unwrap(),
                    project["id"].as_str().unwrap()
                )
            } else {
                "[]".into()
            }
        )
    };
    std::fs::write(root.path().join("RULES.md"), rules(false)).unwrap();
    // RULES revisions invalidate cursors. Bootstrap with the same effective
    // policy that the runtime will use; the hidden phase may require resync.
    routes[5].1 =
        json!({"after":call(&mut service, "changes.get", Some(&ai), json!({}))["change_cursor"]});
    drop(service);
    let pipe = swarm_runtime::pipe_name(root.path()).unwrap();
    let path = root.path().to_owned();
    // One readiness connection, twelve matrix reads, two indistinguishability reads.
    let server = std::thread::spawn(move || swarm_runtime::serve(&path, Some(15)).unwrap());
    ready(&pipe);
    for hidden in [false, true] {
        std::fs::write(root.path().join("RULES.md"), rules(hidden)).unwrap();
        for (operation, args) in &routes {
            let response = rpc(&pipe, req(operation, Some(&ai), args.clone()));
            let encoded = serde_json::to_string(&response).unwrap();
            for secret in [
                "deleted-original-marker",
                "deleted-current-marker",
                &ai,
                &admin,
            ] {
                assert!(
                    !encoded.contains(secret),
                    "{operation} exposed protected content"
                );
            }
            if hidden {
                for secret in [
                    "private-board-marker",
                    "private-thread-marker",
                    board["id"].as_str().unwrap(),
                    thread["id"].as_str().unwrap(),
                ] {
                    assert!(
                        !encoded.contains(secret),
                        "{operation} exposed hidden target"
                    );
                }
                match *operation {
                    "board.list" | "thread.list" | "search" => {
                        assert!(response.ok, "hidden filtered route failed");
                        let data = response.data.as_ref().unwrap();
                        assert_eq!(data["items"], json!([]));
                        if *operation == "search" {
                            assert_eq!(data["total_count"], 0);
                        }
                    }
                    "post.list" | "post.history" => {
                        assert!(!response.ok);
                        assert_eq!(response.error.as_ref().unwrap().code, "PERMISSION_DENIED");
                    }
                    "changes.get" => {
                        assert!(!response.ok);
                        assert_eq!(response.error.as_ref().unwrap().code, "RESYNC_REQUIRED");
                    }
                    _ => unreachable!(),
                }
            } else {
                assert!(response.ok, "visible route {operation} failed");
                let data = response.data.unwrap();
                if *operation == "post.history" || *operation == "search" {
                    assert_eq!(data["items"], json!([]));
                }
                if *operation == "search" {
                    assert_eq!(data["total_count"], 0);
                }
            }
        }
    }
    let hidden = rpc(
        &pipe,
        req(
            "rules.check",
            Some(&ai),
            json!({"operation":"thread.get","target":{"id":thread["id"]}}),
        ),
    );
    let absent = rpc(
        &pipe,
        req(
            "rules.check",
            Some(&ai),
            json!({"operation":"thread.get","target":{"id":"00000000-0000-4000-8000-000000000000"}}),
        ),
    );
    assert!(hidden.ok && absent.ok);
    assert_eq!(
        hidden.data.as_ref().unwrap(),
        &json!({"allowed":false,"reason":"target_unavailable"})
    );
    assert_eq!(
        serde_json::to_value(hidden).unwrap(),
        serde_json::to_value(absent).unwrap()
    );
    server.join().unwrap();
}
