use crate::{
    human,
    protocol::{Fault, Request},
    service,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn missing() -> Fault {
    Fault::new("NOT_FOUND", 5, "The requested item is not available")
}
fn parent(db: &Connection, thread: &str) -> Result<String, Fault> {
    db.query_row("SELECT board_id FROM threads WHERE id=?1", [thread], |r| {
        r.get(0)
    })
    .optional()?
    .ok_or_else(missing)
}
fn redact_parent(db: &Connection, actor: &str, mut item: Value) -> Result<Value, Fault> {
    let board = item["board_id"].as_str().ok_or_else(Fault::internal)?;
    if !human::can(db, actor, "board.get", Some(board), None)? {
        item["board_id"] = Value::Null;
    }
    Ok(item)
}

fn conversation_capabilities(db: &Connection, actor: &str) -> Result<Value, Fault> {
    let board_create = human::can(db, actor, "board.create", None, None)?
        && human::can(db, actor, "board.get", None, None)?;
    let mut boards = Vec::new();
    let mut query =
        db.prepare("SELECT id FROM boards WHERE deleted_at IS NULL ORDER BY created_at,id")?;
    let board_ids = query
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for id in board_ids {
        if !human::can(db, actor, "board.list", Some(&id), None)?
            || !human::can(db, actor, "board.get", Some(&id), None)?
        {
            continue;
        }
        let thread_create = human::can(db, actor, "thread.create", Some(&id), None)?
            && human::can(db, actor, "thread.get", Some(&id), None)?;
        let board_delete = human::can(db, actor, "board.delete", Some(&id), None)?;
        let search = human::can(db, actor, "search", Some(&id), None)?;
        boards.push(json!({"id":id,"thread_create":thread_create,"board_delete":board_delete,"search":search}));
    }

    let mut threads = Vec::new();
    let mut query = db.prepare(
        "SELECT t.id,t.board_id FROM threads t JOIN boards b ON b.id=t.board_id \
         WHERE t.deleted_at IS NULL AND b.deleted_at IS NULL ORDER BY t.created_at,t.id",
    )?;
    let thread_rows = query
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, board) in thread_rows {
        if !human::can(db, actor, "thread.list", Some(&board), Some(&id))?
            || !human::can(db, actor, "thread.get", Some(&board), Some(&id))?
        {
            continue;
        }
        let can_view_posts = human::can(db, actor, "post.list", Some(&board), Some(&id))?
            && human::can(db, actor, "post.get", Some(&board), Some(&id))?;
        let search = human::can(db, actor, "search", Some(&board), Some(&id))?;
        let response_read =
            can_view_posts && human::can(db, actor, "response.list", Some(&board), Some(&id))?;
        let archived = service::thread_get(db, &id, false)?["archived"]
            .as_bool()
            .ok_or_else(Fault::internal)?;
        let post_create = !archived
            && can_view_posts
            && human::can(db, actor, "post.create", Some(&board), Some(&id))?;
        let response_create = !archived
            && can_view_posts
            && human::can(db, actor, "response.create", Some(&board), Some(&id))?;
        let post_edit =
            can_view_posts && human::can(db, actor, "post.edit", Some(&board), Some(&id))?;
        let post_history =
            can_view_posts && human::can(db, actor, "post.history", Some(&board), Some(&id))?;
        let thread_delete = human::can(db, actor, "thread.delete", Some(&board), Some(&id))?;
        let post_delete =
            can_view_posts && human::can(db, actor, "post.delete", Some(&board), Some(&id))?;
        let post_restore =
            can_view_posts && human::can(db, actor, "post.restore", Some(&board), Some(&id))?;
        threads.push(json!({
            "id":id,
            "post_read":can_view_posts,
            "search":search,
            "response_read":response_read,
            "post_create":post_create,
            "response_create":response_create,
            "post_edit":post_edit,
            "post_history":post_history,
            "thread_delete":thread_delete,
            "post_delete":post_delete,
            "post_restore":post_restore
        }));
    }
    Ok(json!({"board_create":board_create,"boards":boards,"threads":threads}))
}

fn deleted_conversations(db: &Connection, actor: &str) -> Result<Value, Fault> {
    let mut boards = Vec::new();
    let mut statement = db.prepare(
        "SELECT id,name,description,created_at,deleted_at FROM boards \
         WHERE deleted_at IS NOT NULL ORDER BY created_at,id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, name, description, created_at, deleted_at) in rows {
        let visible = human::can(db, actor, "board.list", Some(&id), None)?
            && human::can(db, actor, "board.get", Some(&id), None)?;
        let can_restore = human::can(db, actor, "board.restore", Some(&id), None)?;
        let can_purge = human::can(db, actor, "board.purge", Some(&id), None)?;
        if visible && (can_restore || can_purge) {
            boards.push(json!({"id":id,"name":name,"description":description,
                "created_at":created_at,"deleted_at":deleted_at,
                "can_restore":can_restore,"can_purge":can_purge}));
        }
    }

    let mut threads = Vec::new();
    let mut statement = db.prepare(
        "SELECT t.id,t.board_id,t.name,t.description,t.created_at,t.deleted_at,b.deleted_at IS NOT NULL \
         FROM threads t JOIN boards b ON b.id=t.board_id \
         WHERE t.deleted_at IS NOT NULL ORDER BY t.created_at,t.id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, bool>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, board, name, description, created_at, deleted_at, parent_deleted) in rows {
        let can_view_parent = human::can(db, actor, "board.get", Some(&board), None)?;
        let visible = human::can(db, actor, "thread.list", Some(&board), Some(&id))?
            && human::can(db, actor, "thread.get", Some(&board), Some(&id))?
            && (!parent_deleted || can_view_parent);
        let can_restore = human::can(db, actor, "thread.restore", Some(&board), Some(&id))?;
        let can_purge = human::can(db, actor, "thread.purge", Some(&board), Some(&id))?;
        if visible && (can_restore || can_purge) {
            threads.push(json!({"id":id,"board_id":if can_view_parent { json!(board) } else { Value::Null },
                "name":name,"description":description,"created_at":created_at,"deleted_at":deleted_at,
                "parent_deleted":parent_deleted,"can_restore":can_restore,"can_purge":can_purge}));
        }
    }
    Ok(json!({"boards":boards,"threads":threads}))
}

fn stale_selection() -> Fault {
    Fault::new(
        "STALE_SELECTION",
        6,
        "The deleted conversation changed; refresh and try again",
    )
}

fn confirmed(args: &Value) -> Result<(), Fault> {
    if args.get("confirmed").and_then(Value::as_bool) == Some(true) {
        Ok(())
    } else {
        Err(Fault::new(
            "CONFIRMATION_REQUIRED",
            4,
            "Permanent deletion requires explicit confirmation",
        ))
    }
}

fn purge_one(db: &Connection, request: &Request, actor: &str) -> Result<Value, Fault> {
    if request.args.as_object().is_none_or(|object| {
        object.len() != 2
            || object
                .keys()
                .any(|key| !["id", "confirmed"].contains(&key.as_str()))
    }) {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Purge requires only id and confirmed",
        ));
    }
    confirmed(&request.args)?;
    let id = service::required_text(&request.args, "id")?;
    let kind = if request.operation == "board.purge" {
        crate::trash::ConversationKind::Board
    } else {
        crate::trash::ConversationKind::Thread
    };
    if kind == crate::trash::ConversationKind::Board {
        human::require(db, actor, "board.purge", Some(id), None)?;
        human::require(db, actor, "board.list", Some(id), None)?;
        human::require(db, actor, "board.get", Some(id), None)?;
    } else {
        let board = parent(db, id)?;
        human::require(db, actor, "thread.purge", Some(&board), Some(id))?;
        human::require(db, actor, "thread.list", Some(&board), Some(id))?;
        human::require(db, actor, "thread.get", Some(&board), Some(id))?;
    }
    if !crate::trash::deleted(db, kind, id)? {
        return Err(stale_selection());
    }
    crate::trash::purge_conversation(db, kind, id, false)
}

#[derive(Clone)]
struct BatchItem {
    kind: crate::trash::ConversationKind,
    id: String,
    parent: Option<String>,
}

fn conversation_batch(db: &Connection, request: &Request, actor: &str) -> Result<Value, Fault> {
    let args = request
        .args
        .as_object()
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "Batch arguments must be an object"))?;
    if args.len() != 3
        || args
            .keys()
            .any(|key| !["action", "items", "confirmed"].contains(&key.as_str()))
    {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Batch requires action, items, and confirmed",
        ));
    }
    confirmed(&request.args)?;
    let action = request.args["action"]
        .as_str()
        .filter(|value| matches!(*value, "restore" | "purge"))
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "Batch action must be restore or purge"))?;
    let raw_items = request.args["items"]
        .as_array()
        .filter(|items| !items.is_empty())
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "Batch requires at least one item"))?;
    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::with_capacity(raw_items.len());
    for value in raw_items {
        let object = value
            .as_object()
            .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "Each batch item must be an object"))?;
        if object.len() != 2
            || object
                .keys()
                .any(|key| !["kind", "id"].contains(&key.as_str()))
        {
            return Err(Fault::new(
                "INVALID_INPUT",
                2,
                "Each batch item requires only kind and id",
            ));
        }
        let kind = match value["kind"].as_str() {
            Some("board") => crate::trash::ConversationKind::Board,
            Some("thread") => crate::trash::ConversationKind::Thread,
            _ => {
                return Err(Fault::new(
                    "INVALID_INPUT",
                    2,
                    "Batch item kind must be board or thread",
                ));
            }
        };
        let id = value["id"]
            .as_str()
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "Batch item id must be nonblank text"))?
            .to_owned();
        if !seen.insert((kind.as_str(), id.clone())) {
            return Err(Fault::new("INVALID_INPUT", 2, "Batch items must be unique"));
        }
        let parent_id = if kind == crate::trash::ConversationKind::Thread {
            Some(parent(db, &id)?)
        } else {
            None
        };
        let operation = match (action, kind) {
            ("restore", crate::trash::ConversationKind::Board) => "board.restore",
            ("restore", crate::trash::ConversationKind::Thread) => "thread.restore",
            ("purge", crate::trash::ConversationKind::Board) => "board.purge",
            ("purge", crate::trash::ConversationKind::Thread) => "thread.purge",
            _ => unreachable!(),
        };
        if let Some(board) = parent_id.as_deref() {
            human::require(db, actor, operation, Some(board), Some(&id))?;
            human::require(db, actor, "thread.list", Some(board), Some(&id))?;
            human::require(db, actor, "thread.get", Some(board), Some(&id))?;
            if !human::can(db, actor, "board.get", Some(board), None)? {
                return Err(Fault::new(
                    "PERMISSION_DENIED",
                    4,
                    "This operation is not permitted",
                ));
            }
        } else {
            human::require(db, actor, operation, Some(&id), None)?;
            human::require(db, actor, "board.list", Some(&id), None)?;
            human::require(db, actor, "board.get", Some(&id), None)?;
        }
        if !crate::trash::deleted(db, kind, &id)? {
            return Err(stale_selection());
        }
        items.push(BatchItem {
            kind,
            id,
            parent: parent_id,
        });
    }
    let selected_boards = items
        .iter()
        .filter(|item| item.kind == crate::trash::ConversationKind::Board)
        .map(|item| item.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    if action == "restore" {
        for item in &items {
            if let Some(board) = item.parent.as_deref()
                && crate::trash::parent_board_deleted(db, &item.id)?
                && !selected_boards.contains(board)
            {
                return Err(stale_selection());
            }
        }
    }
    let mut execution = items.clone();
    if action == "restore" {
        execution.sort_by_key(|item| {
            (
                item.kind != crate::trash::ConversationKind::Board,
                item.id.clone(),
            )
        });
    }
    let mut results = std::collections::HashMap::new();
    for item in execution {
        if action == "purge"
            && item.kind == crate::trash::ConversationKind::Thread
            && item
                .parent
                .as_deref()
                .is_some_and(|board| selected_boards.contains(board))
        {
            results.insert(
                (item.kind.as_str(), item.id.clone()),
                json!({"kind":"thread","id":item.id,"purged":true,"covered_by_board":item.parent}),
            );
            continue;
        }
        if action == "purge" {
            let result = crate::trash::purge_conversation(db, item.kind, &item.id, false)?;
            results.insert((item.kind.as_str(), item.id.clone()), result);
        } else {
            let mut nested = request.clone();
            nested.operation = format!("{}.restore", item.kind.as_str());
            nested.args = json!({"id":item.id});
            let result = service::dispatch(db, &nested, actor)?;
            results.insert((item.kind.as_str(), item.id.clone()), result);
        }
    }
    let results = items
        .iter()
        .map(|item| {
            results
                .remove(&(item.kind.as_str(), item.id.clone()))
                .ok_or_else(Fault::internal)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"action":action,"items":results}))
}

pub(crate) fn can_read_thread(
    db: &Connection,
    actor: &str,
    board: &str,
    thread: &str,
    posts: bool,
) -> Result<bool, Fault> {
    for op in ["thread.list", "thread.get"] {
        if !human::can(db, actor, op, Some(board), Some(thread))? {
            return Ok(false);
        }
    }
    if posts {
        for op in ["post.list", "post.get"] {
            if !human::can(db, actor, op, Some(board), Some(thread))? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn pins(db: &Connection, actor: &str) -> Result<Value, Fault> {
    let mut query = db.prepare("SELECT t.id,t.board_id FROM thread_pins p JOIN threads t ON t.id=p.thread_id JOIN boards b ON b.id=t.board_id WHERE p.account_id=?1 AND t.deleted_at IS NULL AND b.deleted_at IS NULL ORDER BY p.created_at DESC,t.id")?;
    let rows = query
        .query_map([actor], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut items = Vec::new();
    for (id, board) in rows {
        if can_read_thread(db, actor, &board, &id, true)? {
            items.push(redact_parent(
                db,
                actor,
                service::thread_get(db, &id, false)?,
            )?);
        }
    }
    Ok(json!({"items":items}))
}

fn pin(db: &Connection, actor: &str, args: &Value) -> Result<Value, Fault> {
    let id = service::required_text(args, "id")?;
    let pinned = args["pinned"]
        .as_bool()
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "A boolean pinned is required"))?;
    let board = parent(db, id)?;
    for op in ["thread.list", "thread.get", "post.list", "post.get"] {
        human::require(db, actor, op, Some(&board), Some(id))?;
    }
    service::thread_get(db, id, false)?;
    if pinned {
        db.execute("INSERT OR IGNORE INTO thread_pins(account_id,thread_id,created_at) VALUES(?1,?2,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![actor,id])?;
    } else {
        db.execute(
            "DELETE FROM thread_pins WHERE account_id=?1 AND thread_id=?2",
            params![actor, id],
        )?;
    }
    Ok(json!({"id":id,"pinned":pinned}))
}

fn activity(db: &Connection, actor: &str, args: &Value) -> Result<Value, Fault> {
    let positive = |key: &str, default: i64| -> Result<i64, Fault> {
        match args.get(key) {
            None => Ok(default),
            Some(value) => value
                .as_i64()
                .filter(|n| *n > 0)
                .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "A positive integer is required")),
        }
    };
    let limit = positive("limit", 50)?;
    if limit > 100 {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Activity limit must not exceed 100",
        ));
    }
    let before = positive("before_id", i64::MAX)?;
    let mut query = db.prepare("SELECT sequence,operation,entity_id,created_at FROM changes WHERE sequence<?1 ORDER BY sequence DESC LIMIT ?2")?;
    let mut rows = query
        .query_map(params![before, limit + 1], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let cursor = if more { rows.last().map(|r| r.0) } else { None };
    let mut items = Vec::new();
    for (id, action, entity, occurred_at) in rows {
        let kind = action.split('.').next().unwrap_or("");
        let verb = action.split('.').nth(1).unwrap_or("");
        if !matches!(verb, "create" | "edit" | "delete" | "restore") {
            continue;
        }
        let (thread, no) = match kind {
            "post" | "response" => {
                let found = db
                    .query_row(
                        "SELECT thread_id,no FROM posts WHERE id=?1",
                        [&entity],
                        |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
                    )
                    .optional()?;
                let Some((thread, no)) = found else {
                    continue;
                };
                (Some(thread), Some(no))
            }
            "thread" => (Some(entity.clone()), None),
            "board" => (None, None),
            _ => continue,
        };
        let mut thread_name = None;
        let board = if let Some(thread) = thread.as_deref() {
            let metadata = service::thread_get(db, thread, true)?;
            let board = metadata["board_id"]
                .as_str()
                .ok_or_else(Fault::internal)?
                .to_owned();
            if !can_read_thread(db, actor, &board, thread, no.is_some())?
                || (!metadata["deleted_at"].is_null()
                    && !human::can(db, actor, "thread.restore", Some(&board), Some(thread))?)
            {
                continue;
            }
            thread_name = metadata["name"].as_str().map(str::to_owned);
            board
        } else {
            entity
        };
        let metadata = service::board_get(db, &board, true)?;
        let visible_board = human::can(db, actor, "board.list", Some(&board), None)?
            && human::can(db, actor, "board.get", Some(&board), None)?;
        if thread.is_none() && !visible_board {
            continue;
        }
        if !metadata["deleted_at"].is_null()
            && (!visible_board || !human::can(db, actor, "board.restore", Some(&board), None)?)
        {
            continue;
        }
        items.push(json!({"id":id,"action":action,"occurred_at":occurred_at,"thread_id":thread,"thread_name":thread_name,
            "board_id":if visible_board {Some(board)} else {None},"board_name":if visible_board {metadata["name"].as_str()} else {None},"post_no":no}));
    }
    Ok(json!({"items":items,"next_before_id":cursor}))
}

fn optional_project_id(args: &Value) -> Result<Option<&str>, Fault> {
    match args.get("project_id") {
        None => Ok(None),
        Some(Value::String(id)) if !id.trim().is_empty() => Ok(Some(id)),
        _ => Err(Fault::new(
            "INVALID_INPUT",
            2,
            "project_id must be a nonblank string when supplied",
        )),
    }
}

fn rules_path(
    db: &Connection,
    actor: &str,
    root: &Path,
    project_id: Option<&str>,
) -> Result<PathBuf, Fault> {
    let Some(project_id) = project_id else {
        return Ok(root.join("RULES.md"));
    };
    human::require(db, actor, "project.get", None, None)?;
    let canonical: Option<String> = db
        .query_row(
            "SELECT canonical_root FROM projects WHERE id=?1",
            [project_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(canonical) = canonical else {
        return Err(Fault::new(
            "PROJECT_NOT_REGISTERED",
            5,
            "The requested project is not available",
        ));
    };
    let configured = PathBuf::from(canonical);
    let resolved = std::fs::canonicalize(&configured).map_err(|_| {
        Fault::new(
            "PROJECT_ROOT_UNAVAILABLE",
            4,
            "The registered project root is unavailable",
        )
    })?;
    if !resolved.is_dir() || resolved != configured {
        return Err(Fault::new(
            "PROJECT_ROOT_UNAVAILABLE",
            4,
            "The registered project root is unavailable",
        ));
    }
    Ok(resolved.join("RULES.md"))
}

fn rules_capabilities(db: &Connection, actor: &str) -> Result<Value, Fault> {
    let permission = |operation: &str| human::can(db, actor, operation, None, None);
    let global_read = permission("rules.get")?;
    let global_check = permission("rules.check")?;
    let global_validate = permission("rules.validate")?;
    let global_save = permission("rules.save")?;
    let mut projects = Vec::new();
    if permission("project.list")? && permission("project.get")? {
        let mut statement = db.prepare("SELECT id,name FROM projects ORDER BY created_at,id")?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if global_read || global_check || global_validate || global_save {
            projects.extend(rows.into_iter().map(|(id, name)| {
                json!({"id":id,"name":name,"read":global_read,"check":global_check,"validate":global_validate,"save":global_save})
            }));
        }
    }
    Ok(json!({
        "global":{"read":global_read,"check":global_check,"validate":global_validate,"save":global_save},
        "projects":projects,
        "actions":crate::policy::action_catalog()
    }))
}

fn rules_subjects(db: &Connection, request: &Request, actor: &str) -> Result<Value, Fault> {
    let args = request
        .args
        .as_object()
        .filter(|object| object.keys().all(|key| key == "project_id"))
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "Unsupported rules.subjects argument"))?;
    let project = args
        .get("project_id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "A registered project_id is required"))?;
    for operation in ["rules.check", "rules.get", "project.list", "project.get"] {
        human::require(db, actor, operation, None, None)?;
    }
    // Resolve the canonical registration without returning its path.
    crate::projects::get(db, project)?;
    let now = service::timestamp();
    let mut statement = db.prepare(
        "SELECT i.id,i.project_id,i.model_name FROM ai_identities i \
         JOIN ai_sessions s ON s.actor_id=i.id WHERE i.project_id=?1 AND i.expires_at>?2 \
         ORDER BY i.created_at,i.id",
    )?;
    let items = statement.query_map(params![project, now], |row| {
        Ok(json!({"id":row.get::<_,String>(0)?,"project_id":row.get::<_,String>(1)?,"model_name":row.get::<_,String>(2)?}))
    })?.collect::<Result<Vec<_>, _>>()?;
    Ok(json!({"items":items}))
}

fn rules_check_human(
    db: &Connection,
    request: &Request,
    actor: &str,
    root: &Path,
) -> Result<Value, Fault> {
    let args = request
        .args
        .as_object()
        .filter(|object| {
            object.keys().all(|key| {
                ["project_id", "actor_id", "operation", "target"].contains(&key.as_str())
            })
        })
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "Unsupported rules.check argument"))?;
    let project = args
        .get("project_id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "A registered project_id is required"))?;
    let ai_actor = args
        .get("actor_id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "An eligible AI actor_id is required"))?;
    let operation = args
        .get("operation")
        .and_then(Value::as_str)
        .filter(|operation| !operation.trim().is_empty())
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "An operation is required"))?;
    for required in ["rules.check", "rules.get", "project.get"] {
        human::require(db, actor, required, None, None)?;
    }
    // This also enforces the registered-project root boundary and checks that
    // the configured root remains canonical, without exposing the path.
    let _ = rules_path(db, actor, root, Some(project))?;
    let eligible: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM ai_identities i JOIN ai_sessions s ON s.actor_id=i.id \
         WHERE i.id=?1 AND i.project_id=?2 AND i.expires_at>?3)",
        params![ai_actor, project, service::timestamp()],
        |row| row.get(0),
    )?;
    if !eligible {
        return Err(missing());
    }
    let policy = crate::policy::Policy::load(db, root, ai_actor)?;
    crate::ai::diagnose_permission(
        db,
        operation,
        args.get("target"),
        &request.context,
        ai_actor,
        &policy,
    )
}

fn rules_editor(
    db: &Connection,
    request: &Request,
    actor: &str,
    root: &Path,
) -> Result<Value, Fault> {
    let op = request.operation.as_str();
    if op == "rules.capabilities" {
        if !request
            .args
            .as_object()
            .is_some_and(|object| object.is_empty())
        {
            return Err(Fault::new(
                "INVALID_INPUT",
                2,
                "rules.capabilities accepts no arguments",
            ));
        }
        return rules_capabilities(db, actor);
    }
    if op == "rules.subjects" {
        return rules_subjects(db, request, actor);
    }
    if op == "rules.check" {
        return rules_check_human(db, request, actor, root);
    }

    let fields: &[&str] = match op {
        "rules.get" => &["project_id", "recovery_id"],
        "rules.validate" => &["project_id", "source"],
        "rules.save" => &[
            "project_id",
            "source",
            "if_revision",
            "recovery_id",
            "recovery_revision",
        ],
        _ => {
            return Err(Fault::new(
                "UNKNOWN_OPERATION",
                2,
                "Rules operation is not supported",
            ));
        }
    };
    let object = request
        .args
        .as_object()
        .filter(|object| object.keys().all(|key| fields.contains(&key.as_str())))
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "Unsupported rules argument"))?;
    if op == "rules.validate" && object.get("source").and_then(Value::as_str).is_none() {
        return Err(Fault::new("INVALID_INPUT", 2, "source must be a string"));
    }
    if op == "rules.save"
        && (object.get("source").and_then(Value::as_str).is_none()
            || object
                .get("if_revision")
                .and_then(Value::as_str)
                .is_none_or(|revision| revision.is_empty() || revision.len() > 128))
    {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "source and a valid if_revision are required",
        ));
    }
    let recovery_id = object.get("recovery_id");
    let recovery_revision = object.get("recovery_revision");
    if op == "rules.get"
        && recovery_id.is_some_and(|id| {
            id.as_str()
                .is_none_or(|id| !crate::rules_editor::valid_recovery_id(id))
        })
    {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "A valid RULES recovery identifier is required",
        ));
    }
    if op == "rules.save"
        && (recovery_id.is_some() != recovery_revision.is_some()
            || recovery_id.is_some_and(|id| {
                id.as_str()
                    .is_none_or(|id| !crate::rules_editor::valid_recovery_id(id))
            })
            || recovery_revision.is_some_and(|revision| {
                revision
                    .as_str()
                    .is_none_or(|revision| revision.is_empty() || revision.len() > 128)
            }))
    {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "RULES recovery identifier and revision must be provided together",
        ));
    }
    let project_id = optional_project_id(&request.args)?;
    human::require(db, actor, op, None, None)?;
    if op == "rules.save" && recovery_id.is_some() {
        human::require(db, actor, "rules.get", None, None)?;
    }
    let path = rules_path(db, actor, root, project_id)?;
    let scope = if project_id.is_some() {
        "project"
    } else {
        "global"
    };

    match op {
        "rules.get" => {
            if let Some(recovery_id) = recovery_id.and_then(Value::as_str) {
                let (current, candidate) =
                    crate::rules_editor::recovery_snapshot(&path, recovery_id)?;
                let current_revision = crate::rules_editor::revision(current.as_deref());
                let current_exists = current.is_some();
                let current_source = current
                    .as_deref()
                    .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok());
                let current_utf8 = current_source.is_some() || !current_exists;
                let candidate_revision = crate::rules_editor::revision(Some(&candidate));
                let candidate_source = String::from_utf8(candidate).map_err(|_| {
                    Fault::new("RULES_INVALID", 4, "RULES recovery copy is not valid UTF-8")
                })?;
                let (candidate_valid, candidate_message) =
                    match crate::policy::validate_source(&candidate_source, project_id) {
                        Ok(_) => (true, "RULES recovery copy is valid".to_owned()),
                        Err(error) if error.code == "RULES_INVALID" => (false, error.message),
                        Err(error) => return Err(error),
                    };
                return Ok(json!({
                    "scope":scope,
                    "project_id":project_id,
                    "recovery_id":recovery_id,
                    "current":{
                        "source":current_source,
                        "revision":current_revision,
                        "utf8":current_utf8,
                        "exists":current_exists
                    },
                    "candidate":{
                        "source":candidate_source,
                        "revision":candidate_revision,
                        "valid":candidate_valid,
                        "message":candidate_message
                    }
                }));
            }
            let (source, revision) = crate::rules_editor::read(&path, project_id.is_some())?;
            Ok(json!({"source":source,"revision":revision,"scope":scope,"project_id":project_id}))
        }
        "rules.validate" => {
            let source = request.args["source"]
                .as_str()
                .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "source must be a string"))?;
            match crate::policy::validate_source(source, project_id) {
                Ok(actions) => {
                    Ok(json!({"valid":true,"message":"RULES.md is valid","actions":actions}))
                }
                Err(error) if error.code == "RULES_INVALID" => {
                    Ok(json!({"valid":false,"message":error.message,"actions":[]}))
                }
                Err(error) => Err(error),
            }
        }
        "rules.save" => {
            let source = request.args["source"]
                .as_str()
                .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "source must be a string"))?;
            let expected = request.args["if_revision"]
                .as_str()
                .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "if_revision must be a string"))?;
            if let Some(recovery_id) = recovery_id.and_then(Value::as_str) {
                let recovery_revision =
                    recovery_revision.and_then(Value::as_str).ok_or_else(|| {
                        Fault::new("INVALID_INPUT", 2, "A RULES recovery revision is required")
                    })?;
                let candidate = crate::rules_editor::read_recovery_candidate(&path, recovery_id)?;
                if crate::rules_editor::revision(Some(&candidate)) != recovery_revision
                    || candidate != source.as_bytes()
                {
                    return Err(Fault::new(
                        "RULES_CHANGED",
                        6,
                        "RULES recovery copy changed; inspect it again",
                    ));
                }
            }
            crate::policy::validate_source(source, project_id)?;
            let revision = crate::rules_editor::save(&path, source, expected)?;
            Ok(json!({"source":source,"revision":revision,"scope":scope,"project_id":project_id}))
        }
        _ => unreachable!(),
    }
}

pub(crate) fn dispatch(
    db: &Connection,
    request: &Request,
    actor: &str,
    root: &Path,
) -> Result<Value, Fault> {
    let op = request.operation.as_str();
    if matches!(op, "board.purge" | "thread.purge") {
        return purge_one(db, request, actor);
    }
    if op == "conversation.batch" {
        return conversation_batch(db, request, actor);
    }
    if matches!(op, "display.settings.get" | "display.settings.set") {
        return crate::display_settings::dispatch(db, op, actor, &request.args);
    }
    if matches!(op, "ui.preferences.get" | "ui.preferences.set") {
        return crate::ui_preferences::dispatch(db, op, actor, &request.args);
    }
    if matches!(
        op,
        "conversation.subscriptions"
            | "thread.subscribe"
            | "conversation.unread"
            | "post.read.mark"
            | "notification.settings.get"
            | "notification.settings.set"
    ) {
        return crate::notifications::dispatch(db, request, actor);
    }
    if op.starts_with("rules.") {
        return rules_editor(db, request, actor, root);
    }
    if matches!(op, "changes.get" | "changes.wait") {
        return crate::changes::dispatch(db, request, actor, None);
    }
    if op == "search" {
        return crate::search::human(db, request, actor);
    }
    if op == "management.capabilities" {
        return Ok(json!({
            "account_list":human::can(db,actor,"account.list",None,None)?,
            "account_get":human::can(db,actor,"account.get",None,None)?,
            "account_create":human::can(db,actor,"account.create",None,None)?,
            "account_disable":human::can(db,actor,"account.disable",None,None)?,
            "account_reset":human::can(db,actor,"account.reset",None,None)?
        }));
    }
    if op == "authorization.capabilities" {
        return human::authorization_capabilities(db, actor);
    }
    if op == "conversation.pins" {
        return pins(db, actor);
    }
    if op == "conversation.activity" {
        return activity(db, actor, &request.args);
    }
    if op == "thread.pin" {
        return pin(db, actor, &request.args);
    }
    if op == "conversation.capabilities" {
        return conversation_capabilities(db, actor);
    }
    if op == "conversation.deleted" {
        return deleted_conversations(db, actor);
    }
    if matches!(op, "board.list" | "thread.list") {
        return crate::listing::dispatch(db, request, actor, None);
    }
    let thread = if op.starts_with("post.") || op.starts_with("response.") {
        Some(service::required_text(&request.args, "thread_id")?)
    } else if matches!(
        op,
        "thread.get" | "thread.edit" | "thread.delete" | "thread.restore"
    ) {
        Some(service::required_text(&request.args, "id")?)
    } else {
        None
    };
    let board = if let Some(thread) = thread {
        Some(parent(db, thread)?)
    } else if op.starts_with("board.") && op != "board.create" {
        Some(service::required_text(&request.args, "id")?.to_owned())
    } else if op == "thread.create" {
        Some(service::required_text(&request.args, "board_id")?.to_owned())
    } else {
        None
    };
    let permission_op = if op == "post.page" { "post.list" } else { op };
    human::require(db, actor, permission_op, board.as_deref(), thread)?;
    if op == "post.page" {
        human::require(db, actor, "post.get", board.as_deref(), thread)?;
    }
    match op {
        "board.delete" | "board.restore" => {
            human::require(db, actor, "board.list", board.as_deref(), None)?;
            human::require(db, actor, "board.get", board.as_deref(), None)?;
        }
        "thread.delete" | "thread.restore" => {
            human::require(db, actor, "thread.list", board.as_deref(), thread)?;
            human::require(db, actor, "thread.get", board.as_deref(), thread)?;
        }
        "post.delete" | "post.restore" => {
            human::require(db, actor, "thread.list", board.as_deref(), thread)?;
            human::require(db, actor, "thread.get", board.as_deref(), thread)?;
            human::require(db, actor, "post.list", board.as_deref(), thread)?;
            human::require(db, actor, "post.get", board.as_deref(), thread)?;
        }
        _ => {}
    }
    if op == "post.history" {
        let no = request.args["no"]
            .as_i64()
            .filter(|n| *n > 0)
            .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "A positive integer no is required"))?;
        let item = service::post_get(db, thread.ok_or_else(Fault::internal)?, no)?;
        if item["deleted"] == true {
            human::require(db, actor, "post.deleted_body", board.as_deref(), thread)?;
        }
    }
    let item = service::dispatch(db, request, actor)?;
    if op == "board.create" {
        if !item["deleted_at"].is_null() {
            return Err(missing());
        }
        human::require(db, actor, "board.get", item["id"].as_str(), None)?;
    }
    if op == "thread.create" {
        let id = item["id"].as_str().ok_or_else(Fault::internal)?;
        let live = service::thread_get(db, id, false)?;
        let board = live["board_id"].as_str().ok_or_else(Fault::internal)?;
        human::require(db, actor, "thread.get", Some(board), Some(id))?;
        return redact_parent(db, actor, live);
    }
    if matches!(op, "thread.get" | "thread.edit") {
        return redact_parent(db, actor, item);
    }
    Ok(item)
}
