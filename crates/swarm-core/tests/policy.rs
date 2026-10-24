use serde_json::{Value, json};
use std::path::Path;
use swarm_core::{
    Service,
    protocol::{Fault, Request},
};

fn request(
    service: &mut Service,
    operation: &str,
    auth: Option<&str>,
    args: Value,
    context: Value,
) -> Result<Value, Fault> {
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
) -> Result<Value, Fault> {
    request(service, operation, auth, args, json!({}))
}

fn fixture() -> (tempfile::TempDir, Service, String) {
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"correct horse battery staple"}),
    )
    .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    (data, service, admin)
}

fn project(service: &mut Service, admin: &str, root: &Path, name: &str) -> Value {
    call(
        service,
        "project.register",
        Some(admin),
        json!({"name":name,"root":root}),
    )
    .unwrap()
}

fn identity(service: &mut Service, project: &Value, model: Option<&str>) -> (String, String) {
    let mut args = json!({"project_id":project["id"]});
    if let Some(model) = model {
        args["model_name"] = json!(model);
    }
    let started = call(service, "identity.start", None, args).unwrap();
    (
        started["identity"]["id"].as_str().unwrap().to_owned(),
        started["session_token"].as_str().unwrap().to_owned(),
    )
}

fn board_and_thread(service: &mut Service, admin: &str) -> (Value, Value) {
    let board = call(
        service,
        "board.create",
        Some(admin),
        json!({"name":"Board"}),
    )
    .unwrap();
    let thread = call(
        service,
        "thread.create",
        Some(admin),
        json!({"board_id":board["id"],"name":"Thread"}),
    )
    .unwrap();
    (board, thread)
}

fn write_rules(path: &Path, body: &str) {
    std::fs::write(
        path.join("RULES.md"),
        format!("# Swarm rules\n\n~~~swarm-rules\n{body}\n~~~\n"),
    )
    .unwrap();
}

fn header(rules: &str) -> String {
    let rules = if rules.trim().is_empty() {
        "  []"
    } else {
        rules
    };
    format!(
        "schema_version: 1\ndefaults:\n  ai_read: true\nrules:\n{rules}\naccess:\n  boards: []\n  threads: []"
    )
}

fn allow_create(project_id: &str, when: &str) -> String {
    format!(
        "  - id: allow-create\n    effect: allow\n    subjects:\n      projects: [{project_id}]\n    actions: [post.create]\n    when: {when}"
    )
}

fn create(
    service: &mut Service,
    token: &str,
    thread_id: &str,
    context: Value,
) -> Result<Value, Fault> {
    request(
        service,
        "post.create",
        Some(token),
        json!({"thread_id":thread_id,"body":"AI post"}),
        context,
    )
}

#[test]
fn documented_review_policy_enforces_its_promised_boundaries() {
    let (data, mut service, admin) = fixture();
    let root = tempfile::tempdir().unwrap();
    let other_root = tempfile::tempdir().unwrap();
    let selected = project(&mut service, &admin, root.path(), "Review");
    let other = project(&mut service, &admin, other_root.path(), "Other");
    let (_, thread) = board_and_thread(&mut service, &admin);
    let (_, other_thread) = board_and_thread(&mut service, &admin);
    let thread_id = thread["id"].as_str().unwrap();
    let project_id = selected["id"].as_str().unwrap();
    let source = include_str!("../../../docs/rules-review.example.md")
        .replace("project-id", project_id)
        .replace("thread-id", thread_id);
    let validation = call(
        &mut service,
        "rules.validate",
        Some(&admin),
        json!({"source":source}),
    )
    .unwrap();
    assert_eq!(validation["valid"], true);
    let current = call(&mut service, "rules.get", Some(&admin), json!({})).unwrap();
    call(
        &mut service,
        "rules.save",
        Some(&admin),
        json!({"source":source,"if_revision":current["revision"]}),
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(data.path().join("RULES.md")).unwrap(),
        source
    );
    let (_, token) = identity(&mut service, &selected, None);
    let (_, other_token) = identity(&mut service, &other, None);
    assert!(call(&mut service, "board.list", Some(&token), json!({})).is_ok());
    let post = create(&mut service, &token, thread_id, json!({"task":"review"})).unwrap();
    request(
        &mut service,
        "response.create",
        Some(&token),
        json!({"thread_id":thread_id,"to":post["no"],"body":"Review response"}),
        json!({"task":"review"}),
    )
    .unwrap();
    for (actor, target, context) in [
        (token.as_str(), thread_id, json!({})),
        (token.as_str(), thread_id, json!({"task":"implement"})),
        (
            token.as_str(),
            other_thread["id"].as_str().unwrap(),
            json!({"task":"review"}),
        ),
        (other_token.as_str(), thread_id, json!({"task":"review"})),
    ] {
        assert_eq!(
            create(&mut service, actor, target, context)
                .unwrap_err()
                .code,
            "PERMISSION_DENIED"
        );
    }
    // A local edit grant must not defeat the global example's explicit deny,
    // even for a post owned by this AI at its current revision.
    write_rules(
        root.path(),
        &header("  - id: local-edit\n    effect: allow\n    actions: [post.edit]"),
    );
    assert_eq!(
        call(
            &mut service,
            "post.edit",
            Some(&token),
            json!({"thread_id":thread_id,"no":post["no"],"body":"Changed","if_revision":post["revision"]}),
        ).unwrap_err().code,
        "PERMISSION_DENIED"
    );
    let unchanged = call(
        &mut service,
        "post.get",
        Some(&admin),
        json!({"thread_id":thread_id,"no":post["no"]}),
    )
    .unwrap();
    assert_eq!(unchanged["body"], "AI post");
}

#[test]
fn default_ai_access_allows_reads_but_denies_writes_and_history() {
    let (_data, mut service, admin) = fixture();
    let project_root = tempfile::tempdir().unwrap();
    let project = project(&mut service, &admin, project_root.path(), "Project");
    let (_, thread) = board_and_thread(&mut service, &admin);
    call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread["id"],"body":"Human post"}),
    )
    .unwrap();
    let (_, token) = identity(&mut service, &project, None);

    assert!(call(&mut service, "board.list", Some(&token), json!({})).is_ok());
    assert!(
        call(
            &mut service,
            "thread.get",
            Some(&token),
            json!({"id":thread["id"]})
        )
        .is_ok()
    );
    assert!(
        call(
            &mut service,
            "post.get",
            Some(&token),
            json!({"thread_id":thread["id"],"no":1})
        )
        .is_ok()
    );
    assert_eq!(
        create(
            &mut service,
            &token,
            thread["id"].as_str().unwrap(),
            json!({})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    assert!(
        call(
            &mut service,
            "post.history",
            Some(&token),
            json!({"thread_id":thread["id"],"no":1})
        )
        .unwrap_err()
        .exit_code
            == 4
    );
}

#[test]
fn conditional_allows_fail_closed_on_missing_or_wrong_typed_context_and_not_unknown() {
    let (_data, mut service, admin) = fixture();
    let project_root = tempfile::tempdir().unwrap();
    let project = project(&mut service, &admin, project_root.path(), "Project");
    let (_, thread) = board_and_thread(&mut service, &admin);
    let (_, token) = identity(&mut service, &project, None);
    write_rules(
        project_root.path(),
        &header(&allow_create(
            project["id"].as_str().unwrap(),
            "{ field: context.purpose, op: eq, value: review }",
        )),
    );
    assert_eq!(
        create(
            &mut service,
            &token,
            thread["id"].as_str().unwrap(),
            json!({})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    assert_eq!(
        create(
            &mut service,
            &token,
            thread["id"].as_str().unwrap(),
            json!({"purpose":7})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    assert!(
        create(
            &mut service,
            &token,
            thread["id"].as_str().unwrap(),
            json!({"purpose":"review"})
        )
        .is_ok()
    );

    let not_allow = format!(
        "  - id: not-missing-purpose\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [post.create]\n    when:\n      not:\n        field: context.purpose\n        op: exists\n        value: true",
        project["id"]
    );
    write_rules(project_root.path(), &header(&not_allow));
    assert_eq!(
        create(
            &mut service,
            &token,
            thread["id"].as_str().unwrap(),
            json!({})
        )
        .unwrap_err()
        .exit_code,
        4
    );

    let unconditional_allow = format!(
        "  - id: allow-create\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [post.create]",
        project["id"]
    );
    let deny_unknown = format!(
        "  - id: deny-unknown\n    effect: deny\n    subjects:\n      projects: [{}]\n    actions: [post.create]\n    when:\n      field: context.purpose\n      op: eq\n      value: stop",
        project["id"]
    );
    write_rules(
        project_root.path(),
        &header(&format!("{unconditional_allow}\n{deny_unknown}")),
    );
    assert_eq!(
        create(
            &mut service,
            &token,
            thread["id"].as_str().unwrap(),
            json!({})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    assert!(
        create(
            &mut service,
            &token,
            thread["id"].as_str().unwrap(),
            json!({"purpose":"review"})
        )
        .is_ok()
    );
    assert_eq!(
        create(
            &mut service,
            &token,
            thread["id"].as_str().unwrap(),
            json!({"purpose":"stop"})
        )
        .unwrap_err()
        .exit_code,
        4
    );
}

#[test]
fn trusted_actor_project_and_time_fields_ignore_forged_context_values() {
    let (_data, mut service, admin) = fixture();
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    let first_project = project(&mut service, &admin, first_root.path(), "First");
    let second_project = project(&mut service, &admin, second_root.path(), "Second");
    let (_, thread) = board_and_thread(&mut service, &admin);
    let (first_actor, first_token) = identity(&mut service, &first_project, None);
    let (other_actor, other_token) = identity(&mut service, &first_project, None);
    let (_, other_project_token) = identity(&mut service, &second_project, None);
    let trusted_rule = format!(
        "  - id: trusted-fields\n    effect: allow\n    actions: [post.create]\n    when:\n      all:\n        - field: project.id\n          op: eq\n          value: {}\n        - field: actor.id\n          op: eq\n          value: {first_actor}\n        - field: time.hour\n          op: gte\n          value: 0\n        - field: time.hour\n          op: lte\n          value: 23",
        first_project["id"]
    );
    write_rules(_data.path(), &header(&trusted_rule));

    // Claims shaped like trusted fields cannot override the authenticated identity or the clock.
    let forged = json!({
        "actor":{"id":other_actor},
        "project":{"id":second_project["id"]},
        "time":{"hour":99}
    });
    assert!(
        create(
            &mut service,
            &first_token,
            thread["id"].as_str().unwrap(),
            forged.clone()
        )
        .is_ok()
    );
    assert_eq!(
        create(
            &mut service,
            &other_token,
            thread["id"].as_str().unwrap(),
            json!({
                "actor":{"id":first_actor},
                "project":{"id":first_project["id"]},
                "time":{"hour":0}
            })
        )
        .unwrap_err()
        .exit_code,
        4
    );
    assert_eq!(
        create(
            &mut service,
            &other_project_token,
            thread["id"].as_str().unwrap(),
            json!({
                "actor":{"id":first_actor},
                "project":{"id":first_project["id"]},
                "time":{"hour":0}
            })
        )
        .unwrap_err()
        .exit_code,
        4
    );
}

#[test]
fn explicit_global_deny_wins_and_invalid_local_rules_are_project_scoped() {
    let (_data, mut service, admin) = fixture();
    let first_root = tempfile::tempdir().unwrap();
    let second_root = tempfile::tempdir().unwrap();
    let first = project(&mut service, &admin, first_root.path(), "First");
    let second = project(&mut service, &admin, second_root.path(), "Second");
    let (_, thread) = board_and_thread(&mut service, &admin);
    let (_, first_token) = identity(&mut service, &first, None);
    let (_, second_token) = identity(&mut service, &second, None);
    let local_allow = format!(
        "  - id: local-grant\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [post.create]",
        first["id"]
    );
    write_rules(first_root.path(), &header(&local_allow));
    write_rules(
        second_root.path(),
        &header("  - id: local-grant\n    effect: allow\n    actions: [post.create]"),
    );
    // A global allow and a project-local deny still resolve to a deny.
    let global_allow = allow_create(
        first["id"].as_str().unwrap(),
        "{ field: context.purpose, op: exists, value: true }",
    );
    write_rules(_data.path(), &header(&global_allow));
    let local_deny = format!(
        "  - id: local-deny\n    effect: deny\n    subjects:\n      projects: [{}]\n    actions: [post.create]",
        first["id"]
    );
    write_rules(first_root.path(), &header(&local_deny));
    assert_eq!(
        create(
            &mut service,
            &first_token,
            thread["id"].as_str().unwrap(),
            json!({"purpose":"review"})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    // A malformed project-local file blocks that project, while another project's rules still work.
    write_rules(_data.path(), &header(""));
    write_rules(
        first_root.path(),
        "schema_version: 1\ndefaults: nope\nrules: []",
    );
    assert_eq!(
        create(
            &mut service,
            &first_token,
            thread["id"].as_str().unwrap(),
            json!({})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    write_rules(
        second_root.path(),
        &header("  - id: local-grant\n    effect: allow\n    actions: [post.create]"),
    );
    assert!(
        create(
            &mut service,
            &second_token,
            thread["id"].as_str().unwrap(),
            json!({})
        )
        .is_ok()
    );

    // A malformed global document fails closed for every project, even when both locals grant.
    let local_grant = "  - id: local-grant\n    effect: allow\n    actions: [post.create]";
    write_rules(first_root.path(), &header(local_grant));
    write_rules(second_root.path(), &header(local_grant));
    write_rules(_data.path(), "schema_version: 1\ndefaults: nope\nrules: []");
    for token in [&first_token, &second_token] {
        assert_eq!(
            create(
                &mut service,
                token,
                thread["id"].as_str().unwrap(),
                json!({})
            )
            .unwrap_err()
            .exit_code,
            4
        );
    }
}

#[test]
fn rules_reload_live_access_overrides_and_hard_ai_limits_are_enforced() {
    let (_data, mut service, admin) = fixture();
    let project_root = tempfile::tempdir().unwrap();
    let project = project(&mut service, &admin, project_root.path(), "Project");
    let (board, thread) = board_and_thread(&mut service, &admin);
    let (_, token) = identity(&mut service, &project, None);
    let post = call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread["id"],"body":"Human body"}),
    )
    .unwrap();
    let create_rule = allow_create(
        project["id"].as_str().unwrap(),
        "{ field: context.purpose, op: eq, value: review }",
    );
    write_rules(_data.path(), &header(""));
    assert_eq!(
        create(
            &mut service,
            &token,
            thread["id"].as_str().unwrap(),
            json!({"purpose":"review"})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    write_rules(_data.path(), &header(&create_rule));
    assert!(
        create(
            &mut service,
            &token,
            thread["id"].as_str().unwrap(),
            json!({"purpose":"review"})
        )
        .is_ok()
    );

    let access = format!(
        "  boards:\n    - id: {}\n      projects:\n        {}:\n          read: deny\n  threads:\n    - id: {}\n      projects:\n        {}:\n          read: allow",
        board["id"], project["id"], thread["id"], project["id"]
    );
    let allowed_post = format!(
        "  - id: create\n    effect: allow\n    actions: [post.create]\n    subjects:\n      projects: [{}]",
        project["id"]
    );
    write_rules(
        _data.path(),
        &format!(
            "schema_version: 1\ndefaults:\n  ai_read: true\nrules:\n{allowed_post}\naccess:\n{access}"
        ),
    );
    assert_eq!(
        call(
            &mut service,
            "board.get",
            Some(&token),
            json!({"id":board["id"]})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    assert!(
        call(
            &mut service,
            "thread.get",
            Some(&token),
            json!({"id":thread["id"]})
        )
        .is_ok()
    );
    assert!(
        call(
            &mut service,
            "post.list",
            Some(&token),
            json!({"thread_id":thread["id"]})
        )
        .is_ok()
    );
    assert_eq!(
        call(
            &mut service,
            "thread.get",
            Some(&token),
            json!({"id":thread["id"]})
        )
        .unwrap()["board_id"],
        Value::Null
    );
    let visible_boards = call(&mut service, "board.list", Some(&token), json!({})).unwrap();
    assert!(
        visible_boards["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item["id"] != board["id"])
    );
    let visible_threads = call(&mut service, "thread.list", Some(&token), json!({})).unwrap();
    assert_eq!(visible_threads["items"].as_array().unwrap().len(), 1);
    assert_eq!(visible_threads["items"][0]["id"], thread["id"]);
    assert_eq!(visible_threads["items"][0]["board_id"], Value::Null);

    // An explicit Thread deny also overrides an otherwise readable parent Board.
    let inverse_access = format!(
        "  boards:\n    - id: {}\n      projects:\n        {}:\n          read: allow\n  threads:\n    - id: {}\n      projects:\n        {}:\n          read: deny",
        board["id"], project["id"], thread["id"], project["id"]
    );
    write_rules(
        _data.path(),
        &format!(
            "schema_version: 1\ndefaults:\n  ai_read: true\nrules: []\naccess:\n{inverse_access}"
        ),
    );
    assert!(
        call(
            &mut service,
            "board.get",
            Some(&token),
            json!({"id":board["id"]})
        )
        .is_ok()
    );
    assert_eq!(
        call(
            &mut service,
            "thread.get",
            Some(&token),
            json!({"id":thread["id"]})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    let hidden_threads = call(&mut service, "thread.list", Some(&token), json!({})).unwrap();
    assert!(hidden_threads["items"].as_array().unwrap().is_empty());

    // Even an explicit operation grant cannot permit AI deletion/restoration or editing another author's post.
    let hard_allow = format!(
        "  - id: forbidden-ops\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [post.edit, post.delete, post.restore, post.history]",
        project["id"]
    );
    write_rules(
        _data.path(),
        &format!(
            "schema_version: 1\ndefaults:\n  ai_read: true\nrules:\n{hard_allow}\naccess:\n{access}"
        ),
    );
    assert_eq!(
        call(
            &mut service,
            "post.edit",
            Some(&token),
            json!({"thread_id":thread["id"],"no":1,"body":"tampered","if_revision":1})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    assert_eq!(
        call(
            &mut service,
            "post.delete",
            Some(&token),
            json!({"thread_id":thread["id"],"no":1})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    assert_eq!(
        call(
            &mut service,
            "post.restore",
            Some(&token),
            json!({"thread_id":thread["id"],"no":1})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    let history = call(
        &mut service,
        "post.history",
        Some(&token),
        json!({"thread_id":thread["id"],"no":1}),
    )
    .unwrap();
    assert!(history["items"].as_array().unwrap().is_empty());
    call(
        &mut service,
        "post.edit",
        Some(&admin),
        json!({"thread_id":thread["id"],"no":1,"body":"Revised human body","if_revision":1}),
    )
    .unwrap();
    let visible_history = call(
        &mut service,
        "post.history",
        Some(&token),
        json!({"thread_id":thread["id"],"no":1}),
    )
    .unwrap();
    assert_eq!(visible_history["items"][0]["body"], "Human body");
    assert_eq!(
        request(
            &mut service,
            "rules.check",
            Some(&token),
            json!({"operation":"post.history","target":{"thread_id":thread["id"],"no":1}}),
            json!({})
        )
        .unwrap(),
        json!({"allowed":true,"reason":"allowed"})
    );
    call(
        &mut service,
        "post.delete",
        Some(&admin),
        json!({"thread_id":thread["id"],"no":1}),
    )
    .unwrap();
    let deleted = call(
        &mut service,
        "post.get",
        Some(&token),
        json!({"thread_id":thread["id"],"no":1}),
    )
    .unwrap();
    assert_eq!(deleted["deleted"], true);
    assert!(deleted["body"].is_null());
    assert_eq!(
        request(
            &mut service,
            "rules.check",
            Some(&token),
            json!({"operation":"post.history","target":{"thread_id":thread["id"],"no":1}}),
            json!({})
        )
        .unwrap(),
        json!({"allowed":true,"reason":"allowed"})
    );
    assert!(
        call(
            &mut service,
            "post.history",
            Some(&token),
            json!({"thread_id":thread["id"],"no":1})
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_ne!(post["body"], deleted["body"]);
    let human_history = call(
        &mut service,
        "post.history",
        Some(&admin),
        json!({"thread_id":thread["id"],"no":1}),
    )
    .unwrap();
    assert_eq!(human_history["items"][0]["body"], "Human body");
    call(
        &mut service,
        "post.restore",
        Some(&admin),
        json!({"thread_id":thread["id"],"no":1}),
    )
    .unwrap();
    let restored_history = call(
        &mut service,
        "post.history",
        Some(&token),
        json!({"thread_id":thread["id"],"no":1}),
    )
    .unwrap();
    assert_eq!(restored_history["items"][0]["body"], "Human body");
}

#[test]
fn ai_edits_only_its_own_posts_and_unknown_model_supplement_keeps_old_snapshot() {
    let (_data, mut service, admin) = fixture();
    let project_root = tempfile::tempdir().unwrap();
    let project = project(&mut service, &admin, project_root.path(), "Project");
    let (_, thread) = board_and_thread(&mut service, &admin);
    let (actor, token) = identity(&mut service, &project, None);
    let (_, other_token) = identity(&mut service, &project, None);
    let rule = format!(
        "  - id: own-write\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [post.create, post.edit]",
        project["id"]
    );
    write_rules(_data.path(), &header(&rule));
    let own = call(
        &mut service,
        "post.create",
        Some(&token),
        json!({"thread_id":thread["id"],"body":"Before name"}),
    )
    .unwrap();
    assert_eq!(own["author"]["model_name"], "不明");
    assert_eq!(
        call(
            &mut service,
            "identity.model",
            Some(&token),
            json!({"model_name":"DeclaredModel"})
        )
        .unwrap()["id"],
        actor
    );
    let next = call(
        &mut service,
        "post.create",
        Some(&token),
        json!({"thread_id":thread["id"],"body":"After name"}),
    )
    .unwrap();
    assert_eq!(next["author"]["model_name"], "DeclaredModel");
    let edited = call(
        &mut service,
        "post.edit",
        Some(&token),
        json!({"thread_id":thread["id"],"no":own["no"],"body":"Edited by owner","if_revision":1}),
    )
    .unwrap();
    assert_eq!(edited["body"], "Edited by owner");
    assert_eq!(
        call(
            &mut service,
            "post.edit",
            Some(&other_token),
            json!({"thread_id":thread["id"],"no":own["no"],"body":"Other actor","if_revision":2})
        )
        .unwrap_err()
        .exit_code,
        4
    );
    assert_eq!(
        call(
            &mut service,
            "post.get",
            Some(&token),
            json!({"thread_id":thread["id"],"no":1})
        )
        .unwrap()["author"]["model_name"],
        "不明"
    );
    assert_eq!(
        call(
            &mut service,
            "post.history",
            Some(&token),
            json!({"thread_id":thread["id"],"no":1})
        )
        .unwrap_err()
        .exit_code,
        4
    );
}

#[test]
fn rules_check_is_authenticated_strict_and_uses_the_live_policy() {
    let (data, mut service, admin) = fixture();
    let project_root = tempfile::tempdir().unwrap();
    let project = project(&mut service, &admin, project_root.path(), "Project");
    let (_, thread) = board_and_thread(&mut service, &admin);
    let (_, token) = identity(&mut service, &project, None);
    let rule = format!(
        "  - id: check-and-create\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [rules.check, post.create]\n    when: {{ field: context.purpose, op: eq, value: review }}",
        project["id"]
    );
    write_rules(data.path(), &header(&rule));

    let checked = request(
        &mut service,
        "rules.check",
        Some(&token),
        json!({"operation":"post.create","target":{"thread_id":thread["id"]}}),
        json!({"purpose":"review"}),
    )
    .unwrap();
    assert_eq!(checked, json!({"allowed":true,"reason":"allowed"}));

    let missing_context = request(
        &mut service,
        "rules.check",
        Some(&token),
        json!({"operation":"post.create","target":{"thread_id":thread["id"]}}),
        json!({}),
    )
    .unwrap();
    assert_eq!(
        missing_context,
        json!({"allowed":false,"reason":"not_allowed"})
    );

    for forged in [
        json!({"operation":"post.get","actor_id":"another","target":{"thread_id":thread["id"],"no":1}}),
        json!({"operation":"post.get","project_id":project["id"],"target":{"thread_id":thread["id"],"no":1}}),
    ] {
        assert_eq!(
            request(&mut service, "rules.check", Some(&token), forged, json!({}))
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
    }
    assert_eq!(
        request(
            &mut service,
            "rules.check",
            Some(&token),
            json!({"operation":"search"}),
            json!({})
        )
        .unwrap(),
        json!({"allowed":true,"reason":"allowed"})
    );
    assert_eq!(
        request(
            &mut service,
            "rules.check",
            Some(&token),
            json!({"operation":"post.get","target":{"thread_id":thread["id"],"no":1,"ignored":"x"}}),
            json!({})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );
    assert_eq!(
        request(
            &mut service,
            "rules.check",
            Some(&token),
            json!({"operation":"post.get"}),
            json!({"oversized":"x".repeat(64 * 1024)})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );
}

#[test]
fn local_unknown_posts_keep_their_author_snapshot_after_report_enrichment() {
    let (data, mut service, admin) = fixture();
    let root = tempfile::tempdir().unwrap();
    let project = project(&mut service, &admin, root.path(), "Project");
    let (_, thread) = board_and_thread(&mut service, &admin);
    let grant = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"],"model_local":true}),
    )
    .unwrap();
    let token = grant["session_token"].as_str();
    write_rules(
        data.path(),
        &header(&format!(
            "  - id: local-write\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [post.create]",
            project["id"]
        )),
    );
    let first = call(
        &mut service,
        "post.create",
        token,
        json!({"thread_id":thread["id"],"body":"Before"}),
    )
    .unwrap();
    assert_eq!(first["author"]["model_name"], "ローカルモデル");
    let named = call(&mut service,"identity.model",token,json!({"model_report":{"client":"openai-compatible","session_id":"response","project_root":root.path(),"model_name":"ReportedLocal","basis":"response"},"model_local":true})).unwrap();
    assert_eq!(named["id"], grant["identity"]["id"]);
    let next = call(
        &mut service,
        "post.create",
        token,
        json!({"thread_id":thread["id"],"body":"After"}),
    )
    .unwrap();
    assert_eq!(next["author"]["model_name"], "ReportedLocal");
    assert_eq!(next["author"]["harness_name"], "不明");
    let past = call(
        &mut service,
        "post.get",
        token,
        json!({"thread_id":thread["id"],"no":first["no"]}),
    )
    .unwrap();
    assert_eq!(past["author"]["model_name"], "ローカルモデル");
}

#[test]
fn harness_enrichment_snapshots_new_posts_without_relabeling_old_posts() {
    let (data, mut service, admin) = fixture();
    let root = tempfile::tempdir().unwrap();
    let project = project(&mut service, &admin, root.path(), "Project");
    let (_, thread) = board_and_thread(&mut service, &admin);
    let grant = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    let token = grant["session_token"].as_str();
    write_rules(
        data.path(),
        &header(&format!(
            "  - id: write\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [post.create, response.create]",
            project["id"]
        )),
    );
    let first = call(
        &mut service,
        "post.create",
        token,
        json!({"thread_id":thread["id"],"body":"Before"}),
    )
    .unwrap();
    assert_eq!(first["author"]["harness_name"], "不明");
    let before = call(&mut service, "identity.status", token, json!({})).unwrap();
    let named = call(
        &mut service,
        "identity.model",
        token,
        json!({"model_client":"antigravity","model_name":"Reported"}),
    )
    .unwrap();
    assert_eq!(named["id"], grant["identity"]["id"]);
    assert_eq!(named["expires_at"], before["expires_at"]);
    let next = call(
        &mut service,
        "response.create",
        token,
        json!({"thread_id":thread["id"],"to":1,"body":"After"}),
    )
    .unwrap();
    assert_eq!(next["author"]["model_name"], "Reported");
    assert_eq!(next["author"]["harness_name"], "Antigravity");
    let old = call(
        &mut service,
        "post.get",
        token,
        json!({"thread_id":thread["id"],"no":1}),
    )
    .unwrap();
    assert_eq!(old["author"]["harness_name"], "不明");
    assert_eq!(
        call(
            &mut service,
            "identity.model",
            token,
            json!({"model_client":"codex","model_name":"Reported"})
        )
        .unwrap_err()
        .code,
        "MODEL_CHANGED"
    );
    let again = call(
        &mut service,
        "post.create",
        token,
        json!({"thread_id":thread["id"],"body":"After rejection"}),
    )
    .unwrap();
    assert_eq!(again["author"]["harness_name"], "Antigravity");
}

#[test]
fn new_harness_reports_snapshot_labels_but_do_not_grant_posting_permissions() {
    let (data, mut service, admin) = fixture();
    let root = tempfile::tempdir().unwrap();
    let project = project(&mut service, &admin, root.path(), "Project");
    let (_, thread) = board_and_thread(&mut service, &admin);
    for (client, label) in [
        ("hermes-agent", "Hermes Agent"),
        ("openclaw", "OpenClaw"),
        ("gemini-cli", "Gemini CLI"),
        ("opencode", "OpenCode"),
        ("cursor", "Cursor"),
        ("copilot-cli", "GitHub Copilot CLI"),
    ] {
        write_rules(data.path(), &header(""));
        let grant = call(&mut service,"identity.start",None,json!({"project_id":project["id"],
            "model_report":{"client":client,"session_id":"session","project_root":root.path(),"model_name":"Model","basis":"selection"}})).unwrap();
        let token = grant["session_token"].as_str();
        assert_eq!(grant["identity"]["model_source"], "automatic");
        assert_eq!(
            call(
                &mut service,
                "post.create",
                token,
                json!({"thread_id":thread["id"],"body":"No grant"})
            )
            .unwrap_err()
            .code,
            "PERMISSION_DENIED"
        );
        write_rules(
            data.path(),
            &header(&format!(
                "  - id: write\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [post.create]",
                project["id"]
            )),
        );
        let post = call(
            &mut service,
            "post.create",
            token,
            json!({"thread_id":thread["id"],"body":"With grant"}),
        )
        .unwrap();
        assert_eq!(post["author"]["model_name"], "Model");
        assert_eq!(post["author"]["harness_name"], label);
    }
}

#[test]
fn rules_check_hides_unreadable_and_missing_objects_before_reporting_policy_reasons() {
    let (data, mut service, admin) = fixture();
    let project_root = tempfile::tempdir().unwrap();
    let project = project(&mut service, &admin, project_root.path(), "Project");
    let (board, thread) = board_and_thread(&mut service, &admin);
    let (_, token) = identity(&mut service, &project, None);
    call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread["id"],"body":"private"}),
    )
    .unwrap();
    let rules = format!(
        "  - id: checker\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [rules.check]",
        project["id"]
    );
    let access = format!(
        "  boards:\n    - id: {}\n      projects:\n        {}:\n          read: deny\n  threads:\n    - id: {}\n      projects:\n        {}:\n          read: deny",
        board["id"], project["id"], thread["id"], project["id"]
    );
    write_rules(
        data.path(),
        &format!(
            "schema_version: 1\ndefaults:\n  ai_read: true\nrules:\n{rules}\naccess:\n{access}"
        ),
    );

    let hidden_edit = request(
        &mut service,
        "rules.check",
        Some(&token),
        json!({"operation":"post.edit","target":{"thread_id":thread["id"],"no":1}}),
        json!({}),
    )
    .unwrap();
    let absent_edit = request(
        &mut service,
        "rules.check",
        Some(&token),
        json!({"operation":"post.edit","target":{"thread_id":thread["id"],"no":999}}),
        json!({}),
    )
    .unwrap();
    assert_eq!(
        hidden_edit,
        json!({"allowed":false,"reason":"target_unavailable"})
    );
    assert_eq!(hidden_edit, absent_edit);

    let hidden_create = request(
        &mut service,
        "rules.check",
        Some(&token),
        json!({"operation":"post.create","target":{"thread_id":thread["id"]}}),
        json!({}),
    )
    .unwrap();
    let absent_create = request(
        &mut service,
        "rules.check",
        Some(&token),
        json!({"operation":"post.create","target":{"thread_id":"missing-thread"}}),
        json!({}),
    )
    .unwrap();
    assert_eq!(
        hidden_create,
        json!({"allowed":false,"reason":"target_unavailable"})
    );
    assert_eq!(hidden_create, absent_create);

    let hidden_board = request(
        &mut service,
        "rules.check",
        Some(&token),
        json!({"operation":"board.get","target":{"id":board["id"]}}),
        json!({}),
    )
    .unwrap();
    let missing_board = request(
        &mut service,
        "rules.check",
        Some(&token),
        json!({"operation":"board.get","target":{"id":"missing-board"}}),
        json!({}),
    )
    .unwrap();
    assert_eq!(
        hidden_board,
        json!({"allowed":false,"reason":"target_unavailable"})
    );
    assert_eq!(hidden_board, missing_board);
    let hidden_board_threads = request(
        &mut service,
        "rules.check",
        Some(&token),
        json!({"operation":"thread.list","target":{"board_id":board["id"]}}),
        json!({}),
    )
    .unwrap();
    let missing_board_threads = request(
        &mut service,
        "rules.check",
        Some(&token),
        json!({"operation":"thread.list","target":{"board_id":"missing-board"}}),
        json!({}),
    )
    .unwrap();
    assert_eq!(
        hidden_board_threads,
        json!({"allowed":false,"reason":"target_unavailable"})
    );
    assert_eq!(hidden_board_threads, missing_board_threads);
}

#[test]
fn create_dispatch_checks_readability_after_generating_the_real_target_id() {
    let (data, mut service, admin) = fixture();
    let project_root = tempfile::tempdir().unwrap();
    let project = project(&mut service, &admin, project_root.path(), "Project");
    let (_, token) = identity(&mut service, &project, None);
    let rules = "  - id: create-board\n    effect: allow\n    actions: [board.create]\n  - id: read-created-board\n    effect: allow\n    actions: [board.get]\n    when: { field: target.board_id, op: exists, value: true }\n  - id: create-thread\n    effect: allow\n    actions: [thread.create]\n  - id: read-created-thread\n    effect: allow\n    actions: [thread.get]\n    when: { field: target.thread_id, op: exists, value: true }";
    write_rules(data.path(), &header(rules));

    let board = call(
        &mut service,
        "board.create",
        Some(&token),
        json!({"name":"Created board"}),
    )
    .unwrap();
    let thread = call(
        &mut service,
        "thread.create",
        Some(&token),
        json!({"board_id":board["id"],"name":"Created thread"}),
    )
    .unwrap();
    assert_eq!(thread["board_id"], board["id"]);
}

#[test]
fn rules_check_configuration_failure_does_not_echo_private_source() {
    let (data, mut service, admin) = fixture();
    let project_root = tempfile::tempdir().unwrap();
    let project = project(&mut service, &admin, project_root.path(), "Project");
    let (_, thread) = board_and_thread(&mut service, &admin);
    let (_, token) = identity(&mut service, &project, None);
    let marker = "diagnostic-private-rules-source";
    std::fs::write(
        data.path().join("RULES.md"),
        format!("~~~swarm-rules\ninvalid: {marker}\n~~~\n"),
    )
    .unwrap();

    let error = request(
        &mut service,
        "rules.check",
        Some(&token),
        json!({"operation":"post.get","target":{"thread_id":thread["id"],"no":1}}),
        json!({}),
    )
    .unwrap_err();
    assert_eq!(error.code, "RULES_INVALID");
    assert!(!error.to_string().contains(marker));
    assert!(
        !error
            .to_string()
            .contains(data.path().to_string_lossy().as_ref())
    );
}
