use serde_json::{Value, json};
use swarm_core::{
    Service,
    protocol::{Fault, Request},
};

fn request(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}

fn key(character: char) -> String {
    std::iter::repeat_n(character, 64).collect()
}

fn failing_sink(_: &Value) -> Result<(), Fault> {
    Err(Fault::new(
        "CREDENTIAL_STORAGE_FAILED",
        8,
        "Private credential storage failed",
    ))
}

#[test]
fn failed_bootstrap_sink_rolls_back_grant_and_receipt() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let receipt_key = key('a');
    let setup = request(
        "account.setup",
        None,
        json!({"name":"Admin","password":"safe-password"}),
    );

    let error = service
        .request_with_credential_receipt(setup.clone(), &receipt_key, failing_sink)
        .unwrap_err();

    assert_eq!(error.code, "CREDENTIAL_STORAGE_FAILED");
    assert_eq!(service.credential_receipt(&receipt_key).unwrap(), None);
    assert_eq!(
        service
            .request(request(
                "account.login",
                None,
                json!({"name":"Admin","password":"safe-password"})
            ))
            .unwrap_err()
            .exit_code,
        3
    );
    let mut grant = Value::Null;
    service
        .request_with_credential_receipt(setup, &receipt_key, |value| {
            grant = value.clone();
            Ok(())
        })
        .unwrap();
    assert!(grant["token"].is_string());
    assert!(service.credential_receipt(&receipt_key).unwrap().is_some());
}

#[test]
fn receipt_insert_failure_rolls_back_grant_and_allows_retry() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let receipt_key = key('f');
    let setup = request(
        "account.setup",
        None,
        json!({"name":"Admin","password":"safe-password"}),
    );
    let db = rusqlite::Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_credential_receipt BEFORE INSERT ON app_metadata
         WHEN NEW.key LIKE 'credential-receipt:%'
         BEGIN SELECT RAISE(ABORT, 'receipt insert rejected'); END;",
    )
    .unwrap();
    let mut sink_calls = 0;

    assert!(
        service
            .request_with_credential_receipt(setup.clone(), &receipt_key, |_| {
                sink_calls += 1;
                Ok(())
            })
            .is_err()
    );
    assert_eq!(sink_calls, 1);
    assert_eq!(service.credential_receipt(&receipt_key).unwrap(), None);
    assert_eq!(
        service
            .request(request(
                "account.login",
                None,
                json!({"name":"Admin","password":"safe-password"})
            ))
            .unwrap_err()
            .exit_code,
        3
    );

    db.execute_batch("DROP TRIGGER reject_credential_receipt;")
        .unwrap();
    service
        .request_with_credential_receipt(setup, &receipt_key, |_| Ok(()))
        .unwrap();
    assert!(service.credential_receipt(&receipt_key).unwrap().is_some());
}

#[test]
fn successful_receipt_contains_only_public_metadata_and_survives_reopen() {
    let root = tempfile::tempdir().unwrap();
    let receipt_key = key('b');
    let mut service = Service::open(root.path()).unwrap();
    let mut grant = Value::Null;
    let public = service
        .request_with_credential_receipt(
            request(
                "account.setup",
                None,
                json!({"name":"Admin","password":"safe-password"}),
            ),
            &receipt_key,
            |value| {
                grant = value.clone();
                Ok(())
            },
        )
        .unwrap();

    assert!(grant["token"].is_string());
    assert!(grant["recovery_code"].is_string());
    assert_eq!(public["name"], "Admin");
    assert!(public.get("token").is_none());
    assert!(public.get("recovery_code").is_none());
    let receipt = service.credential_receipt(&receipt_key).unwrap().unwrap();
    assert_eq!(receipt, public);
    let serialized = receipt.to_string();
    assert!(!serialized.contains(grant["token"].as_str().unwrap()));
    assert!(!serialized.contains(grant["recovery_code"].as_str().unwrap()));
    drop(service);

    let reopened = Service::open(root.path()).unwrap();
    assert_eq!(
        reopened.credential_receipt(&receipt_key).unwrap(),
        Some(public)
    );
}

#[test]
fn receipt_conflict_prevents_reissuing_the_same_intent() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let receipt_key = key('c');
    let setup = request(
        "account.setup",
        None,
        json!({"name":"Admin","password":"safe-password"}),
    );
    let mut calls = 0;
    service
        .request_with_credential_receipt(setup.clone(), &receipt_key, |_| {
            calls += 1;
            Ok(())
        })
        .unwrap();

    let error = service
        .request_with_credential_receipt(setup, &receipt_key, |_| {
            calls += 1;
            Ok(())
        })
        .unwrap_err();

    assert_eq!(error.exit_code, 6);
    assert_eq!(calls, 1);
    assert!(service.credential_receipt(&receipt_key).unwrap().is_some());
}

#[test]
fn identity_receipts_cover_new_and_resumed_sessions_without_duplicate_grants() {
    let root = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = service
        .request(request(
            "account.setup",
            None,
            json!({"name":"Admin","password":"safe-password"}),
        ))
        .unwrap();
    let project = service
        .request(request(
            "project.register",
            admin["token"].as_str(),
            json!({"name":"Project","root":project_root.path()}),
        ))
        .unwrap();
    let start_key = key('d');
    let mut first_grant = Value::Null;
    let first_public = service
        .request_with_credential_receipt(
            request("identity.start", None, json!({"project_id":project["id"]})),
            &start_key,
            |value| {
                first_grant = value.clone();
                Ok(())
            },
        )
        .unwrap();
    assert!(first_grant["session_token"].is_string());
    assert!(first_grant["resume_secret"].is_string());
    assert!(first_public.get("session_token").is_none());
    assert!(first_public.get("resume_secret").is_none());
    let first_receipt = service.credential_receipt(&start_key).unwrap().unwrap();
    assert_eq!(first_receipt, first_public);

    let resume_key = key('e');
    let mut resumed_grant = Value::Null;
    let resumed_public = service
        .request_with_credential_receipt(
            request(
                "identity.resume",
                None,
                json!({
                    "actor_id":first_grant["identity"]["id"],
                    "project_id":project["id"],
                    "resume_secret":first_grant["resume_secret"]
                }),
            ),
            &resume_key,
            |value| {
                resumed_grant = value.clone();
                Ok(())
            },
        )
        .unwrap();
    assert!(resumed_grant["session_token"].is_string());
    assert!(resumed_public.get("session_token").is_none());
    assert!(resumed_public.get("resume_secret").is_none());
    assert_eq!(
        service.credential_receipt(&resume_key).unwrap(),
        Some(resumed_public)
    );
    let mut replay_sink_called = false;
    let replay_error = service
        .request_with_credential_receipt(
            request(
                "identity.resume",
                None,
                json!({
                    "actor_id":first_grant["identity"]["id"],
                    "project_id":project["id"],
                    "resume_secret":first_grant["resume_secret"]
                }),
            ),
            &resume_key,
            |_| {
                replay_sink_called = true;
                Ok(())
            },
        )
        .unwrap_err();
    assert_eq!(replay_error.exit_code, 6);
    assert!(!replay_sink_called);
    assert!(
        service
            .request(request(
                "identity.status",
                resumed_grant["session_token"].as_str(),
                json!({})
            ))
            .is_ok()
    );
}

#[test]
fn invalid_receipt_keys_have_no_effects() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut calls = 0;
    let error = service
        .request_with_credential_receipt(
            request(
                "account.setup",
                None,
                json!({"name":"Admin","password":"safe-password"}),
            ),
            "too-short",
            |_| {
                calls += 1;
                Ok(())
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "INVALID_INPUT");
    assert_eq!(error.exit_code, 2);
    assert_eq!(calls, 0);
    assert_eq!(
        service.credential_receipt("too-short").unwrap_err().code,
        "INVALID_INPUT"
    );
    assert_eq!(
        service
            .request(request("account.status", None, json!({})))
            .unwrap_err()
            .exit_code,
        3
    );
    for invalid in [
        "g".repeat(64),
        "a".repeat(63),
        format!("{}é", "a".repeat(63)),
    ] {
        assert_eq!(
            service.credential_receipt(&invalid).unwrap_err().exit_code,
            2
        );
    }
}
