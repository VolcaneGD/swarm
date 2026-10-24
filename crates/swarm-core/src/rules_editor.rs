use crate::protocol::Fault;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[cfg(windows)]
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

const MAX_RULES_BYTES: usize = 1024 * 1024;
const MISSING_REVISION: &str = "missing:v1";

fn invalid(message: &str) -> Fault {
    Fault::new("INVALID_INPUT", 2, message)
}

fn storage() -> Fault {
    Fault::internal()
}

fn changed(message: &str) -> Fault {
    Fault::new("RULES_CHANGED", 6, message)
}

pub(crate) fn revision(bytes: Option<&[u8]>) -> String {
    match bytes {
        None => MISSING_REVISION.to_owned(),
        Some(bytes) => format!("sha256:{:x}", Sha256::digest(bytes)),
    }
}

fn read_bytes(path: &Path) -> Result<Option<Vec<u8>>, Fault> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(storage()),
        #[cfg(windows)]
        Ok(metadata) if metadata.file_attributes() & 0x0000_0400 != 0 => Err(storage()),
        Ok(metadata) if metadata.len() > MAX_RULES_BYTES as u64 => {
            Err(invalid("RULES.md must not exceed 1 MiB"))
        }
        Ok(_) => {
            // Bound allocation even if the file grows after metadata was read.
            let mut bytes = Vec::new();
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
            let file = options.open(path).map_err(|_| storage())?;
            #[cfg(windows)]
            {
                let metadata = file.metadata().map_err(|_| storage())?;
                if metadata.file_attributes() & 0x0000_0400 != 0 || hardlink_count(&file)? > 1 {
                    return Err(storage());
                }
            }
            file.take((MAX_RULES_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|_| storage())?;
            if bytes.len() > MAX_RULES_BYTES {
                return Err(invalid("RULES.md must not exceed 1 MiB"));
            }
            Ok(Some(bytes))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(storage()),
    }
}

pub(crate) fn read(path: &Path, missing_allowed: bool) -> Result<(String, String), Fault> {
    let bytes = read_bytes(path)?;
    let Some(bytes) = bytes else {
        if !missing_allowed {
            return Err(Fault::new(
                "RULES_INVALID",
                4,
                "The global RULES.md file is unavailable",
            ));
        }
        return Ok((String::new(), revision(None)));
    };
    let source = String::from_utf8(bytes.clone())
        .map_err(|_| Fault::new("RULES_INVALID", 4, "RULES.md is not valid UTF-8"))?;
    Ok((source, revision(Some(&bytes))))
}

/// Backup must not follow a registered directory that has since become an alias.
pub(crate) fn read_for_backup(
    path: &Path,
    missing_allowed: bool,
) -> Result<(String, String), Fault> {
    for ancestor in path.parent().ok_or_else(storage)?.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let metadata = fs::symlink_metadata(ancestor).map_err(|_| storage())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(storage());
        }
        #[cfg(windows)]
        if metadata.file_attributes() & 0x0000_0400 != 0 {
            return Err(storage());
        }
    }
    read(path, missing_allowed)
}

pub(crate) fn valid_recovery_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn recovery_path(path: &Path, id: &str) -> Result<PathBuf, Fault> {
    if !valid_recovery_id(id) {
        return Err(invalid("Invalid RULES recovery identifier"));
    }
    let parent = path.parent().ok_or_else(storage)?;
    let name = path.file_name().ok_or_else(storage)?.to_string_lossy();
    Ok(parent.join(format!(".{name}.recovery-{id}")))
}

fn check_safe_ancestors(path: &Path) -> Result<(), Fault> {
    for ancestor in path.parent().ok_or_else(storage)?.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let metadata = fs::symlink_metadata(ancestor).map_err(|_| storage())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(storage());
        }
        #[cfg(windows)]
        if metadata.file_attributes() & 0x0000_0400 != 0 {
            return Err(storage());
        }
    }
    Ok(())
}

fn read_recovery_file(path: &Path, missing_allowed: bool) -> Result<Option<Vec<u8>>, Fault> {
    check_safe_ancestors(path)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(storage()),
        #[cfg(windows)]
        Ok(metadata) if metadata.file_attributes() & 0x0000_0400 != 0 => Err(storage()),
        Ok(metadata) if metadata.len() > MAX_RULES_BYTES as u64 => {
            Err(invalid("RULES.md recovery file must not exceed 1 MiB"))
        }
        Ok(_) => {
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            {
                options.share_mode(0x0000_0001).custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
            }
            let file = options.open(path).map_err(|_| storage())?;
            let metadata = file.metadata().map_err(|_| storage())?;
            if !metadata.is_file() {
                return Err(storage());
            }
            #[cfg(windows)]
            if metadata.file_attributes() & 0x0000_0400 != 0 || hardlink_count(&file)? != 1 {
                return Err(storage());
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.nlink() != 1 {
                    return Err(storage());
                }
            }
            let mut bytes = Vec::new();
            file.take((MAX_RULES_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|_| storage())?;
            if bytes.len() > MAX_RULES_BYTES {
                return Err(invalid("RULES.md recovery file must not exceed 1 MiB"));
            }
            Ok(Some(bytes))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && missing_allowed => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(changed(
            "RULES recovery copy is no longer available; inspect it again",
        )),
        Err(_) => Err(storage()),
    }
}

pub(crate) fn recovery_snapshot(
    path: &Path,
    recovery_id: &str,
) -> Result<(Option<Vec<u8>>, Vec<u8>), Fault> {
    let current = read_recovery_file(path, true)?;
    let candidate_path = recovery_path(path, recovery_id)?;
    let candidate = read_recovery_file(&candidate_path, false)?
        .ok_or_else(|| changed("RULES recovery copy is no longer available; inspect it again"))?;
    Ok((current, candidate))
}

pub(crate) fn read_recovery_candidate(path: &Path, recovery_id: &str) -> Result<Vec<u8>, Fault> {
    let candidate_path = recovery_path(path, recovery_id)?;
    read_recovery_file(&candidate_path, false)?
        .ok_or_else(|| changed("RULES recovery copy is no longer available; inspect it again"))
}

pub(crate) fn save(path: &Path, source: &str, expected_revision: &str) -> Result<String, Fault> {
    if source.len() > MAX_RULES_BYTES {
        return Err(invalid("RULES.md must not exceed 1 MiB"));
    }
    #[cfg(windows)]
    {
        save_windows(path, source, expected_revision)
    }
    #[cfg(not(windows))]
    save_nonwindows(path, source, expected_revision)
}

#[cfg(not(windows))]
fn save_nonwindows(path: &Path, source: &str, expected_revision: &str) -> Result<String, Fault> {
    let current = read_bytes(path)?;
    if revision(current.as_deref()) != expected_revision {
        return Err(changed("RULES.md changed; reload it before saving"));
    }

    let parent = path.parent().ok_or_else(storage)?;
    let name = path.file_name().ok_or_else(storage)?.to_string_lossy();
    let temporary = parent.join(format!(".{name}.{}.tmp", Uuid::new_v4()));
    let _temporary = RemoveOnDrop(temporary.clone());
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| storage())?;
    file.write_all(source.as_bytes()).map_err(|_| storage())?;
    file.flush().map_err(|_| storage())?;
    file.sync_all().map_err(|_| storage())?;
    drop(file);

    // This fallback is not the Windows publication contract. Recheck just
    // before rename; Windows uses an exclusive handle through verification.
    let latest = read_bytes(path)?;
    if revision(latest.as_deref()) != expected_revision {
        return Err(changed("RULES.md changed; reload it before saving"));
    }

    fs::rename(&temporary, path).map_err(|_| storage())?;

    Ok(revision(Some(source.as_bytes())))
}

#[cfg(not(windows))]
struct RemoveOnDrop(PathBuf);

#[cfg(not(windows))]
impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(all(windows, test))]
fn wide(path: &Path) -> Result<Vec<u16>, Fault> {
    use std::os::windows::ffi::OsStrExt;
    Ok(path.as_os_str().encode_wide().chain(Some(0)).collect())
}

#[cfg(windows)]
pub(crate) fn hardlink_count(file: &std::fs::File) -> Result<u32, Fault> {
    use std::{ffi::c_void, os::windows::io::AsRawHandle};

    #[repr(C)]
    #[derive(Default)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct ByHandleFileInformation {
        attributes: u32,
        creation: FileTime,
        access: FileTime,
        write: FileTime,
        size_high: u32,
        size_low: u32,
        volume_serial: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            file: *mut c_void,
            information: *mut ByHandleFileInformation,
        ) -> i32;
    }

    let mut information = ByHandleFileInformation::default();
    let result =
        unsafe { GetFileInformationByHandle(file.as_raw_handle().cast(), &mut information) };
    if result == 0 {
        return Err(storage());
    }
    Ok(information.links)
}

#[cfg(windows)]
fn busy(message: &str) -> Fault {
    Fault::new("RULES_BUSY", 6, message)
}

#[cfg(windows)]
fn uncertain(id: &str) -> Fault {
    Fault::new(
        "RULES_SAVE_UNCERTAIN",
        6,
        &format!(
            "RULES.md may have changed during save. Recovery ID: {id}. Reload or recover before saving again."
        ),
    )
}

#[cfg(windows)]
fn unique_path(parent: &Path, name: &str, kind: &str) -> (PathBuf, String) {
    let id = Uuid::new_v4().simple().to_string();
    (parent.join(format!(".{name}.{kind}-{id}")), id)
}

#[cfg(windows)]
struct BackupGuard {
    path: PathBuf,
    id: String,
    keep: bool,
}

#[cfg(windows)]
impl BackupGuard {
    fn remove_after_success(&mut self) -> Result<(), Fault> {
        self.keep = true;
        fs::remove_file(&self.path).map_err(|_| uncertain(&self.id))?;
        self.keep = false;
        Ok(())
    }

    fn retain(&mut self) {
        self.keep = true;
    }
}

#[cfg(windows)]
impl Drop for BackupGuard {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(windows)]
struct TempGuard(PathBuf);

#[cfg(windows)]
impl Drop for TempGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum SaveStage {
    TargetLocked,
    BackupSynced,
    Truncated,
    PoisonSynced,
    SourcePrefixWritten,
    SourceWritten,
}

#[cfg(windows)]
fn save_windows(path: &Path, source: &str, expected_revision: &str) -> Result<String, Fault> {
    save_windows_with_hook(path, source, expected_revision, |_| {})
}

#[cfg(windows)]
fn save_windows_with_hook(
    path: &Path,
    source: &str,
    expected_revision: &str,
    mut stage_hook: impl FnMut(SaveStage),
) -> Result<String, Fault> {
    let parent = path.parent().ok_or_else(storage)?;
    let name = path.file_name().ok_or_else(storage)?.to_string_lossy();
    let (temporary_path, _) = unique_path(parent, &name, "tmp");
    let _temporary = TempGuard(temporary_path.clone());
    let mut staged = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)
        .map_err(|_| storage())?;
    staged.write_all(source.as_bytes()).map_err(|_| storage())?;
    staged.sync_all().map_err(|_| storage())?;
    drop(staged);
    let staged_source = fs::read(&temporary_path).map_err(|_| storage())?;
    if staged_source != source.as_bytes() {
        return Err(storage());
    }

    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .share_mode(0)
        .custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    let mut target_file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if expected_revision != MISSING_REVISION {
                return Err(changed("RULES.md was removed; reload before saving"));
            }
            return create_missing(path, source, &mut stage_hook);
        }
        Err(error) if is_sharing_error(&error) => {
            return Err(busy(
                "RULES.md is open by another process; close it and retry",
            ));
        }
        Err(_) => return Err(storage()),
    };

    let metadata = target_file.metadata().map_err(|_| storage())?;
    if !metadata.is_file()
        || metadata.file_attributes() & 0x0000_0400 != 0
        || hardlink_count(&target_file)? != 1
    {
        return Err(busy(
            "RULES.md is linked or redirected and cannot be saved safely",
        ));
    }
    if metadata.len() > MAX_RULES_BYTES as u64 {
        return Err(invalid("RULES.md must not exceed 1 MiB"));
    }
    let original = read_handle(&mut target_file)?;
    if revision(Some(&original)) != expected_revision {
        return Err(changed("RULES.md changed; reload it before saving"));
    }
    stage_hook(SaveStage::TargetLocked);

    let (backup_path, recovery_id) = unique_path(parent, &name, "recovery");
    let mut backup = BackupGuard {
        path: backup_path.clone(),
        id: recovery_id,
        keep: false,
    };
    let mut backup_file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup_path)
    {
        Ok(file) => file,
        Err(_) => return Err(storage()),
    };
    if backup_file.write_all(&original).is_err() || backup_file.sync_all().is_err() {
        return Err(storage());
    }
    drop(backup_file);
    backup.retain();
    stage_hook(SaveStage::BackupSynced);

    // Keep an invalid duplicate rules fence at EOF until the complete new
    // source is durable. Readers cannot open the target while its share-0
    // handle is held. After a crash, parser-visible state is invalid or the
    // complete intended policy if the user's tail already hides the sentinel.
    let poison = b"\n~~~swarm-rules\n";
    target_file.set_len(0).map_err(|_| uncertain(&backup.id))?;
    target_file.sync_all().map_err(|_| uncertain(&backup.id))?;
    stage_hook(SaveStage::Truncated);
    target_file
        .seek(SeekFrom::Start(source.len() as u64))
        .map_err(|_| uncertain(&backup.id))?;
    target_file
        .write_all(poison)
        .map_err(|_| uncertain(&backup.id))?;
    target_file.sync_all().map_err(|_| uncertain(&backup.id))?;
    stage_hook(SaveStage::PoisonSynced);
    target_file
        .seek(SeekFrom::Start(0))
        .map_err(|_| uncertain(&backup.id))?;
    let source_prefix_len = staged_source.len() / 2;
    target_file
        .write_all(&staged_source[..source_prefix_len])
        .map_err(|_| uncertain(&backup.id))?;
    target_file.sync_all().map_err(|_| uncertain(&backup.id))?;
    stage_hook(SaveStage::SourcePrefixWritten);
    target_file
        .write_all(&staged_source[source_prefix_len..])
        .map_err(|_| uncertain(&backup.id))?;
    target_file.sync_all().map_err(|_| uncertain(&backup.id))?;
    stage_hook(SaveStage::SourceWritten);
    target_file
        .set_len(source.len() as u64)
        .map_err(|_| uncertain(&backup.id))?;
    target_file.sync_all().map_err(|_| uncertain(&backup.id))?;
    let published = read_handle(&mut target_file).map_err(|_| uncertain(&backup.id))?;
    if published != source.as_bytes() {
        backup.retain();
        return Err(uncertain(&backup.id));
    }
    let new_revision = revision(Some(&published));
    backup.remove_after_success()?;
    Ok(new_revision)
}

#[cfg(windows)]
fn read_handle(file: &mut std::fs::File) -> Result<Vec<u8>, Fault> {
    let metadata = file.metadata().map_err(|_| storage())?;
    if !metadata.is_file()
        || metadata.file_attributes() & 0x0000_0400 != 0
        || hardlink_count(file)? != 1
    {
        return Err(busy(
            "RULES.md is linked or redirected and cannot be saved safely",
        ));
    }
    if metadata.len() > MAX_RULES_BYTES as u64 {
        return Err(invalid("RULES.md must not exceed 1 MiB"));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.seek(SeekFrom::Start(0)).map_err(|_| storage())?;
    (&mut *file)
        .take((MAX_RULES_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| storage())?;
    if bytes.len() > MAX_RULES_BYTES {
        return Err(invalid("RULES.md must not exceed 1 MiB"));
    }
    Ok(bytes)
}

#[cfg(windows)]
fn is_sharing_error(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(32 | 33))
}

#[cfg(windows)]
fn create_missing(
    path: &Path,
    source: &str,
    stage_hook: &mut impl FnMut(SaveStage),
) -> Result<String, Fault> {
    let parent = path.parent().ok_or_else(storage)?;
    let name = path.file_name().ok_or_else(storage)?.to_string_lossy();
    let (backup_path, recovery_id) = unique_path(parent, &name, "recovery");
    let mut backup = BackupGuard {
        path: backup_path.clone(),
        id: recovery_id,
        keep: false,
    };
    let mut backup_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup_path)
        .map_err(|_| storage())?;
    backup_file
        .write_all(source.as_bytes())
        .map_err(|_| storage())?;
    backup_file.sync_all().map_err(|_| storage())?;
    drop(backup_file);
    let mut options = OpenOptions::new();
    options
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(0)
        .custom_flags(0x0020_0000);
    let mut target_file = match options.open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(changed(
                "RULES.md was created externally; reload before saving",
            ));
        }
        Err(error) if is_sharing_error(&error) => {
            return Err(busy(
                "RULES.md was created by another process; reload and retry",
            ));
        }
        Err(_) => return Err(storage()),
    };
    // CREATE_NEW acquires the pathname and this share-0 handle in one
    // operation. Other editors cannot replace the new file during publication.
    backup.retain();
    stage_hook(SaveStage::TargetLocked);
    stage_hook(SaveStage::BackupSynced);

    let poison = b"\n~~~swarm-rules\n";
    target_file
        .seek(SeekFrom::Start(source.len() as u64))
        .map_err(|_| uncertain(&backup.id))?;
    target_file
        .write_all(poison)
        .map_err(|_| uncertain(&backup.id))?;
    target_file.sync_all().map_err(|_| uncertain(&backup.id))?;
    stage_hook(SaveStage::PoisonSynced);
    target_file
        .seek(SeekFrom::Start(0))
        .map_err(|_| uncertain(&backup.id))?;
    let source_prefix_len = source.len() / 2;
    target_file
        .write_all(&source.as_bytes()[..source_prefix_len])
        .map_err(|_| uncertain(&backup.id))?;
    target_file.sync_all().map_err(|_| uncertain(&backup.id))?;
    stage_hook(SaveStage::SourcePrefixWritten);
    target_file
        .write_all(&source.as_bytes()[source_prefix_len..])
        .map_err(|_| uncertain(&backup.id))?;
    target_file.sync_all().map_err(|_| uncertain(&backup.id))?;
    stage_hook(SaveStage::SourceWritten);
    target_file
        .set_len(source.len() as u64)
        .map_err(|_| uncertain(&backup.id))?;
    target_file.sync_all().map_err(|_| uncertain(&backup.id))?;
    let published = match read_handle(&mut target_file) {
        Ok(bytes) => bytes,
        Err(_) => {
            backup.retain();
            return Err(uncertain(&backup.id));
        }
    };
    if published != source.as_bytes() {
        backup.retain();
        return Err(uncertain(&backup.id));
    }
    let new_revision = revision(Some(&published));
    backup.remove_after_success()?;
    Ok(new_revision)
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use std::{
        process::{Command, abort},
        sync::{Arc, Barrier},
    };

    fn original() -> &'static str {
        "# Existing\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules: []\naccess:\n  boards: []\n  threads: []\n~~~\n"
    }

    fn source(tail: &str) -> String {
        format!(
            "# New policy\n\n<!-- prefix Markdown comment -->\n\n~~~text\nMarkdown fence example\n~~~\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: false\nrules:\n  - id: crash-allow\n    effect: allow\n    actions: [post.create]\n    when:\n      field: context.flag\n      op: eq\n      value: permit\n  - id: crash-deny\n    effect: deny\n    actions: [post.create]\n    scope:\n      threads: [RULE_DENY_THREAD_ID]\naccess:\n  boards: []\n  threads:\n    - id: ACCESS_DENY_THREAD_ID\n      projects:\n        probe-project: {{read: deny}}\n~~~\n{tail}"
        )
    }

    fn probe_source(
        tail: &str,
        project_id: &str,
        denied_thread: &str,
        access_denied_thread: &str,
    ) -> String {
        source(tail)
            .replace("probe-project", project_id)
            .replace("RULE_DENY_THREAD_ID", denied_thread)
            .replace("ACCESS_DENY_THREAD_ID", access_denied_thread)
    }

    fn call_service(
        service: &mut crate::Service,
        operation: &str,
        auth: Option<&str>,
        args: serde_json::Value,
        context: serde_json::Value,
    ) -> Result<serde_json::Value, Fault> {
        service.request(crate::protocol::Request {
            operation: operation.into(),
            auth: auth.map(str::to_owned),
            args,
            context,
            request_id: None,
        })
    }

    fn create_policy_probe(
        directory: &Path,
    ) -> (crate::Service, String, String, String, String, String) {
        use serde_json::json;
        let mut service = crate::Service::open(directory).unwrap();
        let admin = call_service(
            &mut service,
            "account.setup",
            None,
            json!({"name":"Admin","password":"rules-crash-fixture-password"}),
            json!({}),
        )
        .unwrap();
        let admin_token = admin["token"].as_str().unwrap();
        let project_root = directory.join("probe-project-root");
        fs::create_dir_all(&project_root).unwrap();
        let project = call_service(
            &mut service,
            "project.register",
            Some(admin_token),
            json!({"name":"Crash probe project","root":project_root}),
            json!({}),
        )
        .unwrap();
        let board = call_service(
            &mut service,
            "board.create",
            Some(admin_token),
            json!({"name":"Crash probe board"}),
            json!({}),
        )
        .unwrap();
        let board_id = board["id"].as_str().unwrap().to_owned();
        let allowed_thread = call_service(
            &mut service,
            "thread.create",
            Some(admin_token),
            json!({"board_id":board_id,"name":"Rule allowed"}),
            json!({}),
        )
        .unwrap();
        let denied_thread = call_service(
            &mut service,
            "thread.create",
            Some(admin_token),
            json!({"board_id":board_id,"name":"Rule denied"}),
            json!({}),
        )
        .unwrap();
        let access_denied_thread = call_service(
            &mut service,
            "thread.create",
            Some(admin_token),
            json!({"board_id":board_id,"name":"Access denied"}),
            json!({}),
        )
        .unwrap();
        (
            service,
            project["id"].as_str().unwrap().to_owned(),
            board_id,
            allowed_thread["id"].as_str().unwrap().to_owned(),
            denied_thread["id"].as_str().unwrap().to_owned(),
            access_denied_thread["id"].as_str().unwrap().to_owned(),
        )
    }

    fn policy_probe(
        service: &mut crate::Service,
        project_id: &str,
        board_id: &str,
        allowed_thread: &str,
        rule_denied_thread: &str,
        access_denied_thread: &str,
    ) -> Result<(bool, bool, bool, bool, bool), Fault> {
        use serde_json::json;
        let identity = call_service(
            service,
            "identity.start",
            None,
            json!({"project_id":project_id,"model_name":"CrashProbe"}),
            json!({}),
        )?;
        let token = identity["session_token"].as_str().unwrap();
        let default_read_denied = call_service(
            service,
            "board.get",
            Some(token),
            json!({"id":board_id}),
            json!({}),
        )
        .is_err();
        let context_allow = call_service(
            service,
            "post.create",
            Some(token),
            json!({"thread_id":allowed_thread,"body":"probe"}),
            json!({"flag":"permit"}),
        )
        .is_ok();
        let context_deny = call_service(
            service,
            "post.create",
            Some(token),
            json!({"thread_id":allowed_thread,"body":"probe"}),
            json!({"flag":"other"}),
        )
        .is_err();
        let explicit_deny = call_service(
            service,
            "post.create",
            Some(token),
            json!({"thread_id":rule_denied_thread,"body":"probe"}),
            json!({"flag":"permit"}),
        )
        .is_err();
        let access_deny = call_service(
            service,
            "post.create",
            Some(token),
            json!({"thread_id":access_denied_thread,"body":"probe"}),
            json!({"flag":"permit"}),
        )
        .is_err();
        Ok((
            default_read_denied,
            context_allow,
            context_deny,
            explicit_deny,
            access_deny,
        ))
    }

    fn crash_at(stage: SaveStage) -> bool {
        let Ok(selected) = std::env::var("SWARM_RULES_CRASH_STAGE") else {
            return false;
        };
        selected
            == match stage {
                SaveStage::TargetLocked => "target_locked",
                SaveStage::BackupSynced => "backup_synced",
                SaveStage::Truncated => "truncated",
                SaveStage::PoisonSynced => "poison_synced",
                SaveStage::SourcePrefixWritten => "source_prefix_written",
                SaveStage::SourceWritten => "source_written",
            }
    }

    #[test]
    #[ignore = "subprocess crash fixture"]
    fn crash_worker() {
        let Ok(raw_path) = std::env::var("SWARM_RULES_CRASH_PATH") else {
            return;
        };
        let path = PathBuf::from(raw_path);
        let tail = std::env::var("SWARM_RULES_CRASH_TAIL").unwrap_or_default();
        let new_source =
            std::env::var("SWARM_RULES_CRASH_SOURCE").unwrap_or_else(|_| source(&tail));
        let old = fs::read(&path).unwrap();
        let expected = revision(Some(&old));
        let _ = save_windows_with_hook(&path, &new_source, &expected, |stage| {
            if crash_at(stage) {
                abort();
            }
        });
        panic!("crash hook stage was not reached");
    }

    #[test]
    fn subprocess_crash_snapshots_are_invalid_or_the_exact_intended_policy() {
        let cases = [
            ("truncated", "", false),
            ("poison_synced", "", false),
            ("source_prefix_written", "", false),
            ("source_written", "", false),
            (
                "source_written",
                "<!-- trailing HTML comment with fake fence\n~~~swarm-rules\nnot a second policy\n",
                true,
            ),
            (
                "source_written",
                "<script>\n// HTML tail with a fake fence\n~~~swarm-rules\nnot a second policy\n",
                true,
            ),
            (
                "source_written",
                "~~~text\n~~~swarm-rules\nignored in generic fence\n",
                true,
            ),
            ("source_written", "ordinary trailing Markdown\n", false),
        ];
        for (stage, tail, should_be_intended) in cases {
            let directory = tempfile::tempdir().unwrap();
            let target = directory.path().join("RULES.md");
            fs::write(&target, original()).unwrap();
            let (
                mut service,
                project_id,
                board_id,
                allowed_thread,
                rule_denied_thread,
                access_denied_thread,
            ) = create_policy_probe(directory.path());
            let new_source = probe_source(
                tail,
                &project_id,
                &rule_denied_thread,
                &access_denied_thread,
            );
            let output = Command::new(std::env::current_exe().unwrap())
                .arg("--exact")
                .arg("rules_editor::windows_tests::crash_worker")
                .arg("--ignored")
                .arg("--nocapture")
                .env("SWARM_RULES_CRASH_PATH", &target)
                .env("SWARM_RULES_CRASH_STAGE", stage)
                .env("SWARM_RULES_CRASH_TAIL", tail)
                .env("SWARM_RULES_CRASH_SOURCE", &new_source)
                .output()
                .unwrap();
            assert!(
                !output.status.success(),
                "child unexpectedly survived: {output:?}"
            );
            let partial = fs::read_to_string(&target).unwrap();
            let parsed = crate::policy::validate_source(&partial, None);
            if should_be_intended {
                assert_eq!(
                    parsed.unwrap(),
                    crate::policy::validate_source(&new_source, None).unwrap(),
                    "the partial file must parse to the exact configured action set"
                );
                let actual = policy_probe(
                    &mut service,
                    &project_id,
                    &board_id,
                    &allowed_thread,
                    &rule_denied_thread,
                    &access_denied_thread,
                )
                .expect("valid crash snapshot must load as a policy");
                assert_eq!(actual, (true, true, true, true, true));
            } else {
                assert!(
                    parsed.is_err(),
                    "crashed intermediate source parsed as valid"
                );
                assert_eq!(
                    policy_probe(
                        &mut service,
                        &project_id,
                        &board_id,
                        &allowed_thread,
                        &rule_denied_thread,
                        &access_denied_thread,
                    )
                    .unwrap_err()
                    .code,
                    "RULES_INVALID"
                );
            }
            let recovery = fs::read_dir(directory.path())
                .unwrap()
                .filter_map(Result::ok)
                .find(|entry| entry.file_name().to_string_lossy().contains(".recovery-"))
                .expect("synced recovery copy must survive process abort");
            assert_eq!(fs::read(recovery.path()).unwrap(), original().as_bytes());
        }
    }

    #[test]
    fn exclusive_handle_blocks_external_write_and_replace_until_policy_commit() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("RULES.md");
        fs::write(&target, original()).unwrap();
        let expected = revision(Some(original().as_bytes()));
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_target = target.clone();
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        let worker = std::thread::spawn(move || {
            save_windows_with_hook(&worker_target, &source(""), &expected, |stage| {
                if stage == SaveStage::TargetLocked {
                    worker_entered.wait();
                    worker_release.wait();
                }
            })
        });

        entered.wait();
        let mut writer = OpenOptions::new();
        writer
            .write(true)
            .share_mode(0x0000_0001 | 0x0000_0002 | 0x0000_0004);
        assert!(
            writer.open(&target).is_err(),
            "exclusive target lock must deny writers"
        );
        let mut reader = OpenOptions::new();
        reader
            .read(true)
            .share_mode(0x0000_0001 | 0x0000_0002 | 0x0000_0004);
        assert!(
            reader.open(&target).is_err(),
            "exclusive target lock must deny readers"
        );

        let replacement = directory.path().join("external.tmp");
        fs::write(&replacement, "external replacement").unwrap();
        let backup = directory.path().join("external.bak");
        let target_wide = wide(&target).unwrap();
        let replacement_wide = wide(&replacement).unwrap();
        let backup_wide = wide(&backup).unwrap();
        #[link(name = "Kernel32")]
        unsafe extern "system" {
            fn ReplaceFileW(
                replaced: *const u16,
                replacement: *const u16,
                backup: *const u16,
                flags: u32,
                exclude: *mut std::ffi::c_void,
                reserved: *mut std::ffi::c_void,
            ) -> i32;
            fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
            fn DeleteFileW(path: *const u16) -> i32;
        }
        let replaced = unsafe {
            ReplaceFileW(
                target_wide.as_ptr(),
                replacement_wide.as_ptr(),
                backup_wide.as_ptr(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(
            replaced, 0,
            "replacement must fail while share-0 handle is held"
        );
        assert_eq!(
            unsafe { MoveFileExW(replacement_wide.as_ptr(), target_wide.as_ptr(), 0x1) },
            0,
            "MoveFileEx replacement must fail while share-0 handle is held"
        );
        assert_eq!(
            unsafe { DeleteFileW(target_wide.as_ptr()) },
            0,
            "delete must fail while share-0 handle is held"
        );
        release.wait();
        let new_revision = worker.join().unwrap().unwrap();
        assert_eq!(new_revision, revision(Some(source("").as_bytes())));
        assert_eq!(fs::read_to_string(&target).unwrap(), source(""));
    }

    #[test]
    fn create_new_handle_blocks_competing_creation_until_publication() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("RULES.md");
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let worker_target = target.clone();
        let worker_entered = entered.clone();
        let worker_release = release.clone();
        let worker = std::thread::spawn(move || {
            save_windows_with_hook(&worker_target, &source(""), MISSING_REVISION, |stage| {
                if stage == SaveStage::TargetLocked {
                    worker_entered.wait();
                    worker_release.wait();
                }
            })
        });
        entered.wait();
        assert!(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
                .is_err(),
            "another creator must not replace or reopen the CREATE_NEW target"
        );
        let alternate = directory.path().join("external.tmp");
        fs::write(&alternate, "outside creator").unwrap();
        let alternate_wide = wide(&alternate).unwrap();
        let target_wide = wide(&target).unwrap();
        #[link(name = "Kernel32")]
        unsafe extern "system" {
            fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
            fn DeleteFileW(path: *const u16) -> i32;
        }
        assert_eq!(
            unsafe { MoveFileExW(alternate_wide.as_ptr(), target_wide.as_ptr(), 0x1) },
            0,
            "external replacement must fail while CREATE_NEW handle is held"
        );
        assert_eq!(
            unsafe { DeleteFileW(target_wide.as_ptr()) },
            0,
            "external delete must fail while CREATE_NEW handle is held"
        );
        release.wait();
        assert_eq!(
            worker.join().unwrap().unwrap(),
            revision(Some(source("").as_bytes()))
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), source(""));
        assert!(
            fs::read_dir(directory.path())
                .unwrap()
                .filter_map(Result::ok)
                .all(|entry| !entry.file_name().to_string_lossy().contains(".recovery-"))
        );
    }
}
