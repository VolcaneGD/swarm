//! Preparation only. No database replacement, RULES writes, or restore permission.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};
use swarm_core::{BackupKind, BackupManifest, Service, inspect_restore_archive, protocol::Fault};
use swarm_platform::{BackupStore, CredentialStore, PlatformError};

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Target {
    pub(super) project_id: Option<String>,
    pub(super) target_path: PathBuf,
    pub(super) revision: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    directory: PathBuf,
    key: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preparation {
    directory: PathBuf,
    key: String,
    input_sha256: String,
    intent: String,
    targets: Vec<Target>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Record {
    version: u32,
    root: PathBuf,
    account_id: String,
    pub(super) directory: PathBuf,
    pub(super) key: String,
    pub(super) input_sha256: String,
    intent: String,
    pub(super) targets: Vec<Target>,
    safety_revision: String,
    pub(super) safety_sha256: Option<String>,
    live_ledger: BTreeMap<String, String>,
}
pub(super) fn fault(code: &str, message: &str) -> Fault {
    Fault::new(code, 8, message)
}
pub(super) fn invalid() -> Fault {
    fault(
        "RESTORE_INVALID",
        "Restore preparation could not be verified",
    )
}
pub(super) fn storage() -> Fault {
    fault(
        "RESTORE_STORAGE",
        "Restore preparation could not be read or saved",
    )
}
pub(super) fn digest_valid(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn input_valid(directory: &Path, key: &str) -> bool {
    directory.is_absolute()
        && directory.as_os_str().len() <= 32768
        && !key.is_empty()
        && key.len() <= 128
}
pub(super) struct HashReader<R> {
    pub(super) inner: R,
    pub(super) digest: Sha256,
}
impl<R: Read> Read for HashReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.digest.update(&buffer[..count]);
        Ok(count)
    }
}
pub(super) fn inspect(directory: &Path, key: &str) -> Result<(BackupManifest, String), Fault> {
    if !input_valid(directory, key) || !directory.is_dir() {
        return Err(invalid());
    }
    let store = BackupStore::open(directory).map_err(|_| storage())?;
    let mut reader = HashReader {
        inner: store.read(key).map_err(|_| storage())?,
        digest: Sha256::new(),
    };
    let checked = inspect_restore_archive(&mut reader)?;
    let manifest =
        serde_json::from_value(serde_json::to_value(checked.manifest()).map_err(|_| invalid())?)
            .map_err(|_| invalid())?;
    Ok((manifest, format!("{:x}", reader.digest.finalize())))
}
pub(super) fn targets(root: &Path, manifest: &BackupManifest) -> Vec<Target> {
    manifest
        .rules
        .iter()
        .map(|r| Target {
            project_id: r.project_id.clone(),
            target_path: if r.project_id.is_none() {
                root.join("RULES.md")
            } else {
                PathBuf::from(&r.source_path)
            },
            revision: r.revision.clone(),
        })
        .collect()
}
fn record_key(account: &str, intent: &str) -> String {
    format!("restore-{:x}-{intent}", Sha256::digest(account.as_bytes()))
}
pub(super) fn load(
    store: &CredentialStore,
    root: &Path,
    account: &str,
    intent: &str,
) -> Result<Option<Record>, Fault> {
    if !swarm_core::valid_backup_intent(intent) {
        return Err(invalid());
    }
    store
        .get(&record_key(account, intent))
        .map_err(|_| storage())?
        .map(|bytes| {
            let r: Record = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
            if r.version != 1
                || r.root != root
                || r.account_id != account
                || r.intent != intent
                || !input_valid(&r.directory, &r.key)
                || !digest_valid(&r.input_sha256)
                || !digest_valid(&r.safety_revision)
                || r.safety_sha256.as_ref().is_some_and(|h| !digest_valid(h))
                || r.targets.is_empty()
                || r.targets.iter().any(|t| {
                    !t.target_path.is_absolute()
                        || t.target_path.file_name().and_then(|s| s.to_str()) != Some("RULES.md")
                })
            {
                return Err(invalid());
            }
            Ok(r)
        })
        .transpose()
}
fn save(store: &CredentialStore, r: &Record) -> Result<(), Fault> {
    let bytes = serde_json::to_vec(r).map_err(|_| storage())?;
    if bytes.len() > 64 * 1024 {
        return Err(fault(
            "RESTORE_SIZE",
            "Restore preparation exceeds protected storage",
        ));
    }
    store
        .put(&record_key(&r.account_id, &r.intent), &bytes)
        .map_err(|_| storage())
}
fn summary(r: &Record) -> Value {
    json!({"intent":r.intent,"state":if r.safety_sha256.is_some(){"prepared"}else{"safety_pending"},"input_sha256":r.input_sha256,"targets":r.targets,"safety_directory":r.root.join("restore-safety"),"safety_key":format!("safety-{}",r.intent),"safety_sha256":r.safety_sha256})
}
pub(super) fn checked_safety(r: &Record) -> Result<String, Fault> {
    let (manifest, hash) = inspect(
        &r.root.join("restore-safety"),
        &format!("safety-{}", r.intent),
    )?;
    if !matches!(manifest.kind, BackupKind::Safety)
        || manifest.content_revision()? != r.safety_revision
        || r.safety_sha256
            .as_ref()
            .is_some_and(|expected| expected != &hash)
    {
        return Err(fault(
            "RESTORE_CONFLICT",
            "Safety backup differs from the captured state",
        ));
    }
    Ok(hash)
}

/// Dispatcher has already checked current human identity, administrator and stop.
pub(crate) fn control(
    service: &mut Service,
    root: &Path,
    account: &str,
    operation: &str,
    args: &Value,
) -> Result<Value, Fault> {
    let root = root.canonicalize().map_err(|_| storage())?;
    match operation {
        "restore.preview" => {
            let input: Input = serde_json::from_value(args.clone()).map_err(|_| invalid())?;
            let (manifest, hash) = inspect(&input.directory, &input.key)?;
            Ok(json!({"input_sha256":hash,"targets":targets(&root,&manifest),"manifest":manifest}))
        }
        "restore.status" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Status {
                intent: String,
            }
            let input: Status = serde_json::from_value(args.clone()).map_err(|_| invalid())?;
            if !swarm_core::valid_backup_intent(&input.intent) {
                return Err(invalid());
            }
            #[cfg(windows)]
            if let Some(result) = crate::restore_apply::status(&root, account, &input.intent)? {
                return Ok(result);
            }
            let store =
                CredentialStore::open(&root.join("restore-preparation")).map_err(|_| storage())?;
            let r = load(&store, &root, account, &input.intent)?.ok_or_else(invalid)?;
            if r.safety_sha256.is_some() {
                checked_safety(&r)?;
            }
            Ok(summary(&r))
        }
        "restore.prepare" => {
            let input: Preparation = serde_json::from_value(args.clone()).map_err(|_| invalid())?;
            if !swarm_core::valid_backup_intent(&input.intent) || !digest_valid(&input.input_sha256)
            {
                return Err(invalid());
            }
            let (manifest, hash) = inspect(&input.directory, &input.key)?;
            if hash != input.input_sha256 {
                return Err(fault(
                    "RESTORE_CHANGED",
                    "Selected restore input changed; inspect it again",
                ));
            }
            let target = targets(&root, &manifest);
            if target != input.targets {
                return Err(fault(
                    "RESTORE_CONFIRMATION",
                    "Confirm the exact RULES destinations and revisions",
                ));
            }
            let directory = input.directory.canonicalize().map_err(|_| storage())?;
            let store =
                CredentialStore::open(&root.join("restore-preparation")).map_err(|_| storage())?;
            let mut captured = None;
            let mut r = match load(&store, &root, account, &input.intent)? {
                Some(r) => {
                    if r.directory != directory
                        || r.key != input.key
                        || r.input_sha256 != hash
                        || r.targets != target
                    {
                        return Err(fault(
                            "RESTORE_CONFLICT",
                            "Restore intent belongs to another selection",
                        ));
                    }
                    r
                }
                None => {
                    let destination = root.join("restore-safety");
                    if destination.exists()
                        && BackupStore::open(&destination)
                            .map_err(|_| storage())?
                            .contains(&format!("safety-{}", input.intent))
                            .map_err(|_| storage())?
                    {
                        return Err(fault(
                            "RESTORE_CONFLICT",
                            "Safety backup already exists without this preparation",
                        ));
                    }
                    let ledger = service.backup_live_ledger()?;
                    let prepared = service.prepare_backup()?;
                    let revision = prepared.content_revision()?;
                    captured = Some(prepared);
                    let r = Record {
                        version: 1,
                        root: root.clone(),
                        account_id: account.into(),
                        directory,
                        key: input.key,
                        input_sha256: hash,
                        intent: input.intent,
                        targets: target,
                        safety_revision: revision,
                        safety_sha256: None,
                        live_ledger: ledger,
                    };
                    save(&store, &r)?;
                    r
                }
            };
            let safety = BackupStore::open(&root.join("restore-safety")).map_err(|_| storage())?;
            let key = format!("safety-{}", r.intent);
            if !safety.contains(&key).map_err(|_| storage())? {
                let prepared = captured.ok_or_else(|| {
                    fault(
                        "RESTORE_RESTART_REQUIRED",
                        "Safety publication was interrupted; start a new preparation",
                    )
                })?;
                safety
                    .write_new(&key, |f| {
                        prepared
                            .write_archive(f, BackupKind::Safety)
                            .map_err(|_| PlatformError::Io)
                    })
                    .map_err(|_| storage())?;
            }
            let hash = checked_safety(&r)?;
            if r.safety_sha256.is_none() {
                r.safety_sha256 = Some(hash);
                save(&store, &r)?;
            }
            Ok(summary(&r))
        }
        _ => Err(invalid()),
    }
}
