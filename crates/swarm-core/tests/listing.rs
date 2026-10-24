use serde_json::{Value, json};
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

fn call(service: &mut Service, operation: &str, auth: Option<&str>, args: Value) -> Value {
    request(service, operation, auth, args, json!({})).unwrap()
}

fn fixture() -> (tempfile::TempDir, Service, String) {
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Listing Admin","password":"listing-admin-password"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    (data, service, admin)
}

fn board(service: &mut Service, admin: &str, name: &str) -> Value {
    call(service, "board.create", Some(admin), json!({"name":name}))
}

fn account(service: &mut Service, admin: &str, name: &str) -> (String, String, String) {
    let created = call(
        service,
        "account.create",
        Some(admin),
        json!({"name":name,"password":format!("{name}-password")}),
    );
    let login = call(
        service,
        "account.login",
        None,
        json!({"name":name,"password":format!("{name}-password")}),
    );
    (
        created["id"].as_str().unwrap().to_owned(),
        login["token"].as_str().unwrap().to_owned(),
        name.to_owned(),
    )
}

fn grant(
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
        "permission.set",
        Some(admin),
        json!({"subject_type":"account","subject_id":account_id,"operation":operation,
            "scope":scope,"target_id":target_id,"decision":decision}),
    );
}

#[test]
fn board_and_thread_pages_cover_more_than_two_hundred_rows_in_stable_order() {
    let (_data, mut service, admin) = fixture();
    let first_board = board(&mut service, &admin, "Board 000");
    for index in 1..205 {
        board(&mut service, &admin, &format!("Board {index:03}"));
    }
    let board_first = call(
        &mut service,
        "board.list",
        Some(&admin),
        json!({"limit":200}),
    );
    assert_eq!(board_first["items"].as_array().unwrap().len(), 200);
    let board_cursor = board_first["next_cursor"].as_str().unwrap();
    assert_eq!(board_cursor.len(), 64);
    assert!(board_cursor.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert!(!board_cursor.contains(first_board["id"].as_str().unwrap()));
    let board_second = call(
        &mut service,
        "board.list",
        Some(&admin),
        json!({"limit":200,"cursor":board_cursor}),
    );
    assert_eq!(board_second["items"].as_array().unwrap().len(), 5);
    assert!(board_second["next_cursor"].is_null());
    let ids = board_first["items"]
        .as_array()
        .unwrap()
        .iter()
        .chain(board_second["items"].as_array().unwrap())
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    let unique = ids.iter().collect::<std::collections::HashSet<_>>();
    assert_eq!(unique.len(), 205);
    let boards_in_order = board_first["items"]
        .as_array()
        .unwrap()
        .iter()
        .chain(board_second["items"].as_array().unwrap())
        .collect::<Vec<_>>();
    assert!(boards_in_order.windows(2).all(|pair| {
        (
            pair[0]["created_at"].as_str().unwrap(),
            pair[0]["id"].as_str().unwrap(),
        ) <= (
            pair[1]["created_at"].as_str().unwrap(),
            pair[1]["id"].as_str().unwrap(),
        )
    }));

    let board_id = first_board["id"].as_str().unwrap();
    for index in 0..205 {
        call(
            &mut service,
            "thread.create",
            Some(&admin),
            json!({"board_id":board_id,"name":format!("Thread {index:03}")}),
        );
    }
    let thread_first = call(
        &mut service,
        "thread.list",
        Some(&admin),
        json!({"board_id":board_id,"limit":200}),
    );
    assert_eq!(thread_first["items"].as_array().unwrap().len(), 200);
    let thread_second = call(
        &mut service,
        "thread.list",
        Some(&admin),
        json!({"board_id":board_id,"limit":200,"cursor":thread_first["next_cursor"]}),
    );
    assert_eq!(thread_second["items"].as_array().unwrap().len(), 5);
    assert!(thread_second["next_cursor"].is_null());
    let threads_in_order = thread_first["items"]
        .as_array()
        .unwrap()
        .iter()
        .chain(thread_second["items"].as_array().unwrap())
        .collect::<Vec<_>>();
    assert!(threads_in_order.windows(2).all(|pair| {
        (
            pair[0]["created_at"].as_str().unwrap(),
            pair[0]["id"].as_str().unwrap(),
        ) <= (
            pair[1]["created_at"].as_str().unwrap(),
            pair[1]["id"].as_str().unwrap(),
        )
    }));
    for limit in [0, 201] {
        let fault = request(
            &mut service,
            "board.list",
            Some(&admin),
            json!({"limit":limit}),
            json!({}),
        )
        .unwrap_err();
        assert_eq!(fault.code, "INVALID_INPUT");
    }
}

#[test]
fn denied_rows_are_skipped_without_count_leaks_and_hidden_parent_ids_stay_redacted() {
    let (_data, mut service, admin) = fixture();
    let boards = (0..5)
        .map(|index| board(&mut service, &admin, &format!("Visible order {index}")))
        .collect::<Vec<_>>();
    let (reader_id, reader, _) = account(&mut service, &admin, "Listing Reader");
    for operation in [
        "read",
        "board.list",
        "board.get",
        "thread.list",
        "thread.get",
    ] {
        grant(
            &mut service,
            &admin,
            &reader_id,
            operation,
            "global",
            "",
            "allow",
        );
    }
    let ordered = call(&mut service, "board.list", Some(&admin), json!({}));
    let ordered_ids = ordered["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    for index in [0, 2] {
        grant(
            &mut service,
            &admin,
            &reader_id,
            "read",
            "board",
            &ordered_ids[index],
            "deny",
        );
    }

    let first = call(
        &mut service,
        "board.list",
        Some(&reader),
        json!({"limit":2}),
    );
    assert_eq!(first["items"].as_array().unwrap().len(), 2);
    assert_eq!(first["items"][0]["id"], ordered_ids[1]);
    assert_eq!(first["items"][1]["id"], ordered_ids[3]);
    assert!(first["next_cursor"].is_string());
    assert!(first.get("total_count").is_none());
    let second = call(
        &mut service,
        "board.list",
        Some(&reader),
        json!({"limit":2,"cursor":first["next_cursor"]}),
    );
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["items"][0]["id"], ordered_ids[4]);
    assert!(second["next_cursor"].is_null());

    let parent = &boards[1];
    let child = call(
        &mut service,
        "thread.create",
        Some(&admin),
        json!({"board_id":parent["id"],"name":"Parent hidden"}),
    );
    grant(
        &mut service,
        &admin,
        &reader_id,
        "board.get",
        "board",
        parent["id"].as_str().unwrap(),
        "deny",
    );
    let threads = call(
        &mut service,
        "thread.list",
        Some(&reader),
        json!({"board_id":parent["id"]}),
    );
    let listed_child = threads["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == child["id"])
        .unwrap();
    assert!(listed_child["board_id"].is_null());
}

#[test]
fn cursors_reject_actor_session_filter_context_permission_and_restore_changes() {
    let (_data, mut service, admin) = fixture();
    let board_a = board(&mut service, &admin, "Cursor A");
    let board_b = board(&mut service, &admin, "Cursor B");
    board(&mut service, &admin, "Cursor C");
    for index in 0..3 {
        call(
            &mut service,
            "thread.create",
            Some(&admin),
            json!({"board_id":board_a["id"],"name":format!("A {index}")}),
        );
    }
    call(
        &mut service,
        "thread.create",
        Some(&admin),
        json!({"board_id":board_b["id"],"name":"B"}),
    );
    let (reader_id, reader, _) = account(&mut service, &admin, "Cursor Reader");
    let (other_id, other, _) = account(&mut service, &admin, "Other Reader");
    for id in [&reader_id, &other_id] {
        for operation in ["read", "board.list", "thread.list", "thread.get"] {
            grant(&mut service, &admin, id, operation, "global", "", "allow");
        }
    }
    let page = call(
        &mut service,
        "thread.list",
        Some(&reader),
        json!({"board_id":board_a["id"],"limit":1}),
    );
    let cursor = page["next_cursor"].as_str().unwrap().to_owned();
    let forged = request(
        &mut service,
        "thread.list",
        Some(&reader),
        json!({"board_id":board_a["id"],"limit":1,"cursor":"0000000000000000000000000000000000000000000000000000000000000000"}),
        json!({}),
    )
    .unwrap_err();
    assert_eq!(forged.code, "RESYNC_REQUIRED");

    for (auth, args, context) in [
        (
            other.as_str(),
            json!({"board_id":board_a["id"],"limit":1,"cursor":cursor}),
            json!({}),
        ),
        (
            reader.as_str(),
            json!({"board_id":board_b["id"],"limit":1,"cursor":cursor}),
            json!({}),
        ),
    ] {
        let fault = request(&mut service, "thread.list", Some(auth), args, context).unwrap_err();
        assert_eq!(fault.code, "RESYNC_REQUIRED");
    }
    let second_login = call(
        &mut service,
        "account.login",
        None,
        json!({"name":"Cursor Reader","password":"Cursor Reader-password"}),
    );
    let other_session = second_login["token"].as_str().unwrap();
    let fault = request(
        &mut service,
        "thread.list",
        Some(other_session),
        json!({"board_id":board_a["id"],"limit":1,"cursor":cursor}),
        json!({}),
    )
    .unwrap_err();
    assert_eq!(fault.code, "RESYNC_REQUIRED");

    let board_page = request(
        &mut service,
        "board.list",
        Some(&reader),
        json!({"limit":1}),
        json!({"purpose":"before"}),
    )
    .unwrap();
    let board_cursor = board_page["next_cursor"].as_str().unwrap().to_owned();
    let fault = request(
        &mut service,
        "board.list",
        Some(&reader),
        json!({"limit":1,"cursor":board_cursor}),
        json!({"purpose":"after"}),
    )
    .unwrap_err();
    assert_eq!(fault.code, "RESYNC_REQUIRED");

    let page = call(
        &mut service,
        "board.list",
        Some(&reader),
        json!({"limit":1}),
    );
    let permission_cursor = page["next_cursor"].as_str().unwrap().to_owned();
    grant(
        &mut service,
        &admin,
        &reader_id,
        "board.list",
        "board",
        board_b["id"].as_str().unwrap(),
        "deny",
    );
    let fault = call_cursor(&mut service, &reader, &permission_cursor);
    assert_eq!(fault.code, "RESYNC_REQUIRED");

    let page = call(
        &mut service,
        "board.list",
        Some(&reader),
        json!({"limit":1}),
    );
    let restore_cursor = page["next_cursor"].as_str().unwrap().to_owned();
    service.invalidate_change_cursors().unwrap();
    let fault = call_cursor(&mut service, &reader, &restore_cursor);
    assert_eq!(fault.code, "RESYNC_REQUIRED");
}

fn call_cursor(service: &mut Service, auth: &str, cursor: &str) -> swarm_core::protocol::Fault {
    request(
        service,
        "board.list",
        Some(auth),
        json!({"limit":1,"cursor":cursor}),
        json!({}),
    )
    .unwrap_err()
}
