//! Issued credentials and cursors across real restore/rollback and Named Pipes.
//! No direct SQLite fixtures or production failure-injection API.
use crate::{SharedService, gui, gui_pipe_name, pipe_name, serve_listener, session};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::{Duration, Instant};
use swarm_core::{CredentialCandidate, Service, protocol::Request};
use swarm_ipc::RpcResponse;
use swarm_platform::{BackupStore, CredentialStore, PlatformError, pipe};

fn request(operation: &str, args: Value, auth: Option<String>) -> Request {
    Request {
        operation: operation.into(),
        args,
        auth,
        context: json!({}),
        request_id: None,
    }
}

fn rpc(root: &Path, state: &SharedService, human: bool, request: Request) -> RpcResponse {
    let endpoint = if human {
        gui_pipe_name(root)
    } else {
        pipe_name(root)
    }
    .unwrap();
    let listener = pipe::Listener::bind(&endpoint).unwrap();
    let server_state = Arc::clone(state);
    let server_root = root.to_owned();
    let server = std::thread::spawn(move || {
        serve_listener(
            listener,
            Some(1),
            server_state,
            Arc::new(AtomicUsize::new(0)),
            human,
            Arc::new(AtomicBool::new(false)),
            server_root,
        )
    });
    let mut connection = pipe::connect(&endpoint).unwrap();
    connection.set_timeout(Duration::from_secs(5));
    swarm_ipc::write_request(&mut connection, &request).unwrap();
    let response = swarm_ipc::read_response(&mut connection).unwrap();
    drop(connection);
    server.join().unwrap().unwrap();
    response
}

fn capability(root: &Path) -> String {
    String::from_utf8(
        CredentialStore::open(&root.join("gui-host"))
            .unwrap()
            .get("transport-capability")
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}

fn human(
    root: &Path,
    state: &SharedService,
    alias: &str,
    operation: &str,
    args: Value,
) -> RpcResponse {
    rpc(
        root,
        state,
        true,
        request(
            "gui.request",
            json!({"alias":alias,"request":request(operation,args,None)}),
            Some(capability(root)),
        ),
    )
}

fn issue(root: &Path, state: &SharedService, alias: &str, operation: &str, intent: char) -> Value {
    let response = rpc(
        root,
        state,
        true,
        request(
            "gui.issue",
            json!({
                "alias":alias,"intent":intent.to_string().repeat(64),
                "request":request(operation,json!({"name":"RestoreAdmin","password":"isolated-restore-fixture"}),None)
            }),
            Some(capability(root)),
        ),
    );
    assert!(response.ok, "human issuance failed");
    response.data.unwrap()
}

fn ai_start(
    root: &Path,
    state: &SharedService,
    project: &Value,
    alias: &str,
    intent: char,
) -> CredentialCandidate {
    let response = rpc(
        root,
        state,
        false,
        request(
            "session.start",
            json!({"alias":alias,"project_id":project}),
            Some(intent.to_string().repeat(64)),
        ),
    );
    assert!(response.ok, "AI issuance failed");
    serde_json::from_slice(
        &CredentialStore::open(&root.join("client-credentials"))
            .unwrap()
            .get(alias)
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}

fn error(response: RpcResponse, expected: &str) {
    assert!(!response.ok, "operation unexpectedly succeeded");
    assert_eq!(response.error.unwrap().code, expected);
}

fn run_case(root: &Path, failure: Option<&str>) {
    std::fs::create_dir_all(root).unwrap();
    let root = root.canonicalize().unwrap();
    let mut service = Service::open(&root).unwrap();
    let sessions = session::SessionBridge::open(&root, &mut service).unwrap();
    let access = gui::GuiAccess::open(&root).unwrap();
    let mut state = Arc::new(std::sync::Mutex::new((service, sessions, access)));
    let admin = issue(&root, &state, "admin", "account.setup", 'a');
    issue(&root, &state, "second", "account.login", 'b');
    let project_root = root.join("project");
    std::fs::create_dir(&project_root).unwrap();
    let project = human(
        &root,
        &state,
        "admin",
        "project.register",
        json!({"name":"Owned project","root":project_root}),
    )
    .data
    .unwrap()["id"]
        .clone();
    assert!(
        human(
            &root,
            &state,
            "admin",
            "board.create",
            json!({"name":"Archived board"})
        )
        .ok
    );
    let ai = ai_start(&root, &state, &project, "agent", 'c');
    let CredentialCandidate::Ai {
        session_token,
        resume_secret,
        id,
        ..
    } = ai
    else {
        panic!("expected AI");
    };
    let human_cursor = human(&root, &state, "second", "changes.get", json!({}))
        .data
        .unwrap()["change_cursor"]
        .clone();
    let ai_cursor = rpc(
        &root,
        &state,
        false,
        request("changes.get", json!({}), Some(session_token.clone())),
    )
    .data
    .unwrap()["change_cursor"]
        .clone();
    assert!(human_cursor.is_string() && ai_cursor.is_string());
    let archives = root.join("input");
    let args = {
        let mut locked = state.lock().unwrap();
        let service = &mut locked.0;
        BackupStore::open(&archives)
            .unwrap()
            .write_new("selected", |file| {
                service
                    .prepare_backup()
                    .unwrap()
                    .write_archive(file, swarm_core::BackupKind::Manual)
                    .map_err(|_| PlatformError::Io)
            })
            .unwrap();
        let preview = crate::restore::control(
            service,
            &root,
            admin["id"].as_str().unwrap(),
            "restore.preview",
            &json!({"directory":archives,"key":"selected"}),
        )
        .unwrap();
        let intent = uuid::Uuid::new_v4().to_string();
        crate::restore::control(
            service,
            &root,
            admin["id"].as_str().unwrap(),
            "restore.prepare",
            &json!({
                "directory":archives,"key":"selected","intent":intent,
                "input_sha256":preview["input_sha256"],"targets":preview["targets"]
            }),
        )
        .unwrap();
        json!({"intent":intent,"input_sha256":preview["input_sha256"],"confirmed":true})
    };
    assert!(
        human(
            &root,
            &state,
            "admin",
            "board.create",
            json!({"name":"Latest live board"})
        )
        .ok
    );
    let expected_state = if failure.is_some() {
        "rolled_back"
    } else {
        "applied"
    };
    {
        let mut locked = state.lock().unwrap();
        let mut injected = false;
        let result = super::apply_observed(
            &mut locked.0,
            &root,
            admin["id"].as_str().unwrap(),
            &args,
            |stage| {
                if failure == Some(stage) {
                    injected = true;
                    return Err(swarm_core::protocol::Fault::new(
                        "STORAGE_ERROR",
                        8,
                        "isolated injected failure",
                    ));
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(injected, failure.is_some());
        assert_eq!(result["state"], expected_state);
    }
    // Both the already-open bridge and a newly opened bridge must refuse old grants.
    for reopen in [false, true] {
        if reopen {
            let (mut service, old_sessions, old_access) = Arc::try_unwrap(state)
                .ok()
                .expect("all pipe workers must release the shared state")
                .into_inner()
                .expect("unpoisoned service state");
            drop(old_sessions);
            drop(old_access);
            let sessions = session::SessionBridge::open(&root, &mut service).unwrap();
            let access = gui::GuiAccess::open(&root).unwrap();
            state = Arc::new(std::sync::Mutex::new((service, sessions, access)));
        }
        for alias in ["admin", "second"] {
            error(
                human(&root, &state, alias, "board.list", json!({})),
                "AUTHENTICATION_REQUIRED",
            );
        }
        error(
            rpc(
                &root,
                &state,
                false,
                request("board.list", json!({}), Some(session_token.clone())),
            ),
            "AUTHENTICATION_REQUIRED",
        );
        error(
            rpc(
                &root,
                &state,
                false,
                request(
                    "session.resume",
                    json!({
                        "alias":"resume-old","project_id":project,"actor_id":id,"resume_secret":resume_secret
                    }),
                    Some(if reopen { 'e' } else { 'd' }.to_string().repeat(64)),
                ),
            ),
            "AUTHENTICATION_REQUIRED",
        );
    }
    assert_eq!(
        issue(&root, &state, "fresh-human", "account.login", 'f')["id"],
        admin["id"]
    );
    error(
        human(
            &root,
            &state,
            "fresh-human",
            "changes.get",
            json!({"after":human_cursor}),
        ),
        "RESYNC_REQUIRED",
    );
    let fresh_ai = ai_start(&root, &state, &project, "fresh-ai", '1');
    let CredentialCandidate::Ai {
        session_token: fresh_token,
        ..
    } = fresh_ai
    else {
        panic!("expected AI");
    };
    error(
        rpc(
            &root,
            &state,
            false,
            request(
                "changes.get",
                json!({"after":ai_cursor}),
                Some(fresh_token.clone()),
            ),
        ),
        "RESYNC_REQUIRED",
    );
    let count = if failure.is_some() { 2 } else { 1 };
    assert_eq!(
        human(&root, &state, "fresh-human", "board.list", json!({}))
            .data
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        count
    );
    // A terminal retry is a lookup: it must not expire freshly reissued grants/cursors.
    let cursor = rpc(
        &root,
        &state,
        false,
        request("changes.get", json!({}), Some(fresh_token.clone())),
    )
    .data
    .unwrap()["change_cursor"]
        .clone();
    assert_eq!(
        super::control(
            &mut state.lock().unwrap().0,
            &root,
            admin["id"].as_str().unwrap(),
            &args
        )
        .unwrap()["state"],
        expected_state
    );
    assert!(human(&root, &state, "fresh-human", "board.list", json!({})).ok);
    assert!(
        rpc(
            &root,
            &state,
            false,
            request("changes.get", json!({"after":cursor}), Some(fresh_token))
        )
        .ok
    );
}

#[test]
#[ignore = "credential/cursor worker; invoke through bounded parent only"]
fn issued_credential_worker() {
    let Ok(root) = std::env::var("SWARM_TEST_ISSUED_RESTORE_ROOT") else {
        return;
    };
    for (label, failure) in [
        ("applied", None),
        ("database-written", Some("database_written")),
        ("credentials-cleared", Some("credentials_cleared")),
    ] {
        run_case(&Path::new(&root).join(label), failure);
    }
}

#[test]
fn issued_sessions_and_cursors_expire_after_apply_and_rollback_over_named_pipes() {
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
                "restore_apply::credential_tests::issued_credential_worker",
                "--nocapture",
            ])
            .env("SWARM_TEST_ISSUED_RESTORE_ROOT", root.path())
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(90);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(
                status.success(),
                "isolated restore credential worker failed"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "isolated restore credential worker exceeded 90-second bound"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
