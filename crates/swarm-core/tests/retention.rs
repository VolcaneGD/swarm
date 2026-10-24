use rusqlite::Connection;
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
) -> Result<Value, Fault> {
    service.request(Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    })
}

fn call(service: &mut Service, operation: &str, auth: Option<&str>, args: Value) -> Value {
    request(service, operation, auth, args).unwrap()
}

fn setup(service: &mut Service) -> String {
    call(
        service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"admin-password"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn authorize_retention(service: &mut Service, admin: &str, account_id: &str) {
    call(
        service,
        "permission.set",
        Some(admin),
        json!({
            "subject_type":"account","subject_id":account_id,
            "operation":"retention.manage","scope":"global",
            "target_id":"","decision":"allow"
        }),
    );
}

#[test]
fn public_settings_are_infinite_by_default_global_grantable_and_human_only() {
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = setup(&mut service);
    assert_eq!(
        call(
            &mut service,
            "retention.settings.get",
            Some(&admin),
            json!({})
        ),
        json!({"history_days":null,"deleted_days":null,"deleted_board_days":null,
            "deleted_thread_days":null,"archived_thread_days":null})
    );

    let member = call(
        &mut service,
        "account.create",
        Some(&admin),
        json!({"name":"Member","password":"member-password"}),
    );
    let member_id = member["id"].as_str().unwrap().to_owned();
    let member = call(
        &mut service,
        "account.login",
        None,
        json!({"name":"Member","password":"member-password"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        request(
            &mut service,
            "retention.settings.get",
            Some(&member),
            json!({})
        )
        .unwrap_err()
        .code,
        "PERMISSION_DENIED"
    );

    authorize_retention(&mut service, &admin, &member_id);
    assert_eq!(
        call(
            &mut service,
            "retention.settings.set",
            Some(&member),
            json!({"history_days":14,"deleted_days":60})
        ),
        json!({"history_days":14,"deleted_days":60,"deleted_board_days":null,
            "deleted_thread_days":null,"archived_thread_days":null})
    );
    assert_eq!(
        call(
            &mut service,
            "retention.settings.get",
            Some(&admin),
            json!({})
        ),
        json!({"history_days":14,"deleted_days":60,"deleted_board_days":null,
            "deleted_thread_days":null,"archived_thread_days":null})
    );
    assert_eq!(
        request(
            &mut service,
            "retention.settings.set",
            Some(&member),
            json!({"history_days":0,"deleted_days":60})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );
    assert_eq!(
        request(
            &mut service,
            "retention.settings.set",
            Some(&member),
            json!({"history_days":14,"deleted_days":60,"other":true})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );

    let project_root = tempfile::tempdir().unwrap();
    let project = call(
        &mut service,
        "project.register",
        Some(&admin),
        json!({"name":"AI retention test","root":project_root.path()}),
    );
    let identity = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    );
    assert_eq!(
        request(
            &mut service,
            "retention.settings.get",
            identity["session_token"].as_str(),
            json!({}),
        )
        .unwrap_err()
        .code,
        "PERMISSION_DENIED"
    );
}

fn create_post(service: &mut Service, admin: &str, thread_id: &str, body: &str) -> Value {
    call(
        service,
        "post.create",
        Some(admin),
        json!({"thread_id":thread_id,"body":body}),
    )
}

fn board_and_thread(service: &mut Service, admin: &str) -> (Value, Value) {
    let board = call(
        service,
        "board.create",
        Some(admin),
        json!({"name":"Board"}),
    );
    let thread = call(
        service,
        "thread.create",
        Some(admin),
        json!({"board_id":board["id"],"name":"Thread"}),
    );
    (board, thread)
}

fn raw_db(root: &Path) -> Connection {
    Connection::open(root.join("swarm.sqlite3")).unwrap()
}

#[test]
fn public_purge_removes_deleted_body_and_history_but_keeps_post_skeleton_and_reply() {
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = setup(&mut service);
    call(
        &mut service,
        "retention.settings.set",
        Some(&admin),
        json!({"history_days":null,"deleted_days":1}),
    );
    let (_, thread) = board_and_thread(&mut service, &admin);
    let thread_id = thread["id"].as_str().unwrap();
    let root = create_post(&mut service, &admin, thread_id, "deleted current body");
    call(
        &mut service,
        "post.edit",
        Some(&admin),
        json!({"thread_id":thread_id,"no":root["no"],"if_revision":1,"body":"deleted historical body"}),
    );
    let response = call(
        &mut service,
        "response.create",
        Some(&admin),
        json!({"thread_id":thread_id,"to":root["no"],"body":"related response"}),
    );
    call(
        &mut service,
        "post.delete",
        Some(&admin),
        json!({"thread_id":thread_id,"no":root["no"]}),
    );
    let db = raw_db(data.path());
    db.execute(
        "UPDATE posts SET deleted_at='2026-01-01T00:00:00Z' WHERE thread_id=?1 AND no=?2",
        rusqlite::params![thread_id, root["no"].as_i64().unwrap()],
    )
    .unwrap();
    drop(db);

    let result = service.purge_expired_data().unwrap();
    assert_eq!(result["purged_posts"], 1);
    assert_eq!(result["purged_revisions"], 1);
    assert_eq!(result["purged_conversations"], 0);
    assert_eq!(result["purged_identities"], 0);
    assert_eq!(result["pruned_requests"], 0);
    assert_eq!(result["storage_reclaimed"], true);
    let skeleton = call(
        &mut service,
        "post.get",
        Some(&admin),
        json!({"thread_id":thread_id,"no":root["no"]}),
    );
    assert_eq!(skeleton["body"], Value::Null);
    assert_eq!(skeleton["deleted"], true);
    assert_eq!(skeleton["restore_available"], false);
    assert_eq!(skeleton["no"], root["no"]);
    assert!(
        request(
            &mut service,
            "post.restore",
            Some(&admin),
            json!({"thread_id":thread_id,"no":root["no"]}),
        )
        .is_err()
    );
    let history = call(
        &mut service,
        "post.history",
        Some(&admin),
        json!({"thread_id":thread_id,"no":root["no"]}),
    );
    assert_eq!(history["items"], json!([]));

    let db = raw_db(data.path());
    let retained: (String, Option<String>, Option<String>, Option<i64>) = db
        .query_row(
            "SELECT body,body_purged_at,deleted_at,reply_to FROM posts WHERE thread_id=?1 AND no=?2",
            rusqlite::params![thread_id, root["no"].as_i64().unwrap()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(retained.0, "");
    assert!(retained.1.is_some());
    assert!(retained.2.is_some());
    assert_eq!(retained.3, None);
    assert_eq!(
        db.query_row(
            "SELECT reply_to FROM posts WHERE thread_id=?1 AND no=?2",
            rusqlite::params![thread_id, response["no"].as_i64().unwrap()],
            |row| row.get::<_, Option<i64>>(0),
        )
        .unwrap(),
        Some(root["no"].as_i64().unwrap())
    );

    // Purging content inherited from a deleted parent must survive restoring
    // that parent: the structural post remains, while its body stays gone.
    let (_, thread2) = board_and_thread(&mut service, &admin);
    let thread2_id = thread2["id"].as_str().unwrap();
    call(
        &mut service,
        "thread.subscribe",
        Some(&admin),
        json!({"id":thread2_id,"subscribed":true}),
    );
    let inherited = create_post(
        &mut service,
        &admin,
        thread2_id,
        "unique-parent-retained-body-token",
    );
    assert_eq!(
        call(&mut service, "conversation.unread", Some(&admin), json!({}))["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["thread_id"] == thread2_id)
            .unwrap()["unread_count"],
        1
    );
    call(
        &mut service,
        "post.edit",
        Some(&admin),
        json!({"thread_id":thread2_id,"no":inherited["no"],"if_revision":1,"body":"unique-parent-history-token"}),
    );
    call(
        &mut service,
        "thread.delete",
        Some(&admin),
        json!({"id":thread2_id}),
    );
    let db = raw_db(data.path());
    db.execute(
        "UPDATE threads SET deleted_at='2026-01-01T00:00:00Z' WHERE id=?1",
        [thread2_id],
    )
    .unwrap();
    drop(db);

    let purged = service.purge_expired_data().unwrap();
    assert_eq!(purged["purged_posts"], 1);
    assert_eq!(purged["purged_revisions"], 1);
    call(
        &mut service,
        "thread.restore",
        Some(&admin),
        json!({"id":thread2_id}),
    );
    let inherited_skeleton = call(
        &mut service,
        "post.get",
        Some(&admin),
        json!({"thread_id":thread2_id,"no":inherited["no"]}),
    );
    assert_eq!(inherited_skeleton["body"], Value::Null);
    assert_eq!(inherited_skeleton["deleted"], true);
    assert_eq!(inherited_skeleton["restore_available"], false);
    assert_eq!(
        call(&mut service, "conversation.unread", Some(&admin), json!({}))["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["thread_id"] == thread2_id)
            .unwrap()["unread_count"],
        0
    );
    for operation in ["post.restore", "post.deleted_body"] {
        assert!(
            request(
                &mut service,
                operation,
                Some(&admin),
                json!({"thread_id":thread2_id,"no":inherited["no"]}),
            )
            .is_err()
        );
    }
    assert!(request(
        &mut service,
        "post.edit",
        Some(&admin),
        json!({"thread_id":thread2_id,"no":inherited["no"],"if_revision":2,"body":"resurrection attempt"}),
    )
    .is_err());
    assert!(
        request(
            &mut service,
            "response.create",
            Some(&admin),
            json!({"thread_id":thread2_id,"to":inherited["no"],"body":"reply to purged content"}),
        )
        .is_err()
    );
    assert_eq!(
        call(
            &mut service,
            "search",
            Some(&admin),
            json!({"query":"unique-parent-retained-body-token","scope":"post"}),
        )["items"],
        json!([])
    );
    assert_eq!(inherited_skeleton["no"], inherited["no"]);
}
