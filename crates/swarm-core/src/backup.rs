//! SQLite and registered RULES preparation. No transport route or restore.
use crate::{changes, protocol::Fault};
use rusqlite::{
    Connection,
    backup::{Backup, StepResult},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupRulesSnapshot {
    pub project_id: Option<String>,
    pub source_path: String,
    pub content: Option<String>,
    pub revision: String,
}

pub struct PreparedBackup {
    pub(crate) database: Connection,
    pub(crate) rules: Vec<BackupRulesSnapshot>,
}

impl PreparedBackup {
    pub fn rules(&self) -> &[BackupRulesSnapshot] {
        &self.rules
    }

    pub fn content_revision(&self) -> Result<String, Fault> {
        use sha2::{Digest, Sha256};
        let bytes = self.database.serialize(rusqlite::MAIN_DB)?;
        crate::backup_archive::content_revision(
            &format!("{:x}", Sha256::digest(&*bytes)),
            &self.rules,
        )
    }
}

/// Exact live journals for a protected restore preparation, never archive input.
pub(crate) fn live_ledger(
    source: &Connection,
) -> Result<std::collections::BTreeMap<String, String>, Fault> {
    crate::backup_schedule::state(source)?;
    let mut ledger = std::collections::BTreeMap::new();
    let mut size = 0usize;
    let mut statement = source.prepare("SELECT key,value FROM app_metadata WHERE key='backup-schedule-v1' OR key LIKE 'backup-manual-v1:%' ORDER BY key")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        if let Some(intent) = key.strip_prefix("backup-manual-v1:") {
            crate::backup_manual::get(source, intent)?.ok_or_else(invalid)?;
        }
        size = size.saturating_add(key.len()).saturating_add(value.len());
        if size > 48 * 1024 {
            return Err(Fault::new(
                "RESTORE_SIZE",
                8,
                "Live backup history exceeds restore preparation storage",
            ));
        }
        ledger.insert(key, value);
    }
    Ok(ledger)
}

pub(crate) fn prepare(
    source: &Connection,
    root: &std::path::Path,
) -> Result<PreparedBackup, Fault> {
    prepare_observed(source, root, || {})
}

fn prepare_observed(
    source: &Connection,
    root: &std::path::Path,
    captured: impl FnOnce(),
) -> Result<PreparedBackup, Fault> {
    let database = snapshot(source)?;
    // Registered roots come from this snapshot, not a second live DB read.
    let projects = database
        .prepare("SELECT id,canonical_root FROM projects ORDER BY id")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut paths = vec![(None, root.join("RULES.md"))];
    paths.extend(
        projects
            .into_iter()
            .map(|(id, root)| (Some(id), std::path::PathBuf::from(root).join("RULES.md"))),
    );
    let mut rules = Vec::new();
    for (project_id, path) in &paths {
        let (content, revision) = crate::rules_editor::read_for_backup(path, project_id.is_some())?;
        let absent = revision == "missing:v1";
        rules.push(BackupRulesSnapshot {
            project_id: project_id.clone(),
            source_path: path
                .parent()
                .ok_or_else(invalid)?
                .canonicalize()
                .map_err(|_| invalid())?
                .join("RULES.md")
                .to_str()
                .ok_or_else(invalid)?
                .to_owned(),
            content: if absent { None } else { Some(content) },
            revision,
        });
    }
    captured();
    // Files are outside SQLite. Detect changes during capture rather than
    // claiming a cross-filesystem transaction; do not silently omit a file.
    for ((project_id, path), captured) in paths.iter().zip(&rules) {
        let (_, revision) = crate::rules_editor::read_for_backup(path, project_id.is_some())?;
        if revision != captured.revision {
            return Err(Fault::new(
                "RULES_CHANGED",
                6,
                "RULES changed during backup; retry the backup",
            ));
        }
    }
    Ok(PreparedBackup { database, rules })
}

fn invalid() -> Fault {
    Fault::new("BACKUP_STATE", 8, "Database snapshot could not be verified")
}

fn validate(db: &Connection) -> Result<(), Fault> {
    let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if !matches!(version, 7..=9) {
        return Err(invalid());
    }
    let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if integrity != "ok" || db.prepare("PRAGMA foreign_key_check")?.exists([])? {
        return Err(invalid());
    }
    Ok(())
}

pub(crate) fn snapshot(source: &Connection) -> Result<Connection, Fault> {
    // The unsanitized image never goes to a staging file. A single SQLite backup
    // step captures a consistent image including committed WAL pages. Do not
    // retry BUSY forever while the runtime's service gate is held.
    let mut copy = Connection::open_in_memory()?;
    copy.execute_batch("PRAGMA temp_store=MEMORY;")?;
    {
        let backup = Backup::new(source, &mut copy)?;
        if !matches!(backup.step(-1)?, StepResult::Done) {
            return Err(invalid());
        }
    }
    validate(&copy)?;
    crate::device_settings::validate_backup_metadata(&copy).map_err(|_| invalid())?;
    copy.execute_batch(
        "PRAGMA temp_store=MEMORY; PRAGMA secure_delete=ON; PRAGMA foreign_keys=ON;",
    )?;
    let tx = copy.transaction()?;
    tx.execute_batch("DELETE FROM human_sessions;
        DELETE FROM ai_sessions;
        UPDATE ai_identities SET resume_hash='';
        DELETE FROM notification_selection;
        DELETE FROM notification_outbox;
        DELETE FROM app_metadata WHERE key LIKE 'google-recovery-%' OR key LIKE 'credential-receipt:%' OR key LIKE 'backup-manual-v1:%' OR key IN ('notification-batch-v1','backup-schedule-v1');")?;
    tx.commit()?;
    changes::invalidate(&mut copy)?;
    // Purge freed pages, not just live SQL rows, before callers may persist it.
    copy.execute_batch("VACUUM;")?;
    validate(&copy)?;
    Ok(copy)
}

#[cfg(test)]
mod tests {
    #[test]
    fn changed_rules_during_capture_reject_the_whole_backup() {
        let root = tempfile::tempdir().unwrap();
        let mut db = crate::store::open(root.path()).unwrap();
        crate::policy::initialize(&mut db, root.path()).unwrap();
        crate::changes::initialize(&mut db).unwrap();
        let error = super::prepare_observed(&db, root.path(), || {
            std::fs::write(root.path().join("RULES.md"), "# changed externally").unwrap();
        })
        .err()
        .unwrap();
        assert_eq!(error.code, "RULES_CHANGED");
        assert_eq!(
            std::fs::read_to_string(root.path().join("RULES.md")).unwrap(),
            "# changed externally"
        );
    }
}
