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
fn failure(_: &Value) -> Result<(), Fault> {
    Err(Fault::new(
        "CREDENTIAL_STORAGE_FAILED",
        8,
        "Private credential storage failed",
    ))
}

#[test]
fn failed_bootstrap_persistence_rolls_back_account_and_allows_retry() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let setup = request(
        "account.setup",
        None,
        json!({"name":"Admin","password":"safe-password"}),
    );
    assert_eq!(
        service
            .request_with_credential_sink(setup.clone(), failure)
            .unwrap_err()
            .code,
        "CREDENTIAL_STORAGE_FAILED"
    );
    let mut private = Value::Null;
    let public = service
        .request_with_credential_sink(setup, |grant| {
            private = grant.clone();
            Ok(())
        })
        .unwrap();
    assert!(private["token"].is_string());
    assert!(private["recovery_code"].is_string());
    assert!(public.get("token").is_none());
    assert!(public.get("recovery_code").is_none());
    assert_eq!(public["name"], "Admin");
    assert!(
        service
            .request(request(
                "account.status",
                private["token"].as_str(),
                json!({})
            ))
            .is_ok()
    );
}

#[test]
fn failed_password_persistence_preserves_old_login_and_password() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let setup = service
        .request(request(
            "account.setup",
            None,
            json!({"name":"Admin","password":"old-password"}),
        ))
        .unwrap();
    let token = setup["token"].as_str().unwrap();
    assert!(
        service
            .request_with_credential_sink(
                request(
                    "account.password",
                    Some(token),
                    json!({"password":"new-password","current_password":"old-password"})
                ),
                failure
            )
            .is_err()
    );
    assert!(
        service
            .request(request("account.status", Some(token), json!({})))
            .is_ok()
    );
    assert!(
        service
            .request(request(
                "account.login",
                None,
                json!({"name":"Admin","password":"old-password"})
            ))
            .is_ok()
    );
    assert_eq!(
        service
            .request(request(
                "account.login",
                None,
                json!({"name":"Admin","password":"new-password"})
            ))
            .unwrap_err()
            .exit_code,
        3
    );
}

#[test]
fn failed_resume_persistence_keeps_old_session_and_resume_secret_valid() {
    let root = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = service
        .request(request(
            "account.setup",
            None,
            json!({"name":"Admin","password":"password"}),
        ))
        .unwrap();
    let project = service
        .request(request(
            "project.register",
            admin["token"].as_str(),
            json!({"name":"Project","root":project_root.path()}),
        ))
        .unwrap();
    let old = service
        .request(request(
            "identity.start",
            None,
            json!({"project_id":project["id"]}),
        ))
        .unwrap();
    let resume = request(
        "identity.resume",
        None,
        json!({"actor_id":old["identity"]["id"],"project_id":project["id"],"resume_secret":old["resume_secret"]}),
    );
    assert!(
        service
            .request_with_credential_sink(resume.clone(), failure)
            .is_err()
    );
    assert!(
        service
            .request(request(
                "identity.status",
                old["session_token"].as_str(),
                json!({})
            ))
            .is_ok()
    );
    let mut private = Value::Null;
    let public = service
        .request_with_credential_sink(resume, |grant| {
            private = grant.clone();
            Ok(())
        })
        .unwrap();
    assert_eq!(public["identity"]["id"], old["identity"]["id"]);
    assert!(public.get("session_token").is_none());
    assert!(public.get("resume_secret").is_none());
    assert!(
        service
            .request(request(
                "identity.status",
                old["session_token"].as_str(),
                json!({})
            ))
            .is_err()
    );
    drop(service);
    let mut reopened = Service::open(root.path()).unwrap();
    assert!(
        reopened
            .request(request(
                "identity.status",
                private["session_token"].as_str(),
                json!({})
            ))
            .is_ok()
    );
}

#[test]
fn normal_requests_cannot_use_the_private_issuance_path() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut invoked = false;
    let error = service
        .request_with_credential_sink(request("board.list", None, json!({})), |_| {
            invoked = true;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.exit_code, 2);
    assert!(!invoked);
}
