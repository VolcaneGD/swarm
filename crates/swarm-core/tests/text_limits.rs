use serde_json::{Value, json};
use swarm_core::{
    Service,
    protocol::{Fault, Request},
};

fn call(
    service: &mut Service,
    token: Option<&str>,
    operation: &str,
    args: Value,
) -> Result<Value, Fault> {
    service.request(Request {
        operation: operation.into(),
        auth: token.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    })
}

#[test]
fn names_and_bodies_enforce_unicode_boundaries_without_mutating_on_rejection() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"admin-password"}),
    )
    .unwrap();
    let token = admin["token"].as_str().unwrap();
    let board_name = "𠮷".repeat(100);
    let board = call(
        &mut service,
        Some(token),
        "board.create",
        json!({"name":board_name}),
    )
    .unwrap();
    assert_eq!(board["name"], board_name);
    for operation in ["board.create", "board.edit"] {
        let err = call(
            &mut service,
            Some(token),
            operation,
            if operation == "board.create" {
                json!({"name":"日".repeat(101)})
            } else {
                json!({"id":board["id"],"name":"日".repeat(101),"if_revision":1})
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "NAME_TOO_LONG");
        assert_eq!(err.exit_code, 2);
        assert!(!err.message.contains(&"日".repeat(101)));
    }
    let thread_name = "😀".repeat(200);
    let thread = call(
        &mut service,
        Some(token),
        "thread.create",
        json!({"board_id":board["id"],"name":thread_name}),
    )
    .unwrap();
    assert_eq!(thread["name"], thread_name);
    for operation in ["thread.create", "thread.edit"] {
        let err = call(
            &mut service,
            Some(token),
            operation,
            if operation == "thread.create" {
                json!({"board_id":board["id"],"name":"a".repeat(201)})
            } else {
                json!({"id":thread["id"],"name":"a".repeat(201),"if_revision":1})
            },
        )
        .unwrap_err();
        assert_eq!(err.code, "NAME_TOO_LONG");
    }
    let body = "𠮷".repeat(100_000);
    let post = call(
        &mut service,
        Some(token),
        "post.create",
        json!({"thread_id":thread["id"],"body":body}),
    )
    .unwrap();
    assert_eq!(post["body"], body);
    for operation in ["post.create", "response.create", "post.edit"] {
        let mut args = json!({"thread_id":thread["id"],"body":"日".repeat(100_001)});
        if operation == "response.create" {
            args["to"] = json!(1);
        }
        if operation == "post.edit" {
            args["no"] = json!(1);
            args["if_revision"] = json!(1);
        }
        let err = call(&mut service, Some(token), operation, args).unwrap_err();
        assert_eq!(err.code, "BODY_TOO_LONG");
    }
    let unchanged = call(
        &mut service,
        Some(token),
        "post.get",
        json!({"thread_id":thread["id"],"no":1}),
    )
    .unwrap();
    assert_eq!(unchanged["body"], body);
    assert_eq!(unchanged["revision"], 1);
    let next = call(
        &mut service,
        Some(token),
        "response.create",
        json!({"thread_id":thread["id"],"to":1,"body":"ok"}),
    )
    .unwrap();
    assert_eq!(next["no"], 2);
    let edit = call(
        &mut service,
        Some(token),
        "post.edit",
        json!({"thread_id":thread["id"],"no":1,"if_revision":1,"body":body}),
    )
    .unwrap();
    assert_eq!(edit["body"], body);
    let err = call(
        &mut service,
        Some(token),
        "post.create",
        json!({"thread_id":thread["id"],"body":"a".repeat(1024 * 1024 + 1)}),
    )
    .unwrap_err();
    assert_eq!(err.code, "BODY_TOO_LARGE");
    assert_eq!(
        call(
            &mut service,
            Some(token),
            "board.get",
            json!({"id":board["id"]})
        )
        .unwrap()["revision"],
        1
    );
    assert_eq!(
        call(
            &mut service,
            Some(token),
            "thread.get",
            json!({"id":thread["id"]})
        )
        .unwrap()["revision"],
        1
    );
}
