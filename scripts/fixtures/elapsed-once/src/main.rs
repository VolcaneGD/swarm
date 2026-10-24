use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use swarm_gui_host::GuiHost;

const BASE: &str = "E:/Swarm-Validation-Host/run-20261019-elapsed-approved-once";
const WORK: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm";
const RUNTIME_SHA: &str = "0cf3ecb753c20830be7edf2c3630b3cc07aac90655d963b1097374f8b2b24210";
type Operation = Box<dyn FnOnce(&mut GuiHost) -> Result<Value, String> + Send>;
type Message = (Operation, mpsc::Sender<Result<Value, String>>);

fn exclusive(path: &std::path::Path, value: &Value) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "exclusive receipt unavailable")?;
    file.write_all(serde_json::to_string_pretty(value).unwrap().as_bytes())
        .map_err(|_| "receipt write failed".to_owned())
}
fn invoke(
    tx: &mpsc::Sender<Message>,
    deadline: Instant,
    op: &'static str,
    args: Value,
) -> Result<Value, String> {
    let (reply, result) = mpsc::channel();
    tx.send((Box::new(move |host| {
        let value = match op {
            "setup" => host.authenticate("setup", "ElapsedApprovedFixture", ""),
            "backup.disable" => host.backup_control(args["account"].as_str().unwrap(), "settings.set",
                json!({"enabled":false,"interval_seconds":86400,"retention":2,"destination":null})),
            "create" => {
                let value = host.create(args["operation"].as_str().unwrap(), args["args"].clone())
                    .map_err(|fault| format!("create: {:?}", fault.code))?;
                host.acknowledge_create().map_err(|fault| format!("acknowledge: {:?}", fault.code))?;
                return Ok(value);
            }
            "edit" => host.edit(args),
            "manage" => host.manage(args["operation"].as_str().unwrap(), args["args"].clone()),
            "retention" => host.retention_control(args["account"].as_str().unwrap(), "retention.settings.set", args["settings"].clone()),
            "read" => host.read(args["operation"].as_str().unwrap(), args["args"].clone()),
            _ => return Err("unsupported test operation".to_owned()),
        };
        value.map_err(|fault| format!("{op}: {:?}", fault.code))
    }), reply)).map_err(|_| "worker disconnected")?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    result
        .recv_timeout(remaining.min(Duration::from_secs(20)))
        .map_err(|_| format!("{op}: IPC bound expired; do not retry uncertain request"))?
}
fn metadata(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter_map(|(key, value)| {
                    if matches!(
                        key.as_str(),
                        "token" | "password" | "recovery_code" | "resume_secret" | "session_token"
                    ) {
                        None
                    } else if key == "body" {
                        Some((
                            "body_sha256".to_owned(),
                            json!(format!(
                                "{:x}",
                                Sha256::digest(value.as_str().unwrap_or("").as_bytes())
                            )),
                        ))
                    } else {
                        Some((key.clone(), metadata(value)))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(metadata).collect()),
        _ => value.clone(),
    }
}
fn main() -> Result<(), String> {
    let extension_two = match std::env::args().nth(2).as_deref() {
        None => false,
        Some("--extension-2") => true,
        _ => return Err("unsupported approval extension".to_owned()),
    };
    let (base_path, approval_name, answer, extension) = if extension_two {
        (
            "E:/Swarm-Validation-Host/run-20261019-elapsed-approved-extension-2",
            "target/elapsed-approval-20261019-extension-2.json",
            "保持期間の準備だけ、新たに追加1回を許可する",
            2,
        )
    } else {
        (
            BASE,
            "target/elapsed-approval-20261019.json",
            "保持期間の準備を追加1回だけ許可",
            1,
        )
    };
    let base = PathBuf::from(base_path);
    let root = base.join("Data");
    let runtime = PathBuf::from(WORK)
        .join("target/distribution/candidate-20261019-account-feedback/swarm-runtime.exe");
    let approval_path = PathBuf::from(WORK).join(approval_name);
    let approval: Value =
        serde_json::from_slice(&fs::read(approval_path).map_err(|_| "approval receipt required")?)
            .map_err(|_| "approval receipt invalid")?;
    if PathBuf::from(approval["fixed_root"].as_str().unwrap_or("")) != root
        || approval["extra_attempts"] != 1
        || approval["user_answer"] != answer
        || approval["incident"] != "elapsed-seed-20261016"
        || approval["automatic_retries"] != 0
        || approval["preparation_seconds"] != 180
        || approval["ipc_seconds"] != 20
        || approval["normal_stop_seconds"] != 15
        || approval["vm_permission"] != false
        || approval["publication_permission"] != false
        || approval["elapsed_acceptance"] != false
    {
        return Err("approval scope mismatch".to_owned());
    }
    let runtime_sha = format!(
        "{:x}",
        Sha256::digest(fs::read(&runtime).map_err(|_| "runtime unreadable")?)
    );
    if runtime_sha != RUNTIME_SHA {
        return Err("candidate hash mismatch".to_owned());
    }
    if base.exists() {
        return Err("fixed run already exists; additional retry prohibited".to_owned());
    }
    if std::env::args().nth(1).as_deref() == Some("--preflight") {
        println!("preflight: approval scope/hash/fresh root verified; no product process launched");
        return Ok(());
    }
    if std::env::args().nth(1).as_deref() != Some("--execute-approved-once") {
        return Err("explicit approved execution flag required".to_owned());
    }
    fs::create_dir(&base).map_err(|_| "exclusive run directory unavailable")?;
    exclusive(
        &base.join("attempt.json"),
        &json!({
            "incident":"elapsed-seed-20261016", "extension_attempt":extension,"automatic_retries":0,
            "user_answer":answer,"runtime_sha256":RUNTIME_SHA
        }),
    )?;
    let deadline = Instant::now() + Duration::from_secs(180);
    // The platform canonicalizes the Data root before opening its lifecycle store.
    // Create that exact directory before the offline preference API, never a substitute root.
    fs::create_dir(&root).map_err(|_| "exclusive Data directory unavailable")?;
    swarm_runtime::startup_preference_set(&root, false)
        .map_err(|fault| format!("offline startup preference: {:?}", fault.code))?;
    let mut child = Command::new(&runtime)
        .args(["--data-dir", root.to_str().unwrap(), "--human-start"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "owned runtime start failed")?;
    exclusive(
        &base.join("owner.json"),
        &json!({"runtime_pid":child.id(),"root":root,"exe":runtime}),
    )?;
    let (tx, rx) = mpsc::channel::<Message>();
    let (finished_tx, finished) = mpsc::channel();
    let worker_root = root.clone();
    std::thread::spawn(move || {
        let outcome = (|| -> Result<bool, String> {
            let ready_by = Instant::now() + Duration::from_secs(20);
            loop {
                if swarm_runtime::gui::GuiClient::open(&worker_root)
                    .and_then(|client| client.setup_status())
                    .is_ok()
                {
                    break;
                }
                if Instant::now() >= ready_by {
                    return Err("actual IPC readiness deadline".to_owned());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            let mut host = GuiHost::open(&worker_root).map_err(|_| "GUI host open failed")?;
            for (operation, reply) in rx {
                let result = operation(&mut host);
                let _ = reply.send(result);
            }
            host.stop_runtime()
                .map_err(|fault| format!("authenticated stop: {:?}", fault.code))?;
            Ok(true)
        })();
        let _ = finished_tx.send(outcome);
    });
    let outcome = (|| -> Result<Value, String> {
        let account = invoke(&tx, deadline, "setup", json!({}))?["account"]["id"].clone();
        invoke(&tx, deadline, "backup.disable", json!({"account":account}))?;
        let create = |operation: &'static str, args: Value| {
            invoke(
                &tx,
                deadline,
                "create",
                json!({"operation":operation,"args":args}),
            )
        };
        let manage = |operation: &'static str, args: Value| {
            invoke(
                &tx,
                deadline,
                "manage",
                json!({"operation":operation,"args":args}),
            )
        };
        let read = |operation: &'static str, args: Value| {
            invoke(
                &tx,
                deadline,
                "read",
                json!({"operation":operation,"args":args}),
            )
        };
        let board = create("board.create", json!({"name":"Elapsed live Board"}))?;
        let thread = create(
            "thread.create",
            json!({"board_id":board["id"],"name":"Elapsed live Thread"}),
        )?;
        let live = create(
            "post.create",
            json!({"thread_id":thread["id"],"body":"ELAPSED_LIVE_CONTROL"}),
        )?;
        let post = create(
            "post.create",
            json!({"thread_id":thread["id"],"body":"ELAPSED_DELETED_BODY"}),
        )?;
        invoke(
            &tx,
            deadline,
            "edit",
            json!({"thread_id":thread["id"],"no":post["no"],"if_revision":1,"body":"ELAPSED_EDITED_DELETED_BODY"}),
        )?;
        manage(
            "post.delete",
            json!({"thread_id":thread["id"],"no":post["no"]}),
        )?;
        let deleted_thread = create(
            "thread.create",
            json!({"board_id":board["id"],"name":"Elapsed deleted Thread"}),
        )?;
        let thread_post = create(
            "post.create",
            json!({"thread_id":deleted_thread["id"],"body":"ELAPSED_THREAD_BODY"}),
        )?;
        manage("thread.delete", json!({"id":deleted_thread["id"]}))?;
        let deleted_board = create("board.create", json!({"name":"Elapsed deleted Board"}))?;
        let child_thread = create(
            "thread.create",
            json!({"board_id":deleted_board["id"],"name":"Elapsed child"}),
        )?;
        let board_post = create(
            "post.create",
            json!({"thread_id":child_thread["id"],"body":"ELAPSED_BOARD_BODY"}),
        )?;
        manage("board.delete", json!({"id":deleted_board["id"]}))?;
        let settings = invoke(
            &tx,
            deadline,
            "retention",
            json!({"account":account,"settings":{
                "history_days":1,"deleted_days":1,"deleted_board_days":1,"deleted_thread_days":1,"archived_thread_days":null
            }}),
        )?;
        let deleted = read("conversation.deleted", json!({}))?;
        if deleted["boards"].as_array().map(Vec::len) != Some(1)
            || deleted["threads"].as_array().map(Vec::len) != Some(1)
        {
            return Err("deleted inventory mismatch".to_owned());
        }
        let current = read(
            "post.get",
            json!({"thread_id":thread["id"],"no":live["no"]}),
        )?;
        if current["body"] != live["body"] {
            return Err("live control changed".to_owned());
        }
        let skeleton = read(
            "post.get",
            json!({"thread_id":thread["id"],"no":post["no"]}),
        )?;
        if !skeleton["deleted_at"].is_string() || skeleton["restore_available"] != true {
            return Err("deleted Post not restorable before cutoff".to_owned());
        }
        let history = read(
            "post.history",
            json!({"thread_id":thread["id"],"no":post["no"]}),
        )?;
        if history["items"].as_array().is_none_or(Vec::is_empty) {
            return Err("history baseline absent".to_owned());
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Ok(metadata(
            &json!({"root":root,"synthetic_only":true,"timestamp_source":"actual product clock",
            "baseline_unix_seconds":now,"observe_not_before_unix_seconds":now+86400,
            "settings":settings,"account_id":account,"live_board":board,"live_thread":thread,
            "live_control":current,"deleted_post":skeleton,"deleted_post_history_count":history["items"].as_array().unwrap().len(),
            "deleted_inventory":deleted,"deleted_thread":deleted_thread,"thread_post":thread_post,
            "deleted_board":deleted_board,"child_thread":child_thread,"board_post":board_post,
            "elapsed_day_accepted":false,"native_gui_observed":false,"archived_thread_case_prepared":false,"background_watcher":false}),
        ))
    })();
    drop(tx);
    let until = Instant::now() + Duration::from_secs(15);
    let stop = finished
        .recv_timeout(Duration::from_secs(15))
        .map_err(|_| "worker stop unconfirmed".to_owned());
    let stop_accepted = matches!(stop, Ok(Ok(true)));
    while child
        .try_wait()
        .map_err(|_| "runtime status unavailable")?
        .is_none()
        && Instant::now() < until
    {
        std::thread::sleep(Duration::from_millis(100));
    }
    let graceful = child
        .try_wait()
        .map_err(|_| "runtime status unavailable")?
        .is_some();
    exclusive(
        &base.join("closeout.json"),
        &json!({"normal_stop_accepted":stop_accepted,"runtime_exited":graceful,
        "runtime_pid":child.id(),"preparation_succeeded":outcome.is_ok(),"retry_allowed":false,"watcher":false,
        "error":outcome.as_ref().err(),"stop_error":format!("{stop:?}")}),
    )?;
    let receipt = outcome?;
    if !stop_accepted || !graceful {
        return Err("normal stop unconfirmed; preserve owned process and state".to_owned());
    }
    exclusive(&base.join("baseline.json"), &receipt)?;
    println!(
        "PASS: real one-day baseline saved; runtime stopped; elapsed acceptance NOT_RUN; no watcher"
    );
    Ok(())
}
