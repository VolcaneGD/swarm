use sha2::{Digest, Sha256};
use std::io::Cursor;
use swarm_core::{BackupKind, Service, inspect_backup_archive, inspect_restore_archive};

fn archive() -> Vec<u8> {
    let root = tempfile::tempdir().unwrap();
    let service = Service::open(root.path()).unwrap();
    let mut bytes = Vec::new();
    service
        .prepare_backup()
        .unwrap()
        .write_archive(&mut bytes, BackupKind::Manual)
        .unwrap();
    bytes
}

fn altered(original: &[u8], sql: &str) -> Vec<u8> {
    let size = u64::from_le_bytes(original[13..21].try_into().unwrap()) as usize;
    let mut manifest: serde_json::Value = serde_json::from_slice(&original[21..21 + size]).unwrap();
    let payload = &original[21 + size..];
    let mut db = rusqlite::Connection::open_in_memory().unwrap();
    db.deserialize_read_exact(
        rusqlite::MAIN_DB,
        Cursor::new(payload),
        payload.len(),
        false,
    )
    .unwrap();
    db.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    db.execute_batch(sql).unwrap();
    let data = db.serialize(rusqlite::MAIN_DB).unwrap();
    manifest["database_bytes"] = serde_json::json!(data.len());
    manifest["database_sha256"] = serde_json::json!(format!("{:x}", Sha256::digest(&*data)));
    let encoded = serde_json::to_vec(&manifest).unwrap();
    let mut out = b"SWARMBACKUP1\n".to_vec();
    out.extend_from_slice(&(encoded.len() as u64).to_le_bytes());
    out.extend(encoded);
    out.extend_from_slice(&data);
    out
}

#[test]
fn valid_snapshot_is_inspected_without_changing_source_or_reading_rules_paths() {
    let bytes = archive(); // original root is already gone
    let before = bytes.clone();
    let result = inspect_restore_archive(&mut Cursor::new(&bytes)).unwrap();
    assert_eq!(result.manifest().schema_version, 9);
    assert_eq!(result.manifest().rules.len(), 1);
    assert_eq!(bytes, before);
}

#[test]
fn checksum_valid_unknown_schema_objects_and_retained_operation_journals_are_rejected() {
    let original = archive();
    for sql in [
        "CREATE TABLE unexpected(secret TEXT);",
        "CREATE VIEW unexpected AS SELECT * FROM accounts;",
        "CREATE TRIGGER unexpected AFTER INSERT ON app_metadata BEGIN DELETE FROM accounts; END;",
        "INSERT INTO app_metadata VALUES('credential-receipt:old','secret');",
        "INSERT INTO app_metadata VALUES('google-recovery-link-v1:old','{}');",
        "INSERT INTO app_metadata VALUES('google-recovery-challenge-v1:old','{}');",
        "INSERT INTO app_metadata VALUES('backup-manual-v1:old','{}');",
        "INSERT INTO app_metadata VALUES('backup-schedule-v1','{}');",
        "INSERT INTO app_metadata VALUES('notification-batch-v1','{}');",
        "INSERT INTO accounts VALUES('a','Admin','hash','recovery',1,'now',0); INSERT INTO actors VALUES('a','human'); INSERT INTO human_sessions VALUES('secret','a');",
        "INSERT INTO accounts VALUES('a','Admin','hash','recovery',1,'now',0); INSERT INTO actors VALUES('a','human'); INSERT INTO notification_selection VALUES(1,'a',printf('%064d',0),'generation');",
        "INSERT INTO project_paths VALUES('orphan','absent');",
        "DROP INDEX notification_outbox_pending;",
        "PRAGMA writable_schema=ON; UPDATE sqlite_schema SET sql=replace(sql,'name TEXT NOT NULL','name BLOB NOT NULL') WHERE name='projects'; PRAGMA writable_schema=OFF;",
    ] {
        let bytes = altered(&original, sql);
        assert!(inspect_backup_archive(&mut Cursor::new(&bytes)).is_ok());
        assert_eq!(
            inspect_restore_archive(&mut Cursor::new(bytes))
                .err()
                .unwrap()
                .code,
            "RESTORE_INVALID",
            "{sql}"
        );
    }
}

fn replace_manifest(original: &[u8], change: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    let size = u64::from_le_bytes(original[13..21].try_into().unwrap()) as usize;
    let mut manifest: serde_json::Value = serde_json::from_slice(&original[21..21 + size]).unwrap();
    change(&mut manifest);
    let encoded = serde_json::to_vec(&manifest).unwrap();
    let mut out = b"SWARMBACKUP1\n".to_vec();
    out.extend_from_slice(&(encoded.len() as u64).to_le_bytes());
    out.extend(encoded);
    out.extend_from_slice(&original[21 + size..]);
    out
}

#[test]
fn registered_project_rules_must_match_archived_roots_without_filesystem_access() {
    let bytes = altered(
        &archive(),
        "INSERT INTO projects VALUES('p','Project','C:\\nonexistent-restore-fixture','now');",
    );
    assert!(inspect_restore_archive(&mut Cursor::new(&bytes)).is_err());
    let matched = replace_manifest(&bytes, |manifest| {
        manifest["rules"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "project_id":"p", "source_path":"C:\\nonexistent-restore-fixture\\RULES.md",
                "content":null, "revision":"missing:v1"
            }));
    });
    assert!(inspect_restore_archive(&mut Cursor::new(&matched)).is_ok());
    for (field, value) in [
        ("project_id", "unknown"),
        ("source_path", "C:\\different\\RULES.md"),
        ("source_path", "relative/RULES.md"),
    ] {
        let invalid = replace_manifest(&matched, |manifest| {
            manifest["rules"][1][field] = serde_json::json!(value)
        });
        assert!(inspect_backup_archive(&mut Cursor::new(&invalid)).is_ok());
        assert!(inspect_restore_archive(&mut Cursor::new(invalid)).is_err());
    }
}

#[test]
fn checksum_valid_database_page_damage_and_truncated_or_trailing_archives_are_rejected() {
    let original = archive();
    let size = u64::from_le_bytes(original[13..21].try_into().unwrap()) as usize;
    let mut damaged = original.clone();
    damaged[21 + size + 100] = 0xff; // invalidate the sqlite_schema root page type
    let hash = format!("{:x}", Sha256::digest(&damaged[21 + size..]));
    let damaged = replace_manifest(&damaged, |manifest| {
        manifest["database_sha256"] = serde_json::json!(hash)
    });
    assert!(inspect_backup_archive(&mut Cursor::new(&damaged)).is_ok());
    assert!(inspect_restore_archive(&mut Cursor::new(damaged)).is_err());
    assert!(inspect_restore_archive(&mut Cursor::new(&original[..original.len() - 1])).is_err());
    let mut trailing = original;
    trailing.push(0);
    assert!(inspect_restore_archive(&mut Cursor::new(trailing)).is_err());
}

#[test]
fn manifest_cap_is_inclusive_and_oversized_lengths_fail_before_payload_reads() {
    use std::io::Read;
    struct HeaderOnly(Cursor<Vec<u8>>);
    impl Read for HeaderOnly {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            assert!(
                self.0.position() < self.0.get_ref().len() as u64,
                "oversized header must be rejected before reading its payload"
            );
            self.0.read(out)
        }
    }
    let original = archive();
    let size = u64::from_le_bytes(original[13..21].try_into().unwrap()) as usize;
    let cap = 16 * 1024 * 1024;
    let mut exact = b"SWARMBACKUP1\n".to_vec();
    exact.extend_from_slice(&(cap as u64).to_le_bytes());
    exact.extend_from_slice(&original[21..21 + size]);
    exact.resize(21 + cap, b' '); // valid JSON trailing whitespace
    exact.extend_from_slice(&original[21 + size..]);
    assert!(inspect_restore_archive(&mut Cursor::new(exact)).is_ok());

    for length in [cap as u64 + 1, u64::MAX] {
        let mut prefix = b"SWARMBACKUP1\n".to_vec();
        prefix.extend_from_slice(&length.to_le_bytes());
        assert_eq!(
            inspect_restore_archive(&mut HeaderOnly(Cursor::new(prefix)))
                .err()
                .unwrap()
                .code,
            "RESTORE_INVALID"
        );
    }
    for length in [1024 * 1024 * 1024 + 1, u64::MAX] {
        let changed = replace_manifest(&original, |manifest| {
            manifest["database_bytes"] = serde_json::json!(length);
        });
        let size = u64::from_le_bytes(changed[13..21].try_into().unwrap()) as usize;
        let prefix = changed[..21 + size].to_vec();
        assert_eq!(
            inspect_restore_archive(&mut HeaderOnly(Cursor::new(prefix)))
                .err()
                .unwrap()
                .code,
            "RESTORE_INVALID"
        );
    }
}
