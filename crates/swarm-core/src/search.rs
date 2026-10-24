use crate::{
    human,
    policy::{Policy, Target},
    protocol::{Fault, Request},
};
use chrono::DateTime;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 100;
const MAX_QUERY_BYTES: usize = 4096;
const MAX_FILTER_BYTES: usize = 4096;
const MAX_CURSOR_BYTES: usize = 8192;
const EXCERPT_CHARS: usize = 240;

fn invalid() -> Fault {
    Fault::new("INVALID_INPUT", 2, "Search request is invalid")
}

fn denied() -> Fault {
    Fault::new("PERMISSION_DENIED", 4, "This operation is not permitted")
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u8,
    filter: String,
    created_at: String,
    id: String,
    kind: String,
}

#[derive(Clone, Debug, Serialize)]
struct FilterKey {
    terms: Vec<String>,
    scope: String,
    board_id: Option<String>,
    thread_id: Option<String>,
    project_id: Option<String>,
    author_id: Option<String>,
    from_ms: Option<i64>,
    to_ms: Option<i64>,
    limit: usize,
}

#[derive(Clone, Debug)]
struct SearchFilter {
    key: FilterKey,
    cursor: Option<Cursor>,
    cursor_signature: Option<String>,
}

#[derive(Clone, Debug)]
struct Candidate {
    kind: &'static str,
    id: String,
    board_id: String,
    thread_id: Option<String>,
    name: Option<String>,
    searchable: String,
    post_no: Option<i64>,
    created_at: String,
}

impl Candidate {
    fn result(&self, board_visible: bool) -> Value {
        json!({
            "kind":self.kind,
            "id":self.id,
            "board_id":if board_visible {Some(self.board_id.as_str())} else {None},
            "thread_id":self.thread_id,
            "name":self.name,
            "excerpt":excerpt(&self.searchable),
            "post_no":self.post_no,
            "created_at":self.created_at
        })
    }
}

fn optional_text(args: &Value, key: &str, max: usize) -> Result<Option<String>, Fault> {
    match args.get(key) {
        None => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() && value.len() <= max => {
            Ok(Some(value.clone()))
        }
        _ => Err(invalid()),
    }
}

fn parse_date(value: Option<String>) -> Result<Option<i64>, Fault> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(&value)
                .map(|date| date.timestamp_millis())
                .map_err(|_| invalid())
        })
        .transpose()
}

fn parse(args: &Value) -> Result<SearchFilter, Fault> {
    let object = args.as_object().ok_or_else(invalid)?;
    let allowed = [
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
    ];
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(invalid());
    }
    let raw_query = object
        .get("query")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= MAX_QUERY_BYTES)
        .ok_or_else(invalid)?;
    let mut terms = raw_query
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    terms.sort();
    terms.dedup();
    if terms.is_empty() || terms.len() > 64 {
        return Err(invalid());
    }
    let scope = match object.get("scope") {
        None => "all",
        Some(Value::String(value)) => value.as_str(),
        _ => return Err(invalid()),
    };
    if !matches!(
        scope,
        "all" | "board" | "thread" | "post" | "response" | "body" | "board_name" | "thread_name"
    ) {
        return Err(invalid());
    }
    let board_id = optional_text(args, "board_id", MAX_FILTER_BYTES)?;
    let thread_id = optional_text(args, "thread_id", MAX_FILTER_BYTES)?;
    let project_id = optional_text(args, "project_id", MAX_FILTER_BYTES)?;
    let author_id = optional_text(args, "author_id", MAX_FILTER_BYTES)?;
    let from_ms = parse_date(optional_text(args, "from", 64)?)?;
    let to_ms = parse_date(optional_text(args, "to", 64)?)?;
    if from_ms.zip(to_ms).is_some_and(|(from, to)| from > to) {
        return Err(invalid());
    }
    let limit = match object.get("limit") {
        None => DEFAULT_LIMIT,
        Some(value) => value
            .as_u64()
            .filter(|value| (1..=MAX_LIMIT as u64).contains(value))
            .map(|value| value as usize)
            .ok_or_else(invalid)?,
    };
    let filter = FilterKey {
        terms,
        scope: scope.to_owned(),
        board_id,
        thread_id,
        project_id,
        author_id,
        from_ms,
        to_ms,
        limit,
    };
    let (cursor, cursor_signature) = match object.get("cursor") {
        None | Some(Value::Null) => (None, None),
        Some(Value::String(value)) if value.len() <= MAX_CURSOR_BYTES => {
            let (cursor, signature) = decode_cursor(value)?;
            (Some(cursor), Some(signature))
        }
        _ => return Err(invalid()),
    };
    Ok(SearchFilter {
        key: filter,
        cursor,
        cursor_signature,
    })
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_hex(value: &str) -> Result<Vec<u8>, Fault> {
    if value.is_empty() || !value.len().is_multiple_of(2) {
        return Err(invalid());
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).ok_or_else(invalid)?;
            let low = (pair[1] as char).to_digit(16).ok_or_else(invalid)?;
            Ok(((high << 4) | low) as u8)
        })
        .collect()
}

fn hash(bytes: &[u8]) -> String {
    lower_hex(&Sha256::digest(bytes))
}

fn cursor_filter(actor: &str, filter: &FilterKey) -> Result<String, Fault> {
    let serialized = serde_json::to_vec(&(actor, filter)).map_err(|_| Fault::internal())?;
    Ok(hash(&serialized))
}

fn hmac(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut normalized = [0u8; 64];
    if key.len() > normalized.len() {
        normalized[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        normalized[..key.len()].copy_from_slice(key);
    }
    let mut inner_key = [0x36u8; 64];
    let mut outer_key = [0x5cu8; 64];
    for index in 0..64 {
        inner_key[index] ^= normalized[index];
        outer_key[index] ^= normalized[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_key);
    inner.update(message);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_key);
    outer.update(inner_hash);
    outer.finalize().into()
}

fn encode_cursor(cursor: &Cursor, key: &[u8]) -> Result<String, Fault> {
    let payload = serde_json::to_vec(cursor).map_err(|_| Fault::internal())?;
    let encoded = lower_hex(&payload);
    let signature = lower_hex(&hmac(key, encoded.as_bytes()));
    Ok(format!("{encoded}.{signature}"))
}

fn decode_cursor(value: &str) -> Result<(Cursor, String), Fault> {
    let (payload, signature) = value.split_once('.').ok_or_else(invalid)?;
    if signature.len() != 64 || value.matches('.').count() != 1 {
        return Err(invalid());
    }
    let payload = decode_hex(payload)?;
    let cursor: Cursor = serde_json::from_slice(&payload).map_err(|_| invalid())?;
    if cursor.version != 1
        || cursor.filter.len() != 64
        || !cursor.filter.bytes().all(|byte| byte.is_ascii_hexdigit())
        || cursor.id.trim().is_empty()
        || cursor.created_at.trim().is_empty()
        || !matches!(
            cursor.kind.as_str(),
            "board" | "thread" | "post" | "response"
        )
    {
        return Err(invalid());
    }
    Ok((cursor, signature.to_owned()))
}

fn excerpt(text: &str) -> String {
    text.chars().take(EXCERPT_CHARS).collect()
}

fn matches_terms(terms: &[String], text: &str) -> bool {
    let normalized = text.to_lowercase();
    terms.iter().all(|term| normalized.contains(term))
}

fn date_matches(created_at: &str, filter: &FilterKey) -> bool {
    let Ok(date) = DateTime::parse_from_rfc3339(created_at) else {
        return false;
    };
    let millis = date.timestamp_millis();
    filter.from_ms.is_none_or(|from| millis >= from) && filter.to_ms.is_none_or(|to| millis <= to)
}

fn before_cursor(candidate: &Candidate, cursor: &Cursor) -> bool {
    candidate.created_at < cursor.created_at
        || (candidate.created_at == cursor.created_at
            && (candidate.id < cursor.id
                || (candidate.id == cursor.id && candidate.kind > cursor.kind.as_str())))
}

fn scan_candidates(
    db: &Connection,
    filter: &FilterKey,
    mut visit: impl FnMut(Candidate) -> Result<bool, Fault>,
) -> Result<(), Fault> {
    // Stream the ordered rows so a large conversation does not require copying
    // every current body into memory for a small result page.
    let mut statement = db.prepare(concat!(
        "SELECT kind,id,board_id,thread_id,name,searchable,post_no,created_at FROM ( ",
        "SELECT 'board' AS kind,b.id AS id,b.id AS board_id,CAST(NULL AS TEXT) AS thread_id, ",
        "b.name AS name,b.name || char(10) || b.description AS searchable, ",
        "CAST(NULL AS INTEGER) AS post_no,b.created_at AS created_at FROM boards b ",
        "WHERE b.deleted_at IS NULL AND (?1 IS NULL OR b.id=?1) AND ?2 IS NULL ",
        "AND ?3 IS NULL AND ?4 IS NULL AND (?5 IN ('all','board','board_name')) UNION ALL ",
        "SELECT 'thread',t.id,t.board_id,t.id,t.name,t.name || char(10) || t.description, ",
        "CAST(NULL AS INTEGER),t.created_at FROM threads t JOIN boards b ON b.id=t.board_id ",
        "WHERE t.deleted_at IS NULL AND b.deleted_at IS NULL ",
        "AND (?1 IS NULL OR t.board_id=?1) AND (?2 IS NULL OR t.id=?2) ",
        "AND ?3 IS NULL AND ?4 IS NULL AND (?5 IN ('all','thread','thread_name')) UNION ALL ",
        "SELECT 'post',p.id,t.board_id,t.id,t.name,p.body,p.no,p.created_at ",
        "FROM posts p JOIN threads t ON t.id=p.thread_id JOIN boards b ON b.id=t.board_id ",
        "LEFT JOIN ai_identities i ON i.id=p.author_id WHERE p.deleted_at IS NULL AND p.body_purged_at IS NULL ",
        "AND p.reply_to IS NULL AND t.deleted_at IS NULL AND b.deleted_at IS NULL ",
        "AND (?1 IS NULL OR t.board_id=?1) AND (?2 IS NULL OR t.id=?2) ",
        "AND (?3 IS NULL OR p.author_id=?3) AND (?4 IS NULL OR i.project_id=?4) ",
        "AND (?5 IN ('all','post','body')) UNION ALL ",
        "SELECT 'response',p.id,t.board_id,t.id,t.name,p.body,p.no,p.created_at ",
        "FROM posts p JOIN threads t ON t.id=p.thread_id JOIN boards b ON b.id=t.board_id ",
        "LEFT JOIN ai_identities i ON i.id=p.author_id WHERE p.deleted_at IS NULL AND p.body_purged_at IS NULL ",
        "AND p.reply_to IS NOT NULL AND t.deleted_at IS NULL AND b.deleted_at IS NULL ",
        "AND (?1 IS NULL OR t.board_id=?1) AND (?2 IS NULL OR t.id=?2) ",
        "AND (?3 IS NULL OR p.author_id=?3) AND (?4 IS NULL OR i.project_id=?4) ",
        "AND (?5 IN ('all','response','body'))) ORDER BY created_at DESC,id DESC,kind ASC",
    ))?;
    let mut rows = statement.query(params![
        filter.board_id.as_deref(),
        filter.thread_id.as_deref(),
        filter.author_id.as_deref(),
        filter.project_id.as_deref(),
        filter.scope,
    ])?;
    while let Some(row) = rows.next()? {
        let kind = match row.get::<_, String>(0)?.as_str() {
            "board" => "board",
            "thread" => "thread",
            "post" => "post",
            "response" => "response",
            _ => return Err(Fault::internal()),
        };
        let candidate = Candidate {
            kind,
            id: row.get(1)?,
            board_id: row.get(2)?,
            thread_id: row.get(3)?,
            name: row.get(4)?,
            searchable: row.get(5)?,
            post_no: row.get(6)?,
            created_at: row.get(7)?,
        };
        let searchable = if matches!(filter.scope.as_str(), "board_name" | "thread_name") {
            candidate.name.as_deref().unwrap_or_default()
        } else {
            &candidate.searchable
        };
        if date_matches(&candidate.created_at, filter)
            && matches_terms(&filter.terms, searchable)
            && !visit(candidate)?
        {
            break;
        }
    }
    Ok(())
}

fn human_visible(db: &Connection, actor: &str, item: &Candidate) -> Result<(bool, bool), Fault> {
    let board = item.board_id.as_str();
    let thread = item.thread_id.as_deref();
    if !human::can(db, actor, "search", Some(board), thread)? {
        return Ok((false, false));
    }
    let requirements: &[&str] = match item.kind {
        "board" => &["board.list", "board.get"],
        "thread" => &["thread.list", "thread.get"],
        "post" => &["thread.list", "thread.get", "post.list", "post.get"],
        "response" => &[
            "thread.list",
            "thread.get",
            "post.list",
            "post.get",
            "response.list",
        ],
        _ => return Err(Fault::internal()),
    };
    for operation in requirements {
        if !human::can(db, actor, operation, Some(board), thread)? {
            return Ok((false, false));
        }
    }
    let board_visible =
        item.kind == "board" || human::can(db, actor, "board.get", Some(board), None)?;
    Ok((true, board_visible))
}

fn human_board_filter_visible(
    db: &Connection,
    actor: &str,
    filter: &SearchFilter,
) -> Result<bool, Fault> {
    let Some(board_id) = filter.key.board_id.as_deref() else {
        return Ok(true);
    };
    Ok(human::can(db, actor, "board.list", Some(board_id), None)?
        && human::can(db, actor, "board.get", Some(board_id), None)?)
}

fn ai_visible(request: &Request, policy: &Policy, item: &Candidate) -> (bool, bool) {
    ai_target_visibility(
        request,
        policy,
        item.kind,
        &item.board_id,
        item.thread_id.as_deref(),
    )
}

/// The single search visibility decision used by result filtering and the
/// read-only rules.check preflight. It never needs candidate text or metadata.
fn ai_target_visibility(
    request: &Request,
    policy: &Policy,
    kind: &str,
    board_id: &str,
    thread_id: Option<&str>,
) -> (bool, bool) {
    let target = Target {
        board: Some(board_id),
        thread: thread_id,
    };
    let mut requirement = request.clone();
    requirement.operation = "search".into();
    if !policy.can(&requirement, target) {
        return (false, false);
    }
    let requirements: &[&str] = match kind {
        "board" => &["board.list", "board.get"],
        "thread" => &["thread.list", "thread.get"],
        "post" => &["thread.list", "thread.get", "post.list", "post.get"],
        "response" => &[
            "thread.list",
            "thread.get",
            "post.list",
            "post.get",
            "response.list",
        ],
        _ => return (false, false),
    };
    let visible = requirements.iter().all(|operation| {
        requirement.operation = (*operation).to_owned();
        policy.can(&requirement, target)
    });
    let board_visible = kind == "board" || {
        requirement.operation = "board.get".into();
        policy.can(
            &requirement,
            Target {
                board: Some(board_id),
                thread: None,
            },
        )
    };
    (visible, board_visible)
}

fn ai_board_filter_visible(request: &Request, policy: &Policy, board_id: &str) -> bool {
    let target = Target {
        board: Some(board_id),
        thread: None,
    };
    let mut requirement = request.clone();
    requirement.operation = "board.list".into();
    policy.can(&requirement, target) && {
        requirement.operation = "board.get".into();
        policy.can(&requirement, target)
    }
}

/// Report whether a live, currently readable search target has any searchable
/// subset the AI can retrieve. The scan reads only IDs and locations, never
/// bodies, names, result counts, or cursor metadata.
pub(crate) fn diagnose_ai_search(
    db: &Connection,
    request: &Request,
    policy: &Policy,
) -> Result<Value, Fault> {
    let args = request.args.as_object().ok_or_else(invalid)?;
    let board_filter = args.get("board_id").and_then(Value::as_str);
    let thread_filter = args.get("thread_id").and_then(Value::as_str);

    if let Some(board_id) = board_filter {
        let live: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM boards WHERE id=?1 AND deleted_at IS NULL)",
            [board_id],
            |row| row.get(0),
        )?;
        if !live || !ai_board_filter_visible(request, policy, board_id) {
            return Ok(json!({"allowed":false,"reason":"target_unavailable"}));
        }
    }
    if let Some(thread_id) = thread_filter {
        let board_id: Option<String> = db
            .query_row(
                "SELECT t.board_id FROM threads t JOIN boards b ON b.id=t.board_id WHERE t.id=?1 AND t.deleted_at IS NULL AND b.deleted_at IS NULL",
                [thread_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(board_id) = board_id else {
            return Ok(json!({"allowed":false,"reason":"target_unavailable"}));
        };
        if board_filter.is_some_and(|filter| filter != board_id) {
            return Ok(json!({"allowed":false,"reason":"target_unavailable"}));
        }
        if !ai_target_visibility(request, policy, "thread", &board_id, Some(thread_id)).0 {
            return Ok(json!({"allowed":false,"reason":"target_unavailable"}));
        }
    }

    let mut statement = db.prepare(concat!(
        "SELECT kind,board_id,thread_id FROM ( ",
        "SELECT 'board' AS kind,b.id AS board_id,CAST(NULL AS TEXT) AS thread_id FROM boards b ",
        "WHERE b.deleted_at IS NULL AND (?1 IS NULL OR b.id=?1) AND ?2 IS NULL UNION ALL ",
        "SELECT 'thread',t.board_id,t.id FROM threads t JOIN boards b ON b.id=t.board_id ",
        "WHERE t.deleted_at IS NULL AND b.deleted_at IS NULL ",
        "AND (?1 IS NULL OR t.board_id=?1) AND (?2 IS NULL OR t.id=?2) UNION ALL ",
        "SELECT CASE WHEN p.reply_to IS NULL THEN 'post' ELSE 'response' END,t.board_id,t.id ",
        "FROM posts p JOIN threads t ON t.id=p.thread_id JOIN boards b ON b.id=t.board_id ",
        "WHERE p.deleted_at IS NULL AND p.body_purged_at IS NULL AND t.deleted_at IS NULL AND b.deleted_at IS NULL ",
        "AND (?1 IS NULL OR t.board_id=?1) AND (?2 IS NULL OR t.id=?2) )",
    ))?;
    let mut rows = statement.query(params![board_filter, thread_filter])?;
    while let Some(row) = rows.next()? {
        let kind: String = row.get(0)?;
        let board_id: String = row.get(1)?;
        let thread_id: Option<String> = row.get(2)?;
        if ai_target_visibility(request, policy, &kind, &board_id, thread_id.as_deref()).0 {
            return Ok(json!({"allowed":true,"reason":"allowed"}));
        }
    }
    Ok(json!({"allowed":false,"reason":"not_allowed"}))
}

fn build_page(
    db: &Connection,
    request: &Request,
    actor: &str,
    filter: SearchFilter,
    visible: impl Fn(&Candidate) -> Result<(bool, bool), Fault>,
) -> Result<Value, Fault> {
    let key = request
        .auth
        .as_deref()
        .filter(|key| !key.trim().is_empty() && key.len() <= 4096)
        .ok_or_else(denied)?;
    let filter_hash = cursor_filter(actor, &filter.key)?;
    if let Some(cursor) = &filter.cursor {
        if cursor.filter != filter_hash {
            return Err(invalid());
        }
        verify_signature(
            cursor,
            filter.cursor_signature.as_deref().ok_or_else(invalid)?,
            key.as_bytes(),
        )?;
    }
    let mut items = Vec::new();
    let mut has_more = false;
    let mut total_count = 0usize;
    scan_candidates(db, &filter.key, |candidate| {
        let (is_visible, board_visible) = visible(&candidate)?;
        if !is_visible {
            return Ok(true);
        }
        total_count += 1;
        if filter
            .cursor
            .as_ref()
            .is_some_and(|cursor| !before_cursor(&candidate, cursor))
        {
            return Ok(true);
        }
        if items.len() < filter.key.limit {
            items.push(candidate.result(board_visible));
        } else {
            has_more = true;
        }
        Ok(true)
    })?;
    let next_cursor = if has_more {
        let last = items.last().ok_or_else(Fault::internal)?;
        let cursor = Cursor {
            version: 1,
            filter: filter_hash,
            created_at: last["created_at"]
                .as_str()
                .ok_or_else(Fault::internal)?
                .to_owned(),
            id: last["id"].as_str().ok_or_else(Fault::internal)?.to_owned(),
            kind: last["kind"]
                .as_str()
                .ok_or_else(Fault::internal)?
                .to_owned(),
        };
        Some(encode_cursor(&cursor, key.as_bytes())?)
    } else {
        None
    };
    Ok(json!({"items":items,"next_cursor":next_cursor,"total_count":total_count}))
}

fn verify_signature(cursor: &Cursor, signature: &str, key: &[u8]) -> Result<(), Fault> {
    let payload = serde_json::to_vec(cursor).map_err(|_| Fault::internal())?;
    let expected = hmac(key, lower_hex(&payload).as_bytes());
    let actual = decode_hex(signature)?;
    if actual.len() != expected.len()
        || actual
            .iter()
            .zip(expected.iter())
            .fold(0u8, |difference, (left, right)| difference | (left ^ right))
            != 0
    {
        return Err(invalid());
    }
    Ok(())
}

pub(crate) fn human(db: &Connection, request: &Request, actor: &str) -> Result<Value, Fault> {
    let filter = parse(&request.args)?;
    if !human_board_filter_visible(db, actor, &filter)? {
        return Ok(json!({"items": [], "next_cursor": null, "total_count": 0}));
    }
    build_page(db, request, actor, filter, |item| {
        human_visible(db, actor, item)
    })
}

pub(crate) fn ai(
    db: &Connection,
    request: &Request,
    actor: &str,
    policy: &Policy,
) -> Result<Value, Fault> {
    let filter = parse(&request.args)?;
    if filter
        .key
        .board_id
        .as_deref()
        .is_some_and(|board_id| !ai_board_filter_visible(request, policy, board_id))
    {
        return Ok(json!({"items": [], "next_cursor": null, "total_count": 0}));
    }
    let own_project: String = db.query_row(
        "SELECT project_id FROM ai_identities WHERE id=?1",
        [actor],
        |row| row.get(0),
    )?;
    if filter
        .key
        .project_id
        .as_deref()
        .is_some_and(|value| value != own_project)
    {
        return build_page(db, request, actor, filter, |_| Ok((false, false)));
    }
    build_page(db, request, actor, filter, |item| {
        Ok(ai_visible(request, policy, item))
    })
}
