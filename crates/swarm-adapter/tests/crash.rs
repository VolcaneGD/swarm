#![cfg(windows)]
use serde_json::{Value, json};
use std::{path::Path, process::Command};
use swarm_adapter::CredentialBroker;
use swarm_core::{CredentialCandidate, Service, protocol::Request};

fn request(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}
fn token(candidate: &CredentialCandidate) -> &str {
    match candidate {
        CredentialCandidate::Human { token, .. } => token,
        _ => panic!("expected human credential"),
    }
}

// Only launched by the parent crash test, in its dedicated temporary directory.
#[test]
#[ignore = "subprocess fixture; exercised by process_exit_recovers_both_transaction_boundaries"]
fn crash_child() {
    let root = std::env::var_os("SWARM_CRASH_TEST_ROOT").expect("fixture root");
    let root = Path::new(&root);
    let phase = std::env::var("SWARM_CRASH_TEST_PHASE").expect("fixture phase");
    let mut service = Service::open(&root.join("data")).unwrap();
    let mut broker = CredentialBroker::open(&root.join("credentials")).unwrap();
    let old = broker
        .credential(&mut service, "human-main")
        .unwrap()
        .unwrap();
    let change = request(
        "account.password",
        Some(token(&old)),
        json!({"password":"new-password","current_password":"old-password"}),
    );
    if phase == "precommit" {
        service
            .request_with_credential_sink(change, |grant| {
                let candidate = CredentialCandidate::from_grant("account.password", grant)?;
                broker.stage_candidate("human-main", candidate)?;
                // Bypass destructors: SQLite must recover an uncommitted transaction.
                std::process::exit(71);
            })
            .unwrap();
    } else if phase == "postcommit" {
        let public = broker.issue(&mut service, "human-main", change).unwrap();
        assert!(public.get("token").is_none());
        // Exit before journal pruning or another lookup.
        std::process::exit(72);
    } else {
        panic!("invalid fixture phase");
    }
}

#[test]
fn process_exit_recovers_both_transaction_boundaries() {
    for (phase, exit_code, expected_password) in [
        ("precommit", 71, "old-password"),
        ("postcommit", 72, "new-password"),
    ] {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("data");
        let vault = root.path().join("credentials");
        let mut service = Service::open(&data).unwrap();
        let mut broker = CredentialBroker::open(&vault).unwrap();
        broker
            .issue(
                &mut service,
                "human-main",
                request(
                    "account.setup",
                    None,
                    json!({"name":"Admin","password":"old-password"}),
                ),
            )
            .unwrap();
        let old = broker
            .credential(&mut service, "human-main")
            .unwrap()
            .unwrap();
        drop(broker);
        drop(service);
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", "crash_child", "--nocapture"])
            .env("SWARM_CRASH_TEST_ROOT", root.path())
            .env("SWARM_CRASH_TEST_PHASE", phase)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(exit_code),
            "child failed before crash boundary: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut service = Service::open(&data).unwrap();
        let mut broker = CredentialBroker::open(&vault).unwrap();
        let recovered = broker
            .credential(&mut service, "human-main")
            .unwrap()
            .unwrap();
        assert!(
            service
                .request(request(
                    "account.status",
                    Some(token(&recovered)),
                    json!({})
                ))
                .is_ok()
        );
        assert_eq!(token(&recovered) == token(&old), phase == "precommit");
        assert!(
            service
                .request(request(
                    "account.login",
                    None,
                    json!({"name":"Admin","password":expected_password})
                ))
                .is_ok()
        );
        let wrong = if phase == "precommit" {
            "new-password"
        } else {
            "old-password"
        };
        assert!(
            service
                .request(request(
                    "account.login",
                    None,
                    json!({"name":"Admin","password":wrong})
                ))
                .is_err()
        );
    }
}

#[test]
fn overlarge_staged_candidate_leaves_the_saved_login_unchanged() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(&root.path().join("data")).unwrap();
    let mut broker = CredentialBroker::open(&root.path().join("credentials")).unwrap();
    broker
        .issue(
            &mut service,
            "human-main",
            request(
                "account.setup",
                None,
                json!({"name":"Admin","password":"old-password"}),
            ),
        )
        .unwrap();
    let old = broker
        .credential(&mut service, "human-main")
        .unwrap()
        .unwrap();
    let mut oversized = old.clone();
    if let CredentialCandidate::Human { token, .. } = &mut oversized {
        *token = "private-marker".repeat(64 * 1024);
    }
    let error = broker.stage_candidate("human-main", oversized).unwrap_err();
    assert_eq!(error.exit_code, 8);
    assert!(!format!("{error:?}").contains("private-marker"));
    assert_eq!(
        broker.credential(&mut service, "human-main").unwrap(),
        Some(old)
    );
}
