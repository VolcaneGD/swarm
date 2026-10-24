//! Fixed-root pending Thread/Board deletion notification safety. No GUI/clock/old-root mutation.
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
use swarm_platform::{
    CredentialStore,
    notifications::{Identity, Registration},
};
use swarm_runtime::gui::GuiClient;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261022-notification-logout";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261022-notification-logout/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const MARKERS: [&str; 4] = [
    "NOTIFY_THREAD_DELETE_POSITIVE_I",
    "NOTIFY_THREAD_DELETE_QUEUED_J",
    "NOTIFY_THREAD_DELETE_DENIED_K",
    "NOTIFY_THREAD_DELETE_FRESH_L",
];
const BOARD_MARKERS: [&str; 4] = [
    "NOTIFY_BOARD_DELETE_POSITIVE_M",
    "NOTIFY_BOARD_DELETE_QUEUED_N",
    "NOTIFY_BOARD_DELETE_DENIED_O",
    "NOTIFY_BOARD_DELETE_FRESH_P",
];
fn board_case() -> bool {
    std::env::args().nth(1).as_deref() == Some("--board")
}
fn case() -> &'static str {
    if board_case() {
        "board-delete"
    } else {
        "thread-delete"
    }
}
fn markers() -> &'static [&'static str; 4] {
    if board_case() {
        &BOARD_MARKERS
    } else {
        &MARKERS
    }
}
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
        .open(Path::new(BASE).join(format!("{}-{n}", case())))?;
    writeln!(f, "{v}")?;
    Ok(())
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
fn call(alias: &str, op: &str, args: Value) -> Result<Value> {
    Ok(GuiClient::open(Path::new(ROOT))?.request(alias, req(op, args))?)
}
fn alias() -> Result<String> {
    let v: Value = serde_json::from_slice(
        &CredentialStore::open(&Path::new(ROOT).join("gui-state"))?
            .get("selection")?
            .ok_or("selection missing")?,
    )?;
    Ok(v["alias"].as_str().ok_or("alias missing")?.into())
}
fn registration() -> Result<Registration> {
    Ok(Registration::new(Identity::new(
        Path::new(ROOT),
        &Path::new(CANDIDATE).join("swarm-desktop.exe"),
    )?)?)
}
fn history() -> Result<[usize; 4]> {
    history_for(markers())
}
fn history_for(markers: &[&str; 4]) -> Result<[usize; 4]> {
    use windows::{
        UI::Notifications::ToastNotificationManager,
        Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
        core::HSTRING,
    };
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    let result = (|| -> Result<[usize; 4]> {
        let identity = Identity::new(
            Path::new(ROOT),
            &Path::new(CANDIDATE).join("swarm-desktop.exe"),
        )?;
        let entries = ToastNotificationManager::History()?
            .GetHistoryWithId(&HSTRING::from(identity.app_id()))?;
        let mut counts = [0; 4];
        for entry in entries {
            let xml = entry.Content()?.GetXml()?.to_string();
            for (i, m) in markers.iter().enumerate() {
                counts[i] += usize::from(xml.contains(m));
            }
        }
        Ok(counts)
    })();
    unsafe {
        RoUninitialize();
    }
    result
}
fn outbox(account: &str) -> Result<i64> {
    let db = rusqlite::Connection::open_with_flags(
        Path::new(ROOT).join("swarm.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    Ok(db.query_row(
        "SELECT COUNT(*) FROM notification_outbox WHERE account_id=?1",
        [account],
        |r| r.get(0),
    )?)
}
fn wait_history(index: usize) -> Result<[usize; 4]> {
    let until = Instant::now() + Duration::from_secs(12);
    loop {
        let h = history()?;
        if h[index] > 0 {
            return Ok(h);
        }
        ensure(
            Instant::now() < until,
            "positive OS history missing; no negative acceptance",
        )?;
        std::thread::sleep(Duration::from_millis(200));
    }
}
fn no_revoked_for(seconds: u64) -> Result<[usize; 4]> {
    let until = Instant::now() + Duration::from_secs(seconds);
    loop {
        let h = history()?;
        ensure(
            h[1] == 0 && h[2] == 0,
            "revoked marker reached owned OS history",
        )?;
        if Instant::now() >= until {
            return Ok(h);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
struct Runtime {
    child: Child,
    admin: Option<String>,
}
impl Runtime {
    fn stop(&mut self) -> Result<()> {
        if self.child.try_wait()?.is_some() {
            return Ok(());
        }
        let admin = match &self.admin {
            Some(admin) => admin.clone(),
            None => alias()?,
        };
        GuiClient::open(Path::new(ROOT))?.stop(&admin)?;
        let until = Instant::now() + Duration::from_secs(12);
        while self.child.try_wait()?.is_none() {
            ensure(
                Instant::now() < until,
                "normal stop deadline; preserve process",
            )?;
            std::thread::sleep(Duration::from_millis(40));
        }
        Ok(())
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.stop();
        if let Ok(r) = registration() {
            let _ = r.remove();
        }
    }
}

fn run() -> Result<()> {
    ensure(
        std::env::args().len() == 2
            && matches!(
                std::env::args().nth(1).as_deref(),
                Some("--thread" | "--board")
            ),
        "choose fixed --thread or --board",
    )?;
    let previous: Value = serde_json::from_slice(&fs::read(Path::new(BASE).join("result.json"))?)?;
    ensure(
        previous["status"] == "NORMAL_LOGOUT_NOTIFICATION_PASS",
        "logout receipt missing",
    )?;
    let prereq = if board_case() {
        "thread-delete-result.json"
    } else {
        "unsubscribe-result.json"
    };
    let preceding: Value = serde_json::from_slice(&fs::read(Path::new(BASE).join(prereq))?)?;
    let required = if board_case() {
        "QUEUED_THREAD_DELETE_NOTIFICATION_PASS"
    } else {
        "QUEUED_UNSUBSCRIBE_NOTIFICATION_PASS"
    };
    ensure(
        preceding["status"] == required,
        "preceding suffix not complete",
    )?;
    ensure(
        swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"] == "stopped",
        "fixture not stopped",
    )?;
    ensure(!registration()?.is_registered()?, "registration present")?;
    for (name, expected) in [
        (
            "swarm-runtime.exe",
            "0cf3ecb753c20830be7edf2c3630b3cc07aac90655d963b1097374f8b2b24210",
        ),
        (
            "swarm-desktop.exe",
            "ffbc41afad0c2a29246b7471b95b1068a40b3052d65ae3944e1547693340209a",
        ),
    ] {
        ensure(
            hash(&fs::read(Path::new(CANDIDATE).join(name))?) == expected,
            "candidate changed",
        )?;
    }
    ensure(history()? == [0, 0, 0, 0], "case markers already present")?;
    save(
        "attempt.json",
        &json!({"scope":case(),"logout_unsubscribe_replay":false,"vm":false,"elapsed":false}),
    )?;
    let admin_alias = format!("account-{}", hash(b"NotifyLogoutAdmin"));
    swarm_runtime::startup_preference_set(Path::new(ROOT), false)?;
    let mut command = Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
    command
        .args(["--data-dir", ROOT, "--human-start"])
        .stdin(Stdio::null())
        .stdout(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join(format!("{}.stdout.log", case())))?,
        )
        .stderr(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join(format!("{}.stderr.log", case())))?,
        );
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut runtime = Runtime {
        child: command.spawn()?,
        admin: Some(admin_alias.clone()),
    };
    save(
        "runtime.json",
        &json!({"pid":runtime.child.id(),"frozen_candidate":CANDIDATE}),
    )?;
    let until = Instant::now() + Duration::from_secs(15);
    while GuiClient::open(Path::new(ROOT))
        .and_then(|c| c.setup_status())
        .is_err()
    {
        ensure(Instant::now() < until, "runtime readiness timeout")?;
        std::thread::sleep(Duration::from_millis(50));
    }

    let recipient_alias = alias()?;
    let recipient_id = previous["recipient_id"]
        .as_str()
        .ok_or("recipient absent")?;
    let t = previous["thread_id"].as_str().ok_or("Thread absent")?;
    let before = call(&recipient_alias, "post.list", json!({"thread_id":t}))?;
    let old_items = before["items"].as_array().ok_or("old Posts absent")?;
    let expected = if board_case() { 11 } else { 8 };
    ensure(old_items.len() == expected, "unexpected prior Post count")?;
    let thread = call(&admin_alias, "thread.get", json!({"id":t}))?;
    let target = if board_case() {
        thread["board_id"].as_str().ok_or("Board missing")?
    } else {
        t
    };
    let delete_op = if board_case() {
        "board.delete"
    } else {
        "thread.delete"
    };
    let restore_op = if board_case() {
        "board.restore"
    } else {
        "thread.restore"
    };
    let rules_hash = hash(&fs::read(Path::new(ROOT).join("RULES.md"))?);
    ensure(outbox(recipient_id)? == 0, "prior pending items")?;
    swarm_runtime::notifications::register(
        Path::new(ROOT),
        &Path::new(CANDIDATE).join("swarm-desktop.exe"),
    )?;
    let m = markers();
    let positive = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":m[0]}),
    )?;
    let positive_history = wait_history(0)?;
    wait_drained(recipient_id)?;
    save(
        "positive.json",
        &json!({"post_id":positive["id"],"history":positive_history,"outbox":0}),
    )?;
    let began = Instant::now();
    let queued = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":m[1]}),
    )?;
    let queued_count = outbox(recipient_id)?;
    ensure(queued_count == 1, "Post was not pending")?;
    call(&admin_alias, delete_op, json!({"id":target}))?;
    let delete_ms = began.elapsed().as_millis();
    ensure(
        delete_ms < 1500,
        "delete missed aggregation margin; not accepted",
    )?;
    let denied_read = GuiClient::open(Path::new(ROOT))?
        .request(&recipient_alias, req("post.list", json!({"thread_id":t})))
        .err()
        .ok_or("deleted content remained readable")?;
    let denied_create = GuiClient::open(Path::new(ROOT))?
        .request(
            &admin_alias,
            req("post.create", json!({"thread_id":t,"body":m[2]})),
        )
        .err()
        .ok_or("deleted Thread accepted new Post")?;
    call(&recipient_alias, "account.status", json!({}))?;
    let negative = no_revoked_for(7)?;
    ensure(outbox(recipient_id)? == 0, "deleted queue retained")?;
    save(
        "negative.json",
        &json!({"queued_post_id":queued["id"],"queued":queued_count,"delete_ms":delete_ms,"read_code":denied_read.code,"create_code":denied_create.code,"history":negative,"outbox":0}),
    )?;
    call(&admin_alias, restore_op, json!({"id":target}))?;
    no_revoked_for(3)?;
    let restored = call(&recipient_alias, "post.list", json!({"thread_id":t}))?;
    ensure(
        restored["items"]
            .as_array()
            .ok_or("restored Posts absent")?
            .len()
            == expected + 2,
        "restored count changed",
    )?;
    let fresh = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":m[3]}),
    )?;
    let final_history = wait_history(3)?;
    wait_drained(recipient_id)?;
    ensure(final_history == [1, 0, 0, 1], "stale or unexpected history")?;
    ensure(alias()? == recipient_alias, "selection changed")?;
    let after = call(&recipient_alias, "post.list", json!({"thread_id":t}))?;
    let items = after["items"].as_array().ok_or("Posts absent")?;
    ensure(items.len() == expected + 3, "final Post count changed")?;
    ensure(
        &items[..expected] == old_items.as_slice(),
        "old Post items changed",
    )?;
    for (p, marker) in items[expected..].iter().zip([m[0], m[1], m[3]]) {
        ensure(
            p["body"] == marker && p["revision"] == 1,
            "new Post changed",
        )?;
    }
    ensure(
        hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == rules_hash,
        "RULES changed",
    )?;
    runtime.stop()?;
    registration()?.remove()?;
    ensure(!registration()?.is_registered()?, "registration remains")?;
    let status = if board_case() {
        "QUEUED_BOARD_DELETE_NOTIFICATION_PASS"
    } else {
        "QUEUED_THREAD_DELETE_NOTIFICATION_PASS"
    };
    save(
        "result.json",
        &json!({"status":status,"runtime_pid":runtime.child.id(),"history":final_history,
        "queued":queued_count,"delete_ms":delete_ms,"read_code":denied_read.code,"create_code":denied_create.code,
        "post_ids":[positive["id"],queued["id"],fresh["id"]],"prior_posts":expected,"final_posts":items.len(),
        "old_full_post_items_unchanged":true,"same_alias_and_selection":true,"rules_unchanged":true,
        "normally_exited":true,"owned_registration_absent":true,"closed_stream_bytes":[
        fs::metadata(Path::new(BASE).join(format!("{}.stdout.log",case())))?.len(),
        fs::metadata(Path::new(BASE).join(format!("{}.stderr.log",case())))?.len()],
        "native_gui_observed":false,"banner_click_observed":false}),
    )?;
    println!("{status}");
    Ok(())
}
fn wait_drained(account: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while outbox(account)? != 0 {
        ensure(
            Instant::now() < deadline,
            "OS history present but outbox not drained",
        )?;
        std::thread::sleep(Duration::from_millis(40));
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        // Product Fault displays fixed safe messages; never print response bodies.
        let _ = save(
            "failure.json",
            &json!({"status":"NOT_ACCEPTED","error":error.to_string()}),
        );
        eprintln!("DELETION_NOTIFICATION_NOT_ACCEPTED: {error}");
        std::process::exit(1);
    }
}
