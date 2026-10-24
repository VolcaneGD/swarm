use crate::protocol::{Fault, Request};
use chrono::{Local, Timelike, Utc};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::Path,
};

const DEFAULT_RULES: &str = "# Swarm rules\n\nAI access starts with reading only.\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules: []\naccess:\n  boards: []\n  threads: []\n~~~\n";
fn invalid() -> Fault {
    Fault::new(
        "RULES_INVALID",
        4,
        "RULES.md is missing, unreadable, or invalid. Validate its swarm-rules YAML block before retrying",
    )
}

#[cfg(test)]
mod change_cursor_clock_tests {
    use super::*;

    #[test]
    fn clock_binding_changes_with_rule_truth_but_not_every_tick() {
        let document = parse(
            "~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: clock-read-boundary\n    effect: deny\n    subjects: {}\n    actions: [changes.get, changes.wait]\n    when:\n      all:\n        - {field: time.hour, op: gte, value: 10}\n        - {field: time.utc, op: contains, value: '2026-10-09'}\naccess:\n  boards: []\n  threads: []\n~~~\n",
            None,
        ).unwrap();
        let mut policy = Policy {
            docs: vec![document],
            actor: "fixture-actor".into(),
            project: "fixture-project".into(),
            model: "fixture-model".into(),
            access_fingerprint: "fixture-policy".into(),
            utc: "2026-10-09T00:00:00Z".into(),
            hour: 9,
        };
        let request = Request {
            operation: "changes.get".into(),
            auth: None,
            args: json!({}),
            context: json!({}),
            request_id: None,
        };
        assert!(policy.can(&request, Target::default()));
        let before = policy.time_predicate_fingerprint(&request).unwrap();
        policy.utc = "2026-10-09T00:00:01Z".into();
        assert_eq!(policy.time_predicate_fingerprint(&request).unwrap(), before);
        policy.hour = 10;
        assert!(!policy.can(&request, Target::default()));
        let after = policy.time_predicate_fingerprint(&request).unwrap();
        assert_ne!(after, before);
        policy.utc = "2026-10-09T00:00:02Z".into();
        assert_eq!(policy.time_predicate_fingerprint(&request).unwrap(), after);
        policy.utc = "2026-10-10T00:00:02Z".into();
        assert!(policy.can(&request, Target::default()));
        assert_ne!(policy.time_predicate_fingerprint(&request).unwrap(), after);
    }
}
fn denied() -> Fault {
    Fault::new(
        "PERMISSION_DENIED",
        4,
        "This operation is not permitted by the effective rules",
    )
}

pub(crate) fn initialize(db: &mut Connection, root: &Path) -> Result<(), Fault> {
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let initialized: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM app_metadata WHERE key='rules_initialized')",
        [],
        |r| r.get(0),
    )?;
    if !initialized {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("RULES.md"))
        {
            Ok(mut file) => file
                .write_all(DEFAULT_RULES.as_bytes())
                .map_err(|_| Fault::internal())?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(Fault::internal()),
        }
        tx.execute(
            "INSERT INTO app_metadata(key,value) VALUES('rules_initialized','true')",
            [],
        )?;
    }
    tx.commit()?;
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema_version: u32,
    #[serde(default)]
    defaults: Defaults,
    #[serde(default)]
    rules: Vec<Rule>,
    #[serde(default)]
    access: Access,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Defaults {
    #[serde(default = "yes")]
    ai_read: bool,
}
fn yes() -> bool {
    true
}
impl Default for Defaults {
    fn default() -> Self {
        Self { ai_read: true }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    id: String,
    effect: Effect,
    #[serde(default)]
    subjects: Subjects,
    actions: Vec<String>,
    #[serde(default)]
    scope: Scope,
    when: Option<Value>,
}
#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Effect {
    Allow,
    Deny,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Subjects {
    projects: Option<Vec<String>>,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Scope {
    boards: Option<Vec<String>>,
    threads: Option<Vec<String>>,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Access {
    #[serde(default)]
    boards: Vec<AccessEntry>,
    #[serde(default)]
    threads: Vec<AccessEntry>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccessEntry {
    id: String,
    projects: HashMap<String, ReadSetting>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadSetting {
    read: Decision,
}
#[derive(Deserialize, PartialEq, Copy, Clone)]
#[serde(rename_all = "lowercase")]
enum Decision {
    Inherit,
    Allow,
    Deny,
}

const ACTIONS: &[&str] = &[
    "board.list",
    "board.get",
    "board.create",
    "board.edit",
    "board.delete",
    "board.restore",
    "thread.list",
    "thread.get",
    "thread.create",
    "thread.edit",
    "thread.delete",
    "thread.restore",
    "post.list",
    "post.get",
    "post.create",
    "post.edit",
    "post.history",
    "post.delete",
    "post.restore",
    "response.create",
    "response.list",
    "project.list",
    "project.get",
    "project.register",
    "search",
    "changes.get",
    "changes.wait",
    "rules.check",
    "rules.validate",
];

pub(crate) fn action_catalog() -> &'static [&'static str] {
    ACTIONS
}

/// Validate the complete Markdown document while preserving its original text
/// for editing. The returned action list is an ordered, deduplicated summary.
pub(crate) fn validate_source(
    source: &str,
    local_project: Option<&str>,
) -> Result<Vec<String>, Fault> {
    if source.len() > 1024 * 1024 {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "RULES.md must not exceed 1 MiB",
        ));
    }
    let document = parse(source, local_project)?;
    let mut actions = Vec::new();
    for rule in document.rules {
        for action in rule.actions {
            if !actions.contains(&action) {
                actions.push(action);
            }
        }
    }
    Ok(actions)
}

fn field_valid(field: &str) -> bool {
    field
        .strip_prefix("context.")
        .is_some_and(|suffix| !suffix.is_empty() && suffix.split('.').all(|s| !s.is_empty()))
        || matches!(
            field,
            "actor.id"
                | "actor.model_name"
                | "project.id"
                | "target.board_id"
                | "target.thread_id"
                | "operation"
                | "time.utc"
                | "time.hour"
        )
}
fn condition_valid(condition: &Value, depth: usize) -> bool {
    if depth > 64 {
        return false;
    }
    let Some(object) = condition.as_object() else {
        return false;
    };
    for key in ["all", "any"] {
        if let Some(children) = object.get(key) {
            return object.len() == 1
                && children
                    .as_array()
                    .is_some_and(|a| a.iter().all(|c| condition_valid(c, depth + 1)));
        }
    }
    if let Some(child) = object.get("not") {
        return object.len() == 1 && condition_valid(child, depth + 1);
    }
    if object
        .keys()
        .any(|k| !matches!(k.as_str(), "field" | "op" | "value"))
    {
        return false;
    }
    let Some(field) = object.get("field").and_then(Value::as_str) else {
        return false;
    };
    let Some(op) = object.get("op").and_then(Value::as_str) else {
        return false;
    };
    if !field_valid(field) {
        return false;
    }
    if op == "exists" {
        return object.get("value").is_none_or(Value::is_boolean);
    }
    let Some(value) = object.get("value") else {
        return false;
    };
    match op {
        "eq" | "ne" | "contains" => !value.is_null(),
        "in" => value.is_array(),
        "lt" | "lte" | "gt" | "gte" => value.is_number(),
        _ => false,
    }
}
#[derive(Copy, Clone)]
enum HtmlBlock {
    Comment,
    Raw(&'static str),
    ProcessingInstruction,
    Declaration,
    Cdata,
    UntilBlankLine,
}

fn raw_html_close(line: &str, tag: &str) -> bool {
    let line = line.to_ascii_lowercase();
    let closing = format!("</{tag}");
    line.match_indices(&closing).any(|(index, _)| {
        line[index + closing.len()..]
            .chars()
            .next()
            .is_some_and(|c| c == '>' || c.is_ascii_whitespace())
    })
}

fn html_block_start(line: &str) -> Option<HtmlBlock> {
    if line.starts_with("<!--") {
        return Some(HtmlBlock::Comment);
    }
    if line.starts_with("<![CDATA[") {
        return Some(HtmlBlock::Cdata);
    }
    if line.starts_with("<?") {
        return Some(HtmlBlock::ProcessingInstruction);
    }
    if line.starts_with("<!")
        && line
            .as_bytes()
            .get(2)
            .is_some_and(|b| b.is_ascii_uppercase())
    {
        return Some(HtmlBlock::Declaration);
    }
    let lower = line.to_ascii_lowercase();
    if let Some(tag) = ["pre", "script", "style", "textarea"]
        .into_iter()
        .find(|tag| {
            let prefix = format!("<{tag}");
            lower.starts_with(&prefix)
                && lower[prefix.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c == '>' || c == '/' || c.is_ascii_whitespace())
        })
    {
        return Some(HtmlBlock::Raw(tag));
    }

    // CommonMark HTML block types 6 and 7 continue through the next blank
    // line. Recognize block-level tags (which may be partial on this line)
    // and complete standalone tags, including custom element names.
    if starts_html_block_tag(&lower) || starts_complete_html_tag(&lower) {
        return Some(HtmlBlock::UntilBlankLine);
    }
    None
}

fn starts_html_block_tag(line: &str) -> bool {
    const BLOCK_TAGS: &[&str] = &[
        "address",
        "article",
        "aside",
        "base",
        "basefont",
        "blockquote",
        "body",
        "caption",
        "center",
        "col",
        "colgroup",
        "dd",
        "details",
        "dialog",
        "dir",
        "div",
        "dl",
        "dt",
        "fieldset",
        "figcaption",
        "figure",
        "footer",
        "form",
        "frame",
        "frameset",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "head",
        "header",
        "hr",
        "html",
        "iframe",
        "legend",
        "li",
        "link",
        "main",
        "menu",
        "menuitem",
        "meta",
        "nav",
        "noframes",
        "ol",
        "optgroup",
        "option",
        "p",
        "param",
        "search",
        "section",
        "summary",
        "table",
        "tbody",
        "td",
        "tfoot",
        "th",
        "thead",
        "title",
        "tr",
        "track",
        "ul",
    ];
    BLOCK_TAGS.iter().any(|tag| {
        let opening = format!("<{tag}");
        let closing = format!("</{tag}");
        [opening, closing].iter().any(|prefix| {
            line.starts_with(prefix)
                && line[prefix.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| c == '>' || c == '/' || c.is_ascii_whitespace())
        })
    })
}

fn starts_complete_html_tag(line: &str) -> bool {
    let Some(rest) = line.strip_prefix('<') else {
        return false;
    };
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    let name_len = rest
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'-')
        .count();
    if name_len == 0 || !rest.as_bytes()[0].is_ascii_alphabetic() {
        return false;
    }
    let tail = &rest[name_len..];
    if !tail
        .chars()
        .next()
        .is_some_and(|c| c == '>' || c == '/' || c.is_ascii_whitespace())
    {
        return false;
    }
    tail.contains('>')
}

fn html_block_closed(block: HtmlBlock, line: &str) -> bool {
    match block {
        HtmlBlock::Comment => line.contains("-->"),
        HtmlBlock::Raw(tag) => raw_html_close(line, tag),
        HtmlBlock::ProcessingInstruction => line.contains("?>"),
        HtmlBlock::Declaration => line.contains('>'),
        HtmlBlock::Cdata => line.contains("]]>"),
        HtmlBlock::UntilBlankLine => line.trim().is_empty(),
    }
}

fn parse(text: &str, local: Option<&str>) -> Result<Document, Fault> {
    let mut block = None;
    let mut active: Option<(char, usize, bool)> = None;
    let mut html: Option<HtmlBlock> = None;
    let mut content = String::new();
    for line in text.lines() {
        let spaces = line.bytes().take_while(|b| *b == b' ').count();
        let eligible_indent =
            spaces <= 3 && line.as_bytes().get(spaces).is_none_or(|b| *b != b'\t');
        let rest = if eligible_indent { &line[spaces..] } else { "" };
        if let Some((marker, length, is_rules)) = active {
            let run = rest.chars().take_while(|c| *c == marker).count();
            let closing = eligible_indent && run >= length && rest[run..].trim().is_empty();
            if closing {
                if is_rules {
                    block = Some(std::mem::take(&mut content));
                }
                active = None;
            } else if is_rules {
                content.push_str(line);
                content.push('\n');
            }
            continue;
        }
        if let Some(active_html) = html {
            if html_block_closed(active_html, line) {
                html = None;
            }
            continue;
        }
        if !eligible_indent {
            continue;
        }
        if let Some(opening) = html_block_start(rest) {
            if !html_block_closed(opening, rest) {
                html = Some(opening);
            }
            continue;
        }
        for marker in ['~', '`'] {
            let length = rest.chars().take_while(|c| *c == marker).count();
            if length < 3 {
                continue;
            }
            let info = rest[length..].trim();
            // CommonMark forbids backticks in a backtick fence's info string.
            if marker == '`' && info.contains('`') {
                continue;
            }
            let is_rules = info == "swarm-rules";
            if is_rules && block.is_some() {
                return Err(invalid());
            }
            active = Some((marker, length, is_rules));
            break;
        }
    }
    if active.is_some_and(|(_, _, is_rules)| is_rules) || block.is_none() {
        return Err(invalid());
    }
    let doc: Document =
        serde_saphyr::from_str(&block.ok_or_else(invalid)?).map_err(|_| invalid())?;
    if doc.schema_version != 1 {
        return Err(invalid());
    }
    let mut ids = HashSet::new();
    for rule in &doc.rules {
        if rule.id.trim().is_empty()
            || !ids.insert(rule.id.clone())
            || rule.actions.is_empty()
            || rule.actions.iter().any(|a| !ACTIONS.contains(&a.as_str()))
            || rule.when.as_ref().is_some_and(|w| !condition_valid(w, 0))
            || local.is_some_and(|id| {
                rule.subjects
                    .projects
                    .as_ref()
                    .is_some_and(|p| p.iter().any(|p| p != id))
            })
        {
            return Err(invalid());
        }
    }
    for entry in doc.access.boards.iter().chain(&doc.access.threads) {
        if entry.id.trim().is_empty()
            || local.is_some_and(|id| entry.projects.keys().any(|p| p != id))
        {
            return Err(invalid());
        }
    }
    Ok(doc)
}

#[cfg(test)]
mod parser_tests {
    use super::parse;

    const YAML: &str = "schema_version: 1\ndefaults:\n  ai_read: true\nrules: []\naccess:\n  boards: []\n  threads: []\n";

    #[test]
    fn accepts_top_level_tilde_and_backtick_rules_fences() {
        for marker in ['~', '`'] {
            let fence = marker.to_string().repeat(3);
            for indent in ["", "   "] {
                let text = format!("notes\n{indent}{fence}swarm-rules\n{YAML}{indent}{fence}\n");
                assert!(
                    parse(&text, None).is_ok(),
                    "marker {marker:?}, indent {indent:?}"
                );
            }
        }
    }

    #[test]
    fn ignores_rules_example_inside_generic_fence() {
        let text = format!("~~~text\n```swarm-rules\n{YAML}```\n~~~\n");
        assert!(parse(&text, None).is_err());
    }

    #[test]
    fn ignores_rules_example_inside_html_comment() {
        let text = format!("<!--\n~~~swarm-rules\n{YAML}~~~\n-->\n");
        assert!(parse(&text, None).is_err());
    }

    #[test]
    fn ignores_rules_examples_inside_raw_html_blocks() {
        for tag in ["pre", "script", "style", "textarea"] {
            let text = format!("<{tag}>\n~~~swarm-rules\n{YAML}~~~\n</{tag}>\n");
            assert!(parse(&text, None).is_err(), "tag {tag}");
        }
    }

    #[test]
    fn ignores_rules_examples_inside_generic_html_blocks() {
        for tag in ["div", "custom-widget"] {
            let text = format!("<{tag}>\n~~~swarm-rules\n{YAML}~~~\n</{tag}>\n");
            assert!(parse(&text, None).is_err(), "tag {tag}");
        }
    }

    #[test]
    fn ignores_rules_examples_inside_declarations_cdata_and_processing_instructions() {
        for (opening, closing) in [("<!DOCTYPE", ">"), ("<![CDATA[", "]]>"), ("<?xml", "?>")] {
            let text = format!("{opening}\n~~~swarm-rules\n{YAML}~~~\n{closing}\n");
            assert!(parse(&text, None).is_err(), "opening {opening}");
        }
    }

    #[test]
    fn accepts_real_rules_fence_after_closed_html_comment() {
        let text = format!("<!--\n~~~swarm-rules\n{YAML}~~~\n-->\n~~~swarm-rules\n{YAML}~~~\n");
        assert!(parse(&text, None).is_ok());
    }

    #[test]
    fn accepts_real_rules_fence_after_generic_html_block_ends() {
        let text = format!("<div>example</div>\n\n~~~swarm-rules\n{YAML}~~~\n");
        assert!(parse(&text, None).is_ok());
    }

    #[test]
    fn rejects_duplicate_top_level_rules_fences() {
        let text = format!("~~~swarm-rules\n{YAML}~~~\n```swarm-rules\n{YAML}```\n");
        assert!(parse(&text, None).is_err());
    }

    #[test]
    fn rejects_unterminated_rules_fence() {
        let text = format!("~~~swarm-rules\n{YAML}");
        assert!(parse(&text, None).is_err());
    }

    #[test]
    fn shorter_fence_does_not_close_rules_block() {
        let text = format!("~~~~swarm-rules\n{YAML}~~~\n~~~~\n");
        assert!(parse(&text, None).is_err());
    }

    #[test]
    fn indented_rules_examples_do_not_count_as_top_level() {
        let text = format!("    ~~~swarm-rules\n{YAML}    ~~~\n");
        assert!(parse(&text, None).is_err());
        let text = format!("\t~~~swarm-rules\n{YAML}\t~~~\n");
        assert!(parse(&text, None).is_err());
    }
}
#[derive(Copy, Clone, PartialEq)]
enum Truth {
    True,
    False,
    Unknown,
}
fn truth(v: bool) -> Truth {
    if v { Truth::True } else { Truth::False }
}
fn negate(t: Truth) -> Truth {
    match t {
        Truth::True => Truth::False,
        Truth::False => Truth::True,
        Truth::Unknown => Truth::Unknown,
    }
}
fn comparable_eq(a: &Value, b: &Value) -> Truth {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => numeric_order(a, b)
            .map(|o| truth(o == std::cmp::Ordering::Equal))
            .unwrap_or(Truth::Unknown),
        (Value::String(_), Value::String(_))
        | (Value::Bool(_), Value::Bool(_))
        | (Value::Array(_), Value::Array(_))
        | (Value::Object(_), Value::Object(_)) => truth(a == b),
        _ => Truth::Unknown,
    }
}
fn numeric_order(a: &serde_json::Number, b: &serde_json::Number) -> Option<std::cmp::Ordering> {
    if let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) {
        return Some(a.cmp(&b));
    }
    if let (Some(a), Some(b)) = (a.as_u64(), b.as_u64()) {
        return Some(a.cmp(&b));
    }
    if a.is_i64() && b.is_u64() && a.as_i64()? < 0 {
        return Some(std::cmp::Ordering::Less);
    }
    if a.is_u64() && b.is_i64() && b.as_i64()? < 0 {
        return Some(std::cmp::Ordering::Greater);
    }
    let (a, b) = (a.as_f64()?, b.as_f64()?);
    // Fail closed rather than round large integers into an equality with a float.
    if a.abs() > 9007199254740991.0 || b.abs() > 9007199254740991.0 {
        return None;
    }
    a.partial_cmp(&b)
}
#[derive(Copy, Clone, Default)]
pub(crate) struct Target<'a> {
    pub board: Option<&'a str>,
    pub thread: Option<&'a str>,
}
pub(crate) struct Policy {
    docs: Vec<Document>,
    actor: String,
    project: String,
    model: String,
    access_fingerprint: String,
    utc: String,
    hour: u32,
}
impl Policy {
    pub(crate) fn load(db: &Connection, root: &Path, actor: &str) -> Result<Self, Fault> {
        let (project,model,canonical):(String,String,String)=db.query_row(
            "SELECT i.project_id,i.model_name,p.canonical_root FROM ai_identities i JOIN projects p ON p.id=i.project_id WHERE i.id=?1",
            [actor],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let global_path = root.join("RULES.md");
        if !Path::new(&canonical).is_dir() {
            return Err(Fault::new(
                "PROJECT_ROOT_UNAVAILABLE",
                4,
                "Project root is unavailable; ask Admin to update its registration",
            ));
        }
        let local_path = Path::new(&canonical).join("RULES.md");
        let global = std::fs::read_to_string(&global_path).map_err(|_| invalid())?;
        let read_local = || match std::fs::read_to_string(&local_path) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(invalid()),
        };
        let local = read_local()?;
        let mut docs = vec![parse(&global, None)?];
        if let Some(text) = &local {
            docs.push(parse(text, Some(&project))?);
        }
        if global != std::fs::read_to_string(&global_path).map_err(|_| invalid())?
            || local != read_local()?
        {
            return Err(Fault::new(
                "RULES_CHANGED",
                4,
                "Rules changed during this request; retry with the current configuration",
            ));
        }
        let mut digest = Sha256::new();
        for part in [
            actor.as_bytes(),
            project.as_bytes(),
            model.as_bytes(),
            canonical.as_bytes(),
            global.as_bytes(),
            local.as_deref().unwrap_or("<no-local-rules>").as_bytes(),
        ] {
            digest.update((part.len() as u64).to_be_bytes());
            digest.update(part);
        }
        let access_fingerprint = format!("{:x}", digest.finalize());
        let now = Utc::now();
        Ok(Self {
            docs,
            actor: actor.to_owned(),
            project,
            model,
            access_fingerprint,
            utc: now.to_rfc3339(),
            hour: now.with_timezone(&Local).hour(),
        })
    }
    fn field(&self, key: &str, request: &Request, target: Target<'_>) -> Option<Value> {
        if let Some(path) = key.strip_prefix("context.") {
            let mut value = &request.context;
            for segment in path.split('.') {
                value = value.get(segment)?;
            }
            return (!value.is_null()).then(|| value.clone());
        }
        match key {
            "actor.id" => Some(json!(self.actor)),
            "actor.model_name" => Some(json!(self.model)),
            "project.id" => Some(json!(self.project)),
            "target.board_id" => target.board.map(|s| json!(s)),
            "target.thread_id" => target.thread.map(|s| json!(s)),
            "operation" => Some(json!(request.operation)),
            "time.utc" => Some(json!(self.utc)),
            "time.hour" => Some(json!(self.hour)),
            _ => None,
        }
    }
    fn evaluate(&self, c: &Value, request: &Request, target: Target<'_>) -> Truth {
        for (key, identity, decisive) in [
            ("all", Truth::True, Truth::False),
            ("any", Truth::False, Truth::True),
        ] {
            if let Some(children) = c.get(key).and_then(Value::as_array) {
                let mut result = identity;
                for child in children {
                    let value = self.evaluate(child, request, target);
                    if value == decisive {
                        return decisive;
                    }
                    if value == Truth::Unknown {
                        result = Truth::Unknown;
                    }
                }
                return result;
            }
        }
        if let Some(child) = c.get("not") {
            return negate(self.evaluate(child, request, target));
        }
        let Some(key) = c["field"].as_str() else {
            return Truth::Unknown;
        };
        let Some(actual) = self.field(key, request, target) else {
            return Truth::Unknown;
        };
        let expected = &c["value"];
        match c["op"].as_str() {
            Some("exists") => truth(expected.as_bool().unwrap_or(true)),
            Some("eq") => comparable_eq(&actual, expected),
            Some("ne") => negate(comparable_eq(&actual, expected)),
            Some("in") => expected
                .as_array()
                .map(|items| {
                    if items
                        .iter()
                        .any(|item| comparable_eq(&actual, item) == Truth::True)
                    {
                        Truth::True
                    } else if items
                        .iter()
                        .any(|item| comparable_eq(&actual, item) == Truth::Unknown)
                    {
                        Truth::Unknown
                    } else {
                        Truth::False
                    }
                })
                .unwrap_or(Truth::Unknown),
            Some("contains") => match (&actual, expected) {
                (Value::String(a), Value::String(b)) => truth(a.contains(b)),
                (Value::Array(a), b) => truth(a.contains(b)),
                _ => Truth::Unknown,
            },
            Some(op @ ("lt" | "lte" | "gt" | "gte")) => {
                let (Some(a), Some(b)) = (actual.as_number(), expected.as_number()) else {
                    return Truth::Unknown;
                };
                numeric_order(a, b)
                    .map(|o| {
                        truth(match op {
                            "lt" => o.is_lt(),
                            "lte" => !o.is_gt(),
                            "gt" => o.is_gt(),
                            _ => !o.is_lt(),
                        })
                    })
                    .unwrap_or(Truth::Unknown)
            }
            _ => Truth::Unknown,
        }
    }
    fn access_decision(&self, board: bool, id: &str) -> Decision {
        let mut selected = Decision::Inherit;
        for doc in &self.docs {
            let entries = if board {
                &doc.access.boards
            } else {
                &doc.access.threads
            };
            for entry in entries.iter().filter(|entry| entry.id == id) {
                if let Some(setting) = entry.projects.get(&self.project) {
                    if setting.read == Decision::Deny {
                        return Decision::Deny;
                    }
                    if setting.read == Decision::Allow {
                        selected = Decision::Allow;
                    }
                }
            }
        }
        selected
    }
    /// Whether the target's explicit location-access layer permits access.
    /// Operation rules are evaluated separately by `can`.
    pub(crate) fn location_accessible(&self, target: Target<'_>) -> bool {
        let board = target
            .board
            .map(|id| self.access_decision(true, id))
            .unwrap_or(Decision::Inherit);
        let thread = target
            .thread
            .map(|id| self.access_decision(false, id))
            .unwrap_or(Decision::Inherit);
        let access = if thread == Decision::Inherit {
            board
        } else {
            thread
        };
        access != Decision::Deny
    }
    pub(crate) fn can(&self, request: &Request, target: Target<'_>) -> bool {
        self.can_documents(request, target, &self.docs)
    }
    pub(crate) fn access_fingerprint(&self) -> &str {
        &self.access_fingerprint
    }
    /// Fingerprint only the truth values of time-dependent conditions. The
    /// raw rules text is already covered by access_fingerprint; excluding the
    /// changing clock value avoids expiring cursors on every poll.
    pub(crate) fn time_predicate_fingerprint(&self, request: &Request) -> Result<String, Fault> {
        fn collect(
            policy: &Policy,
            request: &Request,
            value: &Value,
            path: &str,
            output: &mut Vec<(String, &'static str)>,
        ) {
            if value
                .get("field")
                .and_then(Value::as_str)
                .is_some_and(|field| field.starts_with("time."))
            {
                let truth = match policy.evaluate(value, request, Target::default()) {
                    Truth::True => "true",
                    Truth::False => "false",
                    Truth::Unknown => "unknown",
                };
                output.push((path.to_owned(), truth));
                return;
            }
            match value {
                Value::Object(object) => {
                    for (key, child) in object {
                        collect(policy, request, child, &format!("{path}.{key}"), output);
                    }
                }
                Value::Array(items) => {
                    for (index, child) in items.iter().enumerate() {
                        collect(policy, request, child, &format!("{path}[{index}]"), output);
                    }
                }
                _ => {}
            }
        }

        let mut values = Vec::new();
        for (document_index, document) in self.docs.iter().enumerate() {
            for (rule_index, rule) in document.rules.iter().enumerate() {
                if let Some(condition) = &rule.when {
                    collect(
                        self,
                        request,
                        condition,
                        &format!("{document_index}:{rule_index}:{}", rule.id),
                        &mut values,
                    );
                }
            }
        }
        let bytes = serde_json::to_vec(&values).map_err(|_| Fault::internal())?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
    /// Registration requires a grant in the global document; a local grant
    /// alone cannot acquire authority over another project.
    pub(crate) fn can_register_project(&self, request: &Request) -> bool {
        request.operation == "project.register"
            && self.can_documents(request, Target::default(), &self.docs[..1])
            && self.can(request, Target::default())
    }
    fn can_documents(&self, request: &Request, target: Target<'_>, docs: &[Document]) -> bool {
        let board = target
            .board
            .map(|id| self.access_decision(true, id))
            .unwrap_or(Decision::Inherit);
        let thread = target
            .thread
            .map(|id| self.access_decision(false, id))
            .unwrap_or(Decision::Inherit);
        let access = if thread == Decision::Inherit {
            board
        } else {
            thread
        };
        if access == Decision::Deny {
            return false;
        }
        let reading = matches!(
            request.operation.as_str(),
            "board.list"
                | "board.get"
                | "thread.list"
                | "thread.get"
                | "post.list"
                | "post.get"
                | "response.list"
                | "project.list"
                | "project.get"
                | "search"
                | "changes.get"
                | "changes.wait"
                | "rules.check"
                | "rules.validate"
        );
        let mut allow = reading && docs.iter().all(|doc| doc.defaults.ai_read);
        for doc in docs {
            for rule in &doc.rules {
                let project = rule
                    .subjects
                    .projects
                    .as_ref()
                    .is_none_or(|ids| ids.contains(&self.project));
                let board =
                    rule.scope.boards.as_ref().is_none_or(|ids| {
                        target.board.is_some_and(|id| ids.iter().any(|s| s == id))
                    });
                let thread =
                    rule.scope.threads.as_ref().is_none_or(|ids| {
                        target.thread.is_some_and(|id| ids.iter().any(|s| s == id))
                    });
                if !project
                    || !board
                    || !thread
                    || !rule.actions.iter().any(|op| op == &request.operation)
                {
                    continue;
                }
                let condition = rule
                    .when
                    .as_ref()
                    .map(|c| self.evaluate(c, request, target))
                    .unwrap_or(Truth::True);
                if rule.effect == Effect::Deny && condition != Truth::False {
                    return false;
                }
                if rule.effect == Effect::Allow && condition == Truth::True {
                    allow = true;
                }
            }
        }
        allow
    }
    pub(crate) fn require(&self, request: &Request, target: Target<'_>) -> Result<(), Fault> {
        if self.can(request, target) {
            Ok(())
        } else {
            Err(denied())
        }
    }
}
