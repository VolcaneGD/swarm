//! Small, account-scoped desktop preferences persisted by the authenticated
//! human operation layer and included in normal database backups.
use crate::protocol::Fault;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use uuid::Uuid;

const PREFIX: &str = "ui-preference-v1:";
const RESTORE_MARKER: &str = "ui-preferences-legacy-import-disabled-v1";
const MAX_VALUE_BYTES: usize = 4096;

fn invalid() -> Fault {
    Fault::new("INVALID_INPUT", 2, "Invalid UI preference")
}

fn account_changed() -> Fault {
    Fault::new("ACCOUNT_CHANGED", 6, "The selected account has changed")
}

pub(crate) fn key_for_validation(
    actor: &str,
    scope: &str,
    key: Option<&str>,
) -> Result<String, Fault> {
    let suffix = match scope {
        "layout" | "conversation_view" if key.is_none() => scope.to_owned(),
        "thread_scroll" => {
            let thread = key.ok_or_else(invalid)?;
            let parsed = Uuid::parse_str(thread).map_err(|_| invalid())?;
            if parsed.is_nil() || parsed.hyphenated().to_string() != thread {
                return Err(invalid());
            }
            format!("thread_scroll:{thread}")
        }
        _ => return Err(invalid()),
    };
    Ok(format!("{PREFIX}{actor}:{suffix}"))
}

fn number_in(value: &Value, key: &str, minimum: f64, maximum: f64) -> bool {
    value
        .get(key)
        .and_then(Value::as_f64)
        .is_some_and(|number| number.is_finite() && (minimum..=maximum).contains(&number))
}

fn exact_keys(value: &Value, required: &[&str], optional: &[&str]) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    required.iter().all(|key| object.contains_key(*key))
        && object
            .keys()
            .all(|key| required.contains(&key.as_str()) || optional.contains(&key.as_str()))
}

fn valid_scroll(value: &Value) -> bool {
    if !exact_keys(
        value,
        &["version", "postNo", "offset", "scrollTop", "atBottom"],
        &["page", "pageSize"],
    ) || value["version"] != 1
        || !(value["postNo"].is_null()
            || value["postNo"]
                .as_u64()
                .is_some_and(|n| (1..=1000).contains(&n)))
        || !number_in(value, "offset", -1_000_000.0, 1_000_000.0)
        || !number_in(value, "scrollTop", 0.0, 10_000_000.0)
        || !value["atBottom"].is_boolean()
    {
        return false;
    }
    if value
        .get("page")
        .is_some_and(|page| !page.as_u64().is_some_and(|n| (1..=20).contains(&n)))
    {
        return false;
    }
    if value
        .get("pageSize")
        .is_some_and(|page_size| !matches!(page_size.as_u64(), Some(50 | 100 | 500 | 1000)))
    {
        return false;
    }
    true
}

fn validate(scope: &str, value: &Value) -> Result<(), Fault> {
    let valid = match scope {
        "layout" => {
            exact_keys(
                value,
                &["version", "treeWidth", "composerHeight"],
                &["logExpanded"],
            ) && value["version"] == 1
                && number_in(value, "treeWidth", 220.0, 480.0)
                && number_in(value, "composerHeight", 200.0, 480.0)
                && value.get("logExpanded").is_none_or(Value::is_boolean)
        }
        "conversation_view" => matches!(value.as_str(), Some("timeline" | "threaded")),
        "thread_scroll" => valid_scroll(value),
        _ => false,
    };
    if !valid || serde_json::to_vec(value).map_or(true, |bytes| bytes.len() > MAX_VALUE_BYTES) {
        return Err(invalid());
    }
    Ok(())
}

pub(crate) fn validate_for_backup(scope: &str, value: &Value) -> Result<(), Fault> {
    validate(scope, value)
}

pub(crate) fn dispatch(
    db: &Connection,
    operation: &str,
    actor: &str,
    args: &Value,
) -> Result<Value, Fault> {
    let fields: &[&str] = if operation == "ui.preferences.get" {
        &["expected_account_id", "scope", "key"]
    } else {
        &["expected_account_id", "scope", "key", "value"]
    };
    let object = args.as_object().ok_or_else(invalid)?;
    if object.keys().any(|field| !fields.contains(&field.as_str())) {
        return Err(invalid());
    }
    let expected = args
        .get("expected_account_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid)?;
    if expected != actor {
        return Err(account_changed());
    }
    let scope = args
        .get("scope")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    let key = args.get("key").and_then(Value::as_str);
    if (scope == "thread_scroll") != object.contains_key("key") {
        return Err(invalid());
    }
    let metadata_key = key_for_validation(actor, scope, key)?;
    if operation == "ui.preferences.get" {
        let value: Option<String> = db
            .query_row(
                "SELECT value FROM app_metadata WHERE key=?1",
                [&metadata_key],
                |row| row.get(0),
            )
            .optional()?;
        let value = value
            .map(|encoded| {
                let value: Value = serde_json::from_str(&encoded).map_err(|_| {
                    Fault::new(
                        "UI_PREFERENCE_STATE",
                        8,
                        "Saved UI preference could not be verified",
                    )
                })?;
                validate(scope, &value).map_err(|_| {
                    Fault::new(
                        "UI_PREFERENCE_STATE",
                        8,
                        "Saved UI preference could not be verified",
                    )
                })?;
                Ok::<Value, Fault>(value)
            })
            .transpose()?;
        let restored: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM app_metadata WHERE key=?1)",
            [RESTORE_MARKER],
            |row| row.get(0),
        )?;
        return Ok(json!({"value":value,"migration_allowed":!restored}));
    }
    let value = args.get("value").ok_or_else(invalid)?;
    validate(scope, value)?;
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![metadata_key, serde_json::to_string(value).map_err(|_| Fault::internal())?],
    )?;
    Ok(json!({"saved":true}))
}

pub(crate) fn mark_restored(db: &Connection) -> Result<(), Fault> {
    db.execute(
        "INSERT INTO app_metadata(key,value) VALUES(?1,'true') ON CONFLICT(key) DO UPDATE SET value='true'",
        [RESTORE_MARKER],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_validation_rejects_unknown_or_out_of_range_fields() {
        assert!(
            validate(
                "layout",
                &json!({"version":1,"treeWidth":280,"composerHeight":230})
            )
            .is_ok()
        );
        assert!(
            validate(
                "layout",
                &json!({"version":1,"treeWidth":281,"composerHeight":230,"extra":true})
            )
            .is_err()
        );
        assert!(validate("conversation_view", &json!("threaded")).is_ok());
        assert!(
            validate(
                "layout",
                &json!({"version":1,"treeWidth":280,"composerHeight":230,"logExpanded":true})
            )
            .is_ok()
        );
        assert!(
            validate(
                "layout",
                &json!({"version":1,"treeWidth":280,"composerHeight":230,"logExpanded":"true"})
            )
            .is_err()
        );
        assert!(validate("conversation_view", &json!("unknown")).is_err());
        assert!(key_for_validation("account", "thread_scroll", Some("../bad")).is_err());
    }

    #[test]
    fn restore_marker_disables_legacy_import_even_when_scope_is_absent() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE app_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);")
            .unwrap();
        mark_restored(&db).unwrap();
        let value = dispatch(
            &db,
            "ui.preferences.get",
            "00000000-0000-4000-8000-000000000001",
            &json!({"expected_account_id":"00000000-0000-4000-8000-000000000001","scope":"layout"}),
        )
        .unwrap();
        assert_eq!(value, json!({"value":null,"migration_allowed":false}));
    }
}
