#![cfg(windows)]
use serde_json::Value;
use std::process::Command;
#[test]
fn actual_cli_records_redacted_report_without_login_or_launching_runtime() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("synthetic.json");
    let mut report: Value =
        serde_json::from_str(include_str!("../../../docs/BUG_REPORT_TEMPLATE.json")).unwrap();
    report["app_version"] = "9.9.9".into();
    std::fs::write(&file, serde_json::to_vec(&report).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_swarm"))
        .args([
            "--data-dir",
            root.path().to_str().unwrap(),
            "--format",
            "json",
            "bug-report",
            "submit",
            "--report-file",
            file.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["data"]["delivery"], "queued");
    let stored = swarm_platform::CredentialStore::open(&root.path().join("bug-reports"))
        .unwrap()
        .get(&format!("report-{}", report["report_id"].as_str().unwrap()))
        .unwrap()
        .unwrap();
    let stored: Value = serde_json::from_slice(&stored).unwrap();
    assert_eq!(stored["app_version"], env!("CARGO_PKG_VERSION"));
    assert!(!root.path().join("swarm.db").exists());
    assert_eq!(
        swarm_runtime::lifecycle::status(root.path()).unwrap()["state"],
        "not_running"
    );
}
