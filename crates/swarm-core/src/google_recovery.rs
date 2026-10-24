//! Optional Google recovery. Only verified, fresh ID tokens can change identity bindings.
use crate::{accounts, protocol::Fault, service::required_text};
use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{io::Read, time::Duration};

/// Google recovery is deliberately unavailable in the first beta distribution.
/// Enabling it requires an explicit build feature and renewed acceptance.
pub fn google_recovery_available() -> bool {
    cfg!(feature = "google-recovery")
}

pub fn google_client_id() -> &'static str {
    "1082621526558-3sp83c7bf85cu9bfvikv75bh925qcngb.apps.googleusercontent.com"
}
fn invalid() -> Fault {
    Fault::new("INVALID_INPUT", 2, "Invalid Google recovery request")
}
fn unavailable() -> Fault {
    Fault::new(
        "GOOGLE_UNAVAILABLE",
        8,
        "Google identity verification could not be completed",
    )
}
fn link_key(id: &str) -> String {
    format!("google-recovery-link-v1:{}", accounts::digest(id))
}
fn challenge_key(id: &str) -> String {
    format!("google-recovery-challenge-v1:{}", accounts::digest(id))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Link {
    version: u32,
    subject_hash: String,
    email: Option<String>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Challenge {
    version: u32,
    nonce: String,
    purpose: String,
    created: i64,
    password_revision: String,
    link_revision: String,
}
#[derive(Clone, Deserialize)]
struct Claims {
    aud: String,
    sub: String,
    nonce: String,
    iat: i64,
    exp: i64,
    azp: Option<String>,
    email: Option<String>,
    email_verified: Option<bool>,
}
fn metadata(db: &Connection, key: &str) -> Result<Option<String>, Fault> {
    Ok(db
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [key],
            |row| row.get(0),
        )
        .optional()?)
}
fn save(db: &Connection, key: &str, value: &impl Serialize) -> Result<(), Fault> {
    let value = serde_json::to_string(value).map_err(|_| Fault::internal())?;
    db.execute("INSERT INTO app_metadata(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value])?;
    Ok(())
}
fn read_link(db: &Connection, id: &str) -> Result<Option<Link>, Fault> {
    metadata(db, &link_key(id))?
        .map(|raw| {
            let link: Link = serde_json::from_str(&raw).map_err(|_| Fault::internal())?;
            if link.version != 1
                || link.subject_hash.len() != 64
                || !link.subject_hash.bytes().all(|b| b.is_ascii_hexdigit())
                || link.email.as_ref().is_some_and(|e| e.len() > 320)
            {
                return Err(Fault::internal());
            }
            Ok(link)
        })
        .transpose()
}
pub(crate) fn public_link(db: &Connection, id: &str) -> Result<Value, Fault> {
    Ok(match read_link(db, id)? {
        Some(link) => json!({"linked":true,"email":link.email}),
        None => json!({"linked":false,"email":null}),
    })
}
fn current_password(db: &Connection, id: &str, args: &Value) -> Result<(), Fault> {
    let supplied = args["current_password"]
        .as_str()
        .filter(|v| v.len() <= 4096)
        .ok_or_else(invalid)?;
    let hash: String = db
        .query_row(
            "SELECT password_hash FROM accounts WHERE id=?1 AND disabled=0",
            [id],
            |r| r.get(0),
        )
        .optional()?
        .ok_or_else(Fault::authentication)?;
    if hash.is_empty() {
        if !supplied.is_empty() {
            return Err(invalid());
        }
    } else {
        accounts::verify_password(supplied, &hash)?;
    }
    Ok(())
}
fn subject_hash(subject: &str) -> String {
    accounts::digest(&format!("https://accounts.google.com\0{subject}"))
}
fn bounded<'a>(args: &'a Value, name: &str, max: usize) -> Result<&'a str, Fault> {
    required_text(args, name).and_then(|value| {
        if value.len() <= max {
            Ok(value)
        } else {
            Err(invalid())
        }
    })
}
fn verified(token: &str, nonce: &str) -> Result<Claims, Fault> {
    let header = decode_header(token).map_err(|_| Fault::authentication())?;
    if header.alg != Algorithm::RS256 || header.kid.as_ref().is_none_or(|kid| kid.len() > 256) {
        return Err(Fault::authentication());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| unavailable())?;
    let response = client
        .get("https://www.googleapis.com/oauth2/v3/certs")
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|_| unavailable())?;
    let mut bytes = Vec::new();
    response
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| unavailable())?;
    if bytes.len() > 65536 {
        return Err(unavailable());
    }
    let keys: JwkSet = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    verify_with_keys(
        token,
        nonce,
        &keys,
        google_client_id(),
        Utc::now().timestamp(),
    )
}
fn verify_with_keys(
    token: &str,
    nonce: &str,
    keys: &JwkSet,
    audience: &str,
    now: i64,
) -> Result<Claims, Fault> {
    let header = decode_header(token).map_err(|_| Fault::authentication())?;
    if header.alg != Algorithm::RS256 {
        return Err(Fault::authentication());
    }
    let key = keys
        .find(header.kid.as_deref().ok_or_else(Fault::authentication)?)
        .ok_or_else(Fault::authentication)?;
    let decoding = DecodingKey::from_jwk(key).map_err(|_| Fault::authentication())?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[audience]);
    validation.set_issuer(&["https://accounts.google.com", "accounts.google.com"]);
    validation.set_required_spec_claims(&["exp", "iat", "iss", "aud", "sub"]);
    validation.leeway = 0;
    let claims = decode::<Claims>(token, &decoding, &validation)
        .map_err(|_| Fault::authentication())?
        .claims;
    if claims.aud != audience
        || claims.nonce != nonce
        || claims.sub.is_empty()
        || claims.sub.len() > 255
        || !claims.sub.is_ascii()
        || claims.iat > now + 30
        || claims.iat < now - 330
        || claims.exp <= now
        || claims.azp.as_deref().is_some_and(|v| v != audience)
    {
        return Err(Fault::authentication());
    }
    Ok(claims)
}
pub(crate) fn dispatch(
    db: &Connection,
    operation: &str,
    args: &Value,
    actor: &str,
) -> Result<Value, Fault> {
    dispatch_verified(db, operation, args, actor, verified)
}
fn dispatch_verified(
    db: &Connection,
    operation: &str,
    args: &Value,
    actor: &str,
    verify: impl FnOnce(&str, &str) -> Result<Claims, Fault>,
) -> Result<Value, Fault> {
    if operation == "account.google.unlink" {
        current_password(db, actor, args)?;
        db.execute(
            "DELETE FROM app_metadata WHERE key IN (?1,?2)",
            params![link_key(actor), challenge_key(actor)],
        )?;
        return Ok(json!({"linked":false}));
    }
    let recovery = operation == "account.google.recover"
        || (operation == "account.google.begin" && args["purpose"] == "recover");
    let (id, hash): (String, String) = if recovery {
        let name = bounded(args, "name", 4096)?;
        db.query_row(
            "SELECT id,password_hash FROM accounts WHERE name=?1 AND disabled=0",
            [name],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(Fault::authentication)?
    } else {
        db.query_row(
            "SELECT id,password_hash FROM accounts WHERE id=?1 AND disabled=0",
            [actor],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .ok_or_else(Fault::authentication)?
    };
    let link = metadata(db, &link_key(&id))?;
    if operation == "account.google.begin" {
        if recovery {
            if args["current_password"] != "" || read_link(db, &id)?.is_none() {
                return Err(Fault::authentication());
            }
        } else {
            if args["purpose"] != "link"
                || args["name"].as_str()
                    != Some(
                        db.query_row("SELECT name FROM accounts WHERE id=?1", [&id], |r| {
                            r.get::<_, String>(0)
                        })?
                        .as_str(),
                    )
            {
                return Err(invalid());
            }
            current_password(db, &id, args)?;
            if link.is_some() {
                return Err(Fault::new(
                    "CONFLICT",
                    6,
                    "Unlink the current Google account before linking another",
                ));
            }
        }
        let nonce = accounts::secret();
        let challenge = Challenge {
            version: 1,
            nonce: nonce.clone(),
            purpose: if recovery { "recover" } else { "link" }.into(),
            created: Utc::now().timestamp(),
            password_revision: accounts::digest(&hash),
            link_revision: accounts::digest(link.as_deref().unwrap_or("")),
        };
        save(db, &challenge_key(&id), &challenge)?;
        return Ok(json!({"nonce":nonce,"client_id":google_client_id()}));
    }
    let raw = metadata(db, &challenge_key(&id))?.ok_or_else(Fault::authentication)?;
    let challenge: Challenge = serde_json::from_str(&raw).map_err(|_| Fault::internal())?;
    let now = Utc::now().timestamp();
    let nonce = bounded(args, "challenge", 64)?;
    if challenge.version != 1
        || challenge.nonce != nonce
        || nonce.len() != 64
        || now < challenge.created
        || now - challenge.created >= 300
        || challenge.purpose != if recovery { "recover" } else { "link" }
        || challenge.password_revision != accounts::digest(&hash)
        || challenge.link_revision != accounts::digest(link.as_deref().unwrap_or(""))
    {
        return Err(Fault::authentication());
    }
    let claims = verify(bounded(args, "id_token", 16384)?, nonce)?;
    if Utc::now().timestamp() - challenge.created >= 300 {
        return Err(Fault::authentication());
    }
    if recovery {
        let link = read_link(db, &id)?.ok_or_else(Fault::authentication)?;
        if link.subject_hash != subject_hash(&claims.sub) {
            return Err(Fault::authentication());
        }
        let password = bounded(args, "password", 4096)?;
        let next_hash = accounts::hash_password(password)?;
        let admin: bool =
            db.query_row("SELECT is_admin FROM accounts WHERE id=?1", [&id], |r| {
                r.get(0)
            })?;
        let next_code = if admin {
            Some(accounts::secret())
        } else {
            None
        };
        db.execute(
            "UPDATE accounts SET password_hash=?1 WHERE id=?2",
            params![next_hash, id],
        )?;
        if let Some(code) = &next_code {
            db.execute(
                "UPDATE accounts SET recovery_hash=?1 WHERE id=?2",
                params![accounts::digest(code), id],
            )?;
        }
        db.execute("DELETE FROM human_sessions WHERE account_id=?1", [&id])?;
        db.execute(
            "DELETE FROM app_metadata WHERE key=?1",
            [challenge_key(&id)],
        )?;
        return Ok(
            json!({"id":id,"token":accounts::new_session(db,&id)?,"recovery_code":next_code}),
        );
    }
    if operation != "account.google.link" || link.is_some() {
        return Err(invalid());
    }
    let email = claims
        .email
        .filter(|email| claims.email_verified == Some(true) && email.len() <= 320);
    save(
        db,
        &link_key(&id),
        &Link {
            version: 1,
            subject_hash: subject_hash(&claims.sub),
            email: email.clone(),
        },
    )?;
    db.execute(
        "DELETE FROM app_metadata WHERE key=?1",
        [challenge_key(&id)],
    )?;
    Ok(json!({"linked":true,"email":email}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};

    fn claims(nonce: &str) -> Value {
        let now = Utc::now().timestamp();
        json!({"iss":"https://accounts.google.com","aud":google_client_id(),"sub":"test-google-user","nonce":nonce,"iat":now,"exp":now+600,"email":"fixture@example.test","email_verified":true})
    }
    fn signed(value: &Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("fixture".into());
        encode(
            &header,
            value,
            &EncodingKey::from_rsa_pem(include_bytes!("../tests/fixtures/google-test-private.pem"))
                .unwrap(),
        )
        .unwrap()
    }
    fn keys() -> JwkSet {
        serde_json::from_str(include_str!("../tests/fixtures/google-test-jwks.json")).unwrap()
    }
    #[test]
    fn signed_identity_checks_signature_issuer_audience_nonce_expiry_and_presenter() {
        let value = claims("nonce");
        assert_eq!(
            verify_with_keys(
                &signed(&value),
                "nonce",
                &keys(),
                google_client_id(),
                Utc::now().timestamp()
            )
            .unwrap()
            .sub,
            "test-google-user"
        );
        for (field, replacement) in [
            ("iss", json!("https://attacker.example")),
            ("aud", json!("other-client")),
            ("nonce", json!("other-nonce")),
            ("exp", json!(0)),
            ("iat", json!(Utc::now().timestamp() - 400)),
            ("azp", json!("other-client")),
            ("sub", json!("")),
        ] {
            let mut changed = value.clone();
            changed[field] = replacement;
            assert!(
                verify_with_keys(
                    &signed(&changed),
                    "nonce",
                    &keys(),
                    google_client_id(),
                    Utc::now().timestamp()
                )
                .is_err(),
                "{field}"
            );
        }
        let mut token = signed(&value).into_bytes();
        let index = token.len() - 10;
        token[index] = if token[index] == b'a' { b'b' } else { b'a' };
        assert!(
            verify_with_keys(
                std::str::from_utf8(&token).unwrap(),
                "nonce",
                &keys(),
                google_client_id(),
                Utc::now().timestamp()
            )
            .is_err()
        );
    }
    fn finish(
        db: &Connection,
        op: &str,
        args: &Value,
        actor: &str,
        subject: &str,
    ) -> Result<Value, Fault> {
        dispatch_verified(db, op, args, actor, |_, nonce| {
            let mut value = claims(nonce);
            value["sub"] = json!(subject);
            verify_with_keys(
                &signed(&value),
                nonce,
                &keys(),
                google_client_id(),
                Utc::now().timestamp(),
            )
        })
    }
    #[test]
    fn link_recovery_and_unlink_bind_to_account_consume_challenges_and_revoke_sessions() {
        let mut db = Connection::open_in_memory().unwrap();
        crate::store::migrate(&mut db).unwrap();
        let admin = accounts::setup(&db, &json!({"name":"Admin","password":"original"})).unwrap();
        let id = admin["id"].as_str().unwrap();
        let begin = dispatch(
            &db,
            "account.google.begin",
            &json!({"name":"Admin","purpose":"link","current_password":"original"}),
            id,
        )
        .unwrap();
        let args = json!({"challenge":begin["nonce"],"id_token":"fixture"});
        finish(&db, "account.google.link", &args, id, "identity-A").unwrap();
        assert!(finish(&db, "account.google.link", &args, id, "identity-A").is_err());
        assert_eq!(public_link(&db, id).unwrap()["linked"], true);
        let begin = dispatch(
            &db,
            "account.google.begin",
            &json!({"name":"Admin","purpose":"recover","current_password":""}),
            "",
        )
        .unwrap();
        let args = json!({"name":"Admin","challenge":begin["nonce"],"id_token":"fixture","password":"replacement"});
        assert!(finish(&db, "account.google.recover", &args, "", "identity-B").is_err());
        assert!(accounts::authenticate(&db, admin["token"].as_str()).is_ok());
        let grant = finish(&db, "account.google.recover", &args, "", "identity-A").unwrap();
        assert!(accounts::authenticate(&db, admin["token"].as_str()).is_err());
        assert!(accounts::authenticate(&db, grant["token"].as_str()).is_ok());
        assert!(finish(&db, "account.google.recover", &args, "", "identity-A").is_err());
        assert!(
            dispatch(
                &db,
                "account.google.unlink",
                &json!({"current_password":"original"}),
                id
            )
            .is_err()
        );
        dispatch(
            &db,
            "account.google.unlink",
            &json!({"current_password":"replacement"}),
            id,
        )
        .unwrap();
        assert!(
            dispatch(
                &db,
                "account.google.begin",
                &json!({"name":"Admin","purpose":"recover","current_password":""}),
                ""
            )
            .is_err()
        );
    }
    #[test]
    fn expired_or_changed_account_challenges_cannot_link() {
        let mut db = Connection::open_in_memory().unwrap();
        crate::store::migrate(&mut db).unwrap();
        let admin = accounts::setup(&db, &json!({"name":"Admin","password":""})).unwrap();
        let id = admin["id"].as_str().unwrap();
        let begin = dispatch(
            &db,
            "account.google.begin",
            &json!({"name":"Admin","purpose":"link","current_password":""}),
            id,
        )
        .unwrap();
        let args = json!({"challenge":begin["nonce"],"id_token":"fixture"});
        let mut challenge: Challenge =
            serde_json::from_str(&metadata(&db, &challenge_key(id)).unwrap().unwrap()).unwrap();
        challenge.created -= 300;
        save(&db, &challenge_key(id), &challenge).unwrap();
        assert!(finish(&db, "account.google.link", &args, id, "identity").is_err());
        challenge.created = Utc::now().timestamp();
        save(&db, &challenge_key(id), &challenge).unwrap();
        db.execute(
            "UPDATE accounts SET password_hash='changed' WHERE id=?1",
            [id],
        )
        .unwrap();
        assert!(finish(&db, "account.google.link", &args, id, "identity").is_err());
        assert_eq!(public_link(&db, id).unwrap()["linked"], false);
    }
}
