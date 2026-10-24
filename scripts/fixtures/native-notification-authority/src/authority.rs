//! Consumed-once pending authority joins against the frozen runtime and real owned OS history.
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
fn mode() -> &'static str {
    match std::env::args().nth(1).as_deref() {
        Some("--thread") => "thread-deny",
        Some("--role") => "role-deny",
        Some("--board") => "board-deny",
        _ => "invalid",
    }
}
fn markers() -> [&'static str; 4] {
    if mode() == "thread-deny" {
        [
            "NOTIFY_THREAD_DENY_POS_U",
            "NOTIFY_THREAD_DENY_QUEUED_V",
            "NOTIFY_THREAD_DENY_HIDDEN_W",
            "NOTIFY_THREAD_DENY_FRESH_X",
        ]
    } else if mode() == "role-deny" {
        [
            "NOTIFY_ROLE_DENY_POS_Y",
            "NOTIFY_ROLE_DENY_QUEUED_Z",
            "NOTIFY_ROLE_DENY_HIDDEN_AA",
            "NOTIFY_ROLE_DENY_FRESH_AB",
        ]
    } else {
        [
            "NOTIFY_BOARD_DENY_POS_AC",
            "NOTIFY_BOARD_DENY_QUEUED_AD",
            "NOTIFY_BOARD_DENY_HIDDEN_AE",
            "NOTIFY_BOARD_DENY_FRESH_AF",
        ]
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
        .open(Path::new(BASE).join(format!("{}-{n}", mode())))?;
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
    history_for(&markers())
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
fn permission(
    admin: &str,
    subject: &str,
    id: &Value,
    thread: &str,
    decision: &str,
) -> Result<Value> {
    call(
        admin,
        "permission.set",
        json!({"subject_type":subject,"subject_id":id,
        "operation":"read","scope":if mode()=="board-deny" {"board"} else {"thread"},"target_id":thread,"decision":decision}),
    )
}
fn assignment(admin: &str, recipient: &str, role: &Value, assigned: bool) -> Result<Value> {
    call(
        admin,
        "role.assign",
        json!({"account_id":recipient,"role_id":role,"assigned":assigned}),
    )
}
fn run() -> Result<()> {
    ensure(mode() != "invalid", "choose --thread, --role or --board")?;
    let original: Value = serde_json::from_slice(&fs::read(Path::new(BASE).join("result.json"))?)?;
    ensure(
        original["status"] == "NORMAL_LOGOUT_NOTIFICATION_PASS",
        "original fixture absent",
    )?;
    let (prior_file, prior_status, count) = if mode() == "thread-deny" {
        ("read-mark-result.json", "SPARSE_READ_NOTIFICATION_PASS", 18)
    } else if mode() == "role-deny" {
        (
            "thread-deny-result.json",
            "QUEUED_THREAD_DENY_NOTIFICATION_PASS",
            22,
        )
    } else {
        (
            "role-deny-result.json",
            "QUEUED_ROLE_DENY_NOTIFICATION_PASS",
            26,
        )
    };
    let prior: Value = serde_json::from_slice(&fs::read(Path::new(BASE).join(prior_file))?)?;
    ensure(prior["status"] == prior_status, "prior suffix not complete")?;
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
    ensure(history()? == [0, 0, 0, 0], "new markers already present")?;
    save(
        "attempt.json",
        &json!({"scope":mode(),"prior_posts":count,"vm":false,"elapsed":false,"native_gui":false}),
    )?;
    let admin = format!("account-{}", hash(b"NotifyLogoutAdmin"));
    let mut command = Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
    command
        .args(["--data-dir", ROOT, "--human-start"])
        .stdin(Stdio::null())
        .stdout(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join(format!("{}.stdout.log", mode())))?,
        )
        .stderr(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join(format!("{}.stderr.log", mode())))?,
        );
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut runtime = Runtime {
        child: command.spawn()?,
        admin: Some(admin.clone()),
    };
    save(
        "runtime.json",
        &json!({"pid":runtime.child.id(),"candidate":CANDIDATE}),
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
    let recipient = original["recipient_id"]
        .as_str()
        .ok_or("recipient absent")?;
    let thread = original["thread_id"].as_str().ok_or("Thread absent")?;
    let thread_info = call(&recipient_alias, "thread.get", json!({"id":thread}))?;
    let target = if mode() == "board-deny" {
        thread_info["board_id"]
            .as_str()
            .ok_or("parent Board absent")?
    } else {
        thread
    };
    let before = call(&recipient_alias, "post.list", json!({"thread_id":thread}))?;
    ensure(
        before["items"].as_array().ok_or("Posts absent")?.len() == count,
        "prior Posts count changed",
    )?;
    ensure(outbox(recipient)? == 0, "prior pending items")?;
    let rules = hash(&fs::read(Path::new(ROOT).join("RULES.md"))?);
    let permissions_before = call(
        &admin,
        "permission.list",
        json!({"subject_type":"account","subject_id":recipient}),
    )?;
    ensure(
        !permissions_before["items"]
            .as_array()
            .ok_or("permissions absent")?
            .iter()
            .any(|p| {
                p["operation"] == "read"
                    && ((p["scope"] == "thread" && p["target_id"] == thread)
                        || (mode() == "board-deny"
                            && p["scope"] == "board"
                            && p["target_id"] == target))
            }),
        "existing individual Thread decision; do not overwrite",
    )?;
    let roles_before = call(&admin, "role.list", json!({}))?;
    let role = if mode() == "role-deny" {
        let v = call(
            &admin,
            "role.create",
            json!({"name":"PendingNotificationReadDeny"}),
        )?;
        permission(&admin, "role", &v["id"], target, "deny")?;
        Some(v["id"].clone())
    } else {
        None
    };
    // An unassigned denying role must leave recipient read and subscriptions valid.
    call(&recipient_alias, "post.list", json!({"thread_id":thread}))?;
    swarm_runtime::notifications::register(
        Path::new(ROOT),
        &Path::new(CANDIDATE).join("swarm-desktop.exe"),
    )?;
    let marks = markers();
    let positive = call(
        &admin,
        "post.create",
        json!({"thread_id":thread,"body":marks[0]}),
    )?;
    let positive_history = wait_history(0)?;
    wait_drained(recipient)?;
    save(
        "positive.json",
        &json!({"post_id":positive["id"],"history":positive_history,
        "role_id":role,"account_permissions_before":permissions_before,"role_count_before":roles_before["items"].as_array().map(Vec::len)}),
    )?;
    let began = Instant::now();
    let queued = call(
        &admin,
        "post.create",
        json!({"thread_id":thread,"body":marks[1]}),
    )?;
    let pending = outbox(recipient)?;
    ensure(
        pending == 1,
        "new Post not pending before permission change",
    )?;
    if let Some(role) = &role {
        assignment(&admin, recipient, role, true)?;
    } else {
        permission(&admin, "account", &json!(recipient), target, "deny")?;
    }
    let change_ms = began.elapsed().as_millis();
    ensure(
        change_ms < 1500,
        "authority change missed aggregation margin; not accepted",
    )?;
    let denied = GuiClient::open(Path::new(ROOT))?
        .request(
            &recipient_alias,
            req("post.list", json!({"thread_id":thread})),
        )
        .err()
        .ok_or("old recipient read accepted under deny")?;
    ensure(
        denied.code == "PERMISSION_DENIED",
        "unexpected read refusal",
    )?;
    call(&recipient_alias, "account.status", json!({}))?;
    let hidden = call(
        &admin,
        "post.create",
        json!({"thread_id":thread,"body":marks[2]}),
    )?;
    let negative = no_revoked_for(7)?;
    ensure(outbox(recipient)? == 0, "denied outbox retained")?;
    save(
        "negative.json",
        &json!({"queued_post_id":queued["id"],"hidden_post_id":hidden["id"],
        "pending_before":pending,"create_to_change_ms":change_ms,"read_code":denied.code,
        "history":negative,"outbox":0,"same_credential_account_status_valid":true}),
    )?;
    if let Some(role) = &role {
        assignment(&admin, recipient, role, false)?;
        permission(&admin, "role", role, target, "inherit")?;
    } else {
        permission(&admin, "account", &json!(recipient), target, "inherit")?;
    }
    call(&recipient_alias, "post.list", json!({"thread_id":thread}))?;
    no_revoked_for(3)?;
    let fresh = call(
        &admin,
        "post.create",
        json!({"thread_id":thread,"body":marks[3]}),
    )?;
    let final_history = wait_history(3)?;
    wait_drained(recipient)?;
    ensure(
        final_history == [1, 0, 0, 1],
        "unexpected OS history counts",
    )?;
    ensure(alias()? == recipient_alias, "selection changed")?;
    let after = call(&recipient_alias, "post.list", json!({"thread_id":thread}))?;
    let items = after["items"].as_array().ok_or("Posts absent")?;
    ensure(items.len() == count + 4, "final Post count changed")?;
    ensure(
        &items[..count]
            == before["items"]
                .as_array()
                .ok_or("prior Posts absent")?
                .as_slice(),
        "prior full Post items changed",
    )?;
    for (item, marker) in items[count..].iter().zip(marks) {
        ensure(
            item["body"] == marker && item["revision"] == 1,
            "new Post changed",
        )?;
    }
    let subscriptions = call(&recipient_alias, "conversation.subscriptions", json!({}))?;
    ensure(
        subscriptions["items"]
            .as_array()
            .ok_or("subscriptions absent")?
            .iter()
            .any(|i| i["id"] == thread),
        "subscription lost",
    )?;
    ensure(
        call(
            &admin,
            "permission.list",
            json!({"subject_type":"account","subject_id":recipient}),
        )? == permissions_before,
        "account permission not restored",
    )?;
    if let Some(role) = &role {
        let permissions = call(
            &admin,
            "permission.list",
            json!({"subject_type":"role","subject_id":role}),
        )?;
        ensure(
            permissions["items"]
                .as_array()
                .ok_or("role permissions absent")?
                .is_empty(),
            "temporary role deny remains",
        )?;
        let listing = call(&admin, "account.list", json!({}))?;
        let selected = listing["items"]
            .as_array()
            .ok_or("accounts absent")?
            .iter()
            .find(|r| r["id"] == recipient)
            .ok_or("account missing")?;
        ensure(
            !selected["role_ids"]
                .as_array()
                .ok_or("role IDs absent")?
                .contains(role),
            "temporary role remains assigned",
        )?;
    }
    ensure(
        hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == rules,
        "RULES changed",
    )?;
    runtime.stop()?;
    registration()?.remove()?;
    ensure(
        !registration()?.is_registered()?,
        "owned registration retained",
    )?;
    save(
        "result.json",
        &json!({"status":match mode(){"thread-deny"=>"QUEUED_THREAD_DENY_NOTIFICATION_PASS","role-deny"=>"QUEUED_ROLE_DENY_NOTIFICATION_PASS",_=>"QUEUED_BOARD_DENY_NOTIFICATION_PASS"},
        "runtime_pid":runtime.child.id(),"history":final_history,"pending_before":pending,"create_to_change_ms":change_ms,
        "post_ids":[positive["id"],queued["id"],hidden["id"],fresh["id"]],"prior_posts":count,"final_posts":count+4,
        "prior_full_post_items_unchanged":true,"same_credential_selection_subscription":true,"account_permissions_restored":true,
        "temporary_role_unassigned_and_permission_cleared":role.is_some(),"rules_unchanged":true,
        "normally_exited":true,"owned_registration_absent":true,
        "closed_stream_bytes":[fs::metadata(Path::new(BASE).join(format!("{}.stdout.log",mode())))?.len(),fs::metadata(Path::new(BASE).join(format!("{}.stderr.log",mode())))?.len()],
        "native_gui_observed":false,"banner_click_observed":false}),
    )?;
    println!("QUEUED_AUTHORITY_NOTIFICATION_PASS {}", mode());
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        let _ = save(
            "failure.json",
            &json!({"status":"NOT_ACCEPTED","error":error.to_string()}),
        );
        eprintln!("QUEUED_AUTHORITY_NOTIFICATION_NOT_ACCEPTED: {error}");
        std::process::exit(1);
    }
}
