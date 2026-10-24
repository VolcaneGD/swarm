//! Body-free, permission-filtered change polling with database-bound cursors.
use crate::{
    human,
    policy::{Policy, Target},
    protocol::{Fault, Request},
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;
use uuid::Uuid;

const CURSOR_EPOCH: &str = "changes.cursor_epoch.v1";
const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;
const MAX_CURSOR_BYTES: usize = 8192;
const SCAN_BATCH_ROWS: i64 = 1000;

fn invalid() -> Fault {
    Fault::new("INVALID_INPUT", 2, "Change request or cursor is invalid")
}

fn denied() -> Fault {
    Fault::new("PERMISSION_DENIED", 4, "This operation is not permitted")
}

fn missing() -> Fault {
    Fault::new("NOT_FOUND", 5, "The requested item is not available")
}

fn resync() -> Fault {
    Fault::new(
        "RESYNC_REQUIRED",
        6,
        "The change cursor is no longer valid; request a fresh cursor",
    )
}

fn valid_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Create the database epoch exactly once. A malformed value indicates
/// damaged metadata and must never be silently reset.
pub(crate) fn initialize(db: &mut Connection) -> Result<(), Fault> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let epoch: Option<String> = tx
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [CURSOR_EPOCH],
            |row| row.get(0),
        )
        .optional()?;
    match epoch {
        None => {
            tx.execute(
                "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
                params![CURSOR_EPOCH, Uuid::new_v4().simple().to_string()],
            )?;
        }
        Some(epoch) if valid_hex(&epoch, 32) => {}
        Some(_) => return Err(Fault::internal()),
    }
    tx.commit()?;
    Ok(())
}

fn cursor_epoch(db: &Connection) -> Result<String, Fault> {
    let epoch: Option<String> = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [CURSOR_EPOCH],
            |row| row.get(0),
        )
        .optional()?;
    match epoch {
        Some(epoch) if valid_hex(&epoch, 32) => Ok(epoch),
        _ => Err(Fault::internal()),
    }
}

/// Invalidate all issued cursors after a database restore or replacement.
pub(crate) fn invalidate(db: &mut Connection) -> Result<(), Fault> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    invalidate_in_transaction(&tx)?;
    tx.commit()?;
    Ok(())
}

/// Invalidate all change/listing cursors while the caller owns the write
/// transaction that changed the visible dataset.
pub(crate) fn invalidate_in_transaction(db: &Connection) -> Result<(), Fault> {
    let epoch: Option<String> = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [CURSOR_EPOCH],
            |row| row.get(0),
        )
        .optional()?;
    if epoch.as_deref().is_some_and(|value| !valid_hex(value, 32)) {
        return Err(Fault::internal());
    }
    let next_epoch = Uuid::new_v4().simple().to_string();
    if epoch.is_some() {
        db.execute(
            "UPDATE app_metadata SET value=?1 WHERE key=?2",
            params![&next_epoch, CURSOR_EPOCH],
        )?;
    } else {
        db.execute(
            "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
            params![CURSOR_EPOCH, &next_epoch],
        )?;
    }
    db.execute(
        "DELETE FROM app_metadata WHERE key LIKE 'changes.cursor.token.%' \
         OR key LIKE 'changes.cursor.state.%' \
         OR key LIKE 'listing.cursor.token.%' OR key LIKE 'listing.cursor.state.%'",
        [],
    )?;
    if epoch.as_deref() == Some(next_epoch.as_str()) {
        return Err(Fault::internal());
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u8,
    issued_at: i64,
    sequence: i64,
    epoch: String,
    principal: String,
    scope: String,
    context: String,
    access: String,
}

#[derive(Clone)]
struct Filter {
    board_id: Option<String>,
    thread_id: Option<String>,
    after: Option<String>,
    limit: i64,
}

fn parse(request: &Request) -> Result<Filter, Fault> {
    if request.operation == "changes.wait" {
        changes_wait_timeout(request)?;
    }
    let args = request.args.as_object().ok_or_else(invalid)?;
    let board_id = optional_id(args.get("board_id"))?;
    let thread_id = optional_id(args.get("thread_id"))?;
    if board_id.is_some() && thread_id.is_some() {
        return Err(invalid());
    }
    let after = match args.get("after") {
        None => None,
        Some(Value::String(value)) if !value.is_empty() && value.len() <= MAX_CURSOR_BYTES => {
            Some(value.clone())
        }
        _ => return Err(invalid()),
    };
    let limit = match args.get("limit") {
        None => DEFAULT_LIMIT,
        Some(value) => value
            .as_i64()
            .filter(|limit| (1..=MAX_LIMIT).contains(limit))
            .ok_or_else(invalid)?,
    };
    Ok(Filter {
        board_id,
        thread_id,
        after,
        limit,
    })
}

fn optional_id(value: Option<&Value>) -> Result<Option<String>, Fault> {
    match value {
        None => Ok(None),
        Some(Value::String(id)) if !id.trim().is_empty() && id.len() <= 4096 => {
            Ok(Some(id.clone()))
        }
        _ => Err(invalid()),
    }
}

pub fn changes_wait_timeout(request: &Request) -> Result<Duration, Fault> {
    if request.operation != "changes.wait" {
        return Err(invalid());
    }
    let seconds = match request.args.get("timeout") {
        None => 60,
        Some(value) => value
            .as_u64()
            .filter(|seconds| (1..=300).contains(seconds))
            .ok_or_else(invalid)?,
    };
    Ok(Duration::from_secs(seconds))
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn principal_fingerprint(actor: &str, request: &Request) -> Result<String, Fault> {
    let token = request.auth.as_deref().ok_or_else(Fault::authentication)?;
    Ok(digest(
        &serde_json::to_vec(&(actor, digest(token.as_bytes()))).map_err(|_| Fault::internal())?,
    ))
}

fn scope_fingerprint(filter: &Filter) -> Result<String, Fault> {
    Ok(digest(
        &serde_json::to_vec(&(filter.board_id.as_deref(), filter.thread_id.as_deref()))
            .map_err(|_| Fault::internal())?,
    ))
}

fn context_fingerprint(context: &Value) -> Result<String, Fault> {
    Ok(digest(
        &serde_json::to_vec(context).map_err(|_| Fault::internal())?,
    ))
}

fn token_key(token: &str) -> String {
    format!("changes.cursor.token.{}", digest(token.as_bytes()))
}

fn state_key(cursor: &Cursor) -> Result<String, Fault> {
    let state = serde_json::to_vec(cursor).map_err(|_| Fault::internal())?;
    Ok(format!("changes.cursor.state.{}", digest(&state)))
}

/// Cursors are opaque random handles; their sequence and access binding live
/// only in protected database metadata. A deterministic state index reuses an
/// existing handle when a poll has made no progress, avoiding writes on every
/// timeout retry.
fn encode(db: &Connection, cursor: &Cursor) -> Result<String, Fault> {
    let state = serde_json::to_string(cursor).map_err(|_| Fault::internal())?;
    let state_key = state_key(cursor)?;
    let existing: Option<String> = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [&state_key],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(token) = existing {
        if !valid_hex(&token, 64) {
            return Err(Fault::internal());
        }
        let stored: Option<String> = db
            .query_row(
                "SELECT value FROM app_metadata WHERE key=?1",
                [token_key(&token)],
                |row| row.get(0),
            )
            .optional()?;
        if stored.as_deref() != Some(state.as_str()) {
            return Err(Fault::internal());
        }
        return Ok(token);
    }
    let token = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
        params![state_key, token],
    )?;
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
        params![token_key(&token), state],
    )?;
    Ok(token)
}

fn decode(db: &Connection, value: &str, now: i64) -> Result<Cursor, Fault> {
    if !valid_hex(value, 64) {
        return Err(invalid());
    }
    let stored: Option<String> = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [token_key(value)],
            |row| row.get(0),
        )
        .optional()?;
    let Some(stored) = stored else {
        return Err(resync());
    };
    let value: Value = serde_json::from_str(&stored).map_err(|_| Fault::internal())?;
    if value.get("version").and_then(Value::as_u64) != Some(crate::cursor_metadata::VERSION as u64)
        || value.get("issued_at").and_then(Value::as_i64).is_none()
    {
        return Err(resync());
    }
    let cursor: Cursor = serde_json::from_value(value).map_err(|_| Fault::internal())?;
    crate::cursor_metadata::validate(cursor.version, cursor.issued_at, now)?;
    if cursor.sequence < 0
        || !valid_hex(&cursor.epoch, 32)
        || !valid_hex(&cursor.principal, 64)
        || !valid_hex(&cursor.scope, 64)
        || !valid_hex(&cursor.context, 64)
        || !valid_hex(&cursor.access, 64)
    {
        return Err(Fault::internal());
    }
    Ok(cursor)
}

fn ai_can(policy: &Policy, request: &Request, operation: &str, target: Target<'_>) -> bool {
    let mut check = request.clone();
    check.operation = operation.to_owned();
    policy.can(&check, target)
}

fn human_can(
    db: &Connection,
    actor: &str,
    operation: &str,
    target: Target<'_>,
) -> Result<bool, Fault> {
    human::can(db, actor, operation, target.board, target.thread)
}

fn validate_scope(
    db: &Connection,
    request: &Request,
    actor: &str,
    policy: Option<&Policy>,
    filter: &Filter,
) -> Result<(), Fault> {
    if let Some(board) = filter.board_id.as_deref() {
        let deleted: Option<Option<String>> = db
            .query_row(
                "SELECT deleted_at FROM boards WHERE id=?1",
                [board],
                |row| row.get(0),
            )
            .optional()?;
        let Some(deleted) = deleted else {
            return Err(missing());
        };
        let target = Target {
            board: Some(board),
            thread: None,
        };
        let readable = if policy.is_some() && deleted.is_some() {
            false
        } else if let Some(policy) = policy {
            ai_can(policy, request, request.operation.as_str(), target)
                && ai_can(policy, request, "board.list", target)
                && ai_can(policy, request, "board.get", target)
        } else {
            human_can(db, actor, request.operation.as_str(), target)?
                && human_can(db, actor, "board.list", target)?
                && human_can(db, actor, "board.get", target)?
                && (deleted.is_none() || human_can(db, actor, "board.restore", target)?)
        };
        if !readable {
            return Err(missing());
        }
    } else if let Some(thread) = filter.thread_id.as_deref() {
        let row: Option<(String, Option<String>)> = db
            .query_row(
                "SELECT t.board_id,t.deleted_at FROM threads t JOIN boards b ON b.id=t.board_id \
                 WHERE t.id=?1 AND b.deleted_at IS NULL",
                [thread],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((board, deleted)) = row else {
            return Err(missing());
        };
        let target = Target {
            board: Some(&board),
            thread: Some(thread),
        };
        let readable = if policy.is_some() && deleted.is_some() {
            false
        } else if let Some(policy) = policy {
            ai_can(policy, request, request.operation.as_str(), target)
                && ai_can(policy, request, "thread.list", target)
                && ai_can(policy, request, "thread.get", target)
        } else {
            human_can(db, actor, request.operation.as_str(), target)?
                && human_can(db, actor, "thread.list", target)?
                && human_can(db, actor, "thread.get", target)?
                && (deleted.is_none() || human_can(db, actor, "thread.restore", target)?)
        };
        if !readable {
            return Err(missing());
        }
    } else {
        let mut allowed = false;
        let mut query = db.prepare("SELECT id,deleted_at FROM boards ORDER BY id")?;
        let boards = query
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (board, deleted) in boards {
            let target = Target {
                board: Some(&board),
                thread: None,
            };
            allowed = if policy.is_some() && deleted.is_some() {
                false
            } else if let Some(policy) = policy {
                ai_can(policy, request, request.operation.as_str(), target)
                    && ai_can(policy, request, "board.list", target)
                    && ai_can(policy, request, "board.get", target)
            } else {
                human_can(db, actor, request.operation.as_str(), target)?
                    && human_can(db, actor, "board.list", target)?
                    && human_can(db, actor, "board.get", target)?
                    && (deleted.is_none() || human_can(db, actor, "board.restore", target)?)
            };
            if allowed {
                break;
            }
        }
        if !allowed {
            let mut query = db.prepare(
                "SELECT t.id,t.board_id,t.deleted_at FROM threads t JOIN boards b ON b.id=t.board_id \
                 WHERE b.deleted_at IS NULL",
            )?;
            let rows = query
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            for (thread, board, deleted) in rows {
                let target = Target {
                    board: Some(&board),
                    thread: Some(&thread),
                };
                allowed = if policy.is_some() && deleted.is_some() {
                    false
                } else if let Some(policy) = policy {
                    ai_can(policy, request, request.operation.as_str(), target)
                        && ai_can(policy, request, "thread.list", target)
                        && ai_can(policy, request, "thread.get", target)
                } else {
                    human_can(db, actor, request.operation.as_str(), target)?
                        && human_can(db, actor, "thread.list", target)?
                        && human_can(db, actor, "thread.get", target)?
                        && (deleted.is_none() || human_can(db, actor, "thread.restore", target)?)
                };
                if allowed {
                    break;
                }
            }
        }
        if !allowed {
            return Err(denied());
        }
    }
    Ok(())
}

fn access_fingerprint(
    db: &Connection,
    actor: &str,
    policy: Option<&Policy>,
    request: &Request,
) -> Result<String, Fault> {
    match policy {
        Some(policy) => Ok(digest(
            &serde_json::to_vec(&(
                policy.access_fingerprint(),
                policy.time_predicate_fingerprint(request)?,
            ))
            .map_err(|_| Fault::internal())?,
        )),
        None => human::access_fingerprint(db, actor),
    }
}

struct Event<'a> {
    operation: &'a str,
    entity_id: &'a str,
    created_at: &'a str,
}

type PostLocationRow = (String, String, i64, Option<String>, Option<String>);

fn event_item(
    db: &Connection,
    request: &Request,
    actor: &str,
    policy: Option<&Policy>,
    filter: &Filter,
    event: Event<'_>,
) -> Result<Option<Value>, Fault> {
    let Event {
        operation,
        entity_id,
        created_at,
    } = event;
    let (board_id, thread_id, no, parents_live, target_kind, entity_deleted) =
        if operation.starts_with("board.") {
            let deleted: Option<String> = db
                .query_row(
                    "SELECT deleted_at FROM boards WHERE id=?1",
                    [entity_id],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(Fault::internal)?;
            (
                Some(entity_id.to_owned()),
                None,
                None,
                true,
                "board",
                deleted.is_some(),
            )
        } else if operation.starts_with("thread.") {
            let row: Option<(String, Option<String>, Option<String>)> = db
                .query_row(
                    "SELECT t.board_id,t.deleted_at,b.deleted_at FROM threads t \
                 JOIN boards b ON b.id=t.board_id WHERE t.id=?1",
                    [entity_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()?;
            let Some((board, thread_deleted, board_deleted)) = row else {
                return Ok(None);
            };
            (
                Some(board),
                Some(entity_id.to_owned()),
                None,
                board_deleted.is_none(),
                "thread",
                thread_deleted.is_some(),
            )
        } else if operation.starts_with("post.") || operation == "response.create" {
            let row: Option<PostLocationRow> = db
                .query_row(
                    "SELECT t.board_id,p.thread_id,p.no,t.deleted_at,b.deleted_at \
                 FROM posts p JOIN threads t ON t.id=p.thread_id \
                 JOIN boards b ON b.id=t.board_id WHERE p.id=?1",
                    [entity_id],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )
                .optional()?;
            let Some((board, thread, no, thread_deleted, board_deleted)) = row else {
                return Ok(None);
            };
            (
                Some(board),
                Some(thread),
                Some(no),
                thread_deleted.is_none() && board_deleted.is_none(),
                if operation == "response.create" {
                    "response"
                } else {
                    "post"
                },
                false,
            )
        } else {
            return Ok(None);
        };
    if !parents_live
        || filter
            .board_id
            .as_deref()
            .is_some_and(|expected| board_id.as_deref() != Some(expected))
        || filter
            .thread_id
            .as_deref()
            .is_some_and(|expected| thread_id.as_deref() != Some(expected))
    {
        return Ok(None);
    }
    let board = board_id.as_deref();
    let thread = thread_id.as_deref();
    let target = Target { board, thread };
    let can = |op: &str, target: Target<'_>| -> Result<bool, Fault> {
        match policy {
            Some(policy) => Ok(ai_can(policy, request, op, target)),
            None => human_can(db, actor, op, target),
        }
    };
    if !can(request.operation.as_str(), target)? {
        return Ok(None);
    }
    match target_kind {
        "board" => {
            if !can("board.list", target)? || !can("board.get", target)? {
                return Ok(None);
            }
            if entity_deleted && (policy.is_some() || !can("board.restore", target)?) {
                return Ok(None);
            }
        }
        "thread" => {
            if !can("thread.list", target)? || !can("thread.get", target)? {
                return Ok(None);
            }
            if entity_deleted && (policy.is_some() || !can("thread.restore", target)?) {
                return Ok(None);
            }
        }
        "post" | "response" => {
            if !can("thread.list", target)?
                || !can("thread.get", target)?
                || !can("post.list", target)?
                || !can("post.get", target)?
            {
                return Ok(None);
            }
            if target_kind == "response" && !can("response.list", target)? {
                return Ok(None);
            }
        }
        _ => return Ok(None),
    }
    // Do not expose a parent Board identifier unless that parent is itself
    // currently readable. Thread/Post IDs remain limited to a readable Thread.
    let visible_board = if let Some(id) = board {
        if can(
            "board.list",
            Target {
                board: Some(id),
                thread: None,
            },
        )? && can(
            "board.get",
            Target {
                board: Some(id),
                thread: None,
            },
        )? {
            Some(id)
        } else {
            None
        }
    } else {
        None
    };
    Ok(Some(json!({
        "operation":operation,
        "entity_id":entity_id,
        "board_id":visible_board,
        "thread_id":thread,
        "no":no,
        "created_at":created_at
    })))
}

fn build_cursor(
    sequence: i64,
    epoch: &str,
    principal: &str,
    scope: &str,
    context: &str,
    access: &str,
    issued_at: i64,
) -> Cursor {
    Cursor {
        version: crate::cursor_metadata::VERSION,
        issued_at,
        sequence,
        epoch: epoch.to_owned(),
        principal: principal.to_owned(),
        scope: scope.to_owned(),
        context: context.to_owned(),
        access: access.to_owned(),
    }
}

pub(crate) fn dispatch(
    db: &Connection,
    request: &Request,
    actor: &str,
    policy: Option<&Policy>,
) -> Result<Value, Fault> {
    let now = crate::cursor_metadata::now_epoch_seconds();
    let filter = parse(request)?;
    let epoch = cursor_epoch(db)?;
    let principal = principal_fingerprint(actor, request)?;
    let scope = scope_fingerprint(&filter)?;
    let context = context_fingerprint(&request.context)?;
    let access = access_fingerprint(db, actor, policy, request)?;
    let tip: i64 = db.query_row("SELECT COALESCE(MAX(sequence),0) FROM changes", [], |row| {
        row.get(0)
    })?;
    let Some(after) = filter.after.as_deref() else {
        validate_scope(db, request, actor, policy, &filter)?;
        let cursor = build_cursor(tip, &epoch, &principal, &scope, &context, &access, now);
        return Ok(json!({"items":[],"change_cursor":encode(db,&cursor)?,"has_more":false}));
    };
    let cursor = decode(db, after, now)?;
    if cursor.epoch != epoch
        || cursor.principal != principal
        || cursor.scope != scope
        || cursor.context != context
        || cursor.access != access
        || cursor.sequence > tip
    {
        return Err(resync());
    }
    if cursor.sequence < tip {
        let minimum: Option<i64> =
            db.query_row("SELECT MIN(sequence) FROM changes", [], |row| row.get(0))?;
        if minimum.is_some_and(|minimum| minimum > cursor.sequence.saturating_add(1)) {
            return Err(resync());
        }
    }
    validate_scope(db, request, actor, policy, &filter)?;

    let mut items = Vec::new();
    let mut scanned = cursor.sequence;
    let mut has_more = false;
    let mut statement = db.prepare(
        "SELECT sequence,operation,entity_id,created_at FROM changes \
         WHERE sequence>?1 AND sequence<=?2 ORDER BY sequence ASC LIMIT ?3",
    )?;
    'scan: loop {
        let rows = statement
            .query_map(params![scanned, tip, SCAN_BATCH_ROWS], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.is_empty() {
            scanned = tip;
            break;
        }

        for (sequence, operation, entity_id, created_at) in &rows {
            if let Some(item) = event_item(
                db,
                request,
                actor,
                policy,
                &filter,
                Event {
                    operation,
                    entity_id,
                    created_at,
                },
            )? {
                if items.len() >= filter.limit as usize {
                    has_more = true;
                    break 'scan;
                }
                items.push(item);
            }
            scanned = *sequence;
        }
        if rows.len() < SCAN_BATCH_ROWS as usize {
            scanned = tip;
            break;
        }
    }
    let issued_at = if scanned == cursor.sequence {
        cursor.issued_at
    } else {
        now
    };
    let next = build_cursor(
        scanned, &epoch, &principal, &scope, &context, &access, issued_at,
    );
    Ok(json!({"items":items,"change_cursor":encode(db,&next)?,"has_more":has_more}))
}

/// Non-mutating AI preflight for the change polling operations. This shares
/// the request-specific grant and underlying-read checks used by dispatch.
pub(crate) fn diagnose_ai_changes(
    db: &Connection,
    request: &Request,
    actor: &str,
    policy: &Policy,
) -> Result<Value, Fault> {
    let filter = parse(request)?;
    match validate_scope(db, request, actor, Some(policy), &filter) {
        Ok(()) => Ok(json!({"allowed":true,"reason":"allowed"})),
        Err(error) if error.code == "NOT_FOUND" => {
            Ok(json!({"allowed":false,"reason":"target_unavailable"}))
        }
        Err(error) if error.code == "PERMISSION_DENIED" => {
            Ok(json!({"allowed":false,"reason":"not_allowed"}))
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod persisted_cursor_tests {
    use super::*;

    #[test]
    fn legacy_and_expired_change_handles_require_resync() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE app_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);")
            .unwrap();
        let now = crate::cursor_metadata::now_epoch_seconds();
        let legacy_token = "a".repeat(64);
        db.execute(
            "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
            params![token_key(&legacy_token), r#"{"sequence":7}"#],
        )
        .unwrap();
        assert_eq!(
            decode(&db, &legacy_token, now).unwrap_err().code,
            "RESYNC_REQUIRED"
        );

        let epoch = "0".repeat(32);
        let fingerprint = "1".repeat(64);
        let expired = build_cursor(
            0,
            &epoch,
            &fingerprint,
            &fingerprint,
            &fingerprint,
            &fingerprint,
            now - crate::cursor_metadata::LIFETIME_SECONDS,
        );
        let expired_token = encode(&db, &expired).unwrap();
        assert_eq!(
            decode(&db, &expired_token, now).unwrap_err().code,
            "RESYNC_REQUIRED"
        );
    }
}
