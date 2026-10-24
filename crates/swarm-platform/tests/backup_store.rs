#![cfg(windows)]
use std::{fs, io::Write};
use swarm_platform::{BackupStore, CredentialStore, PlatformError};

#[test]
fn non_local_namespace_and_relative_destination_are_rejected_before_access() {
    for path in [
        r"\\server\share\backups",
        r"\\?\UNC\server\share\backups",
        r"\\.\C:\backups",
        r"C:relative",
        r"relative",
    ] {
        assert!(matches!(
            BackupStore::open(std::path::Path::new(path)),
            Err(PlatformError::InvalidRoot)
        ));
    }
}

#[test]
fn removal_is_idempotent_and_refuses_aliases_or_an_active_reader() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("backups");
    let store = BackupStore::open(&root).unwrap();
    store
        .write_new("test", |f| {
            f.write_all(b"preserved").map_err(|_| PlatformError::Io)
        })
        .unwrap();
    let reader = store.read("test").unwrap();
    assert!(store.remove("test").is_err());
    drop(reader);
    let alias = parent.path().join("alias");
    fs::hard_link(root.join("test.backup"), &alias).unwrap();
    assert!(store.remove("test").is_err());
    assert_eq!(fs::read(&alias).unwrap(), b"preserved");
    fs::remove_file(alias).unwrap();
    store.remove("test").unwrap();
    store.remove("test").unwrap();
    assert!(!store.contains("test").unwrap());
    assert!(store.remove("../outside").is_err());
}

#[test]
fn backup_write_is_complete_immutable_and_supports_more_than_credential_size() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("backups");
    let store = BackupStore::open(&root).unwrap();
    let body = vec![b'X'; 128 * 1024];
    store
        .write_new("manual-test", |file| {
            file.write_all(&body).map_err(|_| PlatformError::Io)
        })
        .unwrap();
    assert_eq!(fs::read(root.join("manual-test.backup")).unwrap(), body);
    assert!(
        store
            .write_new("manual-test", |file| file
                .write_all(b"replacement")
                .map_err(|_| PlatformError::Io))
            .is_err()
    );
    assert_eq!(fs::read(root.join("manual-test.backup")).unwrap(), body);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    CredentialStore::open(&root).unwrap();
}

#[test]
fn writer_failure_does_not_publish_or_leave_a_partial_file() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("backups");
    let store = BackupStore::open(&root).unwrap();
    assert!(
        store
            .write_new("failed", |file| {
                file.write_all(b"partial private data").unwrap();
                Err(PlatformError::Io)
            })
            .is_err()
    );
    assert_eq!(fs::read_dir(root).unwrap().count(), 0);
}

#[test]
fn unsafe_keys_and_unrelated_existing_directory_are_rejected_without_changes() {
    let parent = tempfile::tempdir().unwrap();
    let unrelated = parent.path().join("unrelated");
    fs::create_dir(&unrelated).unwrap();
    fs::write(unrelated.join("keep"), b"unchanged").unwrap();
    assert!(BackupStore::open(&unrelated).is_err());
    assert_eq!(fs::read(unrelated.join("keep")).unwrap(), b"unchanged");
    let root = parent.path().join("backups");
    let store = BackupStore::open(&root).unwrap();
    for key in ["../outside", "", "C:/outside", "bad.key"] {
        assert!(
            store
                .write_new(key, |_| panic!("invalid key must not invoke writer"))
                .is_err()
        );
    }
    assert_eq!(fs::read_dir(root).unwrap().count(), 0);
}

#[test]
fn hard_link_destination_is_rejected_without_changing_external_content() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("backups");
    let store = BackupStore::open(&root).unwrap();
    let outside = parent.path().join("outside");
    fs::write(&outside, b"must be preserved").unwrap();
    fs::hard_link(&outside, root.join("alias.backup")).unwrap();
    assert!(
        store
            .write_new("alias", |_| panic!(
                "existing alias must be rejected before writer"
            ))
            .is_err()
    );
    assert_eq!(fs::read(outside).unwrap(), b"must be preserved");
}

#[test]
fn read_refuses_a_hard_link_added_after_publication() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("backups");
    let store = BackupStore::open(&root).unwrap();
    store
        .write_new("original", |f| {
            f.write_all(b"private content")
                .map_err(|_| PlatformError::Io)
        })
        .unwrap();
    fs::hard_link(root.join("original.backup"), parent.path().join("outside")).unwrap();
    assert!(store.read("original").is_err());
}

#[test]
fn junction_root_is_rejected_without_touching_target() {
    let parent = tempfile::tempdir().unwrap();
    let target = parent.path().join("target");
    let junction = parent.path().join("junction");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep"), b"preserve").unwrap();
    let status = std::process::Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command",
        "New-Item -ItemType Junction -Path $env:SWARM_BACKUP_JUNCTION -Target $env:SWARM_BACKUP_TARGET | Out-Null"])
        .env("SWARM_BACKUP_JUNCTION", &junction).env("SWARM_BACKUP_TARGET",&target).output().unwrap();
    assert!(
        status.status.success(),
        "temporary junction creation failed"
    );
    assert!(BackupStore::open(&junction).is_err());
    fs::remove_dir(&junction).unwrap();
    assert_eq!(fs::read(target.join("keep")).unwrap(), b"preserve");
}

#[test]
fn concurrent_publication_has_one_winner_and_preserves_complete_content() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("backups");
    BackupStore::open(&root).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let workers = [b"first".as_slice(), b"second".as_slice()]
        .into_iter()
        .map(|body| {
            let root = root.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                BackupStore::open(&root).unwrap().write_new("same", |file| {
                    file.write_all(body).map_err(|_| PlatformError::Io)?;
                    barrier.wait();
                    Ok(())
                })
            })
        })
        .collect::<Vec<_>>();
    let successful = workers
        .into_iter()
        .map(|worker| worker.join().unwrap().is_ok())
        .filter(|successful| *successful)
        .count();
    assert_eq!(successful, 1);
    let saved = fs::read(root.join("same.backup")).unwrap();
    assert!(saved == b"first" || saved == b"second");
    assert_eq!(fs::read_dir(root).unwrap().count(), 1);
}

#[test]
fn directory_is_pinned_during_write_and_reader_prevents_file_mutation() {
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("backups");
    let store = BackupStore::open(&root).unwrap();
    store
        .write_new("pinned", |file| {
            assert!(fs::rename(&root, parent.path().join("moved")).is_err());
            file.write_all(b"complete").map_err(|_| PlatformError::Io)
        })
        .unwrap();
    let reader = store.read("pinned").unwrap();
    assert!(
        fs::OpenOptions::new()
            .write(true)
            .open(root.join("pinned.backup"))
            .is_err()
    );
    drop(reader);
    assert_eq!(fs::read(root.join("pinned.backup")).unwrap(), b"complete");
}
