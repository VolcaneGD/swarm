use crate::{
    human_routes,
    protocol::{Fault, Request},
    service,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationKind {
    Post,
    Response,
}

#[derive(Clone, PartialEq, Eq)]
pub struct NotificationDelivery {
    pub id: String,
    pub account_id: String,
    pub thread_id: String,
    pub post_id: String,
    pub kind: NotificationKind,
    pub excerpt: Option<String>,
}

impl std::fmt::Debug for NotificationDelivery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificationDelivery")
            .field("id", &self.id)
            .field("account_id", &self.account_id)
            .field("thread_id", &self.thread_id)
            .field("post_id", &self.post_id)
            .field("kind", &self.kind)
            .field("excerpt", &self.excerpt.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

fn human_candidate(
    candidate: &crate::credentials::CredentialCandidate,
) -> Result<(&str, &str), Fault> {
    match candidate {
        crate::credentials::CredentialCandidate::Human { id, token, .. }
            if !id.trim().is_empty()
                && !token.trim().is_empty()
                && id.len() <= 4096
                && token.len() <= 4096 =>
        {
            Ok((id, token))
        }
        _ => Err(Fault::new(
            "PERMISSION_DENIED",
            4,
            "Windows notifications require a valid selected human session",
        )),
    }
}

fn require_live_human_session(
    db: &Connection,
    account_id: &str,
    token_hash: &str,
) -> Result<(), Fault> {
    let valid: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM human_sessions s JOIN accounts a ON a.id=s.account_id \
         WHERE s.account_id=?1 AND s.token_hash=?2 AND a.disabled=0)",
        params![account_id, token_hash],
        |row| row.get(0),
    )?;
    if valid {
        Ok(())
    } else {
        Err(Fault::authentication())
    }
}

type Selection = (String, String, String);

fn selection(db: &Connection) -> Result<Option<Selection>, Fault> {
    Ok(db
        .query_row(
            "SELECT account_id,token_hash,generation FROM notification_selection WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?)
}

pub(crate) fn require_selected(
    db: &Connection,
    candidate: &crate::credentials::CredentialCandidate,
) -> Result<Selection, Fault> {
    let (id, token) = human_candidate(candidate)?;
    let token_hash = crate::accounts::digest(token);
    let Some(selected) = selection(db)? else {
        return Err(Fault::new(
            "NOT_FOUND",
            5,
            "No notification recipient is selected",
        ));
    };
    if selected.0 != id || selected.1 != token_hash {
        return Err(Fault::new(
            "PERMISSION_DENIED",
            4,
            "The notification recipient selection has changed",
        ));
    }
    require_live_human_session(db, &selected.0, &selected.1)?;
    Ok(selected)
}

pub(crate) fn select_recipient(
    db: &mut Connection,
    candidate: &crate::credentials::CredentialCandidate,
) -> Result<(), Fault> {
    let (id, token) = human_candidate(candidate)?;
    let token_hash = crate::accounts::digest(token);
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    require_live_human_session(&tx, id, &token_hash)?;
    if let Some((old_id, old_token, _)) = selection(&tx)?
        && old_id == id
        && old_token == token_hash
    {
        tx.commit()?;
        return Ok(());
    }
    let generation = uuid::Uuid::new_v4().to_string();
    tx.execute("DELETE FROM notification_outbox", [])?;
    tx.execute(
        "DELETE FROM app_metadata WHERE key='notification-batch-v1'",
        [],
    )?;
    tx.execute(
        "INSERT INTO notification_selection(singleton,account_id,token_hash,generation) \
         VALUES(1,?1,?2,?3) ON CONFLICT(singleton) DO UPDATE SET \
         account_id=excluded.account_id,token_hash=excluded.token_hash,generation=excluded.generation",
        params![id, token_hash, generation],
    )?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn clear_recipient(db: &mut Connection) -> Result<(), Fault> {
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    tx.execute("DELETE FROM notification_outbox", [])?;
    tx.execute(
        "DELETE FROM app_metadata WHERE key='notification-batch-v1'",
        [],
    )?;
    tx.execute("DELETE FROM notification_selection", [])?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn enqueue_created_post(db: &Connection, post_id: &str) -> Result<(), Fault> {
    let Some((account_id, token_hash, generation)) = selection(db)? else {
        return Ok(());
    };
    match require_live_human_session(db, &account_id, &token_hash) {
        Ok(()) => {}
        Err(error) if error.code == "AUTHENTICATION_REQUIRED" => return Ok(()),
        Err(error) => return Err(error),
    }
    let (thread_id, board_id, reply_to, live): (String, String, Option<i64>, bool) = db.query_row(
        "SELECT p.thread_id,t.board_id,p.reply_to,p.deleted_at IS NULL AND p.body_purged_at IS NULL AND \
         t.deleted_at IS NULL AND b.deleted_at IS NULL FROM posts p \
         JOIN threads t ON t.id=p.thread_id JOIN boards b ON b.id=t.board_id WHERE p.id=?1",
        [post_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    if !live {
        return Ok(());
    }
    if !human_routes::can_read_thread(db, &account_id, &board_id, &thread_id, true)? {
        return Ok(());
    }
    let enabled: bool = db.query_row(
        "SELECT COALESCE((SELECT enabled FROM notification_settings WHERE account_id=?1),1)",
        [&account_id],
        |row| row.get(0),
    )?;
    if !enabled {
        return Ok(());
    }
    let direct_reply = if let Some(reply_no) = reply_to {
        db.query_row(
            "SELECT EXISTS(SELECT 1 FROM posts WHERE thread_id=?1 AND no=?2 \
             AND author_id=?3 AND deleted_at IS NULL)",
            params![thread_id, reply_no, account_id],
            |row| row.get::<_, bool>(0),
        )?
    } else {
        false
    };
    let subscribed: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM thread_subscriptions WHERE account_id=?1 AND thread_id=?2)",
        params![account_id, thread_id],
        |row| row.get(0),
    )?;
    if !direct_reply && !subscribed {
        return Ok(());
    }
    let delivery_id = format!(
        "notification:{}",
        crate::accounts::digest(&format!("{generation}\0{account_id}\0{post_id}"))
    );
    db.execute(
        "INSERT OR IGNORE INTO notification_outbox(delivery_id,generation,account_id,thread_id,post_id,kind,created_at) \
         VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![delivery_id, generation, account_id, thread_id, post_id,
            if reply_to.is_some() { "response" } else { "post" }, service::timestamp()],
    )?;
    Ok(())
}

pub(crate) fn prepare_deliveries(
    db: &mut Connection,
    candidate: &crate::credentials::CredentialCandidate,
    limit: usize,
) -> Result<Vec<NotificationDelivery>, Fault> {
    if !(1..=200).contains(&limit) {
        return Err(invalid("limit must be between 1 and 200"));
    }
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let deliveries = prepare_in_transaction(&tx, candidate, limit)?;
    tx.commit()?;
    Ok(deliveries)
}

pub(crate) fn prepare_in_transaction(
    tx: &Connection,
    candidate: &crate::credentials::CredentialCandidate,
    limit: usize,
) -> Result<Vec<NotificationDelivery>, Fault> {
    prepare_filtered(tx, candidate, limit, None, None, None)
}

pub(crate) fn prepare_filtered(
    tx: &Connection,
    candidate: &crate::credentials::CredentialCandidate,
    limit: usize,
    members: Option<&[String]>,
    cutoff_ms: Option<i64>,
    thread_id: Option<&str>,
) -> Result<Vec<NotificationDelivery>, Fault> {
    let members = members
        .map(serde_json::to_string)
        .transpose()
        .map_err(|_| Fault::internal())?;
    let cutoff = cutoff_ms
        .map(|ms| {
            chrono::DateTime::from_timestamp_millis(ms)
                .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
                .ok_or_else(Fault::internal)
        })
        .transpose()?;
    let (account_id, _token_hash, generation) = require_selected(tx, candidate)?;
    let enabled: bool = tx.query_row(
        "SELECT COALESCE((SELECT enabled FROM notification_settings WHERE account_id=?1),1)",
        [&account_id],
        |row| row.get(0),
    )?;
    let include_excerpt: bool = tx.query_row(
        "SELECT COALESCE((SELECT include_excerpt FROM notification_settings WHERE account_id=?1),0)",
        [&account_id],
        |row| row.get(0),
    )?;
    let rows = {
        let mut query = tx.prepare(
            "SELECT o.delivery_id,o.thread_id,o.post_id,o.kind, \
             CASE WHEN ?3=1 THEN substr(p.body,1,241) ELSE '' END,p.reply_to, \
             t.board_id,p.deleted_at IS NULL AND p.body_purged_at IS NULL,t.deleted_at IS NULL,b.deleted_at IS NULL \
             FROM notification_outbox o JOIN posts p ON p.id=o.post_id \
             JOIN threads t ON t.id=p.thread_id JOIN boards b ON b.id=t.board_id \
             WHERE o.generation=?1 AND o.account_id=?2 \
             AND (?4 IS NULL OR o.delivery_id IN (SELECT value FROM json_each(?4))) \
             AND (?5 IS NULL OR o.created_at<=?5) \
             AND (?6 IS NULL OR o.thread_id=?6) ORDER BY o.created_at,o.delivery_id LIMIT 200",
        )?;
        query
            .query_map(
                params![
                    generation,
                    account_id,
                    include_excerpt && enabled,
                    members,
                    cutoff,
                    thread_id
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<i64>>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, bool>(7)?,
                        row.get::<_, bool>(8)?,
                        row.get::<_, bool>(9)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut deliveries = Vec::new();
    for (
        id,
        thread_id,
        post_id,
        kind,
        body,
        reply_to,
        board_id,
        post_live,
        thread_live,
        board_live,
    ) in rows
    {
        let subscribed: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM thread_subscriptions WHERE account_id=?1 AND thread_id=?2)",
            params![account_id,thread_id], |row| row.get(0))?;
        let direct_reply = if let Some(no) = reply_to {
            tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM posts p WHERE p.thread_id=?1 AND p.no=?2 \
                AND p.author_id=?3 AND p.deleted_at IS NULL AND p.body_purged_at IS NULL)",
                params![thread_id, no, account_id],
                |row| row.get::<_, bool>(0),
            )?
        } else {
            false
        };
        let is_read: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM notification_post_reads WHERE account_id=?1 AND post_id=?2)",
            params![account_id,post_id], |row| row.get(0))?;
        let can_read = if thread_live && board_live {
            human_routes::can_read_thread(tx, &account_id, &board_id, &thread_id, true)?
        } else {
            false
        };
        if !enabled
            || !post_live
            || !thread_live
            || !board_live
            || !can_read
            || is_read
            || (!subscribed && !direct_reply)
        {
            tx.execute(
                "DELETE FROM notification_outbox WHERE delivery_id=?1",
                [&id],
            )?;
            continue;
        }
        if deliveries.len() >= limit {
            continue;
        }
        let excerpt = include_excerpt.then(|| bounded_excerpt(&body));
        deliveries.push(NotificationDelivery {
            id,
            account_id: account_id.clone(),
            thread_id,
            post_id,
            kind: if kind == "response" {
                NotificationKind::Response
            } else {
                NotificationKind::Post
            },
            excerpt,
        });
    }
    Ok(deliveries)
}

fn bounded_excerpt(body: &str) -> String {
    let mut chars = body.chars();
    let excerpt: String = chars.by_ref().take(240).collect();
    if chars.next().is_some() {
        format!("{excerpt}…")
    } else {
        excerpt
    }
}

pub(crate) fn acknowledge(
    db: &mut Connection,
    candidate: &crate::credentials::CredentialCandidate,
    delivery_id: &str,
) -> Result<(), Fault> {
    if delivery_id.is_empty() || delivery_id.len() > 128 {
        return Err(invalid("delivery_id is invalid"));
    }
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (account_id, _, generation) = require_selected(&tx, candidate)?;
    tx.execute(
        "DELETE FROM notification_outbox WHERE delivery_id=?1 AND account_id=?2 AND generation=?3",
        params![delivery_id, account_id, generation],
    )?;
    tx.commit()?;
    Ok(())
}

fn invalid(message: &str) -> Fault {
    Fault::new("INVALID_INPUT", 2, message)
}

fn missing() -> Fault {
    Fault::new("NOT_FOUND", 5, "The requested item is not available")
}

fn text<'a>(args: &'a Value, field: &str) -> Result<&'a str, Fault> {
    let text = args
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| invalid("A non-empty string is required"))?;
    if text.len() > 4096 {
        return Err(invalid("Identifier must not exceed 4096 bytes"));
    }
    Ok(text)
}

fn live_thread_board(db: &Connection, id: &str) -> Result<String, Fault> {
    db.query_row(
        "SELECT t.board_id FROM threads t JOIN boards b ON b.id=t.board_id \
         WHERE t.id=?1 AND t.deleted_at IS NULL AND b.deleted_at IS NULL",
        [id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(missing)
}

fn readable_thread(db: &Connection, actor: &str, id: &str) -> Result<String, Fault> {
    let board = live_thread_board(db, id)?;
    if !human_routes::can_read_thread(db, actor, &board, id, true)? {
        return Err(missing());
    }
    Ok(board)
}

fn subscriptions(db: &Connection, actor: &str) -> Result<Value, Fault> {
    let mut statement = db.prepare(
        "SELECT s.thread_id,t.board_id FROM thread_subscriptions s \
         JOIN threads t ON t.id=s.thread_id JOIN boards b ON b.id=t.board_id \
         WHERE s.account_id=?1 AND t.deleted_at IS NULL AND b.deleted_at IS NULL \
         ORDER BY s.created_at,s.thread_id",
    )?;
    let rows = statement
        .query_map([actor], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut items = Vec::new();
    for (id, board) in rows {
        if human_routes::can_read_thread(db, actor, &board, &id, true)? {
            items.push(json!({"id":id}));
        }
    }
    Ok(json!({"items":items}))
}

fn subscribe(db: &Connection, actor: &str, args: &Value) -> Result<Value, Fault> {
    let id = text(args, "id")?;
    let subscribed = args
        .get("subscribed")
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid("subscribed must be a boolean"))?;
    readable_thread(db, actor, id)?;
    if subscribed {
        db.execute(
            "INSERT OR IGNORE INTO thread_subscriptions(account_id,thread_id,created_at) \
             VALUES(?1,?2,?3)",
            params![actor, id, service::timestamp()],
        )?;
    } else {
        db.execute(
            "DELETE FROM thread_subscriptions WHERE account_id=?1 AND thread_id=?2",
            params![actor, id],
        )?;
    }
    Ok(json!({"id":id,"subscribed":subscribed}))
}

fn unread(db: &Connection, actor: &str) -> Result<Value, Fault> {
    let mut statement = db.prepare(
        "SELECT t.id,t.board_id FROM threads t JOIN boards b ON b.id=t.board_id \
         WHERE t.deleted_at IS NULL AND b.deleted_at IS NULL ORDER BY t.created_at,t.id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut items = Vec::new();
    let mut total = 0_i64;
    for (thread_id, board) in rows {
        if !human_routes::can_read_thread(db, actor, &board, &thread_id, true)? {
            continue;
        }
        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM posts p WHERE p.thread_id=?1 AND p.deleted_at IS NULL AND p.body_purged_at IS NULL \
             AND NOT EXISTS(SELECT 1 FROM notification_post_reads r \
                            WHERE r.account_id=?2 AND r.post_id=p.id)",
            params![thread_id, actor],
            |row| row.get(0),
        )?;
        total += count;
        items.push(json!({"thread_id":thread_id,"unread_count":count}));
    }
    Ok(json!({"items":items,"total":total}))
}

fn mark_read(db: &Connection, actor: &str, args: &Value) -> Result<Value, Fault> {
    let thread_id = text(args, "thread_id")?;
    let input = args
        .get("post_ids")
        .and_then(Value::as_array)
        .filter(|ids| !ids.is_empty() && ids.len() <= 200)
        .ok_or_else(|| invalid("post_ids must contain between 1 and 200 IDs"))?;
    let mut unique = BTreeSet::new();
    for value in input {
        let id = value
            .as_str()
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| invalid("Each post ID must be a non-empty string"))?;
        if id.len() > 4096 {
            return Err(invalid("Post IDs must not exceed 4096 bytes"));
        }
        unique.insert(id.to_owned());
    }
    let board = readable_thread(db, actor, thread_id)?;
    // Validate the complete set before inserting anything so an invalid ID cannot
    // partially advance read state. The surrounding service transaction is atomic.
    for id in &unique {
        let valid: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM posts p JOIN threads t ON t.id=p.thread_id \
             JOIN boards b ON b.id=t.board_id WHERE p.id=?1 AND p.thread_id=?2 \
             AND p.deleted_at IS NULL AND p.body_purged_at IS NULL AND t.deleted_at IS NULL AND b.deleted_at IS NULL)",
            params![id, thread_id],
            |row| row.get(0),
        )?;
        if !valid {
            return Err(missing());
        }
    }
    // Re-evaluate visibility after the set is validated at the operation boundary.
    if !human_routes::can_read_thread(db, actor, &board, thread_id, true)? {
        return Err(missing());
    }
    let read_at = service::timestamp();
    for id in &unique {
        db.execute(
            "INSERT OR IGNORE INTO notification_post_reads(account_id,post_id,read_at) \
             VALUES(?1,?2,?3)",
            params![actor, id, read_at],
        )?;
    }
    Ok(json!({"marked":unique.len()}))
}

fn settings(db: &Connection, actor: &str) -> Result<Value, Fault> {
    let row: Option<(bool, bool)> = db
        .query_row(
            "SELECT enabled,include_excerpt FROM notification_settings WHERE account_id=?1",
            [actor],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let (enabled, include_excerpt) = row.unwrap_or((true, false));
    Ok(json!({"enabled":enabled,"include_excerpt":include_excerpt}))
}

fn set_settings(db: &Connection, actor: &str, args: &Value) -> Result<Value, Fault> {
    let enabled = args
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid("enabled must be a boolean"))?;
    let include_excerpt = args
        .get("include_excerpt")
        .and_then(Value::as_bool)
        .ok_or_else(|| invalid("include_excerpt must be a boolean"))?;
    db.execute(
        "INSERT INTO notification_settings(account_id,enabled,include_excerpt) VALUES(?1,?2,?3) \
         ON CONFLICT(account_id) DO UPDATE SET enabled=excluded.enabled, \
         include_excerpt=excluded.include_excerpt",
        params![actor, enabled, include_excerpt],
    )?;
    Ok(json!({"enabled":enabled,"include_excerpt":include_excerpt}))
}

pub(crate) fn dispatch(db: &Connection, request: &Request, actor: &str) -> Result<Value, Fault> {
    match request.operation.as_str() {
        "conversation.subscriptions" => subscriptions(db, actor),
        "thread.subscribe" => subscribe(db, actor, &request.args),
        "conversation.unread" => unread(db, actor),
        "post.read.mark" => mark_read(db, actor, &request.args),
        "notification.settings.get" => settings(db, actor),
        "notification.settings.set" => set_settings(db, actor, &request.args),
        _ => Err(Fault::new(
            "UNKNOWN_OPERATION",
            2,
            "Operation is not supported",
        )),
    }
}
