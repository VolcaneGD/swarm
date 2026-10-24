use crate::protocol::Fault;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const DEFAULT_ALLOW: &[&str] = &[
    "board.list",
    "board.get",
    "thread.list",
    "thread.get",
    "post.list",
    "post.get",
    "post.history",
    "search",
    "post.create",
    "response.list",
    "response.create",
    "account.status",
    "account.password",
    "account.google.begin",
    "account.google.link",
    "account.google.unlink",
    "account.logout",
];

const LOCATION_OPERATIONS: &[&str] = &[
    "board.list",
    "board.get",
    "board.create",
    "board.edit",
    "board.delete",
    "board.restore",
    "board.purge",
    "thread.list",
    "thread.get",
    "thread.create",
    "thread.edit",
    "thread.delete",
    "thread.restore",
    "thread.purge",
    "post.list",
    "post.get",
    "post.create",
    "post.edit",
    "post.history",
    "post.delete",
    "post.restore",
    "post.deleted_body",
    "response.create",
    "response.list",
    "project.list",
    "project.get",
    "project.register",
    "project.alias",
    "project.root",
    "search",
    "changes.get",
    "changes.wait",
];

const OPERATIONS: &[&str] = &[
    "read",
    "board.list",
    "board.get",
    "board.create",
    "board.edit",
    "board.delete",
    "board.restore",
    "board.purge",
    "thread.list",
    "thread.get",
    "thread.create",
    "thread.edit",
    "thread.delete",
    "thread.restore",
    "thread.purge",
    "post.list",
    "post.get",
    "post.create",
    "post.edit",
    "post.history",
    "post.delete",
    "post.restore",
    "post.deleted_body",
    "retention.manage",
    "response.create",
    "response.list",
    "project.list",
    "project.get",
    "project.register",
    "project.alias",
    "project.root",
    "search",
    "changes.get",
    "changes.wait",
    "rules.check",
    "rules.validate",
    "rules.get",
    "rules.save",
    "account.status",
    "account.password",
    "account.google.begin",
    "account.google.link",
    "account.google.unlink",
    "account.logout",
    "account.create",
    "account.list",
    "account.get",
    "account.disable",
    "account.reset",
    "role.create",
    "role.list",
    "role.assign",
    "permission.set",
    "permission.list",
];

/// Public shape of the permission catalog. Keep this derived from the
/// canonical operation and location tables so the GUI cannot invent scopes.
pub(crate) fn authorization_capabilities(db: &Connection, actor: &str) -> Result<Value, Fault> {
    let mut operations = Vec::with_capacity(OPERATIONS.len());
    for operation in OPERATIONS {
        let mut scopes = vec!["global"];
        if *operation == "read" || LOCATION_OPERATIONS.contains(operation) {
            scopes.extend(["board", "thread"]);
        }
        operations.push(json!({"operation":operation,"scopes":scopes}));
    }
    Ok(json!({
        "role_create":can(db,actor,"role.create",None,None)?,
        "role_list":can(db,actor,"role.list",None,None)?,
        "role_assign":can(db,actor,"role.assign",None,None)?,
        "permission_set":can(db,actor,"permission.set",None,None)?,
        "permission_list":can(db,actor,"permission.list",None,None)?,
        "operations":operations
    }))
}

fn invalid(message: &str) -> Fault {
    Fault::new("INVALID_INPUT", 2, message)
}

fn missing() -> Fault {
    Fault::new("NOT_FOUND", 5, "The requested item is not available")
}

fn denied() -> Fault {
    Fault::new("PERMISSION_DENIED", 4, "This operation is not permitted")
}

fn value_text<'a>(args: &'a Value, key: &str) -> Result<&'a str, Fault> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| invalid(&format!("A nonblank {key} is required")))
}

fn exact_fields(args: &Value, fields: &[&str]) -> Result<(), Fault> {
    let object = args
        .as_object()
        .ok_or_else(|| invalid("Arguments must be an object"))?;
    if object.keys().any(|key| !fields.contains(&key.as_str())) {
        return Err(invalid("Unsupported argument"));
    }
    Ok(())
}

fn account_roles(db: &Connection, actor: &str) -> Result<Vec<String>, Fault> {
    let mut statement = db.prepare("SELECT role_id FROM role_members WHERE account_id=?1")?;
    let roles = statement
        .query_map([actor], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(roles)
}

/// Stable digest of this account's current effective-grant inputs. It binds
/// change cursors to role membership and explicit permission edits without
/// including password or session credential material.
pub(crate) fn access_fingerprint(db: &Connection, actor: &str) -> Result<String, Fault> {
    let account: Option<(bool, bool)> = db
        .query_row(
            "SELECT is_admin,disabled FROM accounts WHERE id=?1",
            [actor],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let account = account.ok_or_else(missing)?;
    let mut roles = account_roles(db, actor)?;
    roles.sort();
    let account_permissions = {
        let mut query = db.prepare(
            "SELECT operation,scope,target_id,decision FROM account_permissions \
             WHERE account_id=?1 ORDER BY operation,scope,target_id,decision",
        )?;
        query
            .query_map([actor], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut role_permissions = Vec::new();
    for role in &roles {
        let mut query = db.prepare(
            "SELECT operation,scope,target_id,decision FROM role_permissions \
             WHERE role_id=?1 ORDER BY operation,scope,target_id,decision",
        )?;
        let rows = query
            .query_map([role], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        role_permissions.push((role, rows));
    }
    let encoded = serde_json::to_vec(&json!({
        "account":account,
        "roles":roles,
        "account_permissions":account_permissions,
        "role_permissions":role_permissions
    }))
    .map_err(|_| Fault::internal())?;
    Ok(format!("{:x}", Sha256::digest(encoded)))
}

fn subject_decision(
    db: &Connection,
    actor: &str,
    roles: &[String],
    operation: &str,
    scope: &str,
    target_id: &str,
) -> Result<Option<bool>, Fault> {
    let mut allowed = false;
    let mut denied = false;
    let account: Option<String> = db
        .query_row(
            "SELECT decision FROM account_permissions \
             WHERE account_id=?1 AND operation=?2 AND scope=?3 AND target_id=?4",
            params![actor, operation, scope, target_id],
            |row| row.get(0),
        )
        .optional()?;
    apply_decision(account.as_deref(), &mut allowed, &mut denied)?;

    for role in roles {
        let decision: Option<String> = db
            .query_row(
                "SELECT decision FROM role_permissions \
                 WHERE role_id=?1 AND operation=?2 AND scope=?3 AND target_id=?4",
                params![role, operation, scope, target_id],
                |row| row.get(0),
            )
            .optional()?;
        apply_decision(decision.as_deref(), &mut allowed, &mut denied)?;
    }

    Ok(if denied {
        Some(false)
    } else if allowed {
        Some(true)
    } else {
        None
    })
}

fn apply_decision(
    decision: Option<&str>,
    allowed: &mut bool,
    denied: &mut bool,
) -> Result<(), Fault> {
    match decision {
        None => {}
        Some("allow") => *allowed = true,
        Some("deny") => *denied = true,
        _ => return Err(Fault::internal()),
    }
    Ok(())
}

fn effective_decision(
    db: &Connection,
    actor: &str,
    roles: &[String],
    operation: &str,
    board: Option<&str>,
    thread: Option<&str>,
    default: bool,
) -> Result<bool, Fault> {
    if let Some(id) = thread
        && let Some(decision) = subject_decision(db, actor, roles, operation, "thread", id)?
    {
        return Ok(decision);
    }
    if let Some(id) = board
        && let Some(decision) = subject_decision(db, actor, roles, operation, "board", id)?
    {
        return Ok(decision);
    }
    Ok(subject_decision(db, actor, roles, operation, "global", "")?.unwrap_or(default))
}

/// Evaluate a human account's location-read and operation permissions.
/// Thread settings override board settings; within each tier, any role or
/// account denial overrides every allow at that tier.
pub(crate) fn can(
    db: &Connection,
    actor: &str,
    operation: &str,
    board: Option<&str>,
    thread: Option<&str>,
) -> Result<bool, Fault> {
    let account: Option<(bool, bool)> = db
        .query_row(
            "SELECT is_admin,disabled FROM accounts WHERE id=?1",
            [actor],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((is_admin, disabled)) = account else {
        return Err(missing());
    };
    if is_admin {
        return Ok(true);
    }
    if disabled {
        return Ok(false);
    }

    let mut owned_board = None;
    if let Some(thread_id) = thread {
        let stored_board: Option<String> = db
            .query_row(
                "SELECT board_id FROM threads WHERE id=?1",
                [thread_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(stored_board) = stored_board else {
            return Ok(false);
        };
        if board.is_some_and(|id| id != stored_board) {
            return Ok(false);
        }
        owned_board = Some(stored_board);
    }

    let roles = account_roles(db, actor)?;
    let board = owned_board.as_deref().or(board);
    if operation == "read" {
        return effective_decision(db, actor, &roles, "read", board, thread, true);
    }
    if !OPERATIONS.contains(&operation) {
        return Ok(false);
    }
    if LOCATION_OPERATIONS.contains(&operation)
        && !effective_decision(db, actor, &roles, "read", board, thread, true)?
    {
        return Ok(false);
    }

    let default = DEFAULT_ALLOW.contains(&operation);
    effective_decision(db, actor, &roles, operation, board, thread, default)
}

pub(crate) fn require(
    db: &Connection,
    actor: &str,
    operation: &str,
    board: Option<&str>,
    thread: Option<&str>,
) -> Result<(), Fault> {
    if can(db, actor, operation, board, thread)? {
        Ok(())
    } else {
        Err(denied())
    }
}

fn existing_account(db: &Connection, id: &str) -> Result<(), Fault> {
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1)",
        [id],
        |row| row.get(0),
    )?;
    if exists { Ok(()) } else { Err(missing()) }
}

fn existing_role(db: &Connection, id: &str) -> Result<(), Fault> {
    let exists: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM roles WHERE id=?1)",
        [id],
        |row| row.get(0),
    )?;
    if exists { Ok(()) } else { Err(missing()) }
}

fn validate_target(db: &Connection, scope: &str, target_id: &str) -> Result<(), Fault> {
    match scope {
        "global" if target_id.is_empty() => Ok(()),
        "global" => Err(invalid("Global permissions require an empty target_id")),
        "board" => {
            if target_id.trim().is_empty() {
                return Err(invalid("A board target_id is required"));
            }
            let exists: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM boards WHERE id=?1)",
                [target_id],
                |row| row.get(0),
            )?;
            if exists { Ok(()) } else { Err(missing()) }
        }
        "thread" => {
            if target_id.trim().is_empty() {
                return Err(invalid("A thread target_id is required"));
            }
            let exists: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM threads t JOIN boards b ON b.id=t.board_id WHERE t.id=?1)",
                [target_id],
                |row| row.get(0),
            )?;
            if exists { Ok(()) } else { Err(missing()) }
        }
        _ => Err(invalid("Scope must be global, board, or thread")),
    }
}

pub(crate) fn dispatch(
    db: &Connection,
    operation: &str,
    args: &Value,
    _actor: &str,
) -> Result<Value, Fault> {
    match operation {
        "role.create" => {
            exact_fields(args, &["name"])?;
            let name = value_text(args, "name")?.trim();
            let exists: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM roles WHERE name=?1)",
                [name],
                |row| row.get(0),
            )?;
            if exists {
                return Err(Fault::new(
                    "CONFLICT",
                    6,
                    "A role with this name already exists",
                ));
            }
            let id = Uuid::new_v4().to_string();
            db.execute(
                "INSERT INTO roles(id,name,created_at) VALUES(?1,?2,?3)",
                params![id, name, crate::service::timestamp()],
            )?;
            Ok(json!({"id":id,"name":name}))
        }
        "role.list" => {
            exact_fields(args, &[])?;
            let mut statement =
                db.prepare("SELECT id,name,created_at FROM roles ORDER BY name,id")?;
            let items = statement
                .query_map([], |row| {
                    Ok(json!({
                        "id":row.get::<_,String>(0)?,
                        "name":row.get::<_,String>(1)?,
                        "created_at":row.get::<_,String>(2)?
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(json!({"items":items}))
        }
        "role.assign" => {
            exact_fields(args, &["account_id", "role_id", "assigned"])?;
            let account_id = value_text(args, "account_id")?;
            let role_id = value_text(args, "role_id")?;
            let assigned = args
                .get("assigned")
                .and_then(Value::as_bool)
                .ok_or_else(|| invalid("assigned must be a boolean"))?;
            existing_account(db, account_id)?;
            existing_role(db, role_id)?;
            if assigned {
                db.execute(
                    "INSERT OR IGNORE INTO role_members(role_id,account_id) VALUES(?1,?2)",
                    params![role_id, account_id],
                )?;
            } else {
                db.execute(
                    "DELETE FROM role_members WHERE role_id=?1 AND account_id=?2",
                    params![role_id, account_id],
                )?;
            }
            Ok(json!({"account_id":account_id,"role_id":role_id,"assigned":assigned}))
        }
        "permission.set" => {
            exact_fields(
                args,
                &[
                    "subject_type",
                    "subject_id",
                    "operation",
                    "scope",
                    "target_id",
                    "decision",
                ],
            )?;
            let subject_type = value_text(args, "subject_type")?;
            let subject_id = value_text(args, "subject_id")?;
            let permission = value_text(args, "operation")?;
            let scope = value_text(args, "scope")?;
            let target_id = args
                .get("target_id")
                .and_then(Value::as_str)
                .ok_or_else(|| invalid("target_id must be text"))?;
            let decision = value_text(args, "decision")?;
            if !OPERATIONS.contains(&permission) {
                return Err(invalid("Operation is not supported for permissions"));
            }
            if permission == "retention.manage" && scope != "global" {
                return Err(invalid("Retention permission must use global scope"));
            }
            match subject_type {
                "account" => existing_account(db, subject_id)?,
                "role" => existing_role(db, subject_id)?,
                _ => return Err(invalid("subject_type must be account or role")),
            }
            validate_target(db, scope, target_id)?;
            if !matches!(decision, "allow" | "deny" | "inherit") {
                return Err(invalid("decision must be allow, deny, or inherit"));
            }
            let table = match subject_type {
                "account" => "account_permissions",
                "role" => "role_permissions",
                _ => unreachable!(),
            };
            if decision == "inherit" {
                let sql = format!(
                    "DELETE FROM {table} WHERE {}=?1 AND operation=?2 AND scope=?3 AND target_id=?4",
                    if subject_type == "account" {
                        "account_id"
                    } else {
                        "role_id"
                    }
                );
                db.execute(&sql, params![subject_id, permission, scope, target_id])?;
            } else {
                let (column, subject_table) = if subject_type == "account" {
                    ("account_id", "account_permissions")
                } else {
                    ("role_id", "role_permissions")
                };
                let sql = format!(
                    "INSERT INTO {subject_table}({column},operation,scope,target_id,decision) \
                     VALUES(?1,?2,?3,?4,?5) ON CONFLICT({column},operation,scope,target_id) \
                     DO UPDATE SET decision=excluded.decision"
                );
                db.execute(
                    &sql,
                    params![subject_id, permission, scope, target_id, decision],
                )?;
            }
            Ok(json!({
                "subject_type":subject_type,
                "subject_id":subject_id,
                "operation":permission,
                "scope":scope,
                "target_id":target_id,
                "decision":decision
            }))
        }
        "permission.list" => {
            exact_fields(args, &["subject_type", "subject_id"])?;
            let subject_type = value_text(args, "subject_type")?;
            let subject_id = value_text(args, "subject_id")?;
            let (table, column) = match subject_type {
                "account" => {
                    existing_account(db, subject_id)?;
                    ("account_permissions", "account_id")
                }
                "role" => {
                    existing_role(db, subject_id)?;
                    ("role_permissions", "role_id")
                }
                _ => return Err(invalid("subject_type must be account or role")),
            };
            let sql = format!(
                "SELECT operation,scope,target_id,decision FROM {table} WHERE {column}=?1 \
                 ORDER BY operation,scope,target_id"
            );
            let mut statement = db.prepare(&sql)?;
            let items = statement
                .query_map([subject_id], |row| {
                    Ok(json!({
                        "operation":row.get::<_,String>(0)?,
                        "scope":row.get::<_,String>(1)?,
                        "target_id":row.get::<_,String>(2)?,
                        "decision":row.get::<_,String>(3)?
                    }))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(json!({"subject_type":subject_type,"subject_id":subject_id,"items":items}))
        }
        _ => Err(denied()),
    }
}
