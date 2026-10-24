//! Isolated input inspection only; no restore authorization or application.
use crate::{BackupManifest, protocol::Fault};
use rusqlite::{Connection, config::DbConfig};
use std::{
    io::{Cursor, Read},
    path::Path,
};

/// Inspection deliberately does not expose a database handle or apply operation.
pub struct RestoreInspection {
    manifest: BackupManifest,
    pub(crate) database: Connection,
}

impl RestoreInspection {
    pub fn manifest(&self) -> &BackupManifest {
        &self.manifest
    }
}

fn invalid() -> Fault {
    Fault::new("RESTORE_INVALID", 8, "Restore input could not be verified")
}

/// Validates the exact bytes that are opened, entirely in memory. Archive limits
/// apply; payload buffering and SQLite deserialization can each use up to 1 GiB.
/// A checksum is not source authenticity or permission to restore.
pub fn inspect_restore_archive(reader: &mut impl Read) -> Result<RestoreInspection, Fault> {
    inspect(reader).map_err(|_| invalid())
}

fn inspect(reader: &mut impl Read) -> Result<RestoreInspection, Fault> {
    let manifest = crate::backup_archive::read_manifest(reader)?;
    let len = usize::try_from(manifest.database_bytes).map_err(|_| invalid())?;
    let mut bytes = Vec::new();
    let mut chunk = [0; 64 * 1024];
    while bytes.len() < len {
        let count = (len - bytes.len()).min(chunk.len());
        reader
            .read_exact(&mut chunk[..count])
            .map_err(|_| invalid())?;
        bytes.try_reserve(count).map_err(|_| invalid())?;
        bytes.extend_from_slice(&chunk[..count]);
    }
    let mut tail = [0];
    if reader.read(&mut tail).map_err(|_| invalid())? != 0 {
        return Err(invalid());
    }
    crate::backup_archive::inspect_database(&mut Cursor::new(&bytes), &manifest)?;
    let mut db = Connection::open_in_memory()?;
    db.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
    db.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA temp_store=MEMORY; PRAGMA query_only=ON;")?;
    // The complete buffered cursor avoids partial-read failures in the library's
    // allocation API, and cannot change between checksum and deserialization.
    db.deserialize_read_exact(rusqlite::MAIN_DB, Cursor::new(&bytes), len, true)?;
    drop(bytes);
    let mut reference = Connection::open_in_memory()?;
    crate::store::migrate_to(&mut reference, manifest.schema_version)?;
    reference.execute_batch("VACUUM;")?;
    if objects(&db)? != objects(&reference)? {
        return Err(invalid());
    }
    let version: u32 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if version != manifest.schema_version
        || integrity != "ok"
        || db.prepare("PRAGMA foreign_key_check")?.exists([])?
    {
        return Err(invalid());
    }
    for table in [
        "human_sessions",
        "ai_sessions",
        "notification_selection",
        "notification_outbox",
    ] {
        if db
            .prepare(&format!("SELECT 1 FROM {table} LIMIT 1"))?
            .exists([])?
        {
            return Err(invalid());
        }
    }
    if db.prepare("SELECT 1 FROM ai_identities WHERE resume_hash<>'' LIMIT 1")?.exists([])?
        || db.prepare("SELECT 1 FROM app_metadata WHERE key LIKE 'google-recovery-%' OR key LIKE 'credential-receipt:%' OR key LIKE 'backup-manual-v1:%' OR key IN ('notification-batch-v1','backup-schedule-v1') LIMIT 1")?.exists([])? {
        return Err(invalid());
    }
    crate::device_settings::validate_backup_metadata(&db).map_err(|_| invalid())?;
    let projects = db
        .prepare("SELECT id,canonical_root FROM projects ORDER BY id")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    if manifest.rules.len() != projects.len() + 1 {
        return Err(invalid());
    }
    for rules in &manifest.rules {
        let path = Path::new(&rules.source_path);
        if !path.is_absolute()
            || path.file_name().and_then(|name| name.to_str()) != Some("RULES.md")
        {
            return Err(invalid());
        }
        if let Some(id) = &rules.project_id {
            let Some((_, root)) = projects.iter().find(|(project, _)| project == id) else {
                return Err(invalid());
            };
            if !Path::new(root).is_absolute() || Path::new(root).join("RULES.md") != path {
                return Err(invalid());
            }
        }
    }
    // Validate the exact legacy schema before any trusted migration. Preserve
    // read-only deserialization; migrate a bounded private in-memory copy.
    if version == 7 {
        let mut upgraded = Connection::open_in_memory()?;
        upgraded.execute_batch("PRAGMA temp_store=MEMORY;")?;
        {
            let copy = rusqlite::backup::Backup::new(&db, &mut upgraded)?;
            if !matches!(copy.step(-1)?, rusqlite::backup::StepResult::Done) {
                return Err(invalid());
            }
        }
        crate::store::migrate(&mut upgraded)?;
        db = upgraded;
    }
    Ok(RestoreInspection {
        manifest,
        database: db,
    })
}

type SchemaObject = (String, String, String, Option<String>);

fn objects(db: &Connection) -> Result<Vec<SchemaObject>, Fault> {
    // Include autoindexes and triggers as well as tables. Never run SQL read
    // from the archive to recreate its schema.
    Ok(db
        .prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    #[test]
    fn exact_legacy_archive_migrates_privately_without_truncating_existing_content() {
        let root = tempfile::tempdir().unwrap();
        let service = crate::Service::open(root.path()).unwrap();
        let rules = service.prepare_backup().unwrap().rules().to_vec();
        let mut database = Connection::open_in_memory().unwrap();
        crate::store::migrate_to(&mut database, 7).unwrap();
        database.execute_batch("INSERT INTO actors VALUES('author','human'); INSERT INTO boards(id,name,description,creator_id,created_at,updated_at) VALUES('board','Legacy','','author','2020-01-01','2020-01-01'); INSERT INTO threads(id,board_id,name,description,creator_id,created_at,updated_at,next_no) VALUES('thread','board','Legacy','','author','2020-01-01','2020-01-01',2);").unwrap();
        let name = "見出し".repeat(100);
        let body = "🦀".repeat(100_001);
        database
            .execute("UPDATE boards SET name=?1", [&name])
            .unwrap();
        database
            .execute("UPDATE threads SET name=?1", [&name])
            .unwrap();
        database.execute("INSERT INTO posts(id,thread_id,no,body,author_id,model_name,display_id,created_at,updated_at) VALUES('post','thread',1,?1,'author','USER','legacy','2020-01-01','2020-01-01')", [&body]).unwrap();
        let prepared = crate::backup::PreparedBackup { database, rules };
        let mut archive = Vec::new();
        prepared
            .write_archive(&mut archive, crate::BackupKind::Manual)
            .unwrap();
        let before = archive.clone();
        let restored = inspect_restore_archive(&mut Cursor::new(&archive)).unwrap();
        assert_eq!(restored.manifest().schema_version, 7);
        assert_eq!(
            restored
                .database
                .query_row("PRAGMA user_version", [], |row| row.get::<_, u32>(0))
                .unwrap(),
            9
        );
        assert_eq!(
            restored
                .database
                .query_row("SELECT body FROM posts", [], |row| row.get::<_, String>(0))
                .unwrap(),
            body
        );
        assert_eq!(
            restored
                .database
                .query_row("SELECT name FROM boards", [], |row| row.get::<_, String>(0))
                .unwrap(),
            name
        );
        assert_eq!(
            restored
                .database
                .query_row("SELECT name FROM threads", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            name
        );
        assert!(
            restored
                .database
                .query_row("SELECT body_purged_at FROM posts", [], |row| row
                    .get::<_, Option<String>>(0))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            restored
                .database
                .query_row(
                    "SELECT history_days,deleted_days FROM retention_settings",
                    [],
                    |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?))
                )
                .unwrap(),
            (None, None)
        );
        assert_eq!(archive, before);
    }
}
