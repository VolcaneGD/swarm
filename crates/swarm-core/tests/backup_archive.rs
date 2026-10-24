use std::io::Cursor;
use swarm_core::{BackupKind, Service, inspect_backup_archive};

#[test]
fn archive_roundtrip_preserves_manifest_rules_and_database_and_detects_damage() {
    let root = tempfile::tempdir().unwrap();
    let service = Service::open(root.path()).unwrap();
    let prepared = service.prepare_backup().unwrap();
    let mut bytes = Vec::new();
    prepared
        .write_archive(&mut bytes, BackupKind::Manual)
        .unwrap();
    let manifest = inspect_backup_archive(&mut Cursor::new(&bytes)).unwrap();
    assert_eq!(manifest.format_version, 1);
    assert_eq!(manifest.schema_version, 9);
    assert_eq!(manifest.rules.len(), 1);
    assert!(
        manifest.rules[0]
            .content
            .as_ref()
            .unwrap()
            .contains("swarm-rules")
    );
    assert!(manifest.database_bytes > 0);
    assert!(matches!(manifest.kind, BackupKind::Manual));
    let mut damaged = bytes.clone();
    let last = damaged.len() - 1;
    damaged[last] ^= 0x80;
    assert!(inspect_backup_archive(&mut Cursor::new(damaged)).is_err());
    assert!(inspect_backup_archive(&mut Cursor::new(&bytes[..bytes.len() - 1])).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(inspect_backup_archive(&mut Cursor::new(trailing)).is_err());
    let mut unknown = bytes.clone();
    unknown[0] ^= 1;
    assert!(inspect_backup_archive(&mut Cursor::new(unknown)).is_err());
}

#[test]
fn oversized_manifest_header_is_rejected_without_reading_or_allocating_its_content() {
    let mut header = b"SWARMBACKUP1\n".to_vec();
    header.extend_from_slice(&u64::MAX.to_le_bytes());
    let error = inspect_backup_archive(&mut Cursor::new(header))
        .err()
        .unwrap();
    assert_eq!(error.code, "BACKUP_INVALID");
}

#[test]
fn malformed_manifest_rules_versions_lengths_and_unknown_fields_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let service = Service::open(root.path()).unwrap();
    let mut bytes = Vec::new();
    service
        .prepare_backup()
        .unwrap()
        .write_archive(&mut bytes, BackupKind::Safety)
        .unwrap();
    let size = u64::from_le_bytes(bytes[13..21].try_into().unwrap()) as usize;
    let original: serde_json::Value = serde_json::from_slice(&bytes[21..21 + size]).unwrap();
    for case in 0..5 {
        let mut modified = original.clone();
        match case {
            0 => modified["format_version"] = serde_json::json!(2),
            1 => modified["database_bytes"] = serde_json::json!(u64::MAX),
            2 => modified["unexpected"] = serde_json::json!(true),
            3 => modified["rules"][0]["content"] = serde_json::Value::Null,
            _ => modified["rules"][0]["revision"] = serde_json::json!("wrong"),
        }
        let encoded = serde_json::to_vec(&modified).unwrap();
        let mut invalid = bytes[..13].to_vec();
        invalid.extend_from_slice(&(encoded.len() as u64).to_le_bytes());
        invalid.extend_from_slice(&encoded);
        invalid.extend_from_slice(&bytes[21 + size..]);
        assert!(
            inspect_backup_archive(&mut Cursor::new(invalid)).is_err(),
            "case {case}"
        );
    }
}
