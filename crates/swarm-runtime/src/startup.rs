//! Desired startup state is protected separately from the Windows registration.
//! These Rust host functions are not exposed on either AI RPC or the CLI.
use serde_json::{Value, json};
use std::{fs, path::Path};
use swarm_core::{Service, protocol::Fault};
use swarm_platform::CredentialStore;

fn state_error() -> Fault {
    Fault::new(
        "RUNTIME_STARTUP_STATE",
        8,
        "Windows startup setting could not be read or applied",
    )
}
fn disabled() -> Fault {
    Fault::new("RUNTIME_STARTUP_DISABLED", 7, "Windows startup is disabled")
}
fn store(root: &Path) -> Result<CredentialStore, Fault> {
    CredentialStore::open(&root.join("lifecycle")).map_err(|_| state_error())
}
fn preference(root: &Path) -> Result<Option<bool>, Fault> {
    match fs::symlink_metadata(root.join("lifecycle")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(state_error()),
        Ok(_) => {}
    }
    match store(root)?
        .get("windows-startup")
        .map_err(|_| state_error())?
        .as_deref()
    {
        None => Ok(None),
        Some(b"1:enabled") => Ok(Some(true)),
        Some(b"1:disabled") => Ok(Some(false)),
        _ => Err(state_error()),
    }
}

/// Missing settings default to enabled, without creating a setting on a read.
pub fn desired_enabled(root: &Path) -> Result<bool, Fault> {
    if let Some(value) = database_preference(root)? {
        return Ok(value);
    }
    Ok(preference(root)?.unwrap_or(true))
}

fn database_preference(root: &Path) -> Result<Option<bool>, Fault> {
    if !root.join("swarm.sqlite3").exists() {
        return Ok(None);
    }
    match crate::gui::GuiClient::open(root) {
        Ok(client) => client.startup_preference_get(),
        Err(error) if error.code == "CONNECTION_ERROR" => crate::startup_preference_get(root),
        Err(error) => Err(error),
    }
}

fn database_preference_under_runtime_lock(root: &Path) -> Result<Option<bool>, Fault> {
    if !root.join("swarm.sqlite3").exists() {
        return Ok(None);
    }
    Service::open(root)?.startup_preference()
}

fn persist_database_preference(root: &Path, enabled: bool) -> Result<(), Fault> {
    match crate::gui::GuiClient::open(root) {
        Ok(client) => client.startup_preference_set(enabled),
        Err(error) if error.code == "CONNECTION_ERROR" => {
            crate::startup_preference_set(root, enabled)
        }
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn registration(
    root: &Path,
    executable: &Path,
) -> Result<swarm_platform::startup::Registration, Fault> {
    swarm_platform::startup::Registration::new(executable, root).map_err(|_| state_error())
}

/// Registration is an explicit host/installer action, never a side effect of
/// status, ordinary runtime start, CLI use, or the test server.
#[cfg(windows)]
pub fn configure(root: &Path, executable: &Path, enabled: bool) -> Result<Value, Fault> {
    fs::create_dir_all(root).map_err(|_| state_error())?;
    // Store through the live Service or its runtime-lease fallback before
    // taking startup.lock, preserving the runtime.lock -> startup.lock order.
    persist_database_preference(root, enabled)?;
    let _lock = super::lifecycle::lease(root, "startup.lock")?;
    configure_locked(root, executable, enabled)
}

#[cfg(windows)]
fn configure_locked(root: &Path, executable: &Path, enabled: bool) -> Result<Value, Fault> {
    // Persist disable intent before touching Run: a surviving stale entry must
    // not start the service. If registration fails, return an error and retain
    // the desired/actual distinction for the next explicit recovery attempt.
    store(root)?
        .put(
            "windows-startup",
            if enabled { b"1:enabled" } else { b"1:disabled" },
        )
        .map_err(|_| state_error())?;
    let registration = registration(root, executable)?;
    registration
        .set_enabled(enabled)
        .map_err(|_| state_error())?;
    status_locked(root, executable)
}

/// Called only at the explicit desktop/installer initialization boundary.
/// Existing desired state, including a failed registration, is not overwritten.
#[cfg(windows)]
pub fn initialize(root: &Path, executable: &Path) -> Result<(), Fault> {
    fs::create_dir_all(root).map_err(|_| state_error())?;
    // Read before startup.lock: the offline reader takes runtime.lock first.
    let archived = database_preference(root)?;
    let _lock = super::lifecycle::lease(root, "startup.lock")?;
    let protected = preference(root)?;
    match (archived, protected) {
        (Some(desired), Some(mirrored)) if desired == mirrored => {}
        (Some(desired), _) => {
            configure_locked(root, executable, desired)?;
        }
        (None, Some(mirrored)) => {
            drop(_lock);
            configure(root, executable, mirrored)?;
        }
        (None, None) => {
            drop(_lock);
            configure(root, executable, true)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn status_locked(root: &Path, executable: &Path) -> Result<Value, Fault> {
    let preference = preference(root)?;
    let database_preference = database_preference(root)?;
    let registered = registration(root, executable)?
        .is_registered()
        .map_err(|_| state_error())?;
    Ok(
        json!({"enabled":database_preference.or(preference).unwrap_or(true), "registered":registered, "configured":database_preference.is_some() || preference.is_some()}),
    )
}

#[cfg(windows)]
pub fn status(root: &Path, executable: &Path) -> Result<Value, Fault> {
    // No setting/registry creation on read. Registration requires an existing
    // canonical data directory; an unopened installation is unconfigured.
    if !root.exists() {
        return Ok(json!({"enabled":true,"registered":false,"configured":false}));
    }
    status_locked(root, executable)
}

/// Held under the runtime owner lease, so an old runtime cannot still be
/// recording a stop while this entry considers clearing it.
#[cfg(windows)]
pub(crate) fn authorize_start(root: &Path) -> Result<(), Fault> {
    authorize_start_with(
        root,
        || {
            let executable = std::env::current_exe().map_err(|_| state_error())?;
            registration(root, &executable)?
                .is_registered()
                .map_err(|_| state_error())
        },
        || swarm_platform::lifecycle::boot_id().map_err(|_| state_error()),
    )
}

#[cfg(windows)]
fn authorize_start_with(
    root: &Path,
    registered: impl FnOnce() -> Result<bool, Fault>,
    boot: impl FnOnce() -> Result<String, Fault>,
) -> Result<(), Fault> {
    let _lock = super::lifecycle::lease(root, "startup.lock")?;
    // runtime_lease is already held and Service has not yet been constructed.
    let enabled = database_preference_under_runtime_lock(root)?
        .or(preference(root)?)
        .unwrap_or(true);
    if !enabled {
        return Err(disabled());
    }
    let stopped_boot = super::lifecycle::stop_record(root)?;
    if matches!(
        stopped_boot.as_ref(),
        Some(super::lifecycle::StopRecord::Legacy)
    ) {
        return Err(super::lifecycle::stopped());
    }
    let current_boot = stopped_boot.as_ref().map(|_| boot()).transpose()?;
    if let (Some(super::lifecycle::StopRecord::Current(previous)), Some(current)) =
        (&stopped_boot, &current_boot)
    {
        // Same-boot relogin is prohibited even if registration is missing.
        may_resume(enabled, true, Some(previous), current)?;
    }
    let registered = registered()?;
    if !registered {
        return Err(state_error());
    }
    if stopped_boot.is_some() {
        store(root)?
            .remove("full-stop")
            .map_err(|_| state_error())?;
    }
    Ok(())
}

/// Pure policy tests simulate distinct boot IDs; they are not real reboot evidence.
#[cfg(any(windows, test))]
fn may_resume(
    enabled: bool,
    registered: bool,
    stopped_boot: Option<&str>,
    current_boot: &str,
) -> Result<(), Fault> {
    if !enabled {
        return Err(disabled());
    }
    if !registered {
        return Err(state_error());
    }
    let current = uuid::Uuid::parse_str(current_boot).map_err(|_| state_error())?;
    if current.is_nil() {
        return Err(state_error());
    }
    if let Some(stopped_boot) = stopped_boot {
        let previous = uuid::Uuid::parse_str(stopped_boot).map_err(|_| state_error())?;
        if previous.is_nil() {
            return Err(state_error());
        }
        if previous == current {
            return Err(super::lifecycle::stopped());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIRST: &str = "15c78b82-3726-4dd6-811c-32b6c5e3337e";
    const NEXT: &str = "ac055190-a80e-4e20-a2b8-1e94a4a0019b";
    #[test]
    fn same_boot_relogin_never_clears_stop() {
        assert_eq!(
            may_resume(true, true, Some(FIRST), FIRST).unwrap_err().code,
            "RUNTIME_STOPPED"
        );
        assert_eq!(
            may_resume(true, true, Some(FIRST), &FIRST.to_uppercase())
                .unwrap_err()
                .code,
            "RUNTIME_STOPPED"
        );
    }
    #[test]
    fn only_enabled_valid_registration_on_a_different_boot_resumes() {
        assert!(may_resume(true, true, Some(FIRST), NEXT).is_ok());
        assert!(may_resume(true, true, None, FIRST).is_ok());
        assert_eq!(
            may_resume(false, true, Some(FIRST), NEXT).unwrap_err().code,
            "RUNTIME_STARTUP_DISABLED"
        );
        assert_eq!(
            may_resume(true, false, Some(FIRST), NEXT).unwrap_err().code,
            "RUNTIME_STARTUP_STATE"
        );
    }
    #[test]
    fn unknown_boot_identity_fails_closed() {
        assert!(may_resume(true, true, Some(FIRST), "").is_err());
        assert!(may_resume(true, true, Some("broken"), NEXT).is_err());
        assert!(
            may_resume(
                true,
                true,
                Some(FIRST),
                "00000000-0000-0000-0000-000000000000"
            )
            .is_err()
        );
    }
    #[cfg(windows)]
    #[test]
    fn preference_defaults_without_creating_and_rejects_corruption() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("data");
        assert!(desired_enabled(&root).unwrap());
        assert!(!root.exists());
        fs::create_dir(&root).unwrap();
        store(&root)
            .unwrap()
            .put("windows-startup", b"1:disabled")
            .unwrap();
        assert!(!desired_enabled(&root).unwrap());
        store(&root)
            .unwrap()
            .put("windows-startup", b"true")
            .unwrap();
        assert_eq!(
            desired_enabled(&root).unwrap_err().code,
            "RUNTIME_STARTUP_STATE"
        );
    }

    #[cfg(windows)]
    #[test]
    fn registration_failure_retains_disable_intent_and_initialize_preserves_it() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        // Invalid executable fails before any registry access, using real
        // protected preference persistence rather than a production Run entry.
        let missing = root.join("missing-runtime.exe");
        assert_eq!(
            configure(root, &missing, false).unwrap_err().code,
            "RUNTIME_STARTUP_STATE"
        );
        assert!(!desired_enabled(root).unwrap());
        assert!(initialize(root, &missing).is_ok());
        assert!(!desired_enabled(root).unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn first_initialization_defaults_enabled_but_reports_registration_failure() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let missing = root.join("missing-runtime.exe");
        assert_eq!(
            initialize(root, &missing).unwrap_err().code,
            "RUNTIME_STARTUP_STATE"
        );
        assert_eq!(preference(root).unwrap(), Some(true));
        // An explicit retry is required; a second human launch must not
        // silently rewrite a saved setting or hide its failed registration.
        assert!(initialize(root, &missing).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn restored_database_startup_preference_is_reconciled_on_normal_initialization() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        Service::open(root)
            .unwrap()
            .set_startup_preference(false)
            .unwrap();
        store(root)
            .unwrap()
            .put("windows-startup", b"1:enabled")
            .unwrap();
        let missing = root.join("missing-runtime.exe");
        assert_eq!(
            initialize(root, &missing).unwrap_err().code,
            "RUNTIME_STARTUP_STATE"
        );
        assert!(!desired_enabled(root).unwrap());
        assert_eq!(preference(root).unwrap(), Some(false));
    }

    #[cfg(windows)]
    #[test]
    fn protected_stop_is_removed_only_after_all_startup_checks() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let credentials = store(root).unwrap();
        let marker = format!("2:{FIRST}").into_bytes();
        credentials.put("full-stop", &marker).unwrap();
        // Real protected files/leases, simulated registration and boot providers.
        assert_eq!(
            authorize_start_with(
                root,
                || panic!("same boot must refuse before registry"),
                || Ok(FIRST.into())
            )
            .unwrap_err()
            .code,
            "RUNTIME_STOPPED"
        );
        assert_eq!(credentials.get("full-stop").unwrap().unwrap(), marker);
        assert!(authorize_start_with(root, || Ok(false), || Ok(NEXT.into())).is_err());
        assert_eq!(credentials.get("full-stop").unwrap().unwrap(), marker);
        assert!(authorize_start_with(root, || Err(state_error()), || Ok(NEXT.into())).is_err());
        assert_eq!(credentials.get("full-stop").unwrap().unwrap(), marker);
        credentials.put("windows-startup", b"1:disabled").unwrap();
        assert_eq!(
            authorize_start_with(
                root,
                || panic!("disabled must not query registry"),
                || panic!("disabled must not query boot")
            )
            .unwrap_err()
            .code,
            "RUNTIME_STARTUP_DISABLED"
        );
        assert_eq!(credentials.get("full-stop").unwrap().unwrap(), marker);
        credentials.put("windows-startup", b"1:enabled").unwrap();
        assert!(authorize_start_with(root, || Ok(true), || Ok(NEXT.into())).is_ok());
        assert!(credentials.get("full-stop").unwrap().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn legacy_boot_entry_stop_fails_closed_until_human_start() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let credentials = store(root).unwrap();
        let legacy = format!("1:{FIRST}").into_bytes();
        credentials.put("full-stop", &legacy).unwrap();

        // The v1 value still represents a stop, but is not reboot evidence.
        // Reject before boot or registration checks can authorize a reset.
        assert_eq!(
            authorize_start_with(
                root,
                || panic!("legacy marker must fail before registration"),
                || panic!("legacy marker must fail before boot query")
            )
            .unwrap_err()
            .code,
            "RUNTIME_STOPPED"
        );
        assert_eq!(credentials.get("full-stop").unwrap().unwrap(), legacy);
        assert_eq!(
            super::super::lifecycle::check_running_allowed(root)
                .unwrap_err()
                .code,
            "RUNTIME_STOPPED"
        );
        assert_eq!(credentials.get("full-stop").unwrap().unwrap(), legacy);

        // Human Start is the explicit migration boundary.
        super::super::lifecycle::runtime_lease(root, super::super::lifecycle::StartMode::Human)
            .unwrap();
        assert!(credentials.get("full-stop").unwrap().is_none());
    }
}
