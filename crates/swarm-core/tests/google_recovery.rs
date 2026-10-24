#![cfg(feature = "google-recovery")]
use serde_json::{Value, json};
use swarm_core::{
    Service,
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

#[test]
fn google_link_requires_password_and_recovery_cannot_register_a_new_identity() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"initial-password"}),
    )
    .unwrap();
    let token = admin["token"].as_str().unwrap();
    assert!(
        call(
            &mut service,
            "account.google.begin",
            Some(token),
            json!({"name":"Admin","purpose":"link","current_password":"wrong"})
        )
        .is_err()
    );
    let challenge = call(
        &mut service,
        "account.google.begin",
        Some(token),
        json!({"name":"Admin","purpose":"link","current_password":"initial-password"}),
    )
    .unwrap();
    assert_eq!(challenge["nonce"].as_str().unwrap().len(), 64);
    assert!(
        call(
            &mut service,
            "account.google.begin",
            None,
            json!({"name":"Admin","purpose":"recover","current_password":""})
        )
        .is_err()
    );
    assert!(
        call(
            &mut service,
            "account.google.link",
            Some(token),
            json!({"challenge":challenge["nonce"],"id_token":"forged"})
        )
        .is_err()
    );
    assert_eq!(
        call(&mut service, "account.status", Some(token), json!({})).unwrap()["google_linked"],
        false
    );
    assert!(
        call(
            &mut service,
            "account.google.unlink",
            Some(token),
            json!({"current_password":"wrong"})
        )
        .is_err()
    );
}
