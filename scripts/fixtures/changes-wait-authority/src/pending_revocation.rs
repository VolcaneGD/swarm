//! In-flight AI release and human disable/reset joins; frozen runtime, normal API.
//! Normal product operations, no GUI, SQL mutation, clock or fault endpoint.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use swarm_core::{CredentialCandidate, protocol::Request};
use swarm_gui_host::GuiHost;
use swarm_platform::CredentialStore;
use swarm_runtime::gui::GuiClient;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261022-wait-revocation";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261022-wait-revocation/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const ALIAS: &str = "wait-revocation-20261022";
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
fn cli_child(p: &str, args: &[&str]) -> Result<Child> {
    let mut c = Command::new(Path::new(CANDIDATE).join("swarm.exe"));
    c.args([
        "--data-dir",
        ROOT,
        "--project",
        p,
        "--session",
        ALIAS,
        "--format",
        "json",
    ])
    .args(args)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    Ok(c.spawn()?)
}
fn collect(mut child: Child, expected: i32, label: &str, secrets: &[String]) -> Result<Value> {
    let until = Instant::now() + Duration::from_secs(12);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        ensure(
            Instant::now() < until,
            "CLI deadline; runtime cleanup must release outstanding wait",
        )?;
        std::thread::sleep(Duration::from_millis(15));
    }
    let o = child.wait_with_output()?;
    ensure(
        !secrets.iter().any(|s| {
            !s.is_empty()
                && (o.stdout.windows(s.len()).any(|w| w == s.as_bytes())
                    || o.stderr.windows(s.len()).any(|w| w == s.as_bytes()))
        }),
        "secret output detected; do not print",
    )?;
    let v: Value = serde_json::from_slice(&o.stdout)?;
    save(
        &format!("cli-{label}.json"),
        &json!({"label":label,"expected_exit":expected,"actual_exit":o.status.code(),"ok":v["ok"],"code":v["error"]["code"],"items_count":v["data"]["items"].as_array().map(Vec::len),"stdout_bytes":o.stdout.len(),"stderr_bytes":o.stderr.len(),"no_secret_match":true,"raw_saved":false}),
    )?;
    ensure(
        o.status.code() == Some(expected) && o.stderr.is_empty() && v["ok"] == (expected == 0),
        "CLI contract mismatch; retain safe receipt",
    )?;
    Ok(if expected == 0 {
        v["data"].clone()
    } else {
        v["error"].clone()
    })
}
fn cli(p: &str, args: &[&str], expected: i32, label: &str, secrets: &[String]) -> Result<Value> {
    collect(cli_child(p, args)?, expected, label, secrets)
}
fn pending(child: &mut Child, label: &str) -> Result<Instant> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(600) {
        ensure(
            child.try_wait()?.is_none(),
            "wait completed before authority changed",
        )?;
        std::thread::sleep(Duration::from_millis(20));
    }
    save(
        &format!("pending-{label}.json"),
        &json!({"pid":child.id(),"running_for_ms":start.elapsed().as_millis(),"result_not_yet_available":true,"server_request_read_not_instrumented":true}),
    )?;
    Ok(Instant::now())
}
fn ai() -> Result<CredentialCandidate> {
    Ok(serde_json::from_slice(
        &CredentialStore::open(&Path::new(ROOT).join("client-credentials"))?
            .get(ALIAS)?
            .ok_or("AI missing")?,
    )?)
}
fn ai_secrets(c: &CredentialCandidate) -> Vec<String> {
    match c {
        CredentialCandidate::Ai {
            session_token,
            resume_secret,
            ..
        } => vec![session_token.clone(), resume_secret.clone()],
        _ => vec![],
    }
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
fn assert_old_human(alias: &str, t: &str, label: &str, secrets: &[String]) -> Result<()> {
    let result = GuiClient::open(Path::new(ROOT))?
        .request(alias, req("changes.get", json!({"thread_id":t})));
    let serialized = serde_json::to_string(&result)?;
    ensure(
        !secrets
            .iter()
            .any(|s| !s.is_empty() && serialized.contains(s)),
        "old human output secret match",
    )?;
    let code = result.err().ok_or("old human unexpectedly succeeded")?.code;
    save(
        &format!("human-{label}.json"),
        &json!({"code":code,"no_secret_match":true}),
    )?;
    ensure(
        code == "AUTHENTICATION_REQUIRED",
        "old human credential revived",
    )
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
        h.authenticate("setup", "WaitRevocationAdmin", &admin_password)?["account"]["id"].clone();
    let admin_alias = selected()?;
    runtime.1 = Some(admin_alias.clone());
    h.backup_control(
        account.as_str().ok_or("admin missing")?,
        "settings.set",
        json!({"enabled":false,"interval_seconds":86400,"retention":2,"destination":null}),
    )?;
    let project_root = Path::new(BASE).join("Project");
    fs::create_dir(&project_root)?;
    fs::write(project_root.join("RULES.md"), RULES)?;
    let project = h.project_control(
        account.as_str().ok_or("admin missing")?,
        "project.register",
        json!({"name":"Wait revocation","root":project_root}),
    )?;
    let p = project["id"].as_str().ok_or("project missing")?;
    let board = created(
        &mut h,
        "board.create",
        json!({"name":"WAIT_REVOCATION_BOARD"}),
    )?;
    let thread = created(
        &mut h,
        "thread.create",
        json!({"name":"WAIT_REVOCATION_THREAD","board_id":board["id"]}),
    )?;
    let t = thread["id"].as_str().ok_or("thread missing")?;
    let post = created(
        &mut h,
        "post.create",
        json!({"thread_id":t,"body":"SYNTHETIC_WAIT_REVOCATION_ORIGINAL"}),
    )?;
    let mut secrets = vec![
        admin_password,
        String::from_utf8(
            CredentialStore::open(&Path::new(ROOT).join("gui-host"))?
                .get("transport-capability")?
                .ok_or("capability missing")?,
        )?,
    ];
    cli(p, &["identity", "start"], 0, "start", &secrets)?;
    secrets.extend(ai_secrets(&ai()?));
    let cursor = cli(
        p,
        &["changes", "get", "--thread-id", t],
        0,
        "ai-bootstrap",
        &secrets,
    )?["change_cursor"]
        .as_str()
        .ok_or("AI cursor missing")?
        .to_owned();
    let mut waiter = cli_child(
        p,
        &[
            "changes",
            "wait",
            "--thread-id",
            t,
            "--after",
            &cursor,
            "--timeout",
            "8",
        ],
    )?;
    let release_at = pending(&mut waiter, "ai-release")?;
    cli(
        p,
        &["identity", "release"],
        0,
        "release-during-wait",
        &secrets,
    )?;
    ensure(
        collect(waiter, 3, "released-ai-wait", &secrets)?["code"] == "AUTHENTICATION_REQUIRED",
        "released wait not rejected",
    )?;
    ensure(
        cli(
            p,
            &["changes", "get", "--thread-id", t],
            3,
            "released-ai-fresh-get",
            &secrets,
        )?["code"]
            == "AUTHENTICATION_REQUIRED",
        "released AI fresh read accepted",
    )?;
    save(
        "ai-release.json",
        &json!({"old_wait_exit":3,"fresh_get_exit":3,"elapsed_after_release_ms":release_at.elapsed().as_millis()}),
    )?;

    let password = uuid::Uuid::new_v4().simple().to_string();
    secrets.push(password.clone());
    let recipient = h.manage_account(
        "account.create",
        json!({"name":"WaitRevocationRecipient","password":password}),
    )?;
    h.manage_authorization("permission.set",json!({"subject_type":"account","subject_id":recipient["id"],"operation":"read","scope":"global","target_id":"","decision":"allow"}))?;
    for operation in ["changes.get", "changes.wait"] {
        h.manage_authorization("permission.set",json!({"subject_type":"account","subject_id":recipient["id"],"operation":operation,"scope":"global","target_id":"","decision":"allow"}))?;
    }
    finish_human(
        h,
        runtime,
        admin_alias,
        recipient,
        password,
        json!({"project":p,"thread":t,"post":post,"project_root":project_root}),
        secrets,
    )
}
fn finish_human(
    mut h: GuiHost,
    mut runtime: Runtime,
    admin_alias: String,
    recipient: Value,
    password: String,
    context: Value,
    mut secrets: Vec<String>,
) -> Result<()> {
    let p = context["project"].as_str().ok_or("project missing")?;
    let t = context["thread"].as_str().ok_or("thread missing")?;
    let post = &context["post"];
    let project_root = Path::new(
        context["project_root"]
            .as_str()
            .ok_or("project root missing")?,
    );
    h.authenticate("login", "WaitRevocationRecipient", &password)?;
    let recipient_alias = selected()?;
    let cursor = human_cursor(&recipient_alias, t)?;
    let (rx, handle) = human_wait(&recipient_alias, t, &cursor);
    human_pending(&rx, "human-positive")?;
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
        events.len() == 1
            && events[0]["operation"] == "post.edit"
            && events[0]["entity_id"] == post["id"]
            && !serde_json::to_string(&positive)?.contains(BODY),
        "positive human metadata mismatch",
    )?;
    let cursor = human_cursor(&recipient_alias, t)?;
    let (rx, handle) = human_wait(&recipient_alias, t, &cursor);
    human_pending(&rx, "account-disable")?;
    let disable_at = Instant::now();
    GuiClient::open(Path::new(ROOT))?.request(
        &admin_alias,
        req(
            "account.disable",
            json!({"id":recipient["id"],"disabled":true}),
        ),
    )?;
    human_collect(
        rx,
        handle,
        "disabled-wait",
        Some("AUTHENTICATION_REQUIRED"),
        &secrets,
    )?;
    assert_old_human(&recipient_alias, t, "disabled-fresh-get", &secrets)?;
    GuiClient::open(Path::new(ROOT))?.request(
        &admin_alias,
        req(
            "account.disable",
            json!({"id":recipient["id"],"disabled":false}),
        ),
    )?;
    assert_old_human(&recipient_alias, t, "reenabled-old-get", &secrets)?;
    h.authenticate("login", "WaitRevocationRecipient", &password)?;
    let enabled_alias = selected()?;
    let cursor = human_cursor(&enabled_alias, t)?;
    save(
        "disabled.json",
        &json!({"old_wait_code":"AUTHENTICATION_REQUIRED","reenabled_old_session_refused":true,"new_login_fresh_cursor":true,"elapsed_with_reenable_login_ms":disable_at.elapsed().as_millis()}),
    )?;
    let (rx, handle) = human_wait(&enabled_alias, t, &cursor);
    human_pending(&rx, "password-reset")?;
    let new_password = uuid::Uuid::new_v4().simple().to_string();
    secrets.push(new_password.clone());
    GuiClient::open(Path::new(ROOT))?.request(
        &admin_alias,
        req(
            "account.reset",
            json!({"id":recipient["id"],"password":new_password}),
        ),
    )?;
    human_collect(
        rx,
        handle,
        "reset-wait",
        Some("AUTHENTICATION_REQUIRED"),
        &secrets,
    )?;
    assert_old_human(&enabled_alias, t, "reset-old-get", &secrets)?;
    h.authenticate("login", "WaitRevocationRecipient", &new_password)?;
    let final_alias = selected()?;
    let _ = human_cursor(&final_alias, t)?;
    let posts = h.read("post.list", json!({"thread_id":t}))?;
    let items = posts["items"].as_array().ok_or("final posts missing")?;
    ensure(
        items.len() == 1
            && items[0]["id"] == latest["id"]
            && items[0]["body"] == latest["body"]
            && items[0]["revision"] == latest["revision"],
        "Post changed during revocation",
    )?;
    ensure(
        hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == hash(RULES.as_bytes())
            && hash(&fs::read(project_root.join("RULES.md"))?) == hash(RULES.as_bytes()),
        "RULES changed",
    )?;
    drop(h);
    runtime.stop()?;
    save(
        "result.json",
        &json!({"status":"PENDING_RELEASE_DISABLE_RESET_PASS","project_id":p,"recipient_id":recipient["id"],"thread_id":t,"post_id":latest["id"],"post_revision":latest["revision"],"post_body_sha256":hash(BODY.as_bytes()),"ai_released":true,"recipient_enabled":true,"new_login_valid":true,"runtime_exited":true,"rules_unchanged":true,"closed_stream_bytes":[fs::metadata(Path::new(BASE).join("runtime.stdout.log"))?.len(),fs::metadata(Path::new(BASE).join("runtime.stderr.log"))?.len()],"native_gui_observed":false,"server_admission_instrumented":false,"vm":false,"elapsed":false,"publication":false}),
    )?;
    println!("PENDING_RELEASE_DISABLE_RESET_PASS");
    Ok(())
}
fn continue_human() -> Result<()> {
    ensure(
        Path::new(BASE).join("ai-release.json").exists()
            && !Path::new(BASE).join("pending-human-positive.json").exists()
            && !Path::new(BASE).join("human-continuation.json").exists(),
        "not the unexecuted human preparation boundary",
    )?;
    let prior: Value = serde_json::from_slice(&fs::read(Path::new(BASE).join("ai-release.json"))?)?;
    ensure(
        prior["old_wait_exit"] == 3 && prior["fresh_get_exit"] == 3,
        "AI release incomplete",
    )?;
    ensure(
        hash(&fs::read(Path::new(CANDIDATE).join("swarm-runtime.exe"))?)
            == "0cf3ecb753c20830be7edf2c3630b3cc07aac90655d963b1097374f8b2b24210",
        "frozen runtime changed",
    )?;
    ensure(
        swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"] == "stopped",
        "previous runtime not stopped",
    )?;
    let mut command = Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
    command
        .args(["--data-dir", ROOT, "--human-start"])
        .stdin(Stdio::null())
        .stdout(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join("runtime-recovery.stdout.log"))?,
        )
        .stderr(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join("runtime-recovery.stderr.log"))?,
        );
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let admin_alias = format!("account-{}", hash(b"WaitRevocationAdmin"));
    let runtime = Runtime(command.spawn()?, Some(admin_alias.clone()));
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if GuiClient::open(Path::new(ROOT))
            .and_then(|c| c.setup_status())
            .is_ok()
        {
            break;
        }
        ensure(Instant::now() < until, "continuation readiness deadline")?;
        std::thread::sleep(Duration::from_millis(40));
    }
    let c = GuiClient::open(Path::new(ROOT))?;
    let admin = c.request(&admin_alias, req("account.status", json!({})))?;
    ensure(
        admin["is_admin"] == true && admin["name"] == "WaitRevocationAdmin",
        "wrong retained admin",
    )?;
    let accounts = c.request(&admin_alias, req("account.list", json!({})))?;
    let candidates: Vec<&Value> = accounts["items"]
        .as_array()
        .ok_or("accounts missing")?
        .iter()
        .filter(|a| a["name"] == "WaitRevocationRecipient")
        .collect();
    ensure(candidates.len() == 1, "recipient identity ambiguous")?;
    let recipient = candidates[0].clone();
    let boards = c.request(&admin_alias, req("board.list", json!({})))?;
    let boards = boards["items"].as_array().ok_or("boards missing")?;
    ensure(
        boards.len() == 1 && boards[0]["name"] == "WAIT_REVOCATION_BOARD",
        "Board changed",
    )?;
    let threads = c.request(
        &admin_alias,
        req("thread.list", json!({"board_id":boards[0]["id"]})),
    )?;
    let threads = threads["items"].as_array().ok_or("Threads missing")?;
    ensure(
        threads.len() == 1 && threads[0]["name"] == "WAIT_REVOCATION_THREAD",
        "Thread changed",
    )?;
    let t = threads[0]["id"].as_str().ok_or("Thread missing")?;
    let posts = c.request(&admin_alias, req("post.list", json!({"thread_id":t})))?;
    let posts = posts["items"].as_array().ok_or("Posts missing")?;
    ensure(
        posts.len() == 1
            && posts[0]["revision"] == 1
            && posts[0]["body"] == "SYNTHETIC_WAIT_REVOCATION_ORIGINAL",
        "initial Post changed",
    )?;
    let projects = c.request(&admin_alias, req("project.list", json!({})))?;
    let projects = projects["items"].as_array().ok_or("Projects missing")?;
    ensure(projects.len() == 1, "Project ambiguous")?;
    let p = projects[0]["id"].as_str().ok_or("Project missing")?;
    for operation in ["changes.get", "changes.wait"] {
        c.request(&admin_alias,req("permission.set",json!({"subject_type":"account","subject_id":recipient["id"],"operation":operation,"scope":"global","target_id":"","decision":"allow"})))?;
    }
    let password = uuid::Uuid::new_v4().simple().to_string();
    c.request(
        &admin_alias,
        req(
            "account.reset",
            json!({"id":recipient["id"],"password":password}),
        ),
    )?;
    save(
        "human-continuation.json",
        &json!({"runtime_pid":runtime.0.id(),"same_data":true,"ai_release_not_repeated":true,"recipient_id":recipient["id"],"post_id":posts[0]["id"],"added_explicit_changes_grants":true,"synthetic_password_reset_for_preparation":true,"prior_password_not_recovered":true}),
    )?;
    let h = GuiHost::open(Path::new(ROOT))?;
    let secrets = vec![
        password.clone(),
        String::from_utf8(
            CredentialStore::open(&Path::new(ROOT).join("gui-host"))?
                .get("transport-capability")?
                .ok_or("capability missing")?,
        )?,
    ];
    let context = json!({"project":p,"thread":t,"post":posts[0],"project_root":Path::new(BASE).join("Project")});
    finish_human(
        h,
        runtime,
        admin_alias,
        recipient,
        password,
        context,
        secrets,
    )
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [] => run(),
        [arg] if arg == "--continue-human" => continue_human(),
        _ => Err("unknown operation".into()),
    };
    if let Err(e) = result {
        if let Some(f) = e.downcast_ref::<swarm_core::protocol::Fault>() {
            eprintln!("Product fault code: {}", f.code);
        }
        eprintln!("Pending revocation failed; preserve safe receipts and Data");
        std::process::exit(1);
    }
}
