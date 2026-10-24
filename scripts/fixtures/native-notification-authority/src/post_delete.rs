//! Pending individual Post soft deletion against real owned OS history; preserves the live peer.
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
    "NOTIFY_POST_DELETE_POS_AG",
    "NOTIFY_POST_DELETE_REMOVED_AH",
    "NOTIFY_POST_DELETE_UNREAD_AI",
    "NOTIFY_POST_DELETE_FRESH_AJ",
];
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
    history_for(&MARKERS)
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
    let previous: Value = serde_json::from_slice(&fs::read(Path::new(BASE).join("result.json"))?)?;
    ensure(
        previous["status"] == "NORMAL_LOGOUT_NOTIFICATION_PASS",
        "logout success absent",
    )?;
    let prior: Value =
        serde_json::from_slice(&fs::read(Path::new(BASE).join("board-deny-result.json"))?)?;
    ensure(
        prior["status"] == "QUEUED_BOARD_DENY_NOTIFICATION_PASS",
        "deletion suffix absent",
    )?;
    ensure(
        swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"] == "stopped",
        "fixture not stopped",
    )?;
    ensure(
        !registration()?.is_registered()?,
        "registration unexpectedly present",
    )?;
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
    ensure(history()? == [0, 0, 0, 0], "new markers already present")?;
    save(
        "post-delete-attempt.json",
        &json!({"scope":"queued-post-delete-only","replay_logout":false,"vm":false,"elapsed":false}),
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
                .open(Path::new(BASE).join("post-delete.stdout.log"))?,
        )
        .stderr(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join("post-delete.stderr.log"))?,
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
        "post-delete-runtime.json",
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
    ensure(
        before["items"].as_array().ok_or("Posts absent")?.len() == 30,
        "original Post count changed",
    )?;
    ensure(outbox(recipient_id)? == 0, "old pending items")?;
    let rules_hash = hash(&fs::read(Path::new(ROOT).join("RULES.md"))?);
    swarm_runtime::notifications::register(
        Path::new(ROOT),
        &Path::new(CANDIDATE).join("swarm-desktop.exe"),
    )?;
    let positive = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":MARKERS[0]}),
    )?;
    let positive_history = wait_history(0)?;
    wait_drained(recipient_id)?;
    save(
        "post-delete-positive.json",
        &json!({"post_id":positive["id"],"history":positive_history,"outbox":0}),
    )?;
    let began = Instant::now();
    let queued = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":MARKERS[1]}),
    )?;
    let unread = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":MARKERS[2]}),
    )?;
    let queued_count = outbox(recipient_id)?;
    ensure(queued_count == 2, "two Posts were not pending")?;
    let removed = call(
        &admin_alias,
        "post.delete",
        json!({"thread_id":t,"no":queued["no"]}),
    )?;
    ensure(
        removed["id"] == queued["id"]
            && removed["deleted"] == true
            && removed["body"].is_null()
            && removed["revision"] == 2,
        "deleted Post skeleton mismatch",
    )?;
    let recipient_view = call(
        &recipient_alias,
        "post.get",
        json!({"thread_id":t,"no":queued["no"]}),
    )?;
    ensure(
        recipient_view["deleted"] == true && recipient_view["body"].is_null(),
        "recipient deleted body exposed",
    )?;
    let delete_ms = began.elapsed().as_millis();
    ensure(
        delete_ms < 1500,
        "delete missed aggregation margin; not accepted",
    )?;
    let unread_history = wait_history(2)?;
    ensure(
        unread_history == [1, 0, 1, 0],
        "deleted marker delivered or unread missing",
    )?;
    wait_drained(recipient_id)?;
    let until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < until {
        ensure(history()?[1] == 0, "deleted Post delivered")?;
        std::thread::sleep(Duration::from_millis(200));
    }
    save(
        "post-delete-negative.json",
        &json!({"deleted_post_id":queued["id"],"unread_post_id":unread["id"],
        "queued":queued_count,"delete_ms":delete_ms,"history":unread_history,"outbox":0}),
    )?;
    let restored = call(
        &admin_alias,
        "post.restore",
        json!({"thread_id":t,"no":queued["no"]}),
    )?;
    ensure(
        restored["id"] == queued["id"]
            && restored["body"] == MARKERS[1]
            && restored["revision"] == 3
            && restored["deleted"] == false,
        "restoration mismatch",
    )?;
    let until = Instant::now() + Duration::from_secs(3);
    while Instant::now() < until {
        ensure(history()? == [1, 0, 1, 0], "restored old delivery revived")?;
        std::thread::sleep(Duration::from_millis(200));
    }
    let fresh = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":MARKERS[3]}),
    )?;
    let final_history = wait_history(3)?;
    wait_drained(recipient_id)?;
    ensure(final_history == [1, 0, 1, 1], "unexpected marker counts")?;
    ensure(alias()? == recipient_alias, "selection changed")?;
    let after = call(&recipient_alias, "post.list", json!({"thread_id":t}))?;
    let items = after["items"].as_array().ok_or("Posts absent")?;
    let old_items = before["items"].as_array().ok_or("old Posts absent")?;
    ensure(items.len() == 34, "expected thirtyfour retained Posts")?;
    ensure(
        &items[..30] == old_items.as_slice(),
        "original thirty Posts changed",
    )?;
    for (index, (p, marker)) in items[30..].iter().zip(MARKERS).enumerate() {
        ensure(
            p["body"] == marker && p["revision"] == if index == 1 { 3 } else { 1 },
            "new Posts changed",
        )?;
    }
    let subscriptions = call(&recipient_alias, "conversation.subscriptions", json!({}))?;
    ensure(
        subscriptions["items"]
            .as_array()
            .ok_or("subscriptions absent")?
            .iter()
            .any(|i| i["id"] == t),
        "subscription not restored",
    )?;
    ensure(
        hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == rules_hash,
        "RULES changed",
    )?;
    runtime.stop()?;
    registration()?.remove()?;
    ensure(!registration()?.is_registered()?, "registration remains")?;
    save(
        "post-delete-result.json",
        &json!({
            "status":"SPARSE_POST_DELETE_NOTIFICATION_PASS","runtime_pid":runtime.child.id(),
            "history":final_history,"queued":queued_count,"delete_ms":delete_ms,
            "post_ids":[positive["id"],queued["id"],unread["id"],fresh["id"]],
            "original_thirty_posts_unchanged":true,"thirtyfour_posts_preserved":true,"deleted_restored_post_revision":3,"restoration_no_delivery_revival":true,
            "same_alias_and_selection":true,"subscription_restored":true,"rules_unchanged":true,
            "normally_exited":true,"owned_registration_absent":true,
            "closed_stream_bytes":[fs::metadata(Path::new(BASE).join("post-delete.stdout.log"))?.len(),fs::metadata(Path::new(BASE).join("post-delete.stderr.log"))?.len()],
            "native_gui_observed":false,"banner_click_observed":false
        }),
    )?;
    println!("SPARSE_POST_DELETE_NOTIFICATION_PASS");
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
            "post-delete-failure.json",
            &json!({"status":"NOT_ACCEPTED","error":error.to_string()}),
        );
        eprintln!("SPARSE_POST_DELETE_NOTIFICATION_NOT_ACCEPTED: {error}");
        std::process::exit(1);
    }
}
