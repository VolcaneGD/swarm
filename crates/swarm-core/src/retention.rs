use crate::{human, protocol::Fault};
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};

const RETENTION_PERMISSION: &str = "retention.manage";

fn invalid(message: &str) -> Fault {
    Fault::new("INVALID_INPUT", 2, message)
}

fn settings_state() -> Fault {
    Fault::new(
        "RETENTION_SETTINGS_STATE",
        8,
        "Saved retention settings could not be verified",
    )
}

fn parse_days(value: &Value) -> Result<Option<i64>, Fault> {
    if value.is_null() {
        return Ok(None);
    }
    let days = value
        .as_i64()
        .filter(|days| *days > 0)
        .ok_or_else(|| invalid("Retention days must be a positive integer or null"))?;
    Ok(Some(days))
}

type Settings = [Option<i64>; 5];

fn validate_args(args: &Value, current: Settings) -> Result<Settings, Fault> {
    let object = args
        .as_object()
        .ok_or_else(|| invalid("Retention settings must be an object"))?;
    if object.keys().any(|key| {
        ![
            "history_days",
            "deleted_days",
            "deleted_board_days",
            "deleted_thread_days",
            "archived_thread_days",
        ]
        .contains(&key.as_str())
    }) {
        return Err(invalid("Retention settings contain an unsupported field"));
    }
    let names = [
        "history_days",
        "deleted_days",
        "deleted_board_days",
        "deleted_thread_days",
        "archived_thread_days",
    ];
    let mut parsed = current;
    for (index, name) in names.into_iter().enumerate() {
        if let Some(value) = object.get(name) {
            parsed[index] = parse_days(value)?;
        } else if index < 2 {
            return Err(invalid("history_days and deleted_days are required"));
        }
    }
    Ok(parsed)
}

fn read_settings(db: &Connection) -> Result<Settings, Fault> {
    let row: Option<Settings> = db
        .query_row(
            "SELECT history_days,deleted_days,deleted_board_days,deleted_thread_days,archived_thread_days \
             FROM retention_settings WHERE singleton=1",
            [],
            |row| Ok([row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?]),
        )
        .optional()?;
    let Some(settings) = row else {
        return Err(settings_state());
    };
    for days in settings.into_iter().flatten() {
        if days <= 0 {
            return Err(settings_state());
        }
    }
    Ok(settings)
}

pub(crate) fn get_settings(db: &Connection, actor: &str) -> Result<Value, Fault> {
    human::require(db, actor, RETENTION_PERMISSION, None, None)?;
    let [
        history_days,
        deleted_days,
        deleted_board_days,
        deleted_thread_days,
        archived_thread_days,
    ] = read_settings(db)?;
    Ok(
        json!({"history_days":history_days,"deleted_days":deleted_days,
        "deleted_board_days":deleted_board_days,"deleted_thread_days":deleted_thread_days,
        "archived_thread_days":archived_thread_days}),
    )
}

pub(crate) fn set_settings(db: &Connection, actor: &str, args: &Value) -> Result<Value, Fault> {
    human::require(db, actor, RETENTION_PERMISSION, None, None)?;
    let current = read_settings(db)?;
    let [
        history_days,
        deleted_days,
        deleted_board_days,
        deleted_thread_days,
        archived_thread_days,
    ] = validate_args(args, current)?;
    db.execute(
        "UPDATE retention_settings SET history_days=?1,deleted_days=?2,deleted_board_days=?3, \
         deleted_thread_days=?4,archived_thread_days=?5 WHERE singleton=1",
        params![
            history_days,
            deleted_days,
            deleted_board_days,
            deleted_thread_days,
            archived_thread_days
        ],
    )?;
    if db.changes() != 1 {
        return Err(settings_state());
    }
    Ok(
        json!({"history_days":history_days,"deleted_days":deleted_days,
        "deleted_board_days":deleted_board_days,"deleted_thread_days":deleted_thread_days,
        "archived_thread_days":archived_thread_days}),
    )
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>, Fault> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|_| invalid("Retention timestamps must be RFC3339"))
}

fn is_expired(anchor: DateTime<Utc>, now: DateTime<Utc>, days: i64) -> bool {
    let age_millis = i128::from(now.timestamp_millis()) - i128::from(anchor.timestamp_millis());
    let retention_millis = i128::from(days) * 86_400_000;
    age_millis >= retention_millis
}

fn stored_timestamp(value: &str) -> Result<DateTime<Utc>, Fault> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|_| {
            Fault::new(
                "RETENTION_DATA_STATE",
                8,
                "Stored deletion or history timestamp is invalid",
            )
        })
}

fn latest_timestamp<'a>(
    values: impl Iterator<Item = Option<&'a String>>,
) -> Result<Option<DateTime<Utc>>, Fault> {
    let mut latest = None;
    for value in values.flatten() {
        let parsed = stored_timestamp(value)?;
        if latest.is_none_or(|current| parsed > current) {
            latest = Some(parsed);
        }
    }
    Ok(latest)
}

/// Purge expired history and bodies inside the caller's immediate transaction.
/// This is a trusted maintenance entry point; it intentionally accepts a clock
/// value for deterministic tests and is never request-facing.
pub(crate) fn purge_expired(db: &Connection, now: &str) -> Result<Value, Fault> {
    let now = parse_timestamp(now)?;
    let [
        history_days,
        deleted_days,
        deleted_board_days,
        deleted_thread_days,
        archived_thread_days,
    ] = read_settings(db)?;
    db.pragma_update(None, "secure_delete", "ON")?;
    let mut purged_conversations = 0usize;
    if let Some(days) = deleted_board_days
        && let Some(cutoff) = cutoff(now, days)
    {
        let ids = expired_ids(
            db,
            "SELECT id FROM boards WHERE deleted_at IS NOT NULL AND julianday(deleted_at)<=julianday(?1) ORDER BY id",
            &cutoff,
        )?;
        for id in ids {
            crate::trash::purge_conversation(
                db,
                crate::trash::ConversationKind::Board,
                &id,
                false,
            )?;
            purged_conversations += 1;
        }
    }
    if let Some(days) = deleted_thread_days
        && let Some(cutoff) = cutoff(now, days)
    {
        let ids = expired_ids(
            db,
            "SELECT t.id FROM threads t WHERE t.deleted_at IS NOT NULL AND julianday(t.deleted_at)<=julianday(?1) ORDER BY t.id",
            &cutoff,
        )?;
        for id in ids {
            // A selected Board already removed all of its descendants.
            if crate::trash::deleted(db, crate::trash::ConversationKind::Thread, &id)? {
                crate::trash::purge_conversation(
                    db,
                    crate::trash::ConversationKind::Thread,
                    &id,
                    false,
                )?;
                purged_conversations += 1;
            }
        }
    }
    if let Some(days) = archived_thread_days
        && let Some(cutoff) = cutoff(now, days)
    {
        let ids = expired_ids(
            db,
            "SELECT t.id FROM threads t JOIN boards b ON b.id=t.board_id WHERE t.archived_at IS NOT NULL AND t.deleted_at IS NULL AND b.deleted_at IS NULL AND julianday(t.archived_at)<=julianday(?1) ORDER BY t.id",
            &cutoff,
        )?;
        for id in ids {
            crate::trash::purge_conversation(
                db,
                crate::trash::ConversationKind::Thread,
                &id,
                true,
            )?;
            purged_conversations += 1;
        }
    }

    let posts = {
        let sql = if deleted_days.is_some() {
            "SELECT p.id,p.body_purged_at,p.deleted_at,t.deleted_at,b.deleted_at \
             FROM posts p JOIN threads t ON t.id=p.thread_id \
             JOIN boards b ON b.id=t.board_id \
             WHERE p.body_purged_at IS NOT NULL OR p.deleted_at IS NOT NULL \
                OR t.deleted_at IS NOT NULL OR b.deleted_at IS NOT NULL ORDER BY p.id"
        } else {
            "SELECT id,body_purged_at,deleted_at,NULL,NULL FROM posts \
             WHERE body_purged_at IS NOT NULL ORDER BY id"
        };
        let mut statement = db.prepare(sql)?;
        statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };

    let mut purge_post_ids = Vec::new();
    for (id, already_purged_at, post_deleted, thread_deleted, board_deleted) in &posts {
        if already_purged_at.is_some() {
            purge_post_ids.push(id.as_str());
            continue;
        }
        let Some(days) = deleted_days else {
            continue;
        };
        let anchor = latest_timestamp(
            [
                post_deleted.as_ref(),
                thread_deleted.as_ref(),
                board_deleted.as_ref(),
            ]
            .into_iter(),
        )?;
        if anchor.is_some_and(|anchor| is_expired(anchor, now, days)) {
            purge_post_ids.push(id.as_str());
        }
    }

    let purged_at = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let mut purged_posts = 0usize;
    for post_id in &purge_post_ids {
        purged_posts += db.execute(
            "UPDATE posts SET body='',body_purged_at=?1 \
             WHERE id=?2 AND body_purged_at IS NULL",
            params![purged_at, post_id],
        )?;
    }

    let mut purged_revisions = 0usize;
    for post_id in &purge_post_ids {
        purged_revisions += db.execute("DELETE FROM post_revisions WHERE post_id=?1", [post_id])?;
    }
    if let Some(days) = history_days
        && let Some(duration) = TimeDelta::try_days(days)
        && let Some(cutoff) = now.checked_sub_signed(duration)
    {
        let cutoff = cutoff.to_rfc3339_opts(SecondsFormat::Millis, true);
        purged_revisions += db.execute(
            "DELETE FROM post_revisions WHERE julianday(saved_at)<=julianday(?1)",
            [cutoff],
        )?;
    }
    // Run after conversation and history purges so identities made orphaned by
    // this maintenance pass can be reclaimed in the same transaction.
    let (purged_identities, pruned_requests) = purge_expired_orphan_identities(db, now)?;
    if purged_posts + purged_revisions + purged_conversations + purged_identities + pruned_requests
        > 0
    {
        crate::trash::mark_reclaim_pending(db)?;
    }
    Ok(
        json!({"purged_posts":purged_posts,"purged_revisions":purged_revisions,
        "purged_conversations":purged_conversations,"purged_identities":purged_identities,
        "pruned_requests":pruned_requests}),
    )
}

fn cutoff(now: DateTime<Utc>, days: i64) -> Option<String> {
    TimeDelta::try_days(days)
        .and_then(|duration| now.checked_sub_signed(duration))
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
}

/// Drop expired idempotency receipts and expired AI identities only when no
/// surviving conversation, revision, event, request, or ownership row refers
/// to the actor. This never prunes human accounts or their daily display IDs.
fn purge_expired_orphan_identities(
    db: &Connection,
    now: DateTime<Utc>,
) -> Result<(usize, usize), Fault> {
    let request_cutoff = cutoff(now, 7);
    let pruned_requests = if let Some(cutoff) = request_cutoff.as_deref() {
        db.execute(
            "DELETE FROM create_requests WHERE julianday(created_at)<=julianday(?1)",
            [cutoff],
        )?
    } else {
        0
    };
    let now_text = now.to_rfc3339_opts(SecondsFormat::Millis, true);
    let candidates = {
        let mut statement = db.prepare(
            "SELECT i.id,i.display_id FROM ai_identities i \
             WHERE julianday(i.expires_at)<=julianday(?1) \
             AND NOT EXISTS(SELECT 1 FROM boards WHERE creator_id=i.id) \
             AND NOT EXISTS(SELECT 1 FROM threads WHERE creator_id=i.id) \
             AND NOT EXISTS(SELECT 1 FROM posts WHERE author_id=i.id OR edited_by=i.id) \
             AND NOT EXISTS(SELECT 1 FROM post_revisions WHERE editor_id=i.id) \
             AND NOT EXISTS(SELECT 1 FROM changes WHERE actor_id=i.id) \
             AND NOT EXISTS(SELECT 1 FROM create_requests WHERE actor_id=i.id) \
             ORDER BY i.id",
        )?;
        statement
            .query_map([now_text], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (id, display_id) in &candidates {
        db.execute("DELETE FROM ai_sessions WHERE actor_id=?1", [id])?;
        db.execute(
            "DELETE FROM app_metadata WHERE key=?1",
            [format!("identity-harness-v1:{id}")],
        )?;
        db.execute("DELETE FROM ai_identities WHERE id=?1", [id])?;
        db.execute(
            "DELETE FROM display_ids WHERE display_id=?1 AND actor_id=?2",
            params![display_id, id],
        )?;
        db.execute("DELETE FROM actors WHERE id=?1 AND kind='ai'", [id])?;
    }
    Ok((candidates.len(), pruned_requests))
}

fn expired_ids(db: &Connection, sql: &str, cutoff: &str) -> Result<Vec<String>, Fault> {
    let mut statement = db.prepare(sql)?;
    Ok(statement
        .query_map([cutoff], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{human, store};
    use rusqlite::Connection;
    use serde_json::json;

    fn fixture() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        store::migrate(&mut db).unwrap();
        db.execute_batch(
            "INSERT INTO accounts(id,name,password_hash,recovery_hash,is_admin,created_at,disabled) VALUES
               ('admin','Admin','unused','unused',1,'2026-01-01T00:00:00Z',0),
               ('user','User','unused','unused',0,'2026-01-01T00:00:00Z',0);
             INSERT INTO actors(id,kind) VALUES('admin','human'),('user','human');
             INSERT INTO display_ids(display_id,actor_id) VALUES('display','admin');",
        )
        .unwrap();
        db
    }

    fn purge(db: &mut Connection, now: &str) -> Result<Value, Fault> {
        let tx = db
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|_| Fault::internal())?;
        let result = purge_expired(&tx, now)?;
        tx.commit().map_err(|_| Fault::internal())?;
        Ok(result)
    }

    fn grant(db: &Connection, user: &str) {
        human::dispatch(
            db,
            "permission.set",
            &json!({
                "subject_type":"account","subject_id":user,
                "operation":"retention.manage","scope":"global",
                "target_id":"","decision":"allow"
            }),
            "admin",
        )
        .unwrap();
    }

    fn insert_board_thread(
        db: &Connection,
        board: &str,
        thread: &str,
        board_deleted: Option<&str>,
        thread_deleted: Option<&str>,
    ) {
        db.execute(
            "INSERT INTO boards(id,name,description,creator_id,revision,created_at,updated_at,deleted_at) \
             VALUES(?1,'Board','', 'admin',1,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z',?2)",
            params![board, board_deleted],
        ).unwrap();
        db.execute(
            "INSERT INTO threads(id,board_id,name,description,creator_id,revision,next_no,created_at,updated_at,deleted_at) \
             VALUES(?1,?2,'Thread','', 'admin',1,10,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z',?3)",
            params![thread, board, thread_deleted],
        ).unwrap();
    }

    fn insert_thread(db: &Connection, board: &str, thread: &str, deleted_at: Option<&str>) {
        db.execute(
            "INSERT INTO threads(id,board_id,name,description,creator_id,revision,next_no,created_at,updated_at,deleted_at) \
             VALUES(?1,?2,'Thread','', 'admin',1,10,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z',?3)",
            params![thread, board, deleted_at],
        ).unwrap();
    }

    fn insert_post(
        db: &Connection,
        id: &str,
        thread: &str,
        no: i64,
        body: &str,
        reply_to: Option<i64>,
        deleted_at: Option<&str>,
    ) {
        db.execute(
            "INSERT INTO posts(id,thread_id,no,body,author_id,model_name,display_id,created_at,updated_at,revision,reply_to,deleted_at) \
             VALUES(?1,?2,?3,?4,'admin','USER','display','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z',1,?5,?6)",
            params![id, thread, no, body, reply_to, deleted_at],
        ).unwrap();
    }

    fn insert_revision(db: &Connection, post: &str, saved_at: &str, body: &str) {
        let revision: i64 = db
            .query_row(
                "SELECT COALESCE(MAX(revision),0)+1 FROM post_revisions WHERE post_id=?1",
                [post],
                |row| row.get(0),
            )
            .unwrap();
        db.execute(
            "INSERT INTO post_revisions(post_id,revision,body,saved_at,editor_id,editor_display_id) VALUES(?1,?2,?3,?4,'admin','display')",
            params![post, revision, body, saved_at],
        ).unwrap();
    }

    #[test]
    fn retention_settings_are_infinite_by_default_and_require_global_human_grant() {
        let db = fixture();
        assert_eq!(
            get_settings(&db, "admin").unwrap(),
            json!({"history_days":null,"deleted_days":null,"deleted_board_days":null,
                "deleted_thread_days":null,"archived_thread_days":null})
        );
        assert_eq!(
            get_settings(&db, "user").unwrap_err().code,
            "PERMISSION_DENIED"
        );

        let capabilities = human::authorization_capabilities(&db, "admin").unwrap();
        let operation = capabilities["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["operation"] == RETENTION_PERMISSION)
            .unwrap();
        assert_eq!(operation["scopes"], json!(["global"]));
        grant(&db, "user");
        assert_eq!(
            get_settings(&db, "user").unwrap(),
            json!({"history_days":null,"deleted_days":null,"deleted_board_days":null,
                "deleted_thread_days":null,"archived_thread_days":null})
        );
    }

    #[test]
    fn retention_settings_validate_exact_nullable_positive_integer_fields() {
        let db = fixture();
        assert_eq!(
            set_settings(&db, "admin", &json!({"history_days":30,"deleted_days":90})).unwrap(),
            json!({"history_days":30,"deleted_days":90,"deleted_board_days":null,
                "deleted_thread_days":null,"archived_thread_days":null})
        );
        set_settings(
            &db,
            "admin",
            &json!({"history_days":null,"deleted_days":null,
            "deleted_board_days":12,"deleted_thread_days":18,"archived_thread_days":24}),
        )
        .unwrap();
        assert_eq!(
            set_settings(&db, "admin", &json!({"history_days":30,"deleted_days":90})).unwrap()["deleted_board_days"],
            12
        );
        for invalid in [
            json!({"history_days":0,"deleted_days":90}),
            json!({"history_days":-1,"deleted_days":90}),
            json!({"history_days":"30","deleted_days":90}),
            json!({"history_days":30.5,"deleted_days":90}),
            json!({"history_days":true,"deleted_days":90}),
            json!({"history_days":30}),
            json!({"history_days":30,"deleted_days":90,"extra":1}),
            json!([]),
        ] {
            assert_eq!(
                set_settings(&db, "admin", &invalid).unwrap_err().code,
                "INVALID_INPUT"
            );
        }
        assert_eq!(
            set_settings(
                &db,
                "admin",
                &json!({"history_days":null,"deleted_days":null})
            )
            .unwrap(),
            json!({"history_days":null,"deleted_days":null,"deleted_board_days":12,
                "deleted_thread_days":18,"archived_thread_days":24})
        );
    }

    #[test]
    fn permission_catalog_rejects_non_global_retention_grants() {
        let db = fixture();
        let result = human::dispatch(
            &db,
            "permission.set",
            &json!({
                "subject_type":"account","subject_id":"user",
                "operation":"retention.manage","scope":"thread",
                "target_id":"thread","decision":"allow"
            }),
            "admin",
        );
        assert_eq!(result.unwrap_err().code, "INVALID_INPUT");
    }

    #[test]
    fn purge_uses_latest_active_ancestor_and_deletion_times_without_losing_structure() {
        let mut db = fixture();
        insert_board_thread(
            &db,
            "board-a",
            "thread-a",
            Some("2026-03-10T00:00:00Z"),
            Some("2026-03-01T00:00:00Z"),
        );
        insert_post(
            &db,
            "deleted-root",
            "thread-a",
            1,
            "deleted current body",
            None,
            Some("2026-01-01T00:00:00Z"),
        );
        insert_post(&db, "reply", "thread-a", 2, "reply body", Some(1), None);
        insert_revision(
            &db,
            "deleted-root",
            "2026-03-20T00:00:00Z",
            "deleted historical body",
        );
        insert_revision(&db, "reply", "2026-01-01T00:00:00Z", "expired history");

        insert_board_thread(&db, "board-b", "thread-b", None, None);
        insert_post(&db, "live", "thread-b", 1, "live body", None, None);
        insert_revision(&db, "live", "2026-01-01T00:00:00Z", "expired history");
        insert_revision(&db, "live", "2026-03-20T00:00:00Z", "recent history");

        set_settings(&db, "admin", &json!({"history_days":30,"deleted_days":30})).unwrap();
        let first = purge(&mut db, "2026-04-05T00:00:00Z").unwrap();
        assert_eq!(
            first,
            json!({"purged_posts":0,"purged_revisions":2,"purged_conversations":0,
                "purged_identities":0,"pruned_requests":0})
        );
        let body: String = db
            .query_row("SELECT body FROM posts WHERE id='deleted-root'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(body, "deleted current body");

        db.execute("UPDATE boards SET deleted_at=NULL WHERE id='board-a'", [])
            .unwrap();
        db.execute("UPDATE threads SET deleted_at=NULL WHERE id='thread-a'", [])
            .unwrap();
        let second = purge(&mut db, "2026-04-05T00:00:00Z").unwrap();
        assert_eq!(
            second,
            json!({"purged_posts":1,"purged_revisions":1,"purged_conversations":0,
                "purged_identities":0,"pruned_requests":0})
        );

        let post: (String, Option<String>, Option<String>, i64, Option<i64>) = db.query_row(
            "SELECT body,body_purged_at,deleted_at,no,reply_to FROM posts WHERE id='deleted-root'",
            [],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)),
        ).unwrap();
        assert_eq!(post.0, "");
        assert_eq!(post.1.as_deref(), Some("2026-04-05T00:00:00.000Z"));
        assert_eq!(post.2.as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(post.3, 1);
        assert_eq!(post.4, None);
        assert_eq!(
            db.query_row("SELECT reply_to FROM posts WHERE id='reply'", [], |r| {
                r.get::<_, Option<i64>>(0)
            })
            .unwrap(),
            Some(1)
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM post_revisions WHERE post_id='deleted-root'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM post_revisions WHERE post_id='live'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert!(
            db.execute(
                "UPDATE posts SET body='restored' WHERE id='deleted-root'",
                []
            )
            .is_err()
        );
    }

    #[test]
    fn invalid_maintenance_clock_fails_before_mutating_content() {
        let mut db = fixture();
        insert_board_thread(&db, "board", "thread", None, None);
        insert_post(
            &db,
            "post",
            "thread",
            1,
            "body",
            None,
            Some("2026-01-01T00:00:00Z"),
        );
        set_settings(&db, "admin", &json!({"history_days":null,"deleted_days":1})).unwrap();
        assert_eq!(
            purge(&mut db, "yesterday").unwrap_err().code,
            "INVALID_INPUT"
        );
        assert_eq!(
            db.query_row("SELECT body FROM posts WHERE id='post'", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "body"
        );
    }

    #[test]
    fn conversation_retention_purges_only_expired_board_thread_and_archived_rows() {
        let mut db = fixture();
        insert_board_thread(
            &db,
            "board-expired",
            "thread-under-expired-board",
            Some("2026-02-26T00:00:00Z"),
            None,
        );
        insert_board_thread(
            &db,
            "board-live",
            "thread-expired",
            None,
            Some("2026-02-25T00:00:00Z"),
        );
        insert_thread(&db, "board-live", "thread-survivor", None);
        insert_thread(&db, "board-live", "thread-archived", None);
        db.execute(
            "UPDATE threads SET next_no=1001,archived_at='2026-02-25T00:00:00Z' WHERE id='thread-archived'",
            [],
        ).unwrap();
        insert_board_thread(
            &db,
            "board-future",
            "thread-future",
            Some("2026-02-28T00:00:00Z"),
            None,
        );
        insert_thread(&db, "board-live", "thread-archived-future", None);
        db.execute(
            "UPDATE threads SET next_no=1001,archived_at='2026-02-28T00:00:00Z' WHERE id='thread-archived-future'",
            [],
        ).unwrap();
        set_settings(
            &db,
            "admin",
            &json!({"history_days":null,"deleted_days":null,"deleted_board_days":3,
                "deleted_thread_days":3,"archived_thread_days":3}),
        )
        .unwrap();
        let result = purge(&mut db, "2026-03-01T00:00:00Z").unwrap();
        assert_eq!(result["purged_conversations"], 3);
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM boards WHERE id='board-expired'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(db.query_row("SELECT count(*) FROM threads WHERE id IN ('thread-under-expired-board','thread-expired','thread-archived')", [], |r|r.get::<_,i64>(0)).unwrap(), 0);
        assert_eq!(db.query_row("SELECT count(*) FROM threads WHERE id IN ('thread-survivor','thread-future','thread-archived-future')", [], |r|r.get::<_,i64>(0)).unwrap(), 3);

        set_settings(
            &db,
            "admin",
            &json!({"history_days":null,"deleted_days":null,"deleted_board_days":null,
                "deleted_thread_days":null,"archived_thread_days":null}),
        )
        .unwrap();
        let disabled = purge(&mut db, "2030-01-01T00:00:00Z").unwrap();
        assert_eq!(disabled["purged_conversations"], 0);
    }
}
