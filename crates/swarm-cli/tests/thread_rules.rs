#![cfg(windows)]
use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Command, ExitStatus},
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
fn request(s: &mut Service, op: &str, auth: Option<&str>, args: Value) -> Value {
    s.request(Request {
        operation: op.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    })
    .unwrap()
}
fn cli(root: &Path, args: &[&str]) -> (ExitStatus, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .arg("--data-dir")
        .arg(root)
        .args(["--session", "local-rules-worker", "--format", "json"])
        .args(args)
        .env_remove("SWARM_PROJECT_ID")
        .env_remove("SWARM_SESSION")
        .output()
        .unwrap();
    (
        output.status,
        serde_json::from_slice(&output.stdout).unwrap(),
    )
}
#[test]
fn actual_cli_pipe_announces_local_rules_and_requires_current_confirmation() {
    let root = tempfile::tempdir().unwrap();
    let mut s = Service::open(root.path()).unwrap();
    let human = request(
        &mut s,
        "account.setup",
        None,
        json!({"name":"Admin","password":"fixture-password"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let board = request(
        &mut s,
        "board.create",
        Some(&human),
        json!({"name":"Board"}),
    );
    let thread=request(&mut s,"thread.create",Some(&human),json!({"board_id":board["id"],"name":"Rules","description":"Post all messages in Japanese."}))["id"].as_str().unwrap().to_owned();
    let project = request(
        &mut s,
        "project.register",
        Some(&human),
        json!({"name":"Project","root":root.path()}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    drop(s);
    std::fs::write(root.path().join("RULES.md"),"~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: write\n    effect: allow\n    actions: [post.create, response.create, post.edit]\naccess:\n  boards: []\n  threads: []\n~~~\n").unwrap();
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, None, None).unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    while swarm_platform::pipe::connect(&swarm_runtime::gui_pipe_name(root.path()).unwrap())
        .is_err()
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let (status, start) = cli(
        root.path(),
        &["identity", "start", "--project-id", &project],
    );
    assert!(status.success());
    assert!(
        start["data"]["identity"]["thread_rules_guidance"]["description_usage"]
            .as_str()
            .unwrap()
            .contains("thread-local rules")
    );
    let (_, denied) = cli(
        root.path(),
        &[
            "post",
            "create",
            "--thread-id",
            &thread,
            "--body",
            "Synthetic",
            "--request-id",
            "first",
        ],
    );
    assert_eq!(denied["error"]["code"], "THREAD_RULES_READ_REQUIRED");
    let (status, read) = cli(root.path(), &["thread", "get", "--id", &thread]);
    assert!(status.success());
    let ack = read["data"]["thread_rules"]["ack_token"].as_str().unwrap();
    let (status, posted) = cli(
        root.path(),
        &[
            "post",
            "create",
            "--thread-id",
            &thread,
            "--body",
            "Synthetic",
            "--request-id",
            "first",
            "--thread-rules-token",
            ack,
        ],
    );
    assert!(status.success());
    assert_eq!(posted["data"]["no"], 1);
    let mut s = Service::open(root.path()).unwrap();
    request(
        &mut s,
        "thread.edit",
        Some(&human),
        json!({"id":thread,"description":"Post in Japanese; No.900 owns Part.2 handover.","if_revision":1}),
    );
    drop(s);
    let (_, stale) = cli(
        root.path(),
        &[
            "post",
            "create",
            "--thread-id",
            &thread,
            "--body",
            "Synthetic next",
            "--request-id",
            "second",
            "--thread-rules-token",
            ack,
        ],
    );
    assert_eq!(stale["error"]["code"], "THREAD_RULES_READ_REQUIRED");
    let (_, list) = cli(root.path(), &["post", "list", "--thread-id", &thread]);
    let fresh = list["data"]["thread_rules"]["ack_token"].as_str().unwrap();
    assert_ne!(ack, fresh);
    let (status, posted) = cli(
        root.path(),
        &[
            "response",
            "create",
            "--thread-id",
            &thread,
            "--to",
            "1",
            "--body",
            "Synthetic reply",
            "--request-id",
            "reply",
            "--thread-rules-token",
            fresh,
        ],
    );
    assert!(status.success());
    assert_eq!(posted["data"]["no"], 2);
    let mut host = swarm_gui_host::GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Admin", "fixture-password")
        .unwrap();
    host.stop_runtime().unwrap();
    server.join().unwrap();
}
