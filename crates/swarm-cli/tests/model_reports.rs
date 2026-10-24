use serde_json::{Value, json};
use std::{ffi::OsString, io::Cursor, path::Path};
use swarm_cli::{Command, parse_args};

fn parse(
    path: &Path,
    client: &str,
    session: &str,
    fallback: Option<&str>,
) -> Result<Value, swarm_core::protocol::Fault> {
    parse_with_root(path, client, session, fallback, None)
}

fn parse_with_root(
    path: &Path,
    client: &str,
    session: &str,
    fallback: Option<&str>,
    root: Option<&Path>,
) -> Result<Value, swarm_core::protocol::Fault> {
    let mut args: Vec<OsString> = [
        "--data-dir",
        "unused",
        "--session",
        "agent",
        "identity",
        "start",
        "--project-id",
        "project",
        "--model-client",
        client,
        "--model-session-id",
        session,
        "--model-report-file",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    args.push(path.into());
    if let Some(root) = root {
        args.extend(["--model-project-root".into(), root.into()]);
    }
    if let Some(name) = fallback {
        args.extend(["--model-name".into(), name.into()]);
    }
    match parse_args(&args, &mut Cursor::new([]))? {
        Command::Request(spec) => Ok(spec.args),
        _ => panic!("Expected identity request"),
    }
}

#[test]
fn explicit_codex_session_selection_keeps_its_basis_and_never_exports_transcript() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.jsonl");
    let rows = [
        json!({"type":"session_meta","payload":{"id":"session","cwd":dir.path()}}),
        json!({"type":"response_item","payload":{"content":"PRIVATE-BODY"}}),
        json!({"type":"turn_context","payload":{"model":"selected-model","cwd":dir.path()}}),
    ];
    std::fs::write(
        &path,
        rows.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
    )
    .unwrap();
    let args = parse(&path, "codex", "session", Some("Fallback")).unwrap();
    assert_eq!(args["model_report"]["model_name"], "selected-model");
    assert_eq!(args["model_report"]["basis"], "selection");
    assert!(!args.to_string().contains("PRIVATE-BODY"));
    assert_eq!(args["model_name"], "Fallback");
    assert_eq!(
        parse(&path, "codex", "other", None).unwrap_err().code,
        "INVALID_INPUT"
    );
}

#[test]
fn claude_response_model_is_extracted_but_synthetic_latest_response_is_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.jsonl");
    let row = |model| {
        json!({"type":"assistant","sessionId":"session","cwd":dir.path(),"message":{"role":"assistant","model":model,"content":"PRIVATE-BODY"}}).to_string() + "\n"
    };
    std::fs::write(&path, row("reported-model")).unwrap();
    let args = parse(&path, "claude-code", "session", None).unwrap();
    assert_eq!(args["model_report"]["basis"], "response");
    assert_eq!(args["model_report"]["model_name"], "reported-model");
    assert!(!args.to_string().contains("PRIVATE-BODY"));
    std::fs::write(&path, row("reported-model") + &row("<synthetic>")).unwrap();
    let args = parse(&path, "claude-code", "session", Some("Fallback")).unwrap();
    assert!(args.get("model_report").is_none());
    assert_eq!(args["model_name"], "Fallback");
}

#[test]
fn malformed_partial_foreign_and_unsupported_reports_are_sanitized_errors() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.jsonl");
    for contents in [
        "PRIVATE-BODY",
        "{}",
        "{\"type\":\"assistant\",\"sessionId\":\"other\"}\n",
    ] {
        std::fs::write(&path, contents).unwrap();
        let fault = parse(&path, "claude-code", "session", None).unwrap_err();
        assert_eq!(fault.code, "INVALID_INPUT");
        assert!(!fault.message.contains("PRIVATE-BODY"));
    }
    std::fs::write(&path, "{}\n").unwrap();
    assert_eq!(
        parse(&path, "antigravity", "session", None)
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
    assert_eq!(
        parse(dir.path(), "codex", "session", None)
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
}

#[test]
fn bounded_reads_reject_oversized_lines_files_and_invalid_utf8() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.jsonl");
    for bytes in [vec![b'x'; 1024 * 1024 + 1], vec![0xff, b'\n']] {
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(
            parse(&path, "codex", "session", None).unwrap_err().code,
            "INVALID_INPUT"
        );
    }
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(64 * 1024 * 1024 + 1).unwrap();
    assert_eq!(
        parse(&path, "codex", "session", None).unwrap_err().code,
        "INVALID_INPUT"
    );
}

#[test]
fn codex_missing_name_uses_fallback_and_changed_directory_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.jsonl");
    let meta = json!({"type":"session_meta","payload":{"id":"session","cwd":dir.path()}})
        .to_string()
        + "\n";
    std::fs::write(&path, &meta).unwrap();
    let args = parse(&path, "codex", "session", None).unwrap();
    assert!(args.get("model_report").is_none());
    assert!(args.get("model_name").is_none());
    let turn = json!({"type":"turn_context","payload":{"model":"new-model","cwd":other.path()}})
        .to_string()
        + "\n";
    std::fs::write(&path, meta + &turn).unwrap();
    assert_eq!(
        parse(&path, "codex", "session", None).unwrap_err().code,
        "INVALID_INPUT"
    );
}

#[test]
fn local_harness_reads_an_explicit_openai_response_and_project_root() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("response.json");
    for object in ["chat.completion", "response"] {
        std::fs::write(&path,json!({"id":"response-id","object":object,"model":"local-model","choices":[{"message":{"content":"PRIVATE-BODY"}}]}).to_string()).unwrap();
        let args: Vec<OsString> = [
            "--data-dir",
            "unused",
            "--session",
            "agent",
            "identity",
            "start",
            "--project-id",
            "project",
            "--model-client",
            "openai-compatible",
            "--model-session-id",
            "response-id",
            "--model-report-file",
        ]
        .into_iter()
        .map(Into::into)
        .chain([
            path.clone().into_os_string(),
            "--model-project-root".into(),
            dir.path().into(),
        ])
        .collect();
        let Command::Request(spec) = parse_args(&args, &mut Cursor::new([])).unwrap() else {
            panic!()
        };
        assert_eq!(spec.args["model_report"]["model_name"], "local-model");
        assert_eq!(spec.args["model_report"]["basis"], "response");
        assert!(!spec.args.to_string().contains("PRIVATE-BODY"));
        let mut mismatch = args.clone();
        let selector = mismatch.iter().position(|x| x == "response-id").unwrap();
        mismatch[selector] = "other".into();
        assert_eq!(
            parse_args(&mismatch, &mut Cursor::new([]))
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
    }
    assert_eq!(
        parse(&path, "openai-compatible", "response-id", None)
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
}

#[test]
fn antigravity_hook_selects_exact_conversation_and_discards_private_fields() {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let path = dir.path().join("hook.json");
    let mut row = json!({"conversationId":"conversation","workspacePaths":[dir.path()],
        "modelName":"reported-model","toolCall":{"args":{"body":"PRIVATE-BODY"}},
        "transcriptPath":"PRIVATE-TRANSCRIPT"});
    std::fs::write(&path, row.to_string()).unwrap();
    let args = parse(&path, "antigravity", "conversation", Some("Fallback")).unwrap();
    assert_eq!(args["model_report"]["model_name"], "reported-model");
    assert_eq!(args["model_report"]["basis"], "selection");
    assert!(!args.to_string().contains("PRIVATE-"));
    assert_eq!(
        parse(&path, "antigravity", "other", None).unwrap_err().code,
        "INVALID_INPUT"
    );
    row["modelName"] = json!("auto");
    std::fs::write(&path, row.to_string()).unwrap();
    assert!(
        parse(&path, "antigravity", "conversation", Some("Fallback"))
            .unwrap()
            .get("model_report")
            .is_none()
    );
    row["workspacePaths"] = json!([dir.path(), other.path()]);
    row["modelName"] = json!("reported-model");
    std::fs::write(&path, row.to_string()).unwrap();
    assert_eq!(
        parse(&path, "antigravity", "conversation", None)
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
    let chosen = parse_with_root(
        &path,
        "antigravity",
        "conversation",
        None,
        Some(other.path()),
    )
    .unwrap();
    assert_eq!(
        chosen["model_report"]["project_root"],
        other.path().canonicalize().unwrap().to_str().unwrap()
    );
    let foreign = tempfile::tempdir().unwrap();
    assert_eq!(
        parse_with_root(
            &path,
            "antigravity",
            "conversation",
            None,
            Some(foreign.path())
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );
}

#[test]
fn local_unknown_is_explicit_and_jsonl_accepts_a_complete_final_record() {
    let args: Vec<OsString> = [
        "--data-dir",
        "unused",
        "--session",
        "agent",
        "identity",
        "start",
        "--project-id",
        "project",
        "--local-model",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    let Command::Request(spec) = parse_args(&args, &mut Cursor::new([])).unwrap() else {
        panic!()
    };
    assert_eq!(spec.args["model_local"], true);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.jsonl");
    std::fs::write(&path,json!({"type":"assistant","sessionId":"session","cwd":dir.path(),"message":{"role":"assistant","model":"named"}}).to_string()).unwrap();
    assert_eq!(
        parse(&path, "claude-code", "session", None).unwrap()["model_report"]["model_name"],
        "named"
    );
}

#[cfg(windows)]
#[test]
fn real_cli_report_start_change_refusal_and_resume_use_private_pipe() {
    for client in [
        "claude-code",
        "antigravity",
        "hermes-agent",
        "openclaw",
        "gemini-cli",
        "opencode",
        "cursor",
        "copilot-cli",
    ] {
        report_private_pipe(client);
    }
}

#[cfg(windows)]
fn report_private_pipe(client: &str) {
    use std::{
        process::Command as Process,
        time::{Duration, Instant},
    };
    use swarm_core::{Service, protocol::Request};
    let root = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
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
    let admin = call(
        "account.setup",
        None,
        json!({"name":"Admin","password":"fixture-password"}),
    );
    let project = call(
        "project.register",
        admin["token"].as_str(),
        json!({"name":"Project","root":project_root.path()}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    drop(service);
    let path = root.path().join("report.jsonl");
    let row = |name| {
        match client {
            "antigravity" => json!({"conversationId":"session","workspacePaths":[project_root.path()],"modelName":name,"toolCall":{"args":{"body":"PRIVATE-BODY"}}}).to_string(),
            "hermes-agent" => json!({"id":"session","cwd":project_root.path(),"model":name,"messages":[{"content":"PRIVATE-BODY"}]}).to_string(),
            "openclaw" => json!({"traceSchema":"openclaw-trajectory","schemaVersion":1,"sessionId":"session","model":{"name":name},"config":{"token":"PRIVATE-BODY"}}).to_string(),
            "gemini-cli" => json!({"session_id":"session","cwd":project_root.path(),"hook_event_name":"BeforeModel","llm_request":{"model":name,"messages":[{"content":"PRIVATE-BODY"}]}}).to_string(),
            "opencode" => json!({"info":{"id":"session","location":{"directory":project_root.path()}},"messages":[{"type":"assistant","model":{"id":name,"providerID":"provider"},"content":"PRIVATE-BODY"}]}).to_string(),
            "cursor" => json!({"type":"system","subtype":"init","session_id":"session","cwd":project_root.path(),"model":name,"ignored":"PRIVATE-BODY"}).to_string(),
            "copilot-cli" => json!({"type":"session.start","data":{"sessionId":"session","context":{"cwd":project_root.path()},"selectedModel":name,"ignored":"PRIVATE-BODY"}}).to_string(),
            _ => json!({"type":"assistant","sessionId":"session","cwd":project_root.path(),"message":{"role":"assistant","model":name,"content":"PRIVATE-BODY"}}).to_string() + "\n",
        }
    };
    std::fs::write(&path, row("<synthetic>")).unwrap();
    let server_root = root.path().to_owned();
    let server = std::thread::spawn(move || swarm_runtime::serve(&server_root, Some(7)).unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    let pipe = swarm_runtime::pipe_name(root.path()).unwrap();
    loop {
        if let Ok(stream) = swarm_platform::pipe::connect(&pipe) {
            drop(stream);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let run = |action: &str, extra: &[&str], report: bool| {
        let mut cmd = Process::new(env!("CARGO_BIN_EXE_swarm"));
        cmd.arg("--data-dir").arg(root.path()).args([
            "--session",
            "agent",
            "--format",
            "json",
            "identity",
            action,
        ]);
        if report {
            cmd.arg("--local-model");
            cmd.args([
                "--model-client",
                client,
                "--model-session-id",
                "session",
                "--model-report-file",
            ])
            .arg(&path);
            if client == "openclaw" {
                cmd.arg("--model-project-root").arg(project_root.path());
            }
        }
        cmd.args(extra).output().unwrap()
    };
    let started = run("start", &["--project-id", &project], true);
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stdout)
    );
    let public: Value = serde_json::from_slice(&started.stdout).unwrap();
    let identity = &public["data"]["identity"];
    assert_eq!(identity["model_name"], "ローカルモデル");
    assert_eq!(identity["model_source"], "unknown");
    let id = identity["id"].as_str().unwrap();
    std::fs::write(&path, row("Reported")).unwrap();
    let enriched = run("model", &[], true);
    assert!(enriched.status.success());
    let enriched_value: Value = serde_json::from_slice(&enriched.stdout).unwrap();
    assert_eq!(enriched_value["data"]["model_source"], "automatic");
    assert_eq!(enriched_value["data"]["id"], identity["id"]);
    assert_eq!(enriched_value["data"]["expires_at"], identity["expires_at"]);
    let vault =
        swarm_platform::CredentialStore::open(&root.path().join("client-credentials")).unwrap();
    let old = vault.get("agent").unwrap().unwrap();
    std::fs::write(&path, row("Changed")).unwrap();
    let refused = run("model", &[], true);
    assert_eq!(refused.status.code(), Some(6));
    assert_eq!(vault.get("agent").unwrap().unwrap(), old);
    assert!(run("status", &[], false).status.success());
    std::fs::write(&path, row("Reported")).unwrap();
    let resumed = run(
        "resume",
        &["--actor-id", id, "--project-id", &project],
        true,
    );
    assert!(
        resumed.status.success(),
        "{}",
        String::from_utf8_lossy(&resumed.stdout)
    );
    assert_ne!(vault.get("agent").unwrap().unwrap(), old);
    assert!(run("release", &[], false).status.success());
    for output in [started, enriched, refused, resumed] {
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(!text.contains("PRIVATE-BODY"));
        assert!(!text.contains("session_token"));
        assert!(!text.contains("resume_secret"));
        assert!(!text.contains("project_root"));
    }
    server.join().unwrap();
}
