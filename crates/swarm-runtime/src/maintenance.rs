//! Trusted maintenance while the runtime is running; no public purge endpoint.
#[cfg(windows)]
pub(crate) fn run(
    shared: crate::SharedService,
    stopping: std::sync::Arc<std::sync::atomic::AtomicBool>,
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
            let maintenance_fault = || {
                swarm_core::protocol::Fault::new(
                    "MAINTENANCE_FAILED",
                    8,
                    "Data maintenance could not finish",
                )
            };
            let mut guard = shared.lock().map_err(|_| maintenance_fault())?;
            if stopping.load(Ordering::Acquire) {
                return Ok(());
            }
            let now = i64::try_from(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| maintenance_fault())?
                    .as_secs(),
            )
            .map_err(|_| maintenance_fault())?;
            tick(&mut guard.0, now)?;
            Ok::<_, swarm_core::protocol::Fault>(())
        })();
        match result {
            Ok(()) => reported = false,
            Err(_) if !reported => {
                eprintln!(
                    "Swarm data maintenance could not finish; check retention settings and storage"
                );
                reported = true;
            }
            Err(_) => {}
        }
        next_check = Instant::now() + Duration::from_secs(60);
    }
}

#[cfg(windows)]
fn tick(service: &mut swarm_core::Service, now: i64) -> Result<(), swarm_core::protocol::Fault> {
    crate::restore_apply::blocked(service.data_root())?;
    crate::lifecycle::check_running_allowed(service.data_root())?;
    service.prune_cursor_metadata(now)?;
    service.purge_expired_data()?;
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn restore_recovery_gate_preserves_metadata_before_maintenance() {
        let root = tempfile::tempdir().unwrap();
        let mut service = swarm_core::Service::open(root.path()).unwrap();
        let db = rusqlite::Connection::open(root.path().join("swarm.sqlite3")).unwrap();
        db.execute(
            "INSERT INTO app_metadata VALUES('changes.cursor.token.expired-fixture','{}')",
            [],
        )
        .unwrap();
        let journal =
            swarm_platform::CredentialStore::open(&root.path().join("restore-transactions"))
                .unwrap();
        journal
            .put("active", b"protected recovery fixture")
            .unwrap();
        assert_eq!(
            super::tick(&mut service, 2_000_000).unwrap_err().code,
            "RESTORE_RECOVERY_REQUIRED"
        );
        assert_eq!(db.query_row("SELECT count(*) FROM app_metadata WHERE key='changes.cursor.token.expired-fixture'", [], |row| row.get::<_, i64>(0)).unwrap(), 1);
        assert!(journal.get("active").unwrap().is_some());
    }
}
