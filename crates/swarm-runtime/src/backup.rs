//! Trusted backup primitives and controls behind the private administrator GUI gate.
use swarm_core::{BackupKind, BackupManifest, Service, inspect_backup_archive, protocol::Fault};
use swarm_platform::{BackupStore, PlatformError};

/// Called only after the private GUI dispatcher rechecks the human administrator.
pub(crate) fn control(
    service: &mut Service,
    root: &std::path::Path,
    account_id: &str,
    operation: &str,
    args: &serde_json::Value,
) -> Result<serde_json::Value, Fault> {
    use serde_json::json;
    let invalid = || Fault::new("INVALID_INPUT", 2, "Invalid backup control request");
    match operation {
        #[cfg(windows)]
        "archives.list" | "archives.delete" | "restore.cleanup" => {
            crate::storage_cleanup::control(service, root, operation, args)
        }
        #[cfg(windows)]
        "restore.apply" => crate::restore_apply::control(service, root, account_id, args),
        "restore.preview" | "restore.prepare" | "restore.status" => {
            crate::restore::control(service, root, account_id, operation, args)
        }
        "manual.create" | "manual.status" => {
            let intent = args
                .get("intent")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(invalid)?;
            if args.as_object().is_none_or(|o| o.len() != 1)
                || !swarm_core::valid_backup_intent(intent)
            {
                return Err(invalid());
            }
            let existing = service.manual_backup(intent)?;
            if existing
                .as_ref()
                .is_some_and(|r| r.account_id != account_id)
            {
                return Err(Fault::new(
                    "PERMISSION_DENIED",
                    4,
                    "Manual backup belongs to another account",
                ));
            }
            if operation == "manual.status" {
                return Ok(match existing {
                    None => json!({"intent":intent,"state":"not_started"}),
                    Some(r) => {
                        json!({"intent":intent,"key":format!("manual-{intent}"),"destination":r.destination,"state":if r.completed {"completed"} else {"pending"}})
                    }
                });
            }
            let mut captured = None;
            let mut record = match existing {
                Some(r) => r,
                None => {
                    let settings = service.backup_settings()?;
                    let destination = match settings.destination {
                        Some(p) => p,
                        None => root.canonicalize().map_err(|_| storage())?.join("backups"),
                    };
                    let _ = BackupStore::open(&destination).map_err(|_| storage())?;
                    captured = Some(service.prepare_backup()?);
                    let r = swarm_core::ManualBackup {
                        version: 1,
                        account_id: account_id.into(),
                        intent: intent.into(),
                        destination,
                        completed: false,
                    };
                    service.set_manual_backup(&r)?;
                    r
                }
            };
            let key = format!("manual-{intent}");
            let store = BackupStore::open(&record.destination).map_err(|_| storage())?;
            if !store.contains(&key).map_err(|_| storage())? {
                if record.completed {
                    return Err(storage());
                }
                let prepared = match captured {
                    Some(p) => p,
                    None => service.prepare_backup()?,
                };
                let mut failure = None;
                if store
                    .write_new(&key, |file| {
                        prepared
                            .write_archive(file, BackupKind::Manual)
                            .map_err(|e| {
                                failure = Some(e);
                                PlatformError::Io
                            })
                    })
                    .is_err()
                {
                    return Err(failure.unwrap_or_else(storage));
                }
            }
            if !matches!(inspect_backup(&store, &key)?.kind, BackupKind::Manual) {
                return Err(storage());
            }
            if !record.completed {
                record.completed = true;
                service.set_manual_backup(&record)?;
            }
            Ok(
                json!({"intent":intent,"key":key,"destination":record.destination,"state":"completed"}),
            )
        }
        "settings.get" | "status" => {
            if args.as_object().is_none_or(|o| !o.is_empty()) {
                return Err(invalid());
            }
            let settings = service.backup_settings()?;
            if operation == "settings.get" {
                return serde_json::to_value(settings).map_err(|_| storage());
            }
            // A corrupt journal must remain inspectable alongside the controls that
            // disable scheduling. Never reset it or fabricate an empty history.
            let (schedule, journal_error) = match service.backup_schedule() {
                Ok(state) => (Some(state), None),
                Err(error) => (None, Some(error.code)),
            };
            let pending_stage = schedule
                .as_ref()
                .and_then(|s| s.pending.as_ref())
                .map(|intent| {
                    if !intent.generation.destination.exists() {
                        return "publication_pending";
                    }
                    let checked = (|| {
                        let store = BackupStore::open(&intent.generation.destination)
                            .map_err(|_| storage())?;
                        if !store
                            .contains(&intent.generation.key)
                            .map_err(|_| storage())?
                        {
                            return Ok(false);
                        }
                        let manifest = inspect_backup(&store, &intent.generation.key)?;
                        if !matches!(manifest.kind, BackupKind::Scheduled) {
                            return Err(storage());
                        }
                        Ok(true)
                    })();
                    match checked {
                        Ok(true) => "receipt_pending",
                        Ok(false) => "publication_pending",
                        Err(_) => "unavailable",
                    }
                });
            Ok(
                json!({"settings":settings,"schedule":schedule,"journal_error":journal_error,"pending_stage":pending_stage,
                "default_destination":root.canonicalize().map_err(|_|storage())?.join("backups")}),
            )
        }
        "settings.set" => {
            let settings: swarm_core::BackupSettings =
                serde_json::from_value(args.clone()).map_err(|_| invalid())?;
            if !(60..=31_536_000).contains(&settings.interval_seconds)
                || !(1..=100).contains(&settings.retention)
                || settings
                    .destination
                    .as_ref()
                    .is_some_and(|p| !p.is_absolute())
            {
                return Err(invalid());
            }
            if settings.enabled {
                let destination = match &settings.destination {
                    Some(path) => path.clone(),
                    None => root.canonicalize().map_err(|_| storage())?.join("backups"),
                };
                // Existing folders with unrelated ACLs are refused, never repaired.
                let _ = BackupStore::open(&destination).map_err(|_| storage())?;
            }
            service.set_backup_settings(&settings)?;
            serde_json::to_value(settings).map_err(|_| storage())
        }
        _ => Err(Fault::new(
            "PERMISSION_DENIED",
            4,
            "Backup operation is unavailable",
        )),
    }
}

fn storage() -> Fault {
    Fault::new(
        "BACKUP_STORAGE",
        8,
        "Protected backup storage is unavailable",
    )
}

/// The caller must hold the runtime's service gate across capture/publication.
/// GUI authorization, stop handling and generation retention are separate gates.
pub fn save_backup(
    service: &mut Service,
    store: &BackupStore,
    kind: BackupKind,
) -> Result<String, Fault> {
    let prepared = service.prepare_backup()?;
    let prefix = match kind {
        BackupKind::Manual => "manual",
        BackupKind::Scheduled => "scheduled",
        BackupKind::Safety => "safety",
    };
    let key = format!("{prefix}-{}", uuid::Uuid::new_v4());
    let mut failure = None;
    let result = store.write_new(&key, |file| {
        prepared.write_archive(file, kind).map_err(|error| {
            failure = Some(error);
            PlatformError::Io
        })
    });
    if result.is_err() {
        return Err(failure.unwrap_or_else(storage));
    }
    Ok(key)
}

/// Checks format/checksums for a future human preview. Does not apply a restore.
pub fn inspect_backup(store: &BackupStore, key: &str) -> Result<BackupManifest, Fault> {
    let mut file = store.read(key).map_err(|_| storage())?;
    inspect_backup_archive(&mut file)
}

/// Trusted scheduler tick. Returns a newly acknowledged scheduled generation.
pub fn scheduled_tick(
    service: &mut Service,
    root: &std::path::Path,
    now: i64,
    stopped: bool,
) -> Result<Option<String>, Fault> {
    #[cfg(windows)]
    crate::restore_apply::blocked(root)?;
    if stopped {
        return Ok(None);
    }
    if now < 0 {
        return Err(Fault::new("BACKUP_STATE", 8, "Backup clock is unavailable"));
    }
    crate::lifecycle::check_running_allowed(root)?;
    let settings = service.backup_settings()?;
    if !settings.enabled {
        return Ok(None);
    }
    let mut state = service.backup_schedule()?;
    // Persist a prune queue before deleting files. Missing files are idempotent
    // acknowledgements; aliases and corrupt files are never blindly removed.
    while let Some(generation) = state.prune.first().cloned() {
        let store = BackupStore::open(&generation.destination).map_err(|_| storage())?;
        if store.contains(&generation.key).map_err(|_| storage())? {
            let manifest = inspect_backup(&store, &generation.key)?;
            if !matches!(manifest.kind, BackupKind::Scheduled) {
                return Err(storage());
            }
            store.remove(&generation.key).map_err(|_| storage())?;
        }
        state.prune.remove(0);
        service.set_backup_schedule(&state)?;
    }
    let mut captured = None;
    let pending = match state.pending.clone() {
        Some(intent) => intent,
        None => {
            let interval = i64::try_from(settings.interval_seconds).map_err(|_| storage())?;
            if state
                .last_success_at
                .is_some_and(|last| now.saturating_sub(last) < interval)
            {
                return Ok(None);
            }
            let destination = match &settings.destination {
                Some(path) => path.clone(),
                None => root.canonicalize().map_err(|_| storage())?.join("backups"),
            };
            // Validate destination and capture before journaling a publication.
            let _ = BackupStore::open(&destination).map_err(|_| storage())?;
            captured = Some(service.prepare_backup()?);
            let intent = swarm_core::BackupIntent {
                generation: swarm_core::BackupGeneration {
                    key: format!("scheduled-{}", uuid::Uuid::new_v4()),
                    destination,
                },
                due_at: now,
            };
            state.pending = Some(intent.clone());
            service.set_backup_schedule(&state)?;
            intent
        }
    };
    let store = BackupStore::open(&pending.generation.destination).map_err(|_| storage())?;
    if !store
        .contains(&pending.generation.key)
        .map_err(|_| storage())?
    {
        let prepared = match captured {
            Some(prepared) => prepared,
            None => service.prepare_backup()?,
        };
        let mut failure = None;
        if store
            .write_new(&pending.generation.key, |file| {
                prepared
                    .write_archive(file, BackupKind::Scheduled)
                    .map_err(|error| {
                        failure = Some(error);
                        PlatformError::Io
                    })
            })
            .is_err()
        {
            return Err(failure.unwrap_or_else(storage));
        }
    }
    let manifest = inspect_backup(&store, &pending.generation.key)?;
    if !matches!(manifest.kind, BackupKind::Scheduled) {
        return Err(storage());
    }
    state.pending = None;
    // Resuming an overdue intent acknowledges at recovery time, preventing a
    // second catch-up run at the same resume even if multiple days elapsed.
    state.last_success_at = Some(now);
    state.generations.push(pending.generation.clone());
    let excess = state
        .generations
        .len()
        .saturating_sub(settings.retention as usize);
    state.prune.extend(state.generations.drain(..excess));
    service.set_backup_schedule(&state)?;
    // Prune on the next tick as well as here so a crash never loses the queue.
    while let Some(generation) = state.prune.first().cloned() {
        let old_store = BackupStore::open(&generation.destination).map_err(|_| storage())?;
        if old_store.contains(&generation.key).map_err(|_| storage())? {
            if !matches!(
                inspect_backup(&old_store, &generation.key)?.kind,
                BackupKind::Scheduled
            ) {
                return Err(storage());
            }
            old_store.remove(&generation.key).map_err(|_| storage())?;
        }
        state.prune.remove(0);
        service.set_backup_schedule(&state)?;
    }
    Ok(Some(pending.generation.key))
}

#[cfg(windows)]
pub(crate) fn run(
    shared: crate::SharedService,
    stopping: std::sync::Arc<std::sync::atomic::AtomicBool>,
    root: std::path::PathBuf,
) {
    use std::{
        sync::atomic::Ordering,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    let mut next_check = Instant::now();
    let mut reported = false;
    while !stopping.load(Ordering::Acquire) {
        if Instant::now() < next_check {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        let result = (|| {
            let mut guard = shared.lock().map_err(|_| storage())?;
            if stopping.load(Ordering::Acquire) {
                return Ok(None);
            }
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| storage())?
                .as_secs();
            let now = i64::try_from(now).map_err(|_| storage())?;
            scheduled_tick(&mut guard.0, &root, now, false)
        })();
        let delay = match result {
            Ok(_) => {
                reported = false;
                Duration::from_secs(1)
            }
            Err(error) if error.code == "RUNTIME_STOPPED" => break,
            Err(_) => {
                if !reported {
                    eprintln!(
                        "Swarm scheduled backup is unavailable; prior generations are retained"
                    );
                    reported = true;
                }
                Duration::from_secs(60)
            }
        };
        next_check = Instant::now() + delay;
    }
}

#[cfg(all(test, windows))]
mod worker_tests {
    #[test]
    fn worker_publishes_without_gui_and_does_not_run_when_already_stopping() {
        use std::sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        };
        let root = tempfile::tempdir().unwrap();
        let mut service = swarm_core::Service::open(root.path()).unwrap();
        let sessions = crate::session::SessionBridge::open(root.path(), &mut service).unwrap();
        let gui = crate::gui::GuiAccess::open(root.path()).unwrap();
        let shared = Arc::new(Mutex::new((service, sessions, gui)));
        let stop = Arc::new(AtomicBool::new(false));
        let worker_shared = shared.clone();
        let worker_stop = stop.clone();
        let worker_root = root.path().to_owned();
        let worker =
            std::thread::spawn(move || super::run(worker_shared, worker_stop, worker_root));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut completed = false;
        while std::time::Instant::now() < deadline {
            if shared
                .lock()
                .unwrap()
                .0
                .backup_schedule()
                .unwrap()
                .last_success_at
                .is_some()
            {
                completed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        stop.store(true, Ordering::Release);
        worker.join().unwrap();
        assert!(
            completed,
            "runtime worker must publish a scheduled backup without GUI activity"
        );
        let count = std::fs::read_dir(root.path().join("backups"))
            .unwrap()
            .count();
        super::run(shared, stop, root.path().to_owned());
        assert_eq!(
            std::fs::read_dir(root.path().join("backups"))
                .unwrap()
                .count(),
            count
        );
    }
}
