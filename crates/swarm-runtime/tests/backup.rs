#![cfg(windows)]
use swarm_core::{BackupKind, Service};
use swarm_platform::BackupStore;
use swarm_runtime::backup::{inspect_backup, save_backup};

#[test]
fn real_protected_backup_is_inspectable_and_missing_rules_do_not_publish_a_generation() {
    let root = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let store_root = destination.path().join("backups");
    let store = BackupStore::open(&store_root).unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let setup = service
        .request(swarm_core::protocol::Request {
            operation: "account.setup".into(),
            auth: None,
            args: serde_json::json!({"name":"Admin","password":"archive-test-password"}),
            context: serde_json::json!({}),
            request_id: None,
        })
        .unwrap();
    let token = setup["token"].as_str().unwrap();
    let key = save_backup(&mut service, &store, BackupKind::Manual).unwrap();
    assert!(key.starts_with("manual-"));
    let inspected = inspect_backup(&store, &key).unwrap();
    assert!(matches!(inspected.kind, BackupKind::Manual));
    assert_eq!(inspected.rules.len(), 1);
    let file = store_root.join(format!("{key}.backup"));
    let original = std::fs::read(&file).unwrap();
    use sha2::{Digest, Sha256};
    let credential_hash = format!("{:x}", Sha256::digest(token.as_bytes()));
    assert!(
        !original
            .windows(credential_hash.len())
            .any(|bytes| bytes == credential_hash.as_bytes())
    );
    service
        .request(swarm_core::protocol::Request {
            operation: "account.status".into(),
            auth: Some(token.to_owned()),
            args: serde_json::json!({}),
            context: serde_json::json!({}),
            request_id: None,
        })
        .unwrap();
    std::fs::remove_file(root.path().join("RULES.md")).unwrap();
    assert!(save_backup(&mut service, &store, BackupKind::Scheduled).is_err());
    assert_eq!(std::fs::read_dir(&store_root).unwrap().count(), 1);
    assert_eq!(std::fs::read(&file).unwrap(), original);
    let mut damaged = original;
    let last = damaged.len() - 1;
    damaged[last] ^= 1;
    std::fs::write(&file, damaged).unwrap();
    assert!(inspect_backup(&store, &key).is_err());
}
