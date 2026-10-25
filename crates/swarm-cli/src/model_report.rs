//! Explicit, bounded client report reads. Never discover sessions or retain bodies.
use crate::invalid_input;
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};
use swarm_core::protocol::Fault;

const MAX_REPORT: u64 = 64 * 1024 * 1024;
const MAX_LINE: u64 = 1024 * 1024;

pub(super) fn root(value: &Value) -> Result<String, Fault> {
    let raw = value.as_str().ok_or_else(invalid_input)?;
    if raw.len() > 32768 || !Path::new(raw).is_absolute() {
        return Err(invalid_input());
    }
    let path = Path::new(raw).canonicalize().map_err(|_| invalid_input())?;
    if !path.is_dir() {
        return Err(invalid_input());
    }
    path.to_str().map(str::to_owned).ok_or_else(invalid_input)
}

pub(super) fn name(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|s| {
            !s.trim().is_empty()
                && s.len() <= 256
                && !s.chars().any(char::is_control)
                && !s.starts_with('<')
                && !matches!(
                    *s,
                    "不明" | "ローカルモデル" | "unknown" | "default" | "auto"
                )
        })
        .map(str::to_owned)
}

pub(super) fn read(
    client: &str,
    session: &str,
    path: &Path,
    explicit_root: Option<&str>,
) -> Result<Option<Value>, Fault> {
    if !(matches!(
        client,
        "codex" | "claude-code" | "openai-compatible" | "antigravity" | "generic"
    ) || crate::harness_report::supports(client))
        || session.is_empty()
        || session.len() > 128
        || session.chars().any(char::is_control)
    {
        return Err(invalid_input());
    }
    let file = File::open(path).map_err(|_| invalid_input())?;
    let meta = file.metadata().map_err(|_| invalid_input())?;
    if !meta.is_file() || meta.len() > MAX_REPORT {
        return Err(invalid_input());
    }
    if crate::harness_report::supports(client) {
        return crate::harness_report::read(client, session, file, explicit_root);
    }
    if matches!(client, "openai-compatible" | "antigravity" | "generic") {
        let mut bytes = Vec::new();
        file.take(MAX_LINE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| invalid_input())?;
        if bytes.len() as u64 > MAX_LINE {
            return Err(invalid_input());
        }
        let row: Value = serde_json::from_slice(&bytes).map_err(|_| invalid_input())?;
        if client == "generic" {
            if row["schema_version"] != 1 || row["session_id"] != session || explicit_root.is_some()
            {
                return Err(invalid_input());
            }
            let project_root = root(&row["project_root"])?;
            let engine = name(&row["engine_name"]).ok_or_else(invalid_input)?;
            return Ok(name(&row["model_name"]).map(|model| json!({"client":client,"session_id":session,"project_root":project_root,"model_name":model,"engine_name":engine,"basis":"response"})));
        }
        if client == "antigravity" {
            // Official hook input common fields. Do not infer models from
            // transcript bodies, internal protobufs, process names or UI labels.
            if row["conversationId"] != session {
                return Err(invalid_input());
            }
            let workspaces = row["workspacePaths"].as_array().ok_or_else(invalid_input)?;
            if workspaces.is_empty() || workspaces.len() > 64 {
                return Err(invalid_input());
            }
            let roots = workspaces.iter().map(root).collect::<Result<Vec<_>, _>>()?;
            let project_root = match explicit_root {
                Some(path) => {
                    let selected = root(&json!(path))?;
                    if !roots.contains(&selected) {
                        return Err(invalid_input());
                    }
                    selected
                }
                None if roots.len() == 1 => roots[0].clone(),
                None => return Err(invalid_input()),
            };
            return Ok(name(&row["modelName"]).map(|model| json!({"client":client,"session_id":session,"project_root":project_root,"model_name":model,"basis":"selection"})));
        }
        let project_root = root(&json!(explicit_root.ok_or_else(invalid_input)?))?;
        if row["id"] != session
            || !matches!(row["object"].as_str(), Some("chat.completion" | "response"))
        {
            return Err(invalid_input());
        }
        return Ok(name(&row["model"]).map(|model| json!({"client":client,"session_id":session,"project_root":project_root,"model_name":model,"basis":"response"})));
    }
    if explicit_root.is_some() {
        return Err(invalid_input());
    }
    let mut reader = BufReader::new(file);
    let mut total = 0;
    let mut project_root = None;
    let mut selected = None;
    let mut seen = false;
    loop {
        let mut line = Vec::new();
        let count = (&mut reader)
            .take(MAX_LINE + 1)
            .read_until(b'\n', &mut line)
            .map_err(|_| invalid_input())?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if count as u64 > MAX_LINE || total > MAX_REPORT {
            return Err(invalid_input());
        }
        let row: Value = serde_json::from_slice(&line).map_err(|_| invalid_input())?;
        if !row.is_object() {
            return Err(invalid_input());
        }
        if client == "codex" {
            match row["type"].as_str() {
                Some("session_meta") => {
                    if seen || row["payload"]["id"] != session {
                        return Err(invalid_input());
                    }
                    project_root = Some(root(&row["payload"]["cwd"])?);
                    seen = true;
                }
                Some("turn_context") => {
                    if !seen || Some(root(&row["payload"]["cwd"])?) != project_root {
                        return Err(invalid_input());
                    }
                    selected = name(&row["payload"]["model"]);
                }
                _ => {}
            }
        } else if row["type"] == "assistant" && row["isSidechain"] != true {
            if row["sessionId"] != session || row["message"]["role"] != "assistant" {
                return Err(invalid_input());
            }
            let current = root(&row["cwd"])?;
            if project_root.as_ref().is_some_and(|prior| prior != &current) {
                return Err(invalid_input());
            }
            project_root = Some(current);
            seen = true;
            // A synthetic latest response must not resurrect an older model.
            selected = name(&row["message"]["model"]);
        }
    }
    if !seen {
        return Err(invalid_input());
    }
    Ok(selected.map(|model| json!({"client":client,"session_id":session,"project_root":project_root,"model_name":model,"basis":if client == "codex" {"selection"} else {"response"}})))
}
