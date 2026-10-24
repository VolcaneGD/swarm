#![cfg(windows)]
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use swarm_core::{Service, protocol::Request};
use swarm_gui_host::GuiHost;

fn request(service: &mut Service, auth: Option<&str>, operation: &str, args: Value) -> Value {
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

#[test]
fn selected_human_trash_and_retention_contract_crosses_real_private_transport() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = request(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"fixture-password"}),
    );
    let token = admin["token"].as_str().unwrap();
    let board = request(
        &mut service,
        Some(token),
        "board.create",
        json!({"name":"Disposable trash"}),
    );
    let thread = request(
        &mut service,
        Some(token),
        "thread.create",
        json!({"name":"Disposable child","board_id":board["id"]}),
    );
    request(
        &mut service,
        Some(token),
        "post.create",
        json!({"thread_id":thread["id"],"body":"Synthetic content"}),
    );
    drop(service);
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(200)).unwrap()
    });
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Ok(name) = swarm_runtime::gui_pipe_name(root.path())
            && let Ok(pipe) = swarm_platform::pipe::connect(&name)
        {
            drop(pipe);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Admin", "fixture-password")
        .unwrap();
    let checks = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for args in [
            json!({"id":thread["id"]}),
            json!({"id":thread["id"],"confirmed":false}),
            json!({"id":thread["id"],"confirmed":true,"auth":"forged"}),
        ] {
            assert_eq!(
                host.manage("thread.purge", args).unwrap_err().code,
                "INVALID_INPUT"
            );
        }
        assert!(
            host.manage("thread.purge", json!({"id":thread["id"],"confirmed":true}))
                .is_err()
        );
        host.manage("thread.delete", json!({"id":thread["id"]}))
            .unwrap();
        host.manage("board.delete", json!({"id":board["id"]}))
            .unwrap();
        let deleted = host.read("conversation.deleted", json!({})).unwrap();
        assert_eq!(deleted["threads"][0]["parent_deleted"], true);
        assert_eq!(deleted["threads"][0]["can_purge"], true);
        let items = json!([{"kind":"thread","id":thread["id"]},{"kind":"board","id":board["id"]}]);
        host.manage(
            "conversation.batch",
            json!({"action":"restore","items":items,"confirmed":true}),
        )
        .unwrap();
        assert_eq!(
            host.read("post.get", json!({"thread_id":thread["id"],"no":1}))
                .unwrap()["body"],
            "Synthetic content"
        );
        host.manage("thread.delete", json!({"id":thread["id"]}))
            .unwrap();
        host.manage("board.delete", json!({"id":board["id"]}))
            .unwrap();
        host.manage(
            "conversation.batch",
            json!({"action":"purge","items":items,"confirmed":true}),
        )
        .unwrap();
        assert!(
            host.read("conversation.deleted", json!({})).unwrap()["boards"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            host.read("post.get", json!({"thread_id":thread["id"],"no":1}))
                .is_err()
        );
        let id = admin["account"]["id"]
            .as_str()
            .or_else(|| admin["id"].as_str())
            .unwrap();
        let settings = json!({"history_days":30,"deleted_days":7,"deleted_board_days":90,"deleted_thread_days":30,"archived_thread_days":14});
        assert_eq!(
            host.retention_control(id, "retention.settings.set", settings.clone())
                .unwrap(),
            settings
        );
        let older = host
            .retention_control(
                id,
                "retention.settings.set",
                json!({"history_days":15,"deleted_days":null}),
            )
            .unwrap();
        assert_eq!(older["deleted_board_days"], 90);
        assert_eq!(older["archived_thread_days"], 14);
        assert!(
            host.retention_control(
                id,
                "retention.settings.set",
                json!({"history_days":null,"deleted_days":null,"archived_thread_days":0})
            )
            .is_err()
        );
        let manual = host.backup_control(id, "manual.create", json!({})).unwrap();
        host.backup_control(id, "manual.ack", json!({"intent":manual["intent"]}))
            .unwrap();
        let inventory = host.backup_control(id, "archives.list", json!({})).unwrap();
        assert!(
            inventory["archives"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["key"] == manual["key"])
        );
        host.backup_control(
            id,
            "archives.delete",
            json!({"directory":manual["destination"],"key":manual["key"],"confirmed":true}),
        )
        .unwrap();
        let after = host.backup_control(id, "archives.list", json!({})).unwrap();
        assert!(
            !after["archives"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["key"] == manual["key"])
        );
    }));
    host.stop_runtime().unwrap();
    server.join().unwrap();
    if let Err(error) = checks {
        std::panic::resume_unwind(error);
    }
}
