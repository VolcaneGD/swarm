use crate::protocol::Fault;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualBackup {
    pub version: u32,
    pub account_id: String,
    pub intent: String,
    pub destination: PathBuf,
    pub completed: bool,
}
fn invalid() -> Fault {
    Fault::new(
        "BACKUP_STATE",
        8,
        "Manual backup record could not be verified",
    )
}
pub fn valid_intent(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok_and(|id| !id.is_nil() && id.to_string() == value)
}
fn valid(v: &ManualBackup) -> bool {
    v.version == 1
        && valid_intent(&v.intent)
        && !v.account_id.is_empty()
        && v.account_id.len() <= 128
        && v.destination.is_absolute()
}
pub(crate) fn get(db: &Connection, intent: &str) -> Result<Option<ManualBackup>, Fault> {
    if !valid_intent(intent) {
        return Err(invalid());
    }
    let raw: Option<String> = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [format!("backup-manual-v1:{intent}")],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| {
        let value: ManualBackup = serde_json::from_str(&s).map_err(|_| invalid())?;
        if !valid(&value) || value.intent != intent {
            return Err(invalid());
        }
        Ok(value)
    })
    .transpose()
}
pub(crate) fn set(db: &Connection, value: &ManualBackup) -> Result<(), Fault> {
    if !valid(value) {
        return Err(invalid());
    }
    db.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",rusqlite::params![format!("backup-manual-v1:{}",value.intent),serde_json::to_string(value).map_err(|_|invalid())?])?;
    Ok(())
}

pub(crate) fn list(db: &Connection) -> Result<Vec<ManualBackup>, Fault> {
    let intents = {
        let mut statement = db.prepare(
            "SELECT substr(key,length('backup-manual-v1:')+1) FROM app_metadata \
             WHERE key LIKE 'backup-manual-v1:%' ORDER BY key",
        )?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?
    };
    intents
        .iter()
        .map(|intent| get(db, intent)?.ok_or_else(invalid))
        .collect()
}

pub(crate) fn forget(db: &Connection, intent: &str) -> Result<bool, Fault> {
    let Some(record) = get(db, intent)? else {
        return Ok(false);
    };
    if !record.completed {
        return Err(Fault::new(
            "BACKUP_PENDING",
            6,
            "A pending backup record cannot be forgotten",
        ));
    }
    Ok(db.execute(
        "DELETE FROM app_metadata WHERE key=?1 AND value=?2",
        rusqlite::params![
            format!("backup-manual-v1:{intent}"),
            serde_json::to_string(&record).map_err(|_| invalid())?
        ],
    )? == 1)
}
