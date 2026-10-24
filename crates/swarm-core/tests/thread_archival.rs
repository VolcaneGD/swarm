use rusqlite::Connection;
use serde_json::{Value, json};
use swarm_core::{Service, protocol::Request};

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

#[test]
fn thread_archives_at_post_1000_without_being_deleted() {
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"archive-test-password"}),
    )
    .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let board = call(
        &mut service,
        "board.create",
        Some(&admin),
        json!({"name":"Archive Board"}),
    )
    .unwrap();
    let thread = call(
        &mut service,
        "thread.create",
        Some(&admin),
        json!({"board_id":board["id"],"name":"Archive Boundary Thread"}),
    )
    .unwrap();
    let thread_id = thread["id"].as_str().unwrap();
    call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread_id,"body":"First post"}),
    )
    .unwrap();

    // Set the isolated fixture to 999 allocated numbers without performing 998 writes.
    Connection::open(data.path().join("swarm.sqlite3"))
        .unwrap()
        .execute("UPDATE threads SET next_no=1000 WHERE id=?1", [thread_id])
        .unwrap();
    let before_limit = call(
        &mut service,
        "thread.get",
        Some(&admin),
        json!({"id":thread_id}),
    )
    .unwrap();
    assert_eq!(before_limit["archived"], false);

    let last_post = call(
        &mut service,
        "response.create",
        Some(&admin),
        json!({"thread_id":thread_id,"to":1,"body":"Post number 1000"}),
    )
    .unwrap();
    assert_eq!(last_post["no"], 1000);

    let archived = call(
        &mut service,
        "thread.get",
        Some(&admin),
        json!({"id":thread_id}),
    )
    .unwrap();
    assert_eq!(archived["archived"], true);
    assert_eq!(archived["deleted_at"], Value::Null);

    let listed = call(
        &mut service,
        "thread.list",
        Some(&admin),
        json!({"board_id":board["id"]}),
    )
    .unwrap();
    assert_eq!(listed["items"][0]["archived"], true);

    call(
        &mut service,
        "thread.pin",
        Some(&admin),
        json!({"id":thread_id,"pinned":true}),
    )
    .unwrap();
    let pins = call(&mut service, "conversation.pins", Some(&admin), json!({})).unwrap();
    assert_eq!(pins["items"][0]["archived"], true);

    let capabilities = call(
        &mut service,
        "conversation.capabilities",
        Some(&admin),
        json!({}),
    )
    .unwrap();
    let thread_capabilities = &capabilities["threads"][0];
    assert_eq!(thread_capabilities["post_create"], false);
    assert_eq!(thread_capabilities["response_create"], false);
    assert_eq!(thread_capabilities["post_edit"], true);
    assert_eq!(thread_capabilities["post_history"], true);
    assert_eq!(thread_capabilities["thread_delete"], true);

    call(
        &mut service,
        "post.delete",
        Some(&admin),
        json!({"thread_id":thread_id,"no":1000}),
    )
    .unwrap();
    let after_delete = call(
        &mut service,
        "thread.get",
        Some(&admin),
        json!({"id":thread_id}),
    )
    .unwrap();
    assert_eq!(after_delete["archived"], true);
    assert_eq!(after_delete["deleted_at"], Value::Null);

    let rejected = call(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread_id,"body":"Post number 1001"}),
    )
    .unwrap_err();
    assert_eq!(rejected.code, "THREAD_FULL");

    let search = call(
        &mut service,
        "search",
        Some(&admin),
        json!({"query":"Archive Boundary Thread","scope":"thread_name"}),
    )
    .unwrap();
    assert!(
        search["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| { item["kind"] == "thread" && item["id"] == thread_id })
    );
}
