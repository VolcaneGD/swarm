#![cfg(windows)]
use serde_json::{Value, json};
use std::{
    process::Command,
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};

#[test]
fn real_cli_write_acquires_metadata_without_identity_flags_and_rechecks_each_write() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut call = |op: &str, auth: Option<&str>, args| {
        service
            .request(Request {
                operation: op.into(),
                auth: auth.map(str::to_owned),
                args,
                context: json!({}),
                request_id: None,
            })
            .unwrap()
    };
    let human = call(
        "account.setup",
        None,
        json!({"name":"Admin","password":"fixture-password"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let project = call(
        "project.register",
        Some(&human),
        json!({"name":"Engine","root":root.path()}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let board = call("board.create", Some(&human), json!({"name":"Models"}))["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let thread = call(
        "thread.create",
        Some(&human),
        json!({"board_id":board,"name":"Posts"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    drop(service);
    std::fs::write(root.path().join("RULES.md"),"# Rules\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: writing\n    effect: allow\n    actions: [post.create, response.create, post.edit]\naccess:\n  boards: []\n  threads: []\n~~~\n").unwrap();
    let report = root.path().join("engine.json");
    let write_report = |model: &str| {
        std::fs::write(&report,json!({"schema_version":1,"session_id":"current","project_root":root.path(),"model_name":model,"engine_name":"Engine","ignored_body":"PRIVATE-BODY"}).to_string()).unwrap()
    };
    write_report("engine-v1");
    let server_root = root.path().to_owned();
    let server = std::thread::spawn(move || swarm_runtime::serve(&server_root, Some(8)).unwrap());
    let deadline = Instant::now() + Duration::from_secs(10);
    let pipe = swarm_runtime::pipe_name(root.path()).unwrap();
    loop {
        if swarm_platform::pipe::connect(&pipe).is_ok() {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
            .args(["--data-dir"])
            .arg(root.path())
            .args([
                "--session",
                "engine",
                "--project",
                &project,
                "--format",
                "json",
            ])
            .args(args)
            .env("SWARM_MODEL_CLIENT", "generic")
            .env("SWARM_MODEL_SESSION_ID", "current")
            .env("SWARM_MODEL_REPORT_FILE", &report)
            .env_remove("SWARM_MODEL_PROJECT_ROOT")
            .output()
            .unwrap();
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(!text.contains("PRIVATE-BODY"));
        serde_json::from_str::<Value>(&text).unwrap()
    };
    assert_eq!(run(&["identity", "start"])["ok"], true);
    let post = run(&[
        "post",
        "create",
        "--thread-id",
        &thread,
        "--body",
        "first",
        "--request-id",
        "post",
        "--context-json",
        r#"{"model_capture":{"status":"available","report":{"model_name":"forged"}}}"#,
    ]);
    assert_eq!(post["data"]["author"]["model_name"], "engine-v1");
    assert_eq!(post["data"]["author"]["harness_name"], "Engine");
    assert_eq!(
        post["data"]["author"]["model_capture"]["status"],
        "available"
    );
    assert_eq!(
        run(&["post", "get", "--thread-id", &thread, "--no", "1"])["data"]["author"]["model_name"],
        "engine-v1"
    );
    std::fs::write(&report, "unreadable report").unwrap();
    let reply = run(&[
        "response",
        "create",
        "--thread-id",
        &thread,
        "--to",
        "1",
        "--body",
        "fallback",
        "--request-id",
        "reply",
    ]);
    assert_eq!(
        reply["data"]["author"]["model_capture"]["reason"],
        "report_unreadable_or_invalid"
    );
    assert_eq!(reply["data"]["author"]["model_name"], "不明");
    write_report("engine-v2");
    assert_eq!(
        run(&[
            "post",
            "edit",
            "--thread-id",
            &thread,
            "--no",
            "1",
            "--body",
            "changed",
            "--if-revision",
            "1"
        ])["error"]["code"],
        "MODEL_CHANGED"
    );
    assert_eq!(
        run(&["post", "get", "--thread-id", &thread, "--no", "1"])["data"]["body"],
        "first"
    );
    assert_eq!(run(&["identity", "release"])["ok"], true);
    server.join().unwrap();
}
