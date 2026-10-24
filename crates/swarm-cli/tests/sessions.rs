#![cfg(windows)]
use serde_json::{Value, json};
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
fn request(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}
#[test]
fn actual_cli_starts_and_resumes_without_printing_credentials() {
    let root = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = service
        .request(request(
            "account.setup",
            None,
            json!({"name":"Admin","password":"fixture-password"}),
        ))
        .unwrap();
    let project = service
        .request(request(
            "project.register",
            admin["token"].as_str(),
            json!({"name":"Project","root":project_root.path()}),
        ))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    drop(service);
    let original_cli = Path::new(env!("CARGO_BIN_EXE_swarm"));
    let unavailable_dir = tempfile::tempdir().unwrap();
    let unavailable_cli = unavailable_dir.path().join("swarm.exe");
    std::fs::copy(original_cli, &unavailable_cli).unwrap();
    assert!(!unavailable_dir.path().join("swarm-runtime.exe").exists());
    let run_at = |executable: &Path, args: &[&str]| {
        Command::new(executable)
            .arg("--data-dir")
            .arg(root.path())
            .args(["--session", "agent", "--format", "json"])
            .args(args)
            .output()
            .unwrap()
    };
    let run = |args: &[&str]| run_at(original_cli, args);
    let unavailable = run_at(
        &unavailable_cli,
        &[
            "identity",
            "start",
            "--project-id",
            &project,
            "--model-name",
            "Fixture",
        ],
    );
    assert_eq!(unavailable.status.code(), Some(8));
    let intents =
        swarm_platform::CredentialStore::open(&root.path().join("client-intents")).unwrap();
    let pending = intents.get("agent").unwrap().unwrap();
    let changed = run_at(
        &unavailable_cli,
        &[
            "identity",
            "start",
            "--project-id",
            &project,
            "--model-name",
            "Changed",
        ],
    );
    assert_eq!(changed.status.code(), Some(6));
    assert_eq!(intents.get("agent").unwrap().unwrap(), pending);
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || swarm_runtime::serve(&path, Some(6)).unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    let name = swarm_runtime::pipe_name(root.path()).unwrap();
    loop {
        if let Ok(connection) = swarm_platform::pipe::connect(&name) {
            drop(connection);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let pending_value: Value = serde_json::from_slice(&pending).unwrap();
    let mut dropped = swarm_platform::pipe::connect(&name).unwrap();
    swarm_ipc::write_request(
        &mut dropped,
        &request(
            "session.start",
            pending_value["intent"].as_str(),
            pending_value["args"].clone(),
        ),
    )
    .unwrap();
    drop(dropped);
    let store =
        swarm_platform::CredentialStore::open(&root.path().join("client-credentials")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while store.get("agent").unwrap().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let committed = store.get("agent").unwrap().unwrap();
    let started = run(&[
        "identity",
        "start",
        "--project-id",
        &project,
        "--model-name",
        "Fixture",
    ]);
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let public: Value = serde_json::from_slice(&started.stdout).unwrap();
    let id = public["data"]["identity"]["id"].as_str().unwrap();
    assert_eq!(store.get("agent").unwrap().unwrap(), committed);
    let old: Value = serde_json::from_slice(&store.get("agent").unwrap().unwrap()).unwrap();
    let resumed = run(&[
        "identity",
        "resume",
        "--actor-id",
        id,
        "--project-id",
        &project,
        "--model-name",
        "Fixture",
    ]);
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stderr)
    );
    let new: Value = serde_json::from_slice(&store.get("agent").unwrap().unwrap()).unwrap();
    assert_ne!(old["session_token"], new["session_token"]);
    assert!(run(&["identity", "status"]).status.success());
    assert!(run(&["board", "list"]).status.success());
    for output in [started, resumed] {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        for key in ["session_token", "resume_secret"] {
            assert!(!stdout.contains(old[key].as_str().unwrap()));
            assert!(!stdout.contains(new[key].as_str().unwrap()));
            assert!(!stderr.contains(new[key].as_str().unwrap()));
        }
    }
    assert!(
        swarm_platform::CredentialStore::open(&root.path().join("client-intents"))
            .unwrap()
            .get("agent")
            .unwrap()
            .is_none()
    );
    server.join().unwrap();
}
