use serde_json::{Value, json};
use std::time::Instant;
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

fn permission(
    service: &mut Service,
    admin: &str,
    account_id: &str,
    operation: &str,
    scope: &str,
    target_id: &str,
    decision: &str,
) {
    call(
        service,
        Some(admin),
        "permission.set",
        json!({"subject_type":"account","subject_id":account_id,"operation":operation,
            "scope":scope,"target_id":target_id,"decision":decision}),
    )
    .unwrap();
}

#[test]
fn body_and_name_search_are_separate_and_name_search_excludes_description() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"search-fixture-password"}),
    )
    .unwrap();
    let auth = admin["token"].as_str().unwrap();
    let board = call(
        &mut service,
        Some(auth),
        "board.create",
        json!({"name":"共通 needle board","description":"description-only"}),
    )
    .unwrap();
    let thread = call(&mut service, Some(auth), "thread.create", json!({"board_id":board["id"],"name":"共通 needle thread","description":"description-only"})).unwrap();
    call(
        &mut service,
        Some(auth),
        "post.create",
        json!({"thread_id":thread["id"],"body":"共通 needle post"}),
    )
    .unwrap();
    call(
        &mut service,
        Some(auth),
        "response.create",
        json!({"thread_id":thread["id"],"to":1,"body":"共通 needle response"}),
    )
    .unwrap();
    for (scope, kinds) in [
        ("body", vec!["post", "response"]),
        ("board_name", vec!["board"]),
        ("thread_name", vec!["thread"]),
    ] {
        let page = call(
            &mut service,
            Some(auth),
            "search",
            json!({"query":"共通 needle","scope":scope}),
        )
        .unwrap();
        let items = page["items"].as_array().unwrap();
        assert_eq!(items.len(), kinds.len());
        for kind in kinds {
            assert!(items.iter().any(|item| item["kind"] == kind));
        }
        let empty = call(
            &mut service,
            Some(auth),
            "search",
            json!({"query":"description-only","scope":scope}),
        )
        .unwrap();
        assert!(empty["items"].as_array().unwrap().is_empty());
    }
    let page = call(
        &mut service,
        Some(auth),
        "search",
        json!({"query":"needle","scope":"body","limit":1}),
    )
    .unwrap();
    assert!(
        call(
            &mut service,
            Some(auth),
            "search",
            json!({"query":"needle","scope":"thread_name","limit":1,"cursor":page["next_cursor"]})
        )
        .is_err()
    );
}

#[test]
fn search_matches_current_japanese_and_english_content_and_never_history_or_tombstones() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"search-fixture-password"}),
    )
    .unwrap();
    let admin_token = admin["token"].as_str().unwrap();
    let board = call(
        &mut service,
        Some(admin_token),
        "board.create",
        json!({"name":"alpha % board","description":"shared 100% metadata"}),
    )
    .unwrap();
    let thread = call(
        &mut service,
        Some(admin_token),
        "thread.create",
        json!({"board_id":board["id"],"name":"conversation","description":"beta metadata"}),
    )
    .unwrap();
    call(
        &mut service,
        Some(admin_token),
        "post.create",
        json!({"thread_id":thread["id"],"body":"旧版 retired secret"}),
    )
    .unwrap();
    call(
        &mut service,
        Some(admin_token),
        "post.edit",
        json!({"thread_id":thread["id"],"no":1,"body":"現在の日本語 Current needle","if_revision":1}),
    )
    .unwrap();
    call(
        &mut service,
        Some(admin_token),
        "response.create",
        json!({"thread_id":thread["id"],"body":"日本語 current response needle","to":1}),
    )
    .unwrap();
    call(
        &mut service,
        Some(admin_token),
        "post.create",
        json!({"thread_id":thread["id"],"body":"deleted tombstone needle"}),
    )
    .unwrap();
    call(
        &mut service,
        Some(admin_token),
        "post.delete",
        json!({"thread_id":thread["id"],"no":3}),
    )
    .unwrap();

    let both_terms = call(
        &mut service,
        Some(admin_token),
        "search",
        json!({"query":"CURRENT 日本語","scope":"post"}),
    )
    .unwrap();
    assert_eq!(both_terms["items"].as_array().unwrap().len(), 1);
    assert_eq!(both_terms["items"][0]["post_no"], 1);
    assert!(
        both_terms["items"][0]["excerpt"]
            .as_str()
            .unwrap()
            .contains("Current")
    );
    assert_eq!(
        call(
            &mut service,
            Some(admin_token),
            "search",
            json!({"query":"retired","scope":"post"}),
        )
        .unwrap()["items"],
        json!([])
    );
    assert_eq!(
        call(
            &mut service,
            Some(admin_token),
            "search",
            json!({"query":"tombstone","scope":"all"}),
        )
        .unwrap()["items"],
        json!([])
    );
    assert_eq!(
        call(
            &mut service,
            Some(admin_token),
            "search",
            json!({"query":"alpha %","scope":"board"}),
        )
        .unwrap()["items"][0]["id"],
        board["id"]
    );
    assert_eq!(
        call(
            &mut service,
            Some(admin_token),
            "search",
            json!({"query":"alpha shared","scope":"board"}),
        )
        .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "every whitespace-separated keyword must match"
    );
    assert_eq!(
        call(
            &mut service,
            Some(admin_token),
            "search",
            json!({"query":"alpha missing","scope":"board"}),
        )
        .unwrap()["items"],
        json!([])
    );
    let post_only = call(
        &mut service,
        Some(admin_token),
        "search",
        json!({"query":"needle","author_id":admin["id"],"board_id":board["id"]}),
    )
    .unwrap();
    assert_eq!(post_only["items"].as_array().unwrap().len(), 2);
    assert!(
        post_only["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|item| { matches!(item["kind"].as_str(), Some("post" | "response")) })
    );
}

#[test]
fn search_cursor_is_filtered_signed_actor_bound_and_rechecks_permissions() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"search-cursor-admin"}),
    )
    .unwrap();
    let admin_token = admin["token"].as_str().unwrap().to_owned();
    let board = call(
        &mut service,
        Some(&admin_token),
        "board.create",
        json!({"name":"Search board","description":""}),
    )
    .unwrap();
    let thread = call(
        &mut service,
        Some(&admin_token),
        "thread.create",
        json!({"board_id":board["id"],"name":"Search thread","description":""}),
    )
    .unwrap();
    for index in 0..3 {
        call(
            &mut service,
            Some(&admin_token),
            "post.create",
            json!({"thread_id":thread["id"],"body":format!("pageword {index}")}),
        )
        .unwrap();
    }
    let member = call(
        &mut service,
        Some(&admin_token),
        "account.create",
        json!({"name":"Member","password":"search-cursor-member"}),
    )
    .unwrap();
    let member_login = call(
        &mut service,
        None,
        "account.login",
        json!({"name":"Member","password":"search-cursor-member"}),
    )
    .unwrap();
    let member_token = member_login["token"].as_str().unwrap().to_owned();

    let first = call(
        &mut service,
        Some(&admin_token),
        "search",
        json!({"query":"pageword","scope":"post","limit":1}),
    )
    .unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    assert_eq!(first["total_count"], 3);
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();
    let second = call(
        &mut service,
        Some(&admin_token),
        "search",
        json!({"query":"pageword","scope":"post","limit":1,"cursor":cursor}),
    )
    .unwrap();
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["total_count"], 3);
    assert_ne!(first["items"][0]["id"], second["items"][0]["id"]);

    let mut tampered = first["next_cursor"].as_str().unwrap().to_owned();
    tampered.push('0');
    assert_eq!(
        call(
            &mut service,
            Some(&admin_token),
            "search",
            json!({"query":"pageword","scope":"post","limit":1,"cursor":tampered}),
        )
        .unwrap_err()
        .exit_code,
        2
    );
    assert_eq!(
        call(
            &mut service,
            Some(&admin_token),
            "search",
            json!({"query":"different","scope":"post","limit":1,"cursor":first["next_cursor"]}),
        )
        .unwrap_err()
        .exit_code,
        2
    );
    assert_eq!(
        call(
            &mut service,
            Some(&member_token),
            "search",
            json!({"query":"pageword","scope":"post","limit":1,"cursor":first["next_cursor"]}),
        )
        .unwrap_err()
        .exit_code,
        2,
        "cursor fingerprint is bound to the human actor"
    );

    let member_first = call(
        &mut service,
        Some(&member_token),
        "search",
        json!({"query":"pageword","scope":"post","limit":1}),
    )
    .unwrap();
    assert_eq!(member_first["items"][0]["board_id"], board["id"]);
    let member_cursor = member_first["next_cursor"].as_str().unwrap().to_owned();
    permission(
        &mut service,
        &admin_token,
        member["id"].as_str().unwrap(),
        "search",
        "board",
        board["id"].as_str().unwrap(),
        "deny",
    );
    let after_revoke = call(
        &mut service,
        Some(&member_token),
        "search",
        json!({"query":"pageword","scope":"post","limit":1,"cursor":member_cursor}),
    )
    .unwrap();
    assert_eq!(after_revoke["items"], json!([]));
    assert_eq!(after_revoke["total_count"], 0);
    assert_eq!(after_revoke["next_cursor"], Value::Null);
}

#[test]
fn search_redacts_hidden_parent_and_validates_filters_without_leaking_counts() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"search-parent-admin"}),
    )
    .unwrap();
    let admin_token = admin["token"].as_str().unwrap().to_owned();
    let board = call(
        &mut service,
        Some(&admin_token),
        "board.create",
        json!({"name":"Secret parent board","description":""}),
    )
    .unwrap();
    let thread = call(
        &mut service,
        Some(&admin_token),
        "thread.create",
        json!({"board_id":board["id"],"name":"Visible child","description":""}),
    )
    .unwrap();
    call(
        &mut service,
        Some(&admin_token),
        "post.create",
        json!({"thread_id":thread["id"],"body":"findable content"}),
    )
    .unwrap();
    let member = call(
        &mut service,
        Some(&admin_token),
        "account.create",
        json!({"name":"Member","password":"search-parent-member"}),
    )
    .unwrap();
    let member_login = call(
        &mut service,
        None,
        "account.login",
        json!({"name":"Member","password":"search-parent-member"}),
    )
    .unwrap();
    let member_token = member_login["token"].as_str().unwrap();
    permission(
        &mut service,
        &admin_token,
        member["id"].as_str().unwrap(),
        "board.get",
        "board",
        board["id"].as_str().unwrap(),
        "deny",
    );
    let result = call(
        &mut service,
        Some(member_token),
        "search",
        json!({"query":"findable","thread_id":thread["id"]}),
    )
    .unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    assert_eq!(result["items"][0]["board_id"], Value::Null);
    assert_eq!(result["items"][0]["thread_id"], thread["id"]);
    assert_eq!(result["items"][0]["name"], "Visible child");
    assert_eq!(
        call(
            &mut service,
            Some(member_token),
            "search",
            json!({"query":"Visible","board_id":board["id"]}),
        )
        .unwrap()["items"],
        json!([]),
        "a guessed hidden Board ID must not reveal whether a private child matches"
    );
    assert_eq!(
        call(
            &mut service,
            Some(member_token),
            "search",
            json!({"query":"Visible"}),
        )
        .unwrap()["items"][0]["board_id"],
        Value::Null,
        "unscoped search must retain individually readable children with parent redaction"
    );
    assert_eq!(
        call(
            &mut service,
            Some(member_token),
            "search",
            json!({"query":"findable","thread_id":thread["id"],"unexpected":1}),
        )
        .unwrap_err()
        .exit_code,
        2
    );
    assert_eq!(
        call(
            &mut service,
            Some(member_token),
            "search",
            json!({"query":"findable","scope":"identity"}),
        )
        .unwrap_err()
        .exit_code,
        2
    );
    assert_eq!(
        call(
            &mut service,
            Some(member_token),
            "search",
            json!({"query":"findable","from":"tomorrow"}),
        )
        .unwrap_err()
        .exit_code,
        2
    );
    assert_eq!(
        call(
            &mut service,
            Some(member_token),
            "search",
            json!({"query":"findable","limit":0}),
        )
        .unwrap_err()
        .exit_code,
        2
    );
}

#[test]
fn bounded_search_observation_on_one_thousand_current_posts() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"search-measurement-fixture"}),
    )
    .unwrap();
    let token = admin["token"].as_str().unwrap();
    let board = call(
        &mut service,
        Some(token),
        "board.create",
        json!({"name":"Measurement board","description":""}),
    )
    .unwrap();
    let thread = call(
        &mut service,
        Some(token),
        "thread.create",
        json!({"board_id":board["id"],"name":"Measurement thread","description":""}),
    )
    .unwrap();
    for index in 0..1000 {
        call(
            &mut service,
            Some(token),
            "post.create",
            json!({
                "thread_id":thread["id"],
                "body":format!("measured needle 日本語 entry {index}")
            }),
        )
        .unwrap();
    }

    // This is a bounded observation at the core Service API boundary, without
    // logging the query, content, account, or authentication data.
    let started = Instant::now();
    let page = call(
        &mut service,
        Some(token),
        "search",
        json!({"query":"needle 日本語","scope":"post","limit":50}),
    )
    .unwrap();
    let elapsed_ms = started.elapsed().as_millis();
    let item_count = page["items"].as_array().unwrap().len();
    assert_eq!(item_count, 50);
    assert!(page["next_cursor"].is_string());
    println!(
        "search observation: current_posts=1000 returned_items={item_count} service_elapsed_ms={elapsed_ms}"
    );
}
