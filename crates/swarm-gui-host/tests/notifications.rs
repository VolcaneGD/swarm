#![cfg(windows)]

use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread::JoinHandle,
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
use swarm_gui_host::GuiHost;
use swarm_platform::CredentialStore;

struct RuntimeServer {
    root: PathBuf,
    thread: Option<JoinHandle<()>>,
    stop_alias: Option<String>,
}

impl RuntimeServer {
    fn start(root: &Path) -> Self {
        Self::start_with_ai_budget(root, 1)
    }

    fn start_with_ai_budget(root: &Path, ai_budget: usize) -> Self {
        let path = root.to_owned();
        Self {
            root: path.clone(),
            thread: Some(std::thread::spawn(move || {
                swarm_runtime::serve_with_gui(&path, Some(ai_budget), Some(96)).unwrap()
            })),
            stop_alias: None,
        }
    }

    fn remember_selected_alias(&mut self) {
        let store = CredentialStore::open(&self.root.join("gui-state")).unwrap();
        let selection: Value =
            serde_json::from_slice(&store.get("selection").unwrap().unwrap()).unwrap();
        self.stop_alias = Some(selection["alias"].as_str().unwrap().to_owned());
    }

    fn stop_and_join(&mut self) -> bool {
        let stopped = self
            .stop_alias
            .as_deref()
            .and_then(|alias| {
                swarm_runtime::gui::GuiClient::open(&self.root)
                    .and_then(|client| client.stop(alias).map(|_| ()))
                    .ok()
            })
            .is_some();
        if !stopped {
            // If an assertion fails before login, use the isolated finite
            // listener budget to retire the test server without blocking on
            // an unauthenticated shutdown request.
            let name = swarm_runtime::gui_pipe_name(&self.root).ok();
            if let Some(name) = name {
                for _ in 0..96 {
                    let Ok(pipe) = swarm_platform::pipe::connect(&name) else {
                        break;
                    };
                    drop(pipe);
                }
            }
        }
        let Some(thread) = self.thread.take() else {
            return stopped;
        };
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = thread.join();
            let _ = sender.send(());
        });
        stopped && receiver.recv_timeout(Duration::from_secs(5)).is_ok()
    }
}

impl Drop for RuntimeServer {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}

#[test]
fn notification_click_waits_for_its_account_rechecks_access_and_preserves_newer_intent() {
    use swarm_gui_host::{pending_notification_event, queue_notification_activation};
    use swarm_platform::notifications::{ActivationTarget, Identity};
    let root = tempfile::tempdir().unwrap();
    let exe = std::env::current_exe().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"password"}),
    );
    let token = admin["token"].as_str().unwrap();
    let reader = call(
        &mut service,
        Some(token),
        "account.create",
        json!({"name":"Reader","password":"reader-password"}),
    );
    let board = call(
        &mut service,
        Some(token),
        "board.create",
        json!({"name":"Private board"}),
    );
    let thread = call(
        &mut service,
        Some(token),
        "thread.create",
        json!({"board_id":board["id"],"name":"Private thread"}),
    );
    call(
        &mut service,
        Some(token),
        "permission.set",
        json!({"subject_type":"account","subject_id":reader["id"],"operation":"thread.get","scope":"global","target_id":"","decision":"allow"}),
    );
    drop(service);
    let identity = Identity::new(root.path(), &exe).unwrap();
    let target = ActivationTarget::new(
        reader["id"].as_str().unwrap(),
        thread["id"].as_str().unwrap(),
    )
    .unwrap();
    let uri = identity.activation_uri(&target);
    let first = queue_notification_activation(root.path(), &exe, &uri).unwrap();
    let mut server = RuntimeServer::start(root.path());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.notification_activation().unwrap()["state"],
        "login_required"
    );
    host.authenticate("login", "Admin", "password").unwrap();
    server.remember_selected_alias();
    let wrong = host.notification_activation().unwrap();
    assert_eq!(wrong["state"], "account_required");
    assert!(wrong.get("thread").is_none() && wrong.get("account_id").is_none());
    assert_eq!(
        host.acknowledge_notification_activation(&first)
            .unwrap_err()
            .exit_code,
        3
    );
    host.authenticate("login", "Reader", "reader-password")
        .unwrap();
    let resolved = host.notification_activation().unwrap();
    assert_eq!(resolved["state"], "ready");
    assert_eq!(resolved["thread"]["id"], thread["id"]);
    let newer = queue_notification_activation(root.path(), &exe, &uri).unwrap();
    assert!(!host.acknowledge_notification_activation(&first).unwrap());
    assert_eq!(
        pending_notification_event(root.path()).unwrap().as_deref(),
        Some(newer.as_str())
    );
    assert!(host.acknowledge_notification_activation(&newer).unwrap());
    assert_eq!(host.notification_activation().unwrap()["state"], "none");
    queue_notification_activation(root.path(), &exe, &uri).unwrap();
    host.authenticate("login", "Admin", "password").unwrap();
    host.manage_authorization("permission.set", json!({"subject_type":"account","subject_id":reader["id"],"operation":"thread.get","scope":"global","target_id":"","decision":"deny"})).unwrap();
    host.authenticate("login", "Reader", "reader-password")
        .unwrap();
    let denied = host.notification_activation().unwrap();
    assert_eq!(denied["state"], "unavailable");
    assert!(denied.get("thread").is_none() && denied.get("account_id").is_none());
    let selected: Value = serde_json::from_slice(
        &CredentialStore::open(&root.path().join("gui-state"))
            .unwrap()
            .get("selection")
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    swarm_runtime::gui::GuiClient::open(root.path())
        .unwrap()
        .request(
            selected["alias"].as_str().unwrap(),
            request("account.logout", json!({})),
        )
        .unwrap();
    assert_eq!(
        host.notification_activation().unwrap()["state"],
        "login_required"
    );
    host.authenticate("login", "Admin", "password").unwrap();
    server.remember_selected_alias();
    drop(host);
    assert!(server.stop_and_join());
}

#[test]
fn notification_mailbox_rejects_foreign_uri_without_writing_and_preserves_corruption() {
    use swarm_gui_host::queue_notification_activation;
    use swarm_platform::notifications::{ActivationTarget, Identity};
    let root = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let exe = std::env::current_exe().unwrap();
    let target = ActivationTarget::new(
        &uuid::Uuid::new_v4().to_string(),
        &uuid::Uuid::new_v4().to_string(),
    )
    .unwrap();
    let foreign = Identity::new(other.path(), &exe)
        .unwrap()
        .activation_uri(&target);
    assert_eq!(
        queue_notification_activation(root.path(), &exe, &foreign)
            .unwrap_err()
            .exit_code,
        2
    );
    assert!(!root.path().join("notification-state").exists());
    let host = GuiHost::open(root.path()).unwrap();
    let store = CredentialStore::open(&root.path().join("notification-state")).unwrap();
    store.put("activation", b"malformed").unwrap();
    assert_eq!(
        host.notification_activation().unwrap_err().code,
        "GUI_STATE"
    );
    assert_eq!(store.get("activation").unwrap().unwrap(), b"malformed");
}

#[test]
fn real_gui_selection_drives_transactional_outbox_without_catchup_after_switch() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"password"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let second = call(
        &mut service,
        Some(&admin),
        "account.create",
        json!({"name":"Second","password":"second-password"}),
    );
    let board = call(
        &mut service,
        Some(&admin),
        "board.create",
        json!({"name":"Board"}),
    );
    let thread = call(
        &mut service,
        Some(&admin),
        "thread.create",
        json!({"board_id":board["id"],"name":"Thread"}),
    );
    for operation in ["thread.list", "thread.get", "post.list", "post.get"] {
        call(
            &mut service,
            Some(&admin),
            "permission.set",
            json!({"subject_type":"account","subject_id":second["id"],"operation":operation,"scope":"global","target_id":"","decision":"allow"}),
        );
    }
    call(
        &mut service,
        Some(&admin),
        "post.create",
        json!({"thread_id":thread["id"],"body":"Before any selection"}),
    );
    let project_root = tempfile::tempdir().unwrap();
    let project = call(
        &mut service,
        Some(&admin),
        "project.register",
        json!({"name":"AI project","root":project_root.path()}),
    );
    let ai = call(
        &mut service,
        None,
        "identity.start",
        json!({"project_id":project["id"]}),
    );
    std::fs::write(root.path().join("RULES.md"),format!("~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: notify-test-posting\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [post.create]\n    when: {{ field: time.hour, op: gte, value: 0 }}\naccess:\n  boards: []\n  threads: []\n~~~\n",project["id"].as_str().unwrap())).unwrap();
    drop(service);
    let mut server = RuntimeServer::start_with_ai_budget(root.path(), 3);
    ready(root.path());
    let db = rusqlite::Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    let count = || {
        db.query_row("SELECT count(*) FROM notification_outbox", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap()
    };
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Admin", "password").unwrap();
    server.remember_selected_alias();
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    let selected: Value =
        serde_json::from_slice(&store.get("selection").unwrap().unwrap()).unwrap();
    let admin_alias = selected["alias"].as_str().unwrap().to_owned();
    host.manage(
        "thread.subscribe",
        json!({"id":thread["id"],"subscribed":true}),
    )
    .unwrap();
    assert_eq!(count(), 0);
    let client = swarm_runtime::gui::GuiClient::open(root.path()).unwrap();
    let post = |body: &str| {
        client
            .request(
                &admin_alias,
                request("post.create", json!({"thread_id":thread["id"],"body":body})),
            )
            .unwrap()
    };
    post("While Admin selected");
    assert_eq!(count(), 1);
    host.authenticate("login", "Second", "second-password")
        .unwrap();
    assert_eq!(count(), 0);
    post("While Second selected, not subscribed");
    assert_eq!(count(), 0);
    host.authenticate("login", "Admin", "password").unwrap();
    assert_eq!(count(), 0, "switching back must not enqueue past posts");
    server.remember_selected_alias();
    drop(host);
    post("New with GUI closed");
    assert_eq!(count(), 1);
    let ai_post = |auth: Option<&str>| {
        let mut connection =
            swarm_platform::pipe::connect(&swarm_runtime::pipe_name(root.path()).unwrap()).unwrap();
        let mut req = request(
            "post.create",
            json!({"thread_id":thread["id"],"body":"AI new with GUI closed"}),
        );
        req.auth = auth.map(str::to_owned);
        swarm_ipc::write_request(&mut connection, &req).unwrap();
        swarm_ipc::read_response(&mut connection).unwrap()
    };
    assert!(ai_post(ai["session_token"].as_str()).ok);
    assert_eq!(count(), 2);
    assert!(!ai_post(None).ok);
    assert_eq!(
        count(),
        2,
        "rejected AI request must not alter pending work"
    );
    assert!(server.stop_and_join());
}

fn request(operation: &str, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: None,
        args,
        context: json!({}),
        request_id: None,
    }
}

#[test]
fn retention_settings_use_selected_human_and_reject_stale_account() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"password"}),
    );
    call(
        &mut service,
        admin["token"].as_str(),
        "account.create",
        json!({"name":"Reader","password":"reader-password"}),
    );
    drop(service);
    let mut server = RuntimeServer::start(root.path());
    ready(root.path());
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.retention_control("absent", "retention.settings.get", json!({}))
            .unwrap_err()
            .code,
        "AUTHENTICATION_REQUIRED"
    );
    host.authenticate("login", "Admin", "password").unwrap();
    server.remember_selected_alias();
    let selected_id = host.snapshot().unwrap()["account"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let id = selected_id.as_str();
    assert_eq!(
        host.retention_control(id, "retention.settings.get", json!({}))
            .unwrap(),
        json!({"history_days":null,"deleted_days":null,"deleted_board_days":null,"deleted_thread_days":null,"archived_thread_days":null})
    );
    assert_eq!(
        host.retention_control(
            id,
            "retention.settings.set",
            json!({"history_days":30,"deleted_days":7})
        )
        .unwrap(),
        json!({"history_days":30,"deleted_days":7,"deleted_board_days":null,"deleted_thread_days":null,"archived_thread_days":null})
    );
    assert_eq!(
        host.retention_control(
            id,
            "retention.settings.set",
            json!({"history_days":0,"deleted_days":null})
        )
        .unwrap_err()
        .exit_code,
        2
    );
    host.authenticate("login", "Reader", "reader-password")
        .unwrap();
    server.remember_selected_alias();
    assert_eq!(
        host.retention_control(
            id,
            "retention.settings.set",
            json!({"history_days":1,"deleted_days":1})
        )
        .unwrap_err()
        .code,
        "ACCOUNT_CHANGED"
    );
    let reader = host.snapshot().unwrap()["account"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        host.retention_control(&reader, "retention.settings.get", json!({}))
            .unwrap_err()
            .code,
        "PERMISSION_DENIED"
    );
    assert!(server.stop_and_join());
}

#[test]
fn running_runtime_purges_expired_content_and_cursor_metadata() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"password"}),
    );
    let token = admin["token"].as_str().unwrap();
    let board = call(
        &mut service,
        Some(token),
        "board.create",
        json!({"name":"Board"}),
    );
    let thread = call(
        &mut service,
        Some(token),
        "thread.create",
        json!({"board_id":board["id"],"name":"Thread"}),
    );
    call(
        &mut service,
        Some(token),
        "post.create",
        json!({"thread_id":thread["id"],"body":"expired isolated fixture"}),
    );
    call(
        &mut service,
        Some(token),
        "post.delete",
        json!({"thread_id":thread["id"],"no":1}),
    );
    call(
        &mut service,
        Some(token),
        "retention.settings.set",
        json!({"history_days":null,"deleted_days":1}),
    );
    let db = rusqlite::Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    db.execute("UPDATE posts SET deleted_at='2000-01-01T00:00:00Z'", [])
        .unwrap();
    db.execute("INSERT INTO app_metadata(key,value) VALUES('changes.cursor.token.expired-fixture','{\"version\":2,\"issued_at\":1}')", []).unwrap();
    drop(service);
    let mut server = RuntimeServer::start(root.path());
    ready(root.path());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let purged: bool = db
            .query_row(
                "SELECT body='' AND body_purged_at IS NOT NULL FROM posts",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let cursors: i64 = db.query_row("SELECT count(*) FROM app_metadata WHERE key='changes.cursor.token.expired-fixture'", [], |row| row.get(0)).unwrap();
        if purged && cursors == 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Runtime maintenance did not finish"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        db.query_row("SELECT no FROM posts", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "Admin", "password").unwrap();
    server.remember_selected_alias();
    assert!(server.stop_and_join());
}

fn call(service: &mut Service, auth: Option<&str>, operation: &str, args: Value) -> Value {
    let mut request = request(operation, args);
    request.auth = auth.map(str::to_owned);
    service.request(request).unwrap()
}

fn ready(root: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    for name in [
        swarm_runtime::pipe_name(root).unwrap(),
        swarm_runtime::gui_pipe_name(root).unwrap(),
    ] {
        loop {
            if let Ok(pipe) = swarm_platform::pipe::connect(&name) {
                drop(pipe);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "runtime pipe did not become ready"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn notification_reads_are_human_reads_and_require_the_selected_login() {
    let root = tempfile::tempdir().unwrap();
    let mut host = GuiHost::open(root.path()).unwrap();

    // A recognized, read-only notification operation reaches the normal
    // selected-human authentication boundary before any runtime is required.
    assert_eq!(
        host.read("notification.settings.get", json!({}))
            .unwrap_err()
            .exit_code,
        3
    );
    for operation in [
        "notification.settings.get",
        "conversation.subscriptions",
        "conversation.unread",
    ] {
        for args in [json!({"account_id":"spoof"}), json!(null), json!([])] {
            assert_eq!(host.read(operation, args).unwrap_err().exit_code, 2);
        }
    }
}

#[test]
fn selected_human_notification_state_is_sparse_private_and_persistent_over_pipe() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"admin-password"}),
    )["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut account_ids = Vec::new();
    for name in ["ReaderA", "ReaderB"] {
        let account = call(
            &mut service,
            Some(&admin),
            "account.create",
            json!({"name":name,"password":format!("{name}-password")}),
        );
        account_ids.push(account["id"].as_str().unwrap().to_owned());
    }
    let board = call(
        &mut service,
        Some(&admin),
        "board.create",
        json!({"name":"Notification board"}),
    );
    let thread = call(
        &mut service,
        Some(&admin),
        "thread.create",
        json!({"board_id":board["id"],"name":"Visible thread"}),
    );
    let mut post_ids = Vec::new();
    for no in 1..=3 {
        post_ids.push(
            call(
                &mut service,
                Some(&admin),
                "post.create",
                json!({"thread_id":thread["id"],"body":format!("Post {no}")}),
            )["id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    for account_id in &account_ids {
        for operation in ["thread.list", "thread.get", "post.list", "post.get"] {
            call(
                &mut service,
                Some(&admin),
                "permission.set",
                json!({"subject_type":"account","subject_id":account_id,"operation":operation,"scope":"global","target_id":"","decision":"allow"}),
            );
        }
    }
    drop(service);

    let mut server = RuntimeServer::start(root.path());
    ready(root.path());

    let mut host = GuiHost::open(root.path()).unwrap();
    host.authenticate("login", "ReaderA", "ReaderA-password")
        .unwrap();
    server.remember_selected_alias();
    assert_eq!(host.snapshot().unwrap()["account"]["id"], account_ids[0]);
    host.manage_notifications(
        &account_ids[0],
        "display.settings.set",
        json!({"language":"ja","theme":"dark"}),
    )
    .unwrap();
    assert_eq!(
        host.manage_notifications(
            &account_ids[1],
            "thread.subscribe",
            json!({"id":thread["id"],"subscribed":true})
        )
        .unwrap_err()
        .code,
        "ACCOUNT_CHANGED"
    );
    assert_eq!(
        host.read("notification.settings.get", json!({})).unwrap(),
        json!({"enabled":true,"include_excerpt":false})
    );
    assert_eq!(
        host.read("conversation.subscriptions", json!({})).unwrap()["items"],
        json!([])
    );
    host.manage_notifications(
        &account_ids[0],
        "thread.subscribe",
        json!({"id":thread["id"],"subscribed":true}),
    )
    .unwrap();
    host.manage(
        "notification.settings.set",
        json!({"enabled":false,"include_excerpt":true}),
    )
    .unwrap();
    assert_eq!(
        host.read("conversation.unread", json!({})).unwrap()["total"],
        3
    );
    assert_eq!(
        host.manage(
            "post.read.mark",
            json!({"thread_id":thread["id"],"post_ids":[post_ids[0],post_ids[2],post_ids[0]]}),
        )
        .unwrap(),
        json!({"marked":2})
    );
    assert_eq!(
        host.read("conversation.unread", json!({})).unwrap()["total"],
        1
    );
    assert_eq!(
        host.manage(
            "post.read.mark",
            json!({"thread_id":thread["id"],"post_ids":vec![post_ids[0].clone(); 200]}),
        )
        .unwrap(),
        json!({"marked":1}),
        "the 200-entry limit applies to input length while duplicates count once"
    );

    // Exact payload validation happens at the renderer boundary, before RPC.
    for (operation, args) in [
        ("notification.settings.get", json!({"account_id":"spoof"})),
        (
            "thread.subscribe",
            json!({"id":thread["id"],"subscribed":true,"account_id":"spoof"}),
        ),
        (
            "notification.settings.set",
            json!({"enabled":true,"include_excerpt":"false"}),
        ),
        (
            "notification.settings.set",
            json!({"enabled":true,"include_excerpt":false,"account_id":"spoof"}),
        ),
        (
            "post.read.mark",
            json!({"thread_id":thread["id"],"post_ids":[]}),
        ),
        (
            "post.read.mark",
            json!({"thread_id":thread["id"],"post_ids":[post_ids[0]],"account_id":"spoof"}),
        ),
        (
            "post.read.mark",
            json!({"thread_id":thread["id"],"post_ids":vec![post_ids[0].clone(); 201]}),
        ),
    ] {
        assert_eq!(host.manage(operation, args).unwrap_err().exit_code, 2);
    }
    assert_eq!(
        host.read(
            "thread.subscribe",
            json!({"id":thread["id"],"subscribed":true})
        )
        .unwrap_err()
        .exit_code,
        2,
        "mutations must not enter the read endpoint"
    );
    assert_eq!(
        host.read(
            "post.read.mark",
            json!({"thread_id":thread["id"],"post_ids":[post_ids[0]]})
        )
        .unwrap_err()
        .exit_code,
        2
    );
    assert_eq!(
        host.manage("notification.settings.get", json!({}))
            .unwrap_err()
            .exit_code,
        2,
        "reads must not enter the mutation endpoint"
    );

    // The host stores only the selected account alias. Reopening it preserves
    // that selection and the per-account sparse read set in the core database.
    drop(host);
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(
        host.read("display.settings.get", json!({})).unwrap(),
        json!({"settings":{"language":"ja","theme":"dark"}})
    );
    assert_eq!(
        host.read("conversation.unread", json!({})).unwrap()["total"],
        1
    );
    assert_eq!(
        host.read("conversation.subscriptions", json!({})).unwrap()["items"],
        json!([{"id":thread["id"]}])
    );
    assert_eq!(
        host.read("notification.settings.get", json!({})).unwrap(),
        json!({"enabled":false,"include_excerpt":true})
    );

    let layout = json!({"version":1,"treeWidth":360,"composerHeight":240});
    assert_eq!(
        host.ui_preferences(
            &account_ids[0],
            "ui.preferences.set",
            json!({"scope":"layout","value":layout})
        )
        .unwrap(),
        json!({"saved":true})
    );
    assert_eq!(
        host.ui_preferences(
            &account_ids[0],
            "ui.preferences.get",
            json!({"scope":"layout"})
        )
        .unwrap()["value"],
        layout
    );
    assert_eq!(
        host.ui_preferences(
            &account_ids[0],
            "ui.preferences.set",
            json!({"scope":"layout","value":layout,"expected_account_id":"forged"})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );

    host.authenticate("login", "ReaderB", "ReaderB-password")
        .unwrap();
    server.remember_selected_alias();
    assert_eq!(
        host.ui_preferences(
            &account_ids[0],
            "ui.preferences.set",
            json!({"scope":"layout","value":layout})
        )
        .unwrap_err()
        .code,
        "ACCOUNT_CHANGED"
    );
    assert!(
        host.ui_preferences(
            &account_ids[1],
            "ui.preferences.get",
            json!({"scope":"layout"})
        )
        .unwrap()["value"]
            .is_null()
    );
    for (operation, args) in [
        (
            "display.settings.set",
            json!({"language":"en","theme":"light"}),
        ),
        (
            "post.read.mark",
            json!({"thread_id":thread["id"],"post_ids":[post_ids[1]]}),
        ),
        (
            "notification.settings.set",
            json!({"enabled":false,"include_excerpt":true}),
        ),
        (
            "thread.subscribe",
            json!({"id":thread["id"],"subscribed":true}),
        ),
    ] {
        assert_eq!(
            host.manage_notifications(&account_ids[0], operation, args)
                .unwrap_err()
                .code,
            "ACCOUNT_CHANGED"
        );
    }
    assert_eq!(
        host.read("notification.settings.get", json!({})).unwrap(),
        json!({"enabled":true,"include_excerpt":false})
    );
    assert_eq!(
        host.read("display.settings.get", json!({})).unwrap(),
        json!({"settings":null})
    );
    assert_eq!(
        host.read("conversation.subscriptions", json!({})).unwrap()["items"],
        json!([])
    );
    assert_eq!(
        host.read("conversation.unread", json!({})).unwrap()["total"],
        3
    );
    host.manage(
        "post.read.mark",
        json!({"thread_id":thread["id"],"post_ids":[post_ids[1]]}),
    )
    .unwrap();
    assert_eq!(
        host.read("conversation.unread", json!({})).unwrap()["total"],
        2
    );

    host.authenticate("login", "ReaderA", "ReaderA-password")
        .unwrap();
    assert_eq!(
        host.read("conversation.unread", json!({})).unwrap()["total"],
        1,
        "switching accounts must restore A's sparse read state"
    );

    // Revoke the selected saved session through the private GUI bridge while
    // leaving its selection saved; subsequent content reads must fail closed.
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    let selection: Value =
        serde_json::from_slice(&store.get("selection").unwrap().unwrap()).unwrap();
    let selected_alias = selection["alias"].as_str().unwrap();
    swarm_runtime::gui::GuiClient::open(root.path())
        .unwrap()
        .request(selected_alias, request("account.logout", json!({})))
        .unwrap();
    assert_eq!(
        host.read("conversation.unread", json!({}))
            .unwrap_err()
            .exit_code,
        3
    );
    drop(host);
    assert!(
        server.stop_and_join(),
        "isolated runtime must stop and join"
    );
}
