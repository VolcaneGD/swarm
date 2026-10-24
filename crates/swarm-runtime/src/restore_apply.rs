//! Durable restore transaction owner. The shared Service gate serializes writes,
//! workers and waiter polls; a persisted active record closes the crash window.
use super::restore::{fault, inspect, invalid, load, storage, targets};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use swarm_core::{BackupKind, RestoreRulesGuard, Service, inspect_restore_archive};
use swarm_platform::{BackupStore, CredentialStore, PlatformError};

#[cfg(test)]
#[path = "restore_credential_tests.rs"]
mod credential_tests;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Transaction {
    version: u32,
    root: PathBuf,
    intent: String,
    account: String,
    input_sha256: String,
    safety_sha256: String,
    rules_sha256: String,
    ledger: BTreeMap<String, String>,
    state: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    path: PathBuf,
    content: Option<String>,
}
const MAX_RULES_CAPTURE_BYTES: usize = 32 * 1024 * 1024;

struct LimitedCapture {
    bytes: Vec<u8>,
    limit: usize,
    overflowed: bool,
}
impl Write for LimitedCapture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.overflowed = true;
            return Err(std::io::Error::other("Restore capture limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn encode_rules(rules: &[Rule]) -> Result<Vec<u8>, swarm_core::protocol::Fault> {
    let mut capture = LimitedCapture {
        bytes: Vec::new(),
        limit: MAX_RULES_CAPTURE_BYTES,
        overflowed: false,
    };
    serde_json::to_writer(&mut capture, rules).map_err(|_| {
        if capture.overflowed {
            fault(
                "RESTORE_SIZE",
                "RULES rollback capture exceeds storage limit",
            )
        } else {
            storage()
        }
    })?;
    Ok(capture.bytes)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Apply {
    intent: String,
    input_sha256: String,
    confirmed: bool,
}
fn journal(root: &Path) -> Result<CredentialStore, swarm_core::protocol::Fault> {
    CredentialStore::open(&root.join("restore-transactions")).map_err(|_| storage())
}
fn save(
    store: &CredentialStore,
    key: &str,
    t: &Transaction,
) -> Result<(), swarm_core::protocol::Fault> {
    let bytes = serde_json::to_vec(t).map_err(|_| storage())?;
    if bytes.len() > 64 * 1024 {
        return Err(fault(
            "RESTORE_SIZE",
            "Restore transaction exceeds protected storage",
        ));
    }
    store.put(key, &bytes).map_err(|_| storage())
}
fn read(
    store: &CredentialStore,
    root: &Path,
    key: &str,
) -> Result<Option<Transaction>, swarm_core::protocol::Fault> {
    store
        .get(key)
        .map_err(|_| storage())?
        .map(|b| {
            let t: Transaction = serde_json::from_slice(&b).map_err(|_| invalid())?;
            if t.version != 1
                || t.root != root
                || !swarm_core::valid_backup_intent(&t.intent)
                || t.account.is_empty()
                || !super::restore::digest_valid(&t.input_sha256)
                || !super::restore::digest_valid(&t.safety_sha256)
                || !super::restore::digest_valid(&t.rules_sha256)
                || !matches!(t.state.as_str(), "active" | "applied" | "rolled_back")
            {
                return Err(invalid());
            }
            Ok(t)
        })
        .transpose()
}
fn summary(t: &Transaction) -> Value {
    json!({"intent":t.intent,"state":t.state,"input_sha256":t.input_sha256,"reauthentication_required":true,"safety_directory":t.root.join("restore-safety"),"safety_key":format!("apply-safety-{}",t.intent)})
}
fn clear_credentials(root: &Path) -> Result<(), swarm_core::protocol::Fault> {
    // Capability, runtime stop, Google configuration and restore receipts survive.
    // Old issuance/authentication intents and recovery-code memos must not replay.
    for directory in ["credentials", "client-credentials", "gui-credentials"] {
        CredentialStore::open(&root.join(directory))
            .map_err(|_| storage())?
            .clear()
            .map_err(|_| storage())?;
    }
    let state = CredentialStore::open(&root.join("gui-state")).map_err(|_| storage())?;
    for key in ["selection", "pending-auth", "pending-create"] {
        state.remove(key).map_err(|_| storage())?;
    }
    // GuiHost holds its activation mailbox lease across IPC; taking that lease
    // here would deadlock a request waiting for this Service gate. Remove the
    // actual pending record without acquiring the host-only mailbox lease.
    CredentialStore::open(&root.join("notification-state"))
        .map_err(|_| storage())?
        .remove("activation")
        .map_err(|_| storage())?;
    Ok(())
}
fn rollback_rules(
    store: &BackupStore,
    t: &Transaction,
) -> Result<Vec<Rule>, swarm_core::protocol::Fault> {
    let mut bytes = Vec::new();
    store
        .read(&format!("apply-rules-{}", t.intent))
        .map_err(|_| storage())?
        .take(MAX_RULES_CAPTURE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| storage())?;
    if bytes.len() > MAX_RULES_CAPTURE_BYTES
        || format!("{:x}", Sha256::digest(&bytes)) != t.rules_sha256
    {
        return Err(invalid());
    }
    let rules: Vec<Rule> = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if rules.is_empty()
        || rules[0].path != t.root.join("RULES.md")
        || rules.iter().any(|r| {
            !r.path.is_absolute()
                || r.path.file_name().and_then(|n| n.to_str()) != Some("RULES.md")
                || r.content.as_ref().is_some_and(|c| c.len() > 1024 * 1024)
        })
    {
        return Err(invalid());
    }
    Ok(rules)
}
fn rollback(
    service: &mut Service,
    t: &Transaction,
    locked: Option<&mut RestoreRulesGuard>,
) -> Result<(), swarm_core::protocol::Fault> {
    let store = BackupStore::open(&t.root.join("restore-safety")).map_err(|_| storage())?;
    let mut reader = super::restore::HashReader {
        inner: store
            .read(&format!("apply-safety-{}", t.intent))
            .map_err(|_| storage())?,
        digest: Sha256::new(),
    };
    let input = inspect_restore_archive(&mut reader)?;
    if format!("{:x}", reader.digest.finalize()) != t.safety_sha256
        || !matches!(input.manifest().kind, BackupKind::Safety)
    {
        return Err(invalid());
    }
    let rules = rollback_rules(&store, t)?;
    let contents = rules.iter().map(|r| r.content.clone()).collect::<Vec<_>>();
    if let Some(guard) = locked {
        guard.write(&contents)?;
    } else {
        RestoreRulesGuard::acquire(
            &rules.iter().map(|r| r.path.clone()).collect::<Vec<_>>(),
            true,
        )?
        .write(&contents)?;
    }
    service.install_restore(&input, &t.ledger)?;
    clear_credentials(&t.root)
}

/// Runs before SessionBridge opens any old issuance journals or endpoints.
pub(crate) fn recover(
    service: &mut Service,
    root: &Path,
) -> Result<(), swarm_core::protocol::Fault> {
    let root = root.canonicalize().map_err(|_| storage())?;
    if !root.join("restore-transactions").exists() {
        return Ok(());
    }
    let store = journal(&root)?;
    if let Some(mut t) = read(&store, &root, "active")? {
        let key = format!("result-{}", t.intent);
        if let Some(done) = read(&store, &root, &key)? {
            if done.intent != t.intent
                || done.input_sha256 != t.input_sha256
                || done.safety_sha256 != t.safety_sha256
                || done.rules_sha256 != t.rules_sha256
                || done.account != t.account
                || done.ledger != t.ledger
                || done.state == "active"
            {
                return Err(invalid());
            }
            clear_credentials(&root)?;
        } else {
            rollback(service, &t, None)?;
            t.state = "rolled_back".into();
            save(&store, &key, &t)?;
        }
        store.remove("active").map_err(|_| storage())?;
    }
    Ok(())
}

pub(crate) fn control(
    service: &mut Service,
    root: &Path,
    account: &str,
    args: &Value,
) -> Result<Value, swarm_core::protocol::Fault> {
    apply_observed(service, root, account, args, |_| Ok(()))
}
fn apply_observed(
    service: &mut Service,
    root: &Path,
    account: &str,
    args: &Value,
    mut observe: impl FnMut(&str) -> Result<(), swarm_core::protocol::Fault>,
) -> Result<Value, swarm_core::protocol::Fault> {
    let input: Apply = serde_json::from_value(args.clone()).map_err(|_| invalid())?;
    if !input.confirmed
        || !swarm_core::valid_backup_intent(&input.intent)
        || !super::restore::digest_valid(&input.input_sha256)
    {
        return Err(fault(
            "RESTORE_CONFIRMATION",
            "Confirm replacement and reauthentication immediately before applying",
        ));
    }
    let root = root.canonicalize().map_err(|_| storage())?;
    let journal = journal(&root)?;
    if read(&journal, &root, "active")?.is_some() {
        return Err(fault(
            "RESTORE_RECOVERY_REQUIRED",
            "Restart runtime to recover interrupted restoration",
        ));
    }
    let result_key = format!("result-{}", input.intent);
    if let Some(t) = read(&journal, &root, &result_key)? {
        if t.account != account || t.input_sha256 != input.input_sha256 {
            return Err(invalid());
        }
        return Ok(summary(&t));
    }
    let preparation_store =
        CredentialStore::open(&root.join("restore-preparation")).map_err(|_| storage())?;
    let r = load(&preparation_store, &root, account, &input.intent)?.ok_or_else(invalid)?;
    if r.safety_sha256.is_none() || input.input_sha256 != r.input_sha256 {
        return Err(invalid());
    }
    super::restore::checked_safety(&r)?;
    let store = BackupStore::open(&r.directory).map_err(|_| storage())?;
    let mut reader = super::restore::HashReader {
        inner: store.read(&r.key).map_err(|_| storage())?,
        digest: Sha256::new(),
    };
    let selected = inspect_restore_archive(&mut reader)?;
    if format!("{:x}", reader.digest.finalize()) != r.input_sha256
        || targets(&root, selected.manifest()) != r.targets
    {
        return Err(invalid());
    }
    let safety_store = BackupStore::open(&root.join("restore-safety")).map_err(|_| storage())?;
    let safety_key = format!("apply-safety-{}", input.intent);
    let rules_key = format!("apply-rules-{}", input.intent);
    if safety_store.contains(&safety_key).map_err(|_| storage())?
        || safety_store.contains(&rules_key).map_err(|_| storage())?
    {
        return Err(fault(
            "RESTORE_RESTART_REQUIRED",
            "Interrupted capture requires a new restore preparation",
        ));
    }
    // Fresh capture under the shared gate. Registered RULES plus any extra
    // archive destinations are locked before validating this capture again.
    let ledger = service.backup_live_ledger()?;
    let prepared = service.prepare_backup()?;
    let mut paths = prepared
        .rules()
        .iter()
        .map(|r| {
            if r.project_id.is_none() {
                root.join("RULES.md")
            } else {
                PathBuf::from(&r.source_path)
            }
        })
        .collect::<Vec<_>>();
    for target in &r.targets {
        if !paths.contains(&target.target_path) {
            paths.push(target.target_path.clone());
        }
    }
    let mut guard = RestoreRulesGuard::acquire(&paths, false)?;
    let originals = guard.contents();
    for (index, rule) in prepared.rules().iter().enumerate() {
        if originals[index] != rule.content {
            return Err(fault(
                "RULES_CHANGED",
                "RULES changed during restore capture",
            ));
        }
    }
    let rollback_rules = paths
        .iter()
        .zip(&originals)
        .map(|(p, c)| Rule {
            path: p.clone(),
            content: c.clone(),
        })
        .collect::<Vec<_>>();
    let encoded = encode_rules(&rollback_rules)?;
    safety_store
        .write_new(&safety_key, |f| {
            prepared
                .write_archive(f, BackupKind::Safety)
                .map_err(|_| PlatformError::Io)
        })
        .map_err(|_| storage())?;
    safety_store
        .write_new(&rules_key, |f| {
            f.write_all(&encoded).map_err(|_| PlatformError::Io)
        })
        .map_err(|_| storage())?;
    let (_, safety_hash) = inspect(&root.join("restore-safety"), &safety_key)?;
    let mut t = Transaction {
        version: 1,
        root: root.clone(),
        intent: input.intent,
        account: account.into(),
        input_sha256: r.input_sha256,
        safety_sha256: safety_hash,
        rules_sha256: format!("{:x}", Sha256::digest(&encoded)),
        ledger,
        state: "active".into(),
    };
    observe("before_active_saved")?;
    save(&journal, "active", &t)?;
    let mut restored = originals.clone();
    for (target, rule) in r.targets.iter().zip(&selected.manifest().rules) {
        restored[paths
            .iter()
            .position(|p| *p == target.target_path)
            .ok_or_else(invalid)?] = rule.content.clone();
    }
    let applied = (|| {
        observe("captured")?;
        guard.write(&restored)?;
        observe("rules_written")?;
        service.install_restore(&selected, &t.ledger)?;
        observe("database_written")?;
        clear_credentials(&root)?;
        observe("credentials_cleared")?;
        Ok::<_, swarm_core::protocol::Fault>(())
    })();
    if applied.is_err() {
        if rollback(service, &t, Some(&mut guard)).is_err() {
            return Err(fault(
                "RESTORE_RECOVERY_REQUIRED",
                "Restore rollback requires runtime recovery; operations remain blocked",
            ));
        }
        t.state = "rolled_back".into();
    } else {
        t.state = "applied".into();
    }
    // Release delete-pending RULES handles before recording the terminal result.
    drop(guard);
    save(&journal, &result_key, &t)?;
    observe("result_saved")?;
    journal.remove("active").map_err(|_| storage())?;
    Ok(summary(&t))
}

pub(crate) fn blocked(root: &Path) -> Result<(), swarm_core::protocol::Fault> {
    if root.join("restore-transactions").exists()
        && journal(root)?
            .get("active")
            .map_err(|_| storage())?
            .is_some()
    {
        return Err(fault(
            "RESTORE_RECOVERY_REQUIRED",
            "Restore recovery is required before further operations",
        ));
    }
    Ok(())
}

pub(crate) fn status(
    root: &Path,
    account: &str,
    intent: &str,
) -> Result<Option<Value>, swarm_core::protocol::Fault> {
    if !root.join("restore-transactions").exists() {
        return Ok(None);
    }
    read(&journal(root)?, root, &format!("result-{intent}"))?
        .map(|t| {
            if t.account != account {
                return Err(invalid());
            }
            Ok(summary(&t))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct KillOnDrop(std::process::Child);
    impl Drop for KillOnDrop {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    // Test-only archive construction: preserve the actual schema and RULES,
    // fill SQLite pages with bounded blobs, then stream the exact image.
    // No serialized 1 GiB Vec or production database is used by the builder.
    fn enlarge_archive(source: &Path, bytes: u64) {
        let store = BackupStore::open(&source.join("archives")).unwrap();
        let mut original = store.read("selected").unwrap();
        let mut header = [0; 21];
        original.read_exact(&mut header).unwrap();
        assert_eq!(&header[..13], b"SWARMBACKUP1\n");
        let size = u64::from_le_bytes(header[13..].try_into().unwrap());
        let mut encoded = vec![0; size as usize];
        original.read_exact(&mut encoded).unwrap();
        let mut manifest: Value = serde_json::from_slice(&encoded).unwrap();
        let path = source.join("maximum-fixture.sqlite3");
        let mut file = std::fs::File::create(&path).unwrap();
        std::io::copy(&mut original, &mut file).unwrap();
        drop(file);
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("PRAGMA journal_mode=MEMORY; PRAGMA cache_size=-4096;")
            .unwrap();
        let page_size: u64 = db.query_row("PRAGMA page_size", [], |r| r.get(0)).unwrap();
        assert_eq!(bytes % page_size, 0);
        let target = bytes / page_size;
        let pages = || {
            db.query_row("PRAGMA page_count", [], |r| r.get::<_, u64>(0))
                .unwrap()
        };
        let insert = |index: u64, length: u64| {
            db.execute(
                "INSERT INTO app_metadata VALUES(?1,zeroblob(?2))",
                rusqlite::params![format!("maximum-fixture:{index}"), length],
            )
            .unwrap();
        };
        let mut index = 0;
        let chunk = 16 * 1024 * 1024;
        while target - pages() > chunk / page_size + 32 {
            insert(index, chunk);
            index += 1;
        }
        // The final bounded blob is searched using real rollbackable SQLite
        // transactions, until its page count reaches the exact archive cap.
        let (mut low, mut high) = (0, chunk + page_size * 64);
        let final_length = loop {
            assert!(low <= high, "cannot construct exact page boundary");
            let length = low + (high - low) / 2;
            db.execute_batch("SAVEPOINT sizing;").unwrap();
            insert(index, length);
            let count = pages();
            db.execute_batch("ROLLBACK TO sizing; RELEASE sizing;")
                .unwrap();
            match count.cmp(&target) {
                std::cmp::Ordering::Less => low = length + 1,
                std::cmp::Ordering::Greater => high = length - 1,
                std::cmp::Ordering::Equal => break length,
            }
        };
        insert(index, final_length);
        assert_eq!(pages(), target);
        assert_eq!(
            db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        drop(db);
        assert_eq!(std::fs::metadata(&path).unwrap().len(), bytes);
        let mut digest = Sha256::new();
        let mut input = std::fs::File::open(&path).unwrap();
        let mut buffer = [0; 64 * 1024];
        loop {
            let count = input.read(&mut buffer).unwrap();
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
        }
        manifest["database_bytes"] = json!(bytes);
        manifest["database_sha256"] = json!(format!("{:x}", digest.finalize()));
        let encoded = serde_json::to_vec(&manifest).unwrap();
        store
            .write_new("maximum", |output| {
                output
                    .write_all(b"SWARMBACKUP1\n")
                    .map_err(|_| PlatformError::Io)?;
                output
                    .write_all(&(encoded.len() as u64).to_le_bytes())
                    .map_err(|_| PlatformError::Io)?;
                output.write_all(&encoded).map_err(|_| PlatformError::Io)?;
                std::io::copy(
                    &mut std::fs::File::open(&path).map_err(|_| PlatformError::Io)?,
                    output,
                )
                .map_err(|_| PlatformError::Io)?;
                Ok(())
            })
            .unwrap();
    }

    fn process_memory() -> Value {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;
        let mut counters = PROCESS_MEMORY_COUNTERS_EX {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        // SAFETY: the initialized, correctly sized buffer lives through this
        // synchronous test-only call; the current-process pseudo handle is valid.
        assert_ne!(
            unsafe {
                GetProcessMemoryInfo(
                    GetCurrentProcess(),
                    (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX)
                        .cast::<PROCESS_MEMORY_COUNTERS>(),
                    counters.cb,
                )
            },
            0
        );
        json!({"working_set_bytes": counters.WorkingSetSize,
            "peak_working_set_bytes": counters.PeakWorkingSetSize,
            "private_commit_bytes": counters.PrivateUsage,
            "peak_private_commit_bytes": counters.PeakPagefileUsage})
    }

    #[test]
    #[ignore = "memory observation worker; invoke only through the bounded parent test"]
    fn memory_observation_worker() {
        let Ok(root) = std::env::var("SWARM_TEST_MEMORY_ROOT") else {
            return;
        };
        let root = Path::new(&root);
        let args: Value =
            serde_json::from_str(&std::env::var("SWARM_TEST_MEMORY_ARGS").unwrap()).unwrap();
        let rollback = std::env::var("SWARM_TEST_MEMORY_ROLLBACK").unwrap() == "true";
        let mut service = Service::open(root).unwrap();
        let mut samples = vec![json!({"stage":"opened", "memory":process_memory()})];
        let started = std::time::Instant::now();
        let result = apply_observed(&mut service, root, "admin", &args, |stage| {
            samples.push(json!({"stage":stage,"memory":process_memory()}));
            if rollback && stage == "database_written" {
                Err(storage())
            } else {
                Ok(())
            }
        })
        .unwrap();
        samples.push(json!({"stage":"returned","memory":process_memory()}));
        std::fs::write(
            root.join("memory-observation.json"),
            serde_json::to_vec(&json!({
                "result":{"state":result["state"],"reauthentication_required":result["reauthentication_required"]},
                "elapsed_ms":started.elapsed().as_millis(),"samples":samples
            }))
            .unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn bounded_restore_memory_observations() {
        observe_memory_cases(&[0, 32 * 1024 * 1024], None, 60);
    }

    #[test]
    #[ignore = "opt-in exact 1 GiB archive observation; requires 10 GiB disk and 8 GiB free RAM"]
    fn maximum_restore_memory_observations() {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let path: Vec<u16> = std::env::temp_dir()
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        let mut available = 0u64;
        // SAFETY: the temporary-volume path is NUL-terminated, the output lives
        // through the call, and unused output pointers may be null.
        assert_ne!(
            unsafe {
                GetDiskFreeSpaceExW(
                    path.as_ptr(),
                    &mut available,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            },
            0
        );
        assert!(
            available >= 10 * 1024 * 1024 * 1024,
            "maximum fixture requires 10 GiB free temporary-volume space"
        );
        observe_memory_cases(&[0], Some(1024 * 1024 * 1024), 300);
    }

    fn observe_memory_cases(paddings: &[usize], archive_bytes: Option<u64>, seconds: u64) {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        for &padding in paddings {
            for rollback in [false, true] {
                let f = Fixture::new_with_size(Some("# archived local\n"), padding, archive_bytes);
                let expected_large: (i64, i64) = if archive_bytes.is_some() {
                    rusqlite::Connection::open(f._source.path().join("maximum-fixture.sqlite3"))
                        .unwrap()
                        .query_row(
                            "SELECT count(*),sum(length(CAST(value AS BLOB))) FROM app_metadata WHERE key LIKE 'maximum-fixture:%'",
                            [], |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .unwrap()
                } else {
                    (0, 0)
                };
                let before = f.epoch();
                drop(f.service);
                let child = Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--ignored",
                        "--exact",
                        "restore_apply::tests::memory_observation_worker",
                        "--test-threads=1",
                    ])
                    .env("SWARM_TEST_MEMORY_ROOT", f.root.path())
                    .env("SWARM_TEST_MEMORY_ARGS", f.args.to_string())
                    .env("SWARM_TEST_MEMORY_ROLLBACK", rollback.to_string())
                    .creation_flags(0x0800_0000)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap();
                let mut child = KillOnDrop(child);
                let deadline = Instant::now() + Duration::from_secs(seconds);
                loop {
                    if let Some(status) = child.0.try_wait().unwrap() {
                        assert!(status.success());
                        break;
                    }
                    assert!(Instant::now() < deadline, "memory worker timed out");
                    std::thread::sleep(Duration::from_millis(20));
                }
                let report: Value = serde_json::from_slice(
                    &std::fs::read(f.root.path().join("memory-observation.json")).unwrap(),
                )
                .unwrap();
                assert_eq!(
                    report["result"]["state"],
                    if rollback { "rolled_back" } else { "applied" }
                );
                let db = rusqlite::Connection::open(f.root.path().join("swarm.sqlite3")).unwrap();
                let epoch: String = db
                    .query_row(
                        "SELECT value FROM app_metadata WHERE key='changes.cursor_epoch.v1'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_ne!(epoch, before);
                let sentinel: i64 = db
                    .query_row(
                        "SELECT count(*) FROM app_metadata WHERE key='live-sentinel'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                assert_eq!(sentinel, i64::from(rollback));
                let restored_padding: i64 = db.query_row(
                    "SELECT coalesce(sum(length(value)),0) FROM app_metadata WHERE key='restore-observation-padding'",
                    [], |r| r.get(0),
                ).unwrap();
                assert_eq!(restored_padding, if rollback { 0 } else { padding as i64 });
                let restored_large: (i64, i64) = db
                    .query_row(
                        "SELECT count(*),coalesce(sum(length(CAST(value AS BLOB))),0) FROM app_metadata WHERE key LIKE 'maximum-fixture:%'",
                        [],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .unwrap();
                assert_eq!(
                    restored_large,
                    if rollback { (0, 0) } else { expected_large }
                );
                if archive_bytes.is_some() {
                    assert!(expected_large.0 > 0);
                    assert!(expected_large.1 > 1000 * 1024 * 1024);
                    let changed: i64 = db.query_row(
                        "SELECT count(*) FROM app_metadata WHERE key LIKE 'maximum-fixture:%' AND value<>zeroblob(length(value))",
                        [], |r| r.get(0),
                    ).unwrap();
                    assert_eq!(changed, 0, "maximum fixture payload bytes changed");
                }
                assert_eq!(
                    db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
                        .unwrap(),
                    "ok"
                );
                assert!(
                    !db.prepare("PRAGMA foreign_key_check")
                        .unwrap()
                        .exists([])
                        .unwrap()
                );
                assert_eq!(report["result"]["reauthentication_required"], true);
                assert!(
                    journal(f.root.path())
                        .unwrap()
                        .get("active")
                        .unwrap()
                        .is_none()
                );
                assert_eq!(
                    std::fs::read_to_string(f.project.path().join("RULES.md")).unwrap(),
                    if rollback {
                        "# extra target latest live\n"
                    } else {
                        "# archived local\n"
                    }
                );
                for directory in ["credentials", "client-credentials", "gui-credentials"] {
                    assert!(
                        CredentialStore::open(&f.root.path().join(directory))
                            .unwrap()
                            .get("old-secret")
                            .unwrap()
                            .is_none()
                    );
                }
                for sample in report["samples"].as_array().unwrap() {
                    let m = &sample["memory"];
                    assert!(m["working_set_bytes"].as_u64().unwrap() > 0);
                    assert!(
                        m["peak_working_set_bytes"].as_u64().unwrap()
                            >= m["working_set_bytes"].as_u64().unwrap()
                    );
                    assert!(
                        m["peak_private_commit_bytes"].as_u64().unwrap()
                            >= m["private_commit_bytes"].as_u64().unwrap()
                    );
                }
                println!(
                    "{}",
                    json!({"padding_bytes":padding,"database_bytes":archive_bytes,"large_payload_rows":expected_large.0,"large_payload_bytes":expected_large.1,"rollback":rollback,"observation":report})
                );
            }
        }
    }

    #[test]
    #[ignore = "forced-termination worker; invoke only through the bounded parent test"]
    fn forced_termination_worker() {
        let Ok(root) = std::env::var("SWARM_TEST_RESTORE_ROOT") else {
            return;
        };
        let stage = std::env::var("SWARM_TEST_RESTORE_STAGE").unwrap();
        assert!(matches!(
            stage.as_str(),
            "captured"
                | "rules_written"
                | "database_written"
                | "credentials_cleared"
                | "result_saved"
        ));
        let args: Value =
            serde_json::from_str(&std::env::var("SWARM_TEST_RESTORE_ARGS").unwrap()).unwrap();
        let root = Path::new(&root);
        let mut service = Service::open(root).unwrap();
        apply_observed(&mut service, root, "admin", &args, |current| {
            if current == stage {
                std::fs::write(root.join("forced-termination-ready"), current).unwrap();
                // The parent kills this process while SQLite/RULES handles and
                // the active record are still owned here. No unwinding occurs.
                loop {
                    std::thread::park();
                }
            }
            Ok(())
        })
        .unwrap();
        panic!("worker never reached its requested interruption boundary");
    }

    #[test]
    #[ignore = "runtime restart worker; invoke only through the bounded parent test"]
    fn forced_termination_restart_worker() {
        let Ok(root) = std::env::var("SWARM_TEST_RESTORE_ROOT") else {
            return;
        };
        crate::serve_with_gui(Path::new(&root), Some(0), Some(1)).unwrap();
    }

    #[test]
    fn forced_process_termination_recovers_unfinished_apply_and_preserves_saved_result() {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        for stage in [
            "captured",
            "rules_written",
            "database_written",
            "credentials_cleared",
            "result_saved",
        ] {
            let mut f = Fixture::new();
            // Keep the recovered live ledger observable without an ordinary
            // scheduled backup advancing its timestamp immediately on startup.
            f.service
                .set_backup_settings(&swarm_core::BackupSettings {
                    enabled: false,
                    ..Default::default()
                })
                .unwrap();
            let before = f.epoch();
            let child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "restore_apply::tests::forced_termination_worker",
                    "--nocapture",
                ])
                .env("SWARM_TEST_RESTORE_ROOT", f.root.path())
                .env("SWARM_TEST_RESTORE_STAGE", stage)
                .env("SWARM_TEST_RESTORE_ARGS", f.args.to_string())
                .creation_flags(0x0800_0000)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let mut child = KillOnDrop(child);
            let ready = f.root.path().join("forced-termination-ready");
            let deadline = Instant::now() + Duration::from_secs(30);
            while !ready.exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "worker exited before {stage}"
                );
                assert!(Instant::now() < deadline, "worker did not reach {stage}");
                std::thread::sleep(Duration::from_millis(20));
            }
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            let interrupted_epoch = f.epoch();
            assert!(blocked(f.root.path()).is_err());
            let restart = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "restore_apply::tests::forced_termination_restart_worker",
                    "--nocapture",
                ])
                .env("SWARM_TEST_RESTORE_ROOT", f.root.path())
                .creation_flags(0x0800_0000)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let mut restart = KillOnDrop(restart);
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                if let Ok(client) = crate::gui::GuiClient::open(f.root.path())
                    && client.setup_status().is_ok()
                {
                    break;
                }
                assert!(
                    restart.0.try_wait().unwrap().is_none(),
                    "restart exited before endpoint recovery at {stage}"
                );
                assert!(Instant::now() < deadline, "restart did not recover {stage}");
                std::thread::sleep(Duration::from_millis(20));
            }
            loop {
                if let Some(status) = restart.0.try_wait().unwrap() {
                    assert!(status.success(), "runtime restart failed at {stage}");
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "finite runtime did not drain at {stage}"
                );
                std::thread::sleep(Duration::from_millis(20));
            }
            f.service = Service::open(f.root.path()).unwrap();
            assert!(blocked(f.root.path()).is_ok());
            let expected = if stage == "result_saved" {
                assert_eq!(f.epoch(), interrupted_epoch);
                assert_ne!(f.epoch(), before);
                assert_eq!(
                    std::fs::read_to_string(f.project.path().join("RULES.md")).unwrap(),
                    "# archived local\n"
                );
                "applied"
            } else {
                f.assert_rollback(&before);
                "rolled_back"
            };
            assert_eq!(
                control(&mut f.service, f.root.path(), "admin", &f.args).unwrap()["state"],
                expected
            );
            let epoch = f.epoch();
            recover(&mut f.service, f.root.path()).unwrap();
            assert_eq!(f.epoch(), epoch);
            assert!(crate::lifecycle::check_running_allowed(f.root.path()).is_ok());
        }
    }

    fn protected_path(root: &Path, directory: &str, key: &str) -> PathBuf {
        root.join(directory)
            .join(format!("{:x}.bin", Sha256::digest(key.as_bytes())))
    }

    #[test]
    fn failed_active_publication_never_mutates_live_data_or_clears_credentials() {
        let mut f = Fixture::new();
        let before = f.epoch();
        let active = protected_path(f.root.path(), "restore-transactions", "active");
        assert!(
            apply_observed(&mut f.service, f.root.path(), "admin", &f.args, |stage| {
                if stage == "before_active_saved" {
                    std::fs::create_dir(&active).unwrap();
                }
                Ok(())
            })
            .is_err()
        );
        assert_eq!(f.epoch(), before);
        assert_eq!(
            f.service.backup_schedule().unwrap().last_success_at,
            Some(100)
        );
        let current_db = rusqlite::Connection::open(f.root.path().join("swarm.sqlite3")).unwrap();
        assert_eq!(
            current_db
                .query_row(
                    "SELECT value FROM app_metadata WHERE key='live-sentinel'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "newer-than-preparation"
        );
        assert_eq!(
            std::fs::read_to_string(f.root.path().join("RULES.md")).unwrap(),
            "# latest live global\n"
        );
        assert_eq!(
            std::fs::read_to_string(f.project.path().join("RULES.md")).unwrap(),
            "# extra target latest live\n"
        );
        for directory in ["credentials", "client-credentials", "gui-credentials"] {
            assert_eq!(
                CredentialStore::open(&f.root.path().join(directory))
                    .unwrap()
                    .get("old-secret")
                    .unwrap()
                    .unwrap(),
                b"stale"
            );
        }
        assert_eq!(
            CredentialStore::open(&f.root.path().join("notification-state"))
                .unwrap()
                .get("activation")
                .unwrap()
                .unwrap(),
            b"old-notification-click"
        );
        assert!(blocked(f.root.path()).is_err());
        std::fs::remove_dir(&active).unwrap();
        assert!(blocked(f.root.path()).is_ok());
        // Captures were published but no active transaction was committed.
        // Reusing that intent cannot silently substitute another capture.
        assert_eq!(
            control(&mut f.service, f.root.path(), "admin", &f.args)
                .unwrap_err()
                .code,
            "RESTORE_RESTART_REQUIRED"
        );
        assert_eq!(f.epoch(), before);
    }

    #[test]
    fn locked_credential_deletion_keeps_recovery_record_until_storage_is_available() {
        use std::os::windows::fs::OpenOptionsExt;
        let mut f = Fixture::new();
        let before = f.epoch();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(protected_path(
                f.root.path(),
                "client-credentials",
                "old-secret",
            ))
            .unwrap();
        assert_eq!(
            control(&mut f.service, f.root.path(), "admin", &f.args)
                .unwrap_err()
                .code,
            "RESTORE_RECOVERY_REQUIRED"
        );
        assert!(blocked(f.root.path()).is_err());
        assert!(recover(&mut f.service, f.root.path()).is_err());
        assert!(blocked(f.root.path()).is_err());
        drop(lock);
        recover(&mut f.service, f.root.path()).unwrap();
        f.assert_rollback(&before);
        assert!(blocked(f.root.path()).is_ok());
    }

    #[test]
    fn failed_receipt_write_and_active_delete_recover_at_their_distinct_commit_boundaries() {
        use std::os::windows::fs::OpenOptionsExt;
        for fail_receipt in [true, false] {
            let mut f = Fixture::new();
            let before = f.epoch();
            let receipt = protected_path(
                f.root.path(),
                "restore-transactions",
                &format!("result-{}", f.args["intent"].as_str().unwrap()),
            );
            let active = protected_path(f.root.path(), "restore-transactions", "active");
            let mut lock = None;
            let result = apply_observed(&mut f.service, f.root.path(), "admin", &f.args, |stage| {
                if stage == "credentials_cleared" && fail_receipt {
                    // A real invalid destination causes CredentialStore::put to
                    // refuse publication, without mocking DPAPI or writing outside the fixture.
                    std::fs::create_dir(&receipt).unwrap();
                } else if stage == "result_saved" && !fail_receipt {
                    lock = Some(
                        std::fs::OpenOptions::new()
                            .read(true)
                            .share_mode(1)
                            .open(&active)
                            .unwrap(),
                    );
                }
                Ok(())
            });
            assert!(result.is_err());
            assert!(blocked(f.root.path()).is_err());
            let applied_epoch = f.epoch();
            if fail_receipt {
                assert!(recover(&mut f.service, f.root.path()).is_err());
                assert!(blocked(f.root.path()).is_err());
                std::fs::remove_dir(&receipt).unwrap();
            }
            drop(lock);
            recover(&mut f.service, f.root.path()).unwrap();
            if fail_receipt {
                f.assert_rollback(&before);
            } else {
                assert_eq!(f.epoch(), applied_epoch);
                assert_eq!(
                    std::fs::read_to_string(f.project.path().join("RULES.md")).unwrap(),
                    "# archived local\n"
                );
            }
            assert_eq!(
                control(&mut f.service, f.root.path(), "admin", &f.args).unwrap()["state"],
                if fail_receipt {
                    "rolled_back"
                } else {
                    "applied"
                }
            );
            assert!(blocked(f.root.path()).is_ok());
        }
    }
    struct Fixture {
        root: tempfile::TempDir,
        _source: tempfile::TempDir,
        project: tempfile::TempDir,
        service: Service,
        args: Value,
    }
    impl Fixture {
        fn new() -> Self {
            Self::new_with(Some("# archived local\n"))
        }
        fn new_with(archived_local: Option<&str>) -> Self {
            Self::new_with_payload(archived_local, 0)
        }
        fn new_with_payload(archived_local: Option<&str>, padding_bytes: usize) -> Self {
            Self::new_with_size(archived_local, padding_bytes, None)
        }
        fn new_with_size(
            archived_local: Option<&str>,
            padding_bytes: usize,
            archive_bytes: Option<u64>,
        ) -> Self {
            let root = tempfile::tempdir().unwrap();
            let source = tempfile::tempdir().unwrap();
            let project = tempfile::tempdir().unwrap();
            let source_service = Service::open(source.path()).unwrap();
            let db = rusqlite::Connection::open(source.path().join("swarm.sqlite3")).unwrap();
            if padding_bytes > 0 {
                db.execute(
                    "INSERT INTO app_metadata VALUES('restore-observation-padding',?1)",
                    ["p".repeat(padding_bytes)],
                )
                .unwrap();
            }
            db.execute(
                "INSERT INTO projects VALUES('extra','Extra',?1,'now')",
                [project.path().canonicalize().unwrap().to_str().unwrap()],
            )
            .unwrap();
            if let Some(text) = archived_local {
                std::fs::write(project.path().join("RULES.md"), text).unwrap();
            }
            BackupStore::open(&source.path().join("archives"))
                .unwrap()
                .write_new("selected", |f| {
                    source_service
                        .prepare_backup()
                        .unwrap()
                        .write_archive(f, BackupKind::Manual)
                        .map_err(|_| PlatformError::Io)
                })
                .unwrap();
            if let Some(bytes) = archive_bytes {
                enlarge_archive(source.path(), bytes);
            }
            let key = if archive_bytes.is_some() {
                "maximum"
            } else {
                "selected"
            };
            let mut service = Service::open(root.path()).unwrap();
            let preview = super::super::restore::control(
                &mut service,
                root.path(),
                "admin",
                "restore.preview",
                &json!({"directory":source.path().join("archives"),"key":key}),
            )
            .unwrap();
            let intent = uuid::Uuid::new_v4().to_string();
            super::super::restore::control(&mut service,root.path(),"admin","restore.prepare",&json!({"directory":source.path().join("archives"),"key":key,"intent":intent,"input_sha256":preview["input_sha256"],"targets":preview["targets"]})).unwrap();
            std::fs::write(root.path().join("RULES.md"), "# latest live global\n").unwrap();
            std::fs::write(
                project.path().join("RULES.md"),
                "# extra target latest live\n",
            )
            .unwrap();
            let db = rusqlite::Connection::open(root.path().join("swarm.sqlite3")).unwrap();
            db.execute(
                "INSERT INTO app_metadata VALUES('live-sentinel','newer-than-preparation')",
                [],
            )
            .unwrap();
            service
                .set_backup_schedule(&swarm_core::BackupScheduleState {
                    last_success_at: Some(100),
                    ..Default::default()
                })
                .unwrap();
            for directory in ["credentials", "client-credentials", "gui-credentials"] {
                CredentialStore::open(&root.path().join(directory))
                    .unwrap()
                    .put("old-secret", b"stale")
                    .unwrap();
            }
            CredentialStore::open(&root.path().join("notification-state"))
                .unwrap()
                .put("activation", b"old-notification-click")
                .unwrap();
            Self {
                root,
                _source: source,
                project,
                service,
                args: json!({"intent":intent,"input_sha256":preview["input_sha256"],"confirmed":true}),
            }
        }
        fn epoch(&self) -> String {
            rusqlite::Connection::open(self.root.path().join("swarm.sqlite3"))
                .unwrap()
                .query_row(
                    "SELECT value FROM app_metadata WHERE key='changes.cursor_epoch.v1'",
                    [],
                    |r| r.get(0),
                )
                .unwrap()
        }
        fn assert_rollback(&self, before: &str) {
            assert_eq!(
                std::fs::read_to_string(self.root.path().join("RULES.md")).unwrap(),
                "# latest live global\n"
            );
            assert_eq!(
                std::fs::read_to_string(self.project.path().join("RULES.md")).unwrap(),
                "# extra target latest live\n"
            );
            let db = rusqlite::Connection::open(self.root.path().join("swarm.sqlite3")).unwrap();
            assert_eq!(
                db.query_row(
                    "SELECT value FROM app_metadata WHERE key='live-sentinel'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "newer-than-preparation"
            );
            assert_eq!(
                self.service.backup_schedule().unwrap().last_success_at,
                Some(100)
            );
            assert_ne!(self.epoch(), before);
            for directory in ["credentials", "client-credentials", "gui-credentials"] {
                assert!(
                    CredentialStore::open(&self.root.path().join(directory))
                        .unwrap()
                        .get("old-secret")
                        .unwrap()
                        .is_none()
                );
            }
            assert!(
                CredentialStore::open(&self.root.path().join("notification-state"))
                    .unwrap()
                    .get("activation")
                    .unwrap()
                    .is_none()
            );
        }
    }
    #[test]
    fn oversized_rules_capture_refuses_before_publication_or_live_mutation() {
        let mut f = Fixture::new();
        let before = f.epoch();
        let projects = (0..6)
            .map(|_| tempfile::tempdir().unwrap())
            .collect::<Vec<_>>();
        let content = "\u{1}".repeat(1024 * 1024);
        let db = rusqlite::Connection::open(f.root.path().join("swarm.sqlite3")).unwrap();
        for (index, project) in projects.iter().enumerate() {
            std::fs::write(project.path().join("RULES.md"), &content).unwrap();
            db.execute(
                "INSERT INTO projects VALUES(?1,'Large',?2,'now')",
                rusqlite::params![
                    format!("large-{index}"),
                    project.path().canonicalize().unwrap().to_str().unwrap()
                ],
            )
            .unwrap();
        }
        drop(db);
        assert_eq!(
            control(&mut f.service, f.root.path(), "admin", &f.args)
                .unwrap_err()
                .code,
            "RESTORE_SIZE"
        );
        assert_eq!(f.epoch(), before);
        assert_eq!(
            f.service.backup_schedule().unwrap().last_success_at,
            Some(100)
        );
        let current_db = rusqlite::Connection::open(f.root.path().join("swarm.sqlite3")).unwrap();
        assert_eq!(
            current_db
                .query_row(
                    "SELECT value FROM app_metadata WHERE key='live-sentinel'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "newer-than-preparation"
        );
        assert_eq!(
            std::fs::read_to_string(f.root.path().join("RULES.md")).unwrap(),
            "# latest live global\n"
        );
        assert_eq!(
            std::fs::read_to_string(f.project.path().join("RULES.md")).unwrap(),
            "# extra target latest live\n"
        );
        for project in &projects {
            assert_eq!(
                std::fs::read_to_string(project.path().join("RULES.md")).unwrap(),
                content
            );
        }
        let store = BackupStore::open(&f.root.path().join("restore-safety")).unwrap();
        let intent = f.args["intent"].as_str().unwrap();
        for key in [
            format!("apply-safety-{intent}"),
            format!("apply-rules-{intent}"),
        ] {
            assert!(!store.contains(&key).unwrap());
        }
        assert!(
            read(&journal(f.root.path()).unwrap(), f.root.path(), "active")
                .unwrap()
                .is_none()
        );
        for directory in ["credentials", "client-credentials", "gui-credentials"] {
            assert_eq!(
                CredentialStore::open(&f.root.path().join(directory))
                    .unwrap()
                    .get("old-secret")
                    .unwrap()
                    .as_deref(),
                Some(b"stale".as_slice())
            );
        }
    }
    #[test]
    fn bounded_capture_accepts_exact_limit_and_rejects_an_extra_byte() {
        let mut capture = LimitedCapture {
            bytes: Vec::new(),
            limit: 16,
            overflowed: false,
        };
        capture.write_all(b"12345678").unwrap();
        capture.write_all(b"abcdefgh").unwrap();
        capture.flush().unwrap();
        assert_eq!(capture.bytes, b"12345678abcdefgh");
        assert!(!capture.overflowed);
        assert!(capture.write_all(b"x").is_err());
        assert!(capture.overflowed);
        assert_eq!(capture.bytes, b"12345678abcdefgh");
    }
    #[test]
    fn every_apply_failure_rolls_back_latest_db_extra_rules_and_live_ledger() {
        for fail_at in [
            "captured",
            "rules_written",
            "database_written",
            "credentials_cleared",
        ] {
            let mut f = Fixture::new();
            let before = f.epoch();
            let result = apply_observed(&mut f.service, f.root.path(), "admin", &f.args, |stage| {
                if stage == fail_at {
                    Err(storage())
                } else {
                    Ok(())
                }
            })
            .unwrap();
            assert_eq!(result["state"], "rolled_back");
            f.assert_rollback(&before);
            let epoch = f.epoch();
            assert_eq!(
                control(&mut f.service, f.root.path(), "admin", &f.args).unwrap()["state"],
                "rolled_back"
            );
            assert_eq!(f.epoch(), epoch); // terminal retry never applies again
        }
    }

    #[test]
    fn database_write_lock_retains_active_transaction_until_safety_recovery() {
        let mut f = Fixture::new();
        let mut sessions =
            crate::session::SessionBridge::open(f.root.path(), &mut f.service).unwrap();
        let before = f.epoch();
        let db = rusqlite::Connection::open(f.root.path().join("swarm.sqlite3")).unwrap();
        let result = apply_observed(&mut f.service, f.root.path(), "admin", &f.args, |stage| {
            if stage == "rules_written" {
                // Real competing SQLite writer, not an injected storage Fault.
                db.execute_batch("BEGIN IMMEDIATE").unwrap();
            }
            Ok(())
        })
        .unwrap_err();
        assert_eq!(result.code, "RESTORE_RECOVERY_REQUIRED");
        assert_eq!(
            blocked(f.root.path()).unwrap_err().code,
            "RESTORE_RECOVERY_REQUIRED"
        );
        assert_eq!(
            f.epoch(),
            before,
            "failed replacement did not import an epoch"
        );
        let root = f.root.path().canonicalize().unwrap();
        let store = journal(&root).unwrap();
        let active = store.get("active").unwrap().unwrap();
        assert!(
            store
                .get(&format!("result-{}", f.args["intent"].as_str().unwrap()))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            std::fs::read_to_string(f.project.path().join("RULES.md")).unwrap(),
            "# extra target latest live\n"
        );
        // The unresolved transaction blocks all ordinary runtime requests.
        let rejected = crate::dispatch_with_runtime_state(
            &mut f.service,
            &mut sessions,
            swarm_core::protocol::Request {
                operation: "changes.get".into(),
                auth: None,
                args: json!({}),
                context: json!({}),
                request_id: None,
            },
        );
        assert_eq!(rejected.error.unwrap().code, "RESTORE_RECOVERY_REQUIRED");
        assert_eq!(store.get("active").unwrap().unwrap(), active);
        db.execute_batch("ROLLBACK").unwrap();
        drop(db);
        recover(&mut f.service, &root).unwrap();
        f.assert_rollback(&before);
        assert!(blocked(&root).is_ok());
        assert_eq!(
            status(&root, "admin", f.args["intent"].as_str().unwrap())
                .unwrap()
                .unwrap()["state"],
            "rolled_back"
        );
        let recovered_epoch = f.epoch();
        recover(&mut f.service, &root).unwrap();
        assert_eq!(
            f.epoch(),
            recovered_epoch,
            "terminal recovery is not repeated"
        );
    }
    #[test]
    fn interrupted_apply_blocks_writes_and_restart_rolls_back_before_journal_recovery() {
        for crash_at in [
            "captured",
            "rules_written",
            "database_written",
            "credentials_cleared",
        ] {
            let mut f = Fixture::new();
            let before = f.epoch();
            let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = apply_observed(&mut f.service, f.root.path(), "admin", &f.args, |stage| {
                    if stage == crash_at {
                        panic!("simulated process interruption");
                    }
                    Ok(())
                });
            }));
            assert!(panic.is_err());
            assert_eq!(
                blocked(f.root.path()).unwrap_err().code,
                "RESTORE_RECOVERY_REQUIRED"
            );
            assert!(
                crate::backup::scheduled_tick(&mut f.service, f.root.path(), 200, false).is_err()
            );
            f.service = Service::open(f.root.path()).unwrap();
            recover(&mut f.service, f.root.path()).unwrap();
            f.assert_rollback(&before);
            assert_eq!(
                status(
                    &f.root.path().canonicalize().unwrap(),
                    "admin",
                    f.args["intent"].as_str().unwrap()
                )
                .unwrap()
                .unwrap()["state"],
                "rolled_back"
            );
            let epoch = f.epoch();
            recover(&mut f.service, f.root.path()).unwrap();
            assert_eq!(f.epoch(), epoch);
        }
    }
    #[test]
    fn successful_apply_keeps_live_history_and_terminal_retry_is_idempotent() {
        let mut f = Fixture::new();
        let before = f.epoch();
        let notification =
            CredentialStore::open(&f.root.path().join("notification-state")).unwrap();
        let _host_mailbox_lease = notification.lock("activation").unwrap();
        assert_eq!(
            control(&mut f.service, f.root.path(), "admin", &f.args).unwrap()["state"],
            "applied"
        );
        assert!(notification.get("activation").unwrap().is_none());
        assert_eq!(
            std::fs::read_to_string(f.project.path().join("RULES.md")).unwrap(),
            "# archived local\n"
        );
        assert_eq!(
            f.service.backup_schedule().unwrap().last_success_at,
            Some(100)
        );
        assert_ne!(f.epoch(), before);
        let epoch = f.epoch();
        assert_eq!(
            control(&mut f.service, f.root.path(), "admin", &f.args).unwrap()["state"],
            "applied"
        );
        assert_eq!(f.epoch(), epoch);
    }

    #[test]
    fn readable_credential_delete_conflict_requires_recovery_and_then_rolls_back() {
        use std::os::windows::fs::OpenOptionsExt;
        let mut f = Fixture::new();
        let before = f.epoch();
        // Use an otherwise unused encrypted record, never an authentication token.
        // FILE_SHARE_READ permits existing authorization/preflight reads, but
        // prevents the restore owner's DELETE access after install_restore.
        let store = CredentialStore::open(&f.root.path().join("credentials")).unwrap();
        let key = "restore-delete-conflict-canary";
        store.put(key, b"synthetic").unwrap();
        let path = f
            .root
            .path()
            .join("credentials")
            .join(format!("{:x}.bin", Sha256::digest(key.as_bytes())));
        let lease = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(path)
            .unwrap();
        assert_eq!(store.get(key).unwrap().unwrap(), b"synthetic");
        assert_eq!(
            control(&mut f.service, f.root.path(), "admin", &f.args)
                .unwrap_err()
                .code,
            "RESTORE_RECOVERY_REQUIRED"
        );
        let root = f.root.path().canonicalize().unwrap();
        assert_eq!(
            read(&journal(&root).unwrap(), &root, "active")
                .unwrap()
                .unwrap()
                .state,
            "active"
        );
        assert!(
            status(&root, "admin", f.args["intent"].as_str().unwrap())
                .unwrap()
                .is_none()
        );
        assert!(blocked(&root).is_err());
        drop(lease);
        recover(&mut f.service, &root).unwrap();
        f.assert_rollback(&before);
        assert_eq!(
            status(&root, "admin", f.args["intent"].as_str().unwrap())
                .unwrap()
                .unwrap()["state"],
            "rolled_back"
        );
        assert!(store.get(key).unwrap().is_none());
        let epoch = f.epoch();
        recover(&mut f.service, &root).unwrap();
        assert_eq!(f.epoch(), epoch);
    }

    #[test]
    fn missing_original_extra_rules_are_removed_on_rollback_and_archived_absence_is_restored() {
        let mut f = Fixture::new();
        std::fs::remove_file(f.project.path().join("RULES.md")).unwrap();
        assert_eq!(
            apply_observed(&mut f.service, f.root.path(), "admin", &f.args, |stage| {
                if stage == "database_written" {
                    Err(storage())
                } else {
                    Ok(())
                }
            })
            .unwrap()["state"],
            "rolled_back"
        );
        assert!(!f.project.path().join("RULES.md").exists());
        let mut f = Fixture::new_with(None);
        assert_eq!(
            control(&mut f.service, f.root.path(), "admin", &f.args).unwrap()["state"],
            "applied"
        );
        assert!(!f.project.path().join("RULES.md").exists());
        let mut f = Fixture::new_with(None);
        let before = f.epoch();
        assert_eq!(
            apply_observed(&mut f.service, f.root.path(), "admin", &f.args, |stage| {
                if stage == "database_written" {
                    Err(storage())
                } else {
                    Ok(())
                }
            })
            .unwrap()["state"],
            "rolled_back"
        );
        f.assert_rollback(&before);
    }

    #[test]
    fn corrupt_recovery_record_and_safety_are_preserved_and_fail_closed() {
        let mut f = Fixture::new();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = apply_observed(&mut f.service, f.root.path(), "admin", &f.args, |stage| {
                if stage == "database_written" {
                    panic!("interruption")
                };
                Ok(())
            });
        }));
        let root = f.root.path().canonicalize().unwrap();
        let store = journal(&root).unwrap();
        let bytes = store.get("active").unwrap().unwrap();
        let mut record: Value = serde_json::from_slice(&bytes).unwrap();
        record["version"] = json!(2);
        let broken = serde_json::to_vec(&record).unwrap();
        store.put("active", &broken).unwrap();
        assert!(recover(&mut f.service, &root).is_err());
        assert_eq!(store.get("active").unwrap().unwrap(), broken);
        store.put("active", &bytes).unwrap();
        let safety = BackupStore::open(&root.join("restore-safety")).unwrap();
        safety
            .remove(&format!(
                "apply-rules-{}",
                f.args["intent"].as_str().unwrap()
            ))
            .unwrap();
        assert!(recover(&mut f.service, &root).is_err());
        assert!(blocked(&root).is_err());
        assert_eq!(store.get("active").unwrap().unwrap(), bytes);
    }

    #[test]
    fn busy_or_linked_extra_target_rejects_before_any_restore_capture_publication() {
        use std::os::windows::fs::OpenOptionsExt;
        let mut f = Fixture::new();
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(f.project.path().join("RULES.md"))
            .unwrap();
        assert!(control(&mut f.service, f.root.path(), "admin", &f.args).is_err());
        assert_eq!(
            std::fs::read_dir(f.root.path().join("restore-safety"))
                .unwrap()
                .count(),
            1
        );
        drop(lock);
        std::fs::hard_link(
            f.project.path().join("RULES.md"),
            f.project.path().join("alias.md"),
        )
        .unwrap();
        assert!(control(&mut f.service, f.root.path(), "admin", &f.args).is_err());
        assert_eq!(
            std::fs::read_dir(f.root.path().join("restore-safety"))
                .unwrap()
                .count(),
            1
        );
        assert!(blocked(f.root.path()).is_ok());
    }

    #[test]
    fn ordinary_runtime_recovers_before_endpoints_and_drains_on_unresolved_transaction() {
        use std::time::{Duration, Instant};
        let mut f = Fixture::new();
        f.service
            .set_backup_settings(&swarm_core::BackupSettings {
                enabled: false,
                ..Default::default()
            })
            .unwrap();
        let before = f.epoch();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = apply_observed(&mut f.service, f.root.path(), "admin", &f.args, |stage| {
                if stage == "database_written" {
                    panic!("interruption")
                };
                Ok(())
            });
        }));
        let root = f.root.path().canonicalize().unwrap();
        let store = journal(&root).unwrap();
        let interrupted = store.get("active").unwrap().unwrap();
        let path = root.clone();
        let server = std::thread::spawn(move || crate::serve_with_gui(&path, None, None));
        let deadline = Instant::now() + Duration::from_secs(10);
        let client = loop {
            if let Ok(client) = crate::gui::GuiClient::open(&root)
                && client.setup_status().is_ok()
            {
                break client;
            }
            assert!(
                Instant::now() < deadline,
                "runtime recovery did not open endpoints"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        f.assert_rollback(&before);
        // Test-only injection into this isolated store simulates a receipt error
        // after apply. The owner must release its lease without a full-stop flag.
        store
            .remove(&format!("result-{}", f.args["intent"].as_str().unwrap()))
            .unwrap();
        store.put("active", &interrupted).unwrap();
        assert_eq!(
            client.setup_status().unwrap_err().code,
            "RESTORE_RECOVERY_REQUIRED"
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while !server.is_finished() {
            assert!(
                Instant::now() < deadline,
                "recovery drain retained the runtime lease"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        server.join().unwrap().unwrap();
        assert!(crate::lifecycle::check_running_allowed(&root).is_ok());
        assert!(store.get("active").unwrap().is_some());
        recover(&mut f.service, &root).unwrap();
    }
}
