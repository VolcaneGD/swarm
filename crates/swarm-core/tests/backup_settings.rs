use rusqlite::Connection;
use swarm_core::{BackupSettings, Service};

#[test]
fn settings_persist_reject_invalid_boundaries_and_corruption_without_reset() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let settings = BackupSettings {
        enabled: false,
        interval_seconds: 3600,
        retention: 3,
        destination: Some(root.path().join("custom")),
    };
    service.set_backup_settings(&settings).unwrap();
    drop(service);
    let mut service = Service::open(root.path()).unwrap();
    assert!(service.backup_settings().unwrap() == settings);
    for (interval, retention) in [(59, 3), (31_536_001, 3), (3600, 0), (3600, 101)] {
        let mut invalid = settings.clone();
        invalid.interval_seconds = interval;
        invalid.retention = retention;
        assert!(service.set_backup_settings(&invalid).is_err());
        assert!(service.backup_settings().unwrap() == settings);
    }
    let db = Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    db.execute(
        "UPDATE app_metadata SET value='broken' WHERE key='backup-settings-v1'",
        [],
    )
    .unwrap();
    assert_eq!(
        service.backup_settings().err().unwrap().code,
        "BACKUP_STATE"
    );
    assert_eq!(
        db.query_row(
            "SELECT value FROM app_metadata WHERE key='backup-settings-v1'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "broken"
    );
}

#[test]
fn schedule_journal_is_excluded_from_snapshot_while_settings_are_preserved() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut settings = service.backup_settings().unwrap();
    settings.retention = 3;
    service.set_backup_settings(&settings).unwrap();
    let mut state = service.backup_schedule().unwrap();
    state.last_success_at = Some(100);
    service.set_backup_schedule(&state).unwrap();
    let snapshot = service.credential_free_snapshot().unwrap();
    assert!(
        !snapshot
            .prepare("SELECT 1 FROM app_metadata WHERE key='backup-schedule-v1'")
            .unwrap()
            .exists([])
            .unwrap()
    );
    assert!(
        snapshot
            .prepare("SELECT 1 FROM app_metadata WHERE key='backup-settings-v1'")
            .unwrap()
            .exists([])
            .unwrap()
    );
    assert_eq!(
        service.backup_schedule().unwrap().last_success_at,
        Some(100)
    );
}
