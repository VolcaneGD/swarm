#![cfg(not(feature = "google-recovery"))]
use serde_json::{Value, json};
use swarm_core::{Service, protocol::Request};

fn request(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}

#[test]
fn beta_rejects_google_before_setup_arguments_or_network() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    for operation in [
        "account.google.begin",
        "account.google.link",
        "account.google.unlink",
        "account.google.recover",
    ] {
        let error = service
            .request(request(operation, None, json!({})))
            .expect_err("Google must be unavailable in the first beta");
        assert_eq!(error.code, "FEATURE_UNAVAILABLE");
        assert_eq!(error.exit_code, 2);
    }
    assert!(service.setup_required().unwrap());
}

#[test]
fn beta_google_denial_preserves_password_and_recovery_code_flows() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let setup = service
        .request(request(
            "account.setup",
            None,
            json!({"name":"Beta Admin","password":"fixture-initial-password"}),
        ))
        .unwrap();
    let token = setup["token"].as_str().unwrap();
    let recovery = setup["recovery_code"].as_str().unwrap().to_owned();
    let before = service
        .request(request("account.status", Some(token), json!({})))
        .unwrap();
    for (operation, args) in [
        (
            "account.google.begin",
            json!({"name":"Beta Admin","purpose":"link","current_password":"fixture-initial-password"}),
        ),
        (
            "account.google.link",
            json!({"challenge":"a".repeat(64),"id_token":"synthetic-never-sent"}),
        ),
        (
            "account.google.unlink",
            json!({"current_password":"fixture-initial-password"}),
        ),
        (
            "account.google.recover",
            json!({"name":"Beta Admin","password":"must-not-change","challenge":"a".repeat(64),"id_token":"synthetic-never-sent"}),
        ),
    ] {
        let error = service
            .request(request(operation, Some(token), args))
            .expect_err("No Google operation may execute in the beta");
        assert_eq!(error.code, "FEATURE_UNAVAILABLE");
        assert!(!error.message.contains("synthetic-never-sent"));
        assert_eq!(
            service
                .request(request("account.status", Some(token), json!({})))
                .unwrap(),
            before
        );
    }
    let login = service
        .request(request(
            "account.login",
            None,
            json!({"name":"Beta Admin","password":"fixture-initial-password"}),
        ))
        .unwrap();
    assert_eq!(login["id"], setup["id"]);
    let recovered = service.request(request("account.recover",None,json!({"name":"Beta Admin","password":"fixture-recovered-password","recovery_code":recovery}))).unwrap();
    assert_eq!(recovered["id"], setup["id"]);
    let login = service
        .request(request(
            "account.login",
            None,
            json!({"name":"Beta Admin","password":"fixture-recovered-password"}),
        ))
        .unwrap();
    assert_eq!(login["id"], setup["id"]);
}
