//! Administrator-only lifecycle for separately stored backup copies.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use swarm_core::{Service, protocol::Fault};
use swarm_platform::{BackupStore, CredentialStore};

fn storage() -> Fault {
    Fault::new("BACKUP_STORAGE", 8, "Backup storage could not be verified")
}
fn invalid() -> Fault {
    Fault::new("INVALID_INPUT", 2, "Invalid storage cleanup request")
}
fn owned_key(key: &str) -> bool {
    ["manual-", "scheduled-"].iter().any(|prefix| {
        key.strip_prefix(prefix)
            .is_some_and(swarm_core::valid_backup_intent)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Service) {
        let root = tempfile::tempdir().unwrap();
        let service = Service::open(root.path()).unwrap();
        (root, service)
    }
    #[test]
    fn explicit_archive_deletion_removes_file_and_receipt_but_preserves_unrelated_files() {
        let (root, mut service) = fixture();
        let directory = root.path().join("backups");
        let store = BackupStore::open(&directory).unwrap();
        let intent = uuid::Uuid::new_v4().to_string();
        let key = format!("manual-{intent}");
        use std::io::Write;
        store
            .write_new(&key, |f| {
                f.write_all(b"fixture")
                    .map_err(|_| swarm_platform::PlatformError::Io)
            })
            .unwrap();
        store
            .write_new("unrelated", |f| {
                f.write_all(b"preserve")
                    .map_err(|_| swarm_platform::PlatformError::Io)
            })
            .unwrap();
        service
            .set_manual_backup(&swarm_core::ManualBackup {
                version: 1,
                account_id: "fixture".into(),
                intent: intent.clone(),
                destination: directory.clone(),
                completed: true,
            })
            .unwrap();
        let list = control(&mut service, root.path(), "archives.list", &json!({})).unwrap();
        assert_eq!(list["archives"].as_array().unwrap().len(), 1);
        assert_eq!(list["archives"][0]["bytes"], 7);
        let mut args = json!({"directory":directory,"key":key,"confirmed":false});
        assert_eq!(
            control(&mut service, root.path(), "archives.delete", &args)
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
        assert!(store.contains(&key).unwrap());
        args["confirmed"] = json!(true);
        control(&mut service, root.path(), "archives.delete", &args).unwrap();
        assert!(!store.contains(&key).unwrap());
        assert!(service.manual_backup(&intent).unwrap().is_none());
        assert!(store.contains("unrelated").unwrap());
    }
    #[test]
    fn pending_capture_and_arbitrary_destination_cannot_be_deleted() {
        let (root, mut service) = fixture();
        let directory = root.path().join("backups");
        let store = BackupStore::open(&directory).unwrap();
        let intent = uuid::Uuid::new_v4().to_string();
        let key = format!("manual-{intent}");
        store.write_new(&key, |_| Ok(())).unwrap();
        service
            .set_manual_backup(&swarm_core::ManualBackup {
                version: 1,
                account_id: "fixture".into(),
                intent,
                destination: directory.clone(),
                completed: false,
            })
            .unwrap();
        for destination in [&directory, &root.path().join("other")] {
            assert_eq!(
                control(
                    &mut service,
                    root.path(),
                    "archives.delete",
                    &json!({"directory":destination,"key":key,"confirmed":true})
                )
                .unwrap_err()
                .code,
                "CONFLICT"
            );
        }
        assert!(store.contains(&key).unwrap());
    }
    #[test]
    fn windows_directory_aliases_share_inventory_pending_guard_and_receipt_cleanup() {
        let (root, mut service) = fixture();
        let original = root.path().join("backups");
        let store = BackupStore::open(&original).unwrap();
        let canonical = original.canonicalize().unwrap();
        let plain = PathBuf::from(canonical.to_str().unwrap().strip_prefix(r"\\?\").unwrap());
        assert_ne!(plain, canonical);
        let intent = uuid::Uuid::new_v4().to_string();
        let key = format!("manual-{intent}");
        store.write_new(&key, |_| Ok(())).unwrap();
        let mut settings = service.backup_settings().unwrap();
        settings.destination = Some(plain.clone());
        service.set_backup_settings(&settings).unwrap();
        let mut receipt = swarm_core::ManualBackup {
            version: 1,
            account_id: "fixture".into(),
            intent: intent.clone(),
            destination: canonical,
            completed: false,
        };
        service.set_manual_backup(&receipt).unwrap();
        let entries = inventory(&service, root.path()).unwrap();
        assert_eq!(entries.len(), 1, "one physical archive must have one row");
        assert!(
            !entries[0].deletable,
            "every alias must protect pending capture"
        );
        assert_eq!(
            control(
                &mut service,
                root.path(),
                "archives.delete",
                &json!({
                    "directory":plain,"key":key,"confirmed":true
                })
            )
            .unwrap_err()
            .code,
            "CONFLICT"
        );
        assert!(store.contains(&key).unwrap());
        receipt.completed = true;
        service.set_manual_backup(&receipt).unwrap();
        let entries = inventory(&service, root.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].deletable);
        control(
            &mut service,
            root.path(),
            "archives.delete",
            &json!({
                "directory":entries[0].directory,"key":key,"confirmed":true
            }),
        )
        .unwrap();
        assert!(!store.contains(&key).unwrap());
        assert!(service.manual_backup(&intent).unwrap().is_none());
        assert!(inventory(&service, root.path()).unwrap().is_empty());

        let scheduled_key = format!("scheduled-{}", uuid::Uuid::new_v4());
        store.write_new(&scheduled_key, |_| Ok(())).unwrap();
        let generation = swarm_core::BackupGeneration {
            key: scheduled_key.clone(),
            destination: plain.clone(),
        };
        let mut schedule = swarm_core::BackupScheduleState {
            pending: Some(swarm_core::BackupIntent {
                generation: generation.clone(),
                due_at: 0,
            }),
            ..Default::default()
        };
        service.set_backup_schedule(&schedule).unwrap();
        let entries = inventory(&service, root.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].deletable);
        assert!(
            control(
                &mut service,
                root.path(),
                "archives.delete",
                &json!({
                    "directory":entries[0].directory,"key":scheduled_key,"confirmed":true
                })
            )
            .is_err()
        );
        schedule.pending = None;
        schedule.generations.push(generation);
        let prune_key = format!("scheduled-{}", uuid::Uuid::new_v4());
        store.write_new(&prune_key, |_| Ok(())).unwrap();
        schedule.prune.push(swarm_core::BackupGeneration {
            key: prune_key.clone(),
            destination: plain,
        });
        service.set_backup_schedule(&schedule).unwrap();
        let entries = inventory(&service, root.path()).unwrap();
        let entry = entries
            .iter()
            .find(|entry| entry.key == scheduled_key)
            .unwrap();
        control(
            &mut service,
            root.path(),
            "archives.delete",
            &json!({
                "directory":entry.directory,"key":scheduled_key,"confirmed":true
            }),
        )
        .unwrap();
        let schedule = service.backup_schedule().unwrap();
        assert!(schedule.generations.is_empty());
        assert_eq!(schedule.prune.len(), 1);
        assert!(!store.contains(&scheduled_key).unwrap());
        let entry = entries.iter().find(|entry| entry.key == prune_key).unwrap();
        control(
            &mut service,
            root.path(),
            "archives.delete",
            &json!({
                "directory":entry.directory,"key":prune_key,"confirmed":true
            }),
        )
        .unwrap();
        assert!(service.backup_schedule().unwrap().prune.is_empty());
        assert!(!store.contains(&prune_key).unwrap());
    }
    #[test]
    fn active_restore_blocks_cleanup_and_terminal_artifacts_can_be_discarded() {
        let (root, mut service) = fixture();
        let journal = CredentialStore::open(&root.path().join("restore-transactions")).unwrap();
        let preparation = CredentialStore::open(&root.path().join("restore-preparation")).unwrap();
        journal.put("active", b"protected").unwrap();
        journal.put("result-fixture", b"terminal").unwrap();
        preparation.put("fixture", b"draft").unwrap();
        let store = BackupStore::open(&root.path().join("restore-safety")).unwrap();
        let key = format!("apply-rules-{}", uuid::Uuid::new_v4());
        store.write_new(&key, |_| Ok(())).unwrap();
        assert_eq!(
            control(
                &mut service,
                root.path(),
                "restore.cleanup",
                &json!({"confirmed":true})
            )
            .unwrap_err()
            .code,
            "RESTORE_RECOVERY_REQUIRED"
        );
        assert!(store.contains(&key).unwrap());
        assert!(preparation.get("fixture").unwrap().is_some());
        journal.remove("active").unwrap();
        assert!(
            control(
                &mut service,
                root.path(),
                "restore.cleanup",
                &json!({"confirmed":false})
            )
            .is_err()
        );
        control(
            &mut service,
            root.path(),
            "restore.cleanup",
            &json!({"confirmed":true}),
        )
        .unwrap();
        assert!(!store.contains(&key).unwrap());
        assert!(journal.get("result-fixture").unwrap().is_none());
        assert!(preparation.get("fixture").unwrap().is_none());
        assert!(root.path().join("swarm.sqlite3").is_file());
    }
}
fn safety_key(key: &str) -> bool {
    ["safety-", "apply-safety-", "apply-rules-"]
        .iter()
        .any(|prefix| {
            key.strip_prefix(prefix)
                .is_some_and(swarm_core::valid_backup_intent)
        })
}
#[derive(Serialize)]
struct Archive {
    directory: PathBuf,
    key: String,
    bytes: u64,
    deletable: bool,
}

fn same_directory(left: &Path, right: &Path) -> bool {
    left == right || matches!((left.canonicalize(), right.canonicalize()), (Ok(a), Ok(b)) if a == b)
}

fn inventory(service: &Service, root: &Path) -> Result<Vec<Archive>, Fault> {
    let settings = service.backup_settings()?;
    let schedule = service.backup_schedule()?;
    let manual = service.manual_backups()?;
    let mut directories = BTreeSet::from([root.join("backups")]);
    if let Some(path) = settings.destination {
        directories.insert(path);
    }
    for item in schedule.generations.iter().chain(&schedule.prune) {
        directories.insert(item.destination.clone());
    }
    if let Some(pending) = &schedule.pending {
        directories.insert(pending.generation.destination.clone());
    }
    for item in &manual {
        directories.insert(item.destination.clone());
    }
    let mut archives = Vec::new();
    let mut visited = BTreeSet::new();
    for directory in directories {
        if !directory.exists() {
            continue;
        }
        // Validate the original spelling before canonicalizing: aliases must not
        // bypass the private-store/reparse checks or expose pending captures.
        let store = BackupStore::open(&directory).map_err(|_| storage())?;
        let directory = directory.canonicalize().map_err(|_| storage())?;
        if !visited.insert(directory.clone()) {
            continue;
        }
        for (key, bytes) in store.list().map_err(|_| storage())? {
            if !owned_key(&key) {
                continue;
            }
            let pending = schedule.pending.as_ref().is_some_and(|p| {
                p.generation.key == key && same_directory(&p.generation.destination, &directory)
            }) || manual.iter().any(|m| {
                !m.completed
                    && format!("manual-{}", m.intent) == key
                    && same_directory(&m.destination, &directory)
            });
            archives.push(Archive {
                directory: directory.clone(),
                key,
                bytes,
                deletable: !pending,
            });
        }
    }
    // Keep orphaned completed receipts reachable even after external file removal.
    for item in manual.iter().filter(|m| m.completed) {
        let key = format!("manual-{}", item.intent);
        if !archives
            .iter()
            .any(|a| same_directory(&a.directory, &item.destination) && a.key == key)
        {
            archives.push(Archive {
                directory: item.destination.clone(),
                key,
                bytes: 0,
                deletable: true,
            });
        }
    }
    Ok(archives)
}

/// Caller holds the Service gate and has rechecked current administrator identity.
pub(crate) fn control(
    service: &mut Service,
    root: &Path,
    operation: &str,
    args: &Value,
) -> Result<Value, Fault> {
    crate::restore_apply::blocked(root)?;
    match operation {
        "archives.list" => {
            if args.as_object().is_none_or(|o| !o.is_empty()) {
                return Err(invalid());
            }
            let safety = root.join("restore-safety");
            let recovery_bytes = if safety.exists() {
                BackupStore::open(&safety)
                    .map_err(|_| storage())?
                    .list()
                    .map_err(|_| storage())?
                    .iter()
                    .filter(|(key, _)| safety_key(key))
                    .map(|(_, bytes)| *bytes)
                    .sum::<u64>()
            } else {
                0
            };
            Ok(json!({"archives": inventory(service, root)?, "recovery_bytes": recovery_bytes}))
        }
        "archives.delete" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Input {
                directory: PathBuf,
                key: String,
                confirmed: bool,
            }
            let input: Input = serde_json::from_value(args.clone()).map_err(|_| invalid())?;
            if !input.confirmed {
                return Err(invalid());
            }
            let entries = inventory(service, root)?;
            if !entries.iter().any(|a| {
                a.deletable && same_directory(&a.directory, &input.directory) && a.key == input.key
            }) {
                return Err(Fault::new(
                    "CONFLICT",
                    6,
                    "Refresh the backup inventory before deleting",
                ));
            }
            // Pending schedule/manual captures are excluded above. File-first ordering
            // leaves retryable receipts if storage or metadata persistence fails.
            if input.directory.exists() {
                BackupStore::open(&input.directory)
                    .map_err(|_| storage())?
                    .remove(&input.key)
                    .map_err(|_| storage())?;
            }
            let mut schedule = service.backup_schedule()?;
            schedule.generations.retain(|g| {
                g.key != input.key || !same_directory(&g.destination, &input.directory)
            });
            schedule.prune.retain(|g| {
                g.key != input.key || !same_directory(&g.destination, &input.directory)
            });
            service.set_backup_schedule(&schedule)?;
            if let Some(intent) = input.key.strip_prefix("manual-")
                && service.manual_backup(intent)?.is_some_and(|m| {
                    m.completed && same_directory(&m.destination, &input.directory)
                })
            {
                service.forget_manual_backup(intent)?;
            }
            Ok(json!({"deleted":true}))
        }
        "restore.cleanup" => {
            if args.as_object().is_none_or(|o| o.len() != 1) || args["confirmed"] != true {
                return Err(invalid());
            }
            // No active restore may lose rollback inputs. Everything here is a
            // discardable preparation or terminal result; no credentials/DB/RULES.
            let safety = root.join("restore-safety");
            if safety.exists() {
                let store = BackupStore::open(&safety).map_err(|_| storage())?;
                for (key, _) in store.list().map_err(|_| storage())? {
                    if safety_key(&key) {
                        store.remove(&key).map_err(|_| storage())?;
                    }
                }
            }
            for name in ["restore-preparation", "restore-transactions"] {
                let directory = root.join(name);
                if directory.exists() {
                    CredentialStore::open(&directory)
                        .map_err(|_| storage())?
                        .clear()
                        .map_err(|_| storage())?;
                }
            }
            Ok(json!({"deleted":true}))
        }
        _ => Err(invalid()),
    }
}
