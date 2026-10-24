use crate::protocol::Fault;
use rusqlite::Connection;
use std::{path::Path, time::Duration};

pub(crate) fn open(root: &Path) -> Result<Connection, Fault> {
    std::fs::create_dir_all(root).map_err(|_| Fault::internal())?;
    let mut db = Connection::open(root.join("swarm.sqlite3"))?;
    db.busy_timeout(Duration::from_secs(5))?;
    db.execute_batch("PRAGMA foreign_keys=OFF; PRAGMA journal_mode=WAL;")?;
    migrate(&mut db)?;
    Ok(db)
}

pub(crate) fn migrate(db: &mut Connection) -> Result<(), Fault> {
    migrate_to(db, 9)
}

/// Migrate to a supported schema boundary. Restore inspection uses version 7
/// as a trusted compatibility reference before it migrates an archive to 9.
pub(crate) fn migrate_to(db: &mut Connection, target: u32) -> Result<(), Fault> {
    if !matches!(target, 7..=9) {
        return Err(Fault::new(
            "UNSUPPORTED_SCHEMA",
            8,
            "Requested database schema version is not supported",
        ));
    }
    let tx = db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let version: u32 = tx.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > target {
        return Err(Fault::new(
            "UNSUPPORTED_SCHEMA",
            8,
            "Database schema is newer than this application",
        ));
    }
    if version == 0 {
        tx.execute_batch(include_str!("schema.sql"))?;
        tx.execute_batch("PRAGMA user_version=1;")?;
    }
    if version <= 1 {
        tx.execute_batch(include_str!("migrations/002_actors.sql"))?;
        tx.execute_batch("PRAGMA user_version=2;")?;
    }
    if version <= 2 {
        tx.execute_batch(include_str!("migrations/003_policy.sql"))?;
        tx.execute_batch("PRAGMA user_version=3;")?;
    }
    if version <= 3 {
        tx.execute_batch(include_str!("migrations/004_humans.sql"))?;
        tx.execute_batch("PRAGMA user_version=4;")?;
    }
    if version <= 4 {
        tx.execute_batch(include_str!("migrations/005_pins.sql"))?;
        tx.execute_batch("PRAGMA user_version=5;")?;
    }
    if version <= 5 {
        tx.execute_batch(include_str!("migrations/006_notifications.sql"))?;
        tx.execute_batch("PRAGMA user_version=6;")?;
    }
    if version <= 6 {
        tx.execute_batch(include_str!("migrations/007_notification_outbox.sql"))?;
        tx.execute_batch("PRAGMA user_version=7;")?;
    }
    if target >= 8 && version <= 7 {
        tx.execute_batch(include_str!("migrations/008_retention.sql"))?;
        tx.execute_batch("PRAGMA user_version=8;")?;
    }
    if target >= 9 && version <= 8 {
        tx.execute_batch(include_str!("migrations/009_trash_retention.sql"))?;
        tx.execute_batch("PRAGMA user_version=9;")?;
    }
    let integrity = tx.prepare("PRAGMA foreign_key_check")?.exists([])?;
    if integrity {
        return Err(Fault::new(
            "MIGRATION_INTEGRITY",
            8,
            "Database foreign keys failed validation; migration was rolled back",
        ));
    }
    tx.commit()?;
    db.execute_batch("PRAGMA foreign_keys=ON;")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::migrate_to;
    use rusqlite::Connection;

    #[test]
    fn schema_seven_reference_stays_distinct_from_schema_eight_retention() {
        let mut db = Connection::open_in_memory().unwrap();
        migrate_to(&mut db, 7).unwrap();
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            7
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='retention_settings'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );

        migrate_to(&mut db, 8).unwrap();
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            8
        );
        let settings: (Option<i64>, Option<i64>) = db
            .query_row(
                "SELECT history_days,deleted_days FROM retention_settings WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(settings, (None, None));
        assert!(
            db.prepare("SELECT body_purged_at FROM posts LIMIT 0")
                .is_ok()
        );
        assert!(
            db.prepare("SELECT archived_at FROM threads LIMIT 0")
                .is_err()
        );
        assert!(
            db.prepare("SELECT deleted_board_days FROM retention_settings LIMIT 0")
                .is_err()
        );

        migrate_to(&mut db, 9).unwrap();
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            9
        );
        let settings: (Option<i64>, Option<i64>, Option<i64>) = db
            .query_row(
                "SELECT deleted_board_days,deleted_thread_days,archived_thread_days \
                 FROM retention_settings WHERE singleton=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(settings, (None, None, None));
        assert!(
            db.prepare("SELECT archived_at FROM threads LIMIT 0")
                .is_ok()
        );
    }

    #[test]
    fn schema_eight_migration_backfills_archive_boundary_time() {
        let mut db = Connection::open_in_memory().unwrap();
        migrate_to(&mut db, 8).unwrap();
        db.execute_batch(
            "INSERT INTO accounts(id,name,password_hash,recovery_hash,is_admin,created_at,disabled)
             VALUES('human','Human','unused','unused',1,'2026-01-01T00:00:00Z',0);
             INSERT INTO actors(id,kind) VALUES('human','human');
             INSERT INTO boards(id,name,description,creator_id,created_at,updated_at)
             VALUES('board','Board','','human','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');
             INSERT INTO threads(id,board_id,name,description,creator_id,next_no,created_at,updated_at)
             VALUES('archived','board','Archived','','human',1001,'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');
             INSERT INTO posts(id,thread_id,no,body,author_id,model_name,display_id,created_at,updated_at)
             VALUES('boundary','archived',1000,'body','human','human','display','2026-04-02T03:04:05Z','2026-04-02T03:04:05Z');",
        )
        .unwrap();

        migrate_to(&mut db, 9).unwrap();
        let archived_at: String = db
            .query_row(
                "SELECT archived_at FROM threads WHERE id='archived'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(archived_at, "2026-04-02T03:04:05Z");
    }
}
