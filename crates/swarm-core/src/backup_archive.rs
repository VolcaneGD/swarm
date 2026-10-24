use crate::{BackupRulesSnapshot, PreparedBackup, protocol::Fault};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

const MAGIC: &[u8; 13] = b"SWARMBACKUP1\n";
const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
const MAX_DATABASE_BYTES: u64 = 1024 * 1024 * 1024;

fn invalid() -> Fault {
    Fault::new(
        "BACKUP_INVALID",
        8,
        "Backup format or checksum could not be verified",
    )
}

fn valid_manifest(manifest: &BackupManifest) -> bool {
    if manifest.format_version != 1
        || !matches!(manifest.schema_version, 7..=9)
        || !(100..=MAX_DATABASE_BYTES).contains(&manifest.database_bytes)
        || manifest.database_sha256.len() != 64
        || !manifest
            .database_sha256
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
        || chrono::DateTime::parse_from_rfc3339(&manifest.created_at).is_err()
        || manifest.rules.is_empty()
        || manifest.rules[0].project_id.is_some()
    {
        return false;
    }
    let mut ids = std::collections::HashSet::new();
    for (index, rules) in manifest.rules.iter().enumerate() {
        if rules.source_path.is_empty()
            || rules.source_path.len() > 32768
            || rules.source_path.chars().any(char::is_control)
        {
            return false;
        }
        match &rules.project_id {
            None if index != 0 || rules.content.is_none() => return false,
            Some(id) if id.is_empty() || id.len() > 256 || !ids.insert(id) => return false,
            _ => {}
        }
        match &rules.content {
            None if rules.revision != "missing:v1" => return false,
            Some(content)
                if content.len() > 1024 * 1024
                    || rules.revision
                        != crate::rules_editor::revision(Some(content.as_bytes())) =>
            {
                return false;
            }
            _ => {}
        }
    }
    true
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupKind {
    Manual,
    Scheduled,
    Safety,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupManifest {
    pub format_version: u32,
    pub schema_version: u32,
    pub created_at: String,
    pub kind: BackupKind,
    pub database_bytes: u64,
    pub database_sha256: String,
    pub rules: Vec<BackupRulesSnapshot>,
}

impl BackupManifest {
    /// Binds the captured database and RULES, independently of publication time.
    pub fn content_revision(&self) -> Result<String, Fault> {
        content_revision(&self.database_sha256, &self.rules)
    }
}

pub(crate) fn content_revision(hash: &str, rules: &[BackupRulesSnapshot]) -> Result<String, Fault> {
    let mut digest = Sha256::new();
    digest.update(b"Swarm.BackupContent.v1\0");
    digest.update(hash.as_bytes());
    digest.update(serde_json::to_vec(rules).map_err(|_| invalid())?);
    Ok(format!("{:x}", digest.finalize()))
}

impl PreparedBackup {
    pub fn write_archive(&self, writer: &mut impl Write, kind: BackupKind) -> Result<(), Fault> {
        let pages: u64 = self
            .database
            .query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let page_size: u64 = self
            .database
            .query_row("PRAGMA page_size", [], |r| r.get(0))?;
        if pages
            .checked_mul(page_size)
            .is_none_or(|len| len > MAX_DATABASE_BYTES)
        {
            return Err(invalid());
        }
        let bytes = self.database.serialize(rusqlite::MAIN_DB)?;
        let manifest = BackupManifest {
            format_version: 1,
            schema_version: self
                .database
                .query_row("PRAGMA user_version", [], |row| row.get(0))?,
            created_at: crate::service::timestamp(),
            kind,
            database_bytes: bytes.len() as u64,
            database_sha256: format!("{:x}", Sha256::digest(&*bytes)),
            rules: self.rules.clone(),
        };
        if !valid_manifest(&manifest) {
            return Err(invalid());
        }
        let mut encoded = Vec::new();
        // Bound JSON output during serialization rather than trusting a size
        // header supplied by an eventual restore file.
        struct Bounded<'a>(&'a mut Vec<u8>);
        impl Write for Bounded<'_> {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if self.0.len().saturating_add(bytes.len()) > MAX_MANIFEST_BYTES as usize {
                    return Err(std::io::ErrorKind::FileTooLarge.into());
                }
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        serde_json::to_writer(Bounded(&mut encoded), &manifest).map_err(|_| invalid())?;
        writer
            .write_all(MAGIC)
            .and_then(|_| writer.write_all(&(encoded.len() as u64).to_le_bytes()))
            .and_then(|_| writer.write_all(&encoded))
            .and_then(|_| writer.write_all(&bytes))
            .map_err(|_| Fault::new("BACKUP_STORAGE", 8, "Backup could not be written"))?;
        Ok(())
    }
}

/// Inspects format and checksums only. This is not restore authorization or
/// SQLite integrity/credential validation of an untrusted restore source.
pub fn inspect_backup_archive(reader: &mut impl Read) -> Result<BackupManifest, Fault> {
    let manifest = read_manifest(reader)?;
    inspect_database(reader, &manifest)?;
    Ok(manifest)
}

pub(crate) fn read_manifest(reader: &mut impl Read) -> Result<BackupManifest, Fault> {
    let mut magic = [0; 13];
    reader.read_exact(&mut magic).map_err(|_| invalid())?;
    if &magic != MAGIC {
        return Err(invalid());
    }
    let mut size = [0; 8];
    reader.read_exact(&mut size).map_err(|_| invalid())?;
    let size = u64::from_le_bytes(size);
    if size == 0 || size > MAX_MANIFEST_BYTES {
        return Err(invalid());
    }
    let mut encoded = vec![0; size as usize];
    reader.read_exact(&mut encoded).map_err(|_| invalid())?;
    let manifest: BackupManifest = serde_json::from_slice(&encoded).map_err(|_| invalid())?;
    if !valid_manifest(&manifest) {
        return Err(invalid());
    }
    Ok(manifest)
}

pub(crate) fn inspect_database(
    reader: &mut impl Read,
    manifest: &BackupManifest,
) -> Result<(), Fault> {
    let mut sqlite_header = [0; 100];
    reader
        .read_exact(&mut sqlite_header)
        .map_err(|_| invalid())?;
    if &sqlite_header[..16] != b"SQLite format 3\0"
        || u32::from_be_bytes(sqlite_header[60..64].try_into().map_err(|_| invalid())?)
            != manifest.schema_version
    {
        return Err(invalid());
    }
    let mut hash = Sha256::new();
    hash.update(sqlite_header);
    let mut remaining = manifest.database_bytes - 100;
    let mut buffer = [0; 64 * 1024];
    while remaining > 0 {
        let length = remaining.min(buffer.len() as u64) as usize;
        reader
            .read_exact(&mut buffer[..length])
            .map_err(|_| invalid())?;
        hash.update(&buffer[..length]);
        remaining -= length as u64;
    }
    if format!("{:x}", hash.finalize()) != manifest.database_sha256 {
        return Err(invalid());
    }
    if reader.read(&mut buffer[..1]).map_err(|_| invalid())? != 0 {
        return Err(invalid());
    }
    Ok(())
}
