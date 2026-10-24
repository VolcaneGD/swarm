//! Maintainer-documented saved harness exports, never live client discovery.
use crate::{
    invalid_input,
    model_report::{name, root},
};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
};
use swarm_core::protocol::Fault;

const MAX_REPORT: u64 = 64 * 1024 * 1024;
const MAX_RECORD: u64 = 1024 * 1024;

pub(super) fn supports(client: &str) -> bool {
    matches!(
        client,
        "hermes-agent" | "openclaw" | "gemini-cli" | "opencode" | "cursor" | "copilot-cli"
    )
}

fn matched_root(
    value: &Value,
    explicit: Option<&str>,
    allow_missing: bool,
) -> Result<String, Fault> {
    let reported = if value.is_null() && allow_missing {
        root(&json!(explicit.ok_or_else(invalid_input)?))?
    } else {
        root(value)?
    };
    if let Some(path) = explicit
        && root(&json!(path))? != reported
    {
        return Err(invalid_input());
    }
    Ok(reported)
}

fn json_record(file: File) -> Result<Value, Fault> {
    let mut bytes = Vec::new();
    file.take(MAX_RECORD + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid_input())?;
    if bytes.len() as u64 > MAX_RECORD {
        return Err(invalid_input());
    }
    let row: Value = serde_json::from_slice(&bytes).map_err(|_| invalid_input())?;
    if !row.is_object() {
        return Err(invalid_input());
    }
    Ok(row)
}

fn opencode_model(value: &Value) -> Option<String> {
    let model = name(&value["id"])?;
    let provider = name(&value["providerID"])?;
    name(&json!(format!("{provider}/{model}")))
}

fn opencode_legacy(
    export: &Value,
    session: &str,
    explicit: Option<&str>,
) -> Result<(String, Option<String>, &'static str), Fault> {
    let project = matched_root(&export["info"]["directory"], explicit, false)?;
    let messages = export["messages"].as_array().ok_or_else(invalid_input)?;
    let mut selected = None;
    let mut basis = "selection";
    for message in messages {
        let info = &message["info"];
        if !message.is_object()
            || message.get("type").is_some()
            || !info.is_object()
            || info["sessionID"] != session
        {
            return Err(invalid_input());
        }
        if let Some(path) = info.get("path")
            && (!path.is_object() || matched_root(&path["cwd"], explicit, false)? != project)
        {
            return Err(invalid_input());
        }
        match info["role"].as_str() {
            Some("assistant") => {
                // Native assistant cwd is mandatory; never infer it from a
                // repository root, body, redacted path or previous response.
                if info.get("path").is_none() {
                    return Err(invalid_input());
                }
                selected =
                    opencode_model(&json!({"id":info["modelID"],"providerID":info["providerID"]}));
                basis = "response";
            }
            Some("user") => {
                if let Some(model) = info.get("model")
                    && !model.is_object()
                {
                    return Err(invalid_input());
                }
                // A latest unknown selection clears an older known response.
                selected = opencode_model(
                    &json!({"id":info["model"]["modelID"],"providerID":info["model"]["providerID"]}),
                );
                basis = "selection";
            }
            _ => return Err(invalid_input()),
        }
    }
    Ok((project, selected, basis))
}

pub(super) fn read(
    client: &str,
    session: &str,
    file: File,
    explicit: Option<&str>,
) -> Result<Option<Value>, Fault> {
    if matches!(client, "cursor" | "copilot-cli") {
        return read_events(client, session, file, explicit);
    }
    let row = json_record(file)?;
    let mut basis = "selection";
    let (project_root, selected) = match client {
        "hermes-agent" => {
            // One JSON/JSONL session export, not a bulk backup or imported trace.
            if row["id"] != session || !row["messages"].is_array() {
                return Err(invalid_input());
            }
            (
                matched_root(&row["cwd"], explicit, true)?,
                name(&row["model"]),
            )
        }
        "gemini-cli" => {
            if row["session_id"] != session
                || row["hook_event_name"] != "BeforeModel"
                || !row["llm_request"].is_object()
            {
                return Err(invalid_input());
            }
            (
                matched_root(&row["cwd"], explicit, false)?,
                name(&row["llm_request"]["model"]),
            )
        }
        "openclaw" => {
            // metadata.json from the public trajectory export. Redacted paths
            // are deliberately not inferred; context is explicitly supplied.
            if row["traceSchema"] != "openclaw-trajectory"
                || row["schemaVersion"] != 1
                || row["sessionId"] != session
            {
                return Err(invalid_input());
            }
            (
                root(&json!(explicit.ok_or_else(invalid_input)?))?,
                name(&row["model"]["name"]),
            )
        }
        "opencode" => {
            // Native legacy and v2 SessionTransfer have distinct root fields.
            // Reject ambiguity rather than treating malformed v2 as legacy.
            let export = if row["data"]["info"].is_object() {
                &row["data"]
            } else {
                &row
            };
            if export["info"]["id"] != session {
                return Err(invalid_input());
            }
            match (
                export["info"].get("location"),
                export["info"].get("directory"),
            ) {
                (None, Some(_)) => {
                    let (project, selected, legacy_basis) =
                        opencode_legacy(export, session, explicit)?;
                    return Ok(selected.map(|model| json!({"client":client,"session_id":session,"project_root":project,"model_name":model,"basis":legacy_basis})));
                }
                (Some(_), None) => {}
                _ => return Err(invalid_input()),
            }
            let project = matched_root(&export["info"]["location"]["directory"], explicit, false)?;
            let messages = export["messages"].as_array().ok_or_else(invalid_input)?;
            let mut model = opencode_model(&export["info"]["model"]);
            for message in messages {
                if !message.is_object() || message.get("info").is_some() {
                    return Err(invalid_input());
                }
                match message["type"].as_str() {
                    Some("assistant" | "model-switched") => {
                        model = opencode_model(&message["model"]);
                        basis = if message["type"] == "assistant" {
                            "response"
                        } else {
                            "selection"
                        };
                    }
                    Some("location-switched") => {
                        if matched_root(&message["location"]["directory"], explicit, false)?
                            != project
                        {
                            return Err(invalid_input());
                        }
                    }
                    _ => {}
                }
            }
            (project, model)
        }
        _ => return Err(invalid_input()),
    };
    Ok(selected.map(|model| json!({"client":client,"session_id":session,"project_root":project_root,"model_name":model,"basis":basis})))
}

fn read_events(
    client: &str,
    session: &str,
    file: File,
    explicit: Option<&str>,
) -> Result<Option<Value>, Fault> {
    let mut reader = BufReader::new(file);
    let mut total = 0u64;
    let mut seen = false;
    let mut project_root = None;
    let mut selected = None;
    loop {
        let mut line = Vec::new();
        let count = (&mut reader)
            .take(MAX_RECORD + 1)
            .read_until(b'\n', &mut line)
            .map_err(|_| invalid_input())?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if count as u64 > MAX_RECORD || total > MAX_REPORT {
            return Err(invalid_input());
        }
        let row: Value = serde_json::from_slice(&line).map_err(|_| invalid_input())?;
        if !row.is_object() {
            return Err(invalid_input());
        }
        if client == "cursor" {
            if row.get("session_id").is_some_and(|id| id != session) {
                return Err(invalid_input());
            }
            if row["type"] == "system" && row["subtype"] == "init" {
                if seen || row["session_id"] != session {
                    return Err(invalid_input());
                }
                project_root = Some(matched_root(&row["cwd"], explicit, false)?);
                selected = name(&row["model"]);
                seen = true;
            } else if !seen {
                return Err(invalid_input());
            }
        } else {
            if !row["agentId"].is_null() || row["ephemeral"] == true {
                continue;
            }
            match row["type"].as_str() {
                Some("session.start") => {
                    if seen || row["data"]["sessionId"] != session {
                        return Err(invalid_input());
                    }
                    project_root = Some(matched_root(
                        &row["data"]["context"]["cwd"],
                        explicit,
                        true,
                    )?);
                    selected = name(&row["data"]["selectedModel"]);
                    seen = true;
                }
                Some("session.resume" | "session.model_change" | "session.context_changed") => {
                    if !seen || row["data"].get("sessionId").is_some_and(|id| id != session) {
                        return Err(invalid_input());
                    }
                    if row["type"] == "session.context_changed" {
                        if Some(matched_root(&row["data"]["cwd"], explicit, false)?) != project_root
                        {
                            return Err(invalid_input());
                        }
                    } else if row["type"] == "session.resume" {
                        if !row["data"]["context"]["cwd"].is_null()
                            && Some(matched_root(
                                &row["data"]["context"]["cwd"],
                                explicit,
                                false,
                            )?) != project_root
                        {
                            return Err(invalid_input());
                        }
                        selected = name(&row["data"]["selectedModel"]);
                    } else {
                        selected = name(&row["data"]["newModel"]);
                    }
                }
                _ if !seen => return Err(invalid_input()),
                _ => {}
            }
        }
    }
    if !seen {
        return Err(invalid_input());
    }
    Ok(selected.map(|model| json!({"client":client,"session_id":session,"project_root":project_root,"model_name":model,"basis":"selection"})))
}
