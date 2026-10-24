use crate::{accounts::digest, protocol::Fault};
use rusqlite::{TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_CREDENTIAL_BYTES: usize = 4096;

#[derive(Clone, Deserialize, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CredentialCandidate {
    Human {
        id: String,
        token: String,
        recovery_code: Option<String>,
    },
    Ai {
        id: String,
        project_id: String,
        session_id: String,
        session_token: String,
        resume_secret: String,
    },
}

impl std::fmt::Debug for CredentialCandidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialCandidate([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CredentialValidity {
    pub session: bool,
    pub resume: bool,
    pub recovery: bool,
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, Fault> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty() && value.len() <= MAX_CREDENTIAL_BYTES)
        .ok_or_else(invalid_grant)
}

fn invalid_grant() -> Fault {
    Fault::new(
        "INVALID_CREDENTIAL_GRANT",
        2,
        "The private credential grant is malformed",
    )
}

impl CredentialCandidate {
    pub fn from_grant(operation: &str, grant: &Value) -> Result<Self, Fault> {
        match operation {
            "account.setup"
            | "account.login"
            | "account.password"
            | "account.recover"
            | "account.google.recover" => {
                let id_value = if operation == "account.password" {
                    grant.get("account").ok_or_else(invalid_grant)?
                } else {
                    grant
                };
                let recovery_code = match grant.get("recovery_code") {
                    None | Some(Value::Null) => None,
                    Some(_) => Some(text(grant, "recovery_code")?.to_owned()),
                };
                Ok(Self::Human {
                    id: text(id_value, "id")?.to_owned(),
                    token: text(grant, "token")?.to_owned(),
                    recovery_code,
                })
            }
            "identity.start" | "identity.resume" => {
                let identity = grant.get("identity").ok_or_else(invalid_grant)?;
                Ok(Self::Ai {
                    id: text(identity, "id")?.to_owned(),
                    project_id: text(identity, "project_id")?.to_owned(),
                    session_id: text(grant, "session_id")?.to_owned(),
                    session_token: text(grant, "session_token")?.to_owned(),
                    resume_secret: text(grant, "resume_secret")?.to_owned(),
                })
            }
            _ => Err(invalid_grant()),
        }
    }
}

pub(crate) fn validate(
    db: &mut rusqlite::Connection,
    candidate: &CredentialCandidate,
) -> Result<CredentialValidity, Fault> {
    let bounded = match candidate {
        CredentialCandidate::Human {
            id,
            token,
            recovery_code,
        } => {
            [id, token]
                .into_iter()
                .all(|value| !value.trim().is_empty() && value.len() <= MAX_CREDENTIAL_BYTES)
                && recovery_code.as_ref().is_none_or(|value| {
                    !value.trim().is_empty() && value.len() <= MAX_CREDENTIAL_BYTES
                })
        }
        CredentialCandidate::Ai {
            id,
            project_id,
            session_id,
            session_token,
            resume_secret,
        } => [id, project_id, session_id, session_token, resume_secret]
            .into_iter()
            .all(|value| !value.trim().is_empty() && value.len() <= MAX_CREDENTIAL_BYTES),
    };
    if !bounded {
        return Err(invalid_grant());
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Deferred)?;
    let now = crate::service::timestamp();
    let validity = match candidate {
        CredentialCandidate::Human {
            id,
            token,
            recovery_code,
        } => {
            let session: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM human_sessions s JOIN accounts a ON a.id=s.account_id WHERE s.token_hash=?1 AND s.account_id=?2 AND a.disabled=0)",
                        params![digest(token), id],
                        |row| row.get(0),
                    )?;
            let recovery: bool = if let Some(code) = recovery_code {
                tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1 AND is_admin=1 AND disabled=0 AND recovery_hash=?2)",
                        params![id, digest(code)],
                        |row| row.get(0),
                    )?
            } else {
                false
            };
            CredentialValidity {
                session,
                recovery,
                resume: false,
            }
        }
        CredentialCandidate::Ai {
            id,
            project_id,
            session_id,
            session_token,
            resume_secret,
        } => {
            let session: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM ai_sessions s JOIN ai_identities i ON i.id=s.actor_id WHERE s.actor_id=?1 AND i.project_id=?2 AND s.session_id=?3 AND s.token_hash=?4 AND i.expires_at>?5)",
                        params![id, project_id, session_id, digest(session_token), now],
                        |row| row.get(0),
                    )?;
            let resume: bool = tx
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM ai_identities WHERE id=?1 AND project_id=?2 AND resume_hash=?3 AND expires_at>?4)",
                        params![id, project_id, digest(resume_secret), now],
                        |row| row.get(0),
                    )?;
            CredentialValidity {
                session,
                resume,
                recovery: false,
            }
        }
    };
    tx.commit()?;
    Ok(validity)
}
