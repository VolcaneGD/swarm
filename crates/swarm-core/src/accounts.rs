use crate::{
    protocol::Fault,
    service::{required_text, timestamp},
};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use chrono::Local;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub(crate) fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub(crate) fn secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
pub(crate) fn hash_password(password: &str) -> Result<String, Fault> {
    let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(|_| Fault::internal())?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| Fault::internal())
}
pub(crate) fn verify_password(password: &str, encoded: &str) -> Result<(), Fault> {
    let parsed = PasswordHash::new(encoded).map_err(|_| Fault::internal())?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .map_err(|_| Fault::authentication())
}

fn optional_password(args: &Value) -> Result<&str, Fault> {
    args.get("password")
        .and_then(Value::as_str)
        .filter(|password| {
            password.len() <= 4096 && (password.is_empty() || !password.trim().is_empty())
        })
        .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "Password input is invalid"))
}
pub(crate) fn authenticate(db: &Connection, auth: Option<&str>) -> Result<String, Fault> {
    let token = auth.ok_or_else(Fault::authentication)?;
    db.query_row(
        "SELECT a.id FROM human_sessions s JOIN accounts a ON a.id=s.account_id WHERE s.token_hash=?1 AND a.disabled=0",
        [digest(token)], |r| r.get(0),
    ).optional()?.ok_or_else(Fault::authentication)
}
pub(crate) fn new_session(db: &Connection, account_id: &str) -> Result<String, Fault> {
    let token = secret();
    db.execute(
        "INSERT INTO human_sessions(token_hash,account_id) VALUES(?1,?2)",
        params![digest(&token), account_id],
    )?;
    Ok(token)
}
pub(crate) fn setup(db: &Connection, args: &Value) -> Result<Value, Fault> {
    let count: i64 = db.query_row("SELECT count(*) FROM accounts", [], |r| r.get(0))?;
    if count != 0 {
        return Err(Fault::new(
            "ALREADY_CONFIGURED",
            6,
            "The main Admin already exists",
        ));
    }
    let name = required_text(args, "name")?;
    let password = optional_password(args)?;
    let password_hash = if password.is_empty() {
        String::new()
    } else {
        hash_password(password)?
    };
    let id = Uuid::new_v4().to_string();
    let recovery = secret();
    db.execute("INSERT INTO actors(id,kind) VALUES(?1,'human')", [&id])?;
    db.execute("INSERT INTO accounts(id,name,password_hash,recovery_hash,is_admin,created_at) VALUES(?1,?2,?3,?4,1,?5)",
        params![id,name,password_hash,digest(&recovery),timestamp()])?;
    let token = new_session(db, &id)?;
    // Private GUI bootstrap response. Never expose these credentials through public AI CLI output.
    Ok(json!({"id":id,"name":name,"token":token,"recovery_code":recovery}))
}
pub(crate) fn login(db: &Connection, args: &Value) -> Result<Value, Fault> {
    let name = required_text(args, "name")?;
    let password = optional_password(args)?;
    let account: Option<(String, String)> = db
        .query_row(
            "SELECT id,password_hash FROM accounts WHERE name=?1 AND disabled=0",
            [name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let (id, hash) = account.ok_or_else(Fault::authentication)?;
    if !hash.is_empty() {
        verify_password(password, &hash)?;
    }
    Ok(json!({"id":id,"token":new_session(db,&id)?}))
}
fn missing_account() -> Fault {
    Fault::new("NOT_FOUND", 5, "The account was not found")
}
fn public(db: &Connection, id: &str) -> Result<Value, Fault> {
    let mut account = db
        .query_row(
            "SELECT id,name,is_admin,disabled FROM accounts WHERE id=?1",
            [id],
            |r| {
                let admin: i64 = r.get(2)?;
                let disabled: i64 = r.get(3)?;
                Ok(json!({
                    "id":r.get::<_,String>(0)?,
                    "name":r.get::<_,String>(1)?,
                    "is_admin":admin != 0,
                    "disabled":disabled != 0
                }))
            },
        )
        .optional()?
        .ok_or_else(missing_account)?;
    let mut query =
        db.prepare("SELECT role_id FROM role_members WHERE account_id=?1 ORDER BY role_id")?;
    account["lock_enabled"] = json!(db.query_row(
        "SELECT password_hash<>'' FROM accounts WHERE id=?1",
        [id],
        |r| r.get::<_, bool>(0)
    )?);
    let roles = query
        .query_map([id], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    account["role_ids"] = json!(roles);
    let link = crate::google_recovery::public_link(db, id)?;
    account["google_linked"] = link["linked"].clone();
    account["google_email"] = link["email"].clone();
    Ok(account)
}
fn is_main_admin(db: &Connection, id: &str) -> Result<bool, Fault> {
    db.query_row("SELECT is_admin FROM accounts WHERE id=?1", [id], |r| {
        r.get::<_, bool>(0)
    })
    .optional()?
    .ok_or_else(missing_account)
}
fn reject_main_admin(db: &Connection, id: &str) -> Result<(), Fault> {
    if is_main_admin(db, id)? {
        return Err(Fault::new(
            "PERMISSION_DENIED",
            4,
            "The main Admin account cannot be disabled or reset by this operation",
        ));
    }
    Ok(())
}
pub(crate) fn dispatch(
    db: &Connection,
    operation: &str,
    args: &Value,
    actor: &str,
) -> Result<Value, Fault> {
    match operation {
        "account.google.begin"
        | "account.google.link"
        | "account.google.unlink"
        | "account.google.recover" => crate::google_recovery::dispatch(db, operation, args, actor),
        "account.create" => {
            let name = required_text(args, "name")?;
            let password = optional_password(args)?;
            let exists: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE name=?1)",
                [name],
                |r| r.get(0),
            )?;
            if exists {
                return Err(Fault::new(
                    "ACCOUNT_NAME_CONFLICT",
                    6,
                    "An account with this name already exists",
                ));
            }
            let password_hash = if password.is_empty() {
                String::new()
            } else {
                hash_password(password)?
            };
            let id = Uuid::new_v4().to_string();
            db.execute("INSERT INTO actors(id,kind) VALUES(?1,'human')", [&id])?;
            db.execute(
                "INSERT INTO accounts(id,name,password_hash,recovery_hash,is_admin,disabled,created_at) VALUES(?1,?2,?3,'',0,0,?4)",
                params![id,name,password_hash,timestamp()],
            )?;
            public(db, &id)
        }
        "account.list" => {
            let mut query = db.prepare("SELECT id FROM accounts ORDER BY created_at,id")?;
            let ids = query
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(
                json!({"items":ids.into_iter().map(|id|public(db,&id)).collect::<Result<Vec<_>,_>>()?}),
            )
        }
        "account.get" => public(db, required_text(args, "id")?),
        "account.disable" => {
            let id = required_text(args, "id")?;
            let disabled = args
                .get("disabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| Fault::new("INVALID_INPUT", 2, "disabled must be a boolean"))?;
            reject_main_admin(db, id)?;
            db.execute(
                "UPDATE accounts SET disabled=?1 WHERE id=?2",
                params![disabled, id],
            )?;
            if disabled {
                db.execute("DELETE FROM human_sessions WHERE account_id=?1", [id])?;
            }
            public(db, id)
        }
        "account.reset" => {
            let id = required_text(args, "id")?;
            let password = required_text(args, "password")?;
            reject_main_admin(db, id)?;
            let password_hash = hash_password(password)?;
            db.execute(
                "UPDATE accounts SET password_hash=?1 WHERE id=?2",
                params![password_hash, id],
            )?;
            db.execute("DELETE FROM human_sessions WHERE account_id=?1", [id])?;
            public(db, id)
        }
        "account.password" => {
            let password = optional_password(args)?;
            let (name, previous): (String, String) = db.query_row(
                "SELECT name,password_hash FROM accounts WHERE id=?1",
                [actor],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if let Some(supplied) = args.get("name")
                && supplied.as_str() != Some(name.as_str())
            {
                return Err(Fault::authentication());
            }
            if !previous.is_empty() {
                let current = required_text(args, "current_password")?;
                verify_password(current, &previous)?;
            } else if let Some(current) = args.get("current_password")
                && current.as_str() != Some("")
            {
                return Err(Fault::new(
                    "INVALID_INPUT",
                    2,
                    "Current password is not applicable",
                ));
            }
            let password_hash = if password.is_empty() {
                String::new()
            } else {
                hash_password(password)?
            };
            db.execute(
                "UPDATE accounts SET password_hash=?1 WHERE id=?2",
                params![password_hash, actor],
            )?;
            db.execute("DELETE FROM human_sessions WHERE account_id=?1", [actor])?;
            let token = new_session(db, actor)?;
            // The private human adapter must store this replacement token securely.
            Ok(json!({"account":public(db,actor)?,"token":token}))
        }
        "account.recover" => {
            let name = required_text(args, "name")?;
            let recovery_code = required_text(args, "recovery_code")?;
            let password = required_text(args, "password")?;
            let account: Option<(String, String)> = db
                .query_row(
                    "SELECT id,recovery_hash FROM accounts WHERE name=?1 AND is_admin=1",
                    [name],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let (id, stored_hash) = account.ok_or_else(Fault::authentication)?;
            if digest(recovery_code) != stored_hash {
                return Err(Fault::authentication());
            }
            let password_hash = hash_password(password)?;
            let next_recovery_code = secret();
            db.execute(
                "UPDATE accounts SET password_hash=?1,recovery_hash=?2 WHERE id=?3",
                params![password_hash, digest(&next_recovery_code), id],
            )?;
            db.execute("DELETE FROM human_sessions WHERE account_id=?1", [&id])?;
            let token = new_session(db, &id)?;
            // Recovery credentials are private adapter data and must never reach AI CLI output.
            Ok(json!({"id":id,"token":token,"recovery_code":next_recovery_code}))
        }
        "account.status" => public(db, actor),
        _ => Err(Fault::new(
            "UNKNOWN_OPERATION",
            2,
            "Account operation is not supported",
        )),
    }
}
pub(crate) fn display_id(db: &Connection, account_id: &str) -> Result<String, Fault> {
    let date = Local::now().format("%Y-%m-%d").to_string();
    let existing = db
        .query_row(
            "SELECT display_id FROM human_display_ids WHERE account_id=?1 AND local_date=?2",
            params![account_id, date],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    loop {
        let id = Uuid::new_v4().simple().to_string()[..10].to_string();
        let used: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM display_ids WHERE display_id=?1)",
            [&id],
            |r| r.get(0),
        )?;
        if used {
            continue;
        }
        db.execute(
            "INSERT INTO display_ids(display_id,actor_id) VALUES(?1,?2)",
            params![id, account_id],
        )?;
        db.execute(
            "INSERT INTO human_display_ids(account_id,local_date,display_id) VALUES(?1,?2,?3)",
            params![account_id, date, id],
        )?;
        return Ok(id);
    }
}
