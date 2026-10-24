#![cfg(windows)]

use serde_json::{Value, json};
use std::path::Path;
use swarm_adapter::CredentialBroker;
use swarm_core::{CredentialCandidate, Service, protocol::Request};

const HUMAN_SLOT: &str = "human-main";
const AI_SLOT: &str = "ai-project";

fn request(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}

struct Fixture {
    _root: tempfile::TempDir,
    vault: std::path::PathBuf,
    database: std::path::PathBuf,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let vault = root.path().join("vault");
    let database = root.path().join("database");
    Fixture {
        _root: root,
        vault,
        database,
    }
}

fn account_setup(service: &mut Service, broker: &mut CredentialBroker) -> Value {
    broker
        .issue(
            service,
            HUMAN_SLOT,
            request(
                "account.setup",
                None,
                json!({"name":"Admin","password":"admin-password"}),
            ),
        )
        .unwrap()
}

fn human_candidate(
    broker: &mut CredentialBroker,
    service: &mut Service,
    slot: &str,
) -> CredentialCandidate {
    broker.credential(service, slot).unwrap().unwrap()
}

fn ai_candidate(
    broker: &mut CredentialBroker,
    service: &mut Service,
    slot: &str,
) -> CredentialCandidate {
    broker.credential(service, slot).unwrap().unwrap()
}

fn human_token(candidate: &CredentialCandidate) -> &str {
    match candidate {
        CredentialCandidate::Human { token, .. } => token,
        CredentialCandidate::Ai { .. } => panic!("expected a human candidate"),
    }
}

fn human_fields(candidate: &CredentialCandidate) -> (&str, &str, Option<&str>) {
    match candidate {
        CredentialCandidate::Human {
            id,
            token,
            recovery_code,
        } => (id, token, recovery_code.as_deref()),
        CredentialCandidate::Ai { .. } => panic!("expected a human candidate"),
    }
}

fn ai_fields(candidate: &CredentialCandidate) -> (&str, &str, &str, &str, &str) {
    match candidate {
        CredentialCandidate::Ai {
            id,
            project_id,
            session_id,
            session_token,
            resume_secret,
        } => (id, project_id, session_id, session_token, resume_secret),
        CredentialCandidate::Human { .. } => panic!("expected an AI candidate"),
    }
}

fn registered_project(service: &mut Service, admin_token: &str, path: &Path) -> Value {
    service
        .request(request(
            "project.register",
            Some(admin_token),
            json!({"name":"Project","root":path}),
        ))
        .unwrap()
}

#[test]
fn setup_returns_only_public_metadata_and_private_candidate_is_retrievable() {
    let f = fixture();
    let mut service = Service::open(&f.database).unwrap();
    let mut broker = CredentialBroker::open(&f.vault).unwrap();
    let public = account_setup(&mut service, &mut broker);

    assert_eq!(public["name"], "Admin");
    assert!(public.get("token").is_none());
    assert!(public.get("recovery_code").is_none());
    let candidate = human_candidate(&mut broker, &mut service, HUMAN_SLOT);
    let (id, token, recovery_code) = human_fields(&candidate);
    assert_eq!(id, public["id"]);
    assert!(!token.is_empty());
    assert!(recovery_code.is_some_and(|code| !code.is_empty()));
}

#[test]
fn committed_ai_resume_reconciles_to_new_candidate_after_broker_and_service_restart() {
    let f = fixture();
    let mut service = Service::open(&f.database).unwrap();
    let mut broker = CredentialBroker::open(&f.vault).unwrap();
    let setup = account_setup(&mut service, &mut broker);
    let admin = human_candidate(&mut broker, &mut service, HUMAN_SLOT);
    let (_, admin_token, _) = human_fields(&admin);
    let project_root = tempfile::tempdir().unwrap();
    let project = registered_project(&mut service, admin_token, project_root.path());
    let initial_public = broker
        .issue(
            &mut service,
            AI_SLOT,
            request("identity.start", None, json!({"project_id":project["id"]})),
        )
        .unwrap();
    let initial = ai_candidate(&mut broker, &mut service, AI_SLOT);
    let (id, project_id, _, old_session, resume_secret) = ai_fields(&initial);
    let resumed_public = broker
        .issue(
            &mut service,
            AI_SLOT,
            request(
                "identity.resume",
                None,
                json!({
                    "actor_id":id,
                    "project_id":project_id,
                    "resume_secret":resume_secret,
                }),
            ),
        )
        .unwrap();
    assert_eq!(
        initial_public["identity"]["id"],
        resumed_public["identity"]["id"]
    );
    assert!(resumed_public.get("session_token").is_none());
    assert!(resumed_public.get("resume_secret").is_none());
    drop(broker);
    drop(service);

    let mut service = Service::open(&f.database).unwrap();
    let mut broker = CredentialBroker::open(&f.vault).unwrap();
    let candidate = ai_candidate(&mut broker, &mut service, AI_SLOT);
    let (_, _, _, new_session, _) = ai_fields(&candidate);
    assert_ne!(old_session, new_session);
    assert_eq!(
        service
            .request(request("identity.status", Some(old_session), json!({})))
            .unwrap_err()
            .exit_code,
        3,
    );
    assert!(
        service
            .request(request("identity.status", Some(new_session), json!({})))
            .is_ok()
    );
    assert_eq!(setup["name"], "Admin");
}

#[test]
fn precommit_ai_candidate_is_discarded_and_previous_valid_candidate_survives_restart() {
    let f = fixture();
    let mut service = Service::open(&f.database).unwrap();
    let mut broker = CredentialBroker::open(&f.vault).unwrap();
    let setup = account_setup(&mut service, &mut broker);
    let human = human_candidate(&mut broker, &mut service, HUMAN_SLOT);
    let (_, admin_token, _) = human_fields(&human);
    let project_root = tempfile::tempdir().unwrap();
    let project = registered_project(&mut service, admin_token, project_root.path());
    broker
        .issue(
            &mut service,
            AI_SLOT,
            request("identity.start", None, json!({"project_id":project["id"]})),
        )
        .unwrap();
    let original = ai_candidate(&mut broker, &mut service, AI_SLOT);
    let (id, project_id, _, original_token, _) = ai_fields(&original);
    broker
        .stage_candidate(
            AI_SLOT,
            CredentialCandidate::Ai {
                id: id.into(),
                project_id: project_id.into(),
                session_id: "uncommitted-session".into(),
                session_token: "uncommitted-token".into(),
                resume_secret: "uncommitted-resume".into(),
            },
        )
        .unwrap();
    drop(broker);
    drop(service);

    let mut service = Service::open(&f.database).unwrap();
    let mut broker = CredentialBroker::open(&f.vault).unwrap();
    let recovered = ai_candidate(&mut broker, &mut service, AI_SLOT);
    let (_, _, _, recovered_token, _) = ai_fields(&recovered);
    assert_eq!(original_token, recovered_token);
    assert!(
        service
            .request(request("identity.status", Some(recovered_token), json!({})))
            .is_ok()
    );
    assert_eq!(setup["name"], "Admin");
}

#[test]
fn precommit_human_candidate_with_recovery_code_does_not_replace_valid_login() {
    let f = fixture();
    let mut service = Service::open(&f.database).unwrap();
    let mut broker = CredentialBroker::open(&f.vault).unwrap();
    account_setup(&mut service, &mut broker);
    let original = human_candidate(&mut broker, &mut service, HUMAN_SLOT);
    let (id, old_token, recovery_code) = human_fields(&original);
    broker
        .stage_candidate(
            HUMAN_SLOT,
            CredentialCandidate::Human {
                id: id.into(),
                token: "uncommitted-token".into(),
                recovery_code: recovery_code.map(str::to_owned),
            },
        )
        .unwrap();
    drop(broker);
    drop(service);

    let mut service = Service::open(&f.database).unwrap();
    let mut broker = CredentialBroker::open(&f.vault).unwrap();
    let recovered = human_candidate(&mut broker, &mut service, HUMAN_SLOT);
    assert_eq!(human_token(&recovered), old_token);
    assert!(
        service
            .request(request("account.status", Some(old_token), json!({})))
            .is_ok()
    );
}

#[test]
fn human_login_keeps_the_persisted_recovery_code() {
    let f = fixture();
    let mut service = Service::open(&f.database).unwrap();
    let mut broker = CredentialBroker::open(&f.vault).unwrap();
    account_setup(&mut service, &mut broker);
    let first = human_candidate(&mut broker, &mut service, HUMAN_SLOT);
    let (id, _, recovery_code) = human_fields(&first);
    let recovery_code = recovery_code.unwrap().to_owned();

    broker
        .issue(
            &mut service,
            HUMAN_SLOT,
            request(
                "account.login",
                None,
                json!({"name":"Admin","password":"admin-password"}),
            ),
        )
        .unwrap();
    let logged_in = human_candidate(&mut broker, &mut service, HUMAN_SLOT);
    let (logged_in_id, token, recovered_code) = human_fields(&logged_in);
    assert_eq!(logged_in_id, id);
    assert_eq!(recovered_code, Some(recovery_code.as_str()));
    assert!(
        service
            .request(request("account.status", Some(token), json!({})))
            .is_ok()
    );
}

#[test]
fn wrong_account_cannot_commit_a_login_into_an_already_bound_slot() {
    let f = fixture();
    let mut service = Service::open(&f.database).unwrap();
    let mut broker = CredentialBroker::open(&f.vault).unwrap();
    let setup = account_setup(&mut service, &mut broker);
    let admin = human_candidate(&mut broker, &mut service, HUMAN_SLOT);
    let (_, admin_token, _) = human_fields(&admin);
    let created = service
        .request(request(
            "account.create",
            Some(admin_token),
            json!({"name":"Other","password":"other-password"}),
        ))
        .unwrap();
    let count = |id: &str| {
        rusqlite::Connection::open(f.database.join("swarm.sqlite3"))
            .unwrap()
            .query_row(
                "SELECT count(*) FROM human_sessions WHERE account_id=?1",
                [id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
    };
    assert_eq!(count(created["id"].as_str().unwrap()), 0);
    let error = broker
        .issue(
            &mut service,
            HUMAN_SLOT,
            request(
                "account.login",
                None,
                json!({"name":"Other","password":"other-password"}),
            ),
        )
        .unwrap_err();
    assert_eq!(error.code, "INVALID_CREDENTIAL_JOURNAL");
    assert_eq!(count(created["id"].as_str().unwrap()), 0);
    assert_eq!(setup["name"], "Admin");
}

#[test]
fn disabled_human_and_expired_ai_credentials_are_removed_during_recovery() {
    let f = fixture();
    let mut service = Service::open(&f.database).unwrap();
    let mut broker = CredentialBroker::open(&f.vault).unwrap();
    let setup = account_setup(&mut service, &mut broker);
    let admin = human_candidate(&mut broker, &mut service, HUMAN_SLOT);
    let (_, admin_token, _) = human_fields(&admin);
    let user = service
        .request(request(
            "account.create",
            Some(admin_token),
            json!({"name":"Temporary","password":"temporary-password"}),
        ))
        .unwrap();
    let human_slot = "human-temporary";
    broker
        .issue(
            &mut service,
            human_slot,
            request(
                "account.login",
                None,
                json!({"name":"Temporary","password":"temporary-password"}),
            ),
        )
        .unwrap();
    let human = human_candidate(&mut broker, &mut service, human_slot);
    let (human_id, _, _) = human_fields(&human);
    assert_eq!(human_id, user["id"]);
    let project_root = tempfile::tempdir().unwrap();
    let project = registered_project(&mut service, admin_token, project_root.path());
    broker
        .issue(
            &mut service,
            AI_SLOT,
            request("identity.start", None, json!({"project_id":project["id"]})),
        )
        .unwrap();
    let ai = ai_candidate(&mut broker, &mut service, AI_SLOT);
    let (ai_id, _, _, _, _) = ai_fields(&ai);

    service
        .request(request(
            "account.disable",
            Some(admin_token),
            json!({"id":human_id,"disabled":true}),
        ))
        .unwrap();
    let db = rusqlite::Connection::open(f.database.join("swarm.sqlite3")).unwrap();
    db.execute(
        "UPDATE ai_identities SET expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
        [ai_id],
    )
    .unwrap();
    drop(db);

    assert!(
        broker
            .credential(&mut service, human_slot)
            .unwrap()
            .is_none()
    );
    assert!(broker.credential(&mut service, AI_SLOT).unwrap().is_none());
    assert_eq!(setup["name"], "Admin");
}

#[test]
fn broker_lease_is_exclusive_and_released_when_dropped() {
    let f = fixture();
    let broker = CredentialBroker::open(&f.vault).unwrap();
    assert!(CredentialBroker::open(&f.vault).is_err());
    drop(broker);
    assert!(CredentialBroker::open(&f.vault).is_ok());
}

#[test]
fn corrupted_encrypted_journal_fails_closed_without_resetting_it() {
    let f = fixture();
    let mut service = Service::open(&f.database).unwrap();
    let mut broker = CredentialBroker::open(&f.vault).unwrap();
    account_setup(&mut service, &mut broker);
    drop(broker);
    drop(service);

    let journal_path = std::fs::read_dir(&f.vault)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|extension| extension == "bin"))
        .expect("encrypted journal file");
    let mut encrypted = std::fs::read(&journal_path).unwrap();
    let last = encrypted.last_mut().expect("nonempty ciphertext");
    *last ^= 0x40;
    std::fs::write(&journal_path, &encrypted).unwrap();

    let error = CredentialBroker::open(&f.vault).unwrap_err();
    assert!(matches!(
        error.code.as_str(),
        "INVALID_CREDENTIAL_JOURNAL" | "CREDENTIAL_STORAGE"
    ));
    assert_eq!(std::fs::read(&journal_path).unwrap(), encrypted);
}
