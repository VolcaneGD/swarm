#![cfg(windows)]
use swarm_core::Service;
use swarm_platform::BackupStore;
use swarm_runtime::backup::{save_backup, scheduled_tick};

#[test]
fn persisted_full_stop_blocks_tick_even_without_an_in_memory_stop_flag() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let protected = swarm_platform::CredentialStore::open(&root.path().join("lifecycle")).unwrap();
    let boot = swarm_platform::lifecycle::boot_id().unwrap();
    protected
        .put("full-stop", format!("1:{boot}").as_bytes())
        .unwrap();
    assert_eq!(
        scheduled_tick(&mut service, root.path(), 100, false)
            .err()
            .unwrap()
            .code,
        "RUNTIME_STOPPED"
    );
    assert!(!root.path().join("backups").exists());
    let state = service.backup_schedule().unwrap();
    assert!(state.last_success_at.is_none());
    assert!(state.pending.is_none());
    assert!(state.generations.is_empty());
}

#[test]
fn daily_default_catches_up_once_preserves_manual_and_keeps_seven_successes() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let defaults = service.backup_settings().unwrap();
    assert!(defaults.enabled);
    assert_eq!(defaults.interval_seconds, 86400);
    assert_eq!(defaults.retention, 7);
    let first = scheduled_tick(&mut service, root.path(), 100, false)
        .unwrap()
        .expect("initial backup must run");
    assert!(first.starts_with("scheduled-"));
    let store = BackupStore::open(&root.path().join("backups")).unwrap();
    let manual = save_backup(&mut service, &store, swarm_core::BackupKind::Manual).unwrap();
    assert!(
        scheduled_tick(&mut service, root.path(), 101, false)
            .unwrap()
            .is_none()
    );
    assert!(
        scheduled_tick(&mut service, root.path(), 100 + 86400 * 5, true)
            .unwrap()
            .is_none()
    );
    assert!(
        scheduled_tick(&mut service, root.path(), 100 + 86400 * 5, false)
            .unwrap()
            .is_some()
    );
    assert!(
        scheduled_tick(&mut service, root.path(), 100 + 86400 * 5, false)
            .unwrap()
            .is_none()
    );
    for day in 6..14 {
        assert!(
            scheduled_tick(&mut service, root.path(), 100 + 86400 * day, false)
                .unwrap()
                .is_some()
        );
    }
    let state = service.backup_schedule().unwrap();
    assert_eq!(state.generations.len(), 7);
    assert!(state.prune.is_empty());
    assert_eq!(
        std::fs::read_dir(root.path().join("backups"))
            .unwrap()
            .count(),
        8
    );
    assert!(store.read(&manual).is_ok());
    assert!(store.read(&first).is_err());
    drop(service);
    let mut service = Service::open(root.path()).unwrap();
    assert!(
        scheduled_tick(&mut service, root.path(), 100 + 86400 * 13, false)
            .unwrap()
            .is_none()
    );
}

#[test]
fn failed_capture_keeps_previous_success_and_disabled_or_stopped_never_publish() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    scheduled_tick(&mut service, root.path(), 100, false)
        .unwrap()
        .expect("first");
    std::fs::remove_file(root.path().join("RULES.md")).unwrap();
    assert!(scheduled_tick(&mut service, root.path(), 86500, false).is_err());
    let state = service.backup_schedule().unwrap();
    assert_eq!(state.last_success_at, Some(100));
    assert_eq!(state.generations.len(), 1);
    let mut settings = service.backup_settings().unwrap();
    settings.enabled = false;
    service.set_backup_settings(&settings).unwrap();
    assert!(
        scheduled_tick(&mut service, root.path(), 86500, false)
            .unwrap()
            .is_none()
    );
    assert!(
        scheduled_tick(&mut service, root.path(), 86500, true)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        std::fs::read_dir(root.path().join("backups"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn published_intent_recovers_after_receipt_failure_without_a_second_file_or_catchup() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let db = rusqlite::Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER fail_backup_receipt BEFORE UPDATE ON app_metadata
        WHEN NEW.key='backup-schedule-v1' AND json_extract(NEW.value,'$.pending') IS NULL
        BEGIN SELECT RAISE(ABORT,'controlled receipt failure'); END;",
    )
    .unwrap();
    assert!(scheduled_tick(&mut service, root.path(), 100, false).is_err());
    let pending = service.backup_schedule().unwrap();
    let key = pending.pending.unwrap().generation.key;
    assert!(pending.last_success_at.is_none());
    assert!(pending.generations.is_empty());
    assert_eq!(
        std::fs::read_dir(root.path().join("backups"))
            .unwrap()
            .count(),
        1
    );
    db.execute_batch("DROP TRIGGER fail_backup_receipt")
        .unwrap();
    drop(service);
    let mut service = Service::open(root.path()).unwrap();
    assert_eq!(
        scheduled_tick(&mut service, root.path(), 100 + 86400 * 5, false)
            .unwrap()
            .unwrap(),
        key
    );
    assert!(
        scheduled_tick(&mut service, root.path(), 100 + 86400 * 5, false)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        std::fs::read_dir(root.path().join("backups"))
            .unwrap()
            .count(),
        1
    );
    assert_eq!(
        service.backup_schedule().unwrap().last_success_at,
        Some(100 + 86400 * 5)
    );
}

#[test]
fn retention_failure_keeps_prune_queue_and_external_hard_link_until_safe_retry() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut settings = service.backup_settings().unwrap();
    settings.retention = 1;
    settings.interval_seconds = 60;
    service.set_backup_settings(&settings).unwrap();
    let first = scheduled_tick(&mut service, root.path(), 100, false)
        .unwrap()
        .unwrap();
    let outside = root.path().join("external-backup");
    std::fs::hard_link(
        root.path().join("backups").join(format!("{first}.backup")),
        &outside,
    )
    .unwrap();
    let original = std::fs::read(&outside).unwrap();
    assert!(scheduled_tick(&mut service, root.path(), 160, false).is_err());
    let state = service.backup_schedule().unwrap();
    assert_eq!(state.generations.len(), 1);
    assert_eq!(state.prune.len(), 1);
    assert_eq!(state.last_success_at, Some(160));
    assert_eq!(std::fs::read(&outside).unwrap(), original);
    std::fs::remove_file(outside).unwrap();
    assert!(
        scheduled_tick(&mut service, root.path(), 160, false)
            .unwrap()
            .is_none()
    );
    assert!(service.backup_schedule().unwrap().prune.is_empty());
    assert_eq!(
        std::fs::read_dir(root.path().join("backups"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn destination_change_keeps_manual_and_tracks_old_generations_in_their_original_folder() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let first = scheduled_tick(&mut service, root.path(), 100, false)
        .unwrap()
        .unwrap();
    let old = BackupStore::open(&root.path().join("backups")).unwrap();
    let manual = save_backup(&mut service, &old, swarm_core::BackupKind::Manual).unwrap();
    let mut settings = service.backup_settings().unwrap();
    settings.destination = Some(root.path().join("custom"));
    settings.interval_seconds = 60;
    settings.retention = 1;
    service.set_backup_settings(&settings).unwrap();
    let new = scheduled_tick(&mut service, root.path(), 160, false)
        .unwrap()
        .unwrap();
    assert!(!old.contains(&first).unwrap());
    assert!(old.contains(&manual).unwrap());
    let custom = BackupStore::open(&root.path().join("custom")).unwrap();
    assert!(custom.contains(&new).unwrap());
    assert_eq!(
        service.backup_schedule().unwrap().generations[0].destination,
        root.path().join("custom")
    );
}

#[test]
fn malformed_journal_fails_closed_and_does_not_create_destination() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let db = rusqlite::Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    db.execute(
        "INSERT INTO app_metadata VALUES('backup-schedule-v1','broken')",
        [],
    )
    .unwrap();
    assert_eq!(
        scheduled_tick(&mut service, root.path(), 100, false)
            .err()
            .unwrap()
            .code,
        "BACKUP_STATE"
    );
    assert!(!root.path().join("backups").exists());
    assert_eq!(
        db.query_row(
            "SELECT value FROM app_metadata WHERE key='backup-schedule-v1'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "broken"
    );
}
