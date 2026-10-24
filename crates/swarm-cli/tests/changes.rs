#![cfg(windows)]

use serde_json::{Value, json};
use std::os::windows::process::CommandExt;
use std::{
    process::{Child, Command, Output, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use swarm_core::{CredentialCandidate, Service, protocol::Request};

fn request(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}

struct Fixture {
    root: tempfile::TempDir,
    _project_root: tempfile::TempDir,
    thread_id: String,
}

fn fixture() -> Fixture {
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
        .unwrap();
    let board = service
        .request(request(
            "board.create",
            admin["token"].as_str(),
            json!({"name":"Board"}),
        ))
        .unwrap();
    let thread = service
        .request(request(
            "thread.create",
            admin["token"].as_str(),
            json!({"board_id":board["id"],"name":"Thread"}),
        ))
        .unwrap();
    let ai = service
        .request(request(
            "identity.start",
            None,
            json!({"project_id":project["id"],"model_name":"Fixture"}),
        ))
        .unwrap();
    let candidate = CredentialCandidate::from_grant("identity.start", &ai).unwrap();
    let credentials =
        swarm_platform::CredentialStore::open(&root.path().join("client-credentials")).unwrap();
    credentials
        .put("fixture", &serde_json::to_vec(&candidate).unwrap())
        .unwrap();
    std::fs::write(
        root.path().join("RULES.md"),
        format!(
            "~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: changes-and-posting\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [changes.get, post.create]\n    when: {{ field: time.hour, op: gte, value: 0 }}\naccess:\n  boards: []\n  threads: []\n~~~\n",
            project["id"].as_str().unwrap()
        ),
    )
    .unwrap();
    drop(service);
    Fixture {
        root,
        _project_root: project_root,
        thread_id: thread["id"].as_str().unwrap().to_owned(),
    }
}

fn cli(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_swarm"))
        .arg("--data-dir")
        .arg(root)
        .args(["--session", "fixture", "--format", "json"])
        .args(args)
        .output()
        .unwrap()
}

fn data(output: &std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "exit {:?}, stdout: {}, stderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["ok"], true);
    envelope["data"].clone()
}

fn launch(root: &std::path::Path, connections: usize) -> std::thread::JoinHandle<()> {
    let path = root.to_owned();
    let server = thread::spawn(move || swarm_runtime::serve(&path, Some(connections)).unwrap());
    let name = swarm_runtime::pipe_name(root).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(connection) = swarm_platform::pipe::connect(&name) {
            drop(connection);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "runtime pipe did not become ready"
        );
        thread::sleep(Duration::from_millis(10));
    }
    server
}

fn launch_with_gui(
    root: &std::path::Path,
    ai_connections: usize,
    gui_connections: usize,
) -> std::thread::JoinHandle<()> {
    let path = root.to_owned();
    let server = thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(ai_connections), Some(gui_connections)).unwrap()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    for endpoint in [
        swarm_runtime::pipe_name(root).unwrap(),
        swarm_runtime::gui_pipe_name(root).unwrap(),
    ] {
        loop {
            if let Ok(connection) = swarm_platform::pipe::connect(&endpoint) {
                drop(connection);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "runtime endpoint did not become ready"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    server
}

struct ChildGuard(Option<Child>);

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn assert_pending_for(&mut self, duration: Duration) {
        let deadline = Instant::now() + duration;
        loop {
            let child = self.0.as_mut().expect("child already collected");
            match child.try_wait() {
                Ok(Some(status)) => panic!("wait command exited early with {status}"),
                Ok(None) => {}
                Err(error) => panic!("could not inspect wait command: {error}"),
            }
            if Instant::now() >= deadline {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn collect(mut self, timeout: Duration) -> Output {
        let deadline = Instant::now() + timeout;
        loop {
            let child = self.0.as_mut().expect("child already collected");
            match child.try_wait() {
                Ok(Some(_)) => {
                    return self
                        .0
                        .take()
                        .expect("child already collected")
                        .wait_with_output()
                        .expect("collect wait output");
                }
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                Ok(None) => {
                    let child = self.0.as_mut().expect("child already collected");
                    let _ = child.kill();
                    let output = self
                        .0
                        .take()
                        .expect("child already collected")
                        .wait_with_output()
                        .expect("collect timed-out wait output");
                    panic!(
                        "wait command exceeded {timeout:?}; exit={:?}, stdout={}, stderr={}",
                        output.status.code(),
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
                Err(error) => panic!("could not inspect wait command: {error}"),
            }
        }
    }

    fn wait_for_exit(mut self, timeout: Duration) -> std::process::ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            let child = self.0.as_mut().expect("child already collected");
            match child.try_wait() {
                Ok(Some(_)) => {
                    return self
                        .0
                        .take()
                        .expect("child already collected")
                        .wait()
                        .expect("reap child process");
                }
                Ok(None) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(10));
                }
                Ok(None) => {
                    let child = self.0.as_mut().expect("child already collected");
                    let _ = child.kill();
                    let status = self
                        .0
                        .take()
                        .expect("child already collected")
                        .wait()
                        .expect("reap timed-out child process");
                    panic!("owned runtime process exceeded {timeout:?}: {status}");
                }
                Err(error) => panic!("could not inspect runtime process: {error}"),
            }
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn spawn_cli_wait(root: &std::path::Path, cursor: &str, timeout: &str) -> ChildGuard {
    ChildGuard::new(
        Command::new(env!("CARGO_BIN_EXE_swarm"))
            .arg("--data-dir")
            .arg(root)
            .args([
                "--session",
                "fixture",
                "--format",
                "json",
                "changes",
                "wait",
                "--after",
                cursor,
                "--timeout",
                timeout,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
}

fn runtime_executable() -> std::path::PathBuf {
    let mut path = std::env::current_exe().expect("locate integration test executable");
    path.pop(); // deps
    path.pop(); // target profile directory
    path.push("swarm-runtime.exe");
    assert!(
        path.is_file(),
        "expected the separately built runtime executable at {}",
        path.display()
    );
    path
}

fn spawn_runtime(root: &std::path::Path) -> ChildGuard {
    ChildGuard::new(
        Command::new(runtime_executable())
            .arg("--data-dir")
            .arg(root)
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn isolated runtime executable"),
    )
}

fn wait_for_runtime(
    root: &std::path::Path,
    runtime: &mut ChildGuard,
) -> swarm_runtime::gui::GuiClient {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = runtime
            .0
            .as_mut()
            .expect("runtime child should still be owned")
            .try_wait()
            .expect("inspect runtime during startup")
        {
            panic!("runtime exited before becoming ready: {status}");
        }
        if let Ok(gui) = swarm_runtime::gui::GuiClient::open(root)
            && gui.setup_status().is_ok()
        {
            return gui;
        }
        assert!(
            Instant::now() < deadline,
            "owned runtime did not become ready"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn cli_bootstraps_before_snapshot_and_waits_for_body_free_change_metadata() {
    let fixture = fixture();
    // The probe, bootstrap, snapshot, wait, create, and get each use one connection.
    let server = launch(fixture.root.path(), 6);

    let bootstrap = data(&cli(fixture.root.path(), &["changes", "get"]));
    assert_eq!(bootstrap["items"], json!([]));
    let initial_cursor = bootstrap["change_cursor"].as_str().unwrap().to_owned();
    assert_eq!(bootstrap["has_more"], false);

    // Obtain the current snapshot only after the initial cursor, so the next
    // request observes only changes made after that snapshot boundary.
    let snapshot = data(&cli(
        fixture.root.path(),
        &["post", "list", "--thread", &fixture.thread_id],
    ));
    assert!(snapshot["items"].as_array().unwrap().is_empty());

    let wait_root = fixture.root.path().to_owned();
    let wait_cursor = initial_cursor.clone();
    let mut waiter = spawn_cli_wait(&wait_root, &wait_cursor, "5");
    waiter.assert_pending_for(Duration::from_millis(400));
    let created = data(&cli(
        fixture.root.path(),
        &[
            "post",
            "create",
            "--thread-id",
            &fixture.thread_id,
            "--body",
            "private body must not appear in change metadata",
        ],
    ));
    assert_eq!(created["no"], 1);

    let waited = data(&waiter.collect(Duration::from_secs(3)));
    assert_eq!(waited["items"].as_array().unwrap().len(), 1);
    assert_eq!(waited["items"][0]["operation"], "post.create");
    assert_eq!(waited["items"][0]["thread_id"], fixture.thread_id);
    assert_eq!(waited["items"][0]["no"], 1);
    assert!(waited["change_cursor"].as_str().is_some());
    assert!(
        !serde_json::to_string(&waited)
            .unwrap()
            .contains("private body"),
        "change feed must never include Post bodies"
    );

    let delta = data(&cli(
        fixture.root.path(),
        &["changes", "get", "--after", &initial_cursor, "--limit", "1"],
    ));
    assert_eq!(delta["items"].as_array().unwrap().len(), 1);
    assert_eq!(delta["items"][0]["entity_id"], created["id"]);
    assert_eq!(delta["has_more"], false);
    assert!(delta["change_cursor"].as_str().is_some());
    server.join().unwrap();
}

#[test]
fn cli_changes_wait_returns_empty_on_timeout() {
    let fixture = fixture();
    let server = launch(fixture.root.path(), 3);
    let bootstrap = data(&cli(fixture.root.path(), &["changes", "get"]));
    let cursor = bootstrap["change_cursor"].as_str().unwrap();
    let started = Instant::now();
    let waited =
        data(&spawn_cli_wait(fixture.root.path(), cursor, "1").collect(Duration::from_secs(4)));
    assert!(started.elapsed() >= Duration::from_millis(700));
    assert_eq!(waited["items"], json!([]));
    assert_eq!(waited["has_more"], false);
    assert_eq!(waited["change_cursor"], bootstrap["change_cursor"]);
    server.join().unwrap();
}

#[test]
fn cli_changes_wait_reports_runtime_stop_as_exit_seven() {
    let fixture = fixture();
    // Three AI connections (probe, bootstrap, wait) and three GUI connections
    // (probe, login, stop); listener shutdown interrupts the outstanding wait.
    let server = launch_with_gui(fixture.root.path(), 3, 3);
    let bootstrap = data(&cli(fixture.root.path(), &["changes", "get"]));
    let cursor = bootstrap["change_cursor"].as_str().unwrap().to_owned();
    let wait_root = fixture.root.path().to_owned();
    let mut waiter = spawn_cli_wait(&wait_root, &cursor, "30");
    waiter.assert_pending_for(Duration::from_millis(500));

    let gui = swarm_runtime::gui::GuiClient::open(fixture.root.path()).unwrap();
    let login = gui
        .issue(
            "fixture-admin",
            &"c".repeat(64),
            request(
                "account.login",
                None,
                json!({"name":"Admin","password":"fixture-password"}),
            ),
        )
        .unwrap();
    assert!(login.get("token").is_none());
    gui.stop("fixture-admin").unwrap();

    let stopped_at = Instant::now();
    let output = waiter.collect(Duration::from_secs(5));
    assert!(stopped_at.elapsed() < Duration::from_secs(5));
    assert_eq!(output.status.code(), Some(7));
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "RUNTIME_STOPPED");
    server.join().unwrap();
}

#[test]
fn human_gui_wait_is_released_by_ai_post_on_a_separate_connection() {
    let fixture = fixture();
    let server = launch_with_gui(fixture.root.path(), 2, 4);
    let gui = swarm_runtime::gui::GuiClient::open(fixture.root.path()).unwrap();
    gui.issue(
        "fixture-admin",
        &"d".repeat(64),
        request(
            "account.login",
            None,
            json!({"name":"Admin","password":"fixture-password"}),
        ),
    )
    .unwrap();
    let bootstrap = gui
        .request(
            "fixture-admin",
            request("changes.get", None, json!({"limit":50})),
        )
        .unwrap();
    let cursor = bootstrap["change_cursor"].as_str().unwrap().to_owned();

    let wait_client = gui;
    let wait_cursor = cursor.clone();
    let (sender, receiver) = mpsc::sync_channel(1);
    let waiter = thread::spawn(move || {
        let result = wait_client.request(
            "fixture-admin",
            request(
                "changes.wait",
                None,
                json!({"after":wait_cursor,"limit":50,"timeout":5}),
            ),
        );
        let _ = sender.send(result);
    });
    assert!(matches!(
        receiver.recv_timeout(Duration::from_millis(400)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    let created = data(&cli(
        fixture.root.path(),
        &[
            "post",
            "create",
            "--thread-id",
            &fixture.thread_id,
            "--body",
            "private GUI wait body",
        ],
    ));
    assert_eq!(created["no"], 1);

    let waited = receiver
        .recv_timeout(Duration::from_secs(3))
        .expect("human wait should wake promptly after Post create")
        .unwrap();
    waiter.join().unwrap();
    assert_eq!(waited["items"].as_array().unwrap().len(), 1);
    assert_eq!(waited["items"][0]["operation"], "post.create");
    assert_eq!(waited["items"][0]["entity_id"], created["id"]);
    assert!(
        !serde_json::to_string(&waited)
            .unwrap()
            .contains("private GUI wait body")
    );
    server.join().unwrap();
}

#[test]
fn cli_changes_wait_does_not_return_success_after_rules_revoke_access() {
    let fixture = fixture();
    let server = launch(fixture.root.path(), 3);
    let bootstrap = data(&cli(fixture.root.path(), &["changes", "get"]));
    let cursor = bootstrap["change_cursor"].as_str().unwrap().to_owned();
    let wait_root = fixture.root.path().to_owned();
    let mut waiter = spawn_cli_wait(&wait_root, &cursor, "5");
    waiter.assert_pending_for(Duration::from_millis(400));
    std::fs::write(
        fixture.root.path().join("RULES.md"),
        "~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules: []\naccess:\n  boards: []\n  threads: []\n~~~\n",
    )
    .unwrap();

    let output = waiter.collect(Duration::from_secs(3));
    assert_eq!(output.status.code(), Some(6));
    let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "RESYNC_REQUIRED");
    server.join().unwrap();
}

#[test]
fn separate_runtime_process_serves_changes_wait_and_stops_cleanly_with_marker() {
    let fixture = fixture();
    let mut runtime = spawn_runtime(fixture.root.path());
    let gui = wait_for_runtime(fixture.root.path(), &mut runtime);

    let bootstrap = data(&cli(fixture.root.path(), &["changes", "get"]));
    assert!(bootstrap["items"].as_array().unwrap().is_empty());
    let initial_cursor = bootstrap["change_cursor"].as_str().unwrap().to_owned();
    let snapshot = data(&cli(
        fixture.root.path(),
        &["post", "list", "--thread", &fixture.thread_id],
    ));
    assert!(snapshot["items"].as_array().unwrap().is_empty());

    let mut first_wait = spawn_cli_wait(fixture.root.path(), &initial_cursor, "30");
    first_wait.assert_pending_for(Duration::from_millis(500));
    let created = data(&cli(
        fixture.root.path(),
        &[
            "post",
            "create",
            "--thread-id",
            &fixture.thread_id,
            "--body",
            "process-isolated private body",
        ],
    ));
    let first_result = data(&first_wait.collect(Duration::from_secs(5)));
    assert_eq!(first_result["items"].as_array().unwrap().len(), 1);
    assert_eq!(first_result["items"][0]["operation"], "post.create");
    assert_eq!(first_result["items"][0]["entity_id"], created["id"]);
    assert!(
        !serde_json::to_string(&first_result)
            .unwrap()
            .contains("process-isolated private body")
    );

    let cursor = first_result["change_cursor"].as_str().unwrap();
    let mut second_wait = spawn_cli_wait(fixture.root.path(), cursor, "30");
    second_wait.assert_pending_for(Duration::from_millis(500));
    gui.issue(
        "fixture-admin",
        &"e".repeat(64),
        request(
            "account.login",
            None,
            json!({"name":"Admin","password":"fixture-password"}),
        ),
    )
    .unwrap();
    gui.stop("fixture-admin").unwrap();

    let wait_stopped_at = Instant::now();
    let stop_result = second_wait.collect(Duration::from_secs(5));
    assert!(wait_stopped_at.elapsed() < Duration::from_secs(5));
    assert_eq!(stop_result.status.code(), Some(7));
    let envelope: Value = serde_json::from_slice(&stop_result.stdout).unwrap();
    assert_eq!(envelope["error"]["code"], "RUNTIME_STOPPED");

    let runtime_status = runtime.wait_for_exit(Duration::from_secs(5));
    assert!(
        runtime_status.success(),
        "runtime exited with {runtime_status}"
    );
    assert_eq!(
        swarm_runtime::lifecycle::status(fixture.root.path()).unwrap(),
        json!({"state":"stopped"})
    );
    let lifecycle_store =
        swarm_platform::CredentialStore::open(&fixture.root.path().join("lifecycle")).unwrap();
    let marker = lifecycle_store
        .get("full-stop")
        .unwrap()
        .expect("GUI stop marker remains after child exit");
    assert!(marker.starts_with(b"1:"));
}
