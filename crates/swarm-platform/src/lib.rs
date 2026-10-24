use std::fmt;
#[cfg(not(windows))]
use std::path::Path;

#[cfg(windows)]
mod windows;

pub mod display;
pub mod lifecycle;
pub mod notifications;
pub mod pipe;
pub mod process;
pub mod startup;

/// A deliberately non-diagnostic error for credential operations.
///
/// Errors never include a credential key, filesystem path, or credential data.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PlatformError {
    Unsupported,
    InvalidKey,
    TooLarge,
    InvalidRoot,
    Io,
    Protection,
    InvalidData,
    InvalidEndpoint,
}

impl fmt::Debug for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unsupported => "PlatformError(Unsupported)",
            Self::InvalidKey => "PlatformError(InvalidKey)",
            Self::TooLarge => "PlatformError(TooLarge)",
            Self::InvalidRoot => "PlatformError(InvalidRoot)",
            Self::Io => "PlatformError(Io)",
            Self::Protection => "PlatformError(Protection)",
            Self::InvalidData => "PlatformError(InvalidData)",
            Self::InvalidEndpoint => "PlatformError(InvalidEndpoint)",
        })
    }
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unsupported => "credential storage is unsupported on this platform",
            Self::InvalidKey => "invalid credential key",
            Self::TooLarge => "credential data exceeds the supported size",
            Self::InvalidRoot => "invalid credential storage root",
            Self::Io => "credential storage operation failed",
            Self::Protection => "credential protection operation failed",
            Self::InvalidData => "stored credential data is invalid",
            Self::InvalidEndpoint => "invalid named pipe endpoint",
        })
    }
}

impl std::error::Error for PlatformError {}

#[cfg(windows)]
pub use windows::{BackupStore, CredentialStore};

#[cfg(not(windows))]
pub struct CredentialStore;

#[cfg(not(windows))]
pub struct BackupStore;

#[cfg(not(windows))]
impl BackupStore {
    pub fn list(&self) -> Result<Vec<(String, u64)>, PlatformError> {
        Err(PlatformError::Unsupported)
    }
    pub fn remove(&self, _key: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported)
    }
    pub fn contains(&self, _key: &str) -> Result<bool, PlatformError> {
        Err(PlatformError::Unsupported)
    }
    pub fn open(_root: &Path) -> Result<Self, PlatformError> {
        Err(PlatformError::Unsupported)
    }
    pub fn write_new(
        &self,
        _key: &str,
        _write: impl FnOnce(&mut std::fs::File) -> Result<(), PlatformError>,
    ) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported)
    }
    pub fn read(&self, _key: &str) -> Result<std::fs::File, PlatformError> {
        Err(PlatformError::Unsupported)
    }
}

#[cfg(not(windows))]
impl CredentialStore {
    pub fn clear(&self) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported)
    }
    pub fn open(_root: &Path) -> Result<Self, PlatformError> {
        Err(PlatformError::Unsupported)
    }

    pub fn put(&self, _key: &str, _bytes: &[u8]) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported)
    }

    pub fn get(&self, _key: &str) -> Result<Option<Vec<u8>>, PlatformError> {
        Err(PlatformError::Unsupported)
    }

    pub fn remove(&self, _key: &str) -> Result<(), PlatformError> {
        Err(PlatformError::Unsupported)
    }

    pub fn lock(&self, _key: &str) -> Result<std::fs::File, PlatformError> {
        Err(PlatformError::Unsupported)
    }

    pub fn private_lease(&self, _name: &str) -> std::io::Result<std::fs::File> {
        Err(std::io::ErrorKind::Unsupported.into())
    }
}
