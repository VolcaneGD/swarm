use crate::PlatformError;
use sha2::{Digest, Sha256};
use std::ffi::c_void;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_FILE_EXISTS, GENERIC_READ, GENERIC_WRITE, HANDLE,
    HLOCAL, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Authorization::{
    GetNamedSecurityInfoW, GetSecurityInfo, SE_FILE_OBJECT, SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use windows_sys::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_REVISION, ACL_SIZE_INFORMATION, AclSizeInformation,
    AddAccessAllowedAce, DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetAclInformation,
    GetLengthSid, GetSecurityDescriptorControl, GetSecurityDescriptorDacl,
    GetSecurityDescriptorOwner, GetTokenInformation, InitializeAcl, InitializeSecurityDescriptor,
    IsValidAcl, IsValidSid, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, SetSecurityDescriptorControl,
    SetSecurityDescriptorDacl, SetSecurityDescriptorOwner, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CREATE_NEW, CreateFileW, FILE_ALL_ACCESS, FILE_ATTRIBUTE_NORMAL,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, GetFileInformationByHandle,
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW, OPEN_EXISTING,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

const MAX_PLAINTEXT_BYTES: usize = 64 * 1024;
const MAX_CIPHERTEXT_BYTES: u64 = 1024 * 1024;
const ENTROPY_NAMESPACE: &[u8] = b"Swarm.CredentialStore.v1\0";

/// Immutable backup files protected by the current user's private Windows ACL.
/// Unlike CredentialStore this does not encrypt or impose credential-size limits.
pub struct BackupStore {
    private: CredentialStore,
}

impl BackupStore {
    /// Enumerate only validated, regular backup files inside this private store.
    pub fn list(&self) -> Result<Vec<(String, u64)>, PlatformError> {
        self.private.check_root()?;
        let mut result = Vec::new();
        for entry in fs::read_dir(&self.private.root).map_err(|_| PlatformError::Io)? {
            let path = entry.map_err(|_| PlatformError::Io)?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("backup") {
                continue;
            }
            let key = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or(PlatformError::InvalidData)?;
            validate_key(key)?;
            let file = self.read(key)?;
            let bytes = file.metadata().map_err(|_| PlatformError::Io)?.len();
            result.push((key.to_owned(), bytes));
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(result)
    }

    /// Deletes only an independently validated file by its open handle.
    pub fn remove(&self, key: &str) -> Result<(), PlatformError> {
        use windows_sys::Win32::Storage::FileSystem::{
            DELETE, FILE_DISPOSITION_INFO, FILE_READ_ATTRIBUTES, FileDispositionInfo,
            SetFileInformationByHandle,
        };
        validate_key(key)?;
        self.private.check_root()?;
        let path = self.private.root.join(format!("{key}.backup"));
        self.private.check_entry(&path)?;
        let file = match OpenOptions::new()
            .access_mode(DELETE | FILE_READ_ATTRIBUTES)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(0)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(PlatformError::Io),
        };
        check_file_entry(&file)?;
        self.private.check_root()?;
        let info = FILE_DISPOSITION_INFO { DeleteFile: true };
        if unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle().cast(),
                FileDispositionInfo,
                (&info as *const FILE_DISPOSITION_INFO).cast(),
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        } == 0
        {
            return Err(PlatformError::Io);
        }
        Ok(())
    }

    pub fn contains(&self, key: &str) -> Result<bool, PlatformError> {
        validate_key(key)?;
        self.private.check_root()?;
        let path = self.private.root.join(format!("{key}.backup"));
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                self.private.check_entry(&path)?;
                Ok(true)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(PlatformError::Io),
        }
    }
    pub fn open(root: &Path) -> Result<Self, PlatformError> {
        use std::path::{Component, Prefix};
        use windows_sys::Win32::System::WindowsProgramming::{
            DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOVABLE,
        };
        let drive = match root.components().next() {
            Some(Component::Prefix(prefix)) if root.is_absolute() => match prefix.kind() {
                Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
                _ => return Err(PlatformError::InvalidRoot),
            },
            _ => return Err(PlatformError::InvalidRoot),
        };
        let drive_root = std::path::PathBuf::from(format!("{}:\\", drive as char));
        let kind = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(wide(&drive_root).as_ptr())
        };
        if !matches!(kind, DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_RAMDISK) {
            return Err(PlatformError::InvalidRoot);
        }
        Ok(Self {
            private: CredentialStore::open(root)?,
        })
    }

    pub fn write_new(
        &self,
        key: &str,
        write: impl FnOnce(&mut std::fs::File) -> Result<(), PlatformError>,
    ) -> Result<(), PlatformError> {
        validate_key(key)?;
        self.private.check_root()?;
        // Keep the generation directory from being renamed/replaced while a
        // temporary file is written, published or cleaned up.
        let _directory = OpenOptions::new()
            .read(true)
            .custom_flags(
                FILE_FLAG_OPEN_REPARSE_POINT
                    | windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS,
            )
            .share_mode(
                windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ
                    | windows_sys::Win32::Storage::FileSystem::FILE_SHARE_WRITE,
            )
            .open(&self.private.root)
            .map_err(|_| PlatformError::InvalidRoot)?;
        self.private.check_root()?;
        let path = self.private.root.join(format!("{key}.backup"));
        self.private.check_entry(&path)?;
        if fs::symlink_metadata(&path).is_ok() {
            return Err(PlatformError::InvalidData);
        }
        let temp = self
            .private
            .root
            .join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        let mut cleanup = TempPath(Some(temp.clone()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(0)
            .open(&temp)
            .map_err(|_| PlatformError::Io)?;
        let user = current_user_sid()?;
        set_dacl(&temp, user.sid)?;
        validate_private_acl(&temp, user.sid)?;
        check_file_entry(&file)?;
        write(&mut file)?;
        file.flush().map_err(|_| PlatformError::Io)?;
        file.sync_all().map_err(|_| PlatformError::Io)?;
        drop(file);
        self.private.check_root()?;
        self.private.check_entry(&temp)?;
        self.private.check_entry(&path)?;
        // No replacement flag: concurrent writers to this immutable generation
        // cannot overwrite one another, even after the preflight exists check.
        if unsafe {
            MoveFileExW(
                wide(&temp).as_ptr(),
                wide(&path).as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(PlatformError::Io);
        }
        cleanup.0 = None;
        Ok(())
    }

    pub fn read(&self, key: &str) -> Result<std::fs::File, PlatformError> {
        validate_key(key)?;
        self.private.check_root()?;
        let path = self.private.root.join(format!("{key}.backup"));
        self.private.check_entry(&path)?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
            .open(&path)
            .map_err(|_| PlatformError::Io)?;
        check_file_entry(&file)?;
        self.private.check_root()?;
        Ok(file)
    }
}

pub struct CredentialStore {
    root: PathBuf,
}

impl fmt::Debug for CredentialStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CredentialStore { root: [redacted] }")
    }
}

impl CredentialStore {
    /// Restore owner only: invalidate encrypted entries, preserving held leases.
    /// Delete validated entries by handle; never recursively remove a store.
    pub fn clear(&self) -> Result<(), PlatformError> {
        use windows_sys::Win32::Storage::FileSystem::{
            DELETE, FILE_DISPOSITION_INFO, FILE_READ_ATTRIBUTES, FileDispositionInfo,
            SetFileInformationByHandle,
        };
        self.check_root()?;
        for entry in fs::read_dir(&self.root).map_err(|_| PlatformError::Io)? {
            let path = entry.map_err(|_| PlatformError::Io)?.path();
            if path.extension().and_then(|s| s.to_str()) != Some("bin") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or(PlatformError::InvalidData)?;
            if stem.len() != 64 || !stem.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(PlatformError::InvalidData);
            }
            self.check_entry(&path)?;
            let file = OpenOptions::new()
                .access_mode(DELETE | FILE_READ_ATTRIBUTES)
                .share_mode(0)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&path)
                .map_err(|_| PlatformError::Io)?;
            check_file_entry(&file)?;
            self.check_root()?;
            let info = FILE_DISPOSITION_INFO { DeleteFile: true };
            if unsafe {
                SetFileInformationByHandle(
                    file.as_raw_handle().cast(),
                    FileDispositionInfo,
                    (&info as *const FILE_DISPOSITION_INFO).cast(),
                    size_of::<FILE_DISPOSITION_INFO>() as u32,
                )
            } == 0
            {
                return Err(PlatformError::Io);
            }
        }
        Ok(())
    }
    pub fn open(root: &Path) -> Result<Self, PlatformError> {
        if root.as_os_str().is_empty() {
            return Err(PlatformError::InvalidRoot);
        }
        ensure_no_reparse_components(root)?;

        let (root, created) = match fs::symlink_metadata(root) {
            Ok(meta) => {
                if meta.file_type().is_symlink() || !meta.is_dir() {
                    return Err(PlatformError::InvalidRoot);
                }
                (
                    fs::canonicalize(root).map_err(|_| PlatformError::Io)?,
                    false,
                )
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let parent = root
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .ok_or(PlatformError::InvalidRoot)?;
                let canonical_parent =
                    fs::canonicalize(parent).map_err(|_| PlatformError::InvalidRoot)?;
                ensure_no_reparse_components(&canonical_parent)?;
                let name = root.file_name().ok_or(PlatformError::InvalidRoot)?;
                let created = canonical_parent.join(name);
                fs::create_dir(&created).map_err(|_| PlatformError::Io)?;
                (created, true)
            }
            Err(_) => return Err(PlatformError::Io),
        };

        if ensure_no_reparse_components(&root).is_err() {
            return Err(PlatformError::InvalidRoot);
        }
        let user = current_user_sid()?;
        if created {
            set_dacl(&root, user.sid)?;
        }
        validate_private_acl(&root, user.sid).map_err(|_| PlatformError::InvalidRoot)?;
        Ok(Self { root })
    }

    pub fn put(&self, key: &str, bytes: &[u8]) -> Result<(), PlatformError> {
        let path = self.key_path(key)?;
        if bytes.len() > MAX_PLAINTEXT_BYTES {
            return Err(PlatformError::TooLarge);
        }
        self.check_root()?;
        self.check_entry(&path)?;

        let mut entropy = make_entropy(key);
        let encrypted = protect(bytes, &entropy);
        entropy.fill(0);
        let encrypted = encrypted?;
        if encrypted.len() as u64 > MAX_CIPHERTEXT_BYTES {
            return Err(PlatformError::TooLarge);
        }

        let temp = self
            .root
            .join(format!(".{}.{}.tmp", file_stem(key), uuid::Uuid::new_v4()));
        let mut cleanup = TempPath(Some(temp.clone()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&temp)
            .map_err(|_| PlatformError::Io)?;
        let user = current_user_sid()?;
        set_dacl(&temp, user.sid)?;
        validate_private_acl(&temp, user.sid)?;
        file.write_all(&encrypted).map_err(|_| PlatformError::Io)?;
        file.flush().map_err(|_| PlatformError::Io)?;
        file.sync_all().map_err(|_| PlatformError::Io)?;
        drop(file);

        // Check the destination again immediately before the atomic replacement.
        self.check_root()?;
        self.check_entry(&path)?;
        let from = wide(&temp);
        let to = wide(&path);
        // Same-directory MoveFileEx replacement is atomic on the target volume.
        let ok = unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if ok == 0 {
            return Err(PlatformError::Io);
        }
        cleanup.0 = None;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, PlatformError> {
        let path = self.key_path(key)?;
        self.check_root()?;
        self.check_entry(&path)?;
        let file = match OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)
        {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(PlatformError::Io),
        };
        check_file_entry(&file)?;
        let metadata = file.metadata().map_err(|_| PlatformError::Io)?;
        if !metadata.is_file() || metadata.len() > MAX_CIPHERTEXT_BYTES {
            return Err(PlatformError::InvalidData);
        }
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(PlatformError::InvalidData);
        }
        let cap = MAX_CIPHERTEXT_BYTES as usize + 1;
        let mut encrypted = Vec::with_capacity((metadata.len() as usize).min(cap));
        file.take(cap as u64)
            .read_to_end(&mut encrypted)
            .map_err(|_| PlatformError::Io)?;
        if encrypted.len() > MAX_CIPHERTEXT_BYTES as usize {
            return Err(PlatformError::InvalidData);
        }

        let mut entropy = make_entropy(key);
        let result = unprotect(&encrypted, &entropy);
        entropy.fill(0);
        result.map(Some)
    }

    pub fn remove(&self, key: &str) -> Result<(), PlatformError> {
        let path = self.key_path(key)?;
        self.check_root()?;
        self.check_entry(&path)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(PlatformError::Io),
        }
    }

    /// Acquires an exclusive cross-process lock associated with this private
    /// store. The returned file handle releases the lock when dropped.
    pub fn lock(&self, key: &str) -> Result<std::fs::File, PlatformError> {
        validate_key(key)?;
        let path = self.root.join(format!("{}.lock", file_stem(key)));
        self.check_root()?;
        self.check_entry(&path)?;

        let (file, created) = match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)
        {
            Ok(file) => (file, true),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                    .open(&path)
                    .map_err(|_| PlatformError::Io)?,
                false,
            ),
            Err(_) => return Err(PlatformError::Io),
        };
        check_file_entry(&file)?;
        let user = current_user_sid()?;
        if created {
            set_dacl(&path, user.sid)?;
        }
        validate_private_acl(&path, user.sid).map_err(|_| PlatformError::InvalidData)?;
        self.check_root()?;
        file.lock().map_err(|_| PlatformError::Io)?;
        Ok(file)
    }

    /// Opens a named coordination lease in this private store. The name is used
    /// verbatim so older processes continue to exclude one another on the same
    /// path. Windows share mode zero holds the lease until the returned handle
    /// is dropped; the file is intentionally retained afterward.
    pub fn private_lease(&self, name: &str) -> std::io::Result<std::fs::File> {
        use std::os::windows::io::FromRawHandle;

        if name.is_empty()
            || name == "."
            || name == ".."
            || name.contains("..")
            || name.ends_with('.')
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid private lease name",
            ));
        }
        let device = name.split('.').next().unwrap_or("").to_ascii_uppercase();
        if matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (device.len() == 4
                && (device.starts_with("COM") || device.starts_with("LPT"))
                && matches!(device.as_bytes()[3], b'1'..=b'9'))
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "reserved private lease name",
            ));
        }

        self.check_root().map_err(platform_error_to_io)?;
        let path = self.root.join(name);
        let user = current_user_sid().map_err(platform_error_to_io)?;

        // Build an absolute descriptor while its ACL and SID storage remain
        // alive through CreateFileW. Supplying it at CREATE_NEW avoids the
        // elevated-creator owner and inherited-DACL window of post-open repair.
        let mut acl_storage = vec![0usize; 128];
        let acl_ptr = acl_storage.as_mut_ptr().cast::<ACL>();
        let acl_bytes = (acl_storage.len() * size_of::<usize>()) as u32;
        if unsafe { InitializeAcl(acl_ptr, acl_bytes, ACL_REVISION) } == 0
            || unsafe { AddAccessAllowedAce(acl_ptr, ACL_REVISION, FILE_ALL_ACCESS, user.sid) } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let mut descriptor = [0usize; 8];
        let descriptor_ptr = descriptor.as_mut_ptr().cast();
        if unsafe { InitializeSecurityDescriptor(descriptor_ptr, 1) } == 0
            || unsafe { SetSecurityDescriptorOwner(descriptor_ptr, user.sid, 0) } == 0
            || unsafe { SetSecurityDescriptorDacl(descriptor_ptr, 1, acl_ptr, 0) } == 0
            || unsafe {
                SetSecurityDescriptorControl(descriptor_ptr, SE_DACL_PROTECTED, SE_DACL_PROTECTED)
            } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor_ptr,
            bInheritHandle: 0,
        };

        let flags = FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT;
        let create = unsafe {
            CreateFileW(
                wide(&path).as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                &attributes,
                CREATE_NEW,
                flags,
                std::ptr::null_mut(),
            )
        };
        let handle = if create != INVALID_HANDLE_VALUE {
            create
        } else {
            let error = std::io::Error::last_os_error();
            if !matches!(error.raw_os_error(), Some(code) if code == ERROR_FILE_EXISTS as i32 || code == ERROR_ALREADY_EXISTS as i32)
            {
                return Err(error);
            }
            let existing = unsafe {
                CreateFileW(
                    wide(&path).as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    0,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    flags,
                    std::ptr::null_mut(),
                )
            };
            if existing == INVALID_HANDLE_VALUE {
                return Err(std::io::Error::last_os_error());
            }
            existing
        };

        // SAFETY: CreateFileW returned a valid owned handle and this is the
        // unique conversion into Rust's File owner.
        let file = unsafe { std::fs::File::from_raw_handle(handle.cast()) };
        check_file_entry(&file).map_err(platform_error_to_io)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() != 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "private lease must be an empty regular file",
            ));
        }
        validate_private_acl_handle(&file, user.sid).map_err(platform_error_to_io)?;
        self.check_root().map_err(platform_error_to_io)?;
        Ok(file)
    }

    fn key_path(&self, key: &str) -> Result<PathBuf, PlatformError> {
        validate_key(key)?;
        Ok(self.root.join(format!("{}.bin", file_stem(key))))
    }

    fn check_root(&self) -> Result<(), PlatformError> {
        ensure_no_reparse_components(&self.root).map_err(|_| PlatformError::InvalidRoot)?;
        let meta = fs::symlink_metadata(&self.root).map_err(|_| PlatformError::InvalidRoot)?;
        if !meta.is_dir() || meta.file_type().is_symlink() {
            return Err(PlatformError::InvalidRoot);
        }
        let user = current_user_sid()?;
        validate_private_acl(&self.root, user.sid).map_err(|_| PlatformError::InvalidRoot)?;
        Ok(())
    }

    fn check_entry(&self, path: &Path) -> Result<(), PlatformError> {
        match fs::symlink_metadata(path) {
            Ok(meta) => {
                if meta.file_type().is_symlink() || !meta.is_file() {
                    return Err(PlatformError::InvalidData);
                }
                // Hard links can alias files outside this root, so refuse them
                // before changing permissions or reading/deleting their data.
                let file = OpenOptions::new()
                    .read(true)
                    .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                    .open(path)
                    .map_err(|_| PlatformError::Io)?;
                check_file_entry(&file)?;
                let user = current_user_sid()?;
                validate_private_acl(path, user.sid)?;
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(PlatformError::Io),
        }
    }
}

fn platform_error_to_io(error: PlatformError) -> std::io::Error {
    let kind = match error {
        PlatformError::InvalidKey | PlatformError::InvalidEndpoint => {
            std::io::ErrorKind::InvalidInput
        }
        PlatformError::InvalidRoot | PlatformError::Protection => {
            std::io::ErrorKind::PermissionDenied
        }
        PlatformError::InvalidData => std::io::ErrorKind::InvalidData,
        PlatformError::Unsupported => std::io::ErrorKind::Unsupported,
        PlatformError::TooLarge | PlatformError::Io => std::io::ErrorKind::Other,
    };
    std::io::Error::new(kind, "private lease operation failed")
}

struct TempPath(Option<PathBuf>);
impl Drop for TempPath {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_file(path);
        }
    }
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != -1isize as HANDLE {
            unsafe { CloseHandle(self.0) };
        }
    }
}

struct LocalBuffer(*mut c_void);
impl Drop for LocalBuffer {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { LocalFree(self.0 as HLOCAL) };
        }
    }
}

fn validate_key(key: &str) -> Result<(), PlatformError> {
    let b = key.as_bytes();
    if !(1..=128).contains(&b.len())
        || !b
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'-')
    {
        return Err(PlatformError::InvalidKey);
    }
    Ok(())
}

fn file_stem(key: &str) -> String {
    let hash = Sha256::digest(key.as_bytes());
    let mut out = String::with_capacity(64);
    for b in hash {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

fn make_entropy(key: &str) -> Vec<u8> {
    let mut entropy = Vec::with_capacity(ENTROPY_NAMESPACE.len() + key.len());
    entropy.extend_from_slice(ENTROPY_NAMESPACE);
    entropy.extend_from_slice(key.as_bytes());
    entropy
}

fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

fn check_file_entry(file: &std::fs::File) -> Result<(), PlatformError> {
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut info) };
    if ok == 0
        || info.nNumberOfLinks != 1
        || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(PlatformError::InvalidData);
    }
    Ok(())
}

fn ensure_no_reparse_components(path: &Path) -> Result<(), PlatformError> {
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(PlatformError::InvalidRoot);
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|_| PlatformError::InvalidRoot)?
            .join(path)
    };
    let mut cursor = absolute.as_path();
    if fs::symlink_metadata(cursor).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
    {
        cursor = cursor.parent().ok_or(PlatformError::InvalidRoot)?;
    }
    loop {
        let meta = fs::symlink_metadata(cursor).map_err(|_| PlatformError::InvalidRoot)?;
        if meta.file_type().is_symlink() {
            return Err(PlatformError::InvalidRoot);
        }
        let attrs = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetFileAttributesW(wide(cursor).as_ptr())
        };
        if attrs == u32::MAX || attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(PlatformError::InvalidRoot);
        }
        match cursor.parent() {
            Some(parent) if parent != cursor => cursor = parent,
            _ => break,
        }
    }
    Ok(())
}

fn protect(input: &[u8], entropy: &[u8]) -> Result<Vec<u8>, PlatformError> {
    let input_blob = blob(input);
    let entropy_blob = blob(entropy);
    let mut output: CRYPT_INTEGER_BLOB = unsafe { zeroed() };
    let ok = unsafe {
        CryptProtectData(
            &input_blob,
            std::ptr::null(),
            &entropy_blob,
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(PlatformError::Protection);
    }
    let output_guard = LocalBuffer(output.pbData.cast());
    let result = if output.cbData == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec()
    };
    drop(output_guard);
    Ok(result)
}

fn unprotect(input: &[u8], entropy: &[u8]) -> Result<Vec<u8>, PlatformError> {
    let input_blob = blob(input);
    let entropy_blob = blob(entropy);
    let mut output: CRYPT_INTEGER_BLOB = unsafe { zeroed() };
    let ok = unsafe {
        CryptUnprotectData(
            &input_blob,
            std::ptr::null_mut(),
            &entropy_blob,
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(PlatformError::InvalidData);
    }
    let output_guard = LocalBuffer(output.pbData.cast());
    if output.cbData as usize > MAX_PLAINTEXT_BYTES {
        if !output.pbData.is_null() {
            secure_zero(output.pbData, output.cbData as usize);
        }
        drop(output_guard);
        return Err(PlatformError::InvalidData);
    }
    let result = if output.cbData == 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec()
    };
    if !output.pbData.is_null() {
        secure_zero(output.pbData, output.cbData as usize);
    }
    drop(output_guard);
    Ok(result)
}

fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: bytes.len().try_into().unwrap_or(u32::MAX),
        pbData: bytes.as_ptr() as *mut u8,
    }
}

fn set_dacl(path: &Path, user_sid: *mut c_void) -> Result<(), PlatformError> {
    let mut acl_storage = vec![0usize; 128];
    let acl_ptr = acl_storage.as_mut_ptr().cast::<ACL>();
    let acl_bytes = (acl_storage.len() * size_of::<usize>()) as u32;
    let init = unsafe { InitializeAcl(acl_ptr, acl_bytes, ACL_REVISION) };
    if init == 0 {
        return Err(PlatformError::Io);
    }
    let added = unsafe { AddAccessAllowedAce(acl_ptr, ACL_REVISION, FILE_ALL_ACCESS, user_sid) };
    if added == 0 {
        return Err(PlatformError::Io);
    }
    let wide_path = wide(path);
    let status = unsafe {
        SetNamedSecurityInfoW(
            wide_path.as_ptr() as *mut u16,
            SE_FILE_OBJECT,
            // A new object may inherit Modify without WRITE_OWNER. Its creator
            // can set its DACL first; the private full-access ACE then permits
            // assigning the actual user as owner without enabling privileges.
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            acl_ptr,
            std::ptr::null_mut(),
        )
    };
    if status != 0 {
        return Err(PlatformError::Io);
    }
    // Only newly created objects reach this helper. Elevated tokens may default
    // ownership to Administrators, so explicit current-user ownership is still
    // required after granting the private DACL. Existing entries are not repaired.
    let status = unsafe {
        SetNamedSecurityInfoW(
            wide_path.as_ptr() as *mut u16,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            user_sid,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if status != 0 {
        return Err(PlatformError::Io);
    }
    Ok(())
}

fn validate_private_acl(path: &Path, user_sid: *mut c_void) -> Result<(), PlatformError> {
    let wide_path = wide(path);
    let mut owner = std::ptr::null_mut();
    let mut dacl = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide_path.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 || descriptor.is_null() {
        return Err(PlatformError::InvalidRoot);
    }
    let descriptor_guard = LocalBuffer(descriptor);

    let result = validate_private_acl_descriptor(descriptor, user_sid);
    drop(descriptor_guard);
    result
}

fn validate_private_acl_handle(
    file: &std::fs::File,
    user_sid: *mut c_void,
) -> Result<(), PlatformError> {
    let mut descriptor = std::ptr::null_mut();
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 || descriptor.is_null() {
        return Err(PlatformError::InvalidRoot);
    }
    let descriptor_guard = LocalBuffer(descriptor);
    let result = validate_private_acl_descriptor(descriptor, user_sid);
    drop(descriptor_guard);
    result
}

fn validate_private_acl_descriptor(
    descriptor: *mut c_void,
    user_sid: *mut c_void,
) -> Result<(), PlatformError> {
    let mut dacl = std::ptr::null_mut();
    let mut owner_from_descriptor = std::ptr::null_mut();
    let mut owner_defaulted = 0;
    let owner_ok = unsafe {
        GetSecurityDescriptorOwner(descriptor, &mut owner_from_descriptor, &mut owner_defaulted)
    };
    if owner_ok == 0
        || owner_from_descriptor.is_null()
        || unsafe { EqualSid(owner_from_descriptor, user_sid) } == 0
    {
        return Err(PlatformError::InvalidRoot);
    }

    let mut control = 0u16;
    let mut revision = 0u32;
    if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0
        || control & SE_DACL_PROTECTED == 0
    {
        return Err(PlatformError::InvalidRoot);
    }

    let mut dacl_present = 0;
    let mut dacl_defaulted = 0;
    if unsafe {
        GetSecurityDescriptorDacl(
            descriptor,
            &mut dacl_present,
            &mut dacl,
            &mut dacl_defaulted,
        )
    } == 0
        || dacl_present == 0
        || dacl_defaulted != 0
        || dacl.is_null()
    {
        return Err(PlatformError::InvalidRoot);
    }

    let mut acl_info: ACL_SIZE_INFORMATION = unsafe { zeroed() };
    if unsafe { IsValidAcl(dacl) } == 0
        || unsafe {
            GetAclInformation(
                dacl,
                (&mut acl_info as *mut ACL_SIZE_INFORMATION).cast(),
                size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
        } == 0
        || acl_info.AceCount != 1
    {
        return Err(PlatformError::InvalidRoot);
    }

    let mut ace_ptr = std::ptr::null_mut();
    if unsafe { GetAce(dacl, 0, &mut ace_ptr) } == 0 || ace_ptr.is_null() {
        return Err(PlatformError::InvalidRoot);
    }
    let header = unsafe { &*ace_ptr.cast::<ACE_HEADER>() };
    if (header.AceSize as usize) < size_of::<ACCESS_ALLOWED_ACE>() + size_of::<u32>() {
        return Err(PlatformError::InvalidRoot);
    }
    let ace = unsafe { &*ace_ptr.cast::<ACCESS_ALLOWED_ACE>() };
    let ace_sid = std::ptr::addr_of!(ace.SidStart) as *mut c_void;
    let sid_size = unsafe { GetLengthSid(ace_sid) } as usize;
    let valid = ace.Header.AceType == 0
        && ace.Header.AceFlags == 0
        && unsafe { IsValidSid(ace_sid) } != 0
        && size_of::<ACE_HEADER>() + size_of::<u32>() + sid_size <= ace.Header.AceSize as usize
        && ace.Mask == FILE_ALL_ACCESS
        && unsafe { EqualSid(ace_sid, user_sid) } != 0;
    if valid {
        Ok(())
    } else {
        Err(PlatformError::InvalidRoot)
    }
}

struct TokenUserBuffer {
    _storage: Vec<usize>,
    sid: *mut c_void,
}

fn current_user_sid() -> Result<TokenUserBuffer, PlatformError> {
    let mut raw: HANDLE = std::ptr::null_mut();
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) };
    if opened == 0 {
        return Err(PlatformError::Io);
    }
    let _handle = Handle(raw);
    let mut needed = 0u32;
    unsafe { GetTokenInformation(raw, TokenUser, std::ptr::null_mut(), 0, &mut needed) };
    if needed == 0 {
        return Err(PlatformError::Io);
    }
    let words = (needed as usize).div_ceil(size_of::<usize>());
    let mut storage = vec![0usize; words];
    let ok = unsafe {
        GetTokenInformation(
            raw,
            TokenUser,
            storage.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    if ok == 0 {
        return Err(PlatformError::Io);
    }
    let token_user = unsafe { &*storage.as_ptr().cast::<TOKEN_USER>() };
    Ok(TokenUserBuffer {
        _storage: storage,
        sid: token_user.User.Sid,
    })
}

fn secure_zero(ptr: *mut u8, len: usize) {
    // Use volatile writes so optimization cannot elide the wipe before LocalFree.
    for offset in 0..len {
        unsafe { std::ptr::write_volatile(ptr.add(offset), 0) };
    }
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
}
