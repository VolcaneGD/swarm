//! New in-flight public CLI joins only: Thread deletion and credential rotation.
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
const BASE: &str = "E:/Swarm-Validation-Host/run-20261021-changes-wait-authority";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261021-changes-wait-authority/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const ALIAS: &str = "changes-wait-authority-20261021";
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
struct Runtime(Child);
impl Runtime {
    fn stop(&mut self) -> Result<()> {
        if self.0.try_wait()?.is_some() {
            return Ok(());
        }
        GuiClient::open(Path::new(ROOT))?.stop(&selected()?)?;
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
            "candidate hash changed",
        )?;
    }
    ensure(
        !Path::new(BASE).exists(),
        "existing fixture refused; preserve evidence",
    )?;
    fs::create_dir(BASE)?;
    fs::create_dir(ROOT)?;
    swarm_runtime::startup_preference_set(Path::new(ROOT), false)?;
    fs::write(Path::new(ROOT).join("RULES.md"), RULES)?;
    let mut c = Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
    c.args(["--data-dir", ROOT, "--human-start"])
        .stdin(Stdio::null())
        .stdout(
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
    let mut runtime = Runtime(c.spawn()?);
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
        ensure(Instant::now() < until, "runtime readiness")?;
        std::thread::sleep(Duration::from_millis(40));
    }
    let mut h = GuiHost::open(Path::new(ROOT))?;
    let password = uuid::Uuid::new_v4().simple().to_string();
    let account =
        h.authenticate("setup", "ChangesWaitFixture", &password)?["account"]["id"].clone();
    h.backup_control(
        account.as_str().ok_or("account missing")?,
        "settings.set",
        json!({"enabled":false,"interval_seconds":86400,"retention":2,"destination":null}),
    )?;
    let project_root = Path::new(BASE).join("Project");
    fs::create_dir(&project_root)?;
    fs::write(project_root.join("RULES.md"), RULES)?;
    let project = h.project_control(
        account.as_str().ok_or("account missing")?,
        "project.register",
        json!({"name":"Changes wait authority","root":project_root}),
    )?;
    let p = project["id"].as_str().ok_or("project missing")?;
    let board = created(
        &mut h,
        "board.create",
        json!({"name":"WAIT_AUTHORITY_BOARD"}),
    )?;
    let thread = created(
        &mut h,
        "thread.create",
        json!({"name":"WAIT_AUTHORITY_THREAD","board_id":board["id"]}),
    )?;
    let t = thread["id"].as_str().ok_or("thread missing")?;
    let post = created(
        &mut h,
        "post.create",
        json!({"thread_id":t,"body":"SYNTHETIC_WAIT_POST_ORIGINAL_20261021"}),
    )?;
    let mut secrets = vec![password];
    secrets.push(String::from_utf8(
        CredentialStore::open(&Path::new(ROOT).join("gui-host"))?
            .get("transport-capability")?
            .ok_or("capability missing")?,
    )?);
    cli(p, &["identity", "start"], 0, "start", &secrets)?;
    let initial = ai()?;
    secrets.extend(ai_secrets(&initial));
    let cursor = cli(
        p,
        &["changes", "get", "--thread-id", t],
        0,
        "positive-bootstrap",
        &secrets,
    )?["change_cursor"]
        .as_str()
        .ok_or("cursor missing")?
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
    let changed = pending(&mut waiter, "positive")?;
    let latest = h.edit(json!({"thread_id":t,"no":1,"if_revision":1,"body":BODY}))?;
    let positive = collect(waiter, 0, "positive-wait", &secrets)?;
    let events = positive["items"].as_array().ok_or("events missing")?;
    ensure(
        events.len() == 1
            && events[0]["operation"] == "post.edit"
            && events[0]["entity_id"] == post["id"]
            && !serde_json::to_string(&positive)?.contains(BODY),
        "positive metadata mismatch",
    )?;
    save(
        "positive.json",
        &json!({"event":"post.edit","items":1,"body_absent":true,"completed_after_edit_ms":changed.elapsed().as_millis()}),
    )?;
    let cursor = cli(
        p,
        &["changes", "get", "--thread-id", t],
        0,
        "delete-bootstrap",
        &secrets,
    )?["change_cursor"]
        .as_str()
        .ok_or("cursor missing")?
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
    let changed = pending(&mut waiter, "delete")?;
    GuiClient::open(Path::new(ROOT))?
        .request(&selected()?, req("thread.delete", json!({"id":t})))?;
    ensure(
        collect(waiter, 5, "deleted-wait", &secrets)?["code"] == "NOT_FOUND",
        "deleted wait not concealed",
    )?;
    ensure(
        cli(
            p,
            &["changes", "get", "--thread-id", t],
            5,
            "deleted-get",
            &secrets,
        )?["code"]
            == "NOT_FOUND",
        "deleted get not concealed",
    )?;
    save(
        "deleted.json",
        &json!({"wait_exit":5,"wait_code":"NOT_FOUND","fresh_get_exit":5,"elapsed_after_delete_ms":changed.elapsed().as_millis()}),
    )?;
    GuiClient::open(Path::new(ROOT))?
        .request(&selected()?, req("thread.restore", json!({"id":t})))?;
    let restored = h.read("post.list", json!({"thread_id":t}))?;
    let items = restored["items"]
        .as_array()
        .ok_or("restored items missing")?;
    ensure(
        items.len() == 1
            && items[0]["id"] == latest["id"]
            && items[0]["revision"] == latest["revision"]
            && items[0]["body"] == latest["body"],
        "restoration altered original Post",
    )?;
    let cursor = cli(
        p,
        &["changes", "get", "--thread-id", t],
        0,
        "resume-bootstrap",
        &secrets,
    )?["change_cursor"]
        .as_str()
        .ok_or("cursor missing")?
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
    let changed = pending(&mut waiter, "resume")?;
    let CredentialCandidate::Ai { id, .. } = &initial else {
        return Err("wrong AI candidate".into());
    };
    cli(
        p,
        &["identity", "resume", "--actor-id", id],
        0,
        "resume",
        &secrets,
    )?;
    let current = ai()?;
    secrets.extend(ai_secrets(&current));
    ensure(
        collect(waiter, 3, "old-credential-wait", &secrets)?["code"] == "AUTHENTICATION_REQUIRED",
        "old waiting credential not rejected",
    )?;
    ensure(
        cli(
            p,
            &["changes", "get", "--thread-id", t, "--after", &cursor],
            6,
            "old-cursor-new-credential",
            &secrets,
        )?["code"]
            == "RESYNC_REQUIRED",
        "old credential cursor accepted",
    )?;
    cli(
        p,
        &["changes", "get", "--thread-id", t],
        0,
        "fresh-cursor",
        &secrets,
    )?;
    save(
        "resumed.json",
        &json!({"old_wait_exit":3,"old_wait_code":"AUTHENTICATION_REQUIRED","old_cursor_new_credential_exit":6,"new_cursor_success":true,"elapsed_after_resume_ms":changed.elapsed().as_millis()}),
    )?;
    cli(p, &["identity", "release"], 0, "release", &secrets)?;
    runtime.stop()?;
    ensure(
        hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == hash(RULES.as_bytes()),
        "fixture RULES changed",
    )?;
    save(
        "result.json",
        &json!({"status":"PUBLIC_IN_FLIGHT_PARENT_DELETE_AND_RESUME_PASS","project_id":p,"thread_id":t,"post_id":latest["id"],"post_revision":latest["revision"],"post_body_sha256":hash(BODY.as_bytes()),"thread_restored":true,"identity_released":true,"runtime_exited":true,"closed_stream_bytes":[fs::metadata(Path::new(BASE).join("runtime.stdout.log"))?.len(),fs::metadata(Path::new(BASE).join("runtime.stderr.log"))?.len()],"rules_unchanged":true,"native_gui_observed":false,"vm":false,"elapsed":false,"publication":false}),
    )?;
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        if let Some(f) = e.downcast_ref::<swarm_core::protocol::Fault>() {
            eprintln!("Product fault code: {}", f.code);
        }
        eprintln!("Changes wait fixture failed; preserve safe receipts and owned Data");
        std::process::exit(1);
    }
}
