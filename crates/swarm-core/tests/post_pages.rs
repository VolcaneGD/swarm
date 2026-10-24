use serde_json::{Value, json};
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
    request_with_id(service, operation, auth, args, None)
}

fn request_with_id(
    service: &mut Service,
    operation: &str,
    auth: Option<&str>,
    args: Value,
    request_id: Option<&str>,
) -> Result<Value, Fault> {
    service.request(Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: request_id.map(str::to_owned),
    })
}

fn call(service: &mut Service, operation: &str, auth: Option<&str>, args: Value) -> Value {
    request(service, operation, auth, args).unwrap()
}

fn fixture() -> (tempfile::TempDir, Service, String, String, String) {
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"admin-password"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
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
        json!({"board_id":board["id"],"name":"Thread"}),
    );
    (
        data,
        service,
        admin,
        board["id"].as_str().unwrap().to_owned(),
        thread["id"].as_str().unwrap().to_owned(),
    )
}

fn append_posts(service: &mut Service, admin: &str, thread: &str, count: i64) {
    for no in 1..=count {
        let post = call(
            service,
            "post.create",
            Some(admin),
            json!({"thread_id":thread,"body":format!("Post {no}")}),
        );
        assert_eq!(post["no"], no);
    }
}

fn page(service: &mut Service, admin: &str, thread: &str, args: Value) -> Value {
    let mut args = args;
    args["thread_id"] = json!(thread);
    call(service, "post.page", Some(admin), args)
}

#[test]
fn numbered_pages_cover_all_supported_sizes_and_clamp_out_of_range_pages() {
    let (_data, mut service, admin, _board, thread) = fixture();
    append_posts(&mut service, &admin, &thread, 501);

    for size in [50_i64, 100, 500, 1000] {
        let first = page(&mut service, &admin, &thread, json!({"page_size":size}));
        let expected_pages = (501 + size - 1) / size;
        assert_eq!(first["page"], 1);
        assert_eq!(first["page_size"], size);
        assert_eq!(first["total_posts"], 501);
        assert_eq!(first["total_pages"], expected_pages);
        assert_eq!(first["items"][0]["no"], 1);
        assert_eq!(
            first["items"][first["items"].as_array().unwrap().len() - 1]["no"],
            size.min(501)
        );

        let last = page(
            &mut service,
            &admin,
            &thread,
            json!({"page":expected_pages,"page_size":size}),
        );
        let expected_first = (expected_pages - 1) * size + 1;
        assert_eq!(last["page"], expected_pages);
        assert_eq!(last["items"][0]["no"], expected_first);
        assert_eq!(
            last["items"][last["items"].as_array().unwrap().len() - 1]["no"],
            501
        );

        let beyond = page(
            &mut service,
            &admin,
            &thread,
            json!({"page":10000,"page_size":size}),
        );
        assert_eq!(beyond["page"], expected_pages);
        assert_eq!(beyond["items"], last["items"]);
    }

    let anchored = page(
        &mut service,
        &admin,
        &thread,
        json!({"page_size":50,"anchor_no":151}),
    );
    assert_eq!(anchored["page"], 4);
    assert_eq!(anchored["items"][0]["no"], 151);
    assert_eq!(anchored["items"][49]["no"], 200);
}

#[test]
fn empty_pages_have_one_page_and_tombstones_remain_counted_and_redacted() {
    let (_data, mut service, admin, board, thread) = fixture();
    let empty_thread = call(
        &mut service,
        "thread.create",
        Some(&admin),
        json!({"board_id":board,"name":"Empty"}),
    )["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let empty = page(
        &mut service,
        &admin,
        &empty_thread,
        json!({"page":4,"page_size":50}),
    );
    assert_eq!(empty["page"], 1);
    assert_eq!(empty["total_posts"], 0);
    assert_eq!(empty["total_pages"], 1);
    assert_eq!(empty["items"], json!([]));

    append_posts(&mut service, &admin, &thread, 51);
    call(
        &mut service,
        "post.delete",
        Some(&admin),
        json!({"thread_id":thread,"no":51}),
    );
    let second = page(
        &mut service,
        &admin,
        &thread,
        json!({"page":2,"page_size":50}),
    );
    assert_eq!(second["total_posts"], 51);
    assert_eq!(second["total_pages"], 2);
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert_eq!(second["items"][0]["no"], 51);
    assert_eq!(second["items"][0]["deleted"], true);
    assert!(second["items"][0]["body"].is_null());
}

#[test]
fn page_inputs_and_missing_anchor_are_rejected() {
    let (_data, mut service, admin, _board, thread) = fixture();
    append_posts(&mut service, &admin, &thread, 2);

    for args in [
        json!({"page":0}),
        json!({"page":-1}),
        json!({"page":1.5}),
        json!({"page":null}),
        json!({"page_size":49}),
        json!({"page_size":null}),
        json!({"anchor_no":0}),
        json!({"anchor_no":1.5}),
        json!({"unknown":true}),
    ] {
        let mut invalid_args = args;
        invalid_args["thread_id"] = json!(thread);
        let fault = request(&mut service, "post.page", Some(&admin), invalid_args).unwrap_err();
        assert_eq!(fault.exit_code, 2, "args should fail validation");
    }

    let missing = request(
        &mut service,
        "post.page",
        Some(&admin),
        json!({"thread_id":thread,"anchor_no":3}),
    )
    .unwrap_err();
    assert_eq!(missing.exit_code, 5);
}

#[test]
fn pagination_requires_post_list_permission_and_is_not_an_ai_read_operation() {
    let (data, mut service, admin, _board, thread) = fixture();
    append_posts(&mut service, &admin, &thread, 1);
    let account = call(
        &mut service,
        "account.create",
        Some(&admin),
        json!({"name":"Reader","password":"reader-password"}),
    );
    let reader = call(
        &mut service,
        "account.login",
        None,
        json!({"name":"Reader","password":"reader-password"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    call(
        &mut service,
        "permission.set",
        Some(&admin),
        json!({"subject_type":"account","subject_id":account["id"],"operation":"post.list","scope":"thread","target_id":thread,"decision":"deny"}),
    );
    let denied = request(
        &mut service,
        "post.page",
        Some(&reader),
        json!({"thread_id":thread,"page":1,"page_size":50}),
    )
    .unwrap_err();
    assert_eq!(denied.exit_code, 4);

    let item_denied_account = call(
        &mut service,
        "account.create",
        Some(&admin),
        json!({"name":"ItemDenied","password":"item-denied-password"}),
    );
    let item_denied_reader = call(
        &mut service,
        "account.login",
        None,
        json!({"name":"ItemDenied","password":"item-denied-password"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    call(
        &mut service,
        "permission.set",
        Some(&admin),
        json!({"subject_type":"account","subject_id":item_denied_account["id"],"operation":"post.get","scope":"thread","target_id":thread,"decision":"deny"}),
    );
    let item_denied = request(
        &mut service,
        "post.page",
        Some(&item_denied_reader),
        json!({"thread_id":thread,"page":1,"page_size":50}),
    )
    .unwrap_err();
    assert_eq!(item_denied.exit_code, 4);

    let project_root = tempfile::tempdir().unwrap();
    let project = call(
        &mut service,
        "project.register",
        Some(&admin),
        json!({"name":"AI reader","root":project_root.path()}),
    );
    let identity = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    );
    let ai_fault = request(
        &mut service,
        "post.page",
        identity["session_token"].as_str(),
        json!({"thread_id":thread,"page":1,"page_size":50}),
    )
    .unwrap_err();
    assert_eq!(ai_fault.exit_code, 4);
    drop(data);
}

#[test]
fn thread_quota_counts_posts_responses_and_tombstones() {
    let (_data, mut service, admin, _board, thread) = fixture();
    append_posts(&mut service, &admin, &thread, 999);

    let thousandth_args = json!({"thread_id":thread,"body":"Post 1000"});
    let thousandth = request_with_id(
        &mut service,
        "post.create",
        Some(&admin),
        thousandth_args.clone(),
        Some("post-1000-intent"),
    )
    .unwrap();
    assert_eq!(thousandth["no"], 1000);
    let replay = request_with_id(
        &mut service,
        "post.create",
        Some(&admin),
        thousandth_args,
        Some("post-1000-intent"),
    )
    .unwrap();
    assert_eq!(replay["id"], thousandth["id"]);
    assert_eq!(replay["no"], 1000);
    for (operation, args) in [
        (
            "post.create",
            json!({"thread_id":thread,"body":"Post 1001"}),
        ),
        (
            "response.create",
            json!({"thread_id":thread,"to":1,"body":"Response 1001"}),
        ),
    ] {
        let fault = request(&mut service, operation, Some(&admin), args).unwrap_err();
        assert_eq!(fault.code, "THREAD_FULL");
    }

    call(
        &mut service,
        "post.delete",
        Some(&admin),
        json!({"thread_id":thread,"no":1000}),
    );
    let after_delete = request(
        &mut service,
        "post.create",
        Some(&admin),
        json!({"thread_id":thread,"body":"Slot must stay consumed"}),
    )
    .unwrap_err();
    assert_eq!(after_delete.code, "THREAD_FULL");
}

#[test]
fn response_consumes_the_shared_thread_number_quota() {
    let (_data, mut service, admin, _board, thread) = fixture();
    append_posts(&mut service, &admin, &thread, 999);
    let thousandth = call(
        &mut service,
        "response.create",
        Some(&admin),
        json!({"thread_id":thread,"to":1,"body":"Response 1000"}),
    );
    assert_eq!(thousandth["no"], 1000);

    for (operation, args) in [
        (
            "post.create",
            json!({"thread_id":thread,"body":"Post 1001"}),
        ),
        (
            "response.create",
            json!({"thread_id":thread,"to":1,"body":"Response 1001"}),
        ),
    ] {
        let fault = request(&mut service, operation, Some(&admin), args).unwrap_err();
        assert_eq!(fault.code, "THREAD_FULL");
    }
}

#[test]
fn concurrent_writers_cannot_allocate_past_the_thread_limit() {
    let (data, mut service, admin, _board, thread) = fixture();
    append_posts(&mut service, &admin, &thread, 999);
    drop(service);

    let db_path = data.path().to_owned();
    let thread_a = thread.clone();
    let admin_a = admin.clone();
    let path_a = db_path.clone();
    let first = std::thread::spawn(move || {
        let mut service = Service::open(&path_a).unwrap();
        request(
            &mut service,
            "post.create",
            Some(&admin_a),
            json!({"thread_id":thread_a,"body":"Concurrent A"}),
        )
    });
    let thread_b = thread.clone();
    let admin_b = admin.clone();
    let path_b = db_path;
    let second = std::thread::spawn(move || {
        let mut service = Service::open(&path_b).unwrap();
        request(
            &mut service,
            "post.create",
            Some(&admin_b),
            json!({"thread_id":thread_b,"body":"Concurrent B"}),
        )
    });

    let results = [first.join().unwrap(), second.join().unwrap()];
    let successes = results.iter().filter(|result| result.is_ok()).count();
    assert_eq!(successes, 1);
    let rejected = results
        .iter()
        .find_map(|result| result.as_ref().err())
        .expect("one competing create must be rejected");
    assert_eq!(rejected.code, "THREAD_FULL");

    let mut verify = Service::open(data.path()).unwrap();
    let final_page = call(
        &mut verify,
        "post.page",
        Some(&admin),
        json!({"thread_id":thread,"page":1,"page_size":1000}),
    );
    assert_eq!(final_page["total_posts"], 1000);
    assert_eq!(final_page["items"][999]["no"], 1000);
}
