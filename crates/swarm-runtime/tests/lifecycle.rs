#![cfg(windows)]

use serde_json::{Value, json};
use std::os::windows::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use swarm_core::protocol::Request;
use swarm_runtime::gui::GuiClient;

struct RuntimeChild(Child);

impl RuntimeChild {
    fn spawn(root: &std::path::Path, explicit_human_start: bool) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_swarm-runtime"));
        command
            .arg("--data-dir")
            .arg(root)
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if explicit_human_start {
            command.arg("--human-start");
        }
        let child = command
            .spawn()
            .expect("spawn the isolated Swarm runtime executable");
        Self(child)
    }

    fn wait_for_exit(&mut self, timeout: Duration) -> std::process::ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.0.try_wait().expect("poll runtime child") {
                return status;
            }
            assert!(Instant::now() < deadline, "runtime did not stop in time");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for RuntimeChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn request(operation: &str, args: Value) -> Request {
    Request {
        operation: operation.to_owned(),
        auth: None,
        args,
        context: json!({}),
        request_id: None,
    }
}

fn raw_rpc(endpoint: &str, request: Request) -> swarm_ipc::RpcResponse {
    let mut connection = swarm_platform::pipe::connect(endpoint).expect("connect to endpoint");
    swarm_ipc::write_request(&mut connection, &request).expect("send request frame");
    swarm_ipc::read_response(&mut connection).expect("read response frame")
}

fn wait_for_gui(root: &std::path::Path, child: &mut RuntimeChild) -> GuiClient {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.0.try_wait().expect("poll startup") {
            panic!("runtime exited during startup: {status}");
        }
        if let Ok(client) = GuiClient::open(root)
            && client.setup_status().is_ok()
        {
            return client;
        }
        assert!(
            Instant::now() < deadline,
            "runtime GUI pipe did not become ready"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn human_stop_terminates_runtime_persists_after_reopen_and_cli_launch_cannot_clear_it() {
    let root = tempfile::tempdir().expect("isolated lifecycle data root");
    let mut child = RuntimeChild::spawn(root.path(), false);
    let client = wait_for_gui(root.path(), &mut child);

    let admin = client
        .issue(
            "admin",
            &"a".repeat(64),
            request(
                "account.setup",
                json!({"name":"Lifecycle Admin","password":"fixture-only-password"}),
            ),
        )
        .expect("human setup through the private GUI transport");
    assert!(admin.get("token").is_none());
    client
        .request("admin", request("account.logout", json!({})))
        .expect("release setup session before testing login");
    client
        .issue(
            "admin",
            &"b".repeat(64),
            request(
                "account.login",
                json!({"name":"Lifecycle Admin","password":"fixture-only-password"}),
            ),
        )
        .expect("human login through the private GUI transport");
    client
        .request("admin", request("account.status", json!({})))
        .expect("confirm the authenticated human alias");

    let gui_endpoint = swarm_runtime::gui_pipe_name(root.path()).unwrap();
    let mut wrong_gui_stop = request("gui.runtime_stop", json!({"alias":"admin"}));
    wrong_gui_stop.auth = Some("f".repeat(64));
    assert_eq!(
        raw_rpc(&gui_endpoint, wrong_gui_stop)
            .error
            .expect("wrong GUI capability must be rejected")
            .exit_code,
        3
    );
    assert_eq!(
        swarm_runtime::lifecycle::status(root.path()).unwrap(),
        json!({"state":"running"}),
        "a wrong GUI credential must not stop the runtime"
    );

    let ai_endpoint = swarm_runtime::pipe_name(root.path()).unwrap();
    let mut ai_stop = request("gui.runtime_stop", json!({"alias":"admin"}));
    ai_stop.auth = Some("e".repeat(64));
    assert_eq!(
        raw_rpc(&ai_endpoint, ai_stop)
            .error
            .expect("AI transport must reject human stop")
            .exit_code,
        4
    );
    assert_eq!(
        swarm_runtime::lifecycle::status(root.path()).unwrap(),
        json!({"state":"running"}),
        "an AI credential must not stop the runtime"
    );

    // Establish the data connection before the stop flag is set, but leave
    // its request unread until the human stop has completed.
    let mut queued_data =
        swarm_platform::pipe::connect(&ai_endpoint).expect("queue a client connection before stop");
    // A successful connection to the successor instance proves the listener
    // handed queued_data to a worker. Kernel connection alone is not admission:
    // stop can retire an instance that has not yet been handed off.
    let admission_probe = swarm_platform::pipe::connect(&ai_endpoint)
        .expect("listener published the successor after admitting queued data");
    drop(admission_probe);
    assert_eq!(
        client.stop("admin").expect("authenticated human stop"),
        json!({"state":"stopped"})
    );
    swarm_ipc::write_request(&mut queued_data, &request("board.list", json!({})))
        .expect("send the queued request after stop was accepted");
    assert_eq!(
        swarm_ipc::read_response(&mut queued_data)
            .expect("receive stopped response")
            .error
            .expect("queued work must be rejected")
            .code,
        "RUNTIME_STOPPED"
    );
    assert!(child.wait_for_exit(Duration::from_secs(3)).success());

    let stop_store = swarm_platform::CredentialStore::open(&root.path().join("lifecycle"))
        .expect("reopen protected lifecycle storage");
    let marker = stop_store
        .get("full-stop")
        .expect("read the stop marker")
        .expect("human stop marker persists");
    assert!(marker.starts_with(b"2:"));
    assert_eq!(
        swarm_runtime::lifecycle::status(root.path()).unwrap(),
        json!({"state":"stopped"})
    );
    let stopped = match swarm_runtime::lifecycle::connect(root.path()) {
        Err(error) => error,
        Ok(_) => panic!("library launch path must honor the persistent stop"),
    };
    assert_eq!(stopped.code, "RUNTIME_STOPPED");

    let relaunch = Command::new(env!("CARGO_BIN_EXE_swarm-runtime"))
        .arg("--data-dir")
        .arg(root.path())
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .expect("invoke ordinary runtime launch after full stop");
    assert_eq!(relaunch.status.code(), Some(7));
    assert_eq!(
        stop_store.get("full-stop").unwrap().as_deref(),
        Some(marker.as_slice()),
        "ordinary runtime launch must not clear or rewrite the marker"
    );
    assert_eq!(
        swarm_runtime::lifecycle::status(root.path()).unwrap(),
        json!({"state":"stopped"})
    );

    let mut human_started = RuntimeChild::spawn(root.path(), true);
    let resumed_client = wait_for_gui(root.path(), &mut human_started);
    assert_eq!(
        swarm_runtime::lifecycle::status(root.path()).unwrap(),
        json!({"state":"running"}),
        "the explicit human start entry clears the persistent stop"
    );
    assert!(stop_store.get("full-stop").unwrap().is_none());
    resumed_client
        .request("admin", request("account.status", json!({})))
        .expect("the saved human alias remains valid across runtime restart");
    assert_eq!(
        resumed_client.stop("admin").unwrap(),
        json!({"state":"stopped"})
    );
    assert!(
        human_started
            .wait_for_exit(Duration::from_secs(3))
            .success()
    );
    assert_eq!(
        swarm_runtime::lifecycle::status(root.path()).unwrap(),
        json!({"state":"stopped"})
    );
}
