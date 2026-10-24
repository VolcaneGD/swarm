#![cfg(windows)]

use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::{Child, Command, Output, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use swarm_core::{CredentialCandidate, Service, protocol::Request};
use swarm_runtime::gui::GuiClient;

fn request(operation: &str, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: None,
        args,
        context: json!({}),
        request_id: None,
    }
}

struct RuntimeCleanup {
    root: PathBuf,
    armed: bool,
}

impl RuntimeCleanup {
    fn stop(&mut self) {
        let Ok(client) = GuiClient::open(&self.root) else {
            return;
        };
        let login = request(
            "account.login",
            json!({"name":"Admin","password":"fixture-password"}),
        );
        // Reusing this intent is safe and makes a cleanup attempt idempotent.
        if client.issue("admin", &"c".repeat(64), login).is_ok() && client.stop("admin").is_ok() {
            self.armed = false;
        }
    }
}

impl Drop for RuntimeCleanup {
    fn drop(&mut self) {
        if self.armed {
            self.stop();
        }
    }
}

fn status(root: &std::path::Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .arg("--data-dir")
        .arg(root)
        .args(["--format", "json", "runtime", "status"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    value["data"]["state"].as_str().unwrap().to_owned()
}

fn spawn_board_list(root: &std::path::Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_swarm"))
        .arg("--data-dir")
        .arg(root)
        .args(["--session", "agent", "--format", "json", "board", "list"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn collect_captured_outputs(children: Vec<Child>, cleanup: &mut RuntimeCleanup) -> Vec<Output> {
    enum CliEvent {
        Exited(usize),
        Finished(usize, std::io::Result<Output>),
    }

    let (sender, receiver) = mpsc::channel();
    for (index, child) in children.into_iter().enumerate() {
        let sender = sender.clone();
        thread::spawn(move || {
            let mut child = child;
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => {
                        let _ = sender.send(CliEvent::Exited(index));
                        let _ = sender.send(CliEvent::Finished(index, child.wait_with_output()));
                        break;
                    }
                    Ok(None) => thread::sleep(Duration::from_millis(10)),
                    Err(error) => {
                        let _ = sender.send(CliEvent::Finished(index, Err(error)));
                        break;
                    }
                }
            }
        });
    }
    drop(sender);

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut outputs: Vec<Option<Output>> = (0..2).map(|_| None).collect();
    let mut exited = Vec::new();
    while outputs.iter().any(Option::is_none) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            cleanup.stop();
            panic!("captured CLI deadline expired; exited_but_output_not_eof={exited:?}");
        }
        match receiver.recv_timeout(remaining) {
            Ok(CliEvent::Exited(index)) => exited.push(index),
            Ok(CliEvent::Finished(index, Ok(output))) => outputs[index] = Some(output),
            Ok(CliEvent::Finished(_, Err(error))) => {
                cleanup.stop();
                panic!("captured CLI process failed: {error}");
            }
            Err(_) => {
                cleanup.stop();
                panic!("captured CLI deadline expired; exited_but_output_not_eof={exited:?}");
            }
        }
    }
    outputs.into_iter().map(Option::unwrap).collect()
}

#[test]
fn status_is_read_only_cli_autostarts_and_human_stop_is_persistent() {
    let root = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let mut cleanup = RuntimeCleanup {
        root: root.path().to_owned(),
        armed: false,
    };

    let mut service = Service::open(root.path()).unwrap();
    let admin = service
        .request(request(
            "account.setup",
            json!({"name":"Admin","password":"fixture-password"}),
        ))
        .unwrap();
    let project = service
        .request(Request {
            operation: "project.register".into(),
            auth: admin["token"].as_str().map(str::to_owned),
            args: json!({"name":"Fixture","root":project_root.path()}),
            context: json!({}),
            request_id: None,
        })
        .unwrap();
    let ai = service
        .request(request(
            "identity.start",
            json!({"project_id":project["id"],"model_name":"Fixture"}),
        ))
        .unwrap();
    let candidate = CredentialCandidate::from_grant("identity.start", &ai).unwrap();
    swarm_platform::CredentialStore::open(&root.path().join("client-credentials"))
        .unwrap()
        .put("agent", &serde_json::to_vec(&candidate).unwrap())
        .unwrap();
    std::fs::write(
        root.path().join("RULES.md"),
        "~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules: []\naccess:\n  boards: []\n  threads: []\n~~~\n",
    )
    .unwrap();
    drop(service);

    assert_eq!(status(root.path()), "not_running");
    assert_eq!(status(root.path()), "not_running");

    let identity_status = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .arg("--data-dir")
        .arg(root.path())
        .args([
            "--session",
            "agent",
            "--format",
            "json",
            "identity",
            "status",
        ])
        .output()
        .unwrap();
    assert_eq!(identity_status.status.code(), Some(8));
    let fault: Value = serde_json::from_slice(&identity_status.stdout).unwrap();
    assert_eq!(fault["error"]["code"], "CONNECTION_ERROR");
    assert_eq!(status(root.path()), "not_running");

    cleanup.armed = true;
    cleanup.armed = true;
    let outputs = collect_captured_outputs(
        vec![spawn_board_list(root.path()), spawn_board_list(root.path())],
        &mut cleanup,
    );
    for output in outputs {
        assert!(
            output.status.success(),
            "stdout: {}; stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(envelope["ok"], true);
        assert_eq!(envelope["data"]["items"], json!([]));
    }
    assert_eq!(status(root.path()), "running");
    let gui = GuiClient::open(root.path()).unwrap();
    gui.issue(
        "admin",
        &"c".repeat(64),
        request(
            "account.login",
            json!({"name":"Admin","password":"fixture-password"}),
        ),
    )
    .unwrap();
    assert_eq!(gui.stop("admin").unwrap(), json!({"state":"stopped"}));
    cleanup.armed = false;

    assert_eq!(status(root.path()), "stopped");
    let denied = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .arg("--data-dir")
        .arg(root.path())
        .args(["--session", "agent", "--format", "json", "board", "list"])
        .output()
        .unwrap();
    assert_eq!(denied.status.code(), Some(7));
    let fault: Value = serde_json::from_slice(&denied.stdout).unwrap();
    assert_eq!(fault["error"]["code"], "RUNTIME_STOPPED");
    assert_eq!(status(root.path()), "stopped");
}
