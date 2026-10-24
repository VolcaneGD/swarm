//! Trusted, device-wide preferences that must be available before human login.
use crate::protocol::Fault;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

const GEOMETRY_KEY: &str = "window-geometry-v1";
const STARTUP_KEY: &str = "startup-desired-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowGeometry {
    pub version: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

impl WindowGeometry {
    fn valid(self) -> bool {
        self.version == 1
            && (-32_768..=32_767).contains(&self.x)
            && (-32_768..=32_767).contains(&self.y)
            && (320..=16_384).contains(&self.width)
            && (240..=16_384).contains(&self.height)
    }
}

fn invalid_state() -> Fault {
    Fault::new(
        "DEVICE_SETTINGS_STATE",
        8,
        "Saved device settings could not be verified",
    )
}

pub(crate) fn window_geometry(db: &Connection) -> Result<Option<WindowGeometry>, Fault> {
    let value: Option<String> = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [GEOMETRY_KEY],
            |r| r.get(0),
        )
        .optional()?;
    value
        .map(|encoded| {
            let geometry: WindowGeometry =
                serde_json::from_str(&encoded).map_err(|_| invalid_state())?;
            if !geometry.valid() {
                return Err(invalid_state());
            }
            Ok(geometry)
        })
        .transpose()
}

pub(crate) fn set_window_geometry(db: &Connection, geometry: &WindowGeometry) -> Result<(), Fault> {
    if !geometry.valid() {
        return Err(Fault::new("INVALID_INPUT", 2, "Invalid window geometry"));
    }
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![GEOMETRY_KEY, serde_json::to_string(geometry).map_err(|_| Fault::internal())?],
    )?;
    Ok(())
}

pub(crate) fn startup_preference(db: &Connection) -> Result<Option<bool>, Fault> {
    let value: Option<String> = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [STARTUP_KEY],
            |r| r.get(0),
        )
        .optional()?;
    value
        .map(|encoded| {
            let value: serde_json::Value =
                serde_json::from_str(&encoded).map_err(|_| invalid_state())?;
            if value.get("version") != Some(&serde_json::json!(1))
                || value
                    .as_object()
                    .is_none_or(|object| object.len() != 2 || !object.contains_key("enabled"))
            {
                return Err(invalid_state());
            }
            value["enabled"].as_bool().ok_or_else(invalid_state)
        })
        .transpose()
}

pub(crate) fn set_startup_preference(db: &Connection, enabled: bool) -> Result<(), Fault> {
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![STARTUP_KEY, serde_json::json!({"version":1,"enabled":enabled}).to_string()],
    )?;
    Ok(())
}

pub(crate) fn validate_backup_metadata(db: &Connection) -> Result<(), Fault> {
    window_geometry(db)?;
    startup_preference(db)?;
    let restore_marker: Option<String> = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            ["ui-preferences-legacy-import-disabled-v1"],
            |row| row.get(0),
        )
        .optional()?;
    if restore_marker.is_some_and(|value| value != "true") {
        return Err(invalid_state());
    }
    let mut statement =
        db.prepare("SELECT key,value FROM app_metadata WHERE key LIKE 'ui-preference-v1:%'")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut total = 0usize;
    let mut count = 0usize;
    for row in rows {
        let (key, encoded) = row?;
        total = total
            .saturating_add(key.len())
            .saturating_add(encoded.len());
        count += 1;
        if count > 10_000 || total > 4 * 1024 * 1024 {
            return Err(invalid_state());
        }
        let Some(rest) = key.strip_prefix("ui-preference-v1:") else {
            return Err(invalid_state());
        };
        let (account, suffix) = rest.split_once(':').ok_or_else(invalid_state)?;
        let account_id = uuid::Uuid::parse_str(account).map_err(|_| invalid_state())?;
        if account_id.is_nil() || account_id.hyphenated().to_string() != account {
            return Err(invalid_state());
        }
        let (scope, thread) = match suffix.split_once(':') {
            Some((scope, thread)) => (scope, Some(thread)),
            None => (suffix, None),
        };
        let preference_key = super::ui_preferences::key_for_validation(account, scope, thread)
            .map_err(|_| invalid_state())?;
        if preference_key != key {
            return Err(invalid_state());
        }
        let value: serde_json::Value =
            serde_json::from_str(&encoded).map_err(|_| invalid_state())?;
        super::ui_preferences::validate_for_backup(scope, &value).map_err(|_| invalid_state())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn geometry_and_startup_metadata_are_strictly_validated() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE app_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);")
            .unwrap();
        set_window_geometry(
            &db,
            &WindowGeometry {
                version: 1,
                x: -200,
                y: 40,
                width: 1200,
                height: 800,
                maximized: false,
            },
        )
        .unwrap();
        set_startup_preference(&db, false).unwrap();
        assert!(validate_backup_metadata(&db).is_ok());
        db.execute(
            "UPDATE app_metadata SET value=?1 WHERE key=?2",
            params![
                json!({"version":1,"enabled":"false"}).to_string(),
                STARTUP_KEY
            ],
        )
        .unwrap();
        assert!(validate_backup_metadata(&db).is_err());
    }
}
