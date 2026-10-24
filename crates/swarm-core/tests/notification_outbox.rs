use serde_json::{Value, json};
use swarm_core::{CredentialCandidate, NotificationKind, Service, protocol::Request};

fn call(service: &mut Service, operation: &str, auth: Option<&str>, args: Value) -> Value {
    service
        .request(Request {
            operation: operation.into(),
            auth: auth.map(str::to_owned),
            args,
            context: json!({}),
            request_id: None,
        })
        .unwrap()
}

struct Fixture {
    data: tempfile::TempDir,
    service: Service,
    admin_id: String,
    admin: String,
    candidate: CredentialCandidate,
    thread: String,
}

fn fixture() -> Fixture {
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let grant = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"correct horse battery staple"}),
    );
    let admin = grant["token"].as_str().unwrap().to_owned();
    let admin_id = grant["id"].as_str().unwrap().to_owned();
    let candidate = CredentialCandidate::from_grant("account.setup", &grant).unwrap();
    let board = call(
        &mut service,
        "board.create",
        Some(&admin),
        json!({"name":"Board"}),
    );
    let thread = call(
        &mut service,
        "thread.create",
        Some(&admin),
        json!({"board_id":board["id"],"name":"Topic"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    Fixture {
        data,
        service,
        admin_id,
        admin,
        candidate,
        thread,
    }
}

fn subscribe(f: &mut Fixture) {
    call(
        &mut f.service,
        "thread.subscribe",
        Some(&f.admin),
        json!({"id":f.thread,"subscribed":true}),
    );
}

#[test]
fn delivery_rechecks_display_language_without_changing_batch_or_unread() {
    let mut f = fixture();
    subscribe(&mut f);
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"new"}),
    );
    let now = chrono::Utc::now().timestamp_millis() + 3000;
    let first = f
        .service
        .prepare_notification_batch(&f.candidate, now, 2000)
        .unwrap()
        .unwrap();
    assert_eq!(first.language, None);
    for language in [
        swarm_core::DisplayLanguage::Ja,
        swarm_core::DisplayLanguage::En,
    ] {
        call(
            &mut f.service,
            "display.settings.set",
            Some(&f.admin),
            json!({"language":language,"theme":"dark"}),
        );
        let updated = f
            .service
            .prepare_notification_batch(&f.candidate, now, 2000)
            .unwrap()
            .unwrap();
        assert_eq!(updated.id, first.id);
        assert_eq!(updated.deliveries, first.deliveries);
        assert_eq!(updated.language, Some(language));
    }
    assert_eq!(
        call(
            &mut f.service,
            "conversation.unread",
            Some(&f.admin),
            json!({})
        )["total"],
        1
    );
}

#[test]
fn aggregation_waits_then_freezes_membership_across_restart() {
    let mut f = fixture();
    subscribe(&mut f);
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    let now = chrono::Utc::now().timestamp_millis();
    for body in ["first", "second"] {
        call(
            &mut f.service,
            "post.create",
            Some(&f.admin),
            json!({"thread_id":f.thread,"body":body}),
        );
    }
    assert!(
        f.service
            .prepare_notification_batch(&f.candidate, now, 2000)
            .unwrap()
            .is_none()
    );
    let batch = f
        .service
        .prepare_notification_batch(&f.candidate, now + 3000, 2000)
        .unwrap()
        .unwrap();
    assert_eq!(batch.deliveries.len(), 2);
    assert!(batch.deliveries.iter().all(|item| item.excerpt.is_none()));
    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"later"}),
    );
    drop(f.service);
    let mut service = Service::open(f.data.path()).unwrap();
    assert_eq!(
        service
            .prepare_notification_batch(&f.candidate, now + 4000, 2000)
            .unwrap()
            .unwrap(),
        batch
    );
}

#[test]
fn aggregation_retry_is_durable_and_ack_is_atomic_without_reading_posts() {
    let mut f = fixture();
    subscribe(&mut f);
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"secret"}),
    );
    let now = chrono::Utc::now().timestamp_millis() + 3000;
    let batch = f
        .service
        .prepare_notification_batch(&f.candidate, now, 2000)
        .unwrap()
        .unwrap();
    f.service
        .complete_notification_batch(&f.candidate, &batch.id, now, false)
        .unwrap();
    drop(f.service);
    f.service = Service::open(f.data.path()).unwrap();
    assert!(
        f.service
            .prepare_notification_batch(&f.candidate, now + 4999, 2000)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        f.service
            .prepare_notification_batch(&f.candidate, now + 5000, 2000)
            .unwrap()
            .unwrap(),
        batch
    );
    let db = rusqlite::Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_notification_ack BEFORE DELETE ON notification_outbox BEGIN SELECT RAISE(ABORT, 'controlled receipt failure'); END;").unwrap();
    assert!(
        f.service
            .complete_notification_batch(&f.candidate, &batch.id, now + 5000, true)
            .is_err()
    );
    db.execute_batch("DROP TRIGGER reject_notification_ack;")
        .unwrap();
    assert_eq!(
        f.service
            .prepare_notification_batch(&f.candidate, now + 5000, 2000)
            .unwrap()
            .unwrap(),
        batch
    );
    f.service
        .complete_notification_batch(&f.candidate, &batch.id, now + 5000, true)
        .unwrap();
    assert!(
        f.service
            .prepare_notification_batch(&f.candidate, now + 9000, 2000)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        call(
            &mut f.service,
            "conversation.unread",
            Some(&f.admin),
            json!({})
        )["total"],
        1
    );
}

#[test]
fn aggregation_rechecks_read_receipts_settings_and_cancels_on_selection_change() {
    let mut f = fixture();
    subscribe(&mut f);
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    let mut ids = Vec::new();
    for body in ["first", "second"] {
        ids.push(
            call(
                &mut f.service,
                "post.create",
                Some(&f.admin),
                json!({"thread_id":f.thread,"body":body}),
            )["id"]
                .clone(),
        );
    }
    let now = chrono::Utc::now().timestamp_millis() + 3000;
    let first = f
        .service
        .prepare_notification_batch(&f.candidate, now, 2000)
        .unwrap()
        .unwrap();
    call(
        &mut f.service,
        "post.read.mark",
        Some(&f.admin),
        json!({"thread_id":f.thread,"post_ids":[ids[0]]}),
    );
    let remaining = f
        .service
        .prepare_notification_batch(&f.candidate, now, 2000)
        .unwrap()
        .unwrap();
    assert_eq!(remaining.id, first.id);
    assert_eq!(remaining.deliveries.len(), 1);
    call(
        &mut f.service,
        "notification.settings.set",
        Some(&f.admin),
        json!({"enabled":false,"include_excerpt":false}),
    );
    assert!(
        f.service
            .prepare_notification_batch(&f.candidate, now, 2000)
            .unwrap()
            .is_none()
    );
    f.service.clear_notification_recipient().unwrap();
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    assert!(
        f.service
            .prepare_notification_batch(&f.candidate, now, 2000)
            .unwrap()
            .is_none()
    );
}

#[test]
fn aggregation_keeps_click_target_to_one_thread_and_separates_later_window() {
    let mut f = fixture();
    subscribe(&mut f);
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    let board = call(
        &mut f.service,
        "board.create",
        Some(&f.admin),
        json!({"name":"Other"}),
    );
    let other = call(
        &mut f.service,
        "thread.create",
        Some(&f.admin),
        json!({"board_id":board["id"],"name":"Other"}),
    )["id"]
        .clone();
    call(
        &mut f.service,
        "thread.subscribe",
        Some(&f.admin),
        json!({"id":other,"subscribed":true}),
    );
    let first = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"First"}),
    );
    let later = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"Later"}),
    );
    let now = chrono::Utc::now().timestamp_millis();
    let db = rusqlite::Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    let timestamp = |ms| {
        chrono::DateTime::from_timestamp_millis(ms)
            .unwrap()
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    };
    db.execute(
        "UPDATE notification_outbox SET created_at=?1 WHERE post_id=?2",
        rusqlite::params![timestamp(now), first["id"].as_str().unwrap()],
    )
    .unwrap();
    db.execute(
        "UPDATE notification_outbox SET created_at=?1 WHERE post_id=?2",
        rusqlite::params![timestamp(now + 5000), later["id"].as_str().unwrap()],
    )
    .unwrap();
    assert!(
        f.service
            .prepare_notification_batch(&f.candidate, now, 2000)
            .unwrap()
            .is_none()
    );
    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":other,"body":"Other Thread"}),
    );
    let batch = f
        .service
        .prepare_notification_batch(&f.candidate, now + 9000, 2000)
        .unwrap()
        .unwrap();
    assert_eq!(batch.deliveries.len(), 1);
    assert_eq!(batch.deliveries[0].post_id, first["id"].as_str().unwrap());
    f.service
        .complete_notification_batch(&f.candidate, &batch.id, now + 9000, true)
        .unwrap();
    let second = f
        .service
        .prepare_notification_batch(&f.candidate, now + 9000, 2000)
        .unwrap()
        .unwrap();
    assert!(
        second
            .deliveries
            .iter()
            .all(|d| d.thread_id == second.deliveries[0].thread_id)
    );
    assert_eq!(second.deliveries.len(), 1);
    f.service
        .complete_notification_batch(&f.candidate, &second.id, now + 9000, true)
        .unwrap();
    assert_eq!(
        f.service
            .prepare_notification_batch(&f.candidate, now + 9000, 2000)
            .unwrap()
            .unwrap()
            .deliveries
            .len(),
        1
    );
}

#[test]
fn aggregation_caps_at_200_and_retry_delay_caps_at_five_minutes() {
    let mut f = fixture();
    subscribe(&mut f);
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    for _ in 0..201 {
        call(
            &mut f.service,
            "post.create",
            Some(&f.admin),
            json!({"thread_id":f.thread,"body":"New"}),
        );
    }
    let mut now = chrono::Utc::now().timestamp_millis();
    let batch = f
        .service
        .prepare_notification_batch(&f.candidate, now, 60000)
        .unwrap()
        .unwrap();
    assert_eq!(batch.deliveries.len(), 200);
    for delay in [5000, 10000, 20000, 40000, 80000, 160000, 300000, 300000] {
        f.service
            .complete_notification_batch(&f.candidate, &batch.id, now, false)
            .unwrap();
        assert!(
            f.service
                .prepare_notification_batch(&f.candidate, now + delay - 1, 60000)
                .unwrap()
                .is_none()
        );
        now += delay;
        assert_eq!(
            f.service
                .prepare_notification_batch(&f.candidate, now, 60000)
                .unwrap()
                .unwrap()
                .id,
            batch.id
        );
    }
    f.service
        .complete_notification_batch(&f.candidate, &batch.id, now, true)
        .unwrap();
    assert_eq!(
        f.service
            .prepare_notification_batch(&f.candidate, now, 60000)
            .unwrap()
            .unwrap()
            .deliveries
            .len(),
        1
    );
}

#[test]
fn selection_persists_idempotently_and_only_new_posts_enter_pending_outbox() {
    let mut f = fixture();
    subscribe(&mut f);
    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"Before selection"}),
    );
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    assert!(
        f.service
            .prepare_notifications(&f.candidate, 20)
            .unwrap()
            .is_empty()
    );

    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"After selection"}),
    );
    let first = f.service.prepare_notifications(&f.candidate, 20).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].account_id, f.admin_id);
    assert_eq!(first[0].kind, NotificationKind::Post);
    assert!(first[0].excerpt.is_none());
    assert!(!format!("{first:?}").contains("After selection"));
    let id = first[0].id.clone();

    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    drop(f.service);
    let mut reopened = Service::open(f.data.path()).unwrap();
    let again = reopened.prepare_notifications(&f.candidate, 20).unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].id, id);
    reopened
        .acknowledge_notification(&f.candidate, &id)
        .unwrap();
    reopened
        .acknowledge_notification(&f.candidate, &id)
        .unwrap();
    assert!(
        reopened
            .prepare_notifications(&f.candidate, 20)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn changing_recipient_cancels_old_pending_and_stale_generation_cannot_ack() {
    let mut f = fixture();
    subscribe(&mut f);
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"Pending"}),
    );
    let pending = f.service.prepare_notifications(&f.candidate, 20).unwrap();
    assert_eq!(pending.len(), 1);

    let second = call(
        &mut f.service,
        "account.create",
        Some(&f.admin),
        json!({"name":"Second","password":"second-password"}),
    );
    let login = call(
        &mut f.service,
        "account.login",
        None,
        json!({"name":"Second","password":"second-password"}),
    );
    let second_candidate = CredentialCandidate::from_grant("account.login", &login).unwrap();
    f.service
        .select_notification_recipient(&second_candidate)
        .unwrap();
    assert!(
        f.service
            .prepare_notifications(&second_candidate, 20)
            .unwrap()
            .is_empty()
    );
    assert!(
        f.service
            .acknowledge_notification(&f.candidate, &pending[0].id)
            .is_err()
    );
    assert_ne!(second["id"], f.admin_id);
}

#[test]
fn disabling_or_losing_eligibility_suppresses_and_discards_pending() {
    let mut f = fixture();
    subscribe(&mut f);
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"Do not deliver"}),
    );
    call(
        &mut f.service,
        "notification.settings.set",
        Some(&f.admin),
        json!({"enabled":false,"include_excerpt":true}),
    );
    assert!(
        f.service
            .prepare_notifications(&f.candidate, 20)
            .unwrap()
            .is_empty()
    );
    call(
        &mut f.service,
        "notification.settings.set",
        Some(&f.admin),
        json!({"enabled":true,"include_excerpt":true}),
    );
    assert!(
        f.service
            .prepare_notifications(&f.candidate, 20)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn direct_responses_are_deduplicated_and_excerpt_is_opt_in_and_bounded() {
    let mut f = fixture();
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    let parent = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"Parent post"}),
    );
    assert!(
        f.service
            .prepare_notifications(&f.candidate, 20)
            .unwrap()
            .is_empty()
    );
    let body = "界".repeat(400);
    call(
        &mut f.service,
        "response.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"to":parent["no"],"body":body}),
    );
    call(
        &mut f.service,
        "notification.settings.set",
        Some(&f.admin),
        json!({"enabled":true,"include_excerpt":true}),
    );
    let deliveries = f.service.prepare_notifications(&f.candidate, 20).unwrap();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].kind, NotificationKind::Response);
    let excerpt = deliveries[0].excerpt.as_ref().unwrap();
    assert_eq!(excerpt.chars().count(), 241);
    assert!(excerpt.ends_with('…'));
    assert_eq!(excerpt.chars().filter(|ch| *ch == '界').count(), 240);
    assert!(!format!("{deliveries:?}").contains('界'));
}

#[test]
fn ai_credentials_are_rejected_and_post_creation_replay_does_not_duplicate_delivery() {
    let mut f = fixture();
    subscribe(&mut f);
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    let ai = CredentialCandidate::Ai {
        id: "ai".into(),
        project_id: "project".into(),
        session_id: "session".into(),
        session_token: "token".into(),
        resume_secret: "resume".into(),
    };
    assert!(f.service.select_notification_recipient(&ai).is_err());
    let request = Request {
        operation: "post.create".into(),
        auth: Some(f.admin.clone()),
        args: json!({"thread_id":f.thread,"body":"request replay"}),
        context: json!({}),
        request_id: Some("same-post".into()),
    };
    let created = f.service.request(request.clone()).unwrap();
    let replayed = f.service.request(request).unwrap();
    assert_eq!(created["id"], replayed["id"]);
    assert_eq!(
        f.service
            .prepare_notifications(&f.candidate, 20)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn prepare_rechecks_current_permissions_and_discards_disallowed_targets() {
    let mut f = fixture();
    let reader = call(
        &mut f.service,
        "account.create",
        Some(&f.admin),
        json!({"name":"Reader","password":"reader-password"}),
    );
    let login = call(
        &mut f.service,
        "account.login",
        None,
        json!({"name":"Reader","password":"reader-password"}),
    );
    let reader_id = reader["id"].as_str().unwrap().to_owned();
    let reader_token = login["token"].as_str().unwrap().to_owned();
    let candidate = CredentialCandidate::from_grant("account.login", &login).unwrap();
    for operation in [
        "board.list",
        "board.get",
        "thread.list",
        "thread.get",
        "post.list",
        "post.get",
    ] {
        call(
            &mut f.service,
            "permission.set",
            Some(&f.admin),
            json!({
            "subject_type":"account","subject_id":reader_id,"operation":operation,
            "scope":"global","target_id":"","decision":"allow"}),
        );
    }
    call(
        &mut f.service,
        "thread.subscribe",
        Some(&reader_token),
        json!({"id":f.thread,"subscribed":true}),
    );
    f.service.select_notification_recipient(&candidate).unwrap();
    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"permission hidden"}),
    );
    call(
        &mut f.service,
        "permission.set",
        Some(&f.admin),
        json!({
        "subject_type":"account","subject_id":reader_id,"operation":"post.get",
        "scope":"global","target_id":"","decision":"deny"}),
    );
    assert!(
        f.service
            .prepare_notifications(&candidate, 20)
            .unwrap()
            .is_empty()
    );
    call(
        &mut f.service,
        "permission.set",
        Some(&f.admin),
        json!({
        "subject_type":"account","subject_id":reader_id,"operation":"post.get",
        "scope":"global","target_id":"","decision":"allow"}),
    );
    assert!(
        f.service
            .prepare_notifications(&candidate, 20)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn prepare_discards_a_post_deleted_after_enqueue() {
    let mut f = fixture();
    subscribe(&mut f);
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    let post = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"delete before prepare"}),
    );
    call(
        &mut f.service,
        "post.delete",
        Some(&f.admin),
        json!({"thread_id":f.thread,"no":post["no"]}),
    );
    assert!(
        f.service
            .prepare_notifications(&f.candidate, 20)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn clear_cancels_pending_and_revoked_session_cannot_prepare() {
    let mut f = fixture();
    subscribe(&mut f);
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"cancel me"}),
    );
    f.service.clear_notification_recipient().unwrap();
    assert!(f.service.prepare_notifications(&f.candidate, 20).is_err());
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    call(&mut f.service, "account.logout", Some(&f.admin), json!({}));
    assert_eq!(
        f.service
            .prepare_notifications(&f.candidate, 20)
            .unwrap_err()
            .code,
        "AUTHENTICATION_REQUIRED"
    );
}

#[test]
fn outbox_insert_rolls_back_with_post_and_schema_is_version_nine() {
    let mut f = fixture();
    subscribe(&mut f);
    f.service
        .select_notification_recipient(&f.candidate)
        .unwrap();
    let db = rusqlite::Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER fail_notification BEFORE INSERT ON notification_outbox \
        BEGIN SELECT RAISE(ABORT,'blocked'); END;",
    )
    .unwrap();
    assert!(
        f.service
            .request(Request {
                operation: "post.create".into(),
                auth: Some(f.admin.clone()),
                args: json!({"thread_id":f.thread,"body":"must rollback"}),
                context: json!({}),
                request_id: None,
            })
            .is_err()
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM posts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM notification_outbox", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM changes WHERE operation='post.create'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT next_no FROM threads WHERE id=?1",
            [&f.thread],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        9
    );
}

#[test]
fn version_six_upgrade_preserves_sparse_reads_subscription_and_settings() {
    let mut f = fixture();
    subscribe(&mut f);
    let post = call(
        &mut f.service,
        "post.create",
        Some(&f.admin),
        json!({"thread_id":f.thread,"body":"Existing"}),
    );
    call(
        &mut f.service,
        "post.read.mark",
        Some(&f.admin),
        json!({"thread_id":f.thread,"post_ids":[post["id"]]}),
    );
    call(
        &mut f.service,
        "notification.settings.set",
        Some(&f.admin),
        json!({"enabled":false,"include_excerpt":true}),
    );
    drop(f.service);
    let db = rusqlite::Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    db.execute_batch(
        "DROP TABLE notification_outbox; DROP TABLE notification_selection;
         DROP TRIGGER posts_purged_body_immutable; DROP INDEX post_revisions_saved_at;
         ALTER TABLE posts DROP COLUMN body_purged_at; DROP TABLE retention_settings;
         ALTER TABLE threads DROP COLUMN archived_at;
         PRAGMA user_version=6;",
    )
    .unwrap();
    drop(db);
    let mut reopened = Service::open(f.data.path()).unwrap();
    assert_eq!(
        call(
            &mut reopened,
            "conversation.unread",
            Some(&f.admin),
            json!({})
        )["total"],
        0
    );
    assert_eq!(
        call(
            &mut reopened,
            "conversation.subscriptions",
            Some(&f.admin),
            json!({})
        )["items"],
        json!([{"id":f.thread}])
    );
    assert_eq!(
        call(
            &mut reopened,
            "notification.settings.get",
            Some(&f.admin),
            json!({})
        ),
        json!({"enabled":false,"include_excerpt":true})
    );
    reopened
        .select_notification_recipient(&f.candidate)
        .unwrap();
    assert!(
        reopened
            .prepare_notifications(&f.candidate, 20)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn failed_version_seven_upgrade_rolls_back_table_creation_and_version() {
    let f = fixture();
    drop(f.service);
    let db = rusqlite::Connection::open(f.data.path().join("swarm.sqlite3")).unwrap();
    db.execute_batch("DROP TABLE notification_outbox; DROP TABLE notification_selection; PRAGMA user_version=6; CREATE TABLE notification_outbox(blocked INTEGER);").unwrap();
    assert!(Service::open(f.data.path()).is_err());
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        6
    );
    assert_eq!(db.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='notification_selection'",[],|r|r.get::<_,i64>(0)).unwrap(),0);
}
