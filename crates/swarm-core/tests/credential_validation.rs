use serde_json::{Value, json};
use swarm_core::{
    CredentialCandidate, CredentialValidity, Service,
    protocol::{Fault, Request},
};

fn call(
    service: &mut Service,
    operation: &str,
    auth: Option<&str>,
    args: Value,
) -> Result<Value, Fault> {
    service.request(Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    })
}

fn setup_admin(service: &mut Service) -> Value {
    call(
        service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"admin-password"}),
    )
    .unwrap()
}

fn private_grant(service: &mut Service, operation: &str, args: Value) -> Value {
    let mut grant = None;
    service
        .request_with_credential_sink(
            Request {
                operation: operation.into(),
                auth: None,
                args,
                context: json!({}),
                request_id: None,
            },
            |value| {
                grant = Some(value.clone());
                Ok(())
            },
        )
        .unwrap();
    grant.unwrap()
}

fn ai_fixture() -> (
    tempfile::TempDir,
    tempfile::TempDir,
    Service,
    Value,
    Value,
    Value,
) {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = setup_admin(&mut service);
    let project_dir = tempfile::tempdir().unwrap();
    let project = call(
        &mut service,
        "project.register",
        Some(admin["token"].as_str().unwrap()),
        json!({"name":"Project","root":project_dir.path().to_string_lossy()}),
    )
    .unwrap();
    let grant = private_grant(
        &mut service,
        "identity.start",
        json!({"project_id":project["id"]}),
    );
    (root, project_dir, service, admin, project, grant)
}

#[test]
fn private_grant_parsing_is_typed_bounded_and_redacted() {
    let grant = json!({"id":"human-id","token":"secret-token","recovery_code":"secret-recovery"});
    let candidate = CredentialCandidate::from_grant("account.setup", &grant).unwrap();
    let debug = format!("{candidate:?}");
    assert!(!debug.contains("secret-token"));
    assert!(!debug.contains("secret-recovery"));
    assert!(
        CredentialCandidate::from_grant("account.setup", &json!({"id":"id","token":4})).is_err()
    );
    assert!(
        CredentialCandidate::from_grant("identity.start", &json!({"identity":{"id":"id"}}))
            .is_err()
    );
    assert!(
        CredentialCandidate::from_grant(
            "account.setup",
            &json!({"id":"id","token":"x".repeat(4097)})
        )
        .is_err()
    );
    assert!(CredentialCandidate::from_grant("board.list", &grant).is_err());
    assert_eq!(
        CredentialCandidate::from_grant(
            "account.password",
            &json!({"account":{"id":"changed-id"},"token":"replacement-token"})
        )
        .unwrap(),
        CredentialCandidate::Human {
            id: "changed-id".into(),
            token: "replacement-token".into(),
            recovery_code: None,
        }
    );
}

#[test]
fn human_session_and_recovery_are_bound_to_the_account_and_rotation_state() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let grant = private_grant(
        &mut service,
        "account.setup",
        json!({"name":"Admin","password":"admin-password"}),
    );
    let candidate = CredentialCandidate::from_grant("account.setup", &grant).unwrap();
    assert_eq!(
        service.validate_credential_candidate(&candidate).unwrap(),
        CredentialValidity {
            session: true,
            resume: false,
            recovery: true
        }
    );

    let wrong_subject = CredentialCandidate::from_grant(
        "account.setup",
        &json!({
            "id":"different-account", "token":grant["token"], "recovery_code":grant["recovery_code"]
        }),
    )
    .unwrap();
    assert_eq!(
        service
            .validate_credential_candidate(&wrong_subject)
            .unwrap(),
        CredentialValidity::default()
    );
    let other_kind = CredentialCandidate::Ai {
        id: grant["id"].as_str().unwrap().to_owned(),
        project_id: "project".into(),
        session_id: "session".into(),
        session_token: grant["token"].as_str().unwrap().to_owned(),
        resume_secret: "resume".into(),
    };
    assert_eq!(
        service.validate_credential_candidate(&other_kind).unwrap(),
        CredentialValidity::default()
    );

    call(
        &mut service,
        "account.recover",
        None,
        json!({
            "name":"Admin", "recovery_code":grant["recovery_code"], "password":"changed-password"
        }),
    )
    .unwrap();
    assert_eq!(
        service.validate_credential_candidate(&candidate).unwrap(),
        CredentialValidity::default()
    );
}

#[test]
fn disabled_human_account_invalidates_session_and_candidate_recovery() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = setup_admin(&mut service);
    let created = call(
        &mut service,
        "account.create",
        Some(admin["token"].as_str().unwrap()),
        json!({
            "name":"Member", "password":"member-password"
        }),
    )
    .unwrap();
    let grant = private_grant(
        &mut service,
        "account.login",
        json!({"name":"Member","password":"member-password"}),
    );
    let candidate = CredentialCandidate::from_grant("account.login", &grant).unwrap();
    assert!(
        service
            .validate_credential_candidate(&candidate)
            .unwrap()
            .session
    );
    call(
        &mut service,
        "account.disable",
        Some(admin["token"].as_str().unwrap()),
        json!({
            "id":created["id"], "disabled":true
        }),
    )
    .unwrap();
    assert_eq!(
        service.validate_credential_candidate(&candidate).unwrap(),
        CredentialValidity::default()
    );
}

#[test]
fn ai_release_model_supplement_resume_and_expiry_have_separate_validity() {
    let (root, _project_dir, mut service, _admin, project, grant) = ai_fixture();
    let candidate = CredentialCandidate::from_grant("identity.start", &grant).unwrap();
    assert_eq!(
        service.validate_credential_candidate(&candidate).unwrap(),
        CredentialValidity {
            session: true,
            resume: true,
            recovery: false
        }
    );

    call(
        &mut service,
        "identity.model",
        Some(grant["session_token"].as_str().unwrap()),
        json!({"model_name":"KnownModel"}),
    )
    .unwrap();
    assert_eq!(
        service.validate_credential_candidate(&candidate).unwrap(),
        CredentialValidity {
            session: true,
            resume: true,
            recovery: false
        }
    );

    call(
        &mut service,
        "identity.release",
        Some(grant["session_token"].as_str().unwrap()),
        json!({}),
    )
    .unwrap();
    assert_eq!(
        service.validate_credential_candidate(&candidate).unwrap(),
        CredentialValidity {
            session: false,
            resume: true,
            recovery: false
        }
    );

    let resumed = private_grant(
        &mut service,
        "identity.resume",
        json!({
            "actor_id":grant["identity"]["id"], "project_id":project["id"], "model_name":"KnownModel", "resume_secret":grant["resume_secret"]
        }),
    );
    assert_eq!(
        service.validate_credential_candidate(&candidate).unwrap(),
        CredentialValidity::default()
    );
    let resumed_candidate = CredentialCandidate::from_grant("identity.resume", &resumed).unwrap();
    assert_eq!(
        service
            .validate_credential_candidate(&resumed_candidate)
            .unwrap(),
        CredentialValidity {
            session: true,
            resume: true,
            recovery: false
        }
    );

    let db = rusqlite::Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    db.execute(
        "UPDATE ai_identities SET expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
        [grant["identity"]["id"].as_str().unwrap()],
    )
    .unwrap();
    assert_eq!(
        service
            .validate_credential_candidate(&resumed_candidate)
            .unwrap(),
        CredentialValidity::default()
    );
}

#[test]
fn validation_after_reopen_does_not_mutate_credentials_or_identity_state() {
    let (root, _project_dir, service, _admin, _project, grant) = ai_fixture();
    let candidate = CredentialCandidate::from_grant("identity.start", &grant).unwrap();
    drop(service);
    let db_path = root.path().join("swarm.sqlite3");
    let before = rusqlite::Connection::open(&db_path).unwrap();
    let before_rows: (String, String, String, String, String) = before.query_row(
        "SELECT i.resume_hash,s.token_hash,s.session_id,i.expires_at,i.model_name FROM ai_identities i JOIN ai_sessions s ON s.actor_id=i.id WHERE i.id=?1",
        [grant["identity"]["id"].as_str().unwrap()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    ).unwrap();
    drop(before);

    let mut reopened = Service::open(root.path()).unwrap();
    assert_eq!(
        reopened.validate_credential_candidate(&candidate).unwrap(),
        CredentialValidity {
            session: true,
            resume: true,
            recovery: false
        }
    );
    assert_eq!(
        reopened.validate_credential_candidate(&candidate).unwrap(),
        CredentialValidity {
            session: true,
            resume: true,
            recovery: false
        }
    );
    let after = rusqlite::Connection::open(&db_path).unwrap();
    let after_rows: (String, String, String, String, String) = after.query_row(
        "SELECT i.resume_hash,s.token_hash,s.session_id,i.expires_at,i.model_name FROM ai_identities i JOIN ai_sessions s ON s.actor_id=i.id WHERE i.id=?1",
        [grant["identity"]["id"].as_str().unwrap()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    ).unwrap();
    assert_eq!(before_rows, after_rows);
}
