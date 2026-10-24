//! Local process launch and human full-stop state. CLI launch never clears stop.
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};
use swarm_core::protocol::Fault;
use swarm_platform::{CredentialStore, pipe};

fn storage() -> Fault {
    Fault::new(
        "RUNTIME_STATE",
        8,
        "Runtime state could not be read or saved",
    )
}
fn unavailable() -> Fault {
    Fault::new("CONNECTION_ERROR", 8, "Swarm runtime is unavailable")
}
pub fn stopped() -> Fault {
    Fault::new(
        "RUNTIME_STOPPED",
        7,
        "Swarm was completely stopped by a human; reopen or explicitly start it from the GUI",
    )
}

fn store(root: &Path) -> Result<CredentialStore, Fault> {
    CredentialStore::open(&root.join("lifecycle")).map_err(|_| storage())
}
pub(crate) enum StopRecord {
    Current(String),
    Legacy,
}

pub(crate) fn stop_record(root: &Path) -> Result<Option<StopRecord>, Fault> {
    match fs::symlink_metadata(root.join("lifecycle")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(storage()),
        Ok(_) => {}
    }
    let Some(bytes) = store(root)?.get("full-stop").map_err(|_| storage())? else {
        return Ok(None);
    };
    let text = std::str::from_utf8(&bytes).map_err(|_| storage())?;
    let (record, id) = if let Some(id) = text.strip_prefix("2:") {
        (StopRecord::Current(id.to_owned()), id)
    } else if let Some(id) = text.strip_prefix("1:") {
        // Version 1 stored the boot-entry GUID as if it were a per-boot
        // identity. Preserve the stop, but never use it as reboot evidence.
        (StopRecord::Legacy, id)
    } else {
        return Err(storage());
    };
    if uuid::Uuid::parse_str(id).map_err(|_| storage())?.is_nil() {
        return Err(storage());
    }
    Ok(Some(record))
}

/// Ordinary CLI never clears a stop, even when the Windows boot has changed.
pub fn check_running_allowed(root: &Path) -> Result<(), Fault> {
    if stop_record(root)?.is_some() {
        return Err(stopped());
    }
    Ok(())
}
pub(crate) fn persist_stop(root: &Path) -> Result<(), Fault> {
    let boot = swarm_platform::lifecycle::boot_id().map_err(|_| storage())?;
    store(root)?
        .put("full-stop", format!("2:{boot}").as_bytes())
        .map_err(|_| storage())
}

/// A read-only local status probe: no credentials, launch, DB, or stop reset.
pub fn status(root: &Path) -> Result<Value, Fault> {
    if stop_record(root)?.is_some() {
        return Ok(json!({"state":"stopped"}));
    }
    let running = crate::pipe_name(root)
        .ok()
        .is_some_and(|name| pipe::connect(&name).is_ok());
    Ok(json!({"state":if running {"running"} else {"not_running"}}))
}
pub fn connect_existing(root: &Path) -> Result<pipe::Connection, Fault> {
    check_running_allowed(root)?;
    pipe::connect(&crate::pipe_name(root)?).map_err(|_| unavailable())
}
pub fn connect(root: &Path) -> Result<pipe::Connection, Fault> {
    check_running_allowed(root)?;
    if let Ok(connection) = connect_existing(root) {
        return Ok(connection);
    }
    launch(root, false)?;
    connect_existing(root)
}
/// Called only by the Rust desktop host in response to human application launch
/// or an explicit Start button; never by CLI or AI RPC.
pub fn start_human(root: &Path) -> Result<(), Fault> {
    launch(root, true)
}

#[cfg(windows)]
pub(crate) fn lease(root: &Path, name: &str) -> Result<fs::File, Fault> {
    store(root)?.private_lease(name).map_err(|error| {
        if matches!(
            error.kind(),
            std::io::ErrorKind::InvalidData | std::io::ErrorKind::InvalidInput
        ) {
            storage()
        } else {
            unavailable()
        }
    })
}
#[cfg(not(windows))]
pub(crate) fn lease(_root: &Path, _name: &str) -> Result<fs::File, Fault> {
    Err(unavailable())
}

/// Claimed before opening Service/Broker, held until all admitted requests drain.
#[derive(Clone, Copy)]
pub(crate) enum StartMode {
    Ordinary,
    Human,
    WindowsStartup,
}

pub(crate) fn runtime_lease(root: &Path, mode: StartMode) -> Result<fs::File, Fault> {
    if matches!(mode, StartMode::WindowsStartup) && !root.is_dir() {
        return Err(storage());
    }
    fs::create_dir_all(root).map_err(|_| storage())?;
    let lease = lease(root, "runtime.lock")?;
    match mode {
        StartMode::Human => store(root)?.remove("full-stop").map_err(|_| storage())?,
        StartMode::Ordinary => check_running_allowed(root)?,
        StartMode::WindowsStartup => {
            #[cfg(windows)]
            crate::startup::authorize_start(root)?;
            #[cfg(not(windows))]
            return Err(unavailable());
        }
    }
    Ok(lease)
}

#[cfg(windows)]
fn launch(root: &Path, human: bool) -> Result<(), Fault> {
    if !human {
        check_running_allowed(root)?;
    }
    fs::create_dir_all(root).map_err(|_| storage())?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let _launch_lease = loop {
        match lease(root, "launch.lock") {
            Ok(lease) => break lease,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(40)),
            Err(error) => return Err(error),
        }
    };
    let executable = std::env::current_exe().map_err(|_| unavailable())?;
    let runtime = executable
        .parent()
        .ok_or_else(unavailable)?
        .join("swarm-runtime.exe");
    if !runtime.is_file() {
        return Err(unavailable());
    }
    if human {
        // A runtime still draining holds its lease; wait before clearing stop.
        let _owner = loop {
            match lease(root, "runtime.lock") {
                Ok(lease) => break lease,
                Err(_) if stop_record(root)?.is_some() && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(40))
                }
                Err(_) if stop_record(root)?.is_none() && Instant::now() < deadline => {
                    if crate::pipe_name(root)
                        .ok()
                        .is_some_and(|name| pipe::connect(&name).is_ok())
                    {
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(40));
                }
                Err(error) => return Err(error),
            }
        };
        store(root)?.remove("full-stop").map_err(|_| storage())?;
    } else {
        check_running_allowed(root)?;
    }
    let name = crate::pipe_name(root)?;
    if pipe::connect(&name).is_ok() {
        return Ok(());
    }
    let child = swarm_platform::process::spawn_background(
        &runtime,
        &[
            std::ffi::OsString::from("--data-dir"),
            root.as_os_str().to_owned(),
        ],
    )
    .map_err(|_| unavailable())?;
    loop {
        check_running_allowed(root)?;
        if pipe::connect(&name).is_ok() {
            return Ok(());
        }
        if child.has_exited().map_err(|_| unavailable())? || Instant::now() >= deadline {
            return Err(unavailable());
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}
#[cfg(not(windows))]
fn launch(_root: &Path, _human: bool) -> Result<(), Fault> {
    Err(unavailable())
}
