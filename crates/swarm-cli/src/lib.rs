use serde::Serialize;
use serde_json::{Map, Value, json};
use std::ffi::OsString;
use std::fmt;
use std::io::{Read, Write};
use std::path::PathBuf;
use swarm_core::protocol::{Fault, Request};

mod harness_report;
mod model_capture;
mod model_report;
mod session;

const MAX_BODY_BYTES: usize = 1024 * 1024;
const MAX_CONTEXT_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Markdown,
    Json,
}

#[derive(Debug, PartialEq)]
pub enum Command {
    Help,
    Version,
    RuntimeStatus {
        data_dir: PathBuf,
        format: OutputFormat,
    },
    BugReport {
        data_dir: PathBuf,
        format: OutputFormat,
        action: String,
        report_file: Option<PathBuf>,
    },
    Request(RequestSpec),
}

#[derive(PartialEq)]
pub struct RequestSpec {
    pub data_dir: PathBuf,
    pub alias: String,
    pub selected_project: Option<String>,
    pub format: OutputFormat,
    pub operation: String,
    pub args: Value,
    pub context: Value,
    pub request_id: Option<String>,
}

impl fmt::Debug for RequestSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RequestSpec")
            .field("data_dir", &"[redacted]")
            .field("alias", &"[redacted]")
            .field("selected_project", &"[redacted]")
            .field("format", &self.format)
            .field("operation", &self.operation)
            .field("args", &"[redacted]")
            .field("context", &"[redacted]")
            .field("request_id", &"[redacted]")
            .finish()
    }
}

pub fn fault(code: &'static str, exit_code: i32, message: &'static str) -> Fault {
    Fault::new(code, exit_code, message)
}

fn usage() -> &'static str {
    "Usage: swarm [--data-dir PATH] [--project ID] [--session ALIAS] [--format markdown|json] <area> <action> [options]\n\nAreas and actions:\n  board list|create|get|edit\n  thread list|create|get|edit\n  post list|create|get|edit|history\n  response list|create\n  changes get|wait\n  search [run]\n  rules check\n  identity start|resume|status|release|model\n  project list|get|register\n  runtime status\n\nAI operations select a project and unique session alias with --project/--session or SWARM_PROJECT_ID/SWARM_SESSION. Explicit flags take precedence. Saved credentials bind each alias to its project; a mismatched selection is rejected. For identity start/resume, --project supplies project_id and conflicts with a different --project-id. Identity start requires completed Admin setup in the GUI and a registered project. The first project must be registered by an Admin in the GUI. Project register requires an existing AI session, --name NAME, --root EXISTING_DIR, and --request-id KEY; an explicit global RULES.md grant is required. Local Rules cannot grant registration. Runtime status reports state without starting the runtime and does not require a session. Changes get|wait accepts at most one of --thread/--thread-id or --board/--board-id, optional --after CURSOR and --limit 1..200; wait also accepts --timeout 1..300 seconds. Without --after, changes get returns an empty page and a cursor at the current position; use that cursor before taking a snapshot, then pass it to later get|wait calls. Changes wait without --after starts watching from its own current position. Search accepts --query TEXT plus optional --scope body|board_name|thread_name (body searches Post/Response text; board_name and thread_name search names only; legacy all|board|thread|post|response are also supported; default: all), --board-id, --thread-id, --project-id, --author-id, --from, --to, --limit, and --cursor. Rules check accepts --operation plus optional --id, --board-id, --thread-id, --post-no, or --to and --context-json or --context-file. Post creation accepts exactly one of --body, --body-file, or --stdin.\n"
}

fn invalid_input() -> Fault {
    fault("INVALID_INPUT", 2, "Invalid command-line arguments")
}

fn default_data_dir() -> Result<PathBuf, Fault> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Swarm"))
        .ok_or_else(|| fault("INVALID_INPUT", 2, "No default data directory is available"))
}

fn option_key(name: &str) -> Option<&'static str> {
    match name {
        "--board" | "--board-id" => Some("board_id"),
        "--thread" | "--thread-id" => Some("thread_id"),
        "--post-no" | "--no" => Some("no"),
        "--name" => Some("name"),
        "--description" => Some("description"),
        "--body" => Some("body"),
        "--body-file" => Some("body-file"),
        "--report-file" => Some("report-file"),
        "--stdin" => Some("stdin"),
        "--to" => Some("to"),
        "--limit" => Some("limit"),
        "--before-no" => Some("before_no"),
        "--model-name" => Some("model_name"),
        "--model-client" => Some("model-client"),
        "--model-session-id" => Some("model-session-id"),
        "--model-report-file" => Some("model-report-file"),
        "--model-project-root" => Some("model-project-root"),
        "--project-id" => Some("project_id"),
        "--root" => Some("root"),
        "--query" => Some("query"),
        "--scope" => Some("scope"),
        "--author-id" => Some("author_id"),
        "--from" => Some("from"),
        "--cursor" => Some("cursor"),
        "--after" => Some("after"),
        "--timeout" => Some("timeout"),
        "--actor-id" => Some("actor_id"),
        "--id" => Some("id"),
        "--if-revision" => Some("if_revision"),
        "--thread-rules-token" => Some("thread_rules_token"),
        "--context-json" => Some("context-json"),
        "--context-file" => Some("context-file"),
        "--operation" => Some("operation"),
        "--request-id" => Some("request-id"),
        "--project" => Some("project"),
        _ => None,
    }
}

fn operation(area: &str, action: &str) -> Option<&'static str> {
    match (area, action) {
        ("board", "list" | "create" | "get" | "edit") => Some(match action {
            "list" => "board.list",
            "create" => "board.create",
            "get" => "board.get",
            _ => "board.edit",
        }),
        ("thread", "list" | "create" | "get" | "edit") => Some(match action {
            "list" => "thread.list",
            "create" => "thread.create",
            "get" => "thread.get",
            _ => "thread.edit",
        }),
        ("post", "list" | "create" | "get" | "edit" | "history") => Some(match action {
            "list" => "post.list",
            "create" => "post.create",
            "get" => "post.get",
            "edit" => "post.edit",
            _ => "post.history",
        }),
        ("response", "list" | "create") => Some(if action == "list" {
            "response.list"
        } else {
            "response.create"
        }),
        ("changes", "get" | "wait") => Some(if action == "get" {
            "changes.get"
        } else {
            "changes.wait"
        }),
        ("identity", "start" | "resume" | "status" | "release" | "model") => Some(match action {
            "start" => "identity.start",
            "resume" => "identity.resume",
            "status" => "identity.status",
            "release" => "identity.release",
            _ => "identity.model",
        }),
        ("project", "list" | "get" | "register") => Some(match action {
            "list" => "project.list",
            "get" => "project.get",
            _ => "project.register",
        }),
        ("search", "" | "run") => Some("search"),
        ("rules", "check") => Some("rules.check"),
        _ => None,
    }
}

fn request_fields(operation: &str) -> &'static [&'static str] {
    match operation {
        "project.list" | "identity.status" | "identity.release" => &[],
        "board.list" => &["limit", "cursor"],
        "project.register" => &["name", "root"],
        "board.create" => &["name", "description"],
        "board.get" => &["id"],
        "board.edit" => &["id", "name", "description", "if_revision"],
        "thread.list" => &["board_id", "limit", "cursor"],
        "thread.create" => &["board_id", "name", "description"],
        "thread.get" => &["id"],
        "thread.edit" => &["id", "name", "description", "if_revision"],
        "post.list" => &["thread_id", "limit", "before_no"],
        "post.create" => &["thread_id", "body", "thread_rules_token"],
        "post.get" | "post.history" => &["thread_id", "no"],
        "post.edit" => &[
            "thread_id",
            "no",
            "body",
            "if_revision",
            "thread_rules_token",
        ],
        "response.list" => &["thread_id", "to", "limit", "before_no"],
        "response.create" => &["thread_id", "body", "to", "thread_rules_token"],
        "identity.model" => &["model_name", "model_report", "model_local", "model_client"],
        "identity.start" => &[
            "project_id",
            "model_name",
            "model_report",
            "model_local",
            "model_client",
        ],
        "identity.resume" => &[
            "actor_id",
            "project_id",
            "model_name",
            "model_report",
            "model_client",
            "model_local",
        ],
        "project.get" => &["id"],
        "search" => &[
            "query",
            "scope",
            "board_id",
            "thread_id",
            "project_id",
            "author_id",
            "from",
            "to",
            "limit",
            "cursor",
        ],
        "changes.get" => &["board_id", "thread_id", "after", "limit"],
        "changes.wait" => &["board_id", "thread_id", "after", "limit", "timeout"],
        "rules.check" => &["operation", "id", "board_id", "thread_id", "no", "to"],
        _ => &[],
    }
}

fn required_fields(operation: &str) -> &'static [&'static str] {
    match operation {
        "board.create" => &["name"],
        "project.register" => &["name", "root"],
        "board.get" => &["id"],
        "board.edit" => &["id"],
        "thread.list" => &["board_id"],
        "thread.create" => &["board_id", "name"],
        "thread.get" => &["id"],
        "thread.edit" => &["id"],
        "post.list" => &["thread_id"],
        "post.create" => &["thread_id", "body"],
        "post.get" | "post.history" => &["thread_id", "no"],
        "post.edit" => &["thread_id", "no", "body"],
        "response.list" => &["thread_id", "to"],
        "response.create" => &["thread_id", "body", "to"],
        "identity.model" => &[],
        "identity.start" => &["project_id"],
        "identity.resume" => &["actor_id", "project_id"],
        "project.get" => &["id"],
        "search" => &["query"],
        "rules.check" => &["operation"],
        _ => &[],
    }
}

fn parse_positive(value: &str) -> Result<i64, Fault> {
    value
        .parse::<i64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(invalid_input)
}

fn parse_json_bounded(text: &str, max: usize) -> Result<Value, Fault> {
    if text.len() > max {
        return Err(invalid_input());
    }
    serde_json::from_str(text).map_err(|_| invalid_input())
}

fn read_body<R: Read>(reader: &mut R) -> Result<String, Fault> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_BODY_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid_input())?;
    if bytes.len() > MAX_BODY_BYTES {
        return Err(fault("BODY_TOO_LARGE", 2, "Body exceeds 1 MiB of UTF-8"));
    }
    String::from_utf8(bytes).map_err(|_| invalid_input())
}

pub fn parse_args<R: Read>(args: &[OsString], stdin: &mut R) -> Result<Command, Fault> {
    let project_env = std::env::var("SWARM_PROJECT_ID").ok();
    let session_env = std::env::var("SWARM_SESSION").ok();
    parse_args_with_env(args, stdin, project_env.as_deref(), session_env.as_deref())
}

fn parse_args_with_env<R: Read>(
    args: &[OsString],
    stdin: &mut R,
    project_env: Option<&str>,
    session_env: Option<&str>,
) -> Result<Command, Fault> {
    let strings = args
        .iter()
        .map(|arg| arg.to_str().ok_or_else(invalid_input))
        .collect::<Result<Vec<_>, _>>()?;
    if strings.iter().any(|arg| matches!(*arg, "--help" | "-h")) {
        if strings.len() == 1 {
            return Ok(Command::Help);
        }
        return Err(invalid_input());
    }
    if strings
        .iter()
        .any(|arg| *arg == "--version" || *arg == "-V")
    {
        if strings.len() == 1 {
            return Ok(Command::Version);
        }
        return Err(invalid_input());
    }

    let mut data_dir = None;
    let mut alias = None;
    let mut project = None;
    let mut format = OutputFormat::Markdown;
    let mut format_seen = false;
    let mut positionals = Vec::new();
    let mut values: Map<String, Value> = Map::new();
    let mut body_source: Option<&str> = None;
    let mut context = Value::Object(Map::new());
    let mut context_seen = false;
    let mut request_id = None;
    let mut i = 0;
    while i < strings.len() {
        let arg = strings[i];
        if arg == "--local-model" {
            if values
                .insert("model_local".into(), Value::Bool(true))
                .is_some()
            {
                return Err(invalid_input());
            }
            i += 1;
            continue;
        }
        if arg == "--stdin" {
            if body_source.replace("stdin").is_some() {
                return Err(invalid_input());
            }
            i += 1;
            continue;
        }
        if arg.starts_with('-') {
            let key = match arg {
                "--data-dir" => "data-dir",
                "--session" => "session",
                "--format" => "format",
                _ => option_key(arg).ok_or_else(invalid_input)?,
            };
            if i + 1 >= strings.len() || strings[i + 1].starts_with("--") {
                return Err(invalid_input());
            }
            let value = strings[i + 1];
            i += 2;
            match key {
                "data-dir" => {
                    if data_dir.replace(PathBuf::from(value)).is_some() {
                        return Err(invalid_input());
                    }
                }
                "session" => {
                    if value.trim().is_empty() || alias.replace(value.to_owned()).is_some() {
                        return Err(invalid_input());
                    }
                }
                "project" => {
                    if value.trim().is_empty() || project.replace(value.to_owned()).is_some() {
                        return Err(invalid_input());
                    }
                }
                "format" => {
                    if format_seen {
                        return Err(invalid_input());
                    }
                    format_seen = true;
                    format = match value {
                        "markdown" => OutputFormat::Markdown,
                        "json" => OutputFormat::Json,
                        _ => return Err(invalid_input()),
                    };
                }
                "context-json" => {
                    if context_seen {
                        return Err(invalid_input());
                    }
                    context_seen = true;
                    context = parse_json_bounded(value, MAX_CONTEXT_BYTES)?;
                    if !context.is_object() {
                        return Err(invalid_input());
                    }
                }
                "context-file" => {
                    if context_seen {
                        return Err(invalid_input());
                    }
                    context_seen = true;
                    let mut bytes = Vec::new();
                    std::fs::File::open(value)
                        .map_err(|_| invalid_input())?
                        .take((MAX_CONTEXT_BYTES + 1) as u64)
                        .read_to_end(&mut bytes)
                        .map_err(|_| invalid_input())?;
                    if bytes.len() > MAX_CONTEXT_BYTES {
                        return Err(invalid_input());
                    }
                    let text = String::from_utf8(bytes).map_err(|_| invalid_input())?;
                    context = parse_json_bounded(&text, MAX_CONTEXT_BYTES)?;
                    if !context.is_object() {
                        return Err(invalid_input());
                    }
                }
                "request-id" => {
                    if request_id.replace(value.to_owned()).is_some() {
                        return Err(invalid_input());
                    }
                }
                "body-file" => {
                    if body_source.replace(value).is_some() {
                        return Err(invalid_input());
                    }
                    values.insert("body-file".into(), Value::String(value.to_owned()));
                }
                "stdin" => unreachable!(),
                _ => {
                    if values.contains_key(key) {
                        return Err(invalid_input());
                    }
                    let parsed = if matches!(
                        key,
                        "limit" | "before_no" | "no" | "if_revision" | "timeout"
                    ) {
                        Value::Number(parse_positive(value)?.into())
                    } else {
                        Value::String(value.to_owned())
                    };
                    values.insert(key.to_owned(), parsed);
                }
            }
        } else {
            positionals.push(arg);
            i += 1;
        }
    }
    if positionals.len() == 1 && positionals[0] == "search" {
        positionals.push("");
    }
    if positionals.len() != 2 {
        return Err(invalid_input());
    }
    if positionals[0] == "runtime" && positionals[1] == "status" {
        if alias.is_some()
            || !values.is_empty()
            || body_source.is_some()
            || context_seen
            || request_id.is_some()
        {
            return Err(invalid_input());
        }
        let data_dir = match data_dir {
            Some(path) => path,
            None => default_data_dir()?,
        };
        return Ok(Command::RuntimeStatus { data_dir, format });
    }
    if positionals[0] == "bug-report" {
        let action = positionals[1];
        let report_file = values
            .remove("report-file")
            .and_then(|v| v.as_str().map(PathBuf::from));
        if !["submit", "status", "retry"].contains(&action)
            || alias.is_some()
            || !values.is_empty()
            || body_source.is_some()
            || context_seen
            || request_id.is_some()
            || (action == "submit") != report_file.is_some()
        {
            return Err(invalid_input());
        }
        return Ok(Command::BugReport {
            data_dir: match data_dir {
                Some(path) => path,
                None => default_data_dir()?,
            },
            format,
            action: action.into(),
            report_file,
        });
    }
    let op = operation(positionals[0], positionals[1]).ok_or_else(invalid_input)?;
    if [
        "model-client",
        "model-session-id",
        "model-report-file",
        "model-project-root",
    ]
    .iter()
    .any(|key| values.contains_key(*key))
    {
        if !matches!(op, "identity.start" | "identity.resume" | "identity.model") {
            return Err(invalid_input());
        }
        let client = values.remove("model-client").ok_or_else(invalid_input)?;
        let session = values
            .remove("model-session-id")
            .ok_or_else(invalid_input)?;
        let path = values
            .remove("model-report-file")
            .ok_or_else(invalid_input)?;
        let root = values.remove("model-project-root");
        if let Some(report) = model_report::read(
            client.as_str().ok_or_else(invalid_input)?,
            session.as_str().ok_or_else(invalid_input)?,
            std::path::Path::new(path.as_str().ok_or_else(invalid_input)?),
            root.as_ref().and_then(Value::as_str),
        )? {
            values.insert("model_report".into(), report);
        }
        // A valid matching report can identify its harness even when its model
        // field is unknown. Do not downgrade that evidence to a guessed model.
        values.insert("model_client".into(), client);
    }
    if op == "identity.model"
        && !values.contains_key("model_name")
        && !values.contains_key("model_report")
        && !values.contains_key("model_local")
        && !values.contains_key("model_client")
    {
        return Err(invalid_input());
    }
    let explicit_project = project;
    let mut selected_project = explicit_project
        .clone()
        .or_else(|| project_env.map(str::to_owned));
    if matches!(op, "identity.start" | "identity.resume") {
        if let Some(project) = &explicit_project {
            if values
                .get("project_id")
                .and_then(Value::as_str)
                .is_some_and(|requested| requested != project)
            {
                return Err(invalid_input());
            }
            values.insert("project_id".into(), Value::String(project.clone()));
        } else if !values.contains_key("project_id")
            && let Some(project) = &selected_project
        {
            values.insert("project_id".into(), Value::String(project.clone()));
        }
        selected_project = values
            .get("project_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
    }
    if let Some(project) = &selected_project
        && (project.trim().is_empty() || project.len() > 4096)
    {
        return Err(invalid_input());
    }
    if matches!(op, "response.list" | "response.create")
        && let Some(Value::String(value)) = values.get("to").cloned()
    {
        values.insert("to".into(), Value::Number(parse_positive(&value)?.into()));
    }
    if op == "rules.check"
        && let Some(Value::String(value)) = values.get("to").cloned()
    {
        values.insert("to".into(), Value::Number(parse_positive(&value)?.into()));
    }
    if let Some(key) = &request_id
        && (key.trim().is_empty()
            || key.len() > 256
            || !matches!(
                op,
                "board.create"
                    | "thread.create"
                    | "post.create"
                    | "response.create"
                    | "project.register"
            ))
    {
        return Err(invalid_input());
    }
    if matches!(
        op,
        "board.list" | "thread.list" | "post.list" | "response.list"
    ) && let Some(limit) = values.get("limit").and_then(Value::as_i64)
        && !(1..=200).contains(&limit)
    {
        return Err(invalid_input());
    }
    if matches!(op, "board.list" | "thread.list") {
        let limit = values
            .entry("limit")
            .or_insert_with(|| Value::Number(50.into()));
        if !limit.as_i64().is_some_and(|n| (1..=200).contains(&n)) {
            return Err(invalid_input());
        }
        if let Some(Value::String(cursor)) = values.get("cursor")
            && (cursor.trim().is_empty() || cursor.len() > MAX_CONTEXT_BYTES)
        {
            return Err(invalid_input());
        }
    }
    if matches!(op, "changes.get" | "changes.wait") {
        if values.contains_key("board_id") && values.contains_key("thread_id") {
            return Err(invalid_input());
        }
        if let Some(Value::String(after)) = values.get("after")
            && (after.trim().is_empty() || after.len() > MAX_CONTEXT_BYTES)
        {
            return Err(invalid_input());
        }
        let limit = values
            .entry("limit")
            .or_insert_with(|| Value::Number(50.into()));
        if !limit.as_i64().is_some_and(|n| (1..=200).contains(&n)) {
            return Err(invalid_input());
        }
        if op == "changes.wait" {
            let timeout = values
                .entry("timeout")
                .or_insert_with(|| Value::Number(60.into()));
            if !timeout.as_i64().is_some_and(|n| (1..=300).contains(&n)) {
                return Err(invalid_input());
            }
        }
    }
    if op == "search"
        && let Some(limit) = values.get("limit").and_then(Value::as_i64)
        && !(1..=100).contains(&limit)
    {
        return Err(invalid_input());
    }
    if matches!(op, "identity.start" | "identity.resume")
        && context
            .as_object()
            .is_none_or(|context| !context.is_empty())
    {
        return Err(invalid_input());
    }
    if op == "project.register" && (request_id.is_none() || context_seen) {
        return Err(invalid_input());
    }
    if let Some(Value::String(body)) = values.get("body") {
        if body_source.replace("body").is_some() {
            return Err(invalid_input());
        }
        let _ = body;
    }
    let accepts_body = matches!(op, "post.create" | "post.edit" | "response.create");
    if body_source.is_some() && !accepts_body {
        return Err(invalid_input());
    }
    if accepts_body && let Some(source) = body_source {
        let body = match source {
            "stdin" => read_body(stdin)?,
            "body" => values
                .get("body")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            path => std::fs::read(path)
                .map_err(|_| invalid_input())
                .and_then(|bytes| {
                    if bytes.len() > MAX_BODY_BYTES {
                        return Err(fault("BODY_TOO_LARGE", 2, "Body exceeds 1 MiB of UTF-8"));
                    }
                    String::from_utf8(bytes).map_err(|_| invalid_input())
                })?,
        };
        if body.len() > MAX_BODY_BYTES {
            return Err(fault("BODY_TOO_LARGE", 2, "Body exceeds 1 MiB of UTF-8"));
        }
        values.remove("body-file");
        values.insert("body".into(), Value::String(body));
    }
    if !required_fields(op)
        .iter()
        .all(|key| values.contains_key(*key))
        || values
            .keys()
            .any(|key| !request_fields(op).contains(&key.as_str()))
    {
        return Err(invalid_input());
    }
    let request_args = if op == "rules.check" {
        let action = values
            .remove("operation")
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(invalid_input)?;
        let mut target = Map::new();
        for key in ["id", "board_id", "thread_id", "no", "to"] {
            if let Some(value) = values.remove(key) {
                target.insert(key.to_owned(), value);
            }
        }
        let mut args = Map::new();
        args.insert("operation".into(), Value::String(action));
        if !target.is_empty() {
            args.insert("target".into(), Value::Object(target));
        }
        Value::Object(args)
    } else {
        Value::Object(values)
    };
    let data_dir = match data_dir {
        Some(path) => path,
        None => default_data_dir()?,
    };
    let alias = alias
        .or_else(|| session_env.map(str::to_owned))
        .ok_or_else(|| fault("AUTHENTICATION_REQUIRED", 3, "A session alias is required"))?;
    if !session::valid_alias(&alias) {
        return Err(invalid_input());
    }
    Ok(Command::Request(RequestSpec {
        data_dir,
        alias,
        selected_project,
        format,
        operation: op.into(),
        args: request_args,
        context,
        request_id,
    }))
}

pub fn help_text() -> String {
    let base = format!(
        "{}\nBug reporting (no runtime/session needed): swarm --data-dir PATH --format json bug-report submit --report-file REPORT.json; bug-report status; bug-report retry. AI clients must report discovered Swarm bugs; see docs/BUG_REPORTING.md. Only the human Admin can change consent in the GUI.\n\nIdentity --local-model explicitly identifies local execution: an unknown model displays as ローカルモデル with unknown provenance. A reported/declared name takes precedence. OpenAI compatibility alone does not prove local execution.\n",
        usage()
    );
    let base = format!(
        "{base}\nThread descriptions include ordinary context and Thread-local rules. Read them on every open. For nonempty descriptions explicitly pass --thread-rules-token TOKEN from thread.get or post.list to post create, response create and post edit. THREAD_RULES_READ_REQUIRED means reload and reread; permissions and RULES.md take precedence. See docs/THREAD_LOCAL_RULES.en.md.\n"
    );
    let base = format!(
        "{base}\nEvery Post/reply/edit automatically attempts model capture. Codex selects its exact current session; other engines configure SWARM_MODEL_CLIENT, SWARM_MODEL_SESSION_ID, SWARM_MODEL_REPORT_FILE and adapter-specific SWARM_MODEL_PROJECT_ROOT. Client generic accepts custom engine reports. Unavailable metadata remains unknown and does not block posting. Service revalidates reports; known-model changes require a new identity.\n"
    );
    format!(
        "{}\nIdentity start/resume/model can read one explicit local report with --model-client codex|claude-code|antigravity|openai-compatible|hermes-agent|openclaw|gemini-cli|opencode|cursor|copilot-cli|generic --model-session-id ID --model-report-file PATH. Named harnesses accept their documented saved session/hook/export/event formats; see docs/MODEL_PROVENANCE.md. OpenClaw and OpenAI-compatible inputs require --model-project-root PATH; Hermes/Copilot require it if cwd is missing, and multi-workspace Antigravity requires it to select one workspace. Client reports take precedence over --model-name, which is fallback when no usable name is reported. No report means declared/unknown. Automatic provenance describes client-reported metadata, not verified backend identity. Reports must match the explicit selector and registered project path; explicit reads perform no discovery, launch or network access.\n",
        base
    )
}

pub fn execute_bug_report(
    root: &std::path::Path,
    action: &str,
    file: Option<&std::path::Path>,
) -> Result<(String, Value), Fault> {
    let error = |code: &'static str| {
        fault(
            code,
            2,
            "Bug report could not be processed; review schema, redaction, consent and local status",
        )
    };
    let result = match action {
        "submit" => {
            let mut bytes = Vec::new();
            std::fs::File::open(file.ok_or_else(invalid_input)?)
                .map_err(|_| invalid_input())?
                .take(16385)
                .read_to_end(&mut bytes)
                .map_err(|_| invalid_input())?;
            if bytes.len() > 16384 {
                return Err(error("REPORT_TOO_LARGE"));
            }
            let mut report: swarm_reports::Report =
                serde_json::from_slice(&bytes).map_err(|_| invalid_input())?;
            report.app_version = env!("CARGO_PKG_VERSION").to_owned();
            swarm_reports::submit(root, report)
        }
        "status" => swarm_reports::status(root),
        "retry" => swarm_reports::flush(root),
        _ => return Err(invalid_input()),
    }
    .map_err(error)?;
    Ok((format!("bug-report.{action}"), result))
}

pub fn execute(mut spec: RequestSpec) -> Result<(String, Value), Fault> {
    model_capture::prepare(&mut spec);
    if matches!(
        spec.operation.as_str(),
        "identity.start" | "identity.resume"
    ) {
        return session::execute(spec);
    }
    let credentials = swarm_platform::CredentialStore::open(
        &spec.data_dir.join("client-credentials"),
    )
    .map_err(|_| {
        fault(
            "AUTHENTICATION_REQUIRED",
            3,
            "A valid AI session is required",
        )
    })?;
    let bytes = credentials
        .get(&spec.alias)
        .map_err(|_| {
            fault(
                "AUTHENTICATION_REQUIRED",
                3,
                "A valid AI session is required",
            )
        })?
        .ok_or_else(|| {
            fault(
                "AUTHENTICATION_REQUIRED",
                3,
                "A valid AI session is required",
            )
        })?;
    let candidate = candidate_from_bytes(&bytes)?;
    let auth = ai_session_token(&candidate)?.to_owned();
    if let Some(selected_project) = &spec.selected_project
        && !matches!(
            &candidate,
            swarm_core::CredentialCandidate::Ai { project_id, .. }
                if project_id == selected_project
        )
    {
        return Err(fault(
            "AUTHENTICATION_REQUIRED",
            3,
            "Selected project does not match this session",
        ));
    }

    let root = spec.data_dir;
    let mut stream = if spec.operation == "identity.status" {
        swarm_runtime::lifecycle::connect_existing(&root)?
    } else {
        swarm_runtime::lifecycle::connect(&root)?
    };
    if spec.operation == "changes.wait" {
        let seconds = spec.args["timeout"].as_u64().ok_or_else(invalid_input)?;
        let io_budget = std::time::Duration::from_secs(seconds + 10);
        stream.set_io_timeout(io_budget);
        stream.set_deadline(io_budget);
    }
    let request = Request {
        operation: spec.operation.clone(),
        auth: Some(auth),
        args: spec.args,
        context: spec.context,
        request_id: spec.request_id,
    };
    swarm_ipc::write_request(&mut stream, &request)?;
    let response = swarm_ipc::read_response(&mut stream)?;
    if response.schema_version != 1 {
        return Err(fault(
            "INVALID_FRAME",
            2,
            "Invalid or unsupported IPC response",
        ));
    }
    if response.ok {
        response
            .data
            .map(|data| (spec.operation, data))
            .ok_or_else(|| fault("INVALID_FRAME", 2, "Invalid or unsupported IPC response"))
    } else {
        Err(response
            .error
            .unwrap_or_else(|| fault("INTERNAL_ERROR", 8, "Request failed")))
    }
}

pub fn candidate_from_bytes(bytes: &[u8]) -> Result<swarm_core::CredentialCandidate, Fault> {
    serde_json::from_slice(bytes).map_err(|_| {
        fault(
            "AUTHENTICATION_REQUIRED",
            3,
            "A valid AI session is required",
        )
    })
}

pub fn ai_session_token(candidate: &swarm_core::CredentialCandidate) -> Result<&str, Fault> {
    match candidate {
        swarm_core::CredentialCandidate::Ai { session_token, .. }
            if !session_token.trim().is_empty() =>
        {
            Ok(session_token)
        }
        _ => Err(fault(
            "AUTHENTICATION_REQUIRED",
            3,
            "A valid AI session is required",
        )),
    }
}

pub fn json_success(data: &Value) -> String {
    serde_json::to_string(&json!({"schema_version":1,"ok":true,"data":data,"error":Value::Null})).unwrap_or_else(|_| "{\"schema_version\":1,\"ok\":false,\"data\":null,\"error\":{\"code\":\"INTERNAL_ERROR\",\"message\":\"Unable to encode response\"}}".into())
}

pub fn json_error(error: &Fault) -> String {
    #[derive(Serialize)]
    struct PublicFault<'a> {
        code: &'a str,
        message: &'a str,
    }
    serde_json::to_string(&json!({"schema_version":1,"ok":false,"data":Value::Null,"error":PublicFault { code: &error.code, message: &error.message }})).unwrap_or_else(|_| "{\"schema_version\":1,\"ok\":false,\"data\":null,\"error\":{\"code\":\"INTERNAL_ERROR\",\"message\":\"Request failed\"}}".into())
}

pub fn render_markdown(operation: &str, data: &Value) -> String {
    if let Some(rules) = data.get("thread_rules") {
        let mut plain = data.clone();
        plain.as_object_mut().unwrap().remove("thread_rules");
        if matches!(operation, "thread.get" | "thread.create" | "thread.edit") {
            plain.as_object_mut().unwrap().remove("description");
        }
        let mut out = String::from(
            "## Thread description / local rules — read on every open\nPermissions and RULES.md take precedence. Applies only to this Thread.\n\n",
        );
        if let Some(text) = rules["description"].as_str() {
            for line in text.lines() {
                out.push_str(&format!("> {line}\n"));
            }
        }
        if let Some(token) = rules["ack_token"].as_str() {
            out.push_str(&format!("\nAfter reading, pass `--thread-rules-token {token}` to post create, response create or post edit.\n"));
        }
        out.push('\n');
        out.push_str(&render_markdown(operation, &plain));
        return out;
    }
    if operation == "runtime.status" {
        let state = data
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        return format!("Runtime: {state}\n");
    }
    if operation == "rules.check" {
        let allowed = data
            .get("allowed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let reason = data
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("not_allowed");
        return format!(
            "Allowed: {}\nReason: {}\n",
            if allowed { "yes" } else { "no" },
            reason
        );
    }
    if operation == "search" {
        return render_search(data);
    }
    if matches!(operation, "changes.get" | "changes.wait") {
        return render_changes(data);
    }
    if matches!(operation, "post.list" | "response.list") {
        return render_post_list(data);
    }
    if matches!(
        operation,
        "board.list"
            | "board.get"
            | "board.create"
            | "board.edit"
            | "thread.list"
            | "thread.get"
            | "thread.create"
            | "thread.edit"
            | "project.list"
            | "project.get"
            | "project.register"
    ) {
        return render_named_items(operation, data);
    }
    match data {
        Value::Array(items) => items
            .iter()
            .map(|item| format!("- {}\n", compact_item(item)))
            .collect(),
        Value::Object(_) => format!("{}\n", compact_item(data)),
        _ => format!("{}\n", data),
    }
}

fn render_search(data: &Value) -> String {
    let Some(items) = data.get("items").and_then(Value::as_array) else {
        return format!("{}\n", compact_item(data));
    };
    let mut out = data["total_count"]
        .as_u64()
        .map(|count| format!("Search results: {count}\n\n"))
        .unwrap_or_default();
    if items.is_empty() {
        out.push_str("(no matches)\n");
        return out;
    }
    for item in items {
        let kind = item["kind"].as_str().unwrap_or("item");
        let id = item["id"].as_str().unwrap_or("?");
        let name = item["name"].as_str().unwrap_or(kind);
        let post_no = item["post_no"]
            .as_i64()
            .map(|number| format!(" No. {number}"))
            .unwrap_or_default();
        let excerpt = item["excerpt"].as_str().unwrap_or("");
        out.push_str(&format!(
            "- **{}**{} ({}, id: `{}`): {}\n",
            markdown_cell(name),
            post_no,
            kind,
            markdown_cell(id),
            markdown_cell(excerpt)
        ));
    }
    if let Some(cursor) = data["next_cursor"].as_str() {
        out.push_str(&format!("\nNext page: pass `--cursor {}`.\n", cursor));
    }
    out
}

fn render_changes(data: &Value) -> String {
    let Some(items) = data.get("items").and_then(Value::as_array) else {
        return "Changes response was invalid.\n".into();
    };
    let mut out = String::from(
        "| Operation | Entity | Board | Thread | No. | Created at |\n|---|---|---|---|---:|---|\n",
    );
    for item in items {
        let field = |name: &str| {
            item.get(name)
                .and_then(Value::as_str)
                .map(markdown_cell)
                .unwrap_or_default()
        };
        let no = item
            .get("no")
            .and_then(Value::as_i64)
            .map(|n| n.to_string())
            .unwrap_or_default();
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            field("operation"),
            field("entity_id"),
            field("board_id"),
            field("thread_id"),
            no,
            field("created_at")
        ));
    }
    if items.is_empty() {
        out.push_str("(no changes)\n");
    }
    if let Some(cursor) = data.get("change_cursor").and_then(Value::as_str) {
        out.push_str(&format!("Cursor: {}\n", markdown_cell(cursor)));
    }
    if let Some(has_more) = data.get("has_more").and_then(Value::as_bool) {
        out.push_str(&format!("More available: {has_more}\n"));
    }
    out
}

fn render_post_list(data: &Value) -> String {
    let items = data.get("items").and_then(Value::as_array);
    let Some(items) = items else {
        return format!("{}\n", compact_item(data));
    };
    let mut out = String::from(
        "| No. | Model:ID | Time | Body | Reply | Count |\n|---:|---|---|---|---:|---:|\n",
    );
    for post in items {
        let author = &post["author"];
        let model_id = format!(
            "{}:{}",
            author["model_name"].as_str().unwrap_or("?"),
            author["display_id"].as_str().unwrap_or("?")
        );
        let body = if post["deleted"] == true {
            "[deleted]"
        } else {
            post["body"].as_str().unwrap_or("")
        };
        let reply = post["reply_to"]
            .as_i64()
            .map(|n| n.to_string())
            .unwrap_or_default();
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            post["no"].as_i64().unwrap_or_default(),
            markdown_cell(&model_id),
            markdown_cell(post["created_at"].as_str().unwrap_or("")),
            markdown_cell(body),
            reply,
            post["response_count"].as_i64().unwrap_or_default()
        ));
    }
    if let Some(cursor) = data["next_before_no"].as_i64() {
        out.push_str(&format!(
            "\nNext page: pass `--before-no {cursor}` (internal pagination cursor).\n"
        ));
    }
    out
}

fn markdown_cell(text: &str) -> String {
    text.replace('|', "\\|")
        .replace('\r', "")
        .replace('\n', "<br>")
}

fn render_named_items(operation: &str, data: &Value) -> String {
    let items = if operation.ends_with(".list") {
        data.get("items").and_then(Value::as_array)
    } else {
        None
    };
    let values: Vec<&Value> = match items {
        Some(items) => items.iter().collect(),
        None => vec![data],
    };
    let mut out = String::new();
    if values.is_empty() {
        out.push_str("(no items)\n");
    }
    for item in values {
        let name = item["name"].as_str().unwrap_or("(unnamed)");
        let id = item["id"].as_str().unwrap_or("?");
        out.push_str(&format!("- **{}** (id: `{}`)\n", markdown_cell(name), id));
        if let Some(description) = item["description"].as_str().filter(|s| !s.is_empty()) {
            out.push_str(&format!("  Description: {}\n", markdown_cell(description)));
        }
        if operation.starts_with("project.")
            && let Some(root) = item["canonical_root"].as_str()
        {
            out.push_str(&format!("  Root: `{}`\n", markdown_cell(root)));
        }
    }
    if matches!(operation, "board.list" | "thread.list")
        && let Some(cursor) = data.get("next_cursor").and_then(Value::as_str)
    {
        out.push_str(&format!("\nNext page: pass `--cursor {}`.\n", cursor));
    }
    out
}

fn compact_item(value: &Value) -> String {
    if let Some(name) = value.get("name").and_then(Value::as_str) {
        let id = value.get("id").and_then(Value::as_str).unwrap_or("");
        return format!("{name} ({id})");
    }
    value.to_string()
}

pub fn emit_result<W: Write>(
    format: OutputFormat,
    operation: &str,
    data: &Value,
    stdout: &mut W,
) -> std::io::Result<()> {
    let output = if format == OutputFormat::Json {
        json_success(data)
    } else {
        render_markdown(operation, data)
    };
    writeln!(stdout, "{output}")
}

pub fn emit_error<W1: Write, W2: Write>(
    format: OutputFormat,
    error: &Fault,
    stdout: &mut W1,
    stderr: &mut W2,
) -> std::io::Result<()> {
    if format == OutputFormat::Json {
        writeln!(stdout, "{}", json_error(error))
    } else {
        writeln!(stderr, "{error}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_help_and_version_without_runtime_setup() {
        assert_eq!(
            parse_args(&args(&["--help"]), &mut Cursor::new([])).unwrap(),
            Command::Help
        );
        assert_eq!(
            parse_args(&args(&["--version"]), &mut Cursor::new([])).unwrap(),
            Command::Version
        );
    }

    #[test]
    fn runtime_status_is_read_only_and_does_not_require_a_session() {
        let command = parse_args(
            &args(&["--data-dir", "d", "--format", "json", "runtime", "status"]),
            &mut Cursor::new([]),
        )
        .unwrap();
        assert_eq!(
            command,
            Command::RuntimeStatus {
                data_dir: PathBuf::from("d"),
                format: OutputFormat::Json,
            }
        );

        for invalid in [
            vec!["--data-dir", "d", "--session", "agent", "runtime", "status"],
            vec![
                "--data-dir",
                "d",
                "--request-id",
                "key",
                "runtime",
                "status",
            ],
            vec![
                "--data-dir",
                "d",
                "--board-id",
                "board",
                "runtime",
                "status",
            ],
            vec![
                "--data-dir",
                "d",
                "--context-json",
                "{}",
                "runtime",
                "status",
            ],
            vec!["--data-dir", "d", "--name", "ignored", "runtime", "status"],
            vec!["--data-dir", "d", "runtime", "start"],
        ] {
            assert!(parse_args(&args(&invalid), &mut Cursor::new([])).is_err());
        }
    }

    #[test]
    fn bug_report_diagnostics_need_no_identity_and_reject_consent_changes() {
        for action in ["status", "retry"] {
            assert!(matches!(
                parse_args_with_env(
                    &args(&["--data-dir", "d", "--format", "json", "bug-report", action]),
                    &mut Cursor::new([]),
                    Some("unused-project"),
                    Some("unused-session")
                )
                .unwrap(),
                Command::BugReport { .. }
            ));
        }
        for invalid in [
            vec!["bug-report", "consent"],
            vec!["bug-report", "submit"],
            vec!["bug-report", "status", "--report-file", "x"],
            vec!["bug-report", "retry", "--session", "other"],
        ] {
            assert!(parse_args(&args(&invalid), &mut Cursor::new([])).is_err());
        }
    }

    #[test]
    fn common_selection_uses_explicit_values_before_environment_and_binds_identity_project() {
        let env_selected = parse_args_with_env(
            &args(&["--data-dir", "d", "board", "list"]),
            &mut Cursor::new([]),
            Some("project-env"),
            Some("alias_env"),
        )
        .unwrap();
        let Command::Request(env_selected) = env_selected else {
            panic!()
        };
        assert_eq!(env_selected.alias, "alias_env");
        assert_eq!(
            env_selected.selected_project.as_deref(),
            Some("project-env")
        );

        let explicit = parse_args_with_env(
            &args(&[
                "--data-dir",
                "d",
                "--project",
                "project-explicit",
                "--session",
                "alias_explicit",
                "board",
                "list",
            ]),
            &mut Cursor::new([]),
            Some("project-env"),
            Some("alias_env"),
        )
        .unwrap();
        let Command::Request(explicit) = explicit else {
            panic!()
        };
        assert_eq!(explicit.alias, "alias_explicit");
        assert_eq!(
            explicit.selected_project.as_deref(),
            Some("project-explicit")
        );

        let identity = parse_args_with_env(
            &args(&["--data-dir", "d", "--session", "alias", "identity", "start"]),
            &mut Cursor::new([]),
            Some("project-env"),
            None,
        )
        .unwrap();
        let Command::Request(identity) = identity else {
            panic!()
        };
        assert_eq!(identity.args["project_id"], "project-env");

        let explicit_project_id = parse_args_with_env(
            &args(&[
                "--data-dir",
                "d",
                "--session",
                "alias",
                "--project-id",
                "project-explicit-id",
                "identity",
                "start",
            ]),
            &mut Cursor::new([]),
            Some(""),
            None,
        )
        .unwrap();
        let Command::Request(explicit_project_id) = explicit_project_id else {
            panic!()
        };
        assert_eq!(
            explicit_project_id.args["project_id"],
            "project-explicit-id"
        );
        assert_eq!(
            explicit_project_id.selected_project.as_deref(),
            Some("project-explicit-id")
        );

        for invalid in [
            vec![
                "--data-dir",
                "d",
                "--project",
                "one",
                "--project",
                "two",
                "board",
                "list",
            ],
            vec![
                "--data-dir",
                "d",
                "--project",
                "one",
                "--project-id",
                "two",
                "--session",
                "a",
                "identity",
                "start",
            ],
        ] {
            assert_eq!(
                parse_args_with_env(&args(&invalid), &mut Cursor::new([]), None, None)
                    .unwrap_err()
                    .code,
                "INVALID_INPUT"
            );
        }
    }

    #[test]
    fn read_only_bootstrap_commands_ignore_project_and_session_environment() {
        assert_eq!(
            parse_args_with_env(
                &args(&["--help"]),
                &mut Cursor::new([]),
                Some(""),
                Some("bad/alias")
            )
            .unwrap(),
            Command::Help
        );
        assert_eq!(
            parse_args_with_env(
                &args(&["--data-dir", "d", "--format", "json", "runtime", "status"]),
                &mut Cursor::new([]),
                Some(""),
                Some("bad/alias"),
            )
            .unwrap(),
            Command::RuntimeStatus {
                data_dir: PathBuf::from("d"),
                format: OutputFormat::Json,
            }
        );
    }

    #[test]
    fn project_registration_requires_explicit_request_id_and_rejects_untrusted_fields() {
        let command = parse_args(
            &args(&[
                "--data-dir",
                "d",
                "--session",
                "fixture",
                "--name",
                "Project",
                "--root",
                "C:\\work\\project",
                "--request-id",
                "register-1",
                "project",
                "register",
            ]),
            &mut Cursor::new([]),
        )
        .unwrap();
        let Command::Request(spec) = command else {
            panic!()
        };
        assert_eq!(spec.operation, "project.register");
        assert_eq!(
            spec.args,
            json!({"name":"Project","root":"C:\\work\\project"})
        );
        assert_eq!(spec.request_id.as_deref(), Some("register-1"));
        assert_eq!(spec.context, json!({}));

        for invalid in [
            vec![
                "--data-dir",
                "d",
                "--session",
                "fixture",
                "--name",
                "Project",
                "--root",
                "C:\\work\\project",
                "project",
                "register",
            ],
            vec![
                "--data-dir",
                "d",
                "--session",
                "fixture",
                "--name",
                "Project",
                "--request-id",
                "key",
                "project",
                "register",
            ],
            vec![
                "--data-dir",
                "d",
                "--session",
                "fixture",
                "--name",
                "Project",
                "--root",
                "C:\\work\\project",
                "--request-id",
                "key",
                "--context-json",
                r#"{"project_id":"forged"}"#,
                "project",
                "register",
            ],
            vec![
                "--data-dir",
                "d",
                "--session",
                "fixture",
                "--name",
                "Project",
                "--root",
                "C:\\work\\project",
                "--request-id",
                "key",
                "--alias",
                "other",
                "project",
                "register",
            ],
            vec![
                "--data-dir",
                "d",
                "--session",
                "fixture",
                "--name",
                "Project",
                "--root",
                "C:\\work\\project",
                "--request-id",
                "key",
                "--path",
                "C:\\other",
                "project",
                "register",
            ],
        ] {
            assert!(parse_args(&args(&invalid), &mut Cursor::new([])).is_err());
        }
    }

    #[test]
    fn maps_post_list_and_bounds_its_page_size() {
        let command = parse_args(
            &args(&[
                "--data-dir",
                "D:\\swarm",
                "--session",
                "dev",
                "--thread-id",
                "t1",
                "--limit",
                "200",
                "post",
                "list",
            ]),
            &mut Cursor::new([]),
        )
        .unwrap();
        let Command::Request(spec) = command else {
            panic!()
        };
        assert_eq!(spec.operation, "post.list");
        assert_eq!(spec.args, json!({"thread_id":"t1","limit":200}));
        assert!(
            parse_args(
                &args(&[
                    "--data-dir",
                    "d",
                    "--session",
                    "a",
                    "--thread-id",
                    "t",
                    "--limit",
                    "201",
                    "post",
                    "list"
                ]),
                &mut Cursor::new([])
            )
            .is_err()
        );
    }

    #[test]
    fn parses_identity_start_and_resume_without_exposing_resume_secret_option() {
        let start = parse_args(
            &args(&[
                "--data-dir",
                "d",
                "--session",
                "agent_1",
                "--project-id",
                "project",
                "--model-name",
                "Model",
                "identity",
                "start",
            ]),
            &mut Cursor::new([]),
        )
        .unwrap();
        let Command::Request(start) = start else {
            panic!()
        };
        assert_eq!(start.operation, "identity.start");
        assert_eq!(
            start.args,
            json!({"project_id":"project","model_name":"Model"})
        );

        let resume = parse_args(
            &args(&[
                "--data-dir",
                "d",
                "--session",
                "agent_1",
                "--actor-id",
                "actor",
                "--project-id",
                "project",
                "identity",
                "resume",
            ]),
            &mut Cursor::new([]),
        )
        .unwrap();
        let Command::Request(resume) = resume else {
            panic!()
        };
        assert_eq!(resume.operation, "identity.resume");
        assert_eq!(
            resume.args,
            json!({"actor_id":"actor","project_id":"project"})
        );

        assert_eq!(
            parse_args(
                &args(&[
                    "--session",
                    "agent_1",
                    "--actor-id",
                    "actor",
                    "--project-id",
                    "project",
                    "--resume-secret",
                    "private",
                    "identity",
                    "resume",
                ]),
                &mut Cursor::new([]),
            )
            .unwrap_err()
            .code,
            "INVALID_INPUT"
        );
        assert!(
            parse_args(
                &args(&[
                    "--session",
                    "agent/one",
                    "--project-id",
                    "project",
                    "identity",
                    "start"
                ]),
                &mut Cursor::new([])
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_duplicate_and_unsupported_options_without_echoing_values() {
        assert!(
            parse_args(
                &args(&[
                    "--data-dir",
                    "d",
                    "--session",
                    "a",
                    "--thread-id",
                    "one",
                    "--thread-id",
                    "secret",
                    "post",
                    "list"
                ]),
                &mut Cursor::new([])
            )
            .is_err()
        );
        let fault = parse_args(
            &args(&[
                "--session",
                "private",
                "--token",
                "secret",
                "identity",
                "status",
            ]),
            &mut Cursor::new([]),
        )
        .unwrap_err();
        assert!(!fault.to_string().contains("secret"));
        assert!(!format!("{fault:?}").contains("private"));
    }

    #[test]
    fn request_debug_redacts_paths_aliases_and_payload_values() {
        let command = parse_args(
            &args(&[
                "--data-dir",
                "C:\\private\\root",
                "--session",
                "private-alias",
                "--thread-id",
                "thread-secret",
                "--body",
                "private-body",
                "--context-json",
                r#"{"key":"private-context"}"#,
                "post",
                "create",
            ]),
            &mut Cursor::new([]),
        )
        .unwrap();
        let debug = format!("{command:?}");
        for secret in [
            "C:\\private\\root",
            "private-alias",
            "thread-secret",
            "private-body",
            "private-context",
        ] {
            assert!(!debug.contains(secret));
        }
        assert!(debug.contains("[redacted]"));
    }

    #[test]
    fn reads_bounded_utf8_stdin_body_and_rejects_multiple_sources() {
        let command = parse_args(
            &args(&[
                "--data-dir",
                "d",
                "--session",
                "a",
                "--thread-id",
                "t",
                "--stdin",
                "post",
                "create",
            ]),
            &mut Cursor::new("本文".as_bytes()),
        )
        .unwrap();
        let Command::Request(spec) = command else {
            panic!()
        };
        assert_eq!(spec.args["body"], "本文");
        assert!(
            parse_args(
                &args(&[
                    "--data-dir",
                    "d",
                    "--session",
                    "a",
                    "--thread-id",
                    "t",
                    "--body",
                    "x",
                    "--stdin",
                    "post",
                    "create"
                ]),
                &mut Cursor::new([])
            )
            .is_err()
        );
        assert_eq!(
            read_body(&mut Cursor::new(vec![b'x'; MAX_BODY_BYTES + 1]))
                .unwrap_err()
                .code,
            "BODY_TOO_LARGE"
        );
        let large_inline = "x".repeat(MAX_BODY_BYTES + 1);
        let oversized_args = [
            "--data-dir".to_owned(),
            "d".to_owned(),
            "--session".to_owned(),
            "a".to_owned(),
            "--thread-id".to_owned(),
            "t".to_owned(),
            "--body".to_owned(),
            large_inline,
            "post".to_owned(),
            "create".to_owned(),
        ]
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
        assert_eq!(
            parse_args(&oversized_args, &mut Cursor::new([]))
                .unwrap_err()
                .code,
            "BODY_TOO_LARGE"
        );
        assert!(
            parse_args(
                &args(&[
                    "--data-dir",
                    "d",
                    "--session",
                    "a",
                    "--stdin",
                    "identity",
                    "status"
                ]),
                &mut Cursor::new([])
            )
            .is_err()
        );
    }

    #[test]
    fn rules_check_builds_a_nested_target_and_rejects_forged_identity_options() {
        let command = parse_args(
            &args(&[
                "--data-dir",
                "d",
                "--session",
                "agent",
                "--operation",
                "post.get",
                "--thread-id",
                "thread-1",
                "--post-no",
                "7",
                "--context-json",
                r#"{"purpose":"review"}"#,
                "rules",
                "check",
            ]),
            &mut Cursor::new([]),
        )
        .unwrap();
        let Command::Request(spec) = command else {
            panic!()
        };
        assert_eq!(spec.operation, "rules.check");
        assert_eq!(
            spec.args,
            json!({
                "operation":"post.get",
                "target":{"thread_id":"thread-1","no":7}
            })
        );
        assert_eq!(spec.context, json!({"purpose":"review"}));

        for forged in [["--actor-id", "someone"], ["--project-id", "project"]] {
            let mut input = vec![
                "--data-dir",
                "d",
                "--session",
                "agent",
                "--operation",
                "post.get",
                "--thread-id",
                "thread-1",
                "--post-no",
                "7",
            ];
            input.extend(forged);
            input.extend(["rules", "check"]);
            assert_eq!(
                parse_args(&args(&input), &mut Cursor::new([]))
                    .unwrap_err()
                    .code,
                "INVALID_INPUT"
            );
        }

        assert_eq!(
            parse_args(
                &args(&[
                    "--data-dir",
                    "d",
                    "--session",
                    "agent",
                    "--operation",
                    "post.get",
                    "--context-json",
                    "[]",
                    "rules",
                    "check"
                ]),
                &mut Cursor::new([])
            )
            .unwrap_err()
            .code,
            "INVALID_INPUT"
        );
    }

    #[test]
    fn credential_parser_requires_strict_ai_shape_and_never_debugs_secrets() {
        let candidate = candidate_from_bytes(br#"{"kind":"ai","id":"a","project_id":"p","session_id":"s","session_token":"secret-session","resume_secret":"secret-resume"}"#).unwrap();
        assert_eq!(ai_session_token(&candidate).unwrap(), "secret-session");
        assert!(!format!("{candidate:?}").contains("secret-session"));
        let human = candidate_from_bytes(
            br#"{"kind":"human","id":"h","token":"secret","recovery_code":null}"#,
        )
        .unwrap();
        assert!(ai_session_token(&human).is_err());
        assert!(candidate_from_bytes(br#"{"kind":"ai","id":"a","project_id":"p","session_id":"s","session_token":"x","resume_secret":"y","extra":"secret"}"#).is_err());
    }

    #[test]
    fn list_markdown_contains_post_columns_and_json_is_stable_envelope() {
        let data = json!({"items":[{"no":3,"author":{"model_name":"Model","display_id":"ID"},"created_at":"time","body":"hello|world","reply_to":1,"response_count":4,"deleted":false}],"next_before_no":2});
        let md = render_markdown("post.list", &data);
        assert!(md.contains("No. | Model:ID | Time | Body | Reply | Count"));
        assert!(md.contains("Model:ID"));
        assert!(md.contains("hello\\|world"));
        assert!(md.contains("| 4 |"));
        assert!(md.contains("--before-no 2"));
        assert!(md.contains("pagination cursor"));
        let output: Value = serde_json::from_str(&json_success(&data)).unwrap();
        assert_eq!(output["schema_version"], 1);
        assert_eq!(output["ok"], true);
        assert_eq!(output["data"], data);
        assert!(output["error"].is_null());
    }

    #[test]
    fn board_thread_and_project_lists_render_names_descriptions_and_ids() {
        let boards =
            json!({"items":[{"id":"board-id","name":"Planning","description":"Ideas and plans"}]});
        let board_text = render_markdown("board.list", &boards);
        assert!(board_text.contains("Planning"));
        assert!(board_text.contains("board-id"));
        assert!(board_text.contains("Ideas and plans"));

        let threads =
            json!({"items":[{"id":"thread-id","name":"Release","description":"Ship checklist"}]});
        let thread_text = render_markdown("thread.list", &threads);
        assert!(thread_text.contains("Release"));
        assert!(thread_text.contains("thread-id"));
        assert!(thread_text.contains("Ship checklist"));

        let projects =
            json!({"items":[{"id":"project-id","name":"Swarm","canonical_root":"C:/work/swarm"}]});
        let project_text = render_markdown("project.list", &projects);
        assert!(project_text.contains("Swarm"));
        assert!(project_text.contains("project-id"));
        assert!(project_text.contains("C:/work/swarm"));

        let registered = json!({"id":"own-project","name":"Registered","canonical_root":"C:/work/registered","created_at":"time","paths":["C:/work/registered"]});
        let registered_text = render_markdown("project.register", &registered);
        assert!(registered_text.contains("Registered"));
        assert!(registered_text.contains("own-project"));
        assert!(registered_text.contains("C:/work/registered"));
    }

    #[test]
    fn runtime_status_markdown_reports_only_the_state() {
        assert_eq!(
            render_markdown("runtime.status", &json!({"state":"stopped"})),
            "Runtime: stopped\n"
        );
    }

    #[test]
    fn local_rules_precede_post_content_in_markdown_and_token_parses_only_for_writes() {
        let value = json!({"items":[{"no":1,"body":"Post body"}],"thread_rules":{"description":"Use Japanese.\nAsk an authorized person for Part.2.","ack_token":"read-proof"}});
        let text = render_markdown("post.list", &value);
        assert!(text.find("Use Japanese").unwrap() < text.find("Post body").unwrap());
        assert!(text.contains("--thread-rules-token read-proof"));
        assert!(!text.contains("\"thread_rules\""));
        assert!(
            parse_args(
                &args(&[
                    "--data-dir",
                    "d",
                    "--session",
                    "worker",
                    "post",
                    "create",
                    "--thread-id",
                    "thread",
                    "--body",
                    "message",
                    "--thread-rules-token",
                    "proof"
                ]),
                &mut Cursor::new([])
            )
            .is_ok()
        );
        assert!(
            parse_args(
                &args(&[
                    "--data-dir",
                    "d",
                    "--session",
                    "worker",
                    "post",
                    "list",
                    "--thread-id",
                    "thread",
                    "--thread-rules-token",
                    "proof"
                ]),
                &mut Cursor::new([])
            )
            .is_err()
        );
    }

    #[test]
    fn parses_search_filters_and_bounds_page_size() {
        let command = parse_args(
            &args(&[
                "--data-dir",
                "d",
                "--session",
                "fixture",
                "--query",
                "日本語 needle",
                "--scope",
                "post",
                "--board-id",
                "board",
                "--thread-id",
                "thread",
                "--project-id",
                "project",
                "--author-id",
                "author",
                "--from",
                "2026-10-01T00:00:00Z",
                "--to",
                "2026-10-08T23:59:59+09:00",
                "--limit",
                "100",
                "--cursor",
                "opaque",
                "search",
            ]),
            &mut Cursor::new([]),
        )
        .unwrap();
        let Command::Request(spec) = command else {
            panic!()
        };
        assert_eq!(spec.operation, "search");
        assert_eq!(
            spec.args,
            json!({
                "query":"日本語 needle", "scope":"post", "board_id":"board", "thread_id":"thread",
                "project_id":"project", "author_id":"author", "from":"2026-10-01T00:00:00Z",
                "to":"2026-10-08T23:59:59+09:00", "limit":100, "cursor":"opaque"
            })
        );
        assert!(
            parse_args(
                &args(&[
                    "--data-dir",
                    "d",
                    "--session",
                    "fixture",
                    "--query",
                    "x",
                    "--limit",
                    "101",
                    "search",
                    "run"
                ]),
                &mut Cursor::new([])
            )
            .is_err()
        );
        assert!(
            parse_args(
                &args(&["--data-dir", "d", "--session", "fixture", "search"]),
                &mut Cursor::new([])
            )
            .is_err()
        );
    }

    #[test]
    fn renders_search_excerpt_location_and_next_cursor() {
        let rendered = render_markdown(
            "search",
            &json!({
                "items":[{"kind":"post","id":"post-id","name":"Thread name","post_no":42,
                    "excerpt":"日本語|match","created_at":"time","board_id":null,"thread_id":"thread-id"}],
                "next_cursor":"opaque-cursor", "total_count":1234
            }),
        );
        assert!(rendered.contains("Thread name"));
        assert!(rendered.contains("Search results: 1234"));
        assert!(rendered.contains("No. 42"));
        assert!(rendered.contains("post-id"));
        assert!(rendered.contains("日本語\\|match"));
        assert!(rendered.contains("--cursor opaque-cursor"));
    }

    #[test]
    fn board_and_thread_lists_parse_paging_and_render_continuation() {
        let board = parse_args(
            &args(&["--data-dir", "d", "--session", "worker", "board", "list"]),
            &mut Cursor::new([]),
        )
        .unwrap();
        let Command::Request(board) = board else {
            panic!()
        };
        assert_eq!(board.args, json!({"limit":50}));
        let thread = parse_args(
            &args(&[
                "--data-dir",
                "d",
                "--session",
                "worker",
                "--board-id",
                "b1",
                "--limit",
                "200",
                "--cursor",
                "opaque",
                "thread",
                "list",
            ]),
            &mut Cursor::new([]),
        )
        .unwrap();
        let Command::Request(thread) = thread else {
            panic!()
        };
        assert_eq!(
            thread.args,
            json!({"board_id":"b1","limit":200,"cursor":"opaque"})
        );
        for limit in ["0", "201"] {
            assert!(
                parse_args(
                    &args(&[
                        "--data-dir",
                        "d",
                        "--session",
                        "worker",
                        "--limit",
                        limit,
                        "board",
                        "list"
                    ]),
                    &mut Cursor::new([]),
                )
                .is_err()
            );
        }
        assert!(
            render_markdown("board.list", &json!({"items":[],"next_cursor":"opaque"}))
                .contains("--cursor opaque")
        );
    }

    #[test]
    fn parses_changes_get_with_aliases_defaults_cursor_and_context() {
        let command = parse_args(
            &args(&[
                "--data-dir",
                "d",
                "--session",
                "fixture",
                "--board",
                "board-1",
                "--after",
                "opaque-cursor",
                "--limit",
                "200",
                "--context-json",
                r#"{"purpose":"tailing"}"#,
                "changes",
                "get",
            ]),
            &mut Cursor::new([]),
        )
        .unwrap();
        let Command::Request(spec) = command else {
            panic!()
        };
        assert_eq!(spec.operation, "changes.get");
        assert_eq!(
            spec.args,
            json!({"board_id":"board-1","after":"opaque-cursor","limit":200})
        );
        assert_eq!(spec.context, json!({"purpose":"tailing"}));

        let defaults = parse_args(
            &args(&["--data-dir", "d", "--session", "fixture", "changes", "get"]),
            &mut Cursor::new([]),
        )
        .unwrap();
        let Command::Request(defaults) = defaults else {
            panic!()
        };
        assert_eq!(defaults.args, json!({"limit":50}));
    }

    #[test]
    fn parses_changes_wait_with_thread_alias_and_timeout_defaults() {
        let command = parse_args(
            &args(&[
                "--data-dir",
                "d",
                "--session",
                "fixture",
                "--thread-id",
                "thread-1",
                "--timeout",
                "300",
                "changes",
                "wait",
            ]),
            &mut Cursor::new([]),
        )
        .unwrap();
        let Command::Request(spec) = command else {
            panic!()
        };
        assert_eq!(spec.operation, "changes.wait");
        assert_eq!(
            spec.args,
            json!({"thread_id":"thread-1","limit":50,"timeout":300})
        );
        let default_timeout = parse_args(
            &args(&["--data-dir", "d", "--session", "fixture", "changes", "wait"]),
            &mut Cursor::new([]),
        )
        .unwrap();
        let Command::Request(default_timeout) = default_timeout else {
            panic!()
        };
        assert_eq!(default_timeout.args, json!({"limit":50,"timeout":60}));
    }

    #[test]
    fn changes_parser_rejects_invalid_ranges_duplicate_aliases_and_unknown_options() {
        let base = ["--data-dir", "d", "--session", "fixture"];
        let invalid: &[&[&str]] = &[
            &["--timeout", "1", "changes", "get"],
            &["--timeout", "0", "changes", "wait"],
            &["--timeout", "301", "changes", "wait"],
            &["--timeout", "1.5", "changes", "wait"],
            &["--limit", "0", "changes", "get"],
            &["--limit", "201", "changes", "wait"],
            &["--after", "", "changes", "get"],
            &["--thread", "t1", "--board-id", "b1", "changes", "get"],
            &["--thread", "t1", "--thread-id", "t2", "changes", "get"],
            &["--board", "b1", "--board-id", "b2", "changes", "get"],
            &["--not-a-change-filter", "secret-value", "changes", "get"],
            &["--cursor", "search-cursor", "changes", "get"],
            &["changes", "unknown"],
        ];
        for extra in invalid {
            let mut input = base.to_vec();
            input.extend_from_slice(extra);
            let error = parse_args(&args(&input), &mut Cursor::new([])).unwrap_err();
            assert_eq!(error.code, "INVALID_INPUT", "failed for {extra:?}");
            assert!(!error.to_string().contains("secret-value"));
        }
    }

    #[test]
    fn changes_markdown_renders_metadata_and_never_event_bodies() {
        let rendered = render_markdown(
            "changes.get",
            &json!({
                "items":[{
                    "operation":"post.edit",
                    "entity_id":"entity-1",
                    "board_id":"board-1",
                    "thread_id":"thread-1",
                    "no":42,
                    "created_at":"2026-10-09T12:00:00Z",
                    "body":"private body must never appear"
                }],
                "change_cursor":"opaque-cursor",
                "has_more":true
            }),
        );
        for expected in [
            "post.edit",
            "entity-1",
            "board-1",
            "thread-1",
            "42",
            "2026-10-09T12:00:00Z",
            "opaque-cursor",
            "More available: true",
        ] {
            assert!(rendered.contains(expected), "missing {expected:?}");
        }
        assert!(!rendered.contains("private body must never appear"));
    }
}
