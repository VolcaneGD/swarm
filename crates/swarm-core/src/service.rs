use crate::{
    accounts, ai, human, human_routes, identity, notifications, policy, projects,
    protocol::{Fault, Request},
    store,
};
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use uuid::Uuid;

type CredentialSink<'a> = &'a mut dyn FnMut(&Value) -> Result<(), Fault>;

fn credential_receipt_key(key: &str) -> Result<String, Fault> {
    if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Credential receipt key must be 64 hexadecimal characters",
        ));
    }
    Ok(format!("credential-receipt:{}", accounts::digest(key)))
}

fn credential_receipt_value(db: &Connection, metadata_key: &str) -> Result<Option<Value>, Fault> {
    let encoded: Option<String> = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [metadata_key],
            |row| row.get(0),
        )
        .optional()?;
    encoded
        .map(|value| serde_json::from_str(&value).map_err(|_| Fault::internal()))
        .transpose()
}

fn credential_receipt_exists(db: &Connection, metadata_key: &str) -> Result<bool, Fault> {
    Ok(db.query_row(
        "SELECT EXISTS(SELECT 1 FROM app_metadata WHERE key=?1)",
        [metadata_key],
        |row| row.get(0),
    )?)
}

pub struct Service {
    db: Connection,
    root: PathBuf,
}

#[cfg(test)]
#[path = "restore_capacity_tests.rs"]
mod restore_capacity_tests;

pub(crate) fn timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
pub(crate) fn required_text<'a>(args: &'a Value, key: &str) -> Result<&'a str, Fault> {
    args.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, &format!("A nonblank {key} is required")))
}
fn integer(args: &Value, key: &str) -> Result<i64, Fault> {
    args.get(key)
        .and_then(Value::as_i64)
        .filter(|n| *n > 0)
        .ok_or_else(|| {
            Fault::new(
                "INVALID_INPUT",
                2,
                &format!("A positive integer {key} is required"),
            )
        })
}
fn body(args: &Value) -> Result<&str, Fault> {
    let text = required_text(args, "body")?;
    if text.len() > 1024 * 1024 {
        return Err(Fault::new(
            "BODY_TOO_LARGE",
            2,
            "Body exceeds 1 MiB of UTF-8",
        ));
    }
    if text.chars().count() > 100_000 {
        return Err(Fault::new(
            "BODY_TOO_LONG",
            2,
            "Body exceeds 100000 characters",
        ));
    }
    Ok(text)
}
fn name<'a>(args: &'a Value, operation: &str) -> Result<&'a str, Fault> {
    let text = required_text(args, "name")?;
    let limit = if operation.starts_with("board.") {
        100
    } else {
        200
    };
    if text.chars().count() > limit {
        return Err(Fault::new(
            "NAME_TOO_LONG",
            2,
            &format!("Name exceeds {limit} characters"),
        ));
    }
    Ok(text)
}
fn description(args: &Value) -> Result<&str, Fault> {
    match args.get("description") {
        None => Ok(""),
        Some(Value::String(s)) => Ok(s),
        _ => Err(Fault::new("INVALID_INPUT", 2, "Description must be text")),
    }
}
fn not_found() -> Fault {
    Fault::new("NOT_FOUND", 5, "The requested item is not available")
}
fn validate_arguments(request: &Request) -> Result<(), Fault> {
    if let Some(key) = &request.request_id
        && (!matches!(
            request.operation.as_str(),
            "board.create"
                | "thread.create"
                | "post.create"
                | "response.create"
                | "project.register"
        ) || key.trim().is_empty()
            || key.len() > 256)
    {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Request ID is supported only for creation and must be 1–256 bytes",
        ));
    }
    let fields: &[&str] = match request.operation.as_str() {
        "account.setup" | "account.login" | "account.create" => &["name", "password"],
        "account.recover" => &["name", "recovery_code", "password"],
        "account.google.begin" => &["name", "purpose", "current_password"],
        "account.google.link" => &["challenge", "id_token"],
        "account.google.unlink" => &["current_password"],
        "account.google.recover" => &["name", "challenge", "id_token", "password"],
        "account.reset" => &["id", "password"],
        "account.password" => &["password", "current_password", "name"],
        "account.disable" => &["id", "disabled"],
        "account.get" => &["id"],
        "role.create" => &["name"],
        "role.assign" => &["account_id", "role_id", "assigned"],
        "permission.set" => &[
            "subject_type",
            "subject_id",
            "operation",
            "scope",
            "target_id",
            "decision",
        ],
        "permission.list" => &["subject_type", "subject_id"],
        "search" => &[
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
        ],
        "account.status"
        | "account.list"
        | "role.list"
        | "authorization.capabilities"
        | "rules.capabilities" => &[],
        "rules.get" => &["project_id", "recovery_id"],
        "rules.validate" => &["project_id", "source"],
        "rules.save" => &[
            "project_id",
            "source",
            "if_revision",
            "recovery_id",
            "recovery_revision",
        ],
        "rules.check" => &["project_id", "actor_id", "operation", "target"],
        "rules.subjects" => &["project_id"],
        "account.logout"
        | "conversation.capabilities"
        | "conversation.deleted"
        | "conversation.pins"
        | "management.capabilities"
        | "project.list"
        | "identity.status"
        | "identity.release" => &[],
        "project.register" => &["name", "root"],
        "project.get" => &["id"],
        "project.alias" | "project.root" => &["project_id", "path"],
        "identity.start" => &[
            "project_id",
            "model_name",
            "model_report",
            "model_local",
            "model_client",
        ],
        "identity.resume" => &[
            "actor_id",
            "project_id",
            "model_name",
            "model_report",
            "model_client",
            "model_local",
            "resume_secret",
        ],
        "identity.model" => &["model_name", "model_report", "model_local", "model_client"],
        "board.create" => &["name", "description"],
        "thread.create" => &["board_id", "name", "description"],
        "board.get" | "thread.get" | "board.delete" | "board.restore" | "thread.delete"
        | "thread.restore" => &["id"],
        "board.purge" | "thread.purge" => &["id", "confirmed"],
        "conversation.batch" => &["action", "items", "confirmed"],
        "board.edit" | "thread.edit" => &["id", "name", "description", "if_revision"],
        "thread.list" => &["board_id", "limit", "cursor"],
        "board.list" => &["limit", "cursor"],
        "thread.pin" => &["id", "pinned"],
        "conversation.activity" => &["limit", "before_id"],
        "conversation.subscriptions" | "conversation.unread" | "notification.settings.get" => &[],
        "thread.subscribe" => &["id", "subscribed"],
        "post.read.mark" => &["thread_id", "post_ids"],
        "notification.settings.set" => &["enabled", "include_excerpt"],
        "retention.settings.get" => &[],
        "retention.settings.set" => &[
            "history_days",
            "deleted_days",
            "deleted_board_days",
            "deleted_thread_days",
            "archived_thread_days",
        ],
        "display.settings.get" => &[],
        "display.settings.set" => &["language", "theme"],
        "ui.preferences.get" => &["expected_account_id", "scope", "key"],
        "ui.preferences.set" => &["expected_account_id", "scope", "key", "value"],
        "changes.get" => &["thread_id", "board_id", "after", "limit"],
        "changes.wait" => &["thread_id", "board_id", "after", "limit", "timeout"],
        "post.create" => &["thread_id", "body", "thread_rules_token"],
        "response.create" => &["thread_id", "body", "to", "thread_rules_token"],
        "post.get" | "post.history" | "post.delete" | "post.restore" | "post.deleted_body" => {
            &["thread_id", "no"]
        }
        "post.edit" => &[
            "thread_id",
            "no",
            "body",
            "if_revision",
            "thread_rules_token",
        ],
        "post.list" => &["thread_id", "limit", "before_no"],
        "post.page" => &["thread_id", "page", "page_size", "anchor_no"],
        "response.list" => &["thread_id", "to", "limit", "before_no"],
        _ => {
            return Err(Fault::new(
                "UNKNOWN_OPERATION",
                2,
                "Operation is not supported",
            ));
        }
    };
    let args = request
        .args
        .as_object()
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "Arguments must be an object"))?;
    if args.keys().any(|key| !fields.contains(&key.as_str())) || !request.context.is_object() {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Unsupported argument or invalid context object",
        ));
    }
    Ok(())
}
fn conflict() -> Fault {
    Fault::new(
        "REVISION_CONFLICT",
        6,
        "The item changed; reload and retry with its current revision",
    )
}
fn live_board(db: &Connection, id: &str) -> Result<(), Fault> {
    let live: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM boards WHERE id=?1 AND deleted_at IS NULL)",
        [id],
        |r| r.get(0),
    )?;
    if live { Ok(()) } else { Err(not_found()) }
}
fn live_thread(db: &Connection, id: &str) -> Result<(), Fault> {
    let live: bool=db.query_row("SELECT EXISTS(SELECT 1 FROM threads t JOIN boards b ON b.id=t.board_id WHERE t.id=?1 AND t.deleted_at IS NULL AND b.deleted_at IS NULL)",[id],|r| r.get(0))?;
    if live { Ok(()) } else { Err(not_found()) }
}
fn change(db: &Connection, op: &str, id: &str, actor: &str) -> Result<(), Fault> {
    db.execute(
        "INSERT INTO changes(operation,entity_id,actor_id,created_at) VALUES(?1,?2,?3,?4)",
        params![op, id, actor, timestamp()],
    )?;
    if matches!(
        op,
        "board.create" | "thread.create" | "post.create" | "response.create" | "post.edit"
    ) {
        identity::renew(db, actor)?;
    }
    Ok(())
}
fn author_display(db: &Connection, actor: &str) -> Result<(String, String), Fault> {
    let kind: String =
        db.query_row("SELECT kind FROM actors WHERE id=?1", [actor], |r| r.get(0))?;
    if kind == "ai" {
        let identity = identity::public(db, actor)?;
        Ok((
            identity["model_name"]
                .as_str()
                .ok_or_else(Fault::internal)?
                .to_owned(),
            identity["display_id"]
                .as_str()
                .ok_or_else(Fault::internal)?
                .to_owned(),
        ))
    } else {
        Ok(("USER".into(), accounts::display_id(db, actor)?))
    }
}
pub(crate) fn board_get(db: &Connection, id: &str, include_deleted: bool) -> Result<Value, Fault> {
    let value=db.query_row("SELECT id,name,description,creator_id,revision,created_at,updated_at,deleted_at FROM boards WHERE id=?1",
        [id], |r| Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"description":r.get::<_,String>(2)?,
        "creator_id":r.get::<_,String>(3)?,"revision":r.get::<_,i64>(4)?,"created_at":r.get::<_,String>(5)?,
        "updated_at":r.get::<_,String>(6)?,"deleted_at":r.get::<_,Option<String>>(7)?}))).optional()?.ok_or_else(not_found)?;
    if !include_deleted && !value["deleted_at"].is_null() {
        return Err(not_found());
    }
    Ok(value)
}
pub(crate) fn thread_get(db: &Connection, id: &str, include_deleted: bool) -> Result<Value, Fault> {
    let value=db.query_row("SELECT id,board_id,name,description,creator_id,revision,created_at,updated_at,deleted_at,next_no,archived_at FROM threads WHERE id=?1",
        [id], |r| Ok(json!({"id":r.get::<_,String>(0)?,"board_id":r.get::<_,String>(1)?,"name":r.get::<_,String>(2)?,
        "description":r.get::<_,String>(3)?,"creator_id":r.get::<_,String>(4)?,"revision":r.get::<_,i64>(5)?,
        "created_at":r.get::<_,String>(6)?,"updated_at":r.get::<_,String>(7)?,"deleted_at":r.get::<_,Option<String>>(8)?,
        "archived":r.get::<_,i64>(9)?>1000,"archived_at":r.get::<_,Option<String>>(10)?}))).optional()?.ok_or_else(not_found)?;
    if !include_deleted {
        live_thread(db, id)?;
    }
    Ok(value)
}
pub(crate) fn post_get(db: &Connection, thread: &str, no: i64) -> Result<Value, Fault> {
    live_thread(db, thread)?;
    db.query_row(
        "SELECT p.id,p.no,p.body,p.author_id,p.model_name,p.display_id,p.created_at,p.updated_at,p.revision,p.reply_to,p.edited_by,p.editor_display_id,p.deleted_at,
        (SELECT count(*) FROM posts x WHERE x.thread_id=p.thread_id AND x.reply_to=p.no AND x.deleted_at IS NULL AND x.body_purged_at IS NULL),p.editor_model_name,p.body_purged_at
        ,(SELECT a.name FROM accounts a WHERE a.id=p.author_id)
        ,(SELECT value FROM app_metadata WHERE key='post-harness-v1:' || p.id)
        FROM posts p WHERE p.thread_id=?1 AND p.no=?2",params![thread,no],
        |r| {
            let deleted=r.get::<_,Option<String>>(12)?;
            let purged=r.get::<_,Option<String>>(15)?.is_some();
            Ok(json!({"id":r.get::<_,String>(0)?,"thread_id":thread,"no":r.get::<_,i64>(1)?,
                "body":if deleted.is_none() && !purged {Some(r.get::<_,String>(2)?)}else{None},
                "author":{"id":r.get::<_,String>(3)?,"model_name":r.get::<_,String>(4)?,"display_id":r.get::<_,String>(5)?,"user_name":r.get::<_,Option<String>>(16)?,"harness_name":r.get::<_,Option<String>>(17)?.unwrap_or_else(|| "不明".into())},
                "created_at":r.get::<_,String>(6)?,"updated_at":r.get::<_,String>(7)?,"revision":r.get::<_,i64>(8)?,
                "reply_to":r.get::<_,Option<i64>>(9)?,"edited_by":r.get::<_,Option<String>>(10)?,
                "editor_display_id":r.get::<_,Option<String>>(11)?,"deleted":deleted.is_some() || purged,"deleted_at":deleted,"restore_available":!purged,
                "editor_model_name":r.get::<_,Option<String>>(14)?,
                "response_count":r.get::<_,i64>(13)?}))
        }).optional()?.ok_or_else(not_found)
}
impl Service {
    /// Trusted runtime maintenance; no public/AI purge operation is exposed.
    pub fn purge_expired_data(&mut self) -> Result<Value, Fault> {
        self.db.execute_batch("PRAGMA secure_delete=ON;")?;
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = crate::retention::purge_expired(&tx, &timestamp())?;
        tx.commit()?;
        let needs_reclaim = crate::trash::reclaim_pending(&self.db)?
            || result["purged_posts"].as_u64().unwrap_or(0) > 0
            || result["purged_revisions"].as_u64().unwrap_or(0) > 0
            || result["purged_conversations"].as_u64().unwrap_or(0) > 0
            || result["purged_identities"].as_u64().unwrap_or(0) > 0
            || result["pruned_requests"].as_u64().unwrap_or(0) > 0;
        let storage_reclaimed = needs_reclaim && reclaim_sqlite_space(&self.db);
        let mut result = result;
        if let Some(object) = result.as_object_mut() {
            object.insert("storage_reclaimed".into(), Value::Bool(storage_reclaimed));
        }
        Ok(result)
    }
    /// Runtime owner uses this path for protected recovery gates.
    pub fn data_root(&self) -> &Path {
        &self.root
    }
    /// Private GUI bootstrap probe. Reveals no accounts or credentials.
    pub fn setup_required(&self) -> Result<bool, Fault> {
        Ok(!self
            .db
            .query_row("SELECT EXISTS(SELECT 1 FROM accounts)", [], |row| {
                row.get::<_, bool>(0)
            })?)
    }

    /// Private transport guard: human logins must not enter the AI RPC surface.
    pub fn authenticate_ai_session(&self, token: Option<&str>) -> Result<(), Fault> {
        match identity::authenticate(&self.db, token)? {
            Some(_) => Ok(()),
            None => Err(Fault::authentication()),
        }
    }
    pub fn open(root: &Path) -> Result<Self, Fault> {
        let mut db = store::open(root)?;
        policy::initialize(&mut db, root)?;
        crate::changes::initialize(&mut db)?;
        Ok(Self {
            db,
            root: root.to_path_buf(),
        })
    }

    /// Checks a persisted credential candidate without rotating, renewing, or consuming it.
    pub fn validate_credential_candidate(
        &mut self,
        candidate: &crate::credentials::CredentialCandidate,
    ) -> Result<crate::credentials::CredentialValidity, Fault> {
        crate::credentials::validate(&mut self.db, candidate)
    }

    /// Selects the persisted human session whose newly-created eligible Posts may be notified.
    /// Only the account ID, token hash and a selection generation are retained in SQLite.
    pub fn select_notification_recipient(
        &mut self,
        candidate: &crate::credentials::CredentialCandidate,
    ) -> Result<(), Fault> {
        crate::notifications::select_recipient(&mut self.db, candidate)
    }

    /// Clears the notification recipient and cancels every pending delivery.
    pub fn clear_notification_recipient(&mut self) -> Result<(), Fault> {
        crate::notifications::clear_recipient(&mut self.db)
    }

    /// Returns eligible pending notifications for the currently selected human session.
    pub fn prepare_notification_batch(
        &mut self,
        candidate: &crate::credentials::CredentialCandidate,
        now_ms: i64,
        window_ms: u32,
    ) -> Result<Option<crate::notification_batch::NotificationBatch>, Fault> {
        crate::notification_batch::prepare(&mut self.db, candidate, now_ms, window_ms)
    }

    pub fn complete_notification_batch(
        &mut self,
        candidate: &crate::credentials::CredentialCandidate,
        batch_id: &str,
        now_ms: i64,
        delivered: bool,
    ) -> Result<(), Fault> {
        crate::notification_batch::complete(&mut self.db, candidate, batch_id, now_ms, delivered)
    }

    /// Returns eligible pending notifications for the currently selected human session.
    pub fn prepare_notifications(
        &mut self,
        candidate: &crate::credentials::CredentialCandidate,
        limit: usize,
    ) -> Result<Vec<crate::notifications::NotificationDelivery>, Fault> {
        crate::notifications::prepare_deliveries(&mut self.db, candidate, limit)
    }

    /// Acknowledges one delivery for the matching selected account and generation.
    pub fn acknowledge_notification(
        &mut self,
        candidate: &crate::credentials::CredentialCandidate,
        delivery_id: &str,
    ) -> Result<(), Fault> {
        crate::notifications::acknowledge(&mut self.db, candidate, delivery_id)
    }

    /// Invalidates cursors after a trusted restore/replacement operation.
    /// This is a Rust API only; it is not exposed as an AI request.
    pub fn invalidate_change_cursors(&mut self) -> Result<(), Fault> {
        crate::changes::invalidate(&mut self.db)
    }

    /// Trusted runtime maintenance for persisted change and list cursors.
    /// Expired handles require callers to take a fresh snapshot.
    pub fn prune_cursor_metadata(&mut self, now_epoch_seconds: i64) -> Result<usize, Fault> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let removed = crate::cursor_metadata::prune(&tx, now_epoch_seconds)?
            + crate::thread_rules::prune(&tx)?;
        tx.commit()?;
        Ok(removed)
    }

    /// Trusted runtime-only backup primitive. Never expose the returned database
    /// through the AI transport. The caller owns protected storage and RULES capture.
    pub fn credential_free_snapshot(&self) -> Result<Connection, Fault> {
        crate::backup::snapshot(&self.db)
    }

    /// Captures only SQLite and registered RULES for the trusted backup owner.
    pub fn prepare_backup(&self) -> Result<crate::PreparedBackup, Fault> {
        crate::backup::prepare(&self.db, &self.root)
    }

    /// Trusted restore owner only. Never expose these journal values to GUI or AI.
    pub fn backup_live_ledger(&self) -> Result<std::collections::BTreeMap<String, String>, Fault> {
        crate::backup::live_ledger(&self.db)
    }

    /// Trusted restore owner holds the runtime gate and durable rollback journal.
    /// Build and validate the complete replacement before touching the live DB.
    pub fn install_restore(
        &mut self,
        input: &crate::RestoreInspection,
        ledger: &std::collections::BTreeMap<String, String>,
    ) -> Result<(), Fault> {
        let replacement = crate::backup::snapshot(&input.database)?;
        crate::ui_preferences::mark_restored(&replacement)?;
        for (key, value) in ledger {
            if key != "backup-schedule-v1" && !key.starts_with("backup-manual-v1:") {
                return Err(Fault::new(
                    "RESTORE_INVALID",
                    8,
                    "Invalid live backup history",
                ));
            }
            replacement.execute(
                "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
                params![key, value],
            )?;
        }
        if crate::backup::live_ledger(&replacement)? != *ledger {
            return Err(Fault::new(
                "RESTORE_INVALID",
                8,
                "Invalid live backup history",
            ));
        }
        // snapshot creates a new epoch every time, including rollback and
        // repeated restores of the same archive. Never import its old cursors.
        let backup = rusqlite::backup::Backup::new(&replacement, &mut self.db)?;
        if !matches!(backup.step(-1)?, rusqlite::backup::StepResult::Done) {
            return Err(Fault::new(
                "RESTORE_STORAGE",
                8,
                "Database replacement failed",
            ));
        }
        Ok(())
    }

    pub fn backup_settings(&self) -> Result<crate::BackupSettings, Fault> {
        crate::backup_schedule::settings(&self.db)
    }
    pub fn manual_backup(&self, intent: &str) -> Result<Option<crate::ManualBackup>, Fault> {
        crate::backup_manual::get(&self.db, intent)
    }
    pub fn manual_backups(&self) -> Result<Vec<crate::ManualBackup>, Fault> {
        crate::backup_manual::list(&self.db)
    }
    pub fn forget_manual_backup(&mut self, intent: &str) -> Result<bool, Fault> {
        let tx = self
            .db
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let removed = crate::backup_manual::forget(&tx, intent)?;
        tx.commit()?;
        Ok(removed)
    }
    pub fn set_manual_backup(&mut self, value: &crate::ManualBackup) -> Result<(), Fault> {
        crate::backup_manual::set(&self.db, value)
    }
    pub fn set_backup_settings(&mut self, value: &crate::BackupSettings) -> Result<(), Fault> {
        crate::backup_schedule::set_settings(&self.db, value)
    }
    pub fn backup_schedule(&self) -> Result<crate::BackupScheduleState, Fault> {
        crate::backup_schedule::state(&self.db)
    }
    pub fn set_backup_schedule(&mut self, value: &crate::BackupScheduleState) -> Result<(), Fault> {
        crate::backup_schedule::set_state(&self.db, value)
    }

    /// Trusted native desktop geometry, available before a human signs in.
    pub fn window_geometry_get(&self) -> Result<Option<crate::WindowGeometry>, Fault> {
        crate::device_settings::window_geometry(&self.db)
    }

    /// Trusted native desktop geometry, available before a human signs in.
    pub fn window_geometry_set(&mut self, value: &crate::WindowGeometry) -> Result<(), Fault> {
        crate::device_settings::set_window_geometry(&self.db, value)
    }

    /// Stable startup preference archived as data; applying it to the OS is
    /// always a separate trusted startup-management action.
    pub fn startup_preference(&self) -> Result<Option<bool>, Fault> {
        crate::device_settings::startup_preference(&self.db)
    }

    pub fn set_startup_preference(&mut self, enabled: bool) -> Result<(), Fault> {
        crate::device_settings::set_startup_preference(&self.db, enabled)
    }

    pub fn request(&mut self, request: Request) -> Result<Value, Fault> {
        self.request_inner(request, None, None)
    }

    /// Private adapters must durably persist a grant before its transaction
    /// commits. The sink must retain prior credential candidates during rotation;
    /// an encrypted candidate journal and startup validation are adapter duties.
    /// This path returns only public metadata, never the private grant.
    pub fn request_with_credential_sink(
        &mut self,
        request: Request,
        mut sink: impl FnMut(&Value) -> Result<(), Fault>,
    ) -> Result<Value, Fault> {
        if !matches!(
            request.operation.as_str(),
            "account.setup"
                | "account.login"
                | "account.password"
                | "account.recover"
                | "account.google.recover"
                | "identity.start"
                | "identity.resume"
        ) {
            return Err(Fault::new(
                "INVALID_INPUT",
                2,
                "Credential persistence is supported only for credential issuance",
            ));
        }
        self.request_inner(request, Some(&mut sink), None)
    }

    /// Issues a credential grant once for a private 256-bit intent key and
    /// stores only the sanitized public result in SQLite with the grant.
    pub fn request_with_credential_receipt(
        &mut self,
        request: Request,
        key: &str,
        mut sink: impl FnMut(&Value) -> Result<(), Fault>,
    ) -> Result<Value, Fault> {
        let metadata_key = credential_receipt_key(key)?;
        if !matches!(
            request.operation.as_str(),
            "account.setup"
                | "account.login"
                | "account.password"
                | "account.recover"
                | "account.google.recover"
                | "identity.start"
                | "identity.resume"
        ) {
            return Err(Fault::new(
                "INVALID_INPUT",
                2,
                "Credential persistence is supported only for credential issuance",
            ));
        }
        self.request_inner(request, Some(&mut sink), Some(&metadata_key))
    }

    /// Reads public issuance metadata by private intent key. The key itself is
    /// never stored in the database.
    pub fn credential_receipt(&self, key: &str) -> Result<Option<Value>, Fault> {
        let metadata_key = credential_receipt_key(key)?;
        credential_receipt_value(&self.db, &metadata_key)
    }

    fn request_inner(
        &mut self,
        request: Request,
        sink: Option<CredentialSink<'_>>,
        receipt_key: Option<&str>,
    ) -> Result<Value, Fault> {
        if request.operation.starts_with("account.google.") && !crate::google_recovery_available() {
            return Err(Fault::new(
                "FEATURE_UNAVAILABLE",
                2,
                "Google linking and recovery are unavailable in this beta",
            ));
        }
        // Immediate transactions serialize writes across connections and roll back all failed requests.
        // Cursor-producing change and list reads stay on the immediate
        // transaction path because issuing a private token mapping writes
        // app_metadata even though the public operation is a read.
        let read_only = matches!(
            request.operation.as_str(),
            "board.get"
                | "conversation.capabilities"
                | "conversation.deleted"
                | "conversation.pins"
                | "conversation.activity"
                | "conversation.subscriptions"
                | "conversation.unread"
                | "notification.settings.get"
                | "retention.settings.get"
                | "display.settings.get"
                | "ui.preferences.get"
                | "management.capabilities"
                | "authorization.capabilities"
                | "rules.capabilities"
                | "rules.get"
                | "rules.validate"
                | "search"
                | "rules.check"
                | "rules.subjects"
                | "thread.get"
                | "post.get"
                | "post.list"
                | "post.page"
                | "response.list"
                | "post.history"
                | "post.deleted_body"
                | "identity.status"
                | "project.get"
                | "project.list"
                | "account.status"
                | "account.get"
                | "account.list"
                | "role.list"
                | "permission.list"
        );
        // These reads can issue a session-bound Thread-rule acknowledgement.
        let rule_read = matches!(
            request.operation.as_str(),
            "thread.get" | "post.get" | "post.list" | "response.list" | "post.history"
        );
        let behavior = if read_only && !rule_read {
            TransactionBehavior::Deferred
        } else {
            TransactionBehavior::Immediate
        };
        let tx = self.db.transaction_with_behavior(behavior)?;
        if let Some(receipt_key) = receipt_key
            && credential_receipt_exists(&tx, receipt_key)?
        {
            return Err(Fault::new(
                "CONFLICT",
                6,
                "This credential issuance intent already has a receipt",
            ));
        }
        validate_arguments(&request)?;
        if request.operation.starts_with("account.")
            && identity::authenticate(&tx, request.auth.as_deref())?.is_some()
        {
            return Err(Fault::new(
                "PERMISSION_DENIED",
                4,
                "AI credentials cannot authorize human account operations",
            ));
        }
        let result = if request.operation == "account.setup" {
            accounts::setup(&tx, &request.args)?
        } else if request.operation == "account.login" {
            accounts::login(&tx, &request.args)?
        } else if request.operation == "account.recover"
            || request.operation == "account.google.recover"
            || (request.operation == "account.google.begin" && request.args["purpose"] == "recover")
        {
            if request.auth.is_some() {
                return Err(Fault::new(
                    "INVALID_INPUT",
                    2,
                    "Recovery does not accept a session credential",
                ));
            }
            accounts::dispatch(&tx, &request.operation, &request.args, "")?
        } else if request.operation == "identity.start" {
            let grant = identity::start(&tx, &request.args)?;
            policy::Policy::load(
                &tx,
                &self.root,
                grant["identity"]["id"]
                    .as_str()
                    .ok_or_else(Fault::internal)?,
            )?;
            grant
        } else if request.operation == "identity.resume" {
            let grant = identity::resume(&tx, &request.args)?;
            policy::Policy::load(
                &tx,
                &self.root,
                grant["identity"]["id"]
                    .as_str()
                    .ok_or_else(Fault::internal)?,
            )?;
            grant
        } else if let Some(actor) = identity::authenticate(&tx, request.auth.as_deref())? {
            if matches!(
                request.operation.as_str(),
                "rules.capabilities"
                    | "rules.get"
                    | "rules.validate"
                    | "rules.save"
                    | "rules.subjects"
                    | "retention.settings.get"
                    | "retention.settings.set"
            ) {
                return Err(Fault::new(
                    "PERMISSION_DENIED",
                    4,
                    "These settings operations are available only to an authenticated human",
                ));
            }
            let policy = policy::Policy::load(&tx, &self.root, &actor)?;
            if request.operation == "project.register" {
                if !policy.can_register_project(&request) {
                    return Err(Fault::new(
                        "PERMISSION_DENIED",
                        4,
                        "Project registration is not permitted by the effective global rules",
                    ));
                }
                projects::register_ai(&tx, &request, &actor)?
            } else {
                ai::dispatch(&tx, &request, &actor, &policy)?
            }
        } else {
            let actor = accounts::authenticate(&tx, request.auth.as_deref())?;
            if request.operation == "retention.settings.get" {
                crate::retention::get_settings(&tx, &actor)?
            } else if request.operation == "retention.settings.set" {
                crate::retention::set_settings(&tx, &actor, &request.args)?
            } else if matches!(
                request.operation.as_str(),
                "ui.preferences.get" | "ui.preferences.set"
            ) {
                crate::ui_preferences::dispatch(&tx, &request.operation, &actor, &request.args)?
            } else if request.operation.starts_with("account.")
                && request.operation != "account.logout"
            {
                human::require(&tx, &actor, &request.operation, None, None)?;
                accounts::dispatch(&tx, &request.operation, &request.args, &actor)?
            } else if request.operation.starts_with("role.")
                || request.operation.starts_with("permission.")
            {
                human::require(&tx, &actor, &request.operation, None, None)?;
                human::dispatch(&tx, &request.operation, &request.args, &actor)?
            } else {
                human_routes::dispatch(&tx, &request, &actor, &self.root)?
            }
        };
        let result = if let Some(sink) = sink {
            sink(&result)?;
            let public = match request.operation.as_str() {
                "identity.start" | "identity.resume" => {
                    json!({"identity":result["identity"],"session_id":result["session_id"]})
                }
                "account.setup" => json!({"id":result["id"],"name":result["name"]}),
                "account.password" => json!({"account":result["account"]}),
                _ => json!({"id":result["id"]}),
            };
            if let Some(receipt_key) = receipt_key {
                let encoded = serde_json::to_string(&public).map_err(|_| Fault::internal())?;
                tx.execute(
                    "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
                    params![receipt_key, encoded],
                )?;
            }
            public
        } else {
            result
        };
        tx.commit()?;
        let reclaim_after_commit =
            matches!(request.operation.as_str(), "board.purge" | "thread.purge")
                || (request.operation == "conversation.batch" && request.args["action"] == "purge");
        let storage_reclaimed = reclaim_after_commit
            && crate::trash::reclaim_pending(&self.db)?
            && reclaim_sqlite_space(&self.db);
        let mut result = result;
        if reclaim_after_commit && let Some(object) = result.as_object_mut() {
            object.insert("storage_reclaimed".into(), Value::Bool(storage_reclaimed));
        }
        Ok(result)
    }
}
fn reclaim_sqlite_space(db: &Connection) -> bool {
    let checkpoint = || -> rusqlite::Result<i64> {
        db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))
    };
    if checkpoint() != Ok(0) {
        return false;
    }
    if db.execute_batch("VACUUM;").is_err() {
        return false;
    }
    if !checkpoint().is_ok_and(|busy| busy == 0) {
        return false;
    }
    if db
        .execute(
            "DELETE FROM app_metadata WHERE key='storage.reclaim_pending'",
            [],
        )
        .is_err()
    {
        return false;
    }
    if checkpoint().is_ok_and(|busy| busy == 0) {
        true
    } else {
        // Keep the retry request durable if marker removal could not be
        // checkpointed after contraction.
        let _ = db.execute(
            "INSERT OR REPLACE INTO app_metadata(key,value) VALUES('storage.reclaim_pending','1')",
            [],
        );
        false
    }
}

pub(crate) fn dispatch(db: &Connection, r: &Request, actor: &str) -> Result<Value, Fault> {
    let args = &r.args;
    let creating = matches!(
        r.operation.as_str(),
        "board.create" | "thread.create" | "post.create" | "response.create"
    );
    let mut fingerprint_args = args.clone();
    if matches!(r.operation.as_str(), "post.create" | "response.create")
        && let Some(object) = fingerprint_args.as_object_mut()
    {
        object.remove("thread_rules_token");
    }
    let fingerprint = accounts::digest(
        &serde_json::to_string(&json!({"operation":r.operation,"args":fingerprint_args}))
            .map_err(|_| Fault::internal())?,
    );
    if let Some(key) = &r.request_id {
        if !creating || key.trim().is_empty() || key.len() > 256 {
            return Err(Fault::new(
                "INVALID_INPUT",
                2,
                "Request ID is supported only for creation and must be 1–256 bytes",
            ));
        }
        // Records expire after seven days. Store only the identity, never a body-bearing cached response.
        db.execute(
            "DELETE FROM create_requests WHERE julianday(created_at)<=julianday('now','-7 days')",
            [],
        )?;
        let receipt: Option<(String,String)>=db.query_row("SELECT fingerprint,entity_id FROM create_requests WHERE actor_id=?1 AND request_id=?2",
            params![actor,key],|row| Ok((row.get(0)?,row.get(1)?))).optional()?;
        if let Some((prior, id)) = receipt {
            if prior != fingerprint {
                return Err(Fault::new(
                    "REQUEST_CONFLICT",
                    6,
                    "Request ID was used with different content",
                ));
            }
            return match r.operation.as_str() {
                "board.create" => board_get(db, &id, true),
                "thread.create" => thread_get(db, &id, true),
                _ => {
                    let (thread, no): (String, i64) =
                        db.query_row("SELECT thread_id,no FROM posts WHERE id=?1", [id], |row| {
                            Ok((row.get(0)?, row.get(1)?))
                        })?;
                    post_get(db, &thread, no)
                }
            };
        }
    }
    let result = match r.operation.as_str() {
        "project.register" | "project.get" | "project.list" | "project.alias" | "project.root" => {
            projects::dispatch(db, &r.operation, args)?
        }
        "account.logout" => {
            db.execute(
                "DELETE FROM human_sessions WHERE token_hash=?1",
                [accounts::digest(
                    r.auth.as_deref().ok_or_else(Fault::authentication)?,
                )],
            )?;
            json!({"logged_out":true})
        }
        "board.create" | "thread.create" => {
            let name = name(args, &r.operation)?;
            let desc = description(args)?;
            let id = Uuid::new_v4().to_string();
            let now = timestamp();
            if r.operation == "board.create" {
                db.execute("INSERT INTO boards(id,name,description,creator_id,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?5)",params![id,name,desc,actor,now])?;
                change(db, &r.operation, &id, actor)?;
                board_get(db, &id, false)?
            } else {
                let board = required_text(args, "board_id")?;
                live_board(db, board)?;
                db.execute("INSERT INTO threads(id,board_id,name,description,creator_id,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6)",params![id,board,name,desc,actor,now])?;
                change(db, &r.operation, &id, actor)?;
                thread_get(db, &id, false)?
            }
        }
        "board.get" => board_get(db, required_text(args, "id")?, false)?,
        "thread.get" => thread_get(db, required_text(args, "id")?, false)?,
        "board.list" => {
            let mut query = db
                .prepare("SELECT id FROM boards WHERE deleted_at IS NULL ORDER BY created_at,id")?;
            let ids = query
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            json!({"items":ids.iter().map(|id| board_get(db,id,false)).collect::<Result<Vec<_>,_>>()?})
        }
        "thread.list" => {
            let board = required_text(args, "board_id")?;
            live_board(db, board)?;
            let mut query=db.prepare("SELECT id FROM threads WHERE board_id=?1 AND deleted_at IS NULL ORDER BY created_at,id")?;
            let ids = query
                .query_map([board], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            json!({"items":ids.iter().map(|id| thread_get(db,id,false)).collect::<Result<Vec<_>,_>>()?})
        }
        "board.edit" | "thread.edit" => {
            let id = required_text(args, "id")?;
            if args.get("name").is_none() && args.get("description").is_none() {
                return Err(Fault::new(
                    "INVALID_INPUT",
                    2,
                    "An edit requires a name or description",
                ));
            }
            let current = if r.operation == "board.edit" {
                board_get(db, id, false)?
            } else {
                thread_get(db, id, false)?
            };
            let name = if args.get("name").is_some() {
                name(args, &r.operation)?
            } else {
                current["name"].as_str().ok_or_else(Fault::internal)?
            };
            let desc = if args.get("description").is_some() {
                description(args)?
            } else {
                current["description"]
                    .as_str()
                    .ok_or_else(Fault::internal)?
            };
            let revision = integer(args, "if_revision")?;
            let table = if r.operation == "board.edit" {
                live_board(db, id)?;
                "boards"
            } else {
                live_thread(db, id)?;
                "threads"
            };
            let count=db.execute(&format!("UPDATE {table} SET name=?1,description=?2,revision=revision+1,updated_at=?3 WHERE id=?4 AND revision=?5"),
                params![name,desc,timestamp(),id,revision])?;
            if count != 1 {
                return Err(conflict());
            }
            change(db, &r.operation, id, actor)?;
            if table == "boards" {
                board_get(db, id, false)?
            } else {
                thread_get(db, id, false)?
            }
        }
        "board.delete" | "board.restore" | "thread.delete" | "thread.restore" => {
            let id = required_text(args, "id")?;
            let deleting = r.operation.ends_with(".delete");
            let table = if r.operation.starts_with("board.") {
                board_get(db, id, true)?;
                "boards"
            } else {
                let thread = thread_get(db, id, true)?;
                live_board(db, thread["board_id"].as_str().ok_or_else(Fault::internal)?)?;
                "threads"
            };
            let deleted = if deleting { Some(timestamp()) } else { None };
            let changed=db.execute(&format!("UPDATE {table} SET deleted_at=?1,updated_at=?2,revision=revision+1 WHERE id=?3 AND (deleted_at IS NULL)=?4"),
                params![deleted,timestamp(),id,deleting as i32])?;
            if changed > 0 {
                change(db, &r.operation, id, actor)?;
            }
            json!({"id":id,"deleted":deleting})
        }
        "post.create" | "response.create" => {
            let thread = required_text(args, "thread_id")?;
            live_thread(db, thread)?;
            crate::thread_rules::require(db, r, actor, thread)?;
            let next_no: i64 =
                db.query_row("SELECT next_no FROM threads WHERE id=?1", [thread], |row| {
                    row.get(0)
                })?;
            if next_no > 1000 {
                return Err(Fault::new(
                    "THREAD_FULL",
                    6,
                    "This Thread has reached the limit of 1000 Posts",
                ));
            }
            let body = body(args)?;
            if r.operation == "post.create" && args.get("to").is_some() {
                return Err(Fault::new(
                    "INVALID_INPUT",
                    2,
                    "Use response.create for a formal reply",
                ));
            }
            let target = if r.operation == "response.create" {
                let no = integer(args, "to")?;
                let target = post_get(db, thread, no)?;
                if target["deleted"] == true {
                    return Err(not_found());
                }
                Some(no)
            } else {
                None
            };
            let id = Uuid::new_v4().to_string();
            let no: i64 = db.query_row(
                "UPDATE threads SET next_no=next_no+1 WHERE id=?1 RETURNING next_no-1",
                [thread],
                |row| row.get(0),
            )?;
            let (model, display) = author_display(db, actor)?;
            let now = timestamp();
            db.execute("INSERT INTO posts(id,thread_id,no,body,author_id,model_name,display_id,created_at,updated_at,reply_to) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8,?9)",
                params![id,thread,no,body,actor,model,display,now,target])?;
            if no == 1000 {
                db.execute(
                    "UPDATE threads SET archived_at=COALESCE(archived_at,?1) WHERE id=?2",
                    params![now, thread],
                )?;
            }
            let harness = identity::harness(db, actor)?;
            db.execute(
                "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
                params![format!("post-harness-v1:{id}"), harness],
            )?;
            change(db, &r.operation, &id, actor)?;
            notifications::enqueue_created_post(db, &id)?;
            post_get(db, thread, no)?
        }
        "post.get" => post_get(db, required_text(args, "thread_id")?, integer(args, "no")?)?,
        "post.page" => {
            let thread = required_text(args, "thread_id")?;
            live_thread(db, thread)?;
            let page_size = match args.get("page_size") {
                None => 50,
                Some(value) => value
                    .as_i64()
                    .filter(|n| [50, 100, 500, 1000].contains(n))
                    .ok_or_else(|| {
                        Fault::new(
                            "INVALID_INPUT",
                            2,
                            "Page size must be 50, 100, 500, or 1000",
                        )
                    })?,
            };
            let requested_page = if args.get("page").is_some() {
                integer(args, "page")?
            } else {
                1
            };
            let total_posts: i64 = db.query_row(
                "SELECT COUNT(*) FROM posts WHERE thread_id=?1",
                [thread],
                |row| row.get(0),
            )?;
            let total_pages = if total_posts == 0 {
                1
            } else {
                (total_posts - 1) / page_size + 1
            };
            let page = if args.get("anchor_no").is_some() {
                let anchor = integer(args, "anchor_no")?;
                post_get(db, thread, anchor)?;
                let preceding: i64 = db.query_row(
                    "SELECT COUNT(*) FROM posts WHERE thread_id=?1 AND no<?2",
                    params![thread, anchor],
                    |row| row.get(0),
                )?;
                preceding / page_size + 1
            } else {
                requested_page.min(total_pages)
            };
            let offset = (page - 1) * page_size;
            let mut query = db.prepare(
                "SELECT no FROM posts WHERE thread_id=?1 ORDER BY no ASC LIMIT ?2 OFFSET ?3",
            )?;
            let nos = query
                .query_map(params![thread, page_size, offset], |row| {
                    row.get::<_, i64>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let items = nos
                .into_iter()
                .map(|no| post_get(db, thread, no))
                .collect::<Result<Vec<_>, _>>()?;
            json!({"items":items,"page":page,"page_size":page_size,"total_posts":total_posts,"total_pages":total_pages})
        }
        "post.list" | "response.list" => {
            let thread = required_text(args, "thread_id")?;
            live_thread(db, thread)?;
            let limit = match args.get("limit") {
                None => 50,
                Some(v) => v
                    .as_i64()
                    .filter(|n| (1..=200).contains(n))
                    .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "Limit must be 1–200"))?,
            };
            let before = if args.get("before_no").is_some() {
                integer(args, "before_no")?
            } else {
                i64::MAX
            };
            let target = if r.operation == "response.list" {
                Some(integer(args, "to")?)
            } else {
                None
            };
            let mut query=db.prepare("SELECT no FROM posts WHERE thread_id=?1 AND no<?2 AND (?3 IS NULL OR reply_to=?3) ORDER BY no DESC LIMIT ?4")?;
            let mut nos = query
                .query_map(params![thread, before, target, limit + 1], |row| {
                    row.get::<_, i64>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?;
            let more = nos.len() > limit as usize;
            nos.truncate(limit as usize);
            let next = if more { nos.last().copied() } else { None };
            nos.reverse();
            let items = nos
                .into_iter()
                .map(|no| post_get(db, thread, no))
                .collect::<Result<Vec<_>, _>>()?;
            json!({"items":items,"next_before_no":next})
        }
        "post.edit" => {
            crate::thread_rules::require(db, r, actor, required_text(args, "thread_id")?)?;
            let thread = required_text(args, "thread_id")?;
            let no = integer(args, "no")?;
            let body = body(args)?;
            let revision = integer(args, "if_revision")?;
            let current = post_get(db, thread, no)?;
            if current["deleted"] == true {
                return Err(not_found());
            }
            if current["revision"] != revision {
                return Err(conflict());
            }
            let id = current["id"].as_str().ok_or_else(Fault::internal)?;
            let (model, display) = author_display(db, actor)?;
            let now = timestamp();
            db.execute("INSERT INTO post_revisions(post_id,revision,body,saved_at,editor_id,editor_display_id,editor_model_name) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![id,revision,current["body"].as_str(),now,actor,display,model])?;
            db.execute("UPDATE posts SET body=?1,revision=revision+1,updated_at=?2,edited_by=?3,editor_display_id=?4,editor_model_name=?5 WHERE id=?6 AND revision=?7",
                params![body,now,actor,display,model,id,revision])?;
            change(db, &r.operation, id, actor)?;
            post_get(db, thread, no)?
        }
        "post.history" => {
            let thread = required_text(args, "thread_id")?;
            let current = post_get(db, thread, integer(args, "no")?)?;
            let mut query=db.prepare("SELECT revision,body,saved_at,editor_id,editor_display_id,editor_model_name FROM post_revisions WHERE post_id=?1 ORDER BY revision")?;
            let items=query.query_map([current["id"].as_str().ok_or_else(Fault::internal)?],|row| Ok(json!({
                "revision":row.get::<_,i64>(0)?,"body":row.get::<_,String>(1)?,"saved_at":row.get::<_,String>(2)?,
                "editor_id":row.get::<_,String>(3)?,"editor_display_id":row.get::<_,String>(4)?,"editor_model_name":row.get::<_,String>(5)?
            })))?.collect::<Result<Vec<_>,_>>()?;
            json!({"items":items})
        }
        "post.deleted_body" => {
            let current = post_get(db, required_text(args, "thread_id")?, integer(args, "no")?)?;
            if current["deleted"] != true {
                return Err(Fault::new("INVALID_INPUT", 2, "Post is not deleted"));
            }
            if current["restore_available"] == false {
                return Err(Fault::new(
                    "CONTENT_PURGED",
                    5,
                    "Deleted body retention has expired",
                ));
            }
            let body: String = db.query_row(
                "SELECT body FROM posts WHERE id=?1",
                [current["id"].as_str().ok_or_else(Fault::internal)?],
                |row| row.get(0),
            )?;
            json!({"id":current["id"],"body":body})
        }
        "post.delete" | "post.restore" => {
            let thread = required_text(args, "thread_id")?;
            let no = integer(args, "no")?;
            let current = post_get(db, thread, no)?;
            let deleting = r.operation == "post.delete";
            if !deleting && current["restore_available"] == false {
                return Err(Fault::new(
                    "CONTENT_PURGED",
                    5,
                    "Deleted body retention has expired; restoration is unavailable",
                ));
            }
            if current["deleted"] != deleting {
                let id = current["id"].as_str().ok_or_else(Fault::internal)?;
                let deleted = if deleting { Some(timestamp()) } else { None };
                db.execute(
                    "UPDATE posts SET deleted_at=?1,updated_at=?2,revision=revision+1 WHERE id=?3",
                    params![deleted, timestamp(), id],
                )?;
                change(db, &r.operation, id, actor)?;
            }
            post_get(db, thread, no)?
        }
        _ => {
            return Err(Fault::new(
                "UNKNOWN_OPERATION",
                2,
                "Operation is not supported",
            ));
        }
    };
    if creating && let Some(key) = &r.request_id {
        let id = result["id"].as_str().ok_or_else(Fault::internal)?;
        db.execute("INSERT INTO create_requests(actor_id,request_id,fingerprint,operation,entity_id,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
            params![actor,key,fingerprint,r.operation,id,timestamp()])?;
    }
    Ok(result)
}
