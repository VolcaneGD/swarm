//! Pending human role/explicit permission changes through normal frozen-runtime APIs.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use swarm_core::protocol::Request;
use swarm_gui_host::GuiHost;
use swarm_platform::CredentialStore;
use swarm_runtime::gui::GuiClient;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261022-wait-role";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261022-wait-role/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const BODY: &str = "SYNTHETIC_WAIT_POST_CURRENT_20261021";
const RULES: &str = "# Isolated changes wait authority\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: explicit-changes\n    effect: allow\n    actions: [changes.get, changes.wait]\naccess:\n  boards: []\n  threads: []\n~~~\n";
fn ensure(b: bool, m: &str) -> Result<()> {
    if b { Ok(()) } else { Err(m.into()) }
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn save(n: &str, v: &Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(Path::new(BASE).join(n))?;
    writeln!(f, "{v}")?;
    Ok(())
}
fn selected() -> Result<String> {
    let v: Value = serde_json::from_slice(
        &CredentialStore::open(&Path::new(ROOT).join("gui-state"))?
            .get("selection")?
            .ok_or("selection missing")?,
    )?;
    Ok(v["alias"].as_str().ok_or("alias missing")?.into())
}
fn req(op: &str, args: Value) -> Request {
    Request {
        operation: op.into(),
        args,
        auth: None,
        context: json!({}),
        request_id: None,
    }
}
fn created(h: &mut GuiHost, op: &str, args: Value) -> Result<Value> {
    let v = h.create(op, args)?;
    h.acknowledge_create()?;
    Ok(v)
}
struct Runtime(Child, Option<String>);
impl Runtime {
    fn stop(&mut self) -> Result<()> {
        if self.0.try_wait()?.is_some() {
            return Ok(());
        }
        let cleanup_alias = self.1.clone().unwrap_or(selected()?);
        GuiClient::open(Path::new(ROOT))?.stop(&cleanup_alias)?;
        let until = Instant::now() + Duration::from_secs(12);
        loop {
            if self.0.try_wait()?.is_some() {
                return Ok(());
            }
            ensure(
                Instant::now() < until,
                "runtime exit deadline; preserve process",
            )?;
            std::thread::sleep(Duration::from_millis(30));
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.stop();
        }
    }
}

type HumanResult = std::result::Result<Value, swarm_core::protocol::Fault>;
fn human_wait(
    alias: &str,
    t: &str,
    cursor: &str,
) -> (
    std::sync::mpsc::Receiver<HumanResult>,
    std::thread::JoinHandle<()>,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    let (alias, t, cursor) = (alias.to_owned(), t.to_owned(), cursor.to_owned());
    let handle = std::thread::spawn(move || {
        let result = GuiClient::open(Path::new(ROOT)).and_then(|c| {
            c.request(
                &alias,
                req(
                    "changes.wait",
                    json!({"thread_id":t,"after":cursor,"timeout":8}),
                ),
            )
        });
        let _ = tx.send(result);
    });
    (rx, handle)
}
fn human_pending(rx: &std::sync::mpsc::Receiver<HumanResult>, label: &str) -> Result<()> {
    ensure(
        matches!(
            rx.recv_timeout(Duration::from_millis(600)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ),
        "human wait not pending before change",
    )?;
    save(
        &format!("pending-{label}.json"),
        &json!({"running_for_ms_at_least":600,"result_not_yet_available":true,"server_admission_not_instrumented":true}),
    )
}
fn human_collect(
    rx: std::sync::mpsc::Receiver<HumanResult>,
    handle: std::thread::JoinHandle<()>,
    label: &str,
    code: Option<&str>,
    secrets: &[String],
) -> Result<Value> {
    let result = rx.recv_timeout(Duration::from_secs(12))?;
    handle.join().map_err(|_| "human wait thread failed")?;
    let serialized = serde_json::to_string(&result)?;
    ensure(
        !secrets
            .iter()
            .any(|s| !s.is_empty() && serialized.contains(s)),
        "human output secret match; do not print",
    )?;
    save(
        &format!("human-{label}.json"),
        &json!({"ok":result.is_ok(),"code":result.as_ref().err().map(|f|f.code.as_str()),"no_secret_match":true,"raw_saved":false}),
    )?;
    match (result, code) {
        (Ok(v), None) => Ok(v),
        (Err(f), Some(expected)) if f.code == expected => Ok(json!({"code":f.code})),
        _ => Err("human wait contract mismatch; safe receipt retained".into()),
    }
}
fn human_cursor(alias: &str, t: &str) -> Result<String> {
    let v = GuiClient::open(Path::new(ROOT))?
        .request(alias, req("changes.get", json!({"thread_id":t})))?;
    Ok(v["change_cursor"].as_str().ok_or("cursor missing")?.into())
}
fn run() -> Result<()> {
    for (name, expected) in [
        (
            "swarm-runtime.exe",
            "0cf3ecb753c20830be7edf2c3630b3cc07aac90655d963b1097374f8b2b24210",
        ),
        (
            "swarm.exe",
            "d23b7973d229bea186d4aec95eca3a6f7f2868f7243534156372e5e438ecfd33",
        ),
    ] {
        ensure(
            hash(&fs::read(Path::new(CANDIDATE).join(name))?) == expected,
            "candidate hash mismatch",
        )?;
    }
    ensure(!Path::new(BASE).exists(), "existing fixture refused")?;
    fs::create_dir(BASE)?;
    fs::create_dir(ROOT)?;
    swarm_runtime::startup_preference_set(Path::new(ROOT), false)?;
    fs::write(Path::new(ROOT).join("RULES.md"), RULES)?;
    let mut c = Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
    c.args(["--data-dir", ROOT, "--human-start"])
        .stdin(Stdio::null());
    c.stdout(
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(Path::new(BASE).join("runtime.stdout.log"))?,
    )
    .stderr(
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(Path::new(BASE).join("runtime.stderr.log"))?,
    );
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    let mut runtime = Runtime(c.spawn()?, None);
    save(
        "runtime.json",
        &json!({"pid":runtime.0.id(),"frozen_candidate":CANDIDATE}),
    )?;
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if GuiClient::open(Path::new(ROOT))
            .and_then(|c| c.setup_status())
            .is_ok()
        {
            break;
        }
        ensure(Instant::now() < until, "readiness deadline")?;
        std::thread::sleep(Duration::from_millis(40));
    }
    let mut h = GuiHost::open(Path::new(ROOT))?;
    let admin_password = uuid::Uuid::new_v4().simple().to_string();
    let account =
        h.authenticate("setup", "WaitRoleAdmin", &admin_password)?["account"]["id"].clone();
    let admin_alias = selected()?;
    runtime.1 = Some(admin_alias.clone());
    h.backup_control(
        account.as_str().ok_or("admin missing")?,
        "settings.set",
        json!({"enabled":false,"interval_seconds":86400,"retention":2,"destination":null}),
    )?;
    let board = created(&mut h, "board.create", json!({"name":"WAIT_ROLE_BOARD"}))?;
    let thread = created(
        &mut h,
        "thread.create",
        json!({"name":"WAIT_ROLE_THREAD","board_id":board["id"]}),
    )?;
    let t = thread["id"].as_str().ok_or("thread missing")?;
    let post = created(
        &mut h,
        "post.create",
        json!({"thread_id":t,"body":"SYNTHETIC_WAIT_ROLE_ORIGINAL"}),
    )?;
    let mut secrets = vec![
        admin_password,
        String::from_utf8(
            CredentialStore::open(&Path::new(ROOT).join("gui-host"))?
                .get("transport-capability")?
                .ok_or("capability missing")?,
        )?,
    ];

    let password = uuid::Uuid::new_v4().simple().to_string();
    secrets.push(password.clone());
    let recipient = h.manage_account(
        "account.create",
        json!({"name":"WaitRoleRecipient","password":password}),
    )?;
    let role = h.manage_authorization("role.create", json!({"name":"WaitOnlyRole"}))?;
    for operation in ["read", "changes.get", "changes.wait"] {
        h.manage_authorization("permission.set", json!({"subject_type":"role","subject_id":role["id"],"operation":operation,"scope":"global","target_id":"","decision":"allow"}))?;
    }
    let assignment = |assigned| {
        req(
            "role.assign",
            json!({"account_id":recipient["id"],"role_id":role["id"],"assigned":assigned}),
        )
    };
    GuiClient::open(Path::new(ROOT))?.request(&admin_alias, assignment(true))?;
    h.authenticate("login", "WaitRoleRecipient", &password)?;
    let alias = selected()?;
    let cursor = human_cursor(&alias, t)?;
    let (rx, handle) = human_wait(&alias, t, &cursor);
    human_pending(&rx, "positive")?;
    let latest = GuiClient::open(Path::new(ROOT))?.request(
        &admin_alias,
        req(
            "post.edit",
            json!({"thread_id":t,"no":1,"if_revision":1,"body":BODY}),
        ),
    )?;
    let positive = human_collect(rx, handle, "positive", None, &secrets)?;
    let events = positive["items"]
        .as_array()
        .ok_or("positive items missing")?;
    ensure(
        latest["revision"] == 2
            && events.len() == 1
            && events[0]["entity_id"] == post["id"]
            && events[0]["operation"] == "post.edit"
            && !serde_json::to_string(&positive)?.contains(BODY),
        "positive metadata mismatch",
    )?;
    let cursor = human_cursor(&alias, t)?;
    let (rx, handle) = human_wait(&alias, t, &cursor);
    human_pending(&rx, "role-unassign")?;
    GuiClient::open(Path::new(ROOT))?.request(&admin_alias, assignment(false))?;
    human_collect(
        rx,
        handle,
        "role-unassigned-wait",
        Some("RESYNC_REQUIRED"),
        &secrets,
    )?;
    // Default Post read remains allowed; changes requires an explicit role grant.
    // The guarded continuation completes only the as-yet-unexecuted suffix.
    drop(h);
    runtime.stop()?;
    println!("ROLE_WAIT_PHASE_COMPLETE");
    Ok(())
}
fn check_request(
    alias: &str,
    request: Request,
    code: Option<&str>,
    label: &str,
    secrets: &[String],
) -> Result<Value> {
    let result = GuiClient::open(Path::new(ROOT))?.request(alias, request);
    let serialized = serde_json::to_string(&result)?;
    ensure(
        !secrets
            .iter()
            .any(|s| !s.is_empty() && serialized.contains(s)),
        "secret match; raw output suppressed",
    )?;
    save(
        &format!("read-{label}.json"),
        &json!({"ok":result.is_ok(),"code":result.as_ref().err().map(|f|f.code.as_str()),"no_secret_match":true,"raw_saved":false}),
    )?;
    match (result, code) {
        (Ok(v), None) => Ok(v),
        (Err(f), Some(expected)) if f.code == expected => Ok(json!({"code":f.code})),
        _ => Err("read contract mismatch; safe receipt retained".into()),
    }
}
fn continue_after_role() -> Result<()> {
    ensure(
        !Path::new(BASE).join("continuation.json").exists()
            && !Path::new(BASE).join("result.json").exists(),
        "continuation consumed",
    )?;
    let prior: Value = serde_json::from_slice(&fs::read(
        Path::new(BASE).join("human-role-unassigned-wait.json"),
    )?)?;
    ensure(
        prior["code"] == "RESYNC_REQUIRED" && prior["ok"] == false,
        "unexpected prior wait",
    )?;
    ensure(
        swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"] == "stopped",
        "prior runtime not stopped",
    )?;
    ensure(
        hash(&fs::read(Path::new(CANDIDATE).join("swarm-runtime.exe"))?)
            == "0cf3ecb753c20830be7edf2c3630b3cc07aac90655d963b1097374f8b2b24210",
        "runtime changed",
    )?;
    let alias = selected()?;
    let admin_alias = format!("account-{}", hash(b"WaitRoleAdmin"));
    let secrets = vec![String::from_utf8(
        CredentialStore::open(&Path::new(ROOT).join("gui-host"))?
            .get("transport-capability")?
            .ok_or("capability missing")?,
    )?];
    save(
        "continuation.json",
        &json!({"prior_wait_not_repeated":true,"same_data":true,"canary_count":secrets.len(),"original_passwords_not_recovered":true}),
    )?;
    let mut command = Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
    command
        .args(["--data-dir", ROOT, "--human-start"])
        .stdin(Stdio::null())
        .stdout(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join("continuation.stdout.log"))?,
        )
        .stderr(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join("continuation.stderr.log"))?,
        );
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut runtime = Runtime(command.spawn()?, Some(admin_alias.clone()));
    save("continuation-runtime.json", &json!({"pid":runtime.0.id()}))?;
    let until = Instant::now() + Duration::from_secs(15);
    while GuiClient::open(Path::new(ROOT))
        .and_then(|c| c.setup_status())
        .is_err()
    {
        ensure(Instant::now() < until, "readiness deadline")?;
        std::thread::sleep(Duration::from_millis(40));
    }
    let c = GuiClient::open(Path::new(ROOT))?;
    let admin = c.request(&admin_alias, req("account.status", json!({})))?;
    ensure(
        admin["name"] == "WaitRoleAdmin" && admin["is_admin"] == true,
        "Admin mismatch",
    )?;
    let find_one = |v: Value, name: &str| -> Result<Value> {
        let a = v["items"].as_array().ok_or("items missing")?;
        let matches: Vec<_> = a.iter().filter(|v| v["name"] == name).collect();
        ensure(matches.len() == 1, "identity ambiguous")?;
        Ok(matches[0].clone())
    };
    let recipient = find_one(
        c.request(&admin_alias, req("account.list", json!({})))?,
        "WaitRoleRecipient",
    )?;
    let role = find_one(
        c.request(&admin_alias, req("role.list", json!({})))?,
        "WaitOnlyRole",
    )?;
    let board = find_one(
        c.request(&admin_alias, req("board.list", json!({})))?,
        "WAIT_ROLE_BOARD",
    )?;
    let thread = find_one(
        c.request(
            &admin_alias,
            req("thread.list", json!({"board_id":board["id"]})),
        )?,
        "WAIT_ROLE_THREAD",
    )?;
    let t = thread["id"].as_str().ok_or("thread missing")?;
    let posts = c.request(&admin_alias, req("post.list", json!({"thread_id":t})))?;
    let items = posts["items"].as_array().ok_or("posts missing")?;
    ensure(
        items.len() == 1 && items[0]["body"] == BODY && items[0]["revision"] == 2,
        "retained Post changed",
    )?;
    let latest = items[0].clone();
    let assignment = |assigned| {
        req(
            "role.assign",
            json!({"account_id":recipient["id"],"role_id":role["id"],"assigned":assigned}),
        )
    };
    check_request(
        &alias,
        req("changes.get", json!({"thread_id":t})),
        Some("NOT_FOUND"),
        "role-unassigned-get",
        &secrets,
    )?;
    check_request(
        &alias,
        req("post.list", json!({"thread_id":t})),
        None,
        "role-unassigned-posts",
        &secrets,
    )?;
    GuiClient::open(Path::new(ROOT))?.request(&admin_alias, assignment(true))?;
    // Same credential is retained: permission changes do not invalidate identity.
    let _ = human_cursor(&alias, t)?;
    let restored = check_request(
        &alias,
        req("post.list", json!({"thread_id":t})),
        None,
        "role-reassigned-posts",
        &secrets,
    )?;
    let items = restored["items"]
        .as_array()
        .ok_or("restored posts missing")?;
    ensure(
        items.len() == 1
            && items[0]["id"] == latest["id"]
            && items[0]["body"] == BODY
            && items[0]["revision"] == 2,
        "Post not preserved",
    )?;
    // Distinct scoped deny overrides the re-granted role without disabling identity.
    let cursor = human_cursor(&alias, t)?;
    let (rx, handle) = human_wait(&alias, t, &cursor);
    human_pending(&rx, "thread-deny")?;
    let permission = |decision: &str| {
        req(
            "permission.set",
            json!({"subject_type":"account","subject_id":recipient["id"],"operation":"read","scope":"thread","target_id":t,"decision":decision}),
        )
    };
    GuiClient::open(Path::new(ROOT))?.request(&admin_alias, permission("deny"))?;
    human_collect(
        rx,
        handle,
        "thread-denied-wait",
        Some("RESYNC_REQUIRED"),
        &secrets,
    )?;
    check_request(
        &alias,
        req("changes.get", json!({"thread_id":t})),
        Some("NOT_FOUND"),
        "thread-denied-get",
        &secrets,
    )?;
    check_request(
        &alias,
        req("post.list", json!({"thread_id":t})),
        Some("PERMISSION_DENIED"),
        "thread-denied-posts",
        &secrets,
    )?;
    GuiClient::open(Path::new(ROOT))?.request(&admin_alias, permission("inherit"))?;
    let _ = human_cursor(&alias, t)?;
    let recovered = check_request(
        &alias,
        req("post.list", json!({"thread_id":t})),
        None,
        "thread-inherited-posts",
        &secrets,
    )?;
    ensure(
        recovered["items"] == restored["items"],
        "Thread permission changed Post content",
    )?;
    ensure(
        hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == hash(RULES.as_bytes()),
        "RULES changed",
    )?;
    runtime.stop()?;
    save(
        "result.json",
        &json!({"status":"PENDING_ROLE_AND_THREAD_DENY_PASS","recipient_id":recipient["id"],"role_id":role["id"],"thread_id":t,"post_id":latest["id"],"post_revision":2,"post_body_sha256":hash(BODY.as_bytes()),"same_credential_recovered":true,"runtime_exited":true,"runtime_pid":runtime.0.id(),"closed_stream_bytes":[fs::metadata(Path::new(BASE).join("runtime.stdout.log"))?.len(),fs::metadata(Path::new(BASE).join("runtime.stderr.log"))?.len()],"native_gui_observed":false,"server_admission_instrumented":false}),
    )?;
    println!("PENDING_ROLE_AND_THREAD_DENY_PASS");
    Ok(())
}
fn main() {
    let result = match std::env::args().nth(1).as_deref() {
        None => run(),
        Some("--continue-after-role") => continue_after_role(),
        _ => Err("unknown operation".into()),
    };
    if let Err(e) = result {
        if let Some(f) = e.downcast_ref::<swarm_core::protocol::Fault>() {
            eprintln!("Product fault code: {}", f.code);
        }
        eprintln!("Role wait failed; preserve Data/receipts; no automatic replay");
        std::process::exit(1);
    }
}
