//! Fixed-root normal logout notification safety. No GUI/clock/old-root mutation.
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
    "NOTIFY_LOGOUT_POSITIVE_A",
    "NOTIFY_LOGOUT_QUEUED_B",
    "NOTIFY_LOGOUT_INACTIVE_C",
    "NOTIFY_LOGOUT_NEW_LOGIN_D",
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
fn created(h: &mut GuiHost, op: &str, args: Value) -> Result<Value> {
    let v = h.create(op, args)?;
    h.acknowledge_create()?;
    Ok(v)
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
            "candidate hash changed",
        )?;
    }
    ensure(!Path::new(BASE).exists(), "existing fixed root refused")?;
    fs::create_dir(BASE)?;
    fs::create_dir(ROOT)?;
    save(
        "attempt.json",
        &json!({"scope":"normal-logout-queued-notification","gui":false,"vm":false,"elapsed":false,"publication":false}),
    )?;
    swarm_runtime::startup_preference_set(Path::new(ROOT), false)?;
    let mut command = Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
    command
        .args(["--data-dir", ROOT, "--human-start"])
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
        command.creation_flags(0x08000000);
    }
    let mut runtime = Runtime {
        child: command.spawn()?,
        admin: None,
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
    let mut h = GuiHost::open(Path::new(ROOT))?;
    let admin_password = uuid::Uuid::new_v4().simple().to_string();
    let admin =
        h.authenticate("setup", "NotifyLogoutAdmin", &admin_password)?["account"]["id"].clone();
    let admin_alias = alias()?;
    runtime.admin = Some(admin_alias.clone());
    h.backup_control(
        admin.as_str().ok_or("Admin missing")?,
        "settings.set",
        json!({"enabled":false,"interval_seconds":86400,"retention":2,"destination":null}),
    )?;
    let board = created(
        &mut h,
        "board.create",
        json!({"name":"NOTIFY_LOGOUT_BOARD"}),
    )?;
    let thread = created(
        &mut h,
        "thread.create",
        json!({"name":"NOTIFY_LOGOUT_THREAD","board_id":board["id"]}),
    )?;
    let t = thread["id"].as_str().ok_or("Thread missing")?;
    let password = uuid::Uuid::new_v4().simple().to_string();
    let recipient = h.manage_account(
        "account.create",
        json!({"name":"NotifyLogoutRecipient","password":password}),
    )?;
    let recipient_id = recipient["id"].as_str().ok_or("recipient missing")?;
    h.authenticate("login", "NotifyLogoutRecipient", &password)?;
    let recipient_alias = alias()?;
    call(
        &recipient_alias,
        "thread.subscribe",
        json!({"id":t,"subscribed":true}),
    )?;
    call(
        &recipient_alias,
        "notification.settings.set",
        json!({"enabled":true,"include_excerpt":true}),
    )?;
    // Trusted helper binds the fixed desktop executable, not a renderer-supplied path.
    // This is explicit helper registration and is not native GUI initialization proof.
    ensure(
        !registration()?.is_registered()?,
        "unexpected existing owned registration",
    )?;
    swarm_runtime::notifications::register(
        Path::new(ROOT),
        &Path::new(CANDIDATE).join("swarm-desktop.exe"),
    )?;
    let rules_hash = hash(&fs::read(Path::new(ROOT).join("RULES.md"))?);
    let positive = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":MARKERS[0]}),
    )?;
    save(
        "positive.json",
        &json!({"post_id":positive["id"],"history":wait_history(0)?,"pending":outbox(recipient_id)?}),
    )?;
    wait_drained(recipient_id)?;
    let began = Instant::now();
    let queued = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":MARKERS[1]}),
    )?;
    let queued_count = outbox(recipient_id)?;
    ensure(queued_count == 1, "queued positive was not pending")?;
    h.logout()?;
    let logout_ms = began.elapsed().as_millis();
    ensure(
        logout_ms < 1500,
        "logout missed aggregation margin; not accepted",
    )?;
    ensure(
        CredentialStore::open(&Path::new(ROOT).join("gui-state"))?
            .get("selection")?
            .is_none(),
        "logout selection persisted",
    )?;
    let denied = GuiClient::open(Path::new(ROOT))?
        .request(&recipient_alias, req("account.status", json!({})))
        .err()
        .ok_or("logged-out old session succeeded")?;
    ensure(
        denied.code == "AUTHENTICATION_REQUIRED",
        "logged-out credential unexpected code",
    )?;
    let inactive = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":MARKERS[2]}),
    )?;
    save(
        "logged-out.json",
        &json!({"queued_post_id":queued["id"],"queued_before_logout":queued_count,"create_to_logout_ms":logout_ms,"inactive_post_id":inactive["id"],"old_session_code":denied.code,"history":no_revoked_for(7)?,"pending":outbox(recipient_id)?}),
    )?;
    ensure(outbox(recipient_id)? == 0, "logged-out outbox retained")?;
    h.authenticate("login", "NotifyLogoutRecipient", &password)?;
    let fresh_alias = alias()?;
    let subscriptions = call(&fresh_alias, "conversation.subscriptions", json!({}))?;
    ensure(
        subscriptions["items"]
            .as_array()
            .ok_or("subscriptions missing")?
            .iter()
            .any(|i| i["id"] == t),
        "subscription lost",
    )?;
    no_revoked_for(3)?;
    let fresh = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":MARKERS[3]}),
    )?;
    let final_history = wait_history(3)?;
    ensure(
        final_history[1] == 0 && final_history[2] == 0,
        "stale notification revived",
    )?;
    wait_drained(recipient_id)?;
    // Alias is a stable name locator. Normal login replaces its credential;
    // a request through the same alias after login is not the old session.
    ensure(final_history == [1, 0, 0, 1], "unexpected marker counts")?;
    let posts = call(&fresh_alias, "post.list", json!({"thread_id":t}))?;
    let items = posts["items"].as_array().ok_or("Post list missing")?;
    ensure(items.len() == 4, "Post count changed")?;
    for (p, m) in items.iter().zip(MARKERS) {
        ensure(
            p["body"] == m && p["revision"] == 1,
            "Post content/revision changed",
        )?;
    }
    ensure(
        hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == rules_hash,
        "RULES changed",
    )?;
    let capability = String::from_utf8(
        CredentialStore::open(&Path::new(ROOT).join("gui-host"))?
            .get("transport-capability")?
            .ok_or("capability missing")?,
    )?;
    let safe = json!({"status":"NORMAL_LOGOUT_NOTIFICATION_PASS","recipient_id":recipient_id,"thread_id":t,"post_ids":[positive["id"],queued["id"],inactive["id"],fresh["id"]],"history":final_history,"queued_before_logout":queued_count,"logout_ms":logout_ms,"old_session_remains_refused":true,"subscription_retained":true,"posts_preserved":true,"rules_unchanged":true,"native_gui_observed":false,"banner_click_observed":false,"helper_registered_fixed_candidate":true});
    let encoded = serde_json::to_string(&safe)?;
    ensure(
        ![admin_password, password, capability]
            .iter()
            .any(|s| encoded.contains(s)),
        "safe receipt canary match",
    )?;
    drop(h);
    runtime.stop()?;
    registration()?.remove()?;
    ensure(!registration()?.is_registered()?, "registration remains")?;
    save("result.json", &safe)?;
    save(
        "closeout.json",
        &json!({"runtime_pid":runtime.child.id(),"normally_exited":true,"owned_registration_absent":true,"history_retained":true,"closed_stream_bytes":[fs::metadata(Path::new(BASE).join("runtime.stdout.log"))?.len(),fs::metadata(Path::new(BASE).join("runtime.stderr.log"))?.len()]}),
    )?;
    println!("NORMAL_LOGOUT_NOTIFICATION_PASS");
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
        eprintln!("NORMAL_LOGOUT_NOTIFICATION_NOT_ACCEPTED: {error}");
        std::process::exit(1);
    }
}
