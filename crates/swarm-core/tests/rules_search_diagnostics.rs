use serde_json::{Value, json};
use std::path::Path;
use swarm_core::{Service, protocol::Request};

fn request(
    service: &mut Service,
    operation: &str,
    auth: Option<&str>,
    args: Value,
    context: Value,
) -> Result<Value, swarm_core::protocol::Fault> {
    service.request(Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context,
        request_id: None,
    })
}

fn call(
    service: &mut Service,
    operation: &str,
    auth: Option<&str>,
    args: Value,
) -> Result<Value, swarm_core::protocol::Fault> {
    request(service, operation, auth, args, json!({}))
}

fn fixture() -> (tempfile::TempDir, Service, String) {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"rules-search-admin-fixture"}),
    )
    .unwrap();
    (root, service, admin["token"].as_str().unwrap().to_owned())
}

fn project(service: &mut Service, admin: &str, root: &Path) -> Value {
    call(
        service,
        "project.register",
        Some(admin),
        json!({"name":"Search project","root":root}),
    )
    .unwrap()
}

fn board_thread_post(service: &mut Service, admin: &str, name: &str) -> (Value, Value) {
    let board = call(
        service,
        "board.create",
        Some(admin),
        json!({"name":format!("{name} board"),"description":""}),
    )
    .unwrap();
    let thread = call(
        service,
        "thread.create",
        Some(admin),
        json!({"board_id":board["id"],"name":format!("{name} thread"),"description":""}),
    )
    .unwrap();
    call(
        service,
        "post.create",
        Some(admin),
        json!({"thread_id":thread["id"],"body":format!("{name} searchable needle")}),
    )
    .unwrap();
    (board, thread)
}

fn source(default_read: bool, rules: &str) -> String {
    format!(
        "# Search diagnostic fixture\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: {default_read}\nrules:\n{rules}\naccess:\n  boards: []\n  threads: []\n~~~\n"
    )
}

fn write_rules(root: &Path, text: &str) {
    std::fs::write(root.join("RULES.md"), text).unwrap();
}

fn diagnostic(
    service: &mut Service,
    token: &str,
    target: Value,
    context: Value,
) -> Result<Value, swarm_core::protocol::Fault> {
    request(
        service,
        "rules.check",
        Some(token),
        json!({"operation":"search","target":target}),
        context,
    )
}

fn diagnose_operation(
    service: &mut Service,
    token: &str,
    operation: &str,
    target: Value,
    context: Value,
) -> Result<Value, swarm_core::protocol::Fault> {
    request(
        service,
        "rules.check",
        Some(token),
        json!({"operation":operation,"target":target}),
        context,
    )
}

fn human_diagnose_operation(
    service: &mut Service,
    admin: &str,
    project_id: &str,
    actor_id: &str,
    operation: &str,
    context: Value,
) -> Result<Value, swarm_core::protocol::Fault> {
    request(
        service,
        "rules.check",
        Some(admin),
        json!({
            "project_id":project_id,
            "actor_id":actor_id,
            "operation":operation
        }),
        context,
    )
}

#[test]
fn search_diagnostic_matches_scoped_live_global_and_local_visibility_without_content() {
    let (data, mut service, admin) = fixture();
    let project_root = tempfile::tempdir().unwrap();
    let registered = project(&mut service, &admin, project_root.path());
    let (allowed_board, allowed_thread) = board_thread_post(&mut service, &admin, "allowed");
    let (hidden_board, hidden_thread) = board_thread_post(&mut service, &admin, "hidden");
    let ai = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":registered["id"]}),
    )
    .unwrap();
    let token = ai["session_token"].as_str().unwrap().to_owned();
    let board_id = allowed_board["id"].as_str().unwrap();
    let thread_id = allowed_thread["id"].as_str().unwrap();
    let rules = format!(
        "  - id: check-self\n    effect: allow\n    actions: [rules.check]\n  - id: search-one-board\n    effect: allow\n    actions: [search, board.list, board.get, thread.list, thread.get, post.list, post.get, response.list]\n    scope:\n      boards: [{board_id}]\n    when: {{ field: context.purpose, op: eq, value: review }}"
    );
    write_rules(data.path(), &source(false, &rules));
    let review = json!({"purpose":"review"});

    assert_eq!(
        diagnostic(&mut service, &token, json!({}), review.clone()).unwrap(),
        json!({"allowed":true,"reason":"allowed"})
    );
    assert_eq!(
        diagnostic(
            &mut service,
            &token,
            json!({"board_id":board_id}),
            review.clone()
        )
        .unwrap(),
        json!({"allowed":true,"reason":"allowed"})
    );
    assert_eq!(
        diagnostic(
            &mut service,
            &token,
            json!({"board_id":board_id,"thread_id":thread_id}),
            review.clone()
        )
        .unwrap(),
        json!({"allowed":true,"reason":"allowed"}),
        "a Board and Thread intersection follows the real search filter shape"
    );
    assert_eq!(
        diagnostic(
            &mut service,
            &token,
            json!({"thread_id":thread_id}),
            review.clone()
        )
        .unwrap(),
        json!({"allowed":true,"reason":"allowed"})
    );
    assert_eq!(
        diagnostic(&mut service, &token, json!({}), json!({})).unwrap(),
        json!({"allowed":false,"reason":"not_allowed"}),
        "missing condition data must fail closed"
    );

    let hidden_board_result = diagnostic(
        &mut service,
        &token,
        json!({"board_id":hidden_board["id"]}),
        review.clone(),
    )
    .unwrap();
    let missing_board_result = diagnostic(
        &mut service,
        &token,
        json!({"board_id":"missing-board"}),
        review.clone(),
    )
    .unwrap();
    assert_eq!(
        hidden_board_result,
        json!({"allowed":false,"reason":"target_unavailable"})
    );
    assert_eq!(hidden_board_result, missing_board_result);

    let hidden_thread_result = diagnostic(
        &mut service,
        &token,
        json!({"thread_id":hidden_thread["id"]}),
        review.clone(),
    )
    .unwrap();
    let missing_thread_result = diagnostic(
        &mut service,
        &token,
        json!({"thread_id":"missing-thread"}),
        review.clone(),
    )
    .unwrap();
    assert_eq!(
        hidden_thread_result,
        json!({"allowed":false,"reason":"target_unavailable"})
    );
    assert_eq!(hidden_thread_result, missing_thread_result);
    assert_eq!(
        diagnostic(
            &mut service,
            &token,
            json!({"board_id":hidden_board["id"],"thread_id":thread_id}),
            review.clone()
        )
        .unwrap(),
        json!({"allowed":false,"reason":"target_unavailable"}),
        "incompatible intersections are unavailable without identifying which filter failed"
    );

    let actual_search = request(
        &mut service,
        "search",
        Some(&token),
        json!({"query":"needle","scope":"all"}),
        review.clone(),
    )
    .unwrap();
    assert!(!actual_search["items"].as_array().unwrap().is_empty());
    assert!(
        actual_search["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| {
                item["board_id"] == allowed_board["id"]
                    || item["board_id"].is_null()
                    || item["id"] == allowed_board["id"]
            })
    );
    let no_matches = request(
        &mut service,
        "search",
        Some(&token),
        json!({"query":"not-present","scope":"post"}),
        review.clone(),
    )
    .unwrap();
    assert_eq!(no_matches["items"], json!([]));
    assert_eq!(
        diagnostic(&mut service, &token, json!({}), review.clone()).unwrap(),
        json!({"allowed":true,"reason":"allowed"}),
        "diagnosis reports an accessible subset, not whether this query happens to match"
    );

    let local_deny = source(
        true,
        "  - id: deny-search-locally\n    effect: deny\n    actions: [search]",
    );
    write_rules(project_root.path(), &local_deny);
    assert_eq!(
        diagnostic(&mut service, &token, json!({}), review.clone()).unwrap(),
        json!({"allowed":false,"reason":"not_allowed"}),
        "the current local Policy must be applied"
    );
    let locally_denied_search = request(
        &mut service,
        "search",
        Some(&token),
        json!({"query":"needle"}),
        review.clone(),
    )
    .unwrap();
    assert_eq!(locally_denied_search["items"], json!([]));

    std::fs::remove_file(project_root.path().join("RULES.md")).unwrap();
    assert_eq!(
        diagnostic(&mut service, &token, json!({}), review).unwrap(),
        json!({"allowed":true,"reason":"allowed"}),
        "removing the local deny must immediately restore the global scoped result"
    );
}

#[test]
fn search_collection_diagnostic_denies_empty_permissions_and_self_denial() {
    let (data, mut service, admin) = fixture();
    let project_root = tempfile::tempdir().unwrap();
    let registered = project(&mut service, &admin, project_root.path());
    let (_board, thread) = board_thread_post(&mut service, &admin, "private");
    let ai = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":registered["id"]}),
    )
    .unwrap();
    let token = ai["session_token"].as_str().unwrap().to_owned();
    let posts_before = call(
        &mut service,
        "post.list",
        Some(&admin),
        json!({"thread_id":thread["id"]}),
    )
    .unwrap()["items"]
        .as_array()
        .unwrap()
        .len();
    write_rules(
        data.path(),
        &source(
            false,
            "  - id: check-self-only\n    effect: allow\n    actions: [rules.check]",
        ),
    );

    assert_eq!(
        diagnostic(&mut service, &token, json!({}), json!({})).unwrap(),
        json!({"allowed":false,"reason":"not_allowed"}),
        "a collection is not allowed merely because the `search` action is globally known"
    );
    let actual = request(
        &mut service,
        "search",
        Some(&token),
        json!({"query":"needle"}),
        json!({}),
    )
    .unwrap();
    assert_eq!(actual["items"], json!([]));
    assert_eq!(
        call(
            &mut service,
            "post.list",
            Some(&admin),
            json!({"thread_id":thread["id"]})
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        posts_before,
        "rules.check and search diagnostics do not mutate Posts"
    );

    write_rules(data.path(), &source(false, ""));
    assert_eq!(
        diagnostic(&mut service, &token, json!({}), json!({})).unwrap(),
        json!({"allowed":false,"reason":"not_allowed"}),
        "the diagnostic itself must not recurse or bypass its own permission"
    );
}

#[test]
fn project_register_diagnostic_requires_global_permission_and_applies_local_denial() {
    let (data, mut service, _admin) = fixture();
    let project_root = tempfile::tempdir().unwrap();
    let registered = project(&mut service, &_admin, project_root.path());
    let ai = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":registered["id"]}),
    )
    .unwrap();
    let token = ai["session_token"].as_str().unwrap().to_owned();
    let self_check = "  - id: check-self\n    effect: allow\n    actions: [rules.check]";
    let global_allow = format!(
        "{self_check}\n  - id: register-project\n    effect: allow\n    actions: [project.register]"
    );
    let local_allow = "  - id: local-register\n    effect: allow\n    actions: [project.register]";
    let context = json!({"purpose":"register"});

    write_rules(data.path(), &source(false, &global_allow));
    assert_eq!(
        diagnose_operation(
            &mut service,
            &token,
            "project.register",
            json!({}),
            context.clone(),
        )
        .unwrap(),
        json!({"allowed":true,"reason":"allowed"})
    );

    write_rules(data.path(), &source(false, self_check));
    write_rules(project_root.path(), &source(false, local_allow));
    assert_eq!(
        diagnose_operation(
            &mut service,
            &token,
            "project.register",
            json!({}),
            context.clone(),
        )
        .unwrap(),
        json!({"allowed":false,"reason":"not_allowed"}),
        "a local grant cannot grant authority to register another project"
    );

    write_rules(data.path(), &source(false, &global_allow));
    write_rules(
        project_root.path(),
        &source(
            false,
            "  - id: local-deny-register\n    effect: deny\n    actions: [project.register]",
        ),
    );
    assert_eq!(
        diagnose_operation(&mut service, &token, "project.register", json!({}), context,).unwrap(),
        json!({"allowed":false,"reason":"not_allowed"}),
        "the effective project policy still applies after a global grant"
    );
}

#[test]
fn rules_check_self_diagnostic_uses_current_ai_policy_for_ai_and_human_without_side_effects() {
    let (data, mut service, admin) = fixture();
    let project_root = tempfile::tempdir().unwrap();
    let registered = project(&mut service, &admin, project_root.path());
    let ai = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":registered["id"]}),
    )
    .unwrap();
    let token = ai["session_token"].as_str().unwrap().to_owned();
    let actor = ai["identity"]["id"].as_str().unwrap().to_owned();
    let project_id = registered["id"].as_str().unwrap();
    let allow_check = format!(
        "  - id: check-self\n    effect: allow\n    subjects:\n      projects: [{project_id}]\n    actions: [rules.check]\n    when: {{ field: context.purpose, op: eq, value: inspect }}"
    );
    let context = json!({"purpose":"inspect"});

    let expiry_before =
        call(&mut service, "identity.status", Some(&token), json!({})).unwrap()["expires_at"]
            .clone();
    let project_count_before = call(&mut service, "project.list", Some(&admin), json!({})).unwrap()
        ["items"]
        .as_array()
        .unwrap()
        .len();

    write_rules(data.path(), &source(false, &allow_check));
    assert_eq!(
        diagnose_operation(
            &mut service,
            &token,
            "rules.check",
            json!({}),
            context.clone(),
        )
        .unwrap(),
        json!({"allowed":true,"reason":"allowed"}),
        "the AI self-check must evaluate policy directly, without recursive dispatch"
    );
    assert_eq!(
        human_diagnose_operation(
            &mut service,
            &admin,
            project_id,
            &actor,
            "rules.check",
            context.clone(),
        )
        .unwrap(),
        json!({"allowed":true,"reason":"allowed"}),
        "the human route must use the same targetless AI permission check"
    );

    for missing_context in [
        diagnose_operation(&mut service, &token, "rules.check", json!({}), json!({})).unwrap(),
        human_diagnose_operation(
            &mut service,
            &admin,
            project_id,
            &actor,
            "rules.check",
            json!({}),
        )
        .unwrap(),
    ] {
        assert_eq!(
            missing_context,
            json!({"allowed":false,"reason":"not_allowed"})
        );
    }

    write_rules(
        project_root.path(),
        &source(
            false,
            "  - id: deny-check-locally\n    effect: deny\n    actions: [rules.check]",
        ),
    );
    assert_eq!(
        diagnose_operation(
            &mut service,
            &token,
            "rules.check",
            json!({}),
            context.clone(),
        )
        .unwrap(),
        json!({"allowed":false,"reason":"not_allowed"})
    );
    assert_eq!(
        human_diagnose_operation(
            &mut service,
            &admin,
            project_id,
            &actor,
            "rules.check",
            context.clone(),
        )
        .unwrap(),
        json!({"allowed":false,"reason":"not_allowed"})
    );

    write_rules(data.path(), &source(false, ""));
    write_rules(project_root.path(), &source(false, ""));
    assert_eq!(
        diagnose_operation(
            &mut service,
            &token,
            "rules.check",
            json!({}),
            context.clone(),
        )
        .unwrap(),
        json!({"allowed":false,"reason":"not_allowed"}),
        "a missing global self-check grant denies the diagnostic"
    );
    assert_eq!(
        human_diagnose_operation(
            &mut service,
            &admin,
            project_id,
            &actor,
            "rules.check",
            context,
        )
        .unwrap(),
        json!({"allowed":false,"reason":"not_allowed"})
    );

    assert_eq!(
        call(&mut service, "identity.status", Some(&token), json!({})).unwrap()["expires_at"],
        expiry_before,
        "self-check diagnostics do not renew the AI identity"
    );
    assert_eq!(
        call(&mut service, "project.list", Some(&admin), json!({})).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        project_count_before,
        "self-check diagnostics do not create projects"
    );
}
