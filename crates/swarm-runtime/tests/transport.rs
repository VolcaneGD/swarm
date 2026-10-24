#![cfg(windows)]
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
use swarm_platform::pipe;

fn request(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}
fn connect(name: &str) -> pipe::Connection {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(connection) = pipe::connect(name) {
            return connection;
        }
        assert!(
            Instant::now() < deadline,
            "runtime did not become reachable"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn fixture() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    String,
    String,
    Vec<String>,
) {
    let root = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = service
        .request(request(
            "account.setup",
            None,
            json!({"name":"Admin","password":"fixture-password"}),
        ))
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let project = service
        .request(request(
            "project.register",
            Some(&admin),
            json!({"name":"Project","root":project_root.path()}),
        ))
        .unwrap();
    let board = service
        .request(request(
            "board.create",
            Some(&admin),
            json!({"name":"Board"}),
        ))
        .unwrap();
    let thread = service
        .request(request(
            "thread.create",
            Some(&admin),
            json!({"board_id":board["id"],"name":"Thread"}),
        ))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    std::fs::write(root.path().join("RULES.md"), format!("~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: posting\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [post.create, response.create]\n    when: {{ field: time.hour, op: gte, value: 0 }}\naccess:\n  boards: []\n  threads: []\n~~~\n",project["id"].as_str().unwrap())).unwrap();
    let tokens = (0..20)
        .map(|_| {
            service
                .request(request(
                    "identity.start",
                    None,
                    json!({"project_id":project["id"],"model_name":"Fixture"}),
                ))
                .unwrap()["session_token"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    (root, project_root, admin, thread, tokens)
}

#[test]
fn twenty_ai_clients_post_through_real_named_pipe_without_number_collisions() {
    let (root, _project, admin, thread, tokens) = fixture();
    let mut service = Service::open(root.path()).unwrap();
    let seed = service
        .request(request(
            "post.create",
            Some(&admin),
            json!({"thread_id":thread,"body":"Reply target"}),
        ))
        .unwrap();
    assert_eq!(seed["no"], 1);
    let actors: Vec<_> = tokens
        .iter()
        .map(|token| {
            service
                .request(request("identity.status", Some(token), json!({})))
                .unwrap()["id"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    assert_eq!(actors.iter().collect::<BTreeSet<_>>().len(), 20);
    drop(service);
    let name = swarm_runtime::pipe_name(root.path()).unwrap();
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || swarm_runtime::serve(&path, Some(20)).unwrap());
    let barrier = Arc::new(Barrier::new(20));
    let clients: Vec<_> = tokens
        .into_iter()
        .enumerate()
        .map(|(index, token)| {
            let name = name.clone();
            let thread = thread.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                let mut connection = connect(&name);
                let body = format!("Concurrent worker {index}");
                let (operation, args) = if index % 2 == 0 {
                    ("post.create", json!({"thread_id":thread,"body":body}))
                } else {
                    (
                        "response.create",
                        json!({"thread_id":thread,"body":body,"to":1}),
                    )
                };
                swarm_ipc::write_request(&mut connection, &request(operation, Some(&token), args))
                    .unwrap();
                let response = swarm_ipc::read_response(&mut connection).unwrap();
                assert!(response.ok, "{response:?}");
                (index, response.data.unwrap())
            })
        })
        .collect();
    let results: Vec<_> = clients
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    let mut numbers: Vec<_> = results
        .iter()
        .map(|(_, post)| post["no"].as_i64().unwrap())
        .collect();
    numbers.sort();
    assert_eq!(numbers, (2..=21).collect::<Vec<_>>());
    server.join().unwrap();
    // Reopen after runtime termination: successful replies must describe
    // durable rows, with each client's own content/author and intact relation.
    let mut service = Service::open(root.path()).unwrap();
    let all = service
        .request(request(
            "post.list",
            Some(&admin),
            json!({"thread_id":thread}),
        ))
        .unwrap();
    let posts = all["items"].as_array().unwrap();
    assert_eq!(posts.len(), 21);
    assert_eq!(
        posts
            .iter()
            .map(|post| post["id"].as_str().unwrap())
            .collect::<BTreeSet<_>>()
            .len(),
        21
    );
    assert_eq!(
        posts
            .iter()
            .map(|post| post["no"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        (1..=21).collect::<Vec<_>>()
    );
    for (index, response) in results {
        let stored = posts
            .iter()
            .find(|post| post["id"] == response["id"])
            .unwrap();
        assert_eq!(stored["body"], format!("Concurrent worker {index}"));
        assert_eq!(stored["author"]["id"], actors[index]);
        assert_eq!(stored["thread_id"], thread);
        assert_eq!(stored["no"], response["no"]);
        assert_eq!(
            stored["reply_to"],
            if index % 2 == 0 {
                Value::Null
            } else {
                json!(1)
            }
        );
    }
    let replies = service
        .request(request(
            "response.list",
            Some(&admin),
            json!({"thread_id":thread,"to":1}),
        ))
        .unwrap();
    assert_eq!(replies["items"].as_array().unwrap().len(), 10);
    assert_eq!(posts[0]["response_count"], 10);
    let db = rusqlite::Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(
        db.prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none()
    );
}

#[test]
fn runtime_rejects_human_credentials_admin_operations_and_invalid_frames() {
    let (root, _project, admin, _thread, tokens) = fixture();
    let name = swarm_runtime::pipe_name(root.path()).unwrap();
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || swarm_runtime::serve(&path, Some(4)).unwrap());
    for (operation, token, exit_code) in [
        ("board.list", &admin, 3),
        ("account.list", &tokens[0], 4),
        ("identity.start", &tokens[0], 4),
    ] {
        let mut connection = connect(&name);
        swarm_ipc::write_request(&mut connection, &request(operation, Some(token), json!({})))
            .unwrap();
        let response = swarm_ipc::read_response(&mut connection).unwrap();
        assert!(!response.ok);
        let error = response.error.unwrap();
        assert_eq!(error.exit_code, exit_code);
        assert!(!error.message.contains(token));
    }
    let mut connection = connect(&name);
    use std::io::Write;
    connection
        .write_all(&((swarm_ipc::MAX_FRAME_BYTES + 1) as u32).to_le_bytes())
        .unwrap();
    let response = swarm_ipc::read_response(&mut connection).unwrap();
    assert_eq!(response.error.unwrap().code, "INVALID_FRAME");
    server.join().unwrap();
}
