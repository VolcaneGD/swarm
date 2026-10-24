//! Fixed-root account-disable notification safety. No GUI/clock/old-root mutation.
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
const BASE: &str = "E:/Swarm-Validation-Host/run-20261022-notification-disable";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261022-notification-disable/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const MARKERS: [&str; 4] = [
    "NOTIFY_DISABLE_POSITIVE_A",
    "NOTIFY_DISABLE_QUEUED_B",
    "NOTIFY_DISABLE_INACTIVE_C",
    "NOTIFY_DISABLE_NEW_LOGIN_D",
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
        let admin = self.admin.clone().unwrap_or(alias()?);
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
        &json!({"scope":"account-disable-queued-notification","gui":false,"vm":false,"elapsed":false,"publication":false}),
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
        h.authenticate("setup", "NotifyDisableAdmin", &admin_password)?["account"]["id"].clone();
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
        json!({"name":"NOTIFY_DISABLE_BOARD"}),
    )?;
    let thread = created(
        &mut h,
        "thread.create",
        json!({"name":"NOTIFY_DISABLE_THREAD","board_id":board["id"]}),
    )?;
    let t = thread["id"].as_str().ok_or("Thread missing")?;
    let password = uuid::Uuid::new_v4().simple().to_string();
    let recipient = h.manage_account(
        "account.create",
        json!({"name":"NotifyDisableRecipient","password":password}),
    )?;
    let recipient_id = recipient["id"].as_str().ok_or("recipient missing")?;
    h.authenticate("login", "NotifyDisableRecipient", &password)?;
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
    ensure(outbox(recipient_id)? == 0, "positive outbox not drained")?;
    let began = Instant::now();
    let queued = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":MARKERS[1]}),
    )?;
    let queued_count = outbox(recipient_id)?;
    ensure(queued_count == 1, "queued positive was not pending")?;
    call(
        &admin_alias,
        "account.disable",
        json!({"id":recipient_id,"disabled":true}),
    )?;
    let disable_ms = began.elapsed().as_millis();
    ensure(
        disable_ms < 1500,
        "disable missed aggregation margin; not accepted",
    )?;
    let denied = GuiClient::open(Path::new(ROOT))?
        .request(&recipient_alias, req("account.status", json!({})))
        .err()
        .ok_or("disabled old session succeeded")?;
    ensure(
        denied.code == "AUTHENTICATION_REQUIRED",
        "disabled credential unexpected code",
    )?;
    let inactive = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":MARKERS[2]}),
    )?;
    save(
        "disabled.json",
        &json!({"queued_post_id":queued["id"],"queued_before_disable":queued_count,"create_to_disable_ms":disable_ms,"inactive_post_id":inactive["id"],"old_session_code":denied.code,"history":no_revoked_for(7)?,"pending":outbox(recipient_id)?}),
    )?;
    ensure(outbox(recipient_id)? == 0, "disabled outbox retained")?;
    call(
        &admin_alias,
        "account.disable",
        json!({"id":recipient_id,"disabled":false}),
    )?;
    let denied = GuiClient::open(Path::new(ROOT))?
        .request(&recipient_alias, req("account.status", json!({})))
        .err()
        .ok_or("reenabled old session revived")?;
    ensure(
        denied.code == "AUTHENTICATION_REQUIRED",
        "reenabled credential unexpected code",
    )?;
    h.authenticate("login", "NotifyDisableRecipient", &password)?;
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
    ensure(outbox(recipient_id)? == 0, "fresh notification not drained")?;
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
    let safe = json!({"status":"ACCOUNT_DISABLE_NOTIFICATION_PASS","recipient_id":recipient_id,"thread_id":t,"post_ids":[positive["id"],queued["id"],inactive["id"],fresh["id"]],"history":final_history,"queued_before_disable":queued_count,"disable_ms":disable_ms,"old_session_remains_refused":true,"subscription_retained":true,"posts_preserved":true,"rules_unchanged":true,"native_gui_observed":false,"banner_click_observed":false,"helper_registered_fixed_candidate":true});
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
    println!("ACCOUNT_DISABLE_NOTIFICATION_PASS");
    Ok(())
}
fn continue_settings() -> Result<()> {
    let prior: Value = serde_json::from_slice(&fs::read(Path::new(BASE).join("result.json"))?)?;
    ensure(
        prior["status"] == "ACCOUNT_DISABLE_NOTIFICATION_PASS",
        "prior account-disable not complete",
    )?;
    ensure(
        !Path::new(BASE).join("settings-attempt.json").exists(),
        "settings continuation consumed",
    )?;
    ensure(
        swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"] == "stopped",
        "prior runtime not stopped",
    )?;
    ensure(
        hash(&fs::read(Path::new(CANDIDATE).join("swarm-runtime.exe"))?)
            == "0cf3ecb753c20830be7edf2c3630b3cc07aac90655d963b1097374f8b2b24210",
        "frozen runtime changed",
    )?;
    let recipient_alias = alias()?;
    let admin_alias = format!("account-{}", hash(b"NotifyDisableAdmin"));
    save(
        "settings-attempt.json",
        &json!({"same_data":true,"account_disable_not_repeated":true,"scope":"notification-setting-disable-queued"}),
    )?;
    let mut command = Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
    command
        .args(["--data-dir", ROOT, "--human-start"])
        .stdin(Stdio::null())
        .stdout(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join("settings.stdout.log"))?,
        )
        .stderr(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join("settings.stderr.log"))?,
        );
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut runtime = Runtime {
        child: command.spawn()?,
        admin: Some(admin_alias.clone()),
    };
    save("settings-runtime.json", &json!({"pid":runtime.child.id()}))?;
    let until = Instant::now() + Duration::from_secs(15);
    while GuiClient::open(Path::new(ROOT))
        .and_then(|c| c.setup_status())
        .is_err()
    {
        ensure(Instant::now() < until, "settings readiness deadline")?;
        std::thread::sleep(Duration::from_millis(50));
    }
    let account = call(&recipient_alias, "account.status", json!({}))?;
    ensure(
        account["id"] == prior["recipient_id"],
        "retained recipient mismatch",
    )?;
    let admin = call(&admin_alias, "account.status", json!({}))?;
    ensure(
        admin["is_admin"] == true && admin["name"] == "NotifyDisableAdmin",
        "retained Admin mismatch",
    )?;
    let recipient_id = prior["recipient_id"].as_str().ok_or("recipient missing")?;
    let t = prior["thread_id"].as_str().ok_or("Thread missing")?;
    let settings = call(&recipient_alias, "notification.settings.get", json!({}))?;
    ensure(
        settings["enabled"] == true && settings["include_excerpt"] == true,
        "settings changed",
    )?;
    ensure(
        outbox(recipient_id)? == 0 && !registration()?.is_registered()?,
        "prior closeout changed",
    )?;
    let previous = call(&recipient_alias, "post.list", json!({"thread_id":t}))?;
    let previous_items = previous["items"].as_array().ok_or("prior posts missing")?;
    ensure(previous_items.len() == 4, "prior Post count changed")?;
    for (p, id) in previous_items
        .iter()
        .zip(prior["post_ids"].as_array().ok_or("prior ids missing")?)
    {
        ensure(
            p["id"] == *id && p["revision"] == 1,
            "prior Post identity changed",
        )?;
    }
    let rules_hash = hash(&fs::read(Path::new(ROOT).join("RULES.md"))?);
    swarm_runtime::notifications::register(
        Path::new(ROOT),
        &Path::new(CANDIDATE).join("swarm-desktop.exe"),
    )?;
    let markers = [
        "NOTIFY_SETTINGS_POSITIVE_E",
        "NOTIFY_SETTINGS_QUEUED_F",
        "NOTIFY_SETTINGS_DISABLED_G",
        "NOTIFY_SETTINGS_REENABLED_H",
    ];
    let wait_for = |index: usize| -> Result<[usize; 4]> {
        let until = Instant::now() + Duration::from_secs(12);
        loop {
            let h = history_for(&markers)?;
            ensure(h[1] == 0 && h[2] == 0, "disabled-setting marker reached OS")?;
            if h[index] > 0 {
                return Ok(h);
            }
            ensure(Instant::now() < until, "settings positive history absent")?;
            std::thread::sleep(Duration::from_millis(200));
        }
    };
    let negative = |seconds: u64| -> Result<[usize; 4]> {
        let until = Instant::now() + Duration::from_secs(seconds);
        loop {
            let h = history_for(&markers)?;
            ensure(h[1] == 0 && h[2] == 0, "disabled-setting marker reached OS")?;
            if Instant::now() >= until {
                return Ok(h);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    };
    let mut post_ids = Vec::new();
    let post = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":markers[0]}),
    )?;
    post_ids.push(post["id"].clone());
    save(
        "settings-positive.json",
        &json!({"post_id":post["id"],"history":wait_for(0)?}),
    )?;
    ensure(outbox(recipient_id)? == 0, "settings positive pending")?;
    let began = Instant::now();
    let post = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":markers[1]}),
    )?;
    post_ids.push(post["id"].clone());
    let queued = outbox(recipient_id)?;
    ensure(queued == 1, "settings queued Post absent")?;
    call(
        &recipient_alias,
        "notification.settings.set",
        json!({"enabled":false,"include_excerpt":true}),
    )?;
    let disabled_ms = began.elapsed().as_millis();
    ensure(
        disabled_ms < 1500,
        "settings disable missed aggregation margin",
    )?;
    let post = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":markers[2]}),
    )?;
    post_ids.push(post["id"].clone());
    save(
        "settings-disabled.json",
        &json!({"queued_before_disable":queued,"create_to_disable_ms":disabled_ms,"history":negative(7)?,"pending":outbox(recipient_id)?}),
    )?;
    ensure(
        outbox(recipient_id)? == 0,
        "settings disabled queue remains",
    )?;
    call(
        &recipient_alias,
        "notification.settings.set",
        json!({"enabled":true,"include_excerpt":true}),
    )?;
    negative(3)?;
    let post = call(
        &admin_alias,
        "post.create",
        json!({"thread_id":t,"body":markers[3]}),
    )?;
    post_ids.push(post["id"].clone());
    let final_history = wait_for(3)?;
    ensure(
        outbox(recipient_id)? == 0,
        "settings reenabled positive queue remains",
    )?;
    let posts = call(&recipient_alias, "post.list", json!({"thread_id":t}))?;
    let items = posts["items"].as_array().ok_or("final posts missing")?;
    ensure(
        items.len() == 8 && items[..4] == *previous_items,
        "previous Posts changed",
    )?;
    for (p, m) in items[4..].iter().zip(markers) {
        ensure(
            p["body"] == m && p["revision"] == 1,
            "new Post content/revision changed",
        )?;
    }
    ensure(
        hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == rules_hash,
        "RULES changed",
    )?;
    let subscriptions = call(&recipient_alias, "conversation.subscriptions", json!({}))?;
    ensure(
        subscriptions["items"]
            .as_array()
            .ok_or("subscriptions missing")?
            .iter()
            .any(|i| i["id"] == t),
        "subscription changed",
    )?;
    runtime.stop()?;
    registration()?.remove()?;
    ensure(
        !registration()?.is_registered()?,
        "owned registration remains",
    )?;
    save(
        "settings-result.json",
        &json!({"status":"SETTINGS_DISABLE_NOTIFICATION_PASS","same_recipient":true,"new_post_ids":post_ids,"previous_posts_unchanged":true,"history":final_history,"queued_before_disable":queued,"disable_ms":disabled_ms,"same_credential_retained":true,"subscription_retained":true,"normally_exited":true,"runtime_pid":runtime.child.id(),"owned_registration_absent":true,"closed_stream_bytes":[fs::metadata(Path::new(BASE).join("settings.stdout.log"))?.len(),fs::metadata(Path::new(BASE).join("settings.stderr.log"))?.len()],"native_gui_observed":false,"banner_click_observed":false}),
    )?;
    println!("SETTINGS_DISABLE_NOTIFICATION_PASS");
    Ok(())
}
/// Owned history only. XML remains in memory and is never printed or persisted.
fn toast_records() -> Result<Vec<(String, String)>> {
    use windows::{
        UI::Notifications::ToastNotificationManager,
        Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
        core::HSTRING,
    };
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    let result = (|| -> Result<Vec<(String, String)>> {
        let identity = Identity::new(
            Path::new(ROOT),
            &Path::new(CANDIDATE).join("swarm-desktop.exe"),
        )?;
        let entries = ToastNotificationManager::History()?
            .GetHistoryWithId(&HSTRING::from(identity.app_id()))?;
        let mut records = Vec::new();
        for entry in entries {
            records.push((
                format!("{}:{}", entry.Group()?, entry.Tag()?),
                entry.Content()?.GetXml()?.to_string(),
            ));
        }
        Ok(records)
    })();
    unsafe {
        RoUninitialize();
    }
    result
}
fn next_toast(previous: &[(String, String)]) -> Result<(String, String)> {
    let until = Instant::now() + Duration::from_secs(12);
    loop {
        let current = toast_records()?;
        let mut new = current
            .into_iter()
            .filter(|(key, _)| !previous.iter().any(|(old, _)| old == key));
        if let Some(record) = new.next() {
            ensure(
                new.next().is_none(),
                "multiple new notifications; attribution ambiguous",
            )?;
            return Ok(record);
        }
        ensure(Instant::now() < until, "new owned notification missing")?;
        std::thread::sleep(Duration::from_millis(200));
    }
}
fn continue_excerpt(start_at: usize) -> Result<()> {
    let prior: Value = serde_json::from_slice(&fs::read(Path::new(BASE).join("result.json"))?)?;
    let settings_prior: Value =
        serde_json::from_slice(&fs::read(Path::new(BASE).join("settings-result.json"))?)?;
    ensure(
        prior["status"] == "ACCOUNT_DISABLE_NOTIFICATION_PASS"
            && settings_prior["status"] == "SETTINGS_DISABLE_NOTIFICATION_PASS",
        "prior joins not complete",
    )?;
    ensure(start_at <= 1, "invalid suffix")?;
    if start_at == 0 {
        ensure(
            !Path::new(BASE).join("excerpt-attempt.json").exists(),
            "excerpt continuation consumed",
        )?;
    } else {
        ensure(
            Path::new(BASE).join("excerpt-attempt.json").exists()
                && !Path::new(BASE).join("excerpt-suffix-attempt.json").exists()
                && !Path::new(BASE).join("excerpt-case-0.json").exists()
                && !Path::new(BASE).join("excerpt-case-1.json").exists()
                && !Path::new(BASE).join("excerpt-result.json").exists(),
            "not the initial first-case boundary",
        )?;
    }
    ensure(
        swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"] == "stopped",
        "prior runtime not stopped",
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
            "fixed binary changed",
        )?;
    }
    let recipient_alias = alias()?;
    let admin_alias = format!("account-{}", hash(b"NotifyDisableAdmin"));
    let prefix = if start_at == 0 {
        "excerpt"
    } else {
        "excerpt-suffix"
    };
    save(
        &format!("{prefix}-attempt.json"),
        &json!({"same_data":true,"account_settings_disable_not_repeated":true,"scope":"excerpt-off-on-and-queued-setting-recheck"}),
    )?;
    let mut command = Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
    command
        .args(["--data-dir", ROOT, "--human-start"])
        .stdin(Stdio::null())
        .stdout(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join(format!("{prefix}.stdout.log")))?,
        )
        .stderr(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join(format!("{prefix}.stderr.log")))?,
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
        &format!("{prefix}-runtime.json"),
        &json!({"pid":runtime.child.id()}),
    )?;
    let until = Instant::now() + Duration::from_secs(15);
    while GuiClient::open(Path::new(ROOT))
        .and_then(|c| c.setup_status())
        .is_err()
    {
        ensure(Instant::now() < until, "excerpt readiness deadline")?;
        std::thread::sleep(Duration::from_millis(50));
    }
    let account = call(&recipient_alias, "account.status", json!({}))?;
    ensure(account["id"] == prior["recipient_id"], "recipient mismatch")?;
    let admin = call(&admin_alias, "account.status", json!({}))?;
    ensure(
        admin["is_admin"] == true && admin["name"] == "NotifyDisableAdmin",
        "Admin mismatch",
    )?;
    let recipient_id = prior["recipient_id"].as_str().ok_or("recipient missing")?;
    let t = prior["thread_id"].as_str().ok_or("Thread missing")?;
    let current_settings = call(&recipient_alias, "notification.settings.get", json!({}))?;
    let original_settings = json!({"enabled":true,"include_excerpt":true});
    ensure(
        current_settings["enabled"] == true
            && current_settings["include_excerpt"] == (start_at == 0),
        "settings changed",
    )?;
    ensure(
        outbox(recipient_id)? == 0 && !registration()?.is_registered()?,
        "prior closeout changed",
    )?;
    let previous = call(&recipient_alias, "post.list", json!({"thread_id":t}))?;
    let all_existing = previous["items"].as_array().ok_or("old items missing")?;
    ensure(all_existing.len() == 8 + start_at, "old Post count changed")?;
    let old_items = &all_existing[..8];
    let old_ids: Vec<_> = prior["post_ids"]
        .as_array()
        .ok_or("old ids missing")?
        .iter()
        .chain(
            settings_prior["new_post_ids"]
                .as_array()
                .ok_or("settings ids missing")?,
        )
        .collect();
    for (p, id) in old_items.iter().zip(old_ids) {
        ensure(
            p["id"] == *id && p["revision"] == 1,
            "old Post identity changed",
        )?;
    }
    let rules_hash = hash(&fs::read(Path::new(ROOT).join("RULES.md"))?);
    swarm_runtime::notifications::register(
        Path::new(ROOT),
        &Path::new(CANDIDATE).join("swarm-desktop.exe"),
    )?;
    let bodies = [
        "NOTIFY_EXCERPT_OFF_I_BODY".to_owned(),
        format!("NOTIFY_EXCERPT_ON_J_{}PRIVATE_TAIL_J", "界".repeat(260)),
        "NOTIFY_EXCERPT_QUEUED_ON_TO_OFF_K_BODY".into(),
        "NOTIFY_EXCERPT_QUEUED_OFF_TO_ON_L_BODY".into(),
    ];
    let mut ids = Vec::new();
    let mut receipts = Vec::new();
    if start_at == 1 {
        ensure(
            all_existing[8]["body"] == bodies[0] && all_existing[8]["revision"] == 1,
            "first Post mismatch",
        )?;
        let records = toast_records()?;
        let known = [
            MARKERS[0],
            MARKERS[3],
            "NOTIFY_SETTINGS_POSITIVE_E",
            "NOTIFY_SETTINGS_REENABLED_H",
        ];
        ensure(
            records.len() == 5
                && known
                    .iter()
                    .all(|m| records.iter().filter(|(_, xml)| xml.contains(m)).count() == 1),
            "owned history attribution changed",
        )?;
        let new: Vec<_> = records
            .iter()
            .filter(|(_, xml)| !known.iter().any(|m| xml.contains(m)))
            .collect();
        ensure(
            new.len() == 1 && !new[0].1.contains(&bodies[0]),
            "first OFF toast not independently confirmed",
        )?;
        let recovered = json!({"case":0,"post_id":all_existing[8]["id"],"new_owned_toasts":1,"tag_group_sha256":hash(new[0].0.as_bytes()),"marker_present":false,"raw_xml_saved":false,"independent_after_stop_observation":true,"initial_failure_reason_not_captured":true});
        save("excerpt-case-0-recovered.json", &recovered)?;
        receipts.push(recovered);
        ids.push(all_existing[8]["id"].clone());
    }
    for (index, body) in bodies.iter().enumerate().skip(start_at) {
        save(
            &format!("excerpt-started-{index}.json"),
            &json!({"case":index,"prior_outbox":outbox(recipient_id)?}),
        )?;
        ensure(outbox(recipient_id)? == 0, "prior case pending")?;
        let include_at_create = index == 1 || index == 2;
        call(
            &recipient_alias,
            "notification.settings.set",
            json!({"enabled":true,"include_excerpt":include_at_create}),
        )?;
        let before = toast_records()?;
        let began = Instant::now();
        let post = call(
            &admin_alias,
            "post.create",
            json!({"thread_id":t,"body":body}),
        )?;
        ids.push(post["id"].clone());
        let mut changed_ms = None;
        if index >= 2 {
            ensure(outbox(recipient_id)? == 1, "queued excerpt event missing")?;
            call(
                &recipient_alias,
                "notification.settings.set",
                json!({"enabled":true,"include_excerpt":!include_at_create}),
            )?;
            let elapsed = began.elapsed().as_millis();
            ensure(elapsed < 1500, "excerpt change missed aggregation margin")?;
            changed_ms = Some(elapsed);
        }
        let (key, xml) = next_toast(&before)?;
        let should_include = index == 1 || index == 3;
        let marker = body
            .split('_')
            .take(if index == 1 { 4 } else { 5 })
            .collect::<Vec<_>>()
            .join("_");
        ensure(
            xml.contains(&marker) == should_include,
            "excerpt marker visibility mismatch",
        )?;
        let prefix = body.chars().take(240).collect::<String>();
        if should_include {
            ensure(xml.contains(&prefix), "expected excerpt prefix absent")?;
        } else {
            ensure(!xml.contains(body), "body exposed with excerpt off")?;
        }
        let bounded =
            index != 1 || (xml.contains(&format!("{prefix}…")) && !xml.contains("PRIVATE_TAIL_J"));
        ensure(bounded, "long excerpt not bounded")?;
        let until = Instant::now() + Duration::from_secs(2);
        while outbox(recipient_id)? != 0 {
            ensure(Instant::now() < until, "new toast not acknowledged")?;
            std::thread::sleep(Duration::from_millis(25));
        }
        let receipt = json!({"case":index,"post_id":post["id"],"new_owned_toasts":1,"tag_group_sha256":hash(key.as_bytes()),"include_at_create":include_at_create,"include_at_delivery":should_include,"marker_present":xml.contains(&marker),"unicode_bound_observed":index==1,"changed_after_queued_ms":changed_ms,"raw_xml_saved":false});
        save(&format!("excerpt-case-{index}.json"), &receipt)?;
        receipts.push(receipt);
    }
    let posts = call(&recipient_alias, "post.list", json!({"thread_id":t}))?;
    let items = posts["items"].as_array().ok_or("final items missing")?;
    ensure(
        items.len() == 12 && items[..8] == *old_items,
        "old Posts changed",
    )?;
    for (p, body) in items[8..].iter().zip(&bodies) {
        ensure(p["body"] == *body && p["revision"] == 1, "new Post changed")?;
    }
    call(
        &recipient_alias,
        "notification.settings.set",
        original_settings,
    )?;
    ensure(
        hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == rules_hash,
        "RULES changed",
    )?;
    runtime.stop()?;
    registration()?.remove()?;
    ensure(
        !registration()?.is_registered()?,
        "owned registration remains",
    )?;
    save(
        "excerpt-result.json",
        &json!({"status":"NOTIFICATION_EXCERPT_PASS","recipient_id":recipient_id,"thread_id":t,"new_post_ids":ids,"cases":receipts,"old_eight_posts_unchanged":true,"all_twelve_posts_revision1":true,"settings_restored":true,"same_credential":true,"rules_unchanged":true,"normally_exited":true,"runtime_pid":runtime.child.id(),"owned_registration_absent":true,"closed_stream_bytes":[fs::metadata(Path::new(BASE).join(format!("{prefix}.stdout.log")))?.len(),fs::metadata(Path::new(BASE).join(format!("{prefix}.stderr.log")))?.len()],"native_gui_observed":false,"banner_click_observed":false,"raw_xml_saved":false,"initial_case_not_replayed":start_at==1}),
    )?;
    println!("NOTIFICATION_EXCERPT_PASS");
    Ok(())
}
fn main() {
    let result = match std::env::args().nth(1).as_deref() {
        None => run(),
        Some("--continue-settings") => continue_settings(),
        Some("--continue-excerpt") => continue_excerpt(0),
        Some("--continue-excerpt-suffix") => continue_excerpt(1),
        Some("--excerpt-readonly") => (|| -> Result<()> {
            let db = rusqlite::Connection::open_with_flags(
                Path::new(ROOT).join("swarm.sqlite3"),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            let count: i64 = db.query_row("SELECT COUNT(*) FROM posts", [], |r| r.get(0))?;
            let pending: i64 =
                db.query_row("SELECT COUNT(*) FROM notification_outbox", [], |r| r.get(0))?;
            let records = toast_records()?;
            let markers = [
                "NOTIFY_EXCERPT_OFF_I_BODY",
                "NOTIFY_EXCERPT_ON_J_",
                "NOTIFY_EXCERPT_QUEUED_ON_TO_OFF_K_BODY",
                "NOTIFY_EXCERPT_QUEUED_OFF_TO_ON_L_BODY",
            ];
            let marker_counts: Vec<_> = markers
                .iter()
                .map(|m| records.iter().filter(|(_, xml)| xml.contains(m)).count())
                .collect();
            let settings: Vec<_> = db
                .prepare("SELECT enabled,include_excerpt FROM notification_settings")?
                .query_map([], |r| Ok((r.get::<_, bool>(0)?, r.get::<_, bool>(1)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            println!(
                "{}",
                json!({"readonly":true,"post_count":count,"outbox":pending,"owned_toasts":records.len(),"excerpt_marker_counts":marker_counts,"settings":settings,"registration_present":registration()?.is_registered()?,"runtime_state":swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"]})
            );
            Ok(())
        })(),
        _ => Err("unknown mode".into()),
    };
    if let Err(e) = result {
        if let Some(f) = e.downcast_ref::<swarm_core::protocol::Fault>() {
            eprintln!("Product fault code: {}", f.code);
        }
        eprintln!("Notification disable failed; preserve root and receipts; no replay");
        std::process::exit(1);
    }
}
