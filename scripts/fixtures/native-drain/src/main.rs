use serde_json::{Value, json};
use std::os::windows::fs::OpenOptionsExt;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    time::{Duration, Instant},
};
use swarm_core::protocol::Request;
use swarm_platform::{CredentialStore, pipe};
use swarm_runtime::gui::GuiClient;

const ROOT: &str = "E:/Swarm-Validation-Host/run-20261018-h1-auth/Data";
const OUT: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/native-drain-20261019";

fn request(operation: &str, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: None,
        args,
        context: json!({}),
        request_id: None,
    }
}
fn save(name: &str, data: Value) -> Result<(), String> {
    save_at(Path::new(OUT), name, data)
}
fn save_at(out: &Path, name: &str, data: Value) -> Result<(), String> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out.join(name))
        .map_err(|_| "exclusive receipt refused")?;
    f.write_all(serde_json::to_string_pretty(&data).unwrap().as_bytes())
        .map_err(|_| "receipt write failed".into())
}
fn selection(root: &Path) -> Result<String, String> {
    let state = CredentialStore::open(&root.join("gui-state")).map_err(|_| "state unavailable")?;
    for key in ["pending-auth", "pending-create"] {
        if state
            .get(key)
            .map_err(|_| "pending check failed")?
            .is_some()
        {
            return Err("pending operation; no exercise".into());
        }
    }
    let bytes = state
        .get("selection")
        .map_err(|_| "selection unavailable")?
        .ok_or("selection absent")?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "selection invalid")?;
    value["alias"]
        .as_str()
        .map(str::to_owned)
        .ok_or("alias absent".into())
}
fn verify_original(client: &GuiClient, alias: &str, thread: &Value) -> Result<(), String> {
    let account = client
        .request(alias, request("account.status", json!({})))
        .map_err(|_| "account read failed")?;
    if account["name"] != "H1_TEST_ADMIN" || account["is_admin"] != true {
        return Err("wrong selected identity".into());
    }
    let original = client
        .request(
            alias,
            request("post.get", json!({"thread_id":thread,"no":1})),
        )
        .map_err(|_| "original read failed")?;
    if original["body"] != "H1_PERMISSION_PRIVATE_BODY" || original["revision"] != 1 {
        return Err("original fixture changed".into());
    }
    Ok(())
}
fn main() -> Result<(), String> {
    let mode = std::env::args().nth(1).ok_or("mode required")?;
    if [
        "--prepare-extension-1",
        "--execute-extension-1",
        "--verify-extension-1",
    ]
    .contains(&mode.as_str())
    {
        return extension_one(&mode);
    }
    if !["--preflight", "--execute-once", "--verify-resumed"].contains(&mode.as_str()) {
        return Err("unsupported mode".into());
    }
    let root = Path::new(ROOT);
    if root
        .canonicalize()
        .map_err(|_| "root absent")?
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
        != ROOT
    {
        return Err("root mismatch".into());
    }
    let alias = selection(root)?;
    let staged: Value = serde_json::from_slice(
        &fs::read(root.parent().unwrap().join("thread-stage.json")).map_err(|_| "stage absent")?,
    )
    .map_err(|_| "stage invalid")?;
    let thread = staged["thread_id"].clone();
    let client = GuiClient::open(root).map_err(|_| "existing GUI bridge unavailable")?;
    verify_original(&client, &alias, &thread)?;
    if mode == "--verify-resumed" {
        let result: Value = serde_json::from_slice(
            &fs::read(Path::new(OUT).join("result.json")).map_err(|_| "result absent")?,
        )
        .map_err(|_| "result invalid")?;
        let post = client
            .request(
                &alias,
                request("post.get", json!({"thread_id":thread,"no":result["no"]})),
            )
            .map_err(|_| "post readback failed")?;
        if post["id"] != result["post_id"]
            || post["body"].as_str().map(str::len) != Some(100_000)
            || post["revision"] != 1
        {
            return Err("post persistence mismatch".into());
        }
        save(
            "resumed.json",
            json!({"original_post_unchanged":true,"created_post_persisted":true,"same_admin":true,"full_stop_cleared_by_native_start":swarm_runtime::lifecycle::check_running_allowed(root).is_ok()}),
        )?;
        println!("PASS: native resume readback; original and new synthetic Post retained");
        return Ok(());
    }
    if Path::new(OUT).exists() {
        return Err("attempt already exists; no replay".into());
    }
    if mode == "--preflight" {
        println!(
            "preflight: exact existing H1/Admin/original Post/pending state verified; no mutation"
        );
        return Ok(());
    }
    fs::create_dir(OUT).map_err(|_| "exclusive attempt directory refused")?;
    save(
        "attempt.json",
        json!({"root":ROOT,"attempts":1,"retries":0,"body_bytes":100000,"hold_seconds":4,"native_full_stop_required":true}),
    )?;
    let mut inner = request(
        "post.create",
        json!({"thread_id":thread,"body":"D".repeat(100_000)}),
    );
    inner.request_id = Some("native-drain-20261019-once".into());
    exercise(root, &alias, inner, Path::new(OUT))
}

fn exercise(root: &Path, alias: &str, inner: Request, out: &Path) -> Result<(), String> {
    let capability = CredentialStore::open(&root.join("gui-host"))
        .map_err(|_| "bridge unavailable")?
        .get("transport-capability")
        .map_err(|_| "bridge unavailable")?
        .ok_or("bridge absent")?;
    let mut outer = request("gui.request", json!({"alias":alias,"request":inner}));
    outer.auth = Some(String::from_utf8(capability).map_err(|_| "bridge invalid")?);
    let mut conn =
        pipe::connect(&swarm_runtime::gui_pipe_name(root).map_err(|_| "endpoint unavailable")?)
            .map_err(|_| "connect failed")?;
    swarm_ipc::write_request(&mut conn, &outer).map_err(|_| "request outcome unknown; no retry")?;
    let mut header = [0u8; 4];
    conn.read_exact(&mut header)
        .map_err(|_| "response header unavailable; no retry")?;
    let bytes = u32::from_le_bytes(header) & 0x7fff_ffff;
    if bytes <= 65536 {
        return Err("response too small for backpressure; do not claim drain".into());
    }
    save_at(
        out,
        "ready.json",
        json!({"response_frame_bytes":bytes,"response_header_observed":true,"response_body_unread":true}),
    )?;
    let begin = Instant::now();
    let mut stopped = false;
    while begin.elapsed() < Duration::from_secs(4) {
        match swarm_runtime::lifecycle::check_running_allowed(root) {
            Err(f) if f.code == "RUNTIME_STOPPED" => {
                stopped = true;
                break;
            }
            Err(_) => break,
            Ok(()) => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    let lease_held = matches!(OpenOptions::new().read(true).write(true).share_mode(0)
        .open(root.join("lifecycle/runtime.lock")), Err(ref e) if e.raw_os_error() == Some(32));
    let unread_hold_ms = begin.elapsed().as_millis();
    // Always drain before returning, including missed UI timing. No process kill.
    let mut joined = std::io::Cursor::new(header).chain(conn);
    let response = swarm_ipc::read_response(&mut joined)
        .map_err(|_| "response drain failed; inspect without retry")?;
    if !response.ok {
        return Err("mutation rejected; inspect without retry".into());
    }
    let post = response.data.ok_or("response absent")?;
    if post["body"].as_str().map(str::len) != Some(100_000) {
        return Err("response body mismatch".into());
    }
    let until = Instant::now() + Duration::from_secs(15);
    let lease_released = loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(root.join("lifecycle/runtime.lock"))
        {
            Ok(_) => break true,
            Err(_) if Instant::now() < until => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => break false,
        }
    };
    save_at(
        out,
        "result.json",
        json!({"native_stop_observed_before_read":stopped,"runtime_lease_held_before_drain":lease_held,"lease_released_after_drain":lease_released,"unread_hold_ms":unread_hold_ms,"total_ms":begin.elapsed().as_millis(),"response_frame_bytes":bytes,"response_drained":true,"post_id":post["id"],"no":post["no"],"revision":post["revision"],"transaction_drain_accepted":false,"scope":"committed mutation response drain only"}),
    )?;
    if !stopped || !lease_held || !lease_released {
        return Err("native response-drain boundary not established; no retry".into());
    }
    println!(
        "PASS: committed mutation response drained after native full-stop flag; transaction drain remains separate"
    );
    Ok(())
}

const EXTENSION_ANSWER: &str = "RPC応答drain検査だけ追加1回を許可する";

fn approval_valid(value: &Value) -> bool {
    value["incident"] == "native-drain-20261019"
        && value["user_answer"] == EXTENSION_ANSWER
        && value["fixed_root"] == ROOT
        && value["extension_attempts"] == 1
        && value["retries_after_failure"] == 0
        && value["hold_seconds"] == 4
        && value["vm_permission"] == false
        && value["elapsed_seed_permission"] == false
        && value["publication_permission"] == false
}

fn extension_one(mode: &str) -> Result<(), String> {
    let out = Path::new(OUT).join("extension-1");
    // A proposal is not authorization. Refuse before even opening product stores.
    if mode == "--execute-extension-1" {
        let approval: Value = serde_json::from_slice(
            &fs::read(Path::new(OUT).join("extension-1-approval.json"))
                .map_err(|_| "new explicit approval required; no execution")?,
        )
        .map_err(|_| "approval invalid")?;
        if !approval_valid(&approval) {
            return Err("approval scope mismatch; no execution".into());
        }
    }
    if mode != "--verify-extension-1" && out.exists() {
        return Err("extension consumed; do not retry or change output root".into());
    }
    let prior: Value = serde_json::from_slice(
        &fs::read(Path::new(OUT).join("result.json")).map_err(|_| "original result required")?,
    )
    .map_err(|_| "original result invalid")?;
    if prior["native_stop_observed_before_read"] != false
        || prior["response_drained"] != true
        || prior["no"] != 2
        || prior["revision"] != 1
    {
        return Err("prior failed-boundary receipt mismatch".into());
    }
    let root = Path::new(ROOT);
    if root
        .canonicalize()
        .map_err(|_| "root absent")?
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
        != ROOT
    {
        return Err("root mismatch".into());
    }
    let alias = selection(root)?;
    let staged: Value = serde_json::from_slice(
        &fs::read(root.parent().unwrap().join("thread-stage.json")).map_err(|_| "stage absent")?,
    )
    .map_err(|_| "stage invalid")?;
    let thread = staged["thread_id"].clone();
    let client = GuiClient::open(root).map_err(|_| "GUI bridge unavailable")?;
    verify_original(&client, &alias, &thread)?;
    let post = client
        .request(
            &alias,
            request("post.get", json!({"thread_id":thread,"no":2})),
        )
        .map_err(|_| "prior synthetic Post read failed")?;
    let expected_revision = if mode == "--verify-extension-1" { 2 } else { 1 };
    if post["id"] != prior["post_id"]
        || post["revision"] != expected_revision
        || post["body"] != "D".repeat(100_000)
    {
        return Err("synthetic Post changed; do not execute".into());
    }
    if mode == "--verify-extension-1" {
        save_at(
            &out,
            "resumed.json",
            json!({"same_admin":true,"original_post_unchanged":true,
            "same_synthetic_post_id":true,"synthetic_post_revision":2,
            "native_start_cleared_full_stop":swarm_runtime::lifecycle::check_running_allowed(root).is_ok()}),
        )?;
        println!("PASS: extension resume readback; same synthetic Post revision2");
        return Ok(());
    }
    if mode == "--prepare-extension-1" {
        println!(
            "preparation: exact existing H1/Admin/original and synthetic Posts verified; approval absent is not execution authority"
        );
        return Ok(());
    }
    fs::create_dir(&out).map_err(|_| "exclusive extension receipt refused")?;
    save_at(
        &out,
        "attempt.json",
        json!({"incident":"native-drain-20261019","extension":1,
        "root":ROOT,"extra_attempts":1,"retries":0,"operation":"post.edit","no":2,
        "if_revision":1,"new_rows":0,"hold_seconds":4,"transaction_drain_accepted":false}),
    )?;
    exercise(
        root,
        &alias,
        request(
            "post.edit",
            json!({"thread_id":thread,"no":2,
        "if_revision":1,"body":"D".repeat(100_000)}),
        ),
        &out,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn extension_approval_is_narrow_and_requires_explicit_answer() {
        let valid = json!({"incident":"native-drain-20261019","user_answer":EXTENSION_ANSWER,
            "fixed_root":ROOT,"extension_attempts":1,"retries_after_failure":0,"hold_seconds":4,
            "vm_permission":false,"elapsed_seed_permission":false,"publication_permission":false});
        assert!(approval_valid(&valid));
        for field in [
            "incident",
            "user_answer",
            "fixed_root",
            "extension_attempts",
            "retries_after_failure",
            "hold_seconds",
            "vm_permission",
            "elapsed_seed_permission",
            "publication_permission",
        ] {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(!approval_valid(&missing), "missing required scope {field}");
        }
        let mut broadened = valid.clone();
        broadened["vm_permission"] = json!(true);
        assert!(!approval_valid(&broadened));
        let mut broad_answer = valid;
        broad_answer["user_answer"] = json!("続けてください。");
        assert!(!approval_valid(&broad_answer));
    }
    #[test]
    fn response_larger_than_pipe_quota_remains_unfinished_until_read() {
        let name = format!("Swarm-native-drain-selftest-{}", std::process::id());
        let mut listener = pipe::Listener::bind(&name).unwrap();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let mut conn = listener.accept().unwrap();
            swarm_ipc::write_response(
                &mut conn,
                &swarm_ipc::RpcResponse::from_result(Ok(json!({"body":"D".repeat(100_000)}))),
            )
            .unwrap();
            done_tx.send(()).unwrap();
        });
        let mut conn = pipe::connect(&name).unwrap();
        let mut header = [0; 4];
        conn.read_exact(&mut header).unwrap();
        assert!(
            done_rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "write completed before response read"
        );
        let response =
            swarm_ipc::read_response(&mut std::io::Cursor::new(header).chain(conn)).unwrap();
        assert_eq!(
            response.data.unwrap()["body"].as_str().unwrap().len(),
            100_000
        );
        done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        server.join().unwrap();
    }
}
