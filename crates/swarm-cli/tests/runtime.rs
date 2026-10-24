#![cfg(windows)]
use serde_json::{Value, json};
use std::{
    process::Command,
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
#[test]
fn actual_cli_reads_posts_and_reports_safe_json_errors_via_runtime() {
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
    service
        .request(request(
            "post.create",
            admin["token"].as_str(),
            json!({"thread_id":thread["id"],"body":"Handover from the fixture"}),
        ))
        .unwrap();
    let ai = service
        .request(request(
            "identity.start",
            None,
            json!({"project_id":project["id"],"model_name":"Fixture"}),
        ))
        .unwrap();
    let private = CredentialCandidate::from_grant("identity.start", &ai).unwrap();
    let credentials =
        swarm_platform::CredentialStore::open(&root.path().join("client-credentials")).unwrap();
    credentials
        .put("fixture", &serde_json::to_vec(&private).unwrap())
        .unwrap();
    credentials
        .put(
            "human",
            &serde_json::to_vec(&CredentialCandidate::from_grant("account.setup", &admin).unwrap())
                .unwrap(),
        )
        .unwrap();
    drop(service);
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || swarm_runtime::serve(&path, Some(12)).unwrap());
    // A real probe consumes one test-server connection without issuing an operation.
    let name = swarm_runtime::pipe_name(root.path()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(connection) = swarm_platform::pipe::connect(&name) {
            drop(connection);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let run = |format: &str, area: &str, action: &str, extra: &[&str], alias: &str| {
        Command::new(env!("CARGO_BIN_EXE_swarm"))
            .arg("--data-dir")
            .arg(root.path())
            .args(["--session", alias, "--format", format, area, action])
            .args(extra)
            .output()
            .unwrap()
    };
    let thread_id = thread["id"].as_str().unwrap();
    let listing = run(
        "json",
        "post",
        "list",
        &["--thread-id", thread_id],
        "fixture",
    );
    assert!(
        listing.status.success(),
        "{}",
        String::from_utf8_lossy(&listing.stderr)
    );
    let value: Value = serde_json::from_slice(&listing.stdout).unwrap();
    assert_eq!(
        value["data"]["items"][0]["body"],
        "Handover from the fixture"
    );
    let permission_check = run(
        "json",
        "rules",
        "check",
        &[
            "--operation",
            "post.get",
            "--thread-id",
            thread_id,
            "--post-no",
            "1",
        ],
        "fixture",
    );
    assert!(
        permission_check.status.success(),
        "stderr: {}; stdout: {}",
        String::from_utf8_lossy(&permission_check.stderr),
        String::from_utf8_lossy(&permission_check.stdout)
    );
    let value: Value = serde_json::from_slice(&permission_check.stdout).unwrap();
    assert_eq!(value["data"], json!({"allowed":true,"reason":"allowed"}));
    let prohibited_check = run(
        "markdown",
        "rules",
        "check",
        &[
            "--operation",
            "post.delete",
            "--thread-id",
            thread_id,
            "--post-no",
            "1",
        ],
        "fixture",
    );
    assert!(prohibited_check.status.success());
    let markdown = String::from_utf8_lossy(&prohibited_check.stdout);
    assert!(markdown.contains("operation_prohibited"));
    assert!(!markdown.contains(thread_id));
    let markdown = run(
        "markdown",
        "post",
        "list",
        &["--thread-id", thread_id],
        "fixture",
    );
    assert!(markdown.status.success());
    assert!(String::from_utf8_lossy(&markdown.stdout).contains("Handover from the fixture"));
    let denied = run(
        "json",
        "post",
        "create",
        &["--thread-id", thread_id, "--body", "private-denied-body"],
        "fixture",
    );
    assert_eq!(denied.status.code(), Some(4));
    let error: Value = serde_json::from_slice(&denied.stdout).unwrap();
    assert_eq!(error["ok"], false);
    for result in [&listing, &markdown, &denied] {
        let stdout = String::from_utf8_lossy(&result.stdout);
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(!stdout.contains(ai["session_token"].as_str().unwrap()));
        assert!(!stderr.contains(ai["resume_secret"].as_str().unwrap()));
    }
    assert!(!String::from_utf8_lossy(&denied.stdout).contains("private-denied-body"));
    std::fs::write(root.path().join("RULES.md"), format!("~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: posting\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [post.create]\n    when: {{ field: time.hour, op: gte, value: 0 }}\naccess:\n  boards: []\n  threads: []\n~~~\n",project["id"].as_str().unwrap())).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .arg("--data-dir")
        .arg(root.path())
        .args([
            "--session",
            "fixture",
            "--format",
            "json",
            "post",
            "create",
            "--thread-id",
            thread_id,
            "--stdin",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all("CLIからの引き継ぎ".as_bytes())
        .unwrap();
    let created = child.wait_with_output().unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let value: Value = serde_json::from_slice(&created.stdout).unwrap();
    assert_eq!(value["data"]["no"], 2);
    let listing = run(
        "json",
        "post",
        "list",
        &["--thread-id", thread_id],
        "fixture",
    );
    assert!(listing.status.success());
    let value: Value = serde_json::from_slice(&listing.stdout).unwrap();
    assert_eq!(value["data"]["items"][1]["body"], "CLIからの引き継ぎ");
    let search = run(
        "json",
        "search",
        "run",
        &["--query", "CLIからの引き継ぎ", "--scope", "post"],
        "fixture",
    );
    assert!(
        search.status.success(),
        "{}",
        String::from_utf8_lossy(&search.stderr)
    );
    let value: Value = serde_json::from_slice(&search.stdout).unwrap();
    assert_eq!(value["data"]["items"].as_array().unwrap().len(), 1);
    assert!(
        value["data"]["items"][0]["excerpt"]
            .as_str()
            .unwrap()
            .contains("CLIからの引き継ぎ")
    );
    let human = run("json", "board", "list", &[], "human");
    for (scope, query, kind, expected_id) in [
        ("body", "CLIからの引き継ぎ", "post", None),
        ("board_name", "Board", "board", board["id"].as_str()),
        ("thread_name", "Thread", "thread", Some(thread_id)),
    ] {
        let result = run(
            "json",
            "search",
            "run",
            &["--query", query, "--scope", scope],
            "fixture",
        );
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        let items = value["data"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "scope: {scope}");
        assert_eq!(value["data"]["total_count"], 1, "scope: {scope}");
        assert_eq!(items[0]["kind"], kind);
        if let Some(id) = expected_id {
            assert_eq!(items[0]["id"], id);
        }
    }
    assert_eq!(human.status.code(), Some(3));
    server.join().unwrap();
}
