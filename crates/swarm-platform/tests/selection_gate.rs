#![cfg(windows)]

use std::os::windows::process::CommandExt;
use std::{sync::mpsc, time::Duration};
use swarm_platform::CredentialStore;

#[test]
#[ignore = "subprocess fixture exercised by credential_lock_serializes_processes"]
fn selection_gate_child() {
    let root =
        std::path::PathBuf::from(std::env::var_os("SWARM_SELECTION_GATE_TEST_ROOT").unwrap());
    let store = CredentialStore::open(&root).unwrap();
    std::fs::write(root.join("child-ready"), b"ready").unwrap();
    let _gate = store.lock("notification-selection-gate").unwrap();
    std::fs::write(root.join("child-entered"), b"entered").unwrap();
}

#[test]
fn credential_lock_serializes_processes() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("gui-state");
    let store = CredentialStore::open(&path).unwrap();
    let gate = store.lock("notification-selection-gate").unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "selection_gate_child",
            "--ignored",
            "--nocapture",
        ])
        .env("SWARM_SELECTION_GATE_TEST_ROOT", &path)
        .creation_flags(0x0800_0000)
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !path.join("child-ready").exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let ready = path.join("child-ready").exists();
    std::thread::sleep(Duration::from_millis(100));
    let entered_early = path.join("child-entered").exists();
    drop(gate);
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            break None;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(ready);
    assert!(!entered_early, "child process bypassed selection gate");
    assert!(status.unwrap().success());
    assert!(path.join("child-entered").exists());
}

#[test]
fn credential_store_lock_serializes_independent_handles_until_release() {
    let root = tempfile::tempdir().unwrap();
    let first_store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    let first_lock = first_store.lock("notification-selection-gate").unwrap();

    let path = root.path().join("gui-state");
    let (tx, rx) = mpsc::channel();
    let waiter = std::thread::spawn(move || {
        let store = CredentialStore::open(&path).unwrap();
        let _lock = store.lock("notification-selection-gate").unwrap();
        tx.send(()).unwrap();
    });

    assert!(
        rx.recv_timeout(Duration::from_millis(100)).is_err(),
        "a second handle entered the protected selection transition"
    );
    drop(first_lock);
    rx.recv_timeout(Duration::from_secs(5))
        .expect("the waiting handle should enter after the first lock is released");
    waiter.join().unwrap();
}
