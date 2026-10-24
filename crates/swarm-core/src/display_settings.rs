use crate::protocol::Fault;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DisplayLanguage {
    En,
    Ja,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DisplayTheme {
    Light,
    Dark,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplaySettings {
    pub language: DisplayLanguage,
    pub theme: DisplayTheme,
}

pub(crate) fn read(db: &Connection, actor: &str) -> Result<Option<DisplaySettings>, Fault> {
    let saved: Option<String> = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [format!("display-settings-v1:{actor}")],
            |row| row.get(0),
        )
        .optional()?;
    saved
        .map(|value| {
            serde_json::from_str(&value).map_err(|_| {
                Fault::new(
                    "DISPLAY_SETTINGS_STATE",
                    8,
                    "Saved display settings could not be verified",
                )
            })
        })
        .transpose()
}
pub(crate) fn dispatch(
    db: &Connection,
    operation: &str,
    actor: &str,
    args: &Value,
) -> Result<Value, Fault> {
    if operation == "display.settings.get" {
        return Ok(json!({"settings": read(db, actor)?}));
    }
    let settings: DisplaySettings = serde_json::from_value(args.clone())
        .map_err(|_| Fault::new("INVALID_INPUT", 2, "Invalid display settings"))?;
    db.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![format!("display-settings-v1:{actor}"), serde_json::to_string(&settings).map_err(|_| Fault::internal())?])?;
    Ok(json!(settings))
}
