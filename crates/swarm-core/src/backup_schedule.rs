use crate::protocol::Fault;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupSettings {
    pub enabled: bool,
    pub interval_seconds: u64,
    pub retention: u16,
    pub destination: Option<PathBuf>,
}
impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_seconds: 86400,
            retention: 7,
            destination: None,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupGeneration {
    pub key: String,
    pub destination: PathBuf,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupIntent {
    pub generation: BackupGeneration,
    pub due_at: i64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackupScheduleState {
    pub version: u32,
    pub last_success_at: Option<i64>,
    pub pending: Option<BackupIntent>,
    pub generations: Vec<BackupGeneration>,
    pub prune: Vec<BackupGeneration>,
}
impl Default for BackupScheduleState {
    fn default() -> Self {
        Self {
            version: 1,
            last_success_at: None,
            pending: None,
            generations: vec![],
            prune: vec![],
        }
    }
}
fn state_error() -> Fault {
    Fault::new(
        "BACKUP_STATE",
        8,
        "Backup settings or schedule could not be verified",
    )
}
fn read<T: serde::de::DeserializeOwned + Default>(db: &Connection, key: &str) -> Result<T, Fault> {
    let value: Option<String> = db
        .query_row("SELECT value FROM app_metadata WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()?;
    value
        .map(|v| serde_json::from_str(&v).map_err(|_| state_error()))
        .transpose()
        .map(|v| v.unwrap_or_default())
}
fn write<T: Serialize>(db: &Connection, key: &str, value: &T) -> Result<(), Fault> {
    db.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![key,serde_json::to_string(value).map_err(|_|state_error())?])?;
    Ok(())
}
fn valid_settings(value: &BackupSettings) -> bool {
    (60..=31_536_000).contains(&value.interval_seconds)
        && (1..=100).contains(&value.retention)
        && value.destination.as_ref().is_none_or(|p| p.is_absolute())
}
fn valid_generation(value: &BackupGeneration) -> bool {
    value.destination.is_absolute()
        && value.key.strip_prefix("scheduled-").is_some_and(|key| {
            uuid::Uuid::parse_str(key)
                .is_ok_and(|id| !id.is_nil() && key == id.hyphenated().to_string())
        })
}
fn valid_state(value: &BackupScheduleState) -> bool {
    if value.version != 1
        || value.last_success_at.is_some_and(|n| n < 0)
        || value.generations.len() > 100
        || value.prune.len() > 100
    {
        return false;
    }
    let mut keys = std::collections::HashSet::new();
    for generation in value.generations.iter().chain(&value.prune) {
        if !valid_generation(generation) || !keys.insert(&generation.key) {
            return false;
        }
    }
    value.pending.as_ref().is_none_or(|intent| {
        intent.due_at >= 0
            && valid_generation(&intent.generation)
            && !keys.contains(&intent.generation.key)
    })
}
pub(crate) fn settings(db: &Connection) -> Result<BackupSettings, Fault> {
    let value = read(db, "backup-settings-v1")?;
    if !valid_settings(&value) {
        return Err(state_error());
    }
    Ok(value)
}
pub(crate) fn set_settings(db: &Connection, value: &BackupSettings) -> Result<(), Fault> {
    if !valid_settings(value) {
        return Err(Fault::new("INVALID_INPUT", 2, "Invalid backup settings"));
    }
    write(db, "backup-settings-v1", value)
}
pub(crate) fn state(db: &Connection) -> Result<BackupScheduleState, Fault> {
    let value = read(db, "backup-schedule-v1")?;
    if !valid_state(&value) {
        return Err(state_error());
    }
    Ok(value)
}
pub(crate) fn set_state(db: &Connection, value: &BackupScheduleState) -> Result<(), Fault> {
    if !valid_state(value) {
        return Err(state_error());
    }
    write(db, "backup-schedule-v1", value)
}
