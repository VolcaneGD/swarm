use serde_json::{Value, json};
use std::collections::BTreeSet;
use swarm_core::{Service, protocol::Request};

fn call(
    service: &mut Service,
    auth: Option<&str>,
    operation: &str,
    args: Value,
) -> Result<Value, swarm_core::protocol::Fault> {
    service.request(Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    })
}

#[test]
fn authorization_capabilities_follow_current_grants_and_publish_canonical_scopes() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"admin-fixture-password"}),
    )
    .unwrap();
    let admin_token = admin["token"].as_str().unwrap();
    let member = call(
        &mut service,
        Some(admin_token),
        "account.create",
        json!({"name":"Member","password":"member-fixture-password"}),
    )
    .unwrap();
    let member_login = call(
        &mut service,
        None,
        "account.login",
        json!({"name":"Member","password":"member-fixture-password"}),
    )
    .unwrap();
    let member_token = member_login["token"].as_str().unwrap();

    let admin_capabilities = call(
        &mut service,
        Some(admin_token),
        "authorization.capabilities",
        json!({}),
    )
    .unwrap();
    for capability in [
        "role_create",
        "role_list",
        "role_assign",
        "permission_set",
        "permission_list",
    ] {
        assert_eq!(admin_capabilities[capability], true, "{capability}");
    }

    let baseline = call(
        &mut service,
        Some(member_token),
        "authorization.capabilities",
        json!({}),
    )
    .unwrap();
    for capability in [
        "role_create",
        "role_list",
        "role_assign",
        "permission_set",
        "permission_list",
    ] {
        assert_eq!(baseline[capability], false, "{capability}");
    }
    let operations = baseline["operations"].as_array().unwrap();
    assert!(!operations.is_empty());
    let names = operations
        .iter()
        .map(|entry| entry["operation"].as_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        names.len(),
        operations.len(),
        "catalog operation names are unique"
    );
    assert!(names.contains("read"));
    assert!(names.contains("permission.set"));
    let scopes = |operation: &str| {
        operations
            .iter()
            .find(|entry| entry["operation"] == operation)
            .unwrap()["scopes"]
            .clone()
    };
    assert_eq!(scopes("board.get"), json!(["global", "board", "thread"]));
    assert_eq!(scopes("read"), json!(["global", "board", "thread"]));
    assert_eq!(scopes("post.create"), json!(["global", "board", "thread"]));
    assert_eq!(scopes("role.create"), json!(["global"]));

    let role = call(
        &mut service,
        Some(admin_token),
        "role.create",
        json!({"name":"Authorization managers"}),
    )
    .unwrap();
    call(
        &mut service,
        Some(admin_token),
        "role.assign",
        json!({"account_id":member["id"],"role_id":role["id"],"assigned":true}),
    )
    .unwrap();
    call(
        &mut service,
        Some(admin_token),
        "permission.set",
        json!({"subject_type":"role","subject_id":role["id"],"operation":"role.list","scope":"global","target_id":"","decision":"allow"}),
    )
    .unwrap();
    let delegated = call(
        &mut service,
        Some(member_token),
        "authorization.capabilities",
        json!({}),
    )
    .unwrap();
    assert_eq!(delegated["role_list"], true);
    assert_eq!(delegated["role_create"], false);
    assert_eq!(delegated["permission_set"], false);

    call(
        &mut service,
        Some(admin_token),
        "permission.set",
        json!({"subject_type":"account","subject_id":member["id"],"operation":"role.list","scope":"global","target_id":"","decision":"deny"}),
    )
    .unwrap();
    let denied = call(
        &mut service,
        Some(member_token),
        "authorization.capabilities",
        json!({}),
    )
    .unwrap();
    assert_eq!(
        denied["role_list"], false,
        "same-scope deny overrides role allow"
    );
}

#[test]
fn authorization_capabilities_require_exact_human_read_and_deny_ai() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"admin-fixture-password"}),
    )
    .unwrap();
    let admin_token = admin["token"].as_str().unwrap();
    assert_eq!(
        call(
            &mut service,
            Some(admin_token),
            "authorization.capabilities",
            json!({"ignored":true}),
        )
        .unwrap_err()
        .exit_code,
        2
    );
    assert_eq!(
        call(&mut service, None, "authorization.capabilities", json!({}))
            .unwrap_err()
            .exit_code,
        3
    );

    let project_root = tempfile::tempdir().unwrap();
    let project = call(
        &mut service,
        Some(admin_token),
        "project.register",
        json!({"name":"Fixture","root":project_root.path()}),
    )
    .unwrap();
    let grant = call(
        &mut service,
        None,
        "identity.start",
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    let ai_token = grant["session_token"].as_str().unwrap();
    assert_eq!(
        call(
            &mut service,
            Some(ai_token),
            "authorization.capabilities",
            json!({}),
        )
        .unwrap_err()
        .exit_code,
        4
    );
}
