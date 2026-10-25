#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use serde_json::Value;
use swarm_core::protocol::Fault;
use swarm_gui_host::GuiHost;
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_dialog::MessageDialogButtons;
use tauri_plugin_opener::OpenerExt;

mod launch;
mod links;
mod updates;
mod window_state;

#[derive(Default)]
struct AvailableUpdate {
    available: Mutex<Option<tauri_plugin_updater::Update>>,
    installing: AtomicBool,
}

struct InstallGuard<'a>(&'a AtomicBool);

#[tauri::command]
async fn gui_bug_report_status(
    window: tauri::WebviewWindow,
    host: State<'_, SharedHost>,
) -> Result<Value, String> {
    if window.label() != "main" {
        return Err("Main window required".into());
    }
    let root = with_host(host.inner().clone(), |host| Ok(host.bug_report_root()))
        .await
        .map_err(|_| "Report host unavailable")?;
    tauri::async_runtime::spawn_blocking(move || swarm_reports::status(&root))
        .await
        .map_err(|_| "Report storage unavailable")?
        .map_err(str::to_owned)
}
#[tauri::command]
async fn gui_bug_report_consent(
    window: tauri::WebviewWindow,
    host: State<'_, SharedHost>,
    expected_account_id: String,
    enabled: bool,
) -> Result<Value, Fault> {
    if window.label() != "main" {
        return Err(host_unavailable());
    }
    with_host(host.inner().clone(), move |host| {
        host.bug_report_consent(&expected_account_id, enabled)
    })
    .await
}
#[tauri::command]
async fn gui_bug_report_submit(
    window: tauri::WebviewWindow,
    host: State<'_, SharedHost>,
    mut report: swarm_reports::Report,
) -> Result<Value, String> {
    if window.label() != "main" {
        return Err("Main window required".into());
    }
    let root = with_host(host.inner().clone(), |host| Ok(host.bug_report_root()))
        .await
        .map_err(|_| "Report host unavailable")?;
    report.app_version = window.package_info().version.to_string();
    report.reporter = "human".to_owned();
    tauri::async_runtime::spawn_blocking(move || swarm_reports::submit(&root, report))
        .await
        .map_err(|_| "Report storage unavailable")?
        .map_err(str::to_owned)
}
#[tauri::command]
async fn gui_bug_report_retry(
    window: tauri::WebviewWindow,
    host: State<'_, SharedHost>,
) -> Result<Value, String> {
    if window.label() != "main" {
        return Err("Main window required".into());
    }
    let root = with_host(host.inner().clone(), |host| Ok(host.bug_report_root()))
        .await
        .map_err(|_| "Report host unavailable")?;
    tauri::async_runtime::spawn_blocking(move || swarm_reports::flush(&root))
        .await
        .map_err(|_| "Report storage unavailable")?
        .map_err(str::to_owned)
}
impl Drop for InstallGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
async fn gui_check_update(
    window: tauri::WebviewWindow,
    state: State<'_, AvailableUpdate>,
) -> Result<Option<Value>, String> {
    if window.label() != "main" {
        return Err("Updates are only available from the main window".into());
    }
    if state.installing.load(Ordering::SeqCst) {
        return Err("UPDATE_BUSY".into());
    }
    *state
        .available
        .lock()
        .map_err(|_| "Update state unavailable")? = None;
    let update = updates::check(window.app_handle()).await?;
    let result = update
        .as_ref()
        .map(|update| serde_json::json!({"version":update.version}));
    *state
        .available
        .lock()
        .map_err(|_| "Update state unavailable")? = update;
    Ok(result)
}

#[tauri::command]
async fn gui_install_update(
    window: tauri::WebviewWindow,
    state: State<'_, AvailableUpdate>,
    host: State<'_, SharedHost>,
    tracker: State<'_, Arc<window_state::WindowTracker>>,
    on_progress: tauri::ipc::Channel<Value>,
) -> Result<(), String> {
    if window.label() != "main" {
        return Err("Updates are only available from the main window".into());
    }
    if state.installing.swap(true, Ordering::SeqCst) {
        return Err("UPDATE_BUSY".into());
    }
    let _guard = InstallGuard(&state.installing);
    let update = state
        .available
        .lock()
        .map_err(|_| "Update state unavailable")?
        .as_ref()
        .cloned()
        .ok_or("No update available")?;
    // In-app installation is supported for the packaged NSIS installation only.
    let exe = std::env::current_exe().map_err(|_| "Could not locate installation")?;
    if !exe
        .parent()
        .is_some_and(|dir| dir.join("swarm-installer.ps1").is_file())
    {
        return Err("UPDATE_INSTALLATION_REQUIRED".into());
    }
    let download_update = update.clone();
    let download_progress = on_progress.clone();
    // A panicking dependency must reject the IPC request instead of leaving the
    // renderer waiting forever. Keep all stop/install side effects after success.
    let bytes = tauri::async_runtime::spawn(async move {
        let mut downloaded = 0u64;
        let _ = download_progress.send(serde_json::json!({"phase":"downloading","downloaded":0}));
        download_update.download(|chunk, total| {
            downloaded = downloaded.saturating_add(chunk as u64);
            let _ = download_progress.send(serde_json::json!({"phase":"downloading","downloaded":downloaded,"total":total}));
        }, || { let _ = download_progress.send(serde_json::json!({"phase":"verifying"})); })
            .await.map_err(|_| "UPDATE_DOWNLOAD_OR_SIGNATURE_FAILED")
    })
    .await.map_err(|_| "UPDATE_DOWNLOAD_OR_SIGNATURE_FAILED")??;
    // download verifies the artifact and signed version before any stop or receipt.
    if !tracker.begin_restore_probe() {
        return Err("UPDATE_BUSY".into());
    }
    // Recheck after download: a human may have typed a draft while it was running.
    // A successful handshake freezes further UI input until process exit/failure.
    let Some((request_id, receiver)) = tracker.start_close_handshake() else {
        tracker.finish_restore();
        return Err("Could not prepare update window".into());
    };
    if window
        .emit(
            "swarm-prepare-close",
            serde_json::json!({"request_id":request_id,"reason":"update"}),
        )
        .is_err()
    {
        tracker.cancel_close_request(request_id);
        tracker.finish_restore();
        return Err("Could not prepare update window".into());
    }
    let acknowledged = tauri::async_runtime::spawn_blocking(move || {
        receiver.recv_timeout(std::time::Duration::from_secs(5))
    })
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or(false);
    tracker.cancel_close_request(request_id);
    if !acknowledged {
        tracker.finish_restore();
        return Err("UPDATE_UNSAVED_WORK".into());
    }
    let geometry = tracker.capture_for_close(&window.as_ref().window());
    let host = host.inner().clone();
    let prepare_host = host.clone();
    let from = window.app_handle().package_info().version.to_string();
    let version = update.version.clone();
    let notes = update.body.clone().unwrap_or_default();
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        let mut host = prepare_host
            .lock()
            .map_err(|_| "Update state unavailable")?;
        if let Some(geometry) = geometry {
            host.window_geometry_set(&geometry.to_saved())
                .map_err(|_| "Could not save window settings")?;
        }
        host.stage_update_receipt(&from, &version, &notes)
            .map_err(|_| "Could not save update receipt")?;
        if host
            .runtime_status()
            .map_err(|_| "Runtime status unavailable")?["state"]
            == "running"
        {
            host.stop_runtime()
                .map_err(|_| "Could not safely stop runtime")?;
        }
        Ok::<_, String>(())
    })
    .await
    .map_err(|_| "Could not prepare update".to_string())
    .and_then(|result| result);
    if let Err(error) = prepared {
        tracker.finish_restore();
        return Err(error);
    }
    let _ = on_progress.send(serde_json::json!({"phase":"installing"}));
    // Windows updater starts NSIS with /UPDATE and /ARGS, preserves --data-dir,
    // exits this process, and the installer relaunches the new application.
    let result = tauri::async_runtime::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|_| "Could not launch update installer");
    tracker.finish_restore();
    match result {
        Ok(Ok(())) => Ok(()),
        _ => Err("UPDATE_INSTALL_FAILED_RUNTIME_STOPPED".into()),
    }
}

#[tauri::command]
async fn gui_update_notes(
    window: tauri::WebviewWindow,
    host: State<'_, SharedHost>,
) -> Result<Option<Value>, String> {
    if window.label() != "main" {
        return Err("Updates are only available from the main window".into());
    }
    let version = window.app_handle().package_info().version.to_string();
    with_host(host.inner().clone(), move |host| {
        host.consume_update_notes(&version)
    })
    .await
    .map_err(|_| "Could not read update notes".into())
}

#[tauri::command]
async fn gui_display_defaults() -> Result<swarm_core::DisplaySettings, Fault> {
    tauri::async_runtime::spawn_blocking(swarm_gui_host::display_defaults)
        .await
        .map_err(|_| Fault::new("INTERNAL", 8, "Windows display defaults could not be read"))?
}

#[tauri::command]
async fn gui_open_link(window: tauri::WebviewWindow, url: String) -> Result<(), String> {
    if window.label() != "main" {
        return Err("External links are only available from the main window".into());
    }
    let url = links::normalize_external_url(&url)?;
    window
        .opener()
        .open_url(url, None::<&str>)
        .map_err(|_| "Could not open the external link".into())
}

type SharedHost = Arc<Mutex<GuiHost>>;

#[tauri::command]
async fn gui_google_link(
    window: tauri::WebviewWindow,
    state: State<'_, SharedHost>,
    expected_account_id: String,
    current_password: String,
) -> Result<Value, Fault> {
    if window.label() != "main" {
        return Err(host_unavailable());
    }
    with_host(state.inner().clone(), move |host| {
        host.google_link(&expected_account_id, &current_password, |url| {
            window
                .opener()
                .open_url(url, None::<&str>)
                .map_err(|_| Fault::new("GOOGLE_BROWSER", 8, "Could not open the system browser"))
        })
    })
    .await
}
#[tauri::command]
async fn gui_google_unlink(
    state: State<'_, SharedHost>,
    expected_account_id: String,
    current_password: String,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.google_unlink(&expected_account_id, &current_password)
    })
    .await
}
#[tauri::command]
async fn gui_google_recover(
    window: tauri::WebviewWindow,
    state: State<'_, SharedHost>,
    name: String,
    password: String,
) -> Result<Value, Fault> {
    if window.label() != "main" {
        return Err(host_unavailable());
    }
    with_host(state.inner().clone(), move |host| {
        host.google_recover(&name, &password, |url| {
            window
                .opener()
                .open_url(url, None::<&str>)
                .map_err(|_| Fault::new("GOOGLE_BROWSER", 8, "Could not open the system browser"))
        })
    })
    .await
}

fn host_unavailable() -> Fault {
    Fault::new("HOST_UNAVAILABLE", 8, "The GUI host is unavailable")
}

async fn with_host<T, F>(host: SharedHost, operation: F) -> Result<T, Fault>
where
    T: Send + 'static,
    F: FnOnce(&mut GuiHost) -> Result<T, Fault> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let mut host = host.lock().map_err(|_| host_unavailable())?;
        operation(&mut host)
    })
    .await
    .map_err(|_| host_unavailable())?
}

#[tauri::command]
async fn gui_snapshot(state: State<'_, SharedHost>) -> Result<Value, Fault> {
    with_host(state.inner().clone(), |host| host.snapshot()).await
}

#[tauri::command]
async fn gui_notification_activation(state: State<'_, SharedHost>) -> Result<Value, Fault> {
    with_host(state.inner().clone(), |host| host.notification_activation()).await
}

#[tauri::command]
async fn gui_notification_manage(
    state: State<'_, SharedHost>,
    expected_account_id: String,
    operation: String,
    args: Value,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.manage_notifications(&expected_account_id, &operation, args)
    })
    .await
}

#[tauri::command]
async fn gui_notification_ack(
    state: State<'_, SharedHost>,
    event_id: String,
) -> Result<bool, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.acknowledge_notification_activation(&event_id)
    })
    .await
}

#[tauri::command]
async fn gui_retention_control(
    state: State<'_, SharedHost>,
    expected_account_id: String,
    operation: String,
    args: Value,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.retention_control(&expected_account_id, &operation, args)
    })
    .await
}

#[tauri::command]
async fn gui_backup_control(
    state: State<'_, SharedHost>,
    expected_account_id: String,
    operation: String,
    args: Value,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.backup_control(&expected_account_id, &operation, args)
    })
    .await
}

#[tauri::command]
async fn gui_restore_pick(
    window: tauri::WebviewWindow,
    state: State<'_, SharedHost>,
    expected_account_id: String,
) -> Result<Option<Value>, Fault> {
    if window.label() != "main" {
        return Err(host_unavailable());
    }
    let shared = state.inner().clone();
    let expected = expected_account_id.clone();
    let directory = with_host(shared.clone(), move |host| {
        host.restore_picker_directory(&expected)
    })
    .await?;
    let picker = window
        .dialog()
        .file()
        .set_parent(&window)
        .set_directory(directory)
        .add_filter("Swarm backup", &["backup"]);
    let selected = tauri::async_runtime::spawn_blocking(move || picker.blocking_pick_file())
        .await
        .map_err(|_| host_unavailable())?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected.into_path().map_err(|_| host_unavailable())?;
    // Recheck selection and administrator after the native picker returns.
    with_host(shared, move |host| {
        host.select_restore_backup(&expected_account_id, &path)
    })
    .await
    .map(Some)
}

#[tauri::command]
async fn gui_restore_status(
    window: tauri::WebviewWindow,
    state: State<'_, SharedHost>,
    tracker: State<'_, Arc<window_state::WindowTracker>>,
    expected_account_id: String,
) -> Result<Value, Fault> {
    let tracker = tracker.inner().clone();
    let restore_probe = tracker.begin_restore_probe();
    let result = with_host(state.inner().clone(), move |host| {
        let status = host.restore_status(&expected_account_id)?;
        let applied = status["state"] == "applied";
        let intent = status["preparation"]["intent"].as_str().map(str::to_owned);
        let geometry = if restore_probe && applied {
            Some(host.window_geometry_get())
        } else {
            None
        };
        Ok((status, intent, geometry))
    })
    .await;
    let (status, intent, geometry) = match result {
        Ok(result) => result,
        Err(error) => {
            if restore_probe {
                tracker.block_persistence();
                tracker.finish_restore();
            }
            return Err(error);
        }
    };
    if restore_probe {
        let should_apply = tracker.persistence_blocked()
            || intent
                .as_deref()
                .map(|intent| tracker.record_restore_intent(intent))
                .unwrap_or(true);
        if status["state"] == "applied" && should_apply {
            match geometry.unwrap_or(Ok(None)) {
                Ok(geometry) => schedule_native_geometry_restore(window, tracker, geometry),
                Err(_) => schedule_native_geometry_error(window, tracker),
            }
        } else {
            if tracker.persistence_blocked()
                && let Some(current) = tracker.snapshot()
            {
                tracker.initialize(current);
            }
            tracker.finish_restore();
        }
    }
    Ok(status)
}
#[tauri::command]
async fn gui_restore_prepare(
    state: State<'_, SharedHost>,
    expected_account_id: String,
    confirmed: bool,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.prepare_restore(&expected_account_id, confirmed)
    })
    .await
}
#[tauri::command]
async fn gui_restore_clear(
    state: State<'_, SharedHost>,
    expected_account_id: String,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.clear_restore_selection(&expected_account_id)
    })
    .await
}

#[tauri::command]
async fn gui_restore_apply(
    window: tauri::WebviewWindow,
    state: State<'_, SharedHost>,
    tracker: State<'_, Arc<window_state::WindowTracker>>,
    expected_account_id: String,
    confirmed: bool,
) -> Result<Value, Fault> {
    if window.label() != "main" {
        return Err(host_unavailable());
    }
    let tracker = tracker.inner().clone();
    if !tracker.begin_restore_probe() {
        return Err(Fault::new(
            "HOST_BUSY",
            8,
            "A restore or close operation is already in progress",
        ));
    }
    let result = with_host(state.inner().clone(), move |host| {
        let prior = host.restore_status(&expected_account_id)?;
        let intent = prior["preparation"]["intent"].as_str().map(str::to_owned);
        let outcome = host.apply_restore(&expected_account_id, confirmed)?;
        let geometry = if outcome["state"] == "applied" {
            Some(host.window_geometry_get())
        } else {
            None
        };
        Ok((outcome, intent, geometry))
    })
    .await;
    let (outcome, intent, geometry) = match result {
        Ok(result) => result,
        Err(error) => {
            tracker.block_persistence();
            tracker.finish_restore();
            return Err(error);
        }
    };
    let should_apply = tracker.persistence_blocked()
        || intent
            .as_deref()
            .map(|intent| tracker.record_restore_intent(intent))
            .unwrap_or(true);
    if outcome["state"] == "applied" && should_apply {
        match geometry.unwrap_or(Ok(None)) {
            Ok(geometry) => schedule_native_geometry_restore(window, tracker, geometry),
            Err(_) => schedule_native_geometry_error(window, tracker),
        }
    } else {
        tracker.finish_restore();
    }
    Ok(outcome)
}

#[tauri::command]
async fn gui_runtime_status(state: State<'_, SharedHost>) -> Result<Value, Fault> {
    with_host(state.inner().clone(), |host| host.runtime_status()).await
}

#[tauri::command]
async fn gui_runtime_start(state: State<'_, SharedHost>) -> Result<Value, Fault> {
    with_host(state.inner().clone(), |host| host.start_runtime()).await
}

#[tauri::command]
async fn gui_runtime_stop(state: State<'_, SharedHost>) -> Result<Value, Fault> {
    with_host(state.inner().clone(), |host| host.stop_runtime()).await
}

#[tauri::command]
async fn gui_startup_status(state: State<'_, SharedHost>) -> Result<Value, Fault> {
    with_host(state.inner().clone(), |host| host.startup_status()).await
}

#[tauri::command]
async fn gui_startup_set(state: State<'_, SharedHost>, enabled: bool) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| host.set_startup(enabled)).await
}

#[tauri::command]
async fn gui_authenticate(
    state: State<'_, SharedHost>,
    mode: String,
    name: String,
    password: String,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.authenticate(&mode, &name, &password)
    })
    .await
}

#[tauri::command]
async fn gui_retry_pending(state: State<'_, SharedHost>) -> Result<Value, Fault> {
    with_host(state.inner().clone(), |host| host.retry_pending()).await
}

#[tauri::command]
async fn gui_set_password(
    state: State<'_, SharedHost>,
    expected_account_id: String,
    current_password: String,
    password: String,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.set_password(&expected_account_id, &current_password, &password)
    })
    .await
}

#[tauri::command]
async fn gui_recover_account(
    state: State<'_, SharedHost>,
    name: String,
    recovery_code: String,
    password: String,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.recover_account(&name, &recovery_code, &password)
    })
    .await
}

#[tauri::command]
async fn gui_cancel_pending(state: State<'_, SharedHost>) -> Result<Value, Fault> {
    with_host(state.inner().clone(), |host| host.cancel_pending()).await
}

#[tauri::command]
async fn gui_reveal_recovery_code(state: State<'_, SharedHost>) -> Result<Value, Fault> {
    with_host(state.inner().clone(), |host| host.reveal_recovery_code()).await
}

#[tauri::command]
async fn gui_logout(state: State<'_, SharedHost>) -> Result<Value, Fault> {
    with_host(state.inner().clone(), |host| host.logout()).await
}

#[tauri::command]
async fn gui_read(
    state: State<'_, SharedHost>,
    operation: String,
    args: Value,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.read(&operation, args)
    })
    .await
}

#[tauri::command]
async fn gui_create(
    state: State<'_, SharedHost>,
    operation: String,
    args: Value,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.create(&operation, args)
    })
    .await
}

#[tauri::command]
async fn gui_edit(state: State<'_, SharedHost>, args: Value) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| host.edit(args)).await
}

#[tauri::command]
async fn gui_manage(
    state: State<'_, SharedHost>,
    operation: String,
    args: Value,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.manage(&operation, args)
    })
    .await
}

#[tauri::command]
async fn gui_account_manage(
    state: State<'_, SharedHost>,
    operation: String,
    args: Value,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.manage_account(&operation, args)
    })
    .await
}

#[tauri::command]
async fn gui_project_control(
    state: State<'_, SharedHost>,
    expected_account_id: String,
    operation: String,
    args: Value,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.project_control(&expected_account_id, &operation, args)
    })
    .await
}

#[tauri::command]
async fn gui_retry_create(state: State<'_, SharedHost>) -> Result<Value, Fault> {
    with_host(state.inner().clone(), |host| host.retry_create()).await
}

#[tauri::command]
async fn gui_authorization_manage(
    state: State<'_, SharedHost>,
    operation: String,
    args: Value,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.manage_authorization(&operation, args)
    })
    .await
}

#[tauri::command]
async fn gui_creation_status(state: State<'_, SharedHost>) -> Result<Value, Fault> {
    with_host(state.inner().clone(), |host| host.creation_status()).await
}

#[tauri::command]
async fn gui_rules_manage(
    state: State<'_, SharedHost>,
    operation: String,
    args: Value,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.manage_rules(&operation, args)
    })
    .await
}

#[tauri::command]
async fn gui_rules_check(
    state: State<'_, SharedHost>,
    args: Value,
    context: Value,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.check_rules(args, context)
    })
    .await
}

#[tauri::command]
async fn gui_ui_preferences(
    state: State<'_, SharedHost>,
    expected_account_id: String,
    operation: String,
    args: Value,
) -> Result<Value, Fault> {
    with_host(state.inner().clone(), move |host| {
        host.ui_preferences(&expected_account_id, &operation, args)
    })
    .await
}

#[tauri::command]
async fn gui_close_ready(
    window: tauri::WebviewWindow,
    tracker: State<'_, Arc<window_state::WindowTracker>>,
    request_id: u64,
    success: bool,
) -> Result<bool, Fault> {
    if window.label() != "main" {
        return Err(host_unavailable());
    }
    Ok(tracker.complete_close_handshake(request_id, success))
}

#[tauri::command]
async fn gui_discard_create(state: State<'_, SharedHost>) -> Result<Value, Fault> {
    with_host(state.inner().clone(), |host| host.discard_create()).await
}

#[tauri::command]
async fn gui_ack_create(state: State<'_, SharedHost>) -> Result<Value, Fault> {
    with_host(state.inner().clone(), |host| host.acknowledge_create()).await
}

fn default_data_root() -> Result<PathBuf, &'static str> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .ok_or("Swarm could not find the local application data directory")?;
    Ok(Path::new(&local_app_data).join("Swarm"))
}

fn run() -> Result<(), &'static str> {
    let launch = launch::parse(std::env::args_os().skip(1), default_data_root()?)?;
    let state_root = launch.root;
    let report_root = state_root.clone();
    std::thread::spawn(move || {
        loop {
            let _ = swarm_reports::flush(&report_root);
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });
    if let Some(uri) = &launch.activation {
        let executable =
            std::env::current_exe().map_err(|_| "Swarm could not locate its GUI executable")?;
        swarm_gui_host::queue_notification_activation(&state_root, &executable, uri)
            .map_err(|_| "Swarm could not validate or save its notification activation")?;
    } else {
        std::fs::create_dir_all(&state_root)
            .map_err(|_| "Swarm could not create its application data directory")?;
        if swarm_gui_host::initialize_startup(&state_root).is_err() {
            eprintln!("Swarm could not apply its Windows startup setting");
        }
        if swarm_gui_host::initialize_notifications(&state_root).is_err() {
            eprintln!("Swarm could not apply its Windows notification registration");
        }
        if swarm_gui_host::start_runtime(&state_root).is_err() {
            eprintln!("Swarm could not start its background runtime");
        }
    }
    let host = match GuiHost::open(&state_root) {
        Ok(host) => host,
        Err(error) if launch.activation.is_some() && error.code == "GUI_ALREADY_RUNNING" => {
            return Ok(());
        }
        Err(_) => return Err("Swarm could not initialize its GUI host"),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .manage(Arc::new(Mutex::new(host)))
        .manage(Arc::new(window_state::WindowTracker::default()))
        .manage(AvailableUpdate::default())
        .on_window_event(|window, event| {
            let Some(tracker) = window
                .app_handle()
                .try_state::<Arc<window_state::WindowTracker>>()
            else {
                return;
            };
            match event {
                tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                    tracker.observe(window);
                }
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    if tracker.take_close_permission() {
                        return;
                    }
                    api.prevent_close();
                    if !tracker.begin_close_save() {
                        if tracker.restore_in_flight() {
                            show_restore_in_progress(window);
                        }
                        return;
                    }
                    let Some(host) = window.app_handle().try_state::<SharedHost>() else {
                        tracker.finish_close_save();
                        show_window_geometry_error(window);
                        return;
                    };
                    let Some((request_id, receiver)) = tracker.start_close_handshake() else {
                        tracker.finish_close_save();
                        show_window_geometry_error(window);
                        return;
                    };
                    if window
                        .emit(
                            "swarm-prepare-close",
                            serde_json::json!({"request_id": request_id}),
                        )
                        .is_err()
                    {
                        tracker.finish_close_save();
                        show_window_geometry_error(window);
                        return;
                    }
                    let host = host.inner().clone();
                    let window = (*window).clone();
                    let app = window.app_handle().clone();
                    let tracker = tracker.inner().clone();
                    tauri::async_runtime::spawn(async move {
                        let acknowledged = tauri::async_runtime::spawn_blocking(move || {
                            receiver.recv_timeout(std::time::Duration::from_secs(5))
                        })
                        .await
                        .ok()
                        .and_then(Result::ok)
                        .unwrap_or(false);
                        tracker.cancel_close_request(request_id);
                        if !acknowledged {
                            tracker.finish_close_save();
                            let _ = app.run_on_main_thread(move || {
                                show_window_geometry_error(&window);
                            });
                            return;
                        }
                        let Some(geometry) = tracker.capture_for_close(&window) else {
                            tracker.retry_save();
                            tracker.finish_close_save();
                            let _ = app.run_on_main_thread(move || {
                                show_window_geometry_error(&window);
                            });
                            return;
                        };
                        let mut latest = geometry;
                        let mut failed = false;
                        let mut current_saved = tracker.persistence_blocked();
                        for _ in 0..if current_saved { 0 } else { 8 } {
                            let write_host = host.clone();
                            let saved = tauri::async_runtime::spawn_blocking(move || {
                                let mut host = write_host.lock().map_err(|_| host_unavailable())?;
                                host.window_geometry_set(&latest.to_saved())
                            })
                            .await
                            .map_err(|_| host_unavailable())
                            .and_then(|result| result);
                            if saved.is_err() {
                                failed = true;
                                break;
                            }
                            match tracker.snapshot() {
                                Some(current) if current != latest => latest = current,
                                _ => {
                                    current_saved = true;
                                    break;
                                }
                            }
                        }
                        if failed || !current_saved {
                            tracker.retry_save();
                            tracker.finish_close_save();
                            let _ = app.run_on_main_thread(move || {
                                show_window_geometry_error(&window);
                            });
                        } else {
                            tracker.permit_close_once();
                            if window.close().is_err() {
                                tracker.finish_close_save();
                                let _ = app.run_on_main_thread(move || {
                                    show_window_geometry_error(&window);
                                });
                            } else {
                                tauri::async_runtime::spawn_blocking(move || {
                                    std::thread::sleep(std::time::Duration::from_secs(2));
                                    tracker.finish_close_save();
                                });
                            }
                        }
                    });
                }
                _ => {}
            }
        })
        .setup(move |app| {
            if let Some(webview_window) = app.get_webview_window("main") {
                let window = webview_window.as_ref().window();
                let saved_result = app
                    .state::<SharedHost>()
                    .lock()
                    .map_err(|_| host_unavailable())
                    .and_then(|mut host| host.window_geometry_get());
                let restore_failed = saved_result.is_err();
                let saved = saved_result.unwrap_or(None);
                let restore_result = window_state::restore(&window, saved);
                app.state::<Arc<window_state::WindowTracker>>()
                    .initialize(restore_result.geometry);
                // The window starts hidden so a stored position can be restored first.
                // Storage and monitor failures must still leave the app usable.
                if window.show().is_err() {
                    eprintln!("Swarm could not show its main window");
                }
                if restore_failed || (saved.is_some() && !restore_result.applied) {
                    app.state::<Arc<window_state::WindowTracker>>()
                        .block_persistence();
                    show_window_geometry_error(&window);
                }
            }
            let handle = app.handle().clone();
            let poll_root = state_root.clone();
            std::thread::spawn(move || {
                let mut seen = None;
                loop {
                    let Some(window) = handle.get_webview_window("main") else {
                        break;
                    };
                    if let Some(geometry) = handle
                        .try_state::<Arc<window_state::WindowTracker>>()
                        .and_then(|tracker| tracker.take_debounced_save(std::time::Instant::now()))
                    {
                        let save_result = if let Some(host) = handle.try_state::<SharedHost>() {
                            if let Ok(mut host) = host.lock() {
                                if handle
                                    .try_state::<Arc<window_state::WindowTracker>>()
                                    .is_some_and(|tracker| tracker.should_write(geometry))
                                {
                                    Some(host.window_geometry_set(&geometry.to_saved()).is_ok())
                                } else {
                                    None
                                }
                            } else {
                                Some(false)
                            }
                        } else {
                            Some(false)
                        };
                        if save_result == Some(false)
                            && let Some(tracker) =
                                handle.try_state::<Arc<window_state::WindowTracker>>()
                        {
                            tracker.retry_save();
                        }
                    }
                    if let Ok(Some(event)) = swarm_gui_host::pending_notification_event(&poll_root)
                        && seen.as_ref() != Some(&event)
                        && window.show().is_ok()
                        && window.unminimize().is_ok()
                        && window.set_focus().is_ok()
                    {
                        seen = Some(event);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            gui_snapshot,
            gui_notification_activation,
            gui_notification_ack,
            gui_notification_manage,
            gui_retention_control,
            gui_backup_control,
            gui_restore_pick,
            gui_restore_status,
            gui_restore_prepare,
            gui_restore_clear,
            gui_restore_apply,
            gui_display_defaults,
            gui_runtime_status,
            gui_runtime_start,
            gui_runtime_stop,
            gui_startup_status,
            gui_startup_set,
            gui_authenticate,
            gui_set_password,
            gui_google_link,
            gui_google_unlink,
            gui_google_recover,
            gui_retry_pending,
            gui_recover_account,
            gui_cancel_pending,
            gui_reveal_recovery_code,
            gui_logout,
            gui_open_link,
            gui_check_update,
            gui_bug_report_status,
            gui_bug_report_consent,
            gui_bug_report_submit,
            gui_bug_report_retry,
            gui_install_update,
            gui_update_notes,
            gui_read,
            gui_create,
            gui_edit,
            gui_manage,
            gui_account_manage,
            gui_project_control,
            gui_authorization_manage,
            gui_rules_manage,
            gui_rules_check,
            gui_ui_preferences,
            gui_close_ready,
            gui_retry_create,
            gui_creation_status,
            gui_discard_create,
            gui_ack_create
        ])
        .run(tauri::generate_context!())
        .map_err(|_| "Swarm desktop could not start")
}

fn show_window_geometry_error(window: &tauri::Window) {
    window
        .dialog()
        .message("Swarm could not finish saving the window bounds or view settings. The window remains open; resolve the storage issue and try again.\n\nウィンドウの位置・サイズまたは画面設定を保存できませんでした。ウィンドウは開いたままです。ストレージの問題を解消して、もう一度お試しください。")
        .title("Swarm")
        .buttons(MessageDialogButtons::Ok)
        .show(|_| {});
}

fn show_restore_in_progress(window: &tauri::Window) {
    window
        .dialog()
        .message("Swarm is applying a backup and will keep the window open until it finishes.\n\nSwarm がバックアップを適用しています。完了するまでウィンドウを開いたままにします。")
        .title("Swarm")
        .buttons(MessageDialogButtons::Ok)
        .show(|_| {});
}

fn schedule_native_geometry_restore(
    window: tauri::WebviewWindow,
    tracker: Arc<window_state::WindowTracker>,
    geometry: Option<swarm_core::WindowGeometry>,
) {
    let callback_window = window.clone();
    let callback_tracker = tracker.clone();
    if window
        .run_on_main_thread(move || {
            let native = callback_window.as_ref().window();
            let result = window_state::restore(&native, geometry);
            callback_tracker.initialize(result.geometry);
            if !result.applied {
                callback_tracker.block_persistence();
                show_window_geometry_error(&native);
            }
            callback_tracker.finish_restore();
        })
        .is_err()
    {
        tracker.finish_restore();
    }
}

fn schedule_native_geometry_error(
    window: tauri::WebviewWindow,
    tracker: Arc<window_state::WindowTracker>,
) {
    let callback_window = window.clone();
    let callback_tracker = tracker.clone();
    if window
        .run_on_main_thread(move || {
            let native = callback_window.as_ref().window();
            show_window_geometry_error(&native);
            callback_tracker.block_persistence();
            callback_tracker.finish_restore();
        })
        .is_err()
    {
        tracker.finish_restore();
    }
}

fn main() {
    if let Err(message) = run() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}
