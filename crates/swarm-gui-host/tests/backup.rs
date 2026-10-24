#![cfg(windows)]
use serde_json::json;
use std::time::{Duration, Instant};
use swarm_gui_host::GuiHost;

#[test]
fn unacknowledged_manual_result_survives_host_reopen() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&path, Some(1), Some(48)).unwrap()
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
    let account = host
        .authenticate("setup", "Admin", "fixture-password")
        .unwrap();
    let id = account["account"]["id"].as_str().unwrap();
    let result = host.backup_control(id, "manual.create", json!({})).unwrap();
    drop(host);
    let mut host = GuiHost::open(root.path()).unwrap();
    let checks = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_eq!(
            host.backup_control(id, "status", json!({})).unwrap()["manual"],
            result
        );
        assert_eq!(
            host.backup_control(id, "manual.create", json!({})).unwrap(),
            result
        );
        host.backup_control(id, "manual.ack", json!({"intent":result["intent"]}))
            .unwrap();
        assert!(host.backup_control(id, "status", json!({})).unwrap()["manual"].is_null());
    }));
    host.stop_runtime().unwrap();
    server.join().unwrap();
    if let Err(e) = checks {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn selected_admin_settings_roundtrip_over_real_private_pipe() {
    let root = tempfile::tempdir().unwrap();
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
    let account = host
        .authenticate("setup", "Admin", "fixture-password")
        .unwrap();
    let id = account["account"]["id"].as_str().unwrap();
    let result = || {
        let status = host.backup_control(id, "status", json!({})).unwrap();
        assert_eq!(
            host.restore_picker_directory(id).unwrap(),
            root.path().canonicalize().unwrap().join("backups")
        );
        assert_eq!(
            host.restore_picker_directory("another-account")
                .unwrap_err()
                .code,
            "ACCOUNT_CHANGED"
        );
        assert_eq!(status["settings"]["interval_seconds"], 86400);
        assert_eq!(
            host.backup_control("another-account", "status", json!({}))
                .unwrap_err()
                .code,
            "ACCOUNT_CHANGED"
        );
        let settings =
            json!({"enabled":false,"interval_seconds":3600,"retention":4,"destination":null});
        assert_eq!(
            host.backup_control(id, "settings.set", settings.clone())
                .unwrap(),
            settings
        );
        assert_eq!(
            host.backup_control(id, "status", json!({})).unwrap()["settings"],
            settings
        );
        let custom = root.path().join("custom-backup-folder");
        std::fs::create_dir(&custom).unwrap();
        let custom = custom.canonicalize().unwrap();
        let mut configured = settings.clone();
        configured["destination"] = json!(custom);
        host.backup_control(id, "settings.set", configured).unwrap();
        assert_eq!(host.restore_picker_directory(id).unwrap(), custom);
        host.backup_control(id, "settings.set", settings.clone())
            .unwrap();
        assert_eq!(
            host.backup_control(id, "remove", json!({"path":"unrelated"}))
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
        let first = host.backup_control(id, "manual.create", json!({})).unwrap();
        assert_eq!(first["state"], "completed");
        assert_eq!(
            host.backup_control(id, "manual.create", json!({})).unwrap(),
            first
        );
        assert_eq!(
            host.backup_control(id, "status", json!({})).unwrap()["manual"],
            first
        );
        assert!(
            host.backup_control(
                id,
                "manual.ack",
                json!({"intent":uuid::Uuid::new_v4().to_string()})
            )
            .is_err()
        );
        host.backup_control(id, "manual.ack", json!({"intent":first["intent"]}))
            .unwrap();
        let next = host.backup_control(id, "manual.create", json!({})).unwrap();
        assert_ne!(next["key"], first["key"]);
    };
    // Always stop the isolated owner, including after an assertion failure.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(result));
    host.stop_runtime().unwrap();
    server.join().unwrap();
    if let Err(error) = outcome {
        std::panic::resume_unwind(error);
    }
}
