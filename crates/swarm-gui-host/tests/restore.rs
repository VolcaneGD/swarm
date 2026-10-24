#![cfg(windows)]
use std::time::{Duration, Instant};
use swarm_gui_host::GuiHost;

#[test]
fn native_selection_is_checked_by_private_runtime_without_applying_input() {
    let root = tempfile::tempdir().unwrap();
    let original = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let archives = destination.path().join("archives");
    let service = swarm_core::Service::open(original.path()).unwrap();
    let prepared = service.prepare_backup().unwrap();
    swarm_platform::BackupStore::open(&archives)
        .unwrap()
        .write_new("selected", |f| {
            prepared
                .write_archive(f, swarm_core::BackupKind::Manual)
                .map_err(|_| swarm_platform::PlatformError::Io)
        })
        .unwrap();
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(80)).unwrap()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(name) = swarm_runtime::gui_pipe_name(root.path())
            && let Ok(pipe) = swarm_platform::pipe::connect(&name)
        {
            drop(pipe);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut host = GuiHost::open(root.path()).unwrap();
    let snapshot = host
        .authenticate("setup", "Admin", "fixture-password")
        .unwrap();
    let id = snapshot["account"]["id"].as_str().unwrap().to_owned();
    let mut host = Some(host);
    let checks = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let host_ref = host.as_mut().unwrap();
        let result = host_ref
            .select_restore_backup(&id, &archives.join("selected.backup"))
            .unwrap();
        assert_eq!(result["state"], "selected");
        assert!(result["preview"]["manifest"]["rules"][0]["content"].is_string());
        assert!(!root.path().join("restore-safety").exists());
        assert_eq!(
            host_ref.prepare_restore(&id, false).unwrap_err().code,
            "RESTORE_CONFIRMATION"
        );
        assert_eq!(
            host_ref
                .select_restore_backup("stale-account", &archives.join("selected.backup"))
                .unwrap_err()
                .code,
            "ACCOUNT_CHANGED"
        );
        let result = host_ref.prepare_restore(&id, true).unwrap();
        assert_eq!(result["state"], "prepared");
        let preparation = result["preparation"].clone();
        assert!(
            host_ref
                .select_restore_backup(&id, &archives.join("selected.backup"))
                .is_err()
        );
        drop(host.take());
        host = Some(GuiHost::open(root.path()).unwrap());
        let reopened = host.as_mut().unwrap();
        assert_eq!(
            reopened.restore_status(&id).unwrap()["preparation"],
            preparation
        );
        assert_eq!(
            reopened.prepare_restore(&id, true).unwrap()["preparation"],
            preparation
        );
        assert!(reopened.clear_restore_selection(&id).unwrap().is_null());
        assert!(reopened.restore_status(&id).unwrap().is_null());
        assert_eq!(
            std::fs::read_dir(root.path().join("restore-safety"))
                .unwrap()
                .count(),
            1
        );
    }));
    host.as_mut().unwrap().stop_runtime().unwrap();
    server.join().unwrap();
    if let Err(error) = checks {
        std::panic::resume_unwind(error);
    }
}

#[test]
fn restore_application_over_real_pipe_requires_final_confirmation_and_reauthentication() {
    use serde_json::json;
    let root = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let archives = destination.path().join("archives");
    {
        let mut service = swarm_core::Service::open(root.path()).unwrap();
        service
            .request(swarm_core::protocol::Request {
                operation: "account.setup".into(),
                auth: None,
                args: json!({"name":"Admin","password":"fixture-password"}),
                context: json!({}),
                request_id: None,
            })
            .unwrap();
        swarm_platform::BackupStore::open(&archives)
            .unwrap()
            .write_new("selected", |f| {
                service
                    .prepare_backup()
                    .unwrap()
                    .write_archive(f, swarm_core::BackupKind::Manual)
                    .map_err(|_| swarm_platform::PlatformError::Io)
            })
            .unwrap();
    }
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(80)).unwrap()
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(name) = swarm_runtime::gui_pipe_name(root.path())
            && let Ok(pipe) = swarm_platform::pipe::connect(&name)
        {
            drop(pipe);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut host = GuiHost::open(root.path()).unwrap();
    let snapshot = host
        .authenticate("login", "Admin", "fixture-password")
        .unwrap();
    let id = snapshot["account"]["id"].as_str().unwrap().to_owned();
    let checks = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host.select_restore_backup(&id, &archives.join("selected.backup"))
            .unwrap();
        assert!(host.apply_restore(&id, true).is_err()); // selected is not prepared
        host.prepare_restore(&id, true).unwrap();
        assert_eq!(
            host.apply_restore(&id, false).unwrap_err().code,
            "RESTORE_CONFIRMATION"
        );
        std::fs::write(root.path().join("RULES.md"), "# newest live\n").unwrap();
        assert_eq!(host.apply_restore(&id, true).unwrap()["state"], "applied");
        assert!(host.snapshot().unwrap()["account"].is_null());
        assert!(host.restore_status(&id).is_err());
        assert_eq!(
            host.authenticate("login", "Admin", "fixture-password")
                .unwrap()["account"]["id"],
            id
        );
        assert_eq!(host.restore_status(&id).unwrap()["state"], "applied");
        // Terminal result still works after input removal. Retry never reapplies.
        swarm_platform::BackupStore::open(&archives)
            .unwrap()
            .remove("selected")
            .unwrap();
        assert_eq!(host.restore_status(&id).unwrap()["state"], "applied");
        assert_eq!(host.apply_restore(&id, true).unwrap()["state"], "applied");
    }));
    // Recovery in the assertion cleanup also keeps a failed test bounded.
    if host.snapshot().ok().is_none_or(|s| s["account"].is_null()) {
        let _ = host.authenticate("login", "Admin", "fixture-password");
    }
    host.stop_runtime().unwrap();
    server.join().unwrap();
    if let Err(error) = checks {
        std::panic::resume_unwind(error);
    }
}
