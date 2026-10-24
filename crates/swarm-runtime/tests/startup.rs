#![cfg(windows)]

use serde_json::{Value, json};
use std::{
    os::windows::process::CommandExt,
    process::{Child, Command, ExitStatus, Output, Stdio},
    time::{Duration, Instant},
};
use swarm_core::{CredentialCandidate, Service, protocol::Request};
use swarm_runtime::gui::GuiClient;

const ADMIN_NAME: &str = "Startup Fixture Admin";
const ADMIN_PASSWORD: &str = "fixture-only-password";

fn request(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.to_owned(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}

struct RuntimeChild(Child);

impl RuntimeChild {
    fn spawn(root: &std::path::Path) -> Self {
        let child = Command::new(env!("CARGO_BIN_EXE_swarm-runtime"))
            .arg("--data-dir")
            .arg(root)
            .creation_flags(0x0800_0000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn isolated runtime child");
        Self(child)
    }

    fn wait_for_exit(&mut self, timeout: Duration) -> ExitStatus {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(status) = self.0.try_wait().expect("poll isolated runtime") {
                return status;
            }
            assert!(Instant::now() < deadline, "isolated runtime did not exit");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for RuntimeChild {
    fn drop(&mut self) {
        // This handle belongs to the subprocess created by this test and uses
        // only the temp data root above.
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn run_bounded(mut child: Child, timeout: Duration) -> Output {
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait().expect("poll startup invocation").is_some() {
            return child
                .wait_with_output()
                .expect("collect startup invocation output");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("startup invocation remained running; stopped this test-owned process");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn startup_invocation(root: &std::path::Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_swarm-runtime"))
        .arg("--data-dir")
        .arg(root)
        .arg("--windows-startup")
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("invoke startup mode using the isolated test root")
}

fn pipe_endpoints_are_unbound(root: &std::path::Path) {
    let ai_endpoint = swarm_runtime::pipe_name(root).expect("name isolated AI pipe");
    let gui_endpoint = swarm_runtime::gui_pipe_name(root).expect("name isolated GUI pipe");
    assert!(swarm_platform::pipe::connect(&ai_endpoint).is_err());
    assert!(swarm_platform::pipe::connect(&gui_endpoint).is_err());
}

fn startup_invocation_with_args(root: &std::path::Path, extra: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_swarm-runtime"));
    command
        .arg("--data-dir")
        .arg(root)
        .args(extra)
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    run_bounded(
        command.spawn().expect("spawn isolated argument test"),
        Duration::from_secs(5),
    )
}

fn wait_for_gui(root: &std::path::Path, child: &mut RuntimeChild) -> GuiClient {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.0.try_wait().expect("poll runtime startup") {
            panic!("runtime exited before the GUI pipe became ready: {status}");
        }
        if let Ok(client) = GuiClient::open(root)
            && client.setup_status().is_ok()
        {
            return client;
        }
        assert!(Instant::now() < deadline, "GUI pipe did not become ready");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn make_ai_fixture(root: &std::path::Path) -> String {
    let project_root = tempfile::tempdir().expect("isolated project root");
    let mut service = Service::open(root).expect("open isolated service for fixture setup");
    let admin = service
        .request(request(
            "account.setup",
            None,
            json!({"name":ADMIN_NAME,"password":ADMIN_PASSWORD}),
        ))
        .expect("create isolated fixture Admin");
    let project = service
        .request(request(
            "project.register",
            admin["token"].as_str(),
            json!({"name":"Startup Fixture","root":project_root.path()}),
        ))
        .expect("register project only in the isolated fixture");
    let ai = service
        .request(request(
            "identity.start",
            None,
            json!({"project_id":project["id"],"model_name":"Fixture"}),
        ))
        .expect("issue fixture AI identity");
    let candidate = CredentialCandidate::from_grant("identity.start", &ai)
        .expect("convert only this fixture grant");
    swarm_platform::CredentialStore::open(&root.join("client-credentials"))
        .expect("open isolated client credential store")
        .put("agent", &serde_json::to_vec(&candidate).unwrap())
        .expect("save isolated AI credential");
    project["id"].as_str().unwrap().to_owned()
}

fn authenticate_and_stop(client: &GuiClient) -> Result<(), swarm_core::protocol::Fault> {
    client.issue(
        "admin",
        &"a".repeat(64),
        request(
            "account.setup",
            None,
            json!({"name":ADMIN_NAME,"password":ADMIN_PASSWORD}),
        ),
    )?;
    client.request("admin", request("account.logout", None, json!({})))?;
    client.issue(
        "admin",
        &"b".repeat(64),
        request(
            "account.login",
            None,
            json!({"name":ADMIN_NAME,"password":ADMIN_PASSWORD}),
        ),
    )?;
    client.stop("admin")?;
    Ok(())
}

#[test]
fn disabled_windows_startup_does_not_open_the_database_or_bind_pipes() {
    let root = tempfile::tempdir().expect("isolated disabled startup root");
    swarm_platform::CredentialStore::open(&root.path().join("lifecycle"))
        .expect("create isolated lifecycle preference store")
        .put("windows-startup", b"1:disabled")
        .expect("set disabled preference only in temp root");

    let output = run_bounded(startup_invocation(root.path()), Duration::from_secs(5));

    assert_eq!(output.status.code(), Some(7));
    assert!(
        !root.path().join("swarm.sqlite3").exists(),
        "disabled startup must reject before Service opens the SQLite database"
    );
    pipe_endpoints_are_unbound(root.path());
}

#[test]
fn windows_startup_with_missing_data_root_fails_without_creating_it() {
    let temporary = tempfile::tempdir().expect("isolated missing-root parent");
    let root = temporary.path().join("not-created");
    assert!(!root.exists());

    let output = run_bounded(startup_invocation(&root), Duration::from_secs(5));

    assert_eq!(output.status.code(), Some(8));
    assert!(
        !root.exists(),
        "startup must not create a missing data root"
    );
}

#[test]
fn default_enabled_but_unregistered_startup_fails_before_database_or_pipes() {
    let root = tempfile::tempdir().expect("isolated unregistered startup root");
    assert!(swarm_runtime::startup::desired_enabled(root.path()).unwrap());

    let output = run_bounded(startup_invocation(root.path()), Duration::from_secs(5));

    assert_eq!(output.status.code(), Some(8));
    assert!(
        !root.path().join("swarm.sqlite3").exists(),
        "unregistered startup must reject before opening the service database"
    );
    pipe_endpoints_are_unbound(root.path());
}

#[test]
fn malformed_startup_preference_fails_before_database_or_pipes() {
    let root = tempfile::tempdir().expect("isolated malformed preference root");
    swarm_platform::CredentialStore::open(&root.path().join("lifecycle"))
        .expect("create isolated lifecycle preference store")
        .put("windows-startup", b"enabled")
        .expect("seed malformed value only in the temp root");

    let output = run_bounded(startup_invocation(root.path()), Duration::from_secs(5));

    assert_eq!(output.status.code(), Some(8));
    assert!(!root.path().join("swarm.sqlite3").exists());
    pipe_endpoints_are_unbound(root.path());
}

#[test]
fn startup_mode_rejects_unknown_or_mixed_runtime_arguments() {
    let root = tempfile::tempdir().expect("isolated argument-test root");
    for extra in [
        vec!["--windows-startup", "--unknown"],
        vec!["--windows-startup", "--human-start"],
        vec!["--unknown"],
    ] {
        let output = startup_invocation_with_args(root.path(), &extra);
        assert_eq!(output.status.code(), Some(2), "args={extra:?}");
    }
    assert!(!root.path().join("swarm.sqlite3").exists());
    pipe_endpoints_are_unbound(root.path());
}

fn pe_subsystem(image: &[u8]) -> Option<u16> {
    if image.get(..2)? != b"MZ" {
        return None;
    }
    let pe_offset = u32::from_le_bytes(image.get(0x3c..0x40)?.try_into().ok()?) as usize;
    if image.get(pe_offset..pe_offset.checked_add(4)?)? != b"PE\0\0" {
        return None;
    }
    let file_header = pe_offset.checked_add(4)?;
    let optional_size_offset = file_header.checked_add(16)?;
    let optional_size = u16::from_le_bytes(
        image
            .get(optional_size_offset..optional_size_offset + 2)?
            .try_into()
            .ok()?,
    ) as usize;
    let optional_header = file_header.checked_add(20)?;
    let magic = u16::from_le_bytes(
        image
            .get(optional_header..optional_header + 2)?
            .try_into()
            .ok()?,
    );
    if !matches!(magic, 0x10b | 0x20b) || optional_size < 70 {
        return None;
    }
    let subsystem_offset = optional_header.checked_add(68)?;
    Some(u16::from_le_bytes(
        image
            .get(subsystem_offset..subsystem_offset + 2)?
            .try_into()
            .ok()?,
    ))
}

#[test]
fn runtime_executable_declares_windows_gui_subsystem_metadata() {
    let image = std::fs::read(env!("CARGO_BIN_EXE_swarm-runtime"))
        .expect("read only the test-built runtime executable");
    assert_eq!(
        pe_subsystem(&image),
        Some(2),
        "PE OptionalHeader Subsystem=2 is metadata only, not evidence of real logon startup"
    );
}

#[test]
fn ordinary_runtime_connection_preserves_a_valid_stop_marker_from_another_boot() {
    let root = tempfile::tempdir().expect("isolated prior-boot stop root");
    make_ai_fixture(root.path());
    let current_boot = swarm_platform::lifecycle::boot_id().expect("read current Windows boot ID");
    let prior_boot = if current_boot == "15c78b82-3726-4dd6-811c-32b6c5e3337e" {
        "ac055190-a80e-4e20-a2b8-1e94a4a0019b"
    } else {
        "15c78b82-3726-4dd6-811c-32b6c5e3337e"
    };
    assert_ne!(current_boot, prior_boot);
    let lifecycle = swarm_platform::CredentialStore::open(&root.path().join("lifecycle"))
        .expect("open isolated stop store");
    let marker = format!("1:{prior_boot}").into_bytes();
    lifecycle
        .put("full-stop", &marker)
        .expect("seed prior-boot state only in isolated temp root");

    let error = match swarm_runtime::lifecycle::connect(root.path()) {
        Ok(_) => panic!("ordinary runtime connection must honor a prior-boot stop marker"),
        Err(error) => error,
    };
    assert_eq!(error.exit_code, 7);
    assert_eq!(error.code, "RUNTIME_STOPPED");
    assert_eq!(
        lifecycle.get("full-stop").unwrap().as_deref(),
        Some(marker.as_slice()),
        "ordinary CLI cannot clear or rewrite a prior-boot marker"
    );
    assert!(
        swarm_platform::pipe::connect(&swarm_runtime::pipe_name(root.path()).unwrap()).is_err()
    );
}

#[test]
fn current_boot_stop_marker_is_not_cleared_by_windows_startup_mode() {
    let root = tempfile::tempdir().expect("isolated current-boot stop root");
    let mut runtime = RuntimeChild::spawn(root.path());
    let client = wait_for_gui(root.path(), &mut runtime);
    authenticate_and_stop(&client).expect("authenticated human stop in isolated runtime");
    assert!(runtime.wait_for_exit(Duration::from_secs(5)).success());

    let lifecycle = swarm_platform::CredentialStore::open(&root.path().join("lifecycle"))
        .expect("reopen isolated stop store");
    let marker = lifecycle
        .get("full-stop")
        .expect("read isolated stop marker")
        .expect("human stop should persist");
    let boot_id = swarm_platform::lifecycle::boot_id().expect("read current Windows boot ID");
    assert_eq!(marker, format!("2:{boot_id}").as_bytes());

    let output = run_bounded(startup_invocation(root.path()), Duration::from_secs(5));

    assert_eq!(output.status.code(), Some(7));
    assert_eq!(
        lifecycle.get("full-stop").unwrap().as_deref(),
        Some(marker.as_slice())
    );
    assert!(
        swarm_platform::pipe::connect(&swarm_runtime::pipe_name(root.path()).unwrap()).is_err()
    );
}
