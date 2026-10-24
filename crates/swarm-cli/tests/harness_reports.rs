//! Public maintainer schema fixtures; not live harness adoption or native GUI QA.
use serde_json::{Value, json};
use std::{ffi::OsString, io::Cursor, path::Path};
use swarm_cli::{Command, parse_args};

fn parse(
    path: &Path,
    client: &str,
    root: Option<&Path>,
    fallback: Option<&str>,
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
        "session",
        "--model-report-file",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    args.push(path.into());
    if let Some(root) = root {
        args.extend(["--model-project-root".into(), root.into()]);
    }
    if let Some(fallback) = fallback {
        args.extend(["--model-name".into(), fallback.into()]);
    }
    match parse_args(&args, &mut Cursor::new([]))? {
        Command::Request(spec) => Ok(spec.args),
        _ => panic!("Expected identity request"),
    }
}

#[test]
fn session_export_and_hook_contracts_normalize_models_without_exporting_bodies() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.json");
    let cases = [
        (
            "hermes-agent",
            json!({"id":"session","cwd":dir.path(),"model":"Model-H","messages":[{"content":"PRIVATE-BODY"}]}),
            "Model-H",
            "selection",
        ),
        (
            "gemini-cli",
            json!({"session_id":"session","cwd":dir.path(),"hook_event_name":"BeforeModel","llm_request":{"model":"Model-G","messages":[{"content":"PRIVATE-BODY"}]}}),
            "Model-G",
            "selection",
        ),
        (
            "openclaw",
            json!({"traceSchema":"openclaw-trajectory","schemaVersion":1,"sessionId":"session","model":{"name":"Model-O"},"config":{"token":"PRIVATE-BODY"}}),
            "Model-O",
            "selection",
        ),
        (
            "opencode",
            json!({"info":{"id":"session","location":{"directory":dir.path()}},"messages":[{"type":"assistant","model":{"id":"Model-P","providerID":"provider"},"content":"PRIVATE-BODY"}]}),
            "provider/Model-P",
            "response",
        ),
    ];
    for (client, report, expected, basis) in cases {
        std::fs::write(&path, report.to_string()).unwrap();
        let args = parse(&path, client, Some(dir.path()), None).unwrap();
        assert_eq!(args["model_report"]["model_name"], expected);
        assert_eq!(args["model_report"]["basis"], basis);
        assert_eq!(args["model_client"], client);
        assert!(!args.to_string().contains("PRIVATE-BODY"));
        assert_eq!(
            args["model_report"]["project_root"],
            dir.path().canonicalize().unwrap().to_str().unwrap()
        );
    }
}

#[test]
fn cursor_and_copilot_events_bind_main_session_and_latest_selection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("events.jsonl");
    let cursor = [
        json!({"type":"system","subtype":"init","session_id":"session","cwd":dir.path(),"model":"Cursor-Model"}),
        json!({"type":"assistant","session_id":"session","message":{"content":"PRIVATE-BODY"}}),
    ];
    std::fs::write(
        &path,
        cursor
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    assert_eq!(
        parse(&path, "cursor", None, None).unwrap()["model_report"]["model_name"],
        "Cursor-Model"
    );
    let mut copilot = vec![
        json!({"type":"session.start","data":{"sessionId":"session","context":{"cwd":dir.path()},"selectedModel":"Old"}}),
        json!({"type":"session.model_change","data":{"newModel":"New"}}),
        json!({"type":"session.model_change","agentId":"subagent","data":{"newModel":"PRIVATE-BODY"}}),
    ];
    std::fs::write(
        &path,
        copilot
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let args = parse(&path, "copilot-cli", None, None).unwrap();
    assert_eq!(args["model_report"]["model_name"], "New");
    assert!(!args.to_string().contains("PRIVATE-BODY"));
    copilot.push(json!({"type":"session.model_change","data":{"newModel":"auto"}}));
    std::fs::write(
        &path,
        copilot
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let fallback = parse(&path, "copilot-cli", None, Some("Fallback")).unwrap();
    assert!(fallback.get("model_report").is_none());
    assert_eq!(fallback["model_name"], "Fallback");
    assert_eq!(fallback["model_client"], "copilot-cli");
}

#[test]
fn opencode_legacy_export_keeps_native_session_root_and_latest_model_without_bodies() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.json");
    let mut report = json!({"info":{"id":"session","directory":dir.path(),"version":"1.14.30"},"messages":[
        {"info":{"sessionID":"session","role":"user","model":{"modelID":"Old","providerID":"provider"}},"parts":[{"text":"PRIVATE-BODY"}]},
        {"info":{"sessionID":"session","role":"assistant","path":{"cwd":dir.path()},"modelID":"New","providerID":"provider"},"parts":[{"text":"PRIVATE-BODY"}]}
    ]});
    std::fs::write(&path, report.to_string()).unwrap();
    let args = parse(&path, "opencode", Some(dir.path()), None).unwrap();
    assert_eq!(args["model_report"]["model_name"], "provider/New");
    assert_eq!(args["model_report"]["basis"], "response");
    assert_eq!(
        args["model_report"]["project_root"],
        dir.path().canonicalize().unwrap().to_str().unwrap()
    );
    assert!(!args.to_string().contains("PRIVATE-BODY"));
    report["messages"].as_array_mut().unwrap().push(json!({"info":{"sessionID":"session","role":"user","model":{"modelID":"auto","providerID":"provider"}},"parts":[]}));
    std::fs::write(&path, report.to_string()).unwrap();
    assert!(
        parse(&path, "opencode", None, None)
            .unwrap()
            .get("model_report")
            .is_none()
    );
    assert_eq!(
        parse(&path, "opencode", None, Some("Declared")).unwrap()["model_name"],
        "Declared"
    );
    report["messages"][2]["info"]["model"]["modelID"] = json!("Selected");
    std::fs::write(&path, report.to_string()).unwrap();
    assert_eq!(
        parse(&path, "opencode", None, None).unwrap()["model_report"]["basis"],
        "selection"
    );
}

#[test]
fn opencode_legacy_rejects_foreign_mixed_redacted_and_changed_context() {
    let dir = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.json");
    let baseline = json!({"info":{"id":"session","directory":dir.path()},"messages":[{"info":{"sessionID":"session","role":"assistant","path":{"cwd":dir.path()},"modelID":"Model","providerID":"provider"},"parts":[]}]});
    let mut cases = vec![];
    let mut v = baseline.clone();
    v["info"]["id"] = json!("foreign");
    cases.push(v);
    let mut v = baseline.clone();
    v["messages"][0]["info"]["sessionID"] = json!("foreign");
    cases.push(v);
    let mut v = baseline.clone();
    v["messages"][0]["info"]["path"]["cwd"] = json!(other.path());
    cases.push(v);
    let mut v = baseline.clone();
    v["messages"][0]["info"]["path"] = Value::Null;
    cases.push(v);
    let mut v = baseline.clone();
    v["info"]["location"] = json!({"directory":dir.path()});
    cases.push(v);
    let mut v = baseline.clone();
    v["info"]["directory"] = json!("[redacted:session-directory]");
    cases.push(v);
    let mut v = baseline.clone();
    v["messages"][0]["type"] = json!("assistant");
    cases.push(v);
    let mut v = baseline.clone();
    v["messages"][0]["info"]["role"] = json!("system");
    cases.push(v);
    let mut v = baseline.clone();
    v["messages"] = json!([false]);
    cases.push(v);
    let mut v = baseline.clone();
    v["info"] = json!({"id":"session","location":{"directory":dir.path()},"model":{"id":"Header","providerID":"provider"}});
    cases.push(v);
    let mut v = baseline.clone();
    v["info"] = json!({"id":"session","location":{"directory":dir.path()}});
    v["messages"][0]["type"] = json!("assistant");
    v["messages"][0]["model"] = json!({"id":"Model","providerID":"provider"});
    cases.push(v);
    for v in cases {
        std::fs::write(&path, v.to_string()).unwrap();
        assert_eq!(
            parse(&path, "opencode", Some(dir.path()), Some("Fallback"))
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
    }
    std::fs::write(&path, baseline.to_string()).unwrap();
    assert_eq!(
        parse(&path, "opencode", Some(other.path()), None)
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
    let mut oversized = baseline;
    oversized["messages"][0]["parts"] = json!([{"text":"x".repeat(1024 * 1024)}]);
    std::fs::write(&path, oversized.to_string()).unwrap();
    assert_eq!(
        parse(&path, "opencode", None, None).unwrap_err().code,
        "INVALID_INPUT"
    );
}

#[test]
fn foreign_session_workspace_changes_and_missing_explicit_scope_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let foreign = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.json");
    for (client, report, explicit) in [
        (
            "hermes-agent",
            json!({"id":"other","cwd":dir.path(),"model":"M","messages":[]}),
            Some(dir.path()),
        ),
        (
            "gemini-cli",
            json!({"session_id":"session","cwd":foreign.path(),"hook_event_name":"BeforeModel","llm_request":{"model":"M"}}),
            Some(dir.path()),
        ),
        (
            "openclaw",
            json!({"traceSchema":"openclaw-trajectory","schemaVersion":1,"sessionId":"session","model":{"name":"M"}}),
            None,
        ),
        (
            "opencode",
            json!({"info":{"id":"session","location":{"directory":dir.path()}},"messages":[{"type":"location-switched","location":{"directory":foreign.path()}}]}),
            None,
        ),
    ] {
        std::fs::write(&path, report.to_string()).unwrap();
        assert_eq!(
            parse(&path, client, explicit, None).unwrap_err().code,
            "INVALID_INPUT"
        );
    }
    let events = [
        json!({"type":"session.start","data":{"sessionId":"session","context":{"cwd":dir.path()}}}),
        json!({"type":"session.context_changed","data":{"cwd":foreign.path()}}),
    ];
    std::fs::write(
        &path,
        events
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    assert_eq!(
        parse(&path, "copilot-cli", None, None).unwrap_err().code,
        "INVALID_INPUT"
    );
}

#[test]
fn single_session_export_requires_scope_when_cwd_is_missing_and_enforces_input_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.json");
    std::fs::write(
        &path,
        json!({"id":"session","model":"M","messages":[]}).to_string(),
    )
    .unwrap();
    assert_eq!(
        parse(&path, "hermes-agent", None, None).unwrap_err().code,
        "INVALID_INPUT"
    );
    assert_eq!(
        parse(&path, "hermes-agent", Some(dir.path()), None).unwrap()["model_report"]["model_name"],
        "M"
    );
    std::fs::write(&path, b"\xff").unwrap();
    assert_eq!(
        parse(&path, "gemini-cli", Some(dir.path()), None)
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
    std::fs::write(&path, " ".repeat(1024 * 1024 + 1)).unwrap();
    assert_eq!(
        parse(&path, "openclaw", Some(dir.path()), None)
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
}
