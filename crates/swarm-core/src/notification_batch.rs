//! Private, bounded aggregation journal. No body or credential is persisted here.
use crate::{CredentialCandidate, NotificationDelivery, notifications, protocol::Fault};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

const KEY: &str = "notification-batch-v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotificationBatch {
    pub id: String,
    pub deliveries: Vec<NotificationDelivery>,
    pub language: Option<crate::DisplayLanguage>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    generation: String,
    thread_id: String,
    id: String,
    members: Vec<String>,
    ready_at: i64,
    frozen: bool,
    attempts: u32,
    retry_at: i64,
}

fn load(db: &Connection) -> Result<Option<Journal>, Fault> {
    let raw: Option<String> = db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [KEY],
            |row| row.get(0),
        )
        .optional()?;
    raw.map(|raw| {
        if raw.len() > 32768 {
            return Err(Fault::internal());
        }
        let journal: Journal = serde_json::from_str(&raw).map_err(|_| Fault::internal())?;
        if journal.members.is_empty()
            || journal.members.len() > 200
            || journal
                .members
                .iter()
                .any(|id| id.is_empty() || id.len() > 128)
            || !journal.id.starts_with("batch:")
            || journal.id.len() != 42
            || journal.generation.is_empty()
            || journal.generation.len() > 64
            || journal.thread_id.is_empty()
            || journal.thread_id.len() > 4096
        {
            return Err(Fault::internal());
        }
        Ok(journal)
    })
    .transpose()
}

fn save(db: &Connection, journal: &Journal) -> Result<(), Fault> {
    let value = serde_json::to_string(journal).map_err(|_| Fault::internal())?;
    db.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![KEY,value])?;
    Ok(())
}

pub(crate) fn prepare(
    db: &mut Connection,
    candidate: &CredentialCandidate,
    now_ms: i64,
    window_ms: u32,
) -> Result<Option<NotificationBatch>, Fault> {
    if now_ms < 0 || !(1..=60000).contains(&window_ms) {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Invalid notification aggregation clock or window",
        ));
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (account, _, generation) = notifications::require_selected(&tx, candidate)?;
    let mut journal = load(&tx)?;
    if journal.as_ref().is_some_and(|j| j.generation != generation) {
        return Err(Fault::internal());
    }
    let mut eligible = notifications::prepare_filtered(
        &tx,
        candidate,
        200,
        journal
            .as_ref()
            .filter(|j| j.frozen)
            .map(|j| j.members.as_slice()),
        journal.as_ref().filter(|j| !j.frozen).map(|j| j.ready_at),
        journal.as_ref().map(|j| j.thread_id.as_str()),
    )?;
    if let Some(j) = &journal
        && !eligible.iter().any(|item| j.members.contains(&item.id))
    {
        tx.execute("DELETE FROM app_metadata WHERE key=?1", [KEY])?;
        journal = None;
        eligible = notifications::prepare_in_transaction(&tx, candidate, 200)?;
    }
    if journal.is_none() && !eligible.is_empty() {
        // One Thread per batch makes the authorized click target unambiguous.
        let thread_id = eligible[0].thread_id.clone();
        eligible.retain(|item| item.thread_id == thread_id);
        let ids = serde_json::to_string(&eligible.iter().map(|item| &item.id).collect::<Vec<_>>())
            .map_err(|_| Fault::internal())?;
        let oldest: String = tx.query_row(
            "SELECT MIN(created_at) FROM notification_outbox WHERE generation=?1 AND delivery_id IN (SELECT value FROM json_each(?2))",
            params![generation, ids],
            |row| row.get(0),
        )?;
        let born = chrono::DateTime::parse_from_rfc3339(&oldest)
            .map_err(|_| Fault::internal())?
            .timestamp_millis();
        let members: Vec<String> = eligible.iter().map(|item| item.id.clone()).collect();
        eligible = notifications::prepare_filtered(
            &tx,
            candidate,
            200,
            Some(&members),
            Some(born.saturating_add(i64::from(window_ms))),
            Some(&thread_id),
        )?;
        journal = Some(Journal {
            generation,
            thread_id,
            id: format!("batch:{}", uuid::Uuid::new_v4()),
            members: eligible.iter().map(|item| item.id.clone()).collect(),
            ready_at: born.saturating_add(i64::from(window_ms)),
            frozen: false,
            attempts: 0,
            retry_at: 0,
        });
    }
    let Some(mut journal) = journal else {
        tx.commit()?;
        return Ok(None);
    };
    if !journal.frozen {
        journal.members = eligible.iter().map(|item| item.id.clone()).collect();
        journal.frozen = now_ms >= journal.ready_at || journal.members.len() == 200;
    }
    let ready = journal.frozen && now_ms >= journal.retry_at;
    let deliveries = eligible
        .into_iter()
        .filter(|item| journal.members.contains(&item.id))
        .collect();
    save(&tx, &journal)?;
    let batch = ready.then_some(NotificationBatch {
        id: journal.id,
        deliveries,
        language: crate::display_settings::read(&tx, &account)?.map(|settings| settings.language),
    });
    tx.commit()?;
    Ok(batch)
}

pub(crate) fn complete(
    db: &mut Connection,
    candidate: &CredentialCandidate,
    batch_id: &str,
    now_ms: i64,
    delivered: bool,
) -> Result<(), Fault> {
    if now_ms < 0 || batch_id.len() != 42 || !batch_id.starts_with("batch:") {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Invalid notification batch receipt",
        ));
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (_, _, generation) = notifications::require_selected(&tx, candidate)?;
    let Some(mut journal) = load(&tx)? else {
        tx.commit()?;
        return Ok(());
    };
    if journal.generation != generation || journal.id != batch_id || !journal.frozen {
        return Err(Fault::new(
            "NOTIFICATION_CONFLICT",
            6,
            "Notification batch receipt is stale",
        ));
    }
    if delivered {
        for id in &journal.members {
            tx.execute(
                "DELETE FROM notification_outbox WHERE delivery_id=?1 AND generation=?2",
                params![id, generation],
            )?;
        }
        tx.execute("DELETE FROM app_metadata WHERE key=?1", [KEY])?;
    } else {
        let delay = 5000_i64
            .saturating_mul(1_i64 << journal.attempts.min(6))
            .min(300000);
        journal.attempts = journal.attempts.saturating_add(1);
        journal.retry_at = now_ms.saturating_add(delay);
        save(&tx, &journal)?;
    }
    tx.commit()?;
    Ok(())
}
