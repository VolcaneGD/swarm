//! Shared physical deletion owner for human purge and retention maintenance.
use crate::{changes, protocol::Fault};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConversationKind {
    Board,
    Thread,
}

const RECLAIM_PENDING: &str = "storage.reclaim_pending";

pub(crate) fn reclaim_pending(db: &Connection) -> Result<bool, Fault> {
    Ok(db
        .query_row(
            "SELECT 1 FROM app_metadata WHERE key=?1",
            [RECLAIM_PENDING],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

pub(crate) fn mark_reclaim_pending(db: &Connection) -> Result<(), Fault> {
    db.execute(
        "INSERT OR REPLACE INTO app_metadata(key,value) VALUES(?1,'1')",
        [RECLAIM_PENDING],
    )?;
    Ok(())
}

impl ConversationKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Board => "board",
            Self::Thread => "thread",
        }
    }
}

fn stale() -> Fault {
    Fault::new(
        "STALE_SELECTION",
        6,
        "The deleted conversation changed; refresh and try again",
    )
}

pub(crate) fn deleted(db: &Connection, kind: ConversationKind, id: &str) -> Result<bool, Fault> {
    let query = match kind {
        ConversationKind::Board => "SELECT deleted_at IS NOT NULL FROM boards WHERE id=?1",
        ConversationKind::Thread => "SELECT deleted_at IS NOT NULL FROM threads WHERE id=?1",
    };
    Ok(db
        .query_row(query, [id], |row| row.get::<_, bool>(0))
        .optional()?
        .unwrap_or(false))
}

pub(crate) fn parent_board_deleted(db: &Connection, thread: &str) -> Result<bool, Fault> {
    Ok(db.query_row(
        "SELECT b.deleted_at IS NOT NULL FROM threads t JOIN boards b ON b.id=t.board_id WHERE t.id=?1",
        [thread],
        |row| row.get(0),
    ).optional()?.unwrap_or(false))
}

/// Permanently remove a deleted Board or Thread and all conversation-owned
/// rows. The caller owns the transaction and performs authorization.
pub(crate) fn purge_conversation(
    db: &Connection,
    kind: ConversationKind,
    id: &str,
    allow_archived: bool,
) -> Result<Value, Fault> {
    let archived_eligible = allow_archived
        && kind == ConversationKind::Thread
        && db.query_row(
            "SELECT EXISTS(SELECT 1 FROM threads t JOIN boards b ON b.id=t.board_id \
             WHERE t.id=?1 AND t.archived_at IS NOT NULL AND t.deleted_at IS NULL AND b.deleted_at IS NULL)",
            [id],
            |row| row.get::<_, bool>(0),
        )?;
    if !deleted(db, kind, id)? && !archived_eligible {
        return Err(stale());
    }
    let (thread_filter, filter_arg) = match kind {
        ConversationKind::Board => ("SELECT id FROM threads WHERE board_id=?1", id),
        ConversationKind::Thread => ("SELECT id FROM threads WHERE id=?1", id),
    };
    let threads = {
        let sql = format!("{thread_filter} ORDER BY id");
        let mut statement = db.prepare(&sql)?;
        statement
            .query_map([filter_arg], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    let posts = {
        let sql = format!("SELECT id FROM posts WHERE thread_id IN ({thread_filter}) ORDER BY id");
        let mut statement = db.prepare(&sql)?;
        statement
            .query_map([filter_arg], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    let revisions: i64 = db.query_row(
        &format!("SELECT count(*) FROM post_revisions WHERE post_id IN (SELECT id FROM posts WHERE thread_id IN ({thread_filter}))"),
        [filter_arg],
        |row| row.get(0),
    )?;

    // Remove rows with restrictive foreign keys before deleting Posts. Bulk
    // deletion keeps reply-to references valid until the statement completes,
    // including arbitrary nested reply depth.
    db.execute(
        &format!("DELETE FROM notification_outbox WHERE thread_id IN ({thread_filter})"),
        [filter_arg],
    )?;
    db.execute(
        &format!("DELETE FROM thread_pins WHERE thread_id IN ({thread_filter})"),
        [filter_arg],
    )?;
    db.execute(
        &format!("DELETE FROM thread_subscriptions WHERE thread_id IN ({thread_filter})"),
        [filter_arg],
    )?;
    db.execute(
        &format!("DELETE FROM notification_post_reads WHERE post_id IN (SELECT id FROM posts WHERE thread_id IN ({thread_filter}))"),
        [filter_arg],
    )?;
    db.execute(
        &format!("DELETE FROM post_revisions WHERE post_id IN (SELECT id FROM posts WHERE thread_id IN ({thread_filter}))"),
        [filter_arg],
    )?;
    db.execute(
        &format!("DELETE FROM app_metadata WHERE key IN (SELECT 'post-harness-v1:' || id FROM posts WHERE thread_id IN ({thread_filter}))"),
        [filter_arg],
    )?;
    for thread in &threads {
        let suffix = format!(":thread_scroll:{thread}");
        db.execute(
            "DELETE FROM app_metadata WHERE key LIKE 'ui-preference-v1:%' AND substr(key,-length(?1))=?1",
            [suffix],
        )?;
    }

    // Remove change history, but keep body-free creation receipts until their
    // existing seven-day expiry. A delayed retry must find a missing entity,
    // rather than recreate a deliberately purged Board/Thread with a new id.
    let mut entity_ids = Vec::with_capacity(1 + threads.len() + posts.len());
    entity_ids.push(id.to_owned());
    entity_ids.extend(threads.iter().cloned());
    entity_ids.extend(posts.iter().cloned());
    for entity in &entity_ids {
        db.execute("DELETE FROM changes WHERE entity_id=?1", [entity])?;
    }
    changes::invalidate_in_transaction(db)?;

    match kind {
        ConversationKind::Board => {
            db.execute(
                "DELETE FROM posts WHERE thread_id IN (SELECT id FROM threads WHERE board_id=?1)",
                [id],
            )?;
            db.execute("DELETE FROM threads WHERE board_id=?1", [id])?;
            db.execute("DELETE FROM boards WHERE id=?1", [id])?;
        }
        ConversationKind::Thread => {
            db.execute("DELETE FROM posts WHERE thread_id=?1", [id])?;
            db.execute("DELETE FROM threads WHERE id=?1", [id])?;
        }
    }
    mark_reclaim_pending(db)?;
    Ok(json!({
        "kind":kind.as_str(),"id":id,"removed_threads":threads.len(),
        "removed_posts":posts.len(),"removed_revisions":revisions
    }))
}
