//! Permission-filtered, cursor-based Board and Thread listing.
use crate::{
    human,
    policy::{Policy, Target},
    protocol::{Fault, Request},
    service,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const EPOCH: &str = "changes.cursor_epoch.v1";
const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;
const MAX_CURSOR_BYTES: usize = 8192;

fn invalid() -> Fault {
    Fault::new("INVALID_INPUT", 2, "List request or cursor is invalid")
}
fn resync() -> Fault {
    Fault::new(
        "RESYNC_REQUIRED",
        6,
        "The list cursor is no longer valid; request a fresh page",
    )
}
fn digest(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn fingerprint<T: Serialize>(value: &T) -> Result<String, Fault> {
    Ok(digest(
        &serde_json::to_vec(value).map_err(|_| Fault::internal())?,
    ))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CursorState {
    version: u8,
    issued_at: i64,
    operation: String,
    last_created_at: String,
    last_id: String,
    epoch: String,
    principal: String,
    filter: String,
    context: String,
    access: String,
}

struct ListCursorContext<'a> {
    request: &'a Request,
    actor: &'a str,
    policy: Option<&'a Policy>,
    operation: &'a str,
    filter: &'a str,
    limit: i64,
    now: i64,
}

fn cursor_epoch(db: &Connection) -> Result<String, Fault> {
    let epoch: Option<String> = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [EPOCH],
            |row| row.get(0),
        )
        .optional()?;
    match epoch {
        Some(value) if value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit()) => {
            Ok(value)
        }
        _ => Err(Fault::internal()),
    }
}

fn access(
    db: &Connection,
    actor: &str,
    request: &Request,
    policy: Option<&Policy>,
) -> Result<String, Fault> {
    match policy {
        Some(policy) => fingerprint(&(
            policy.access_fingerprint(),
            policy.time_predicate_fingerprint(request)?,
        )),
        None => human::access_fingerprint(db, actor),
    }
}

fn principal(request: &Request, actor: &str) -> Result<String, Fault> {
    let auth = request.auth.as_deref().ok_or_else(Fault::authentication)?;
    fingerprint(&(actor, digest(auth.as_bytes())))
}

fn parse_limit(request: &Request) -> Result<i64, Fault> {
    let limit = request
        .args
        .get("limit")
        .map_or(Ok(DEFAULT_LIMIT), |value| {
            value.as_i64().ok_or_else(invalid)
        })?;
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(invalid());
    }
    Ok(limit)
}

fn permission(
    db: &Connection,
    request: &Request,
    actor: &str,
    policy: Option<&Policy>,
    op: &str,
    board: &str,
    thread: Option<&str>,
) -> Result<bool, Fault> {
    let target = Target {
        board: Some(board),
        thread,
    };
    if let Some(policy) = policy {
        Ok(policy.location_accessible(target) && policy.can(request, target))
    } else {
        human::can(db, actor, op, Some(board), thread)
    }
}

fn redact_board_parent(
    db: &Connection,
    actor: &str,
    policy: Option<&Policy>,
    request: &Request,
    board: &str,
    mut item: Value,
) -> Result<Value, Fault> {
    let visible = if let Some(policy) = policy {
        let mut read = request.clone();
        read.operation = "board.get".into();
        policy.can(
            &read,
            Target {
                board: Some(board),
                thread: None,
            },
        )
    } else {
        human::can(db, actor, "board.get", Some(board), None)?
    };
    if !visible {
        item["board_id"] = Value::Null;
    }
    Ok(item)
}

fn load_cursor(
    db: &Connection,
    context: &ListCursorContext<'_>,
) -> Result<Option<(String, String)>, Fault> {
    let Some(raw) = context.request.args.get("cursor") else {
        return Ok(None);
    };
    let token = raw
        .as_str()
        .filter(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(invalid)?;
    if token.len() > MAX_CURSOR_BYTES {
        return Err(invalid());
    }
    let key = format!("changes.cursor.token.listing.{}", digest(token.as_bytes()));
    let encoded: Option<String> = db
        .query_row("SELECT value FROM app_metadata WHERE key=?1", [&key], |r| {
            r.get(0)
        })
        .optional()?;
    let encoded = encoded.ok_or_else(resync)?;
    let value: Value = serde_json::from_str(&encoded).map_err(|_| Fault::internal())?;
    if value.get("version").and_then(Value::as_u64) != Some(crate::cursor_metadata::VERSION as u64)
        || value.get("issued_at").and_then(Value::as_i64).is_none()
    {
        return Err(resync());
    }
    let state: CursorState = serde_json::from_value(value).map_err(|_| Fault::internal())?;
    crate::cursor_metadata::validate(state.version, state.issued_at, context.now)?;
    let request_context = fingerprint(&context.request.context)?;
    let filter = fingerprint(&(context.operation, context.filter, context.limit))?;
    if state.version != crate::cursor_metadata::VERSION
        || state.operation != context.operation
        || state.epoch != cursor_epoch(db)?
        || state.principal != principal(context.request, context.actor)?
        || state.filter != filter
        || state.context != request_context
        || state.access != access(db, context.actor, context.request, context.policy)?
    {
        return Err(resync());
    }
    Ok(Some((state.last_created_at, state.last_id)))
}

fn issue_cursor(
    db: &Connection,
    context: &ListCursorContext<'_>,
    marker: &(String, String),
) -> Result<String, Fault> {
    let state = CursorState {
        version: crate::cursor_metadata::VERSION,
        issued_at: context.now,
        operation: context.operation.into(),
        last_created_at: marker.0.clone(),
        last_id: marker.1.clone(),
        epoch: cursor_epoch(db)?,
        principal: principal(context.request, context.actor)?,
        filter: fingerprint(&(context.operation, context.filter, context.limit))?,
        context: fingerprint(&context.request.context)?,
        access: access(db, context.actor, context.request, context.policy)?,
    };
    let encoded = serde_json::to_string(&state).map_err(|_| Fault::internal())?;
    let state_key = format!(
        "changes.cursor.state.listing.{}",
        digest(encoded.as_bytes())
    );
    let existing: Option<String> = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [&state_key],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(token) = existing {
        if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Fault::internal());
        }
        let token_key = format!("changes.cursor.token.listing.{}", digest(token.as_bytes()));
        let stored: Option<String> = db
            .query_row(
                "SELECT value FROM app_metadata WHERE key=?1",
                [token_key],
                |r| r.get(0),
            )
            .optional()?;
        if stored.as_deref() != Some(encoded.as_str()) {
            return Err(Fault::internal());
        }
        return Ok(token);
    }
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let token_key = format!("changes.cursor.token.listing.{}", digest(token.as_bytes()));
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
        params![state_key, token],
    )?;
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
        params![token_key, encoded],
    )?;
    Ok(token)
}

pub(crate) fn dispatch(
    db: &Connection,
    request: &Request,
    actor: &str,
    policy: Option<&Policy>,
) -> Result<Value, Fault> {
    let op = request.operation.as_str();
    let now = crate::cursor_metadata::now_epoch_seconds();
    let limit = parse_limit(request)?;
    let board_filter = if op == "thread.list" {
        request
            .args
            .get("board_id")
            .map(|_| service::required_text(&request.args, "board_id"))
            .transpose()?
    } else {
        None
    };
    let filter = board_filter.unwrap_or("");
    let cursor_context = ListCursorContext {
        request,
        actor,
        policy,
        operation: op,
        filter,
        limit,
        now,
    };
    let marker = load_cursor(db, &cursor_context)?;
    let mut visible: Vec<(String, String, String)> = Vec::new();
    if op == "board.list" {
        let mut stmt = db.prepare("SELECT id,created_at FROM boards WHERE deleted_at IS NULL AND (?1 IS NULL OR created_at>?1 OR (created_at=?1 AND id>?2)) ORDER BY created_at,id")?;
        let rows = stmt.query_map(
            params![marker.as_ref().map(|m| &m.0), marker.as_ref().map(|m| &m.1)],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?;
        for row in rows {
            let (id, created) = row?;
            if permission(db, request, actor, policy, "board.list", &id, None)? {
                visible.push((id, String::new(), created));
                if visible.len() as i64 > limit {
                    break;
                }
            }
        }
    } else {
        let mut stmt = db.prepare("SELECT t.id,t.board_id,t.created_at FROM threads t JOIN boards b ON b.id=t.board_id WHERE t.deleted_at IS NULL AND b.deleted_at IS NULL AND (?1 IS NULL OR t.board_id=?1) AND (?2 IS NULL OR t.created_at>?2 OR (t.created_at=?2 AND t.id>?3)) ORDER BY t.created_at,t.id")?;
        let rows = stmt.query_map(
            params![
                board_filter,
                marker.as_ref().map(|m| &m.0),
                marker.as_ref().map(|m| &m.1)
            ],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )?;
        for row in rows {
            let (id, board, created) = row?;
            if permission(db, request, actor, policy, "thread.list", &board, Some(&id))? {
                visible.push((id, board, created));
                if visible.len() as i64 > limit {
                    break;
                }
            }
        }
    }
    let more = visible.len() as i64 > limit;
    visible.truncate(limit as usize);
    let mut items = Vec::with_capacity(visible.len());
    for (id, board, _) in &visible {
        let item = if op == "board.list" {
            service::board_get(db, id, false)?
        } else {
            redact_board_parent(
                db,
                actor,
                policy,
                request,
                board,
                service::thread_get(db, id, false)?,
            )?
        };
        items.push(item);
    }
    let next = if more {
        let (id, _, created) = visible.last().ok_or_else(Fault::internal)?;
        Some(issue_cursor(
            db,
            &cursor_context,
            &(created.clone(), id.clone()),
        )?)
    } else {
        None
    };
    Ok(json!({"items":items,"next_cursor":next}))
}
