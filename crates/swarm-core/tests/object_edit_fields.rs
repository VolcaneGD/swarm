use serde_json::{Value, json};
use swarm_core::{Service, protocol::Request};

fn request(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}

#[test]
fn partial_object_edits_preserve_omitted_fields_and_require_a_real_field() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let account = service
        .request(request(
            "account.setup",
            None,
            json!({"name":"Admin","password":""}),
        ))
        .unwrap();
    let token = account["token"].as_str();
    let board = service
        .request(request(
            "board.create",
            token,
            json!({"name":"Original","description":"Keep"}),
        ))
        .unwrap();
    let thread = service
        .request(request(
            "thread.create",
            token,
            json!({"board_id":board["id"],"name":"Original","description":"Keep"}),
        ))
        .unwrap();
    for (kind, object) in [("board", board), ("thread", thread)] {
        let operation = format!("{kind}.edit");
        let id = &object["id"];
        let edited = service
            .request(request(
                &operation,
                token,
                json!({"id":id,"description":"Changed","if_revision":1}),
            ))
            .unwrap();
        assert_eq!(edited["name"], "Original");
        assert_eq!(edited["description"], "Changed");
        assert_eq!(edited["revision"], 2);
        assert_eq!(
            service
                .request(request(
                    &operation,
                    token,
                    json!({"id":id,"description":"stale","if_revision":1})
                ))
                .unwrap_err()
                .code,
            "REVISION_CONFLICT"
        );
        let renamed = service
            .request(request(
                &operation,
                token,
                json!({"id":id,"name":"Renamed","if_revision":2}),
            ))
            .unwrap();
        assert_eq!(renamed["description"], "Changed");
        let cleared = service
            .request(request(
                &operation,
                token,
                json!({"id":id,"description":"","if_revision":3}),
            ))
            .unwrap();
        assert_eq!(cleared["description"], "");
        assert_eq!(cleared["name"], "Renamed");
        for args in [
            json!({"id":id,"if_revision":4}),
            json!({"id":id,"name":"","if_revision":4}),
            json!({"id":id,"description":null,"if_revision":4}),
        ] {
            assert_eq!(
                service
                    .request(request(&operation, token, args))
                    .unwrap_err()
                    .code,
                "INVALID_INPUT"
            );
        }
        assert_eq!(
            service
                .request(request(&format!("{kind}.get"), token, json!({"id":id})))
                .unwrap()["revision"],
            4
        );
    }
}
