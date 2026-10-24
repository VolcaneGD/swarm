//! Bounded subprocess checks of restore gate wait and lost Named Pipe response.
use super::*;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};
use swarm_platform::{BackupStore, CredentialStore, PlatformError, pipe};

type Observers = Mutex<HashMap<PathBuf, mpsc::Sender<()>>>;
fn observers() -> &'static Observers {
    static OBSERVERS: OnceLock<Observers> = OnceLock::new();
    OBSERVERS.get_or_init(|| Mutex::new(HashMap::new()))
}
pub(super) fn request_read(root: &Path, request: &Request) {
    if request.operation == "gui.backup"
        && request.args["operation"] == "restore.apply"
        && let Some(sender) = observers().lock().unwrap().get(root)
    {
        let _ = sender.send(());
    }
}
fn request(operation: &str, args: Value, auth: Option<String>) -> Request {
    Request {
        operation: operation.into(),
        args,
        auth,
        context: json!({}),
        request_id: None,
    }
}

fn run_case(root: &Path, lose_response: bool, gate_seconds: u64) {
    std::fs::create_dir_all(root).unwrap();
    let root = root.canonicalize().unwrap();
    let mut service = Service::open(&root).unwrap();
    let mut sessions = session::SessionBridge::open(&root, &mut service).unwrap();
    let access = gui::GuiAccess::open(&root).unwrap();
    let capability = String::from_utf8(
        CredentialStore::open(&root.join("gui-host"))
            .unwrap()
            .get("transport-capability")
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    let setup = access.dispatch(&mut service, &mut sessions, request("gui.issue", json!({
        "alias":"admin","intent":"a".repeat(64),"request":{
            "operation":"account.setup","args":{"name":"Admin","password":"fixture-password"},"context":{}
        }
    }), Some(capability.clone()))).data.unwrap();
    let id = setup["id"].as_str().unwrap().to_owned();
    let archives = root.join("input");
    BackupStore::open(&archives)
        .unwrap()
        .write_new("selected", |f| {
            service
                .prepare_backup()
                .unwrap()
                .write_archive(f, swarm_core::BackupKind::Manual)
                .map_err(|_| PlatformError::Io)
        })
        .unwrap();
    let preview = restore::control(
        &mut service,
        &root,
        &id,
        "restore.preview",
        &json!({"directory":archives,"key":"selected"}),
    )
    .unwrap();
    let intent = uuid::Uuid::new_v4().to_string();
    restore::control(
        &mut service,
        &root,
        &id,
        "restore.prepare",
        &json!({
            "directory":archives,"key":"selected","intent":intent,
            "input_sha256":preview["input_sha256"],"targets":preview["targets"]
        }),
    )
    .unwrap();
    std::fs::write(root.join("RULES.md"), "# newest live\n").unwrap();
    let args = json!({"intent":intent,"input_sha256":preview["input_sha256"],"confirmed":true});
    let shared: SharedService = Arc::new(Mutex::new((service, sessions, access)));
    let listener = pipe::Listener::bind(&gui_pipe_name(&root).unwrap()).unwrap();
    let (read_tx, read_rx) = mpsc::channel();
    observers().lock().unwrap().insert(root.clone(), read_tx);
    let gate = shared.lock().unwrap();
    let server_shared = Arc::clone(&shared);
    let server_root = root.clone();
    let server = std::thread::spawn(move || {
        serve_listener(
            listener,
            Some(1),
            server_shared,
            Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            true,
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            server_root,
        )
    });
    let client_root = root.clone();
    let client_id = id.clone();
    let client_args = args.clone();
    let client = std::thread::spawn(move || {
        if lose_response {
            let mut connection = pipe::connect(&gui_pipe_name(&client_root).unwrap()).unwrap();
            connection.set_timeout(Duration::from_millis(200));
            swarm_ipc::write_request(&mut connection, &request("gui.backup", json!({
                "alias":"admin","account_id":client_id,"operation":"restore.apply","args":client_args
            }), Some(capability))).unwrap();
            assert!(swarm_ipc::read_response(&mut connection).is_err());
            // Dropping the client cancels its I/O, not the already accepted intent.
            None
        } else {
            Some(gui::GuiClient::open(&client_root).unwrap().backup(
                "admin",
                &client_id,
                "restore.apply",
                client_args,
            ))
        }
    });
    read_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("RULES.md")).unwrap(),
        "# newest live\n"
    );
    assert!(
        !root.join("restore-transactions").exists(),
        "queued restore has not started a transaction"
    );
    if lose_response {
        client.join().unwrap();
        drop(gate);
    } else {
        // Deliberately cross the production server's former 30-second deadline.
        std::thread::sleep(Duration::from_secs(gate_seconds));
        if gate_seconds > 300 {
            assert!(
                client.is_finished(),
                "production client must time out before the 310-second gate release"
            );
            assert_eq!(
                client.join().unwrap().unwrap().unwrap_err().code,
                "CONNECTION_ERROR"
            );
            drop(gate);
        } else {
            assert!(
                !client.is_finished(),
                "backup client must still wait for the gate"
            );
            drop(gate);
            assert_eq!(client.join().unwrap().unwrap().unwrap()["state"], "applied");
        }
    }
    server.join().unwrap().unwrap();
    observers().lock().unwrap().remove(&root);
    let mut state = shared.lock().unwrap();
    let (service, sessions, access) = &mut *state;
    let epoch = || {
        rusqlite::Connection::open(root.join("swarm.sqlite3"))
            .unwrap()
            .query_row(
                "SELECT value FROM app_metadata WHERE key='changes.cursor_epoch.v1'",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap()
    };
    let applied_epoch = epoch();
    assert_eq!(
        restore_apply::status(&root, &id, &intent).unwrap().unwrap()["state"],
        "applied"
    );
    assert_eq!(
        restore_apply::control(service, &root, &id, &args).unwrap()["state"],
        "applied"
    );
    assert_eq!(epoch(), applied_epoch, "result lookup must never reapply");
    let stale = access.dispatch(
        service,
        sessions,
        request(
            "gui.request",
            json!({
                "alias":"admin","request":{"operation":"account.status","args":{},"context":{}}
            }),
            Some(gui_capability(&root)),
        ),
    );
    assert!(
        !stale.ok,
        "accepted restore invalidates the previous credential"
    );
    let login = access.dispatch(service, sessions, request("gui.issue", json!({
        "alias":"admin","intent":"b".repeat(64),"request":{
            "operation":"account.login","args":{"name":"Admin","password":"fixture-password"},"context":{}
        }
    }), Some(gui_capability(&root))));
    assert!(login.ok);
    assert_eq!(login.data.unwrap()["id"], id);
    let result = access.dispatch(service, sessions, request("gui.backup", json!({
        "alias":"admin","account_id":id,"operation":"restore.status","args":{"intent":intent}
    }), Some(gui_capability(&root))));
    assert_eq!(result.data.unwrap()["state"], "applied");
    assert_eq!(epoch(), applied_epoch);
}
fn gui_capability(root: &Path) -> String {
    String::from_utf8(
        CredentialStore::open(&root.join("gui-host"))
            .unwrap()
            .get("transport-capability")
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}

#[test]
#[ignore = "restore transport worker; invoke only through the bounded parent"]
fn restore_transport_worker() {
    let Ok(root) = std::env::var("SWARM_TEST_RESTORE_TRANSPORT_ROOT") else {
        return;
    };
    run_case(&Path::new(&root).join("long-wait"), false, 32);
    run_case(&Path::new(&root).join("lost-response"), true, 0);
}

#[test]
#[ignore = "310-second isolated deadline worker; invoke only through its bounded parent"]
fn restore_over_three_hundred_seconds_worker() {
    let Ok(root) = std::env::var("SWARM_TEST_RESTORE_TRANSPORT_ROOT") else {
        return;
    };
    run_case(&Path::new(&root).join("production-timeout"), false, 310);
}

#[test]
#[ignore = "opt-in 310-second production deadline verification"]
fn restore_over_three_hundred_seconds_preserves_accepted_result() {
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let root = tempfile::tempdir().unwrap();
    let mut child = OwnedChild(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "restore_transport_tests::restore_over_three_hundred_seconds_worker",
                "--nocapture",
            ])
            .env("SWARM_TEST_RESTORE_TRANSPORT_ROOT", root.path())
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(360);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "310-second restore child exceeded 360-second bound"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn restore_gate_over_thirty_seconds_and_lost_response_preserve_terminal_result() {
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let root = tempfile::tempdir().unwrap();
    let mut child = OwnedChild(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "restore_transport_tests::restore_transport_worker",
                "--nocapture",
            ])
            .env("SWARM_TEST_RESTORE_TRANSPORT_ROOT", root.path())
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "restore transport child timed out"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
