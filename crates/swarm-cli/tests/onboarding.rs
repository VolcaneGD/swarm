#![cfg(windows)]
use serde_json::{Value, json};
use std::{
    path::Path,
    time::{Duration, Instant},
};
use swarm_gui_host::GuiHost;
use swarm_platform::pipe;

fn ready(root: &Path) {
    for name in [
        swarm_runtime::pipe_name(root).unwrap(),
        swarm_runtime::gui_pipe_name(root).unwrap(),
    ] {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(connection) = pipe::connect(&name) {
                drop(connection);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
fn cli(root: &Path, args: &[&str]) -> Value {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args([
            "--data-dir",
            root.to_str().unwrap(),
            "--session",
            "bootstrap-ai",
            "--format",
            "json",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["ok"], true);
    result["data"].clone()
}

#[test]
fn fresh_admin_registers_first_project_before_real_cli_session() {
    let data = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let root = data.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&root, Some(4), Some(64)).unwrap()
    });
    ready(data.path());
    let mut host = GuiHost::open(data.path()).unwrap();
    let snapshot = host.authenticate("setup", "Admin", "").unwrap();
    let admin = snapshot["account"]["id"].as_str().unwrap();
    assert_eq!(
        host.project_control(admin, "project.list", json!({}))
            .unwrap()["items"],
        json!([])
    );
    assert_eq!(
        host.project_control(
            "old-account",
            "project.register",
            json!({"name":"Project", "root":project_root.path()})
        )
        .unwrap_err()
        .code,
        "ACCOUNT_CHANGED"
    );
    assert_eq!(
        host.project_control(
            admin,
            "project.register",
            json!({"name":"Project", "root":project_root.path(), "auth":"renderer-token"})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );
    assert_eq!(
        host.project_control(admin, "project.root", json!({}))
            .unwrap_err()
            .code,
        "INVALID_INPUT"
    );
    assert_eq!(
        host.project_control(
            admin,
            "project.register",
            json!({"name":"Relative", "root":"relative"})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );
    assert_eq!(
        host.project_control(
            admin,
            "project.register",
            json!({"name":"Project", "root":project_root.path().join("missing")})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );
    let project = host
        .project_control(
            admin,
            "project.register",
            json!({"name":"初回プロジェクト", "root":project_root.path()}),
        )
        .unwrap();
    let id = project["id"].as_str().unwrap();
    assert_eq!(
        project["canonical_root"],
        project_root
            .path()
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
    );
    assert!(!project_root.path().join("RULES.md").exists());
    let same = host
        .project_control(
            admin,
            "project.register",
            json!({"name":"Different name", "root":project_root.path()}),
        )
        .unwrap();
    assert_eq!(same["id"], id);
    assert_eq!(same["name"], "初回プロジェクト");
    assert_eq!(
        host.project_control(admin, "project.list", json!({}))
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let session = cli(data.path(), &["identity", "start", "--project-id", id]);
    assert_eq!(session["identity"]["project_id"], id);
    let status = cli(data.path(), &["identity", "status"]);
    assert_eq!(status["project_id"], id);
    cli(data.path(), &["identity", "release"]);
    host.manage_account("account.create", json!({"name":"Member", "password":""}))
        .unwrap();
    host.logout().unwrap();
    let member = host.authenticate("login", "Member", "").unwrap();
    let member_id = member["account"]["id"].as_str().unwrap();
    assert_eq!(
        host.project_control(member_id, "project.list", json!({}))
            .unwrap_err()
            .code,
        "PERMISSION_DENIED"
    );
    assert_eq!(
        host.project_control(
            member_id,
            "project.register",
            json!({"name":"Denied", "root":project_root.path()})
        )
        .unwrap_err()
        .code,
        "PERMISSION_DENIED"
    );
    for _ in 0..64 {
        if host
            .read("rules.capabilities", json!({}))
            .is_err_and(|error| error.exit_code == 8)
        {
            break;
        }
    }
    drop(host);
    server.join().unwrap();
}
