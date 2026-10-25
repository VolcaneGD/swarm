#![cfg(windows)]
use serde_json::{Value, json};
use std::{
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
use swarm_gui_host::GuiHost;

struct Runtime(Child);
impl Drop for Runtime {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn request(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}
fn cli(root: &Path, args: &[&str]) -> Value {
    // The command matrix exercises permitted writes, not the missing-proof
    // refusal (covered by thread_rules). Follow the current public protocol.
    let mut write_args = args.to_vec();
    let proof;
    if matches!(
        args.get(..2),
        Some(["post", "create" | "edit"] | ["response", "create"])
    ) && !args.contains(&"--thread-rules-token")
        && let Some(index) = args.iter().position(|arg| *arg == "--thread-id")
    {
        let current = cli(root, &["thread", "get", "--id", args[index + 1]]);
        proof = current["thread_rules"]["ack_token"]
            .as_str()
            .unwrap()
            .to_owned();
        write_args.extend(["--thread-rules-token", proof.as_str()]);
    }
    let args = write_args.as_slice();
    let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
    command.args(["--data-dir", root.to_str().unwrap(), "--format", "json"]);
    if args.first() != Some(&"runtime") {
        command.args(["--session", "command-matrix"]);
    }
    let mut process = command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = process.stdout.take().unwrap();
    let stderr = process.stderr.take().unwrap();
    let read = |stream: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stream
                .take(2 * 1024 * 1024)
                .read_to_end(&mut bytes)
                .unwrap();
            bytes
        })
    };
    let out = read(Box::new(stdout));
    let err = read(Box::new(stderr));
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = process.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = process.kill();
            let _ = process.wait();
            panic!("isolated CLI command exceeded its deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let bytes = out.join().unwrap();
    let error_bytes = err.join().unwrap();
    let parsed: Value = serde_json::from_slice(&bytes).unwrap();
    let code = parsed["error"]["code"].as_str().unwrap_or("NO_CODE");
    assert!(
        status.success(),
        "public command {:?} failed: {code}",
        &args[..args.len().min(2)]
    );
    assert!(error_bytes.is_empty(), "unexpected stderr (not printed)");
    let response = parsed;
    assert_eq!(response["ok"], true);
    assert!(response.get("token").is_none());
    response["data"].clone()
}

#[test]
fn documented_public_commands_execute_on_owned_runtime_and_persist_expected_results() {
    let data = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = service
        .request(request(
            "account.setup",
            None,
            json!({"name":"Matrix Admin","password":""}),
        ))
        .unwrap();
    let project = service
        .request(request(
            "project.register",
            admin["token"].as_str(),
            json!({"name":"Matrix Project","root":project_root.path()}),
        ))
        .unwrap();
    let project_id = project["id"].as_str().unwrap();
    // Fixture-only human policy, before any runtime starts. No production RULES.
    let policy = format!(
        "~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: matrix-mutators\n    effect: allow\n    subjects:\n      projects: [{project_id}]\n    actions: [board.create, board.edit, thread.create, thread.edit, post.create, response.create, post.edit, post.history, changes.get, project.register]\naccess:\n  boards: []\n  threads: []\n~~~\n"
    );
    std::fs::write(data.path().join("RULES.md"), policy).unwrap();
    drop(service);
    assert_eq!(
        cli(data.path(), &["runtime", "status"])["state"],
        "not_running"
    );
    let runtime_path = Path::new(env!("CARGO_BIN_EXE_swarm"))
        .parent()
        .unwrap()
        .join("swarm-runtime.exe");
    let mut runtime = Runtime(
        Command::new(runtime_path)
            .arg("--data-dir")
            .arg(data.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if swarm_platform::pipe::connect(&swarm_runtime::gui_pipe_name(data.path()).unwrap())
            .is_ok()
        {
            break;
        }
        assert!(runtime.0.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut host = GuiHost::open(data.path()).unwrap();
    host.authenticate("login", "Matrix Admin", "").unwrap();
    assert_eq!(cli(data.path(), &["runtime", "status"])["state"], "running");
    let identity = cli(
        data.path(),
        &["identity", "start", "--project-id", project_id],
    );
    let actor = identity["identity"]["id"].as_str().unwrap();
    assert_eq!(cli(data.path(), &["identity", "status"])["id"], actor);
    cli(
        data.path(),
        &["identity", "model", "--model-name", "Matrix Model"],
    );
    assert_eq!(
        cli(data.path(), &["project", "list"])["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        cli(data.path(), &["project", "get", "--id", project_id])["id"],
        project_id
    );
    cli(
        data.path(),
        &[
            "project",
            "register",
            "--name",
            "Second Project",
            "--root",
            second_root.path().to_str().unwrap(),
            "--request-id",
            "matrix-project",
        ],
    );
    let board = cli(data.path(), &["board", "create", "--name", "Matrix Board"]);
    let board_id = board["id"].as_str().unwrap();
    cli(
        data.path(),
        &[
            "board",
            "edit",
            "--id",
            board_id,
            "--name",
            "Edited Board",
            "--if-revision",
            "1",
        ],
    );
    assert_eq!(
        cli(data.path(), &["board", "get", "--id", board_id])["name"],
        "Edited Board"
    );
    assert_eq!(
        cli(data.path(), &["board", "list"])["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let thread = cli(
        data.path(),
        &[
            "thread",
            "create",
            "--board-id",
            board_id,
            "--name",
            "Matrix Thread",
        ],
    );
    let thread_id = thread["id"].as_str().unwrap();
    cli(
        data.path(),
        &[
            "thread",
            "edit",
            "--id",
            thread_id,
            "--description",
            "Edited description",
            "--if-revision",
            "1",
        ],
    );
    assert_eq!(
        cli(data.path(), &["thread", "get", "--id", thread_id])["description"],
        "Edited description"
    );
    assert_eq!(
        cli(data.path(), &["thread", "list", "--board-id", board_id])["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    cli(
        data.path(),
        &[
            "post",
            "create",
            "--thread-id",
            thread_id,
            "--body",
            "Matrix original",
        ],
    );
    cli(
        data.path(),
        &[
            "post",
            "edit",
            "--thread-id",
            thread_id,
            "--no",
            "1",
            "--body",
            "Matrix edited",
            "--if-revision",
            "1",
        ],
    );
    assert_eq!(
        cli(
            data.path(),
            &["post", "get", "--thread-id", thread_id, "--no", "1"]
        )["body"],
        "Matrix edited"
    );
    assert_eq!(
        cli(data.path(), &["post", "list", "--thread-id", thread_id])["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    cli(
        data.path(),
        &["post", "history", "--thread-id", thread_id, "--no", "1"],
    );
    let reply = cli(
        data.path(),
        &[
            "response",
            "create",
            "--thread-id",
            thread_id,
            "--to",
            "1",
            "--body",
            "Matrix reply",
        ],
    );
    assert_eq!(reply["no"], 2);
    assert_eq!(
        cli(
            data.path(),
            &["response", "list", "--thread-id", thread_id, "--to", "1"]
        )["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        cli(
            data.path(),
            &[
                "search",
                "run",
                "--query",
                "Matrix edited",
                "--scope",
                "body"
            ]
        )["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let changes = cli(data.path(), &["changes", "get", "--thread-id", thread_id]);
    let cursor = changes["change_cursor"].as_str().unwrap();
    assert!(
        cli(
            data.path(),
            &[
                "changes",
                "wait",
                "--thread-id",
                thread_id,
                "--after",
                cursor,
                "--timeout",
                "1"
            ]
        )["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        cli(
            data.path(),
            &[
                "rules",
                "check",
                "--operation",
                "post.create",
                "--thread-id",
                thread_id
            ]
        )["allowed"],
        true
    );
    cli(
        data.path(),
        &[
            "identity",
            "resume",
            "--actor-id",
            actor,
            "--project-id",
            project_id,
            "--model-name",
            "Matrix Model",
        ],
    );
    cli(data.path(), &["identity", "release"]);
    host.stop_runtime().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while runtime.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(cli(data.path(), &["runtime", "status"])["state"], "stopped");
    let db = rusqlite::Connection::open(data.path().join("swarm.sqlite3")).unwrap();
    let count: i64 = db
        .query_row("SELECT count(*) FROM posts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
}
