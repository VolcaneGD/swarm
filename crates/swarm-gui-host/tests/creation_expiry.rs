#![cfg(windows)]

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    thread::JoinHandle,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use swarm_core::{Service, protocol::Request};
use swarm_gui_host::GuiHost;
use swarm_platform::CredentialStore;

const CHUNK_BYTES: usize = 60 * 1024;
const MAX_AGE_SECONDS: u64 = 7 * 24 * 60 * 60;

fn req(operation: &str, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: None,
        args,
        context: json!({}),
        request_id: None,
    }
}

fn start_runtime(
    root: &Path,
    gui_connections: usize,
) -> JoinHandle<Result<(), swarm_core::protocol::Fault>> {
    let path = root.to_owned();
    std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, Some(1), Some(gui_connections)))
}

fn ready(root: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    for name in [
        swarm_runtime::pipe_name(root).unwrap(),
        swarm_runtime::gui_pipe_name(root).unwrap(),
    ] {
        loop {
            if let Ok(pipe) = swarm_platform::pipe::connect(&name) {
                drop(pipe);
                break;
            }
            assert!(Instant::now() < deadline, "runtime endpoint did not start");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn authenticated_host_with_stopped_runtime(root: &Path) -> GuiHost {
    Service::open(root)
        .unwrap()
        .request(req(
            "account.setup",
            json!({"name":"Admin","password":"fixture-password"}),
        ))
        .unwrap();
    let server = start_runtime(root, 4); // readiness probe, login, and post-login snapshot
    ready(root);
    let mut host = GuiHost::open(root).unwrap();
    host.authenticate("login", "Admin", "fixture-password")
        .unwrap();
    server.join().unwrap().unwrap();
    host
}

// The host intentionally exposes no state path, so each test passes the gui-state store directly.
fn save_created_request(root: &Path, host: &mut GuiHost, name: &str) -> String {
    assert_eq!(
        host.create("board.create", json!({"name":name}))
            .unwrap_err()
            .exit_code,
        8
    );
    let status = host.creation_status().unwrap();
    assert_eq!(status["pending"], true);
    assert_eq!(status["operation"], "board.create");
    assert_eq!(status["expired"], false);
    pending_value(root)["request_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn pending_manifest(root: &Path) -> Value {
    let store = CredentialStore::open(&root.join("gui-state")).unwrap();
    serde_json::from_slice(&store.get("pending-create").unwrap().unwrap()).unwrap()
}

fn pending_value(root: &Path) -> Value {
    let store = CredentialStore::open(&root.join("gui-state")).unwrap();
    let manifest = pending_manifest(root);
    let group = manifest["group"].as_str().unwrap();
    let mut bytes = Vec::new();
    for index in 0..manifest["chunks"].as_u64().unwrap() as usize {
        let key = format!("pending-create-{group}-{index:03}");
        bytes.extend(store.get(&key).unwrap().unwrap());
    }
    assert_eq!(bytes.len(), manifest["bytes"].as_u64().unwrap() as usize);
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        manifest["sha256"].as_str().unwrap()
    );
    serde_json::from_slice(&bytes).unwrap()
}

fn rewrite_created_at(root: &Path, timestamp: Value) {
    let store = CredentialStore::open(&root.join("gui-state")).unwrap();
    let mut manifest = pending_manifest(root);
    let group = manifest["group"].as_str().unwrap().to_owned();
    let mut pending = pending_value(root);
    pending["created_at"] = timestamp;
    let bytes = serde_json::to_vec(&pending).unwrap();
    let chunks = bytes.chunks(CHUNK_BYTES).collect::<Vec<_>>();
    for (index, chunk) in chunks.iter().enumerate() {
        store
            .put(&format!("pending-create-{group}-{index:03}"), chunk)
            .unwrap();
    }
    manifest["chunks"] = json!(chunks.len());
    manifest["bytes"] = json!(bytes.len());
    manifest["sha256"] = json!(format!("{:x}", Sha256::digest(&bytes)));
    store
        .put("pending-create", &serde_json::to_vec(&manifest).unwrap())
        .unwrap();
}

fn assert_expired_retry_is_retained(root: &Path, host: &mut GuiHost) {
    let request_id = pending_value(root)["request_id"].clone();
    assert_eq!(host.creation_status().unwrap()["expired"], true);
    assert_eq!(host.retry_create().unwrap_err().exit_code, 6);
    assert_eq!(pending_value(root)["request_id"], request_id);
    assert_eq!(host.creation_status().unwrap()["pending"], true);
    assert_eq!(host.creation_status().unwrap()["expired"], true);
    assert_eq!(host.discard_create().unwrap()["pending"], false);
    assert!(
        CredentialStore::open(&root.join("gui-state"))
            .unwrap()
            .get("pending-create")
            .unwrap()
            .is_none()
    );
}

#[test]
fn old_future_and_invalid_timestamps_are_never_replayed() {
    let root = tempfile::tempdir().unwrap();
    let mut host = authenticated_host_with_stopped_runtime(root.path());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    save_created_request(root.path(), &mut host, "old request");
    rewrite_created_at(root.path(), json!(now.saturating_sub(MAX_AGE_SECONDS)));
    assert_expired_retry_is_retained(root.path(), &mut host);

    save_created_request(root.path(), &mut host, "future request");
    rewrite_created_at(root.path(), json!(now + 60));
    assert_expired_retry_is_retained(root.path(), &mut host);

    save_created_request(root.path(), &mut host, "invalid timestamp request");
    rewrite_created_at(root.path(), json!("invalid"));
    assert_expired_retry_is_retained(root.path(), &mut host);
}

#[test]
fn corrupted_chunk_fails_closed_and_keeps_the_manifest() {
    let root = tempfile::tempdir().unwrap();
    let mut host = authenticated_host_with_stopped_runtime(root.path());
    save_created_request(root.path(), &mut host, "chunk damage");
    let manifest = pending_manifest(root.path());
    let group = manifest["group"].as_str().unwrap();
    let key = format!("pending-create-{group}-000");
    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    let manifest_bytes = store.get("pending-create").unwrap().unwrap();
    let mut chunk = store.get(&key).unwrap().unwrap();
    chunk[0] ^= 0x01;
    store.put(&key, &chunk).unwrap();

    assert_eq!(host.retry_create().unwrap_err().exit_code, 8);
    assert_eq!(host.creation_status().unwrap_err().exit_code, 8);
    assert_eq!(
        store.get("pending-create").unwrap().unwrap(),
        manifest_bytes
    );
}

#[test]
fn discard_removes_only_local_pending_state_and_does_not_create_a_board() {
    let root = tempfile::tempdir().unwrap();
    let mut host = authenticated_host_with_stopped_runtime(root.path());
    save_created_request(root.path(), &mut host, "discarded board");
    let manifest = pending_manifest(root.path());
    let group = manifest["group"].as_str().unwrap().to_owned();
    assert_eq!(host.discard_create().unwrap()["pending"], false);

    let store = CredentialStore::open(&root.path().join("gui-state")).unwrap();
    assert!(store.get("pending-create").unwrap().is_none());
    for index in 0..manifest["chunks"].as_u64().unwrap() as usize {
        assert!(
            store
                .get(&format!("pending-create-{group}-{index:03}"))
                .unwrap()
                .is_none()
        );
    }

    let server = start_runtime(root.path(), 2); // readiness probe plus board read
    ready(root.path());
    assert!(
        host.read("board.list", json!({})).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    server.join().unwrap().unwrap();
}
