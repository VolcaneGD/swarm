use crate::{
    accounts::{digest, secret},
    projects,
    protocol::Fault,
    service::{required_text, timestamp},
};
use chrono::{Duration, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use uuid::Uuid;

fn deadline() -> String {
    (Utc::now() + Duration::days(7)).to_rfc3339_opts(SecondsFormat::Millis, true)
}
pub(crate) fn renew(db: &Connection, id: &str) -> Result<(), Fault> {
    db.execute(
        "UPDATE ai_identities SET expires_at=?1 WHERE id=?2",
        params![deadline(), id],
    )?;
    Ok(())
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientReport {
    client: String,
    session_id: String,
    project_root: String,
    model_name: String,
    basis: String,
}

fn invalid_report() -> Fault {
    Fault::new("INVALID_INPUT", 2, "Invalid client model report")
}

pub(crate) fn harness(db: &Connection, id: &str) -> Result<String, Fault> {
    Ok(db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [format!("identity-harness-v1:{id}")],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_else(|| "不明".into()))
}

fn record_harness(db: &Connection, id: &str, args: &Value) -> Result<(), Fault> {
    let client = args
        .get("model_client")
        .or_else(|| args.get("model_report").and_then(|r| r.get("client")));
    let Some(client) = client else {
        return Ok(());
    };
    let label = match client.as_str() {
        Some("codex") => "Codex",
        Some("claude-code") => "Claude Code",
        Some("antigravity") => "Antigravity",
        Some("hermes-agent") => "Hermes Agent",
        Some("openclaw") => "OpenClaw",
        Some("gemini-cli") => "Gemini CLI",
        Some("opencode") => "OpenCode",
        Some("cursor") => "Cursor",
        Some("copilot-cli") => "GitHub Copilot CLI",
        Some("openai-compatible") => "不明",
        _ => return Err(invalid_report()),
    };
    if let Some(reported) = args.get("model_report").and_then(|r| r.get("client"))
        && reported != client
    {
        return Err(invalid_report());
    }
    // An API response format identifies neither the originating harness nor
    // local execution. Preserve existing evidence rather than infer a name.
    if label == "不明" {
        return Ok(());
    }
    let prior = harness(db, id)?;
    if prior != "不明" && prior != label {
        return Err(Fault::new(
            "MODEL_CHANGED",
            6,
            "A harness change requires a new identity",
        ));
    }
    db.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![format!("identity-harness-v1:{id}"), label])?;
    Ok(())
}

fn model(db: &Connection, args: &Value, project: &str) -> Result<(String, &'static str), Fault> {
    if args
        .get("model_local")
        .is_some_and(|value| !value.is_boolean())
    {
        return Err(invalid_report());
    }
    let unknown = if args["model_local"] == true {
        "ローカルモデル"
    } else {
        "不明"
    };
    if let Some(value) = args.get("model_report") {
        let report: ClientReport =
            serde_json::from_value(value.clone()).map_err(|_| invalid_report())?;
        if !matches!(
            (report.client.as_str(), report.basis.as_str()),
            ("codex", "selection")
                | ("antigravity", "selection")
                | ("claude-code", "response")
                | ("openai-compatible", "response")
                | (
                    "hermes-agent" | "openclaw" | "gemini-cli" | "cursor" | "copilot-cli",
                    "selection"
                )
                | ("opencode", "selection" | "response")
        ) || report.session_id.trim().is_empty()
            || report.session_id.len() > 128
            || report.session_id.chars().any(char::is_control)
            || report.model_name.trim().is_empty()
            || report.model_name.len() > 256
            || report.model_name.chars().any(char::is_control)
            || report.model_name.starts_with('<')
            || matches!(
                report.model_name.as_str(),
                "不明" | "ローカルモデル" | "unknown" | "default" | "auto"
            )
            || report.project_root.len() > 32768
            || !std::path::Path::new(&report.project_root).is_absolute()
        {
            return Err(invalid_report());
        }
        let root = std::path::Path::new(&report.project_root)
            .canonicalize()
            .map_err(|_| invalid_report())?;
        let root = root.to_str().ok_or_else(invalid_report)?;
        let registered: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM project_paths WHERE project_id=?1 AND path=?2)",
            params![project, root],
            |row| row.get(0),
        )?;
        if !registered {
            return Err(invalid_report());
        }
        // Describes extraction from a client-reported field, never backend attestation.
        return Ok((report.model_name, "automatic"));
    }
    match args.get("model_name") {
        None => Ok((unknown.into(), "unknown")),
        Some(Value::String(name)) if name == "不明" => Ok((unknown.into(), "unknown")),
        Some(Value::String(name)) if name == "ローカルモデル" => {
            Ok((name.clone(), "unknown"))
        }
        Some(Value::String(name)) if !name.trim().is_empty() => Ok((name.clone(), "declared")),
        _ => Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Model name must be nonblank text or omitted",
        )),
    }
}
pub(crate) fn public(db: &Connection, id: &str) -> Result<Value, Fault> {
    db.query_row("SELECT id,project_id,model_name,model_source,display_id,expires_at,created_at FROM ai_identities WHERE id=?1",[id],|row|Ok(json!({
        "id":row.get::<_,String>(0)?,"project_id":row.get::<_,String>(1)?,"model_name":row.get::<_,String>(2)?,
        "model_source":row.get::<_,String>(3)?,"display_id":row.get::<_,String>(4)?,
        "expires_at":row.get::<_,String>(5)?,"created_at":row.get::<_,String>(6)?,
        "thread_rules_guidance":crate::thread_rules::guidance()
    }))).optional()?.ok_or_else(Fault::authentication)
}
pub(crate) fn authenticate(db: &Connection, auth: Option<&str>) -> Result<Option<String>, Fault> {
    let Some(token) = auth else { return Ok(None) };
    let identity:Option<(String,String)>=db.query_row(
        "SELECT i.id,i.expires_at FROM ai_sessions s JOIN ai_identities i ON i.id=s.actor_id WHERE s.token_hash=?1",
        [digest(token)],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
    match identity {
        Some((id, expires)) if expires > timestamp() => Ok(Some(id)),
        Some(_) => Err(Fault::authentication()),
        None => Ok(None),
    }
}
fn grant(db: &Connection, id: &str) -> Result<Value, Fault> {
    let session_token = secret();
    let resume_secret = secret();
    let session_id = Uuid::new_v4().to_string();
    db.execute("DELETE FROM ai_sessions WHERE actor_id=?1", [id])?;
    db.execute(
        "INSERT INTO ai_sessions(token_hash,actor_id,session_id) VALUES(?1,?2,?3)",
        params![digest(&session_token), id, session_id],
    )?;
    db.execute(
        "UPDATE ai_identities SET resume_hash=?1 WHERE id=?2",
        params![digest(&resume_secret), id],
    )?;
    // Internal grant for credential storage by the CLI adapter, not normal stdout.
    Ok(
        json!({"identity":public(db,id)?,"session_id":session_id,"session_token":session_token,"resume_secret":resume_secret}),
    )
}
pub(crate) fn start(db: &Connection, args: &Value) -> Result<Value, Fault> {
    let initialized: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE is_admin=1)",
        [],
        |row| row.get(0),
    )?;
    if !initialized {
        return Err(Fault::new(
            "SETUP_REQUIRED",
            3,
            "Complete the initial Admin setup in the GUI",
        ));
    }
    let project = required_text(args, "project_id")?;
    projects::get(db, project)?;
    let (name, source) = model(db, args, project)?;
    let id = Uuid::new_v4().to_string();
    db.execute("INSERT INTO actors(id,kind) VALUES(?1,'ai')", [&id])?;
    let display = loop {
        let candidate = Uuid::new_v4().simple().to_string()[..10].to_string();
        let exists: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM display_ids WHERE display_id=?1)",
            [&candidate],
            |row| row.get(0),
        )?;
        if !exists {
            break candidate;
        }
    };
    db.execute(
        "INSERT INTO display_ids(display_id,actor_id) VALUES(?1,?2)",
        params![display, id],
    )?;
    db.execute("INSERT INTO ai_identities(id,project_id,model_name,model_source,display_id,expires_at,resume_hash,created_at) VALUES(?1,?2,?3,?4,?5,?6,'',?7)",
        params![id,project,name,source,display,deadline(),timestamp()])?;
    record_harness(db, &id, args)?;
    grant(db, &id)
}
pub(crate) fn resume(db: &Connection, args: &Value) -> Result<Value, Fault> {
    let id = required_text(args, "actor_id")?;
    let project = required_text(args, "project_id")?;
    let (name, _) = model(db, args, project)?;
    let credential = required_text(args, "resume_secret")?;
    let eligible:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM ai_identities WHERE id=?1 AND project_id=?2 AND model_name=?3 AND resume_hash=?4 AND expires_at>?5)",
        params![id,project,name,digest(credential),timestamp()],|row|row.get(0))?;
    if !eligible {
        return Err(Fault::authentication());
    }
    record_harness(db, id, args)?;
    db.execute(
        "UPDATE ai_identities SET expires_at=?1 WHERE id=?2",
        params![deadline(), id],
    )?;
    grant(db, id)
}
pub(crate) fn dispatch(
    db: &Connection,
    operation: &str,
    args: &Value,
    id: &str,
) -> Result<Value, Fault> {
    match operation {
        "identity.status" => public(db, id),
        "identity.release" => {
            db.execute("DELETE FROM ai_sessions WHERE actor_id=?1", [id])?;
            Ok(json!({"released":true,"id":id}))
        }
        "identity.model" => {
            let current = public(db, id)?;
            if args.get("model_name").is_none()
                && args.get("model_report").is_none()
                && args.get("model_client").is_none()
                && args["model_local"] != true
            {
                return Err(invalid_report());
            }
            let project = current["project_id"].as_str().ok_or_else(Fault::internal)?;
            let (name, source) = model(db, args, project)?;
            record_harness(db, id, args)?;
            if current["model_name"] != name && current["model_source"] != "unknown" {
                return Err(Fault::new(
                    "MODEL_CHANGED",
                    6,
                    "A model change requires a new identity",
                ));
            }
            if name != "不明" && (current["model_source"] != "automatic" || source == "automatic")
            {
                db.execute(
                    "UPDATE ai_identities SET model_name=?1,model_source=?2 WHERE id=?3",
                    params![name, source, id],
                )?;
            }
            public(db, id)
        }
        _ => Err(Fault::new(
            "PERMISSION_DENIED",
            4,
            "AI board operations require the RULES.md evaluator, which is not implemented yet",
        )),
    }
}
