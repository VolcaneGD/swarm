//! Local runtime with separate AI and private human RPC surfaces.
use std::path::Path;
use swarm_core::{
    Service,
    protocol::{Fault, Request},
};
use swarm_ipc::RpcResponse;

pub mod backup;
pub mod gui;
pub mod lifecycle;
mod maintenance;
#[cfg(windows)]
pub mod notifications;
mod restore;
#[cfg(windows)]
mod restore_apply;
pub mod session;
pub mod startup;
#[cfg(windows)]
mod storage_cleanup;

#[cfg(all(test, windows))]
mod lifecycle_drain_tests;
#[cfg(all(test, windows))]
mod restore_transport_tests;

pub fn gui_pipe_name(root: &Path) -> Result<String, Fault> {
    Ok(format!("{}-gui", pipe_name(root)?))
}

pub fn pipe_name(root: &Path) -> Result<String, Fault> {
    use sha2::{Digest, Sha256};
    let root = std::fs::canonicalize(root).map_err(|_| connection_error())?;
    #[cfg(windows)]
    let bytes = {
        use std::os::windows::ffi::OsStrExt;
        root.as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    };
    #[cfg(not(windows))]
    let bytes = root.as_os_str().as_encoded_bytes().to_vec();
    Ok(format!("Swarm-{:x}", Sha256::digest(bytes)))
}

/// Trusted GUI fallback used only when the runtime pipe is absent. The runtime
/// lease serializes the one-shot metadata access with normal Service owners and
/// does not clear a persisted human full-stop.
#[cfg(windows)]
pub fn window_geometry_get(root: &Path) -> Result<Option<swarm_core::WindowGeometry>, Fault> {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(5);
    let _lease = loop {
        match lifecycle::lease(root, "runtime.lock") {
            Ok(lease) => break lease,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(error) => return Err(error),
        }
    };
    restore_apply::blocked(root)?;
    let service = Service::open(root)?;
    service.window_geometry_get()
}

#[cfg(windows)]
pub fn window_geometry_set(
    root: &Path,
    geometry: &swarm_core::WindowGeometry,
) -> Result<(), Fault> {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(5);
    let _lease = loop {
        match lifecycle::lease(root, "runtime.lock") {
            Ok(lease) => break lease,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(error) => return Err(error),
        }
    };
    restore_apply::blocked(root)?;
    let mut service = Service::open(root)?;
    service.window_geometry_set(geometry)
}

/// Executes a saved-human UI preference operation when the private GUI pipe is
/// absent. This lock does not clear the persisted full-stop, unlike a human
/// runtime launch, and the human bridge still validates the saved credential.
#[cfg(windows)]
pub fn ui_preferences(
    root: &Path,
    alias: &str,
    request: Request,
) -> Result<serde_json::Value, Fault> {
    use std::time::{Duration, Instant};
    if !matches!(
        request.operation.as_str(),
        "ui.preferences.get" | "ui.preferences.set"
    ) || request.auth.is_some()
        || request.request_id.is_some()
        || (!request.context.is_null() && request.context != serde_json::json!({}))
    {
        return Err(Fault::new(
            "INVALID_INPUT",
            2,
            "Invalid UI preference request",
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let _lease = loop {
        match lifecycle::lease(root, "runtime.lock") {
            Ok(lease) => break lease,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(error) => return Err(error),
        }
    };
    restore_apply::blocked(root)?;
    let mut service = Service::open(root)?;
    let mut sessions = session::SessionBridge::open(root, &mut service)?;
    sessions.gui_request(&mut service, alias, request)
}

#[cfg(not(windows))]
pub fn ui_preferences(
    _root: &Path,
    _alias: &str,
    _request: Request,
) -> Result<serde_json::Value, Fault> {
    Err(connection_error())
}

#[cfg(windows)]
pub fn startup_preference_get(root: &Path) -> Result<Option<bool>, Fault> {
    if !root.join("swarm.sqlite3").exists() {
        return Ok(None);
    }
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(5);
    let _lease = loop {
        match lifecycle::lease(root, "runtime.lock") {
            Ok(lease) => break lease,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(error) => return Err(error),
        }
    };
    restore_apply::blocked(root)?;
    Service::open(root)?.startup_preference()
}

#[cfg(windows)]
pub fn startup_preference_set(root: &Path, enabled: bool) -> Result<(), Fault> {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(5);
    let _lease = loop {
        match lifecycle::lease(root, "runtime.lock") {
            Ok(lease) => break lease,
            Err(error) if Instant::now() < deadline => {
                let _ = error;
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(error) => return Err(error),
        }
    };
    restore_apply::blocked(root)?;
    Service::open(root)?.set_startup_preference(enabled)
}

#[cfg(not(windows))]
pub fn window_geometry_get(_root: &Path) -> Result<Option<swarm_core::WindowGeometry>, Fault> {
    Err(connection_error())
}

#[cfg(not(windows))]
pub fn window_geometry_set(
    _root: &Path,
    _geometry: &swarm_core::WindowGeometry,
) -> Result<(), Fault> {
    Err(connection_error())
}

#[cfg(not(windows))]
pub fn startup_preference_get(_root: &Path) -> Result<Option<bool>, Fault> {
    Err(connection_error())
}

#[cfg(not(windows))]
pub fn startup_preference_set(_root: &Path, _enabled: bool) -> Result<(), Fault> {
    Err(connection_error())
}

#[cfg(all(test, windows))]
mod device_settings_tests {
    use super::*;
    use swarm_platform::CredentialStore;

    #[test]
    fn stopped_runtime_geometry_fallback_keeps_human_full_stop() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        Service::open(root).unwrap();
        let protected = CredentialStore::open(&root.join("lifecycle")).unwrap();
        protected
            .put("full-stop", b"1:00000000-0000-4000-8000-000000000001")
            .unwrap();
        let geometry = swarm_core::WindowGeometry {
            version: 1,
            x: -200,
            y: 40,
            width: 1200,
            height: 800,
            maximized: false,
        };
        window_geometry_set(root, &geometry).unwrap();
        assert_eq!(window_geometry_get(root).unwrap(), Some(geometry));
        assert_eq!(
            protected.get("full-stop").unwrap().unwrap(),
            b"1:00000000-0000-4000-8000-000000000001"
        );
    }
}
fn connection_error() -> Fault {
    Fault::new("CONNECTION_ERROR", 8, "Local Swarm connection failed")
}
fn allowed(operation: &str) -> bool {
    matches!(
        operation,
        "board.list"
            | "board.get"
            | "board.create"
            | "board.edit"
            | "thread.list"
            | "thread.get"
            | "thread.create"
            | "thread.edit"
            | "post.list"
            | "post.get"
            | "post.create"
            | "post.edit"
            | "post.history"
            | "response.list"
            | "response.create"
            | "search"
            | "changes.get"
            | "changes.wait"
            | "identity.status"
            | "identity.release"
            | "identity.model"
            | "project.get"
            | "project.list"
            | "project.register"
            | "rules.check"
    )
}
pub fn dispatch(service: &mut Service, request: Request) -> RpcResponse {
    let result = if !allowed(&request.operation) {
        Err(Fault::new(
            "PERMISSION_DENIED",
            4,
            "Operation is unavailable on the AI transport",
        ))
    } else {
        service
            .authenticate_ai_session(request.auth.as_deref())
            .and_then(|()| service.request(request))
    };
    RpcResponse::from_result(result)
}

#[cfg(windows)]
fn dispatch_with_runtime_state(
    service: &mut Service,
    sessions: &mut session::SessionBridge,
    request: Request,
) -> RpcResponse {
    if let Err(error) = restore_apply::blocked(service.data_root()) {
        return RpcResponse::from_result(Err(error));
    }
    if matches!(
        request.operation.as_str(),
        "session.start" | "session.resume"
    ) {
        return RpcResponse::from_result(sessions.dispatch(service, request));
    }
    if !matches!(
        request.operation.as_str(),
        "post.create" | "response.create"
    ) {
        return dispatch(service, request);
    }
    if let Err(error) = service.authenticate_ai_session(request.auth.as_deref()) {
        return RpcResponse::from_result(Err(error));
    }
    let _selection_gate = match sessions.notification_selection_gate() {
        Ok(gate) => gate,
        Err(error) => return RpcResponse::from_result(Err(error)),
    };
    if let Err(error) = sessions.synchronize_notification_recipient_locked(service) {
        return RpcResponse::from_result(Err(error));
    }
    dispatch(service, request)
}

#[cfg(windows)]
pub fn serve(root: &Path, connections: Option<usize>) -> Result<(), Fault> {
    serve_endpoints(
        root,
        connections,
        connections.is_none().then_some(None),
        lifecycle::StartMode::Ordinary,
    )
}

/// Process entry used only for an explicit human launch, never the CLI launcher.
#[cfg(windows)]
pub fn serve_human(root: &Path) -> Result<(), Fault> {
    serve_endpoints(root, None, Some(None), lifecycle::StartMode::Human)
}

/// Dedicated registered Windows logon entry; never sent by ordinary CLI.
#[cfg(windows)]
pub fn serve_windows_startup(root: &Path) -> Result<(), Fault> {
    serve_endpoints(root, None, Some(None), lifecycle::StartMode::WindowsStartup)
}

/// Run both transports against one service; finite limits are for integration tests.
#[cfg(windows)]
pub fn serve_with_gui(
    root: &Path,
    ai_connections: Option<usize>,
    gui_connections: Option<usize>,
) -> Result<(), Fault> {
    serve_endpoints(
        root,
        ai_connections,
        Some(gui_connections),
        lifecycle::StartMode::Ordinary,
    )
}

#[cfg(windows)]
type SharedService =
    std::sync::Arc<std::sync::Mutex<(Service, session::SessionBridge, gui::GuiAccess)>>;

/// Waiters share a process-wide budget, leaving normal requests and human stop
/// requests room in the transport's admission budget.
#[cfg(windows)]
static CHANGE_WAITERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(windows)]
fn wait_for_changes(
    shared: &SharedService,
    stopping: &std::sync::atomic::AtomicBool,
    connection: &mut swarm_platform::pipe::Connection,
    mut request: Request,
    is_gui: bool,
    root: &Path,
) -> RpcResponse {
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};
    struct WaitAdmission;
    impl Drop for WaitAdmission {
        fn drop(&mut self) {
            CHANGE_WAITERS.fetch_sub(1, Ordering::AcqRel);
        }
    }
    let poll = |request: Request| match shared.lock() {
        Ok(mut state) => {
            if stopping.load(Ordering::Acquire) {
                RpcResponse::from_result(Err(drain_fault(root)))
            } else {
                if is_gui {
                    let (service, sessions, gui) = &mut *state;
                    gui.dispatch(service, sessions, request)
                } else {
                    let (service, sessions, _) = &mut *state;
                    dispatch_with_runtime_state(service, sessions, request)
                }
            }
        }
        Err(_) => RpcResponse::from_result(Err(connection_error())),
    };
    // Authentication, scope visibility and strict argument validation all run
    // before a caller can occupy a long-lived waiter or extend its deadline.
    let mut response = poll(request.clone());
    if !response.ok {
        return response;
    }
    let args = if is_gui {
        &request.args["request"]["args"]
    } else {
        &request.args
    };
    let timeout = args
        .get("timeout")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(60);
    if !(1..=300).contains(&timeout) {
        return RpcResponse::from_result(Err(Fault::new(
            "INVALID_ARGUMENT",
            2,
            "Invalid wait timeout",
        )));
    }
    if CHANGE_WAITERS
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
            (count < 16).then_some(count + 1)
        })
        .is_err()
    {
        return RpcResponse::from_result(Err(Fault::new(
            "CONNECTION_ERROR",
            8,
            "Change wait capacity is unavailable",
        )));
    }
    let _admission = WaitAdmission;
    let duration = Duration::from_secs(timeout);
    connection.set_deadline(duration + Duration::from_secs(10));
    let deadline = Instant::now() + duration;
    loop {
        if stopping.load(Ordering::Acquire) {
            return RpcResponse::from_result(Err(drain_fault(root)));
        }
        if !response.ok {
            return response;
        }
        let Some(data) = response.data.as_ref() else {
            return RpcResponse::from_result(Err(connection_error()));
        };
        if data
            .get("items")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|items| !items.is_empty())
        {
            return response;
        }
        if Instant::now() >= deadline {
            let authorized = poll(request.clone());
            return if authorized.ok { response } else { authorized };
        }
        let Some(cursor) = data
            .get("change_cursor")
            .and_then(serde_json::Value::as_str)
        else {
            return RpcResponse::from_result(Err(connection_error()));
        };
        let args = if is_gui {
            &mut request.args["request"]["args"]
        } else {
            &mut request.args
        };
        args["after"] = serde_json::Value::String(cursor.to_owned());
        // No SQL transaction or service mutex is held during this bounded wait.
        std::thread::sleep(
            Duration::from_millis(100).min(deadline.saturating_duration_since(Instant::now())),
        );
        if stopping.load(Ordering::Acquire) {
            return RpcResponse::from_result(Err(drain_fault(root)));
        }
        if Instant::now() >= deadline {
            // Restoration may have expired credentials while this waiter slept.
            let authorized = poll(request.clone());
            return if authorized.ok { response } else { authorized };
        }
        let next = poll(request.clone());
        if stopping.load(Ordering::Acquire) {
            return RpcResponse::from_result(Err(drain_fault(root)));
        }
        // A poll may be delayed by another transaction. Preserve the previous
        // empty cursor on timeout so late visible events remain retrievable.
        if next.ok && Instant::now() >= deadline {
            return response;
        }
        response = next;
    }
}

#[cfg(windows)]
fn drain_fault(root: &Path) -> Fault {
    restore_apply::blocked(root)
        .err()
        .unwrap_or_else(lifecycle::stopped)
}

#[cfg(windows)]
fn serve_endpoints(
    root: &Path,
    connections: Option<usize>,
    gui_connections: Option<Option<usize>>,
    mode: lifecycle::StartMode,
) -> Result<(), Fault> {
    use std::sync::{Arc, Mutex, atomic::AtomicUsize};
    use swarm_platform::pipe::Listener;
    let _lease = lifecycle::runtime_lease(root, mode)?;
    let mut service = Service::open(root)?;
    restore_apply::recover(&mut service, root)?;
    let sessions = session::SessionBridge::open(root, &mut service)?;
    let access = gui::GuiAccess::open(root)?;
    let shared = Arc::new(Mutex::new((service, sessions, access)));
    let listener = Listener::bind(&pipe_name(root)?).map_err(|_| connection_error())?;
    let gui_listener = gui_connections
        .map(|_| Listener::bind(&gui_pipe_name(root)?).map_err(|_| connection_error()))
        .transpose()?;
    let active = Arc::new(AtomicUsize::new(0));
    let stopping = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let notification_worker = {
        let shared = Arc::clone(&shared);
        let stopping = Arc::clone(&stopping);
        let sink = notifications::native_sink(root);
        std::thread::spawn(move || notifications::run(shared, stopping, sink))
    };
    let backup_worker = {
        let shared = Arc::clone(&shared);
        let stopping = Arc::clone(&stopping);
        let root = root.to_owned();
        std::thread::spawn(move || backup::run(shared, stopping, root))
    };
    let maintenance_worker = {
        let shared = Arc::clone(&shared);
        let stopping = Arc::clone(&stopping);
        std::thread::spawn(move || maintenance::run(shared, stopping))
    };
    let gui_worker = match (gui_listener, gui_connections) {
        (Some(listener), Some(limit)) => {
            let shared = Arc::clone(&shared);
            let active = Arc::clone(&active);
            let stopping = Arc::clone(&stopping);
            let root = root.to_path_buf();
            Some(std::thread::spawn(move || {
                let result = serve_listener(
                    listener,
                    limit,
                    shared,
                    active,
                    true,
                    stopping.clone(),
                    root,
                );
                if result.is_err() {
                    stopping.store(true, std::sync::atomic::Ordering::Release);
                }
                result
            }))
        }
        _ => None,
    };
    let result = serve_listener(
        listener,
        connections,
        shared,
        active,
        false,
        stopping.clone(),
        root.to_path_buf(),
    );
    if result.is_err() {
        stopping.store(true, std::sync::atomic::Ordering::Release);
    }
    let gui_result = gui_worker
        .map(|worker| {
            worker
                .join()
                .map_err(|_| connection_error())
                .and_then(|result| result)
        })
        .transpose();
    stopping.store(true, std::sync::atomic::Ordering::Release);
    let notification_result = notification_worker.join().map_err(|_| connection_error());
    let backup_result = backup_worker.join().map_err(|_| connection_error());
    let maintenance_result = maintenance_worker.join().map_err(|_| connection_error());
    notification_result?;
    backup_result?;
    maintenance_result?;
    gui_result?;
    result
}

#[cfg(windows)]
fn serve_listener(
    mut listener: swarm_platform::pipe::Listener,
    connections: Option<usize>,
    shared: SharedService,
    active: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    is_gui: bool,
    stopping: std::sync::Arc<std::sync::atomic::AtomicBool>,
    root: std::path::PathBuf,
) -> Result<(), Fault> {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let mut workers = Vec::new();
    let mut accepted = 0;
    let mut listener_error = None;
    while connections.is_none_or(|limit| accepted < limit) {
        let mut connection = match listener.accept_until(&stopping) {
            Ok(Some(connection)) => connection,
            Ok(None) => break,
            Err(_) => {
                stopping.store(true, Ordering::Release);
                listener_error = Some(connection_error());
                break;
            }
        };
        accepted += 1;
        if active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < 64).then_some(count + 1)
            })
            .is_err()
        {
            continue;
        }
        let shared = Arc::clone(&shared);
        let active = Arc::clone(&active);
        let stopping = Arc::clone(&stopping);
        let root = root.clone();
        let worker = std::thread::spawn(move || {
            struct Admission(Arc<AtomicUsize>);
            impl Drop for Admission {
                fn drop(&mut self) {
                    self.0.fetch_sub(1, Ordering::AcqRel);
                }
            }
            let _admission = Admission(active);
            let response = match swarm_ipc::read_request(&mut connection) {
                Ok(Some(envelope)) => {
                    #[cfg(test)]
                    lifecycle_drain_tests::queued_request_read(&root, &envelope.request.operation);
                    if is_gui && envelope.request.operation == "gui.backup" {
                        // Match GuiClient's backup/restore response budget,
                        // including time queued behind the Service gate. Keep
                        // the short per-I/O stall timeout and recheck authority
                        // in the normal dispatcher after acquiring the gate.
                        connection.set_deadline(std::time::Duration::from_secs(300));
                    }
                    #[cfg(test)]
                    if is_gui {
                        restore_transport_tests::request_read(&root, &envelope.request);
                    }
                    if (!is_gui && envelope.request.operation == "changes.wait")
                        || (is_gui
                            && envelope.request.operation == "gui.request"
                            && envelope.request.args["request"]["operation"] == "changes.wait")
                    {
                        let response = wait_for_changes(
                            &shared,
                            &stopping,
                            &mut connection,
                            envelope.request,
                            is_gui,
                            &root,
                        );
                        let _ = swarm_ipc::write_response(&mut connection, &response);
                        return;
                    }
                    #[cfg(test)]
                    if is_gui && envelope.request.operation == "gui.runtime_stop" {
                        lifecycle_drain_tests::stop_request_read(&root);
                    }
                    match shared.lock() {
                        Ok(mut state) => {
                            if stopping.load(Ordering::Acquire) {
                                return {
                                    let _ = swarm_ipc::write_response(
                                        &mut connection,
                                        &RpcResponse::from_result(Err(drain_fault(&root))),
                                    );
                                };
                            }
                            let (service, sessions, gui) = &mut *state;
                            if is_gui {
                                if envelope.request.operation == "gui.runtime_stop" {
                                    let result = gui
                                        .authorize_stop(service, sessions, envelope.request)
                                        .and_then(|()| lifecycle::persist_stop(&root))
                                        .map(|()| {
                                            stopping.store(true, Ordering::Release);
                                            serde_json::json!({"state":"stopped"})
                                        });
                                    RpcResponse::from_result(result)
                                } else {
                                    gui.dispatch(service, sessions, envelope.request)
                                }
                            } else {
                                dispatch_with_runtime_state(service, sessions, envelope.request)
                            }
                        }
                        Err(_) => RpcResponse::from_result(Err(connection_error())),
                    }
                }
                Ok(None) => return,
                Err(error) => RpcResponse::from_result(Err(error)),
            };
            let _ = swarm_ipc::write_response(&mut connection, &response);
            // A failed cross-store transaction must release the runtime lease
            // after draining. A fresh owner recovers before opening endpoints;
            // do not leave an unauthenticatable runtime holding the lease or
            // record this recovery drain as a human full-stop.
            if restore_apply::blocked(&root).is_err() {
                stopping.store(true, Ordering::Release);
            }
        });
        workers.retain(|worker: &std::thread::JoinHandle<()>| !worker.is_finished());
        workers.push(worker);
    }
    for worker in workers {
        worker.join().map_err(|_| connection_error())?;
    }
    match listener_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
#[cfg(not(windows))]
pub fn serve(_root: &Path, _connections: Option<usize>) -> Result<(), Fault> {
    Err(connection_error())
}
#[cfg(not(windows))]
pub fn serve_human(_root: &Path) -> Result<(), Fault> {
    Err(connection_error())
}
#[cfg(not(windows))]
pub fn serve_windows_startup(_root: &Path) -> Result<(), Fault> {
    Err(connection_error())
}
