use serde_json::{Value, json};
use swarm_core::{Service, protocol::Request};
#[test]
fn renaming_preserves_description_and_deleted_thread_restore_is_idempotent() {
    let (_dir, mut service, admin) = fixture();
    let board = call(
        &mut service,
        "board.create",
        Some(&admin),
        json!({"name":"Before","description":"Preserve this"}),
    )
    .unwrap();
    let board_id = board["id"].as_str().unwrap();
    let renamed = call(
        &mut service,
        "board.edit",
        Some(&admin),
        json!({"id":board_id,"name":"After","if_revision":1}),
    )
    .unwrap();
    assert_eq!(renamed["description"], "Preserve this");
    let topic = call(
        &mut service,
        "thread.create",
        Some(&admin),
        json!({"board_id":board_id,"name":"Topic"}),
    )
    .unwrap();
    let topic_id = topic["id"].as_str().unwrap();
    call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":topic_id,"body":"Preserved"}),
    )
    .unwrap();
    for _ in 0..2 {
        call(
            &mut service,
            "thread.delete",
            Some(&admin),
            json!({"id":topic_id}),
        )
        .unwrap();
    }
    assert!(
        call(
            &mut service,
            "post.list",
            Some(&admin),
            json!({"thread_id":topic_id})
        )
        .is_err()
    );
    for _ in 0..2 {
        call(
            &mut service,
            "thread.restore",
            Some(&admin),
            json!({"id":topic_id}),
        )
        .unwrap();
    }
    let topic = call(
        &mut service,
        "thread.get",
        Some(&admin),
        json!({"id":topic_id}),
    )
    .unwrap();
    assert_eq!(topic["revision"], 3);
    assert_eq!(
        call(
            &mut service,
            "post.get",
            Some(&admin),
            json!({"thread_id":topic_id,"no":1})
        )
        .unwrap()["body"],
        "Preserved"
    );
}
#[test]
fn editing_cannot_change_identity_number_or_reply_target() {
    let (_dir, mut service, admin) = fixture();
    let thread = thread(&mut service, &admin);
    call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread,"body":"Original"}),
    )
    .unwrap();
    for (field, value) in [
        ("reply_to", json!(3)),
        ("author_id", json!("forged")),
        ("display_id", json!("forged")),
        ("to", json!(3)),
    ] {
        let mut args = json!({"thread_id":thread,"no":1,"body":"Changed","if_revision":1});
        args[field] = value;
        assert_eq!(
            call(&mut service, "post.edit", Some(&admin), args)
                .unwrap_err()
                .exit_code,
            2
        );
    }
    let unchanged = call(
        &mut service,
        "post.get",
        Some(&admin),
        json!({"thread_id":thread,"no":1}),
    )
    .unwrap();
    assert_eq!(unchanged["body"], "Original");
    assert_eq!(unchanged["revision"], 1);
}

#[test]
fn password_token_and_request_debug_do_not_expose_secrets() {
    let (dir, _service, admin) = fixture();
    let db = rusqlite::Connection::open(dir.path().join("swarm.sqlite3")).unwrap();
    let hash: String = db
        .query_row("SELECT password_hash FROM accounts", [], |row| row.get(0))
        .unwrap();
    assert!(hash.starts_with("$argon2id$"));
    assert!(!hash.contains("correct horse"));
    let stored_token: String = db
        .query_row("SELECT token_hash FROM human_sessions", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_ne!(stored_token, admin);
    assert_eq!(stored_token.len(), 64);
    let request = Request {
        operation: "account.setup".into(),
        auth: Some(admin.clone()),
        args: json!({"name":"Admin","password":"secret-password","body":"secret-body"}),
        context: json!({"secret":"secret-context"}),
        request_id: None,
    };
    let debug = format!("{request:?}");
    for secret in [&admin, "secret-password", "secret-body", "secret-context"] {
        assert!(!debug.contains(secret));
    }
}

#[test]
fn persistent_history_and_events_commit_together_and_reject_history_rewrites() {
    let (dir, mut service, admin) = fixture();
    let thread = thread(&mut service, &admin);
    call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread,"body":"Version one"}),
    )
    .unwrap();
    call(
        &mut service,
        "post.edit",
        Some(&admin),
        json!({"thread_id":thread,"no":1,"body":"Version two","if_revision":1}),
    )
    .unwrap();
    assert!(
        call(
            &mut service,
            "post.edit",
            Some(&admin),
            json!({"thread_id":thread,"no":1,"body":"Failed","if_revision":1})
        )
        .is_err()
    );
    drop(service);
    let mut reopened = Service::open(dir.path()).unwrap();
    assert_eq!(
        call(
            &mut reopened,
            "post.get",
            Some(&admin),
            json!({"thread_id":thread,"no":1})
        )
        .unwrap()["body"],
        "Version two"
    );
    let history = call(
        &mut reopened,
        "post.history",
        Some(&admin),
        json!({"thread_id":thread,"no":1}),
    )
    .unwrap();
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
    assert_eq!(history["items"][0]["body"], "Version one");
    let db = rusqlite::Connection::open(dir.path().join("swarm.sqlite3")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM changes WHERE operation='post.edit'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert!(
        db.execute("UPDATE post_revisions SET body='Tampered'", [])
            .is_err()
    );
    assert!(db.execute("UPDATE posts SET no=99", []).is_err());
}
#[test]
fn parent_restore_preserves_independent_child_deletions() {
    let (_dir, mut service, admin) = fixture();
    let thread = thread(&mut service, &admin);
    let thread_info = call(
        &mut service,
        "thread.get",
        Some(&admin),
        json!({"id":thread}),
    )
    .unwrap();
    let board = thread_info["board_id"].as_str().unwrap();
    call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread,"body":"Keep"}),
    )
    .unwrap();
    call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread,"body":"Deleted"}),
    )
    .unwrap();
    call(
        &mut service,
        "post.delete",
        Some(&admin),
        json!({"thread_id":thread,"no":2}),
    )
    .unwrap();
    call(
        &mut service,
        "board.delete",
        Some(&admin),
        json!({"id":board}),
    )
    .unwrap();
    assert_eq!(
        call(
            &mut service,
            "post.get",
            Some(&admin),
            json!({"thread_id":thread,"no":1})
        )
        .unwrap_err()
        .exit_code,
        5
    );
    assert!(
        call(
            &mut service,
            "thread.create",
            Some(&admin),
            json!({"board_id":board,"name":"Blocked"})
        )
        .is_err()
    );
    call(
        &mut service,
        "board.restore",
        Some(&admin),
        json!({"id":board}),
    )
    .unwrap();
    assert_eq!(
        call(
            &mut service,
            "post.get",
            Some(&admin),
            json!({"thread_id":thread,"no":1})
        )
        .unwrap()["body"],
        "Keep"
    );
    let deleted = call(
        &mut service,
        "post.get",
        Some(&admin),
        json!({"thread_id":thread,"no":2}),
    )
    .unwrap();
    assert_eq!(deleted["deleted"], true);
    assert!(deleted["body"].is_null());
    call(
        &mut service,
        "post.restore",
        Some(&admin),
        json!({"thread_id":thread,"no":2}),
    )
    .unwrap();
    assert_eq!(
        call(
            &mut service,
            "post.get",
            Some(&admin),
            json!({"thread_id":thread,"no":2})
        )
        .unwrap()["body"],
        "Deleted"
    );
}

#[test]
fn rejected_body_and_reply_do_not_consume_numbers() {
    let (_dir, mut service, admin) = fixture();
    let thread = thread(&mut service, &admin);
    for text in [
        "",
        "   ",
        &"a".repeat(1024 * 1024 + 1),
        &"あ".repeat(349526),
    ] {
        assert_eq!(
            call(
                &mut service,
                "post.create",
                Some(&admin),
                json!({"thread_id":thread,"body":text})
            )
            .unwrap_err()
            .exit_code,
            2
        );
    }
    assert!(
        call(
            &mut service,
            "response.create",
            Some(&admin),
            json!({"thread_id":thread,"to":12,"body":"No target"})
        )
        .is_err()
    );
    let accepted = call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread,"body":"a".repeat(100_000)}),
    )
    .unwrap();
    assert_eq!(accepted["no"], 1);
}

#[test]
fn login_survives_reopen_and_explicit_logout_revokes_the_token() {
    let (dir, mut service, admin) = fixture();
    drop(service);
    service = Service::open(dir.path()).unwrap();
    assert!(call(&mut service, "board.list", Some(&admin), json!({})).is_ok());
    assert_eq!(
        call(&mut service, "board.list", Some("forged"), json!({}))
            .unwrap_err()
            .exit_code,
        3
    );
    assert_eq!(
        call(&mut service, "board.list", None, json!({}))
            .unwrap_err()
            .exit_code,
        3
    );
    assert_eq!(
        call(
            &mut service,
            "account.setup",
            None,
            json!({"name":"Other","password":"pw"})
        )
        .unwrap_err()
        .exit_code,
        6
    );
    call(&mut service, "account.logout", Some(&admin), json!({})).unwrap();
    assert_eq!(
        call(&mut service, "board.list", Some(&admin), json!({}))
            .unwrap_err()
            .exit_code,
        3
    );
    assert_eq!(
        call(
            &mut service,
            "account.login",
            None,
            json!({"name":"Admin","password":"wrong"})
        )
        .unwrap_err()
        .exit_code,
        3
    );
    let login = call(
        &mut service,
        "account.login",
        None,
        json!({"name":"Admin","password":"correct horse battery staple"}),
    )
    .unwrap();
    assert!(
        call(
            &mut service,
            "board.list",
            login["token"].as_str(),
            json!({})
        )
        .is_ok()
    );
    assert_ne!(login["token"], admin);
}

#[test]
fn retry_is_idempotent_and_cannot_replay_deleted_content() {
    let (_dir, mut service, admin) = fixture();
    let thread = thread(&mut service, &admin);
    let request = Request {
        operation: "post.create".into(),
        auth: Some(admin.clone()),
        args: json!({"thread_id":thread,"body":"Sensitive"}),
        context: json!({}),
        request_id: Some("retry-1".into()),
    };
    let first = service.request(request.clone()).unwrap();
    let repeat = service.request(request.clone()).unwrap();
    assert_eq!(first["id"], repeat["id"]);
    let mut mismatch = request.clone();
    mismatch.args["body"] = json!("Different");
    assert_eq!(service.request(mismatch).unwrap_err().exit_code, 6);
    call(
        &mut service,
        "post.delete",
        Some(&admin),
        json!({"thread_id":thread,"no":1}),
    )
    .unwrap();
    let repeated = service.request(request).unwrap();
    assert!(repeated["body"].is_null());
    assert_eq!(repeated["deleted"], true);
    assert_eq!(
        call(
            &mut service,
            "post.create",
            Some(&admin),
            json!({"thread_id":thread,"body":"Next"})
        )
        .unwrap()["no"],
        2
    );
}

#[test]
fn twenty_connections_allocate_unique_contiguous_numbers() {
    let (dir, mut service, admin) = fixture();
    let thread = thread(&mut service, &admin);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(20));
    let workers: Vec<_> = (0..20)
        .map(|index| {
            let root = dir.path().to_path_buf();
            let token = admin.clone();
            let thread = thread.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let mut service = Service::open(&root).unwrap();
                barrier.wait();
                call(
                    &mut service,
                    "post.create",
                    Some(&token),
                    json!({"thread_id":thread,"body":format!("Worker {index}")}),
                )
                .unwrap()["no"]
                    .as_u64()
                    .unwrap()
            })
        })
        .collect();
    let mut numbers: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    numbers.sort();
    assert_eq!(numbers, (1..=20).collect::<Vec<_>>());
    let all = call(
        &mut service,
        "post.list",
        Some(&admin),
        json!({"thread_id":thread}),
    )
    .unwrap();
    assert_eq!(all["items"].as_array().unwrap().len(), 20);
}

#[test]
fn pagination_returns_latest_fifty_in_order_without_gaps() {
    let (_dir, mut service, admin) = fixture();
    let thread = thread(&mut service, &admin);
    for no in 1..=55 {
        call(
            &mut service,
            "post.create",
            Some(&admin),
            json!({"thread_id":thread,"body":format!("{no}")}),
        )
        .unwrap();
    }
    let latest = call(
        &mut service,
        "post.list",
        Some(&admin),
        json!({"thread_id":thread}),
    )
    .unwrap();
    assert_eq!(latest["items"].as_array().unwrap().len(), 50);
    assert_eq!(latest["items"][0]["no"], 6);
    assert_eq!(latest["items"][49]["no"], 55);
    let earlier = call(
        &mut service,
        "post.list",
        Some(&admin),
        json!({"thread_id":thread,"before_no":latest["next_before_no"]}),
    )
    .unwrap();
    assert_eq!(earlier["items"].as_array().unwrap().len(), 5);
    assert_eq!(earlier["items"][0]["no"], 1);
    assert_eq!(earlier["items"][4]["no"], 5);
    assert!(earlier["next_before_no"].is_null());
    for limit in [0, 201, -1] {
        assert_eq!(
            call(
                &mut service,
                "post.list",
                Some(&admin),
                json!({"thread_id":thread,"limit":limit})
            )
            .unwrap_err()
            .exit_code,
            2
        );
    }
}

#[test]
fn restored_reply_counts_again_and_edited_author_snapshot_stays_fixed() {
    let (_dir, mut service, admin) = fixture();
    let thread = thread(&mut service, &admin);
    let original = call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread,"body":"Original"}),
    )
    .unwrap();
    call(
        &mut service,
        "response.create",
        Some(&admin),
        json!({"thread_id":thread,"to":1,"body":"Reply"}),
    )
    .unwrap();
    call(
        &mut service,
        "post.delete",
        Some(&admin),
        json!({"thread_id":thread,"no":2}),
    )
    .unwrap();
    call(
        &mut service,
        "post.restore",
        Some(&admin),
        json!({"thread_id":thread,"no":2}),
    )
    .unwrap();
    let edited = call(
        &mut service,
        "post.edit",
        Some(&admin),
        json!({"thread_id":thread,"no":1,"body":"Edited","if_revision":1}),
    )
    .unwrap();
    assert_eq!(edited["author"], original["author"]);
    assert_eq!(edited["response_count"], 1);
    assert_eq!(edited["created_at"], original["created_at"]);
    assert_eq!(edited["edited_by"], original["author"]["id"]);
    assert!(
        call(
            &mut service,
            "post.edit",
            Some(&admin),
            json!({"thread_id":thread,"no":1,"body":"Missing revision"})
        )
        .is_err()
    );
}

#[test]
fn newer_schema_is_rejected_without_rewriting_it() {
    let dir = tempfile::tempdir().unwrap();
    let db = rusqlite::Connection::open(dir.path().join("swarm.sqlite3")).unwrap();
    db.execute_batch("PRAGMA user_version=200;").unwrap();
    assert_eq!(
        Service::open(dir.path()).err().unwrap().code,
        "UNSUPPORTED_SCHEMA"
    );
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        200
    );
}

fn call(
    service: &mut Service,
    operation: &str,
    auth: Option<&str>,
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
fn fixture() -> (tempfile::TempDir, Service, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut service = Service::open(dir.path()).unwrap();
    let setup = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"correct horse battery staple"}),
    )
    .unwrap();
    let token = setup["token"].as_str().unwrap().to_string();
    (dir, service, token)
}
fn thread(service: &mut Service, admin: &str) -> String {
    let board = call(
        service,
        "board.create",
        Some(admin),
        json!({"name":"Reviews","description":"Review requests"}),
    )
    .unwrap();
    let thread = call(
        service,
        "thread.create",
        Some(admin),
        json!({"board_id":board["id"],"name":"Handover"}),
    )
    .unwrap();
    thread["id"].as_str().unwrap().to_string()
}
#[test]
fn posts_and_responses_share_numbers_and_only_direct_live_replies_count() {
    let (_dir, mut service, admin) = fixture();
    let thread = thread(&mut service, &admin);
    let one = call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread,"body":"First"}),
    )
    .unwrap();
    let two = call(
        &mut service,
        "response.create",
        Some(&admin),
        json!({"thread_id":thread,"to":1,"body":"Reply"}),
    )
    .unwrap();
    let three = call(
        &mut service,
        "response.create",
        Some(&admin),
        json!({"thread_id":thread,"to":2,"body":"Nested"}),
    )
    .unwrap();
    assert_eq!(
        (one["no"].as_u64(), two["no"].as_u64(), three["no"].as_u64()),
        (Some(1), Some(2), Some(3))
    );
    let fetched = call(
        &mut service,
        "post.get",
        Some(&admin),
        json!({"thread_id":thread,"no":1}),
    )
    .unwrap();
    assert_eq!(fetched["response_count"], 1);
    call(
        &mut service,
        "post.delete",
        Some(&admin),
        json!({"thread_id":thread,"no":2}),
    )
    .unwrap();
    assert_eq!(
        call(
            &mut service,
            "post.get",
            Some(&admin),
            json!({"thread_id":thread,"no":1})
        )
        .unwrap()["response_count"],
        0
    );
    assert!(
        call(
            &mut service,
            "response.create",
            Some(&admin),
            json!({"thread_id":thread,"to":2,"body":"Not allowed"})
        )
        .is_err()
    );
    let next = call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread,"body":"Next"}),
    )
    .unwrap();
    assert_eq!(next["no"], 4);
}
#[test]
fn stale_revision_cannot_overwrite_an_edit_and_history_keeps_the_original() {
    let (_dir, mut service, admin) = fixture();
    let thread = thread(&mut service, &admin);
    call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread,"body":"Original"}),
    )
    .unwrap();
    let edited = call(
        &mut service,
        "post.edit",
        Some(&admin),
        json!({"thread_id":thread,"no":1,"body":"Changed","if_revision":1}),
    )
    .unwrap();
    assert_eq!(edited["revision"], 2);
    let conflict = call(
        &mut service,
        "post.edit",
        Some(&admin),
        json!({"thread_id":thread,"no":1,"body":"Stale","if_revision":1}),
    )
    .unwrap_err();
    assert_eq!(conflict.exit_code, 6);
    let history = call(
        &mut service,
        "post.history",
        Some(&admin),
        json!({"thread_id":thread,"no":1}),
    )
    .unwrap();
    assert_eq!(history["items"][0]["body"], "Original");
}
#[test]
fn reply_target_must_be_in_the_same_thread() {
    let (_dir, mut service, admin) = fixture();
    let a = thread(&mut service, &admin);
    let b = thread(&mut service, &admin);
    call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":a,"body":"A"}),
    )
    .unwrap();
    let result = call(
        &mut service,
        "response.create",
        Some(&admin),
        json!({"thread_id":b,"to":1,"body":"Invalid"}),
    );
    assert!(result.is_err());
}
