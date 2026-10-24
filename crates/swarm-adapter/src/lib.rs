//! Private credential persistence for the future CLI and GUI bridges.
//! No credential returned by this crate belongs in public RPC, stdout, or logs.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs::File, path::Path};
use swarm_core::{
    CredentialCandidate, Service,
    protocol::{Fault, Request},
};
use swarm_platform::CredentialStore;

pub mod human;

const JOURNAL_KEY: &str = "candidate-journal";
const MAX_JOURNAL_BYTES: usize = 64 * 1024;

struct JournalBuffer(Vec<u8>);
impl std::io::Write for JournalBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_JOURNAL_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other("Private journal limit exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    slots: BTreeMap<String, Vec<CredentialCandidate>>,
}

pub struct CredentialBroker {
    store: CredentialStore,
    _lease: File,
}
impl std::fmt::Debug for CredentialBroker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialBroker { [redacted] }")
    }
}
fn storage_error() -> Fault {
    Fault::new("CREDENTIAL_STORAGE", 8, "Private credential storage failed")
}
fn invalid() -> Fault {
    Fault::new(
        "INVALID_CREDENTIAL_JOURNAL",
        8,
        "Private credential journal is invalid",
    )
}
fn valid_slot(slot: &str) -> bool {
    (1..=128).contains(&slot.len())
        && slot
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn same_subject(a: &CredentialCandidate, b: &CredentialCandidate) -> bool {
    match (a, b) {
        (CredentialCandidate::Human { id: a, .. }, CredentialCandidate::Human { id: b, .. }) => {
            a == b
        }
        (
            CredentialCandidate::Ai {
                id: a,
                project_id: ap,
                ..
            },
            CredentialCandidate::Ai {
                id: b,
                project_id: bp,
                ..
            },
        ) => a == b && ap == bp,
        _ => false,
    }
}
impl CredentialBroker {
    /// One broker owns the shared vault at a time, including all of its slots.
    /// A competing open fails until this broker is dropped or its process exits.
    pub fn open(root: &Path) -> Result<Self, Fault> {
        let store = CredentialStore::open(root).map_err(|_| storage_error())?;
        let lease = acquire_lease(&store)?;
        let broker = Self {
            store,
            _lease: lease,
        };
        broker.load()?;
        Ok(broker)
    }

    fn load(&self) -> Result<Journal, Fault> {
        let Some(bytes) = self.store.get(JOURNAL_KEY).map_err(|_| storage_error())? else {
            return Ok(Journal {
                version: 1,
                slots: BTreeMap::new(),
            });
        };
        let journal: Journal = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if journal.version != 1
            || journal.slots.iter().any(|(key, values)| {
                !valid_slot(key)
                    || values.is_empty()
                    || values.len() > 2
                    || values
                        .windows(2)
                        .any(|pair| !same_subject(&pair[0], &pair[1]))
            })
        {
            return Err(invalid());
        }
        Ok(journal)
    }
    fn save(&self, journal: &Journal) -> Result<(), Fault> {
        let mut bytes = JournalBuffer(Vec::new());
        serde_json::to_writer(&mut bytes, journal).map_err(|_| storage_error())?;
        self.store
            .put(JOURNAL_KEY, &bytes.0)
            .map_err(|_| storage_error())
    }

    /// Persist an uncommitted candidate without discarding the previous one.
    /// The transaction owner must hold the broker lease until commit or rollback.
    pub fn stage_candidate(
        &mut self,
        slot: &str,
        mut candidate: CredentialCandidate,
    ) -> Result<(), Fault> {
        if !valid_slot(slot) {
            return Err(invalid());
        }
        let mut journal = self.load()?;
        let values = journal.slots.entry(slot.to_owned()).or_default();
        if let Some(previous) = values.last() {
            if !same_subject(previous, &candidate) {
                return Err(invalid());
            }
            if let (
                CredentialCandidate::Human {
                    recovery_code: old, ..
                },
                CredentialCandidate::Human {
                    recovery_code: next,
                    ..
                },
            ) = (previous, &mut candidate)
                && next.is_none()
            {
                *next = old.clone();
            }
        }
        if values.last() == Some(&candidate) {
            return Ok(());
        }
        if values.len() >= 2 {
            return Err(invalid());
        }
        values.push(candidate);
        self.save(&journal)
    }

    pub fn issue(
        &mut self,
        service: &mut Service,
        slot: &str,
        request: Request,
    ) -> Result<serde_json::Value, Fault> {
        if !valid_slot(slot) {
            return Err(invalid());
        }
        self.recover(service)?;
        let operation = request.operation.clone();
        service.request_with_credential_sink(request, |grant| {
            let candidate = CredentialCandidate::from_grant(&operation, grant)?;
            self.stage_candidate(slot, candidate)
        })
    }

    /// Save an adapter memo with the grant before its atomic receipt commits.
    pub fn issue_with_receipt(
        &mut self,
        service: &mut Service,
        slot: &str,
        request: Request,
        receipt: &str,
        mut save_memo: impl FnMut(&CredentialCandidate) -> Result<(), Fault>,
    ) -> Result<serde_json::Value, Fault> {
        if !valid_slot(slot) {
            return Err(invalid());
        }
        self.recover(service)?;
        let operation = request.operation.clone();
        service.request_with_credential_receipt(request, receipt, |grant| {
            let candidate = CredentialCandidate::from_grant(&operation, grant)?;
            self.stage_candidate(slot, candidate.clone())?;
            save_memo(&candidate)
        })
    }

    /// Reconcile encrypted candidates against a committed, read-only DB view.
    /// Does not resume sessions, renew identities, or grant application rights.
    pub fn recover(&mut self, service: &mut Service) -> Result<(), Fault> {
        let mut journal = self.load()?;
        let mut changed = false;
        let mut reconciled = BTreeMap::new();
        for (slot, candidates) in &journal.slots {
            let mut selected = None;
            let mut selected_rank = 0;
            for candidate in candidates {
                let validity = service.validate_credential_candidate(candidate)?;
                let rank = if validity.session {
                    3
                } else if validity.resume {
                    2
                } else if validity.recovery {
                    1
                } else {
                    0
                };
                if rank > 0 && rank >= selected_rank {
                    selected = Some(candidate.clone());
                    selected_rank = rank;
                }
            }
            match selected {
                Some(candidate) => {
                    changed |= candidates != &vec![candidate.clone()];
                    reconciled.insert(slot.clone(), vec![candidate]);
                }
                None => changed = true,
            }
        }
        if changed {
            journal.slots = reconciled;
            self.save(&journal)?;
        }
        Ok(())
    }

    /// Private adapter access only. Never serialize this result as a public reply.
    pub fn credential(
        &mut self,
        service: &mut Service,
        slot: &str,
    ) -> Result<Option<CredentialCandidate>, Fault> {
        if !valid_slot(slot) {
            return Err(invalid());
        }
        self.recover(service)?;
        Ok(self
            .load()?
            .slots
            .remove(slot)
            .and_then(|mut values| values.pop()))
    }
}

#[cfg(windows)]
fn acquire_lease(store: &CredentialStore) -> Result<File, Fault> {
    // No secrets are stored in this coordination file. Share mode zero prevents
    // concurrent brokers from losing another slot's journal update.
    store
        .private_lease(".broker.lock")
        .map_err(|_| storage_error())
}
#[cfg(not(windows))]
fn acquire_lease(_store: &CredentialStore) -> Result<File, Fault> {
    Err(storage_error())
}
