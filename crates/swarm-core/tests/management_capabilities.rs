use serde_json::{Value, json};
use swarm_core::{Service, protocol::Request};

fn call(service: &mut Service, token: Option<&str>, operation: &str, args: Value) -> Value {
    service
        .request(Request {
            operation: operation.into(),
            auth: token.map(str::to_owned),
            args,
            context: json!({}),
            request_id: None,
        })
        .unwrap()
}

#[test]
fn account_management_capabilities_follow_current_delegation_without_exposing_accounts() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"fixture-password"}),
    );
    let token = admin["token"].as_str().unwrap();
    let capabilities = call(
        &mut service,
        Some(token),
        "management.capabilities",
        json!({}),
    );
    assert_eq!(
        capabilities,
        json!({"account_list":true,"account_get":true,"account_create":true,"account_disable":true,"account_reset":true})
    );
    let member = call(
        &mut service,
        Some(token),
        "account.create",
        json!({"name":"Member","password":"member-password"}),
    );
    let login = call(
        &mut service,
        None,
        "account.login",
        json!({"name":"Member","password":"member-password"}),
    );
    let member_token = login["token"].as_str().unwrap();
    let baseline = call(
        &mut service,
        Some(member_token),
        "management.capabilities",
        json!({}),
    );
    assert!(
        baseline
            .as_object()
            .unwrap()
            .values()
            .all(|value| value == false)
    );
    for decision in ["allow", "deny"] {
        call(
            &mut service,
            Some(token),
            "permission.set",
            json!({"subject_type":"account","subject_id":member["id"],"operation":"account.create","scope":"global","target_id":"","decision":decision}),
        );
        let current = call(
            &mut service,
            Some(member_token),
            "management.capabilities",
            json!({}),
        );
        assert_eq!(current["account_create"], decision == "allow");
        assert_eq!(current["account_list"], false);
        assert_eq!(current["account_disable"], false);
        assert_eq!(current.as_object().unwrap().len(), 5);
    }
}

#[test]
fn management_capabilities_require_authenticated_human_and_exact_empty_arguments() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"fixture-password"}),
    );
    for (auth, args, expected) in [
        (None, json!({}), 3),
        (admin["token"].as_str(), json!({"account_id":"other"}), 2),
    ] {
        let fault = service
            .request(Request {
                operation: "management.capabilities".into(),
                auth: auth.map(str::to_owned),
                args,
                context: json!({}),
                request_id: None,
            })
            .unwrap_err();
        assert_eq!(fault.exit_code, expected);
    }
}
