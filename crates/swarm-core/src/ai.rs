use crate::{
    identity,
    policy::{Policy, Target},
    projects,
    protocol::{Fault, Request},
    service,
};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Value, json};

fn denied() -> Fault {
    Fault::new(
        "PERMISSION_DENIED",
        4,
        "This operation is not permitted for this AI identity",
    )
}
fn missing() -> Fault {
    Fault::new("NOT_FOUND", 5, "The requested item is not available")
}
fn number(args: &Value) -> Result<i64, Fault> {
    args["no"]
        .as_i64()
        .filter(|n| *n > 0)
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "A positive integer no is required"))
}
fn target<'a>(
    db: &Connection,
    request: &'a Request,
) -> Result<(Option<String>, Option<&'a str>), Fault> {
    let op = request.operation.as_str();
    let thread = if op.starts_with("post.") || op.starts_with("response.") {
        Some(service::required_text(&request.args, "thread_id")?)
    } else if matches!(op, "thread.get" | "thread.edit") {
        Some(service::required_text(&request.args, "id")?)
    } else if op == "search" || matches!(op, "changes.get" | "changes.wait") {
        request.args.get("thread_id").and_then(Value::as_str)
    } else {
        None
    };
    if let Some(thread) = thread {
        let board=db.query_row("SELECT t.board_id FROM threads t JOIN boards b ON b.id=t.board_id WHERE t.id=?1 AND t.deleted_at IS NULL AND b.deleted_at IS NULL",
            [thread],|row|row.get::<_,String>(0)).optional()?.ok_or_else(missing)?;
        return Ok((Some(board), Some(thread)));
    }
    let board = match op {
        "board.get" | "board.edit" => Some(service::required_text(&request.args, "id")?.to_owned()),
        "thread.create" | "thread.list" => request
            .args
            .get("board_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        "search" | "changes.get" | "changes.wait" => request
            .args
            .get("board_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    };
    Ok((board, None))
}
fn redact_parent(policy: &Policy, request: &Request, board: &str, mut result: Value) -> Value {
    let mut parent = request.clone();
    parent.operation = "board.get".into();
    if !policy.can(
        &parent,
        Target {
            board: Some(board),
            thread: None,
        },
    ) {
        result["board_id"] = Value::Null;
    }
    result
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Preflight {
    Allowed,
    NotAllowed,
    OperationProhibited,
    TargetUnavailable,
    OwnerOnly,
    ThreadFull,
}

fn prohibited(operation: &str) -> bool {
    matches!(
        operation,
        "board.delete"
            | "board.restore"
            | "thread.delete"
            | "thread.restore"
            | "post.delete"
            | "post.restore"
            | "post.deleted_body"
            | "project.register"
            | "project.alias"
            | "project.root"
            | "rules.check"
            | "rules.subjects"
            | "rules.validate"
    ) || operation.starts_with("account.")
        || operation.starts_with("role.")
        || operation.starts_with("permission.")
}

fn unavailable(error: &Fault) -> bool {
    error.code == "NOT_FOUND"
}

fn decision_fault(decision: Preflight) -> Result<(), Fault> {
    match decision {
        Preflight::Allowed => Ok(()),
        Preflight::NotAllowed | Preflight::OperationProhibited | Preflight::OwnerOnly => {
            Err(denied())
        }
        Preflight::TargetUnavailable => Err(missing()),
        Preflight::ThreadFull => Err(Fault::new(
            "THREAD_FULL",
            6,
            "This Thread has reached the limit of 1000 Posts",
        )),
    }
}

fn location_allowed(request: &Request, policy: &Policy, target: Target<'_>) -> bool {
    policy.location_accessible(target) && policy.can(request, target)
}

/// Shared non-mutating authorization preflight. It deliberately excludes
/// payload validation and does not reserve identifiers or quota.
fn preflight(
    db: &Connection,
    request: &Request,
    actor: &str,
    policy: &Policy,
) -> Result<Preflight, Fault> {
    let op = request.operation.as_str();
    if prohibited(op) {
        return Ok(Preflight::OperationProhibited);
    }

    if op == "board.list" {
        let mut query =
            db.prepare("SELECT id FROM boards WHERE deleted_at IS NULL ORDER BY created_at,id")?;
        let ids = query.query_map([], |row| row.get::<_, String>(0))?;
        let mut any = false;
        for row in ids {
            let id = row?;
            if location_allowed(
                request,
                policy,
                Target {
                    board: Some(&id),
                    thread: None,
                },
            ) {
                any = true;
                break;
            }
        }
        if any || policy.can(request, Target::default()) {
            return Ok(Preflight::Allowed);
        }
        return Ok(Preflight::NotAllowed);
    }
    if op == "thread.list" {
        let filter = request.args.get("board_id").and_then(Value::as_str);
        let mut query = db.prepare("SELECT t.id,t.board_id FROM threads t JOIN boards b ON b.id=t.board_id WHERE t.deleted_at IS NULL AND b.deleted_at IS NULL AND (?1 IS NULL OR t.board_id=?1) ORDER BY t.created_at,t.id")?;
        let rows = query.query_map([filter], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut any = false;
        for row in rows {
            let (id, board) = row?;
            if location_allowed(
                request,
                policy,
                Target {
                    board: Some(&board),
                    thread: Some(&id),
                },
            ) {
                any = true;
                break;
            }
        }
        if any
            || policy.can(
                request,
                Target {
                    board: filter,
                    thread: None,
                },
            )
        {
            return Ok(Preflight::Allowed);
        }
        return Ok(Preflight::NotAllowed);
    }

    let (board, thread) = match target(db, request) {
        Ok(value) => value,
        Err(error) if unavailable(&error) => return Ok(Preflight::TargetUnavailable),
        Err(error) => return Err(error),
    };
    let target = Target {
        board: board.as_deref(),
        thread,
    };
    if !policy.location_accessible(target) {
        // Preserve dispatch's established permission-denied behavior here.
        // Only rules.check applies the separate privacy gate that maps an
        // unreadable target to the same generic result as a missing one.
        return Ok(Preflight::NotAllowed);
    }
    if !policy.can(request, target) {
        return Ok(Preflight::NotAllowed);
    }

    match op {
        "board.get" | "board.edit" => {
            let item =
                match service::board_get(db, service::required_text(&request.args, "id")?, false) {
                    Ok(item) => item,
                    Err(error) if unavailable(&error) => return Ok(Preflight::TargetUnavailable),
                    Err(error) => return Err(error),
                };
            if op == "board.edit" && item["creator_id"] != actor {
                return Ok(Preflight::OwnerOnly);
            }
        }
        "thread.get" | "thread.edit" => {
            let item = match service::thread_get(
                db,
                service::required_text(&request.args, "id")?,
                false,
            ) {
                Ok(item) => item,
                Err(error) if unavailable(&error) => return Ok(Preflight::TargetUnavailable),
                Err(error) => return Err(error),
            };
            if op == "thread.edit" && item["creator_id"] != actor {
                return Ok(Preflight::OwnerOnly);
            }
        }
        "post.edit" | "post.history" => {
            let item = match service::post_get(
                db,
                thread.ok_or_else(Fault::internal)?,
                number(&request.args)?,
            ) {
                Ok(item) => item,
                Err(error) if unavailable(&error) => return Ok(Preflight::TargetUnavailable),
                Err(error) => return Err(error),
            };
            if op == "post.edit" && item["deleted"] == true {
                return Ok(Preflight::TargetUnavailable);
            }
            if op == "post.edit" && item["author"]["id"] != actor {
                return Ok(Preflight::OwnerOnly);
            }
        }
        "post.get" => {
            if let Err(error) = service::post_get(
                db,
                thread.ok_or_else(Fault::internal)?,
                number(&request.args)?,
            ) {
                if unavailable(&error) {
                    return Ok(Preflight::TargetUnavailable);
                }
                return Err(error);
            }
        }
        "post.create" | "response.create" => {
            let thread = thread.ok_or_else(Fault::internal)?;
            let next_no: i64 =
                db.query_row("SELECT next_no FROM threads WHERE id=?1", [thread], |row| {
                    row.get(0)
                })?;
            if next_no > 1000 {
                return Ok(Preflight::ThreadFull);
            }
            if op == "response.create" {
                let reply = match service::post_get(db, thread, number_field(&request.args, "to")?)
                {
                    Ok(item) => item,
                    Err(error) if unavailable(&error) => return Ok(Preflight::TargetUnavailable),
                    Err(error) => return Err(error),
                };
                if reply["deleted"] == true {
                    return Ok(Preflight::TargetUnavailable);
                }
            }
        }
        "thread.create" => {
            let board = board.as_deref().ok_or_else(Fault::internal)?;
            match service::board_get(db, board, false) {
                Ok(_) => {}
                Err(error) if unavailable(&error) => return Ok(Preflight::TargetUnavailable),
                Err(error) => return Err(error),
            }
        }
        "project.get" => {
            let own = identity::public(db, actor)?;
            if let Some(id) = request.args.get("id").and_then(Value::as_str)
                && own["project_id"] != id
            {
                return Ok(Preflight::TargetUnavailable);
            }
        }
        _ => {}
    }
    Ok(Preflight::Allowed)
}

fn number_field(args: &Value, field: &str) -> Result<i64, Fault> {
    args.get(field)
        .and_then(Value::as_i64)
        .filter(|n| *n > 0)
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "A positive integer target is required"))
}

fn diagnostic_target(value: Option<&Value>, operation: &str) -> Result<Value, Fault> {
    let Some(value) = value else {
        return Ok(json!({}));
    };
    let object = value
        .as_object()
        .filter(|object| {
            object
                .keys()
                .all(|key| ["id", "board_id", "thread_id", "no", "to"].contains(&key.as_str()))
        })
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "Invalid rules.check target"))?;
    let allowed: &[&str] = match operation {
        "board.list" | "board.create" | "project.list" => &[],
        "board.get" | "board.edit" | "thread.get" | "thread.edit" | "project.get" => &["id"],
        "thread.list" | "thread.create" => &["board_id"],
        "post.list" | "post.create" => &["thread_id"],
        "post.get" | "post.edit" | "post.history" => &["thread_id", "no"],
        "response.list" => &["thread_id", "to"],
        "response.create" => &["thread_id", "to"],
        "changes.get" | "changes.wait" => &["board_id", "thread_id"],
        "board.delete" | "board.restore" | "thread.delete" | "thread.restore" => &["id"],
        "post.delete" | "post.restore" | "post.deleted_body" => &["thread_id", "no"],
        "rules.check" | "rules.validate" | "project.register" | "project.alias"
        | "project.root" => &[],
        "search" => &["board_id", "thread_id"],
        _ => return Err(Fault::new("INVALID_INPUT", 2, "Invalid rules.check target")),
    };
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(Fault::new("INVALID_INPUT", 2, "Invalid rules.check target"));
    }
    let required: &[&str] = match operation {
        "board.get" | "board.edit" | "thread.get" | "thread.edit" | "project.get" => &["id"],
        "thread.create" => &["board_id"],
        "post.list" | "post.create" => &["thread_id"],
        "post.get" | "post.edit" | "post.history" | "post.delete" | "post.restore"
        | "post.deleted_body" => &["thread_id", "no"],
        "response.list" | "response.create" => &["thread_id", "to"],
        "changes.get" | "changes.wait" => &[],
        "board.delete" | "board.restore" | "thread.delete" | "thread.restore" => &["id"],
        _ => &[],
    };
    if required.iter().any(|key| !object.contains_key(*key)) {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Rules.check target is incomplete",
        ));
    }
    let mut args = serde_json::Map::new();
    for (key, value) in object {
        let expected = if matches!(key.as_str(), "no" | "to") {
            value.as_i64().is_some_and(|number| number > 0)
        } else {
            value.as_str().is_some_and(|text| !text.trim().is_empty())
        };
        if !expected {
            return Err(Fault::new("INVALID_INPUT", 2, "Invalid rules.check target"));
        }
        args.insert(key.to_owned(), value.clone());
    }
    Ok(Value::Object(args))
}

fn supported_for_preflight(operation: &str) -> bool {
    matches!(
        operation,
        "board.list"
            | "board.get"
            | "board.create"
            | "board.edit"
            | "thread.list"
            | "thread.get"
            | "thread.create"
            | "thread.edit"
            | "post.list"
            | "post.get"
            | "post.create"
            | "post.edit"
            | "post.history"
            | "response.create"
            | "response.list"
            | "project.list"
            | "project.get"
            | "project.register"
            | "rules.check"
            | "search"
            | "changes.get"
            | "changes.wait"
    )
}

fn known_prohibited(operation: &str) -> bool {
    matches!(
        operation,
        "board.delete"
            | "board.restore"
            | "thread.delete"
            | "thread.restore"
            | "post.delete"
            | "post.restore"
            | "post.deleted_body"
            | "project.alias"
            | "project.root"
            | "rules.validate"
    )
}

pub(crate) fn rules_check(
    db: &Connection,
    request: &Request,
    actor: &str,
    policy: &Policy,
) -> Result<Value, Fault> {
    let args = request
        .args
        .as_object()
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "Arguments must be an object"))?;
    if args
        .keys()
        .any(|key| !["operation", "target"].contains(&key.as_str()))
    {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Unsupported rules.check argument",
        ));
    }
    if request.context.as_object().is_none()
        || serde_json::to_vec(&request.context).map_or(true, |bytes| bytes.len() > 64 * 1024)
    {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Context must be an object no larger than 64 KiB",
        ));
    }
    let operation = args
        .get("operation")
        .and_then(Value::as_str)
        .filter(|operation| !operation.trim().is_empty())
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "A rules.check operation is required"))?;
    let mut self_check = request.clone();
    self_check.operation = "rules.check".into();
    self_check.args = json!({});
    if !policy.can(&self_check, Target::default()) {
        return Ok(json!({"allowed":false,"reason":"not_allowed"}));
    }
    diagnose_permission(
        db,
        operation,
        args.get("target"),
        &request.context,
        actor,
        policy,
    )
}

pub(crate) fn diagnose_permission(
    db: &Connection,
    operation: &str,
    target: Option<&Value>,
    context: &Value,
    actor: &str,
    policy: &Policy,
) -> Result<Value, Fault> {
    if context.as_object().is_none()
        || serde_json::to_vec(context).map_or(true, |bytes| bytes.len() > 64 * 1024)
    {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Context must be an object no larger than 64 KiB",
        ));
    }
    if !known_prohibited(operation) && !supported_for_preflight(operation) {
        return Ok(json!({"allowed":false,"reason":"not_supported"}));
    }
    let target_args = diagnostic_target(target, operation)?;
    if known_prohibited(operation) {
        return Ok(json!({"allowed":false,"reason":"operation_prohibited"}));
    }
    let candidate = Request {
        operation: operation.to_owned(),
        auth: None,
        args: target_args,
        context: context.clone(),
        request_id: None,
    };
    if operation == "rules.check" {
        let allowed = policy.can(&candidate, Target::default());
        return Ok(json!({
            "allowed": allowed,
            "reason": if allowed { "allowed" } else { "not_allowed" }
        }));
    }
    if operation == "project.register" {
        let allowed = policy.can_register_project(&candidate);
        return Ok(json!({
            "allowed": allowed,
            "reason": if allowed { "allowed" } else { "not_allowed" }
        }));
    }
    if operation == "search" {
        return crate::search::diagnose_ai_search(db, &candidate, policy);
    }
    if matches!(operation, "changes.get" | "changes.wait") {
        return crate::changes::diagnose_ai_changes(db, &candidate, actor, policy);
    }
    if !diagnostic_target_readable(db, &candidate, policy)? {
        return Ok(json!({"allowed":false,"reason":"target_unavailable"}));
    }
    match preflight(db, &candidate, actor, policy)? {
        Preflight::Allowed => Ok(json!({"allowed":true,"reason":"allowed"})),
        Preflight::NotAllowed => Ok(json!({"allowed":false,"reason":"not_allowed"})),
        Preflight::OperationProhibited => {
            Ok(json!({"allowed":false,"reason":"operation_prohibited"}))
        }
        Preflight::TargetUnavailable => Ok(json!({"allowed":false,"reason":"target_unavailable"})),
        Preflight::OwnerOnly => Ok(json!({"allowed":false,"reason":"owner_only"})),
        Preflight::ThreadFull => Ok(json!({"allowed":false,"reason":"thread_full"})),
    }
}

fn diagnostic_target_readable(
    db: &Connection,
    request: &Request,
    policy: &Policy,
) -> Result<bool, Fault> {
    let op = request.operation.as_str();
    if matches!(
        op,
        "board.list" | "board.create" | "project.list" | "project.get"
    ) || (op == "thread.list" && request.args.get("board_id").is_none())
    {
        return Ok(true);
    }
    let (board, thread) = match target(db, request) {
        Ok(target) => target,
        Err(error) if unavailable(&error) => return Ok(false),
        Err(error) => return Err(error),
    };
    let operation = match op {
        "board.get" | "board.edit" | "thread.list" => "board.get",
        "thread.get" | "thread.edit" => "thread.get",
        "thread.create" => "board.get",
        "post.get" | "post.edit" | "post.history" => "post.get",
        "post.list" | "post.create" | "response.list" | "response.create" => "thread.get",
        _ => return Ok(true),
    };
    let read = Request {
        operation: operation.into(),
        auth: None,
        args: json!({}),
        context: request.context.clone(),
        request_id: None,
    };
    let location = if op == "thread.create" {
        Target {
            board: board.as_deref(),
            thread: None,
        }
    } else {
        Target {
            board: board.as_deref(),
            thread,
        }
    };
    if !policy.location_accessible(location) || !policy.can(&read, location) {
        return Ok(false);
    }
    match op {
        "board.get" | "board.edit" | "thread.list" => {
            let field = if op == "thread.list" {
                "board_id"
            } else {
                "id"
            };
            match service::board_get(db, service::required_text(&request.args, field)?, false) {
                Ok(_) => Ok(true),
                Err(error) if unavailable(&error) => Ok(false),
                Err(error) => Err(error),
            }
        }
        "thread.get" | "thread.edit" => {
            match service::thread_get(db, service::required_text(&request.args, "id")?, false) {
                Ok(_) => Ok(true),
                Err(error) if unavailable(&error) => Ok(false),
                Err(error) => Err(error),
            }
        }
        "thread.create" => {
            match service::board_get(db, board.as_deref().ok_or_else(Fault::internal)?, false) {
                Ok(_) => Ok(true),
                Err(error) if unavailable(&error) => Ok(false),
                Err(error) => Err(error),
            }
        }
        "post.get" | "post.edit" | "post.history" => match service::post_get(
            db,
            thread.ok_or_else(Fault::internal)?,
            number(&request.args)?,
        ) {
            Ok(item) => Ok(op == "post.get" || op == "post.history" || item["deleted"] != true),
            Err(error) if unavailable(&error) => Ok(false),
            Err(error) => Err(error),
        },
        "response.create" => match service::post_get(
            db,
            thread.ok_or_else(Fault::internal)?,
            number_field(&request.args, "to")?,
        ) {
            Ok(item) => Ok(item["deleted"] != true),
            Err(error) if unavailable(&error) => Ok(false),
            Err(error) => Err(error),
        },
        "post.list" | "post.create" | "response.list" => Ok(true),
        _ => Ok(true),
    }
}
pub(crate) fn dispatch(
    db: &Connection,
    request: &Request,
    actor: &str,
    policy: &Policy,
) -> Result<Value, Fault> {
    let op = request.operation.as_str();
    if matches!(
        op,
        "conversation.capabilities"
            | "conversation.deleted"
            | "conversation.pins"
            | "conversation.activity"
            | "conversation.subscriptions"
            | "conversation.unread"
            | "thread.pin"
            | "thread.subscribe"
            | "post.read.mark"
            | "notification.settings.get"
            | "notification.settings.set"
            | "display.settings.get"
            | "display.settings.set"
            | "ui.preferences.get"
            | "ui.preferences.set"
            | "management.capabilities"
            | "authorization.capabilities"
            | "post.page"
            | "rules.subjects"
    ) {
        return Err(denied());
    }
    if op == "rules.check" {
        return rules_check(db, request, actor, policy);
    }
    if op.starts_with("identity.") {
        return identity::dispatch(db, op, &request.args, actor);
    }
    if op == "search" {
        return crate::search::ai(db, request, actor, policy);
    }
    if matches!(op, "changes.get" | "changes.wait") {
        return crate::changes::dispatch(db, request, actor, Some(policy));
    }
    if prohibited(op) {
        return Err(denied());
    }
    if op == "project.get" || op == "project.list" {
        decision_fault(preflight(db, request, actor, policy)?)?;
        let own = identity::public(db, actor)?;
        let id = own["project_id"].as_str().ok_or_else(Fault::internal)?;
        if op == "project.get" {
            if service::required_text(&request.args, "id")? != id {
                return Err(missing());
            }
            return projects::get(db, id);
        }
        return Ok(json!({"items":[projects::get(db,id)?]}));
    }
    if matches!(op, "board.list" | "thread.list") {
        decision_fault(preflight(db, request, actor, policy)?)?;
        return crate::listing::dispatch(db, request, actor, Some(policy));
    }
    let decision = preflight(db, request, actor, policy)?;
    // Current visibility and grants were checked before ThreadFull. The common
    // dispatcher must resolve an actor-bound creation receipt before enforcing
    // capacity, so confirmed retries remain usable without permitting new Posts.
    if decision != Preflight::ThreadFull {
        decision_fault(decision)?;
    }
    let (board, _thread) = target(db, request)?;
    // Operation grants never permit AI access to retained deleted bodies,
    // including revisions saved before deletion. Keep human recovery separate.
    if op == "post.history" {
        let current = service::post_get(
            db,
            service::required_text(&request.args, "thread_id")?,
            number(&request.args)?,
        )?;
        if current["deleted"] == true {
            return Ok(json!({"items":[]}));
        }
    }
    let mut result = service::dispatch(db, request, actor)?;
    if matches!(
        op,
        "thread.get" | "thread.edit" | "post.get" | "post.list" | "response.list" | "post.history"
    ) {
        let thread = if op.starts_with("thread.") {
            service::required_text(&request.args, "id")?
        } else {
            service::required_text(&request.args, "thread_id")?
        };
        result = crate::thread_rules::attach(db, request, actor, policy, thread, result)?;
    }
    if op == "board.create" && !result["deleted_at"].is_null() {
        return Err(Fault::new("NOT_FOUND", 5, "Board is unavailable"));
    }
    if op == "board.create" {
        let mut read = request.clone();
        read.operation = "board.get".into();
        policy.require(
            &read,
            Target {
                board: result["id"].as_str(),
                thread: None,
            },
        )?;
    }
    if op == "thread.create" {
        let id = result["id"].as_str().ok_or_else(Fault::internal)?;
        let current = service::thread_get(db, id, false)?;
        let parent = current["board_id"]
            .as_str()
            .ok_or_else(Fault::internal)?
            .to_owned();
        let mut read = request.clone();
        read.operation = "thread.get".into();
        policy.require(
            &read,
            Target {
                board: Some(&parent),
                thread: Some(id),
            },
        )?;
        let current = crate::thread_rules::attach(db, request, actor, policy, id, current)?;
        return Ok(redact_parent(policy, request, &parent, current));
    }
    if op == "thread.get" || op == "thread.edit" {
        Ok(redact_parent(
            policy,
            request,
            board.as_deref().ok_or_else(Fault::internal)?,
            result,
        ))
    } else {
        Ok(result)
    }
}
