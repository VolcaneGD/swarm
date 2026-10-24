#![cfg(windows)]

use serde_json::{Value, json};
use std::{
    process::{Command, Output},
    thread,
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

fn run(args: &[&str], project_env: &str, session_env: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args(args)
        .env("SWARM_PROJECT_ID", project_env)
        .env("SWARM_SESSION", session_env)
        .output()
        .unwrap()
}

#[test]
fn environment_selection_binds_alias_and_rejects_mismatched_release_before_pipe_mutation() {
    let root = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let second_project_root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = service
        .request(request(
            "account.setup",
            None,
            json!({"name":"Admin","password":"fixture-password"}),
        ))
        .unwrap();
    let project_id = service
        .request(request(
            "project.register",
            admin["token"].as_str(),
            json!({"name":"Project","root":project_root.path()}),
        ))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let second_project_id = service
        .request(request(
            "project.register",
            admin["token"].as_str(),
            json!({"name":"Second Project","root":second_project_root.path()}),
        ))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    drop(service);

    let serve_root = root.path().to_owned();
    let server = thread::spawn(move || swarm_runtime::serve(&serve_root, Some(7)).unwrap());
    let pipe_name = swarm_runtime::pipe_name(root.path()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(pipe) = swarm_platform::pipe::connect(&pipe_name) {
            drop(pipe);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "runtime pipe did not become ready"
        );
        thread::sleep(Duration::from_millis(10));
    }

    let start = run(
        &[
            "--data-dir",
            root.path().to_str().unwrap(),
            "--format",
            "json",
            "identity",
            "start",
        ],
        &project_id,
        "worker_a",
    );
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stdout)
    );
    let status = run(
        &[
            "--data-dir",
            root.path().to_str().unwrap(),
            "--format",
            "json",
            "identity",
            "status",
        ],
        &project_id,
        "worker_a",
    );
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stdout)
    );

    let runtime_status = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args([
            "--data-dir",
            root.path().to_str().unwrap(),
            "--format",
            "json",
            "runtime",
            "status",
        ])
        .env("SWARM_PROJECT_ID", "")
        .env("SWARM_SESSION", "bad/alias")
        .output()
        .unwrap();
    assert!(
        runtime_status.status.success(),
        "{}",
        String::from_utf8_lossy(&runtime_status.stdout)
    );

    let mismatch = run(
        &[
            "--data-dir",
            root.path().to_str().unwrap(),
            "--session",
            "worker_a",
            "--project",
            "another-project-id",
            "--format",
            "json",
            "identity",
            "release",
        ],
        "ignored-project-env",
        "ignored-session-env",
    );
    assert_eq!(mismatch.status.code(), Some(3));
    let public = String::from_utf8_lossy(&mismatch.stdout);
    assert!(public.contains("AUTHENTICATION_REQUIRED"));
    assert!(public.contains("Selected project does not match this session"));
    assert!(!public.contains(&project_id));
    assert!(!public.contains("another-project-id"));

    let credentials =
        swarm_platform::CredentialStore::open(&root.path().join("client-credentials")).unwrap();
    let env_mismatch = run(
        &[
            "--data-dir",
            root.path().to_str().unwrap(),
            "--format",
            "json",
            "identity",
            "release",
        ],
        "another-project-id",
        "worker_a",
    );
    assert_eq!(env_mismatch.status.code(), Some(3));
    assert!(credentials.get("worker_a").unwrap().is_some());

    assert!(credentials.get("worker_a").unwrap().is_some());
    let release = run(
        &[
            "--data-dir",
            root.path().to_str().unwrap(),
            "--session",
            "worker_a",
            "--project",
            &project_id,
            "--format",
            "json",
            "identity",
            "release",
        ],
        "ignored-project-env",
        "ignored-session-env",
    );
    assert!(
        release.status.success(),
        "{}",
        String::from_utf8_lossy(&release.stdout)
    );

    let second_start = run(
        &[
            "--data-dir",
            root.path().to_str().unwrap(),
            "--session",
            "worker_b",
            "--project",
            &second_project_id,
            "--format",
            "json",
            "identity",
            "start",
        ],
        &project_id,
        "worker_a",
    );
    assert!(
        second_start.status.success(),
        "{}",
        String::from_utf8_lossy(&second_start.stdout)
    );
    let second_status = run(
        &[
            "--data-dir",
            root.path().to_str().unwrap(),
            "--format",
            "json",
            "identity",
            "status",
        ],
        &second_project_id,
        "worker_b",
    );
    assert!(
        second_status.status.success(),
        "{}",
        String::from_utf8_lossy(&second_status.stdout)
    );
    assert!(
        String::from_utf8_lossy(&second_status.stdout)
            .to_ascii_lowercase()
            .contains(&second_project_id.to_ascii_lowercase())
    );
    server.join().unwrap();

    let help = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .arg("--help")
        .env("SWARM_PROJECT_ID", "")
        .env("SWARM_SESSION", "bad/alias")
        .output()
        .unwrap();
    assert!(help.status.success());
}
