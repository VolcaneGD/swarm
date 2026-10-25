//! Mandatory best-effort acquisition at the CLI boundary; no client is launched.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

fn unavailable(reason: &str) -> Value {
    json!({"status":"unavailable","reason":reason})
}

fn from_report(client: &str, session: &str, path: &Path, root: Option<&str>) -> Value {
    match crate::model_report::read(client, session, path, root) {
        Ok(Some(report)) => json!({"status":"available","report":report}),
        Ok(None) => unavailable("model_not_exposed"),
        Err(_) => unavailable("report_unreadable_or_invalid"),
    }
}

// An explicit adapter environment works for every supported client, including
// generic engine reports. Never guess a model from a process name or scan for
// the newest conversation. Codex discovery selects only its exact session ID.
fn acquire(env: &BTreeMap<String, String>) -> Value {
    let keys = [
        "SWARM_MODEL_CLIENT",
        "SWARM_MODEL_SESSION_ID",
        "SWARM_MODEL_REPORT_FILE",
    ];
    if keys.iter().any(|key| env.contains_key(*key)) {
        let Some(client) = env.get(keys[0]) else {
            return unavailable("adapter_not_configured");
        };
        let Some(session) = env.get(keys[1]) else {
            return unavailable("adapter_not_configured");
        };
        let Some(path) = env.get(keys[2]) else {
            return unavailable("adapter_not_configured");
        };
        return from_report(
            client,
            session,
            Path::new(path),
            env.get("SWARM_MODEL_PROJECT_ROOT").map(String::as_str),
        );
    }
    let Some(session) = env
        .get("CODEX_THREAD_ID")
        .or_else(|| env.get("CODEX_SESSION_ID"))
    else {
        return unavailable("adapter_not_configured");
    };
    if uuid::Uuid::parse_str(session).is_err() {
        return unavailable("session_not_identified");
    }
    let home = env.get("CODEX_HOME").map(PathBuf::from).or_else(|| {
        env.get("USERPROFILE")
            .or_else(|| env.get("HOME"))
            .map(|p| Path::new(p).join(".codex"))
    });
    let Some(home) = home else {
        return unavailable("adapter_not_configured");
    };
    // Enumerate filenames only, at the documented year/month/day depth. Bound
    // both directory and entry counts; never read another session's contents.
    let mut pending = vec![(home.join("sessions"), 0)];
    let mut found = Vec::new();
    let mut visited = 0;
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries {
            visited += 1;
            if visited > 20000 {
                return unavailable("discovery_limit");
            }
            let Ok(entry) = entry else { continue };
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() && depth < 3 {
                pending.push((entry.path(), depth + 1));
            } else if kind.is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.ends_with(&format!("-{session}.jsonl")))
            {
                found.push(entry.path());
                if found.len() > 1 {
                    return unavailable("ambiguous_session");
                }
            }
        }
    }
    match found.first() {
        Some(path) => from_report("codex", session, path, None),
        None => unavailable("report_not_found"),
    }
}

pub(super) fn prepare(spec: &mut crate::RequestSpec) {
    if !matches!(
        spec.operation.as_str(),
        "post.create" | "response.create" | "post.edit"
    ) {
        return;
    }
    // Overwrite caller context: acquisition cannot be skipped by a CLI flag or
    // a context-file claim. Runtime still validates its normalized result.
    let env = [
        "SWARM_MODEL_CLIENT",
        "SWARM_MODEL_SESSION_ID",
        "SWARM_MODEL_REPORT_FILE",
        "SWARM_MODEL_PROJECT_ROOT",
        "CODEX_THREAD_ID",
        "CODEX_SESSION_ID",
        "CODEX_HOME",
        "USERPROFILE",
        "HOME",
    ]
    .into_iter()
    .filter_map(|key| Some((key.to_owned(), std::env::var(key).ok()?)))
    .collect();
    spec.context["model_capture"] = acquire(&env);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn exact_codex_session_latest_model_and_no_foreign_contents() {
        let home = tempfile::tempdir().unwrap();
        let dir = home.path().join("sessions/2026/10/25");
        fs::create_dir_all(&dir).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let report = [
            json!({"type":"session_meta","payload":{"id":id,"cwd":home.path()}}),
            json!({"type":"turn_context","payload":{"cwd":home.path(),"model":"model-one"}}),
            json!({"type":"turn_context","payload":{"cwd":home.path(),"model":"model-two"}}),
        ]
        .map(|v| v.to_string())
        .join("\n");
        fs::write(dir.join(format!("rollout-test-{id}.jsonl")), report).unwrap();
        fs::write(
            dir.join("rollout-foreign.jsonl"),
            "not valid JSON or model evidence",
        )
        .unwrap();
        let env = BTreeMap::from([
            ("CODEX_HOME".into(), home.path().to_str().unwrap().into()),
            ("CODEX_THREAD_ID".into(), id),
        ]);
        assert_eq!(acquire(&env)["report"]["model_name"], "model-two");
    }
    #[test]
    fn unsupported_and_incomplete_are_explicit_unavailable() {
        assert_eq!(
            acquire(&BTreeMap::new())["reason"],
            "adapter_not_configured"
        );
        let env = BTreeMap::from([("SWARM_MODEL_CLIENT".into(), "generic".into())]);
        assert_eq!(acquire(&env)["status"], "unavailable");
    }
    #[test]
    fn generic_engine_requires_matching_session_and_root() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("engine.json");
        fs::write(&path,json!({"schema_version":1,"session_id":"selected","project_root":dir.path(),"model_name":"local-weight-v2","engine_name":"Custom Engine"}).to_string()).unwrap();
        let report = from_report("generic", "selected", &path, None);
        assert_eq!(report["report"]["engine_name"], "Custom Engine");
        assert_eq!(
            from_report("generic", "foreign", &path, None)["status"],
            "unavailable"
        );
    }
}
