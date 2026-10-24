//! Subprocess fixture for a real Service transaction racing human full-stop.
//!
//! These tests run only on Windows because they exercise the protected
//! CredentialStore and named-pipe listeners. The ignored worker is launched by
//! the ordinary test below with a fresh data root and a bounded lifetime.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use swarm_core::protocol::{Fault, Request};
use swarm_core::{CredentialCandidate, Service};
use swarm_platform::CredentialStore;

type StopObserverMap = Mutex<HashMap<PathBuf, mpsc::Sender<()>>>;

fn stop_observers() -> &'static StopObserverMap {
    static OBSERVERS: OnceLock<StopObserverMap> = OnceLock::new();
    OBSERVERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn queued_observers() -> &'static StopObserverMap {
    static OBSERVERS: OnceLock<StopObserverMap> = OnceLock::new();
    OBSERVERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn native_traces() -> &'static Mutex<HashMap<PathBuf, PathBuf>> {
    static TRACES: OnceLock<Mutex<HashMap<PathBuf, PathBuf>>> = OnceLock::new();
    TRACES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn queued_request_read(root: &Path, operation: &str) {
    if let Ok(canonical) = root.canonicalize()
        && let Some(path) = native_traces().lock().unwrap().get(&canonical)
    {
        use std::io::Write;
        let safe_operation = match operation {
            "gui.runtime_stop" => "gui.runtime_stop",
            "gui.notification_activation" => "gui.notification_activation",
            "gui.request" => "gui.request",
            _ => "other",
        };
        let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        writeln!(file, "{}", json!({"operation":safe_operation,"unix_ms":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()})).unwrap();
    }
    if let Ok(root) = root.canonicalize()
        && let Some(sender) = queued_observers().lock().unwrap().get(&root)
    {
        let _ = sender.send(());
    }
}

#[test]
fn queued_mutation_routes_cannot_dispatch_after_the_common_stop_fence() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use swarm_platform::pipe::Listener;
    // Public product mutations and credential/GUI control routes. These are
    // explicit contract cases, not inferred from implementation string matches.
    let operations = [
        "account.setup",
        "account.login",
        "account.create",
        "account.recover",
        "account.google.begin",
        "account.google.link",
        "account.google.unlink",
        "account.google.recover",
        "account.reset",
        "account.password",
        "account.disable",
        "account.logout",
        "role.create",
        "role.assign",
        "permission.set",
        "rules.save",
        "project.register",
        "project.alias",
        "project.root",
        "identity.start",
        "identity.resume",
        "identity.release",
        "identity.model",
        "board.create",
        "board.edit",
        "board.delete",
        "board.restore",
        "thread.create",
        "thread.edit",
        "thread.delete",
        "thread.restore",
        "thread.pin",
        "thread.subscribe",
        "post.read.mark",
        "notification.settings.set",
        "retention.settings.set",
        "display.settings.set",
        "ui.preferences.set",
        "post.create",
        "response.create",
        "post.edit",
        "post.delete",
        "post.restore",
        "session.start",
        "session.resume",
        "gui.issue",
        "gui.request",
        "gui.window_geometry_set",
        "gui.startup_preference_set",
        "gui.reveal_recovery_code",
        "gui.runtime_stop",
        "gui.backup",
        "gui.google_begin_recovery",
    ];
    for batch in operations.chunks(10) {
        let root = tempfile::tempdir().unwrap();
        let canonical = root.path().canonicalize().unwrap();
        let mut service = Service::open(root.path()).unwrap();
        let sessions = super::session::SessionBridge::open(root.path(), &mut service).unwrap();
        let gui = super::gui::GuiAccess::open(root.path()).unwrap();
        let shared: super::SharedService = Arc::new(Mutex::new((service, sessions, gui)));
        let stopping = Arc::new(AtomicBool::new(false));
        let active = Arc::new(AtomicUsize::new(0));
        let (seen_tx, seen_rx) = mpsc::channel();
        queued_observers()
            .lock()
            .unwrap()
            .insert(canonical.clone(), seen_tx);
        // Hold the production gate until every request has been read. This
        // forces the race that a request accepted before stop must recheck.
        let gate = shared.lock().unwrap();
        let mut listeners = Vec::new();
        let mut clients = Vec::new();
        for is_gui in [false, true] {
            let name = if is_gui {
                super::gui_pipe_name(root.path())
            } else {
                super::pipe_name(root.path())
            }
            .unwrap();
            let listener = Listener::bind(&name).unwrap();
            let shared = Arc::clone(&shared);
            let stopping = Arc::clone(&stopping);
            let active = Arc::clone(&active);
            let data_root = root.path().to_path_buf();
            let count = batch.len();
            listeners.push(std::thread::spawn(move || {
                super::serve_listener(
                    listener,
                    Some(count),
                    shared,
                    active,
                    is_gui,
                    stopping,
                    data_root,
                )
            }));
            for &operation in batch {
                let name = name.clone();
                clients.push(std::thread::spawn(move || {
                    let mut connection = swarm_platform::pipe::connect(&name).unwrap();
                    swarm_ipc::write_request(&mut connection, &request(operation, json!({})))
                        .unwrap();
                    swarm_ipc::read_response(&mut connection).unwrap()
                }));
            }
        }
        for _ in 0..batch.len() * 2 {
            seen_rx
                .recv_timeout(Duration::from_secs(15))
                .expect("all accepted requests reach the common gate");
        }
        stopping.store(true, Ordering::Release);
        drop(gate);
        for client in clients {
            let response = client.join().unwrap();
            assert!(!response.ok);
            assert_eq!(response.error.unwrap().code, "RUNTIME_STOPPED");
        }
        for listener in listeners {
            listener.join().unwrap().unwrap();
        }
        queued_observers().lock().unwrap().remove(&canonical);
        assert!(shared.lock().unwrap().0.setup_required().unwrap());
        // No unauthenticated account.setup can have reached the SQLite layer.
    }
}

/// Called by the test-only listener hook after reading a stop request and
/// before trying to acquire the shared service mutex. With no observer it is
/// deliberately a no-op.
pub(super) fn stop_request_read(root: &Path) {
    let Ok(root) = root.canonicalize() else {
        return;
    };
    let sender = stop_observers()
        .lock()
        .ok()
        .and_then(|observers| observers.get(&root).cloned());
    if let Some(sender) = sender {
        let _ = sender.send(());
    }
}

pub(super) struct StopObserver(PathBuf);

impl StopObserver {
    pub(super) fn register(root: &Path) -> (Self, mpsc::Receiver<()>) {
        let root = root.canonicalize().expect("canonical fixture root");
        let (sender, receiver) = mpsc::channel();
        stop_observers()
            .lock()
            .expect("observer registry lock")
            .insert(root.clone(), sender);
        (Self(root), receiver)
    }
}

impl Drop for StopObserver {
    fn drop(&mut self) {
        if let Ok(mut observers) = stop_observers().lock() {
            observers.remove(&self.0);
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

fn fixture_fault() -> Fault {
    Fault::new(
        "DRAIN_FIXTURE_ABORT",
        8,
        "The isolated transaction-drain fixture aborted before commit",
    )
}

fn store_candidate(root: &Path, candidate: &CredentialCandidate) -> Result<(), Fault> {
    let bytes = serde_json::to_vec(candidate).map_err(|_| fixture_fault())?;
    CredentialStore::open(&root.join("drain-fixture-credentials"))
        .and_then(|store| store.put("candidate", &bytes))
        .map_err(|_| fixture_fault())
}

fn read_candidate(root: &Path) -> CredentialCandidate {
    let bytes = CredentialStore::open(&root.join("drain-fixture-credentials"))
        .expect("reopen fixture-only DPAPI store")
        .get("candidate")
        .expect("read fixture candidate")
        .expect("candidate was persisted before the transaction barrier");
    serde_json::from_slice(&bytes).expect("decode fixture candidate")
}

fn native_receipt(root: &Path, name: &str, value: &Value) {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(
            root.parent()
                .expect("owned native fixture parent")
                .join(name),
        )
        .expect("preserve every earlier receipt");
    writeln!(file, "{value}").expect("write safe native receipt");
}

fn run_drain_case(root: &Path, rollback: bool, native: bool) {
    use std::sync::atomic::{AtomicBool, AtomicUsize};
    use swarm_platform::pipe::Listener;

    // Hold the same lease and service tuple as the real runtime. Listener
    // workers use the production request/stop dispatch path.
    let lease = super::lifecycle::runtime_lease(root, super::lifecycle::StartMode::Human)
        .expect("claim isolated runtime lease");
    let mut service = Service::open(root).expect("open isolated SQLite service");
    if native {
        service
            .set_startup_preference(false)
            .expect("fixture startup off");
    }
    let sessions = super::session::SessionBridge::open(root, &mut service)
        .expect("open isolated credential bridge");
    let access = super::gui::GuiAccess::open(root).expect("open isolated GUI access");
    let shared: super::SharedService = Arc::new(Mutex::new((service, sessions, access)));
    if native {
        let path = root.parent().unwrap().join("request-trace.jsonl");
        std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        native_traces()
            .lock()
            .unwrap()
            .insert(root.canonicalize().unwrap(), path);
    }
    let stopping = Arc::new(AtomicBool::new(false));
    let active = Arc::new(AtomicUsize::new(0));

    let ai_listener = Listener::bind(&super::pipe_name(root).expect("AI endpoint"))
        .expect("bind real AI Named Pipe");
    let gui_listener = Listener::bind(&super::gui_pipe_name(root).expect("GUI endpoint"))
        .expect("bind real GUI Named Pipe");

    let ai_thread = {
        let shared = Arc::clone(&shared);
        let stopping = Arc::clone(&stopping);
        let active = Arc::clone(&active);
        let root = root.to_path_buf();
        std::thread::spawn(move || {
            super::serve_listener(ai_listener, None, shared, active, false, stopping, root)
        })
    };
    let gui_thread = {
        let shared = Arc::clone(&shared);
        let stopping = Arc::clone(&stopping);
        let active = Arc::clone(&active);
        let root = root.to_path_buf();
        std::thread::spawn(move || {
            super::serve_listener(gui_listener, None, shared, active, true, stopping, root)
        })
    };

    let client = super::gui::GuiClient::open(root).expect("open isolated GUI client");
    client
        .issue(
            "drain-admin",
            &"a".repeat(64),
            request(
                "account.setup",
                json!({"name":"Drain Fixture Admin","password":"fixture-password-only"}),
            ),
        )
        .expect("create fixture-only Admin through GUI Named Pipe");
    client
        .request("drain-admin", request("account.logout", json!({})))
        .expect("release setup login");
    client
        .issue(
            "drain-admin",
            &"b".repeat(64),
            request(
                "account.login",
                json!({"name":"Drain Fixture Admin","password":"fixture-password-only"}),
            ),
        )
        .expect("authenticate fixture-only Admin");

    if native {
        native_receipt(
            root,
            "server-ready.json",
            &json!({"root":root,"phase":"prepare-native-stop-confirmation","rollback":rollback,"startup_enabled":false}),
        );
        let deadline = Instant::now() + Duration::from_secs(180);
        while !root.parent().unwrap().join("arm-native.json").exists() {
            if Instant::now() >= deadline {
                client
                    .stop("drain-admin")
                    .expect("normal closeout on unarmed deadline");
                ai_thread.join().unwrap().unwrap();
                gui_thread.join().unwrap().unwrap();
                native_receipt(
                    root,
                    "unarmed-closeout.json",
                    &json!({"native_accepted":false,"normal_stop":true}),
                );
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let arm: Value = serde_json::from_slice(
            &std::fs::read(root.parent().unwrap().join("arm-native.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(arm["root"], root.to_string_lossy().as_ref());
        assert_eq!(arm["rollback"], rollback);
    }

    let (entered_tx, entered_rx) = mpsc::channel::<CredentialCandidate>();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (operation_tx, operation_rx) = mpsc::channel::<Result<Value, Fault>>();
    let transaction_root = root.to_path_buf();
    let transaction_shared = Arc::clone(&shared);
    let transaction_thread = std::thread::spawn(move || {
        let result = (|| {
            let mut state = transaction_shared.lock().map_err(|_| fixture_fault())?;
            let (service, _, _) = &mut *state;
            service.request_with_credential_sink(
                request(
                    "account.login",
                    json!({"name":"Drain Fixture Admin","password":"fixture-password-only"}),
                ),
                |grant| {
                    let candidate = CredentialCandidate::from_grant("account.login", grant)?;
                    store_candidate(&transaction_root, &candidate)?;
                    entered_tx.send(candidate).map_err(|_| fixture_fault())?;
                    release_rx
                        .recv_timeout(Duration::from_secs(20))
                        .map_err(|_| fixture_fault())?;
                    if rollback {
                        Err(fixture_fault())
                    } else {
                        Ok(())
                    }
                },
            )
        })();
        let _ = operation_tx.send(result);
    });

    let candidate = entered_rx
        .recv_timeout(Duration::from_secs(15))
        .expect("SQL mutation reached the pre-commit credential sink");
    // The callback persisted this exact candidate through the protected store
    // before signaling; no private value is emitted to test output.
    assert!(
        candidate == read_candidate(root),
        "persisted candidate differs; values withheld"
    );

    let (_observer, stop_read_rx) = StopObserver::register(root);
    let (stop_tx, stop_rx) = mpsc::channel();
    let stop_thread = if native {
        native_receipt(
            root,
            "transaction-entered.json",
            &json!({"root":root,"pre_commit":true,"candidate_persisted_encrypted":true,"stop_marker_present":false,"native_stop_deadline_seconds":8,"rollback":rollback}),
        );
        None
    } else {
        let stop_client = super::gui::GuiClient::open(root).unwrap();
        Some(std::thread::spawn(move || {
            let _ = stop_tx.send(stop_client.stop("drain-admin"));
        }))
    };
    let native_stop_received = stop_read_rx
        .recv_timeout(Duration::from_secs(if native { 8 } else { 15 }))
        .is_ok();
    if !native {
        assert!(
            native_stop_received,
            "runtime read stop request before transaction released its mutex"
        );
    }
    if native {
        native_receipt(
            root,
            "stop-before-release.json",
            &json!({"native_stop_received":native_stop_received,"stop_marker_present":super::lifecycle::stop_record(root).unwrap().is_some(),"transaction_still_owns_gate":true,"rollback":rollback}),
        );
    }
    assert!(
        matches!(stop_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "stop must remain pending while the transaction owns SharedService"
    );
    assert!(
        super::lifecycle::stop_record(root)
            .expect("read stop state")
            .is_none(),
        "stop marker must not persist before the transaction releases the mutex"
    );

    release_tx.send(()).expect("release pre-commit callback");
    let operation_result = operation_rx
        .recv_timeout(Duration::from_secs(15))
        .expect("transaction must produce a definitive result before stop completes");
    if rollback {
        let error = operation_result.expect_err("controlled callback fault must abort commit");
        assert_eq!(error.code, "DRAIN_FIXTURE_ABORT");
    } else {
        let public = operation_result.expect("released transaction must commit");
        assert!(
            public.get("token").is_none(),
            "public result excludes credentials"
        );
    }
    transaction_thread.join().expect("join transaction worker");

    if let Some(stop_thread) = stop_thread {
        assert_eq!(
            stop_rx
                .recv_timeout(Duration::from_secs(15))
                .expect("stop response after transaction finishes")
                .expect("authenticated stop succeeds"),
            json!({"state":"stopped"})
        );
        stop_thread.join().expect("join stop client");
    } else if !native_stop_received {
        client
            .stop("drain-admin")
            .expect("normal cleanup after native deadline, not acceptance");
    }
    ai_thread
        .join()
        .expect("join AI listener")
        .expect("AI listener drains cleanly");
    gui_thread
        .join()
        .expect("join GUI listener")
        .expect("GUI listener drains cleanly");

    drop(shared);
    drop(lease);
    let mut reopened = Service::open(root).expect("reopen SQLite after runtime drain");
    let persisted = read_candidate(root);
    let validity = reopened
        .validate_credential_candidate(&persisted)
        .expect("validate persisted candidate against reopened database");
    assert_eq!(
        validity.session, !rollback,
        "credential validity distinguishes committed and rolled-back login"
    );
    assert!(
        super::lifecycle::stop_record(root)
            .expect("read persisted stop record")
            .is_some(),
        "human stop marker is persisted after the transaction is resolved"
    );
    if native {
        native_receipt(
            root,
            "result.json",
            &json!({"status":if native_stop_received {"NATIVE_PRECOMMIT_STOP_PASS"} else {"NATIVE_STOP_DEADLINE_NORMAL_CLOSEOUT"},"rollback":rollback,"native_stop_received_before_release":native_stop_received,"credential_valid_after_reopen":validity.session,"stop_persisted":true,"listeners_drained":true,"lease_released":true,"frozen_runtime_executable":false}),
        );
    }
}

fn spawn_worker(root: &Path, mode: &str) {
    use std::os::windows::process::CommandExt;

    let mut child = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--ignored")
        .arg("--exact")
        .arg("lifecycle_drain_tests::drain_worker")
        .arg("--nocapture")
        .env("SWARM_TEST_DRAIN_ROOT", root)
        .env("SWARM_TEST_DRAIN_MODE", mode)
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn isolated drain fixture worker");
    wait_worker(&mut child, Duration::from_secs(45));
}

fn wait_worker(child: &mut Child, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().expect("poll isolated worker") {
            assert!(status.success(), "isolated drain fixture failed: {status}");
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("isolated drain fixture exceeded its bounded deadline");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn full_stop_waits_for_started_transaction_commit_and_rollback() {
    for mode in ["commit", "rollback"] {
        let root = tempfile::tempdir().expect("isolated drain data root");
        spawn_worker(root.path(), mode);
        assert!(
            super::lifecycle::stop_record(root.path())
                .expect("reopen isolated lifecycle storage")
                .is_some(),
            "worker must persist stop for the {mode} case"
        );
    }
}

#[test]
#[ignore = "subprocess drain fixture; invoke only through the bounded parent test"]
fn drain_worker() {
    let Ok(root) = std::env::var("SWARM_TEST_DRAIN_ROOT") else {
        return;
    };
    let mode = std::env::var("SWARM_TEST_DRAIN_MODE").expect("fixture mode");
    assert!(matches!(mode.as_str(), "commit" | "rollback"));
    run_drain_case(Path::new(&root), mode == "rollback", false);
}

#[test]
#[ignore = "native GUI worker; use only the fixed owned root and guarded native driver"]
fn native_drain_worker() {
    let mode = std::env::var("SWARM_TEST_NATIVE_DRAIN_MODE").expect("explicit native mode");
    assert!(matches!(
        mode.as_str(),
        "commit" | "rollback" | "rollback-diagnostic"
    ));
    let base =
        Path::new("E:/Swarm-Validation-Host/run-20261022-native-transaction-drain").join(&mode);
    std::fs::create_dir_all(&base).unwrap();
    let root = base.join("Data");
    assert!(!root.exists(), "existing native fixture refused");
    native_receipt(
        &root,
        "attempt.json",
        &json!({"root":root,"mode":mode,"vm":false,"elapsed":false,"h1_mutation":false}),
    );
    run_drain_case(&root, mode != "commit", true);
}
