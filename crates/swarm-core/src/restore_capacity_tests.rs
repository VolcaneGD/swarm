//! SQLite page-limit failures without filling the host filesystem.
use super::*;
use crate::{BackupKind, RestoreInspection, inspect_restore_archive};

fn inspection(service: &Service) -> RestoreInspection {
    let mut archive = Vec::new();
    service
        .prepare_backup()
        .unwrap()
        .write_archive(&mut archive, BackupKind::Manual)
        .unwrap();
    inspect_restore_archive(&mut std::io::Cursor::new(archive)).unwrap()
}
fn epoch(service: &Service) -> String {
    service
        .db
        .query_row(
            "SELECT value FROM app_metadata WHERE key='changes.cursor_epoch.v1'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn sqlite_full_during_database_replacement_preserves_live_image_and_allows_safety_restore() {
    let root = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let setup = service
        .request(Request {
            operation: "account.setup".into(),
            auth: None,
            args: json!({"name":"Admin","password":"fixture-password"}),
            context: json!({}),
            request_id: None,
        })
        .unwrap();
    let token = setup["token"].as_str().unwrap().to_owned();
    service
        .db
        .execute(
            "INSERT INTO app_metadata VALUES('live-sentinel','newest-live')",
            [],
        )
        .unwrap();
    service
        .set_backup_schedule(&crate::BackupScheduleState {
            last_success_at: Some(100),
            ..Default::default()
        })
        .unwrap();
    let safety = inspection(&service);
    let ledger = service.backup_live_ledger().unwrap();
    let source_service = Service::open(source.path()).unwrap();
    source_service
        .db
        .execute(
            "INSERT INTO app_metadata VALUES('archive-padding',?1)",
            ["p".repeat(2 * 1024 * 1024)],
        )
        .unwrap();
    let input = inspection(&source_service);
    let pages: u32 = service
        .db
        .pragma_query_value(None, "page_count", |r| r.get(0))
        .unwrap();
    let original_limit: u32 = service
        .db
        .pragma_query_value(None, "max_page_count", |r| r.get(0))
        .unwrap();
    service
        .db
        .pragma_update(None, "max_page_count", pages + 16)
        .unwrap();
    let before = service.db.serialize(rusqlite::MAIN_DB).unwrap().to_vec();
    let before_epoch = epoch(&service);
    // Exercise the same SQLite backup primitive independently and verify the
    // actual SQLite error, rather than injecting a fabricated storage Fault.
    {
        let replacement = crate::backup::snapshot(&input.database).unwrap();
        let backup = rusqlite::backup::Backup::new(&replacement, &mut service.db).unwrap();
        let error = backup.step(-1).unwrap_err();
        assert_eq!(
            error.sqlite_error_code(),
            Some(rusqlite::ErrorCode::DiskFull)
        );
    }
    assert_eq!(
        service.db.serialize(rusqlite::MAIN_DB).unwrap().as_ref(),
        before.as_slice()
    );
    assert_eq!(
        service.install_restore(&input, &ledger).unwrap_err().code,
        "INTERNAL_ERROR"
    );
    assert_eq!(
        service.db.serialize(rusqlite::MAIN_DB).unwrap().as_ref(),
        before.as_slice()
    );
    assert_eq!(epoch(&service), before_epoch);
    assert_eq!(service.backup_live_ledger().unwrap(), ledger);
    assert!(
        service
            .request(Request {
                operation: "account.status".into(),
                auth: Some(token.clone()),
                args: json!({}),
                context: json!({}),
                request_id: None
            })
            .is_ok()
    );
    // The smaller latest Safety still fits the limit. Recovery expires the
    // credential and cursor while preserving the latest data and live ledger.
    service.install_restore(&safety, &ledger).unwrap();
    assert_ne!(epoch(&service), before_epoch);
    assert_eq!(
        service
            .db
            .query_row(
                "SELECT value FROM app_metadata WHERE key='live-sentinel'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "newest-live"
    );
    assert_eq!(service.backup_live_ledger().unwrap(), ledger);
    assert!(
        service
            .request(Request {
                operation: "account.status".into(),
                auth: Some(token),
                args: json!({}),
                context: json!({}),
                request_id: None
            })
            .is_err()
    );
    assert_eq!(
        service
            .db
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    // Removing only the fixture limit also makes the larger replacement succeed.
    service
        .db
        .pragma_update(None, "max_page_count", original_limit)
        .unwrap();
    service.install_restore(&input, &ledger).unwrap();
    assert_eq!(
        service
            .db
            .query_row(
                "SELECT length(value) FROM app_metadata WHERE key='archive-padding'",
                [],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
        2 * 1024 * 1024
    );
    assert_eq!(service.backup_live_ledger().unwrap(), ledger);
}
