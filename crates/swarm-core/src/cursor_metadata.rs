//! Lifetime and maintenance for persisted opaque change/list cursor mappings.
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;
use sha2::Digest;

pub(crate) const VERSION: u8 = 2;
pub(crate) const LIFETIME_SECONDS: i64 = 7 * 24 * 60 * 60;

pub(crate) fn now_epoch_seconds() -> i64 {
    chrono::Utc::now().timestamp()
}

pub(crate) fn is_live(version: u8, issued_at: i64, now: i64) -> bool {
    version == VERSION && issued_at >= 0 && now.saturating_sub(issued_at) < LIFETIME_SECONDS
}

pub(crate) fn validate(
    version: u8,
    issued_at: i64,
    now: i64,
) -> Result<(), crate::protocol::Fault> {
    if is_live(version, issued_at, now) {
        Ok(())
    } else {
        Err(crate::protocol::Fault::new(
            "RESYNC_REQUIRED",
            6,
            "The cursor is no longer valid; request a fresh snapshot",
        ))
    }
}

fn token_key_for_state(state_key: &str, token: &str) -> String {
    let namespace = if state_key.starts_with("changes.cursor.state.listing.") {
        "changes.cursor.token.listing."
    } else {
        "changes.cursor.token."
    };
    format!("{namespace}{:x}", sha2::Sha256::digest(token.as_bytes()))
}

fn state_key_for_token(token_key: &str, encoded: &str) -> String {
    let namespace = if token_key.starts_with("changes.cursor.token.listing.") {
        "changes.cursor.state.listing."
    } else {
        "changes.cursor.state."
    };
    format!("{namespace}{:x}", sha2::Sha256::digest(encoded.as_bytes()))
}

fn token_is_expired(encoded: &str, now: i64) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(encoded) else {
        return true;
    };
    let version = value
        .get("version")
        .and_then(Value::as_u64)
        .and_then(|v| u8::try_from(v).ok());
    let issued_at = value.get("issued_at").and_then(Value::as_i64);
    match (version, issued_at) {
        (Some(version), Some(issued_at)) => !is_live(version, issued_at, now),
        // Legacy cursor states have no trusted issuance time. They are no
        // longer resumable, so remove them without guessing their age.
        _ => true,
    }
}

/// Remove expired or legacy cursor handles and their deterministic state
/// indexes. Call inside the runtime's short Immediate maintenance transaction.
pub(crate) fn prune(db: &Connection, now: i64) -> Result<usize, crate::protocol::Fault> {
    let tokens = {
        let mut query = db.prepare(
            "SELECT key,value FROM app_metadata WHERE key LIKE 'changes.cursor.token.%'",
        )?;
        query
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut removed = 0;
    for (token_key, encoded) in tokens {
        if !token_is_expired(&encoded, now) {
            continue;
        }
        let state_key = state_key_for_token(&token_key, &encoded);
        removed += db.execute("DELETE FROM app_metadata WHERE key=?1", [&token_key])?;
        removed += db.execute("DELETE FROM app_metadata WHERE key=?1", [&state_key])?;
    }

    // A missing token row cannot be resumed. Remove its orphan state index
    // rather than retaining dead metadata indefinitely.
    let states = {
        let mut query = db.prepare(
            "SELECT key,value FROM app_metadata WHERE key LIKE 'changes.cursor.state.%'",
        )?;
        query
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (state_key, token) in states {
        let token_key = token_key_for_state(&state_key, &token);
        let exists: Option<i64> = db
            .query_row(
                "SELECT 1 FROM app_metadata WHERE key=?1",
                [token_key],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            removed += db.execute("DELETE FROM app_metadata WHERE key=?1", [&state_key])?;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{TransactionBehavior, params};

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE app_metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);")
            .unwrap();
        db
    }

    fn insert_pair(db: &Connection, namespace: &str, encoded: &str, token: &str) {
        let state_key = format!(
            "changes.cursor.state.{namespace}{:x}",
            sha2::Sha256::digest(encoded.as_bytes())
        );
        let token_key = format!(
            "changes.cursor.token.{namespace}{:x}",
            sha2::Sha256::digest(token.as_bytes())
        );
        db.execute(
            "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
            params![state_key, token],
        )
        .unwrap();
        db.execute(
            "INSERT INTO app_metadata(key,value) VALUES(?1,?2)",
            params![token_key, encoded],
        )
        .unwrap();
    }

    #[test]
    fn expiry_is_fixed_at_issue_time_and_pruning_removes_both_indexes() {
        let mut db = db();
        let issued = 1_800_000_000_i64;
        let now = issued + LIFETIME_SECONDS;
        assert!(is_live(VERSION, issued, now - 1));
        assert!(!is_live(VERSION, issued, now));
        for namespace in ["", "listing."] {
            insert_pair(
                &db,
                namespace,
                &format!(r#"{{"version":2,"issued_at":{issued}}}"#),
                &format!("{namespace}token"),
            );
        }
        insert_pair(&db, "", r#"{"version":1,"sequence":12}"#, "legacy-token");
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        let removed = prune(&tx, now).unwrap();
        tx.commit().unwrap();
        assert_eq!(removed, 6);
        let remaining: i64 = db
            .query_row("SELECT COUNT(*) FROM app_metadata", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining, 0);
        assert_eq!(
            validate(VERSION, issued, now).unwrap_err().code,
            "RESYNC_REQUIRED"
        );
        assert_eq!(
            validate(1, issued, now - 1).unwrap_err().code,
            "RESYNC_REQUIRED"
        );
    }

    #[test]
    fn live_cursor_pair_survives_maintenance() {
        let mut db = db();
        let issued = 1_800_000_000_i64;
        insert_pair(
            &db,
            "",
            &format!(r#"{{"version":2,"issued_at":{issued}}}"#),
            "live-token",
        );
        let tx = db
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert_eq!(prune(&tx, issued + LIFETIME_SECONDS - 1).unwrap(), 0);
        tx.commit().unwrap();
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM app_metadata", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }
}
