//! Minimal selected-recipient switch safety through frozen runtime and owned OS history.
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
const BASE: &str = "E:/Swarm-Validation-Host/run-20261022-notification-switch";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261022-notification-switch/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const MARKERS: [&str; 7] = [
    "NOTIFY_SWITCH_A_POSITIVE_M",
    "NOTIFY_SWITCH_A_QUEUED_N",
    "NOTIFY_SWITCH_A_INACTIVE_O",
    "NOTIFY_SWITCH_B_POSITIVE_P",
    "NOTIFY_SWITCH_B_QUEUED_Q",
    "NOTIFY_SWITCH_B_INACTIVE_R",
    "NOTIFY_SWITCH_A_RETURNED_S",
];
#[derive(Debug)]
struct Guard(&'static str);
impl std::fmt::Display for Guard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Guard {}
fn ensure(b: bool, m: &'static str) -> Result<()> {
    if b { Ok(()) } else { Err(Box::new(Guard(m))) }
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
fn drain(account: &str) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(2);
    while outbox(account)? != 0 {
        ensure(Instant::now() < until, "outbox completion timeout")?;
        std::thread::sleep(Duration::from_millis(25));
    }
    Ok(())
}
fn history(targets: &[(&str, &str); 7]) -> Result<[usize; 7]> {
    use windows::{
        UI::Notifications::ToastNotificationManager,
        Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
        core::HSTRING,
    };
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    let result = (|| -> Result<[usize; 7]> {
        let identity = Identity::new(
            Path::new(ROOT),
            &Path::new(CANDIDATE).join("swarm-desktop.exe"),
        )?;
        let entries = ToastNotificationManager::History()?
            .GetHistoryWithId(&HSTRING::from(identity.app_id()))?;
        let mut counts = [0; 7];
        for entry in entries {
            let xml = entry.Content()?.GetXml()?.to_string();
            for (i, m) in MARKERS.iter().enumerate() {
                if xml.contains(m) {
                    counts[i] += 1;
                    ensure(
                        xml.contains(targets[i].0) && xml.contains(targets[i].1),
                        "OS activation recipient/Thread mismatch",
                    )?;
                }
            }
        }
        Ok(counts)
    })();
    unsafe {
        RoUninitialize();
    }
    result
}
fn safe_counts(targets: &[(&str, &str); 7]) -> Result<[usize; 7]> {
    let h = history(targets)?;
    ensure(
        [1, 2, 4, 5].iter().all(|i| h[*i] == 0),
        "queued/inactive notification appeared",
    )?;
    Ok(h)
}
fn wait_positive(targets: &[(&str, &str); 7], index: usize) -> Result<[usize; 7]> {
    let until = Instant::now() + Duration::from_secs(12);
    loop {
        let h = safe_counts(targets)?;
        if h[index] == 1 {
            return Ok(h);
        }
        ensure(Instant::now() < until, "positive OS delivery timeout")?;
        std::thread::sleep(Duration::from_millis(200));
    }
}
fn observe(targets: &[(&str, &str); 7], seconds: u64) -> Result<[usize; 7]> {
    let until = Instant::now() + Duration::from_secs(seconds);
    loop {
        let h = safe_counts(targets)?;
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
            ensure(Instant::now() < until, "normal stop timeout")?;
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
            "frozen binary changed",
        )?;
    }
    ensure(!Path::new(BASE).exists(), "existing fixed root refused")?;
    fs::create_dir(BASE)?;
    fs::create_dir(ROOT)?;
    save(
        "attempt.json",
        &json!({"scope":"selected-recipient-switch-safety","native_gui":false,"all_account_matrix":false,"vm":false,"elapsed":false,"publication":false}),
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
    save("runtime.json", &json!({"pid":runtime.child.id()}))?;
    let until = Instant::now() + Duration::from_secs(15);
    while GuiClient::open(Path::new(ROOT))
        .and_then(|c| c.setup_status())
        .is_err()
    {
        ensure(Instant::now() < until, "readiness timeout")?;
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut h = GuiHost::open(Path::new(ROOT))?;
    let admin_password = uuid::Uuid::new_v4().simple().to_string();
    let admin =
        h.authenticate("setup", "NotifySwitchAdmin", &admin_password)?["account"]["id"].clone();
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
        json!({"name":"NOTIFY_SWITCH_BOARD"}),
    )?;
    let ta = created(
        &mut h,
        "thread.create",
        json!({"name":"NOTIFY_SWITCH_A","board_id":board["id"]}),
    )?;
    let tb = created(
        &mut h,
        "thread.create",
        json!({"name":"NOTIFY_SWITCH_B","board_id":board["id"]}),
    )?;
    let pa = uuid::Uuid::new_v4().simple().to_string();
    let pb = uuid::Uuid::new_v4().simple().to_string();
    let a = h.manage_account(
        "account.create",
        json!({"name":"NotifySwitchA","password":pa}),
    )?;
    let b = h.manage_account(
        "account.create",
        json!({"name":"NotifySwitchB","password":pb}),
    )?;
    for (subject, thread) in [(&a, &tb), (&b, &ta)] {
        h.manage_authorization("permission.set",json!({"subject_type":"account","subject_id":subject["id"],"operation":"read","scope":"thread","target_id":thread["id"],"decision":"deny"}))?;
    }
    let aid = a["id"].as_str().ok_or("A missing")?;
    let bid = b["id"].as_str().ok_or("B missing")?;
    let at = ta["id"].as_str().ok_or("A Thread missing")?;
    let bt = tb["id"].as_str().ok_or("B Thread missing")?;
    h.authenticate("login", "NotifySwitchB", &pb)?;
    let b_alias = alias()?;
    call(
        &b_alias,
        "thread.subscribe",
        json!({"id":bt,"subscribed":true}),
    )?;
    call(
        &b_alias,
        "notification.settings.set",
        json!({"enabled":true,"include_excerpt":true}),
    )?;
    h.authenticate("login", "NotifySwitchA", &pa)?;
    let a_alias = alias()?;
    call(
        &a_alias,
        "thread.subscribe",
        json!({"id":at,"subscribed":true}),
    )?;
    call(
        &a_alias,
        "notification.settings.set",
        json!({"enabled":true,"include_excerpt":true}),
    )?;
    ensure(
        !registration()?.is_registered()?,
        "existing owned registration",
    )?;
    swarm_runtime::notifications::register(
        Path::new(ROOT),
        &Path::new(CANDIDATE).join("swarm-desktop.exe"),
    )?;
    let rules_hash = hash(&fs::read(Path::new(ROOT).join("RULES.md"))?);
    let targets = [
        (aid, at),
        (aid, at),
        (aid, at),
        (bid, bt),
        (bid, bt),
        (bid, bt),
        (aid, at),
    ];
    let mut posts = Vec::new();
    save(
        "positive-a-started.json",
        &json!({"scope":"initial-A-positive"}),
    )?;
    posts.push(call(
        &admin_alias,
        "post.create",
        json!({"thread_id":at,"body":MARKERS[0]}),
    )?);
    save(
        "positive-a.json",
        &json!({"history":wait_positive(&targets,0)?}),
    )?;
    drain(aid)?;
    save(
        "switch-a-to-b-started.json",
        &json!({"scope":"queued-A-to-B"}),
    )?;
    let began = Instant::now();
    posts.push(call(
        &admin_alias,
        "post.create",
        json!({"thread_id":at,"body":MARKERS[1]}),
    )?);
    ensure(outbox(aid)? == 1, "A queue not pending")?;
    h.authenticate("login", "NotifySwitchB", &pb)?;
    let switch_ab_ms = began.elapsed().as_millis();
    ensure(switch_ab_ms < 1500, "A-to-B missed aggregation margin")?;
    let selected_b = alias()?;
    ensure(
        call(&selected_b, "account.status", json!({}))?["id"] == b["id"],
        "B not selected",
    )?;
    ensure(
        call(&a_alias, "account.status", json!({}))?["id"] == a["id"],
        "A credential unexpectedly revoked",
    )?;
    posts.push(call(
        &admin_alias,
        "post.create",
        json!({"thread_id":at,"body":MARKERS[2]}),
    )?);
    posts.push(call(
        &admin_alias,
        "post.create",
        json!({"thread_id":bt,"body":MARKERS[3]}),
    )?);
    wait_positive(&targets, 3)?;
    drain(bid)?;
    save(
        "switched-b.json",
        &json!({"switch_ms":switch_ab_ms,"history":observe(&targets,7)?,"old_identity_valid":true,"queued_A_discarded":outbox(aid)?==0}),
    )?;
    save(
        "switch-b-to-a-started.json",
        &json!({"scope":"queued-B-to-A"}),
    )?;
    let began = Instant::now();
    posts.push(call(
        &admin_alias,
        "post.create",
        json!({"thread_id":bt,"body":MARKERS[4]}),
    )?);
    ensure(outbox(bid)? == 1, "B queue not pending")?;
    h.authenticate("login", "NotifySwitchA", &pa)?;
    let switch_ba_ms = began.elapsed().as_millis();
    ensure(switch_ba_ms < 1500, "B-to-A missed aggregation margin")?;
    let final_a = alias()?;
    ensure(
        call(&final_a, "account.status", json!({}))?["id"] == a["id"],
        "A not selected",
    )?;
    posts.push(call(
        &admin_alias,
        "post.create",
        json!({"thread_id":bt,"body":MARKERS[5]}),
    )?);
    posts.push(call(
        &admin_alias,
        "post.create",
        json!({"thread_id":at,"body":MARKERS[6]}),
    )?);
    wait_positive(&targets, 6)?;
    drain(aid)?;
    let counts = observe(&targets, 7)?;
    ensure(counts == [1, 0, 0, 1, 0, 0, 1], "final history mismatch")?;
    ensure(
        outbox(aid)? == 0 && outbox(bid)? == 0,
        "final pending queue remains",
    )?;
    for (thread, expected) in [(at, vec![0, 1, 2, 6]), (bt, vec![3, 4, 5])] {
        let listed = call(&admin_alias, "post.list", json!({"thread_id":thread}))?;
        let items = listed["items"].as_array().ok_or("Post list missing")?;
        ensure(items.len() == expected.len(), "Post count changed")?;
        for (item, index) in items.iter().zip(expected) {
            ensure(
                item["id"] == posts[index]["id"]
                    && item["body"] == MARKERS[index]
                    && item["revision"] == 1,
                "Post identity/body/revision changed",
            )?;
        }
    }
    ensure(
        hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == rules_hash,
        "RULES changed",
    )?;
    let result = json!({"status":"SELECTED_RECIPIENT_SWITCH_PASS","a_id":aid,"b_id":bid,"a_thread":at,"b_thread":bt,"post_ids":posts.iter().map(|p|p["id"].clone()).collect::<Vec<_>>(),"history":counts,"switch_a_to_b_ms":switch_ab_ms,"switch_b_to_a_ms":switch_ba_ms,"final_selected":"A","all_seven_posts_preserved":true,"old_A_credential_valid_after_B_selection":true,"raw_xml_saved":false,"activation_targets_checked":true,"native_gui_observed":false,"banner_click_observed":false});
    let encoded = serde_json::to_string(&result)?;
    ensure(
        ![admin_password, pa, pb].iter().any(|s| encoded.contains(s)),
        "safe receipt password match",
    )?;
    drop(h);
    runtime.stop()?;
    registration()?.remove()?;
    ensure(
        !registration()?.is_registered()?,
        "owned registration remains",
    )?;
    save("result.json", &result)?;
    save(
        "closeout.json",
        &json!({"runtime_pid":runtime.child.id(),"normally_exited":true,"owned_registration_absent":true,"history_retained":true,"closed_stream_bytes":[fs::metadata(Path::new(BASE).join("runtime.stdout.log"))?.len(),fs::metadata(Path::new(BASE).join("runtime.stderr.log"))?.len()]}),
    )?;
    println!("SELECTED_RECIPIENT_SWITCH_PASS");
    Ok(())
}
fn continue_unexecuted_b_to_a() -> Result<()> {
    ensure(
        Path::new(BASE).join("positive-a.json").exists()
            && Path::new(BASE).join("switch-a-to-b-started.json").exists()
            && !Path::new(BASE).join("positive-b.json").exists()
            && !Path::new(BASE).join("continuation-attempt.json").exists()
            && !Path::new(BASE).join("result.json").exists(),
        "not the unexecuted B-to-A boundary",
    )?;
    ensure(
        swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"] == "stopped",
        "previous runtime not stopped",
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
            "frozen binary changed",
        )?;
    }
    let b_alias = alias()?;
    let admin_alias = format!("account-{}", hash(b"NotifySwitchAdmin"));
    save(
        "continuation-attempt.json",
        &json!({"same_data":true,"A_to_B_not_replayed":true,"scope":"unexecuted-B-to-A-only"}),
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
    let mut runtime = Runtime {
        child: command.spawn()?,
        admin: Some(admin_alias.clone()),
    };
    save(
        "continuation-runtime.json",
        &json!({"pid":runtime.child.id()}),
    )?;
    let until = Instant::now() + Duration::from_secs(15);
    while GuiClient::open(Path::new(ROOT))
        .and_then(|c| c.setup_status())
        .is_err()
    {
        ensure(Instant::now() < until, "continuation readiness timeout")?;
        std::thread::sleep(Duration::from_millis(50));
    }
    let admin = call(&admin_alias, "account.status", json!({}))?;
    ensure(
        admin["is_admin"] == true && admin["name"] == "NotifySwitchAdmin",
        "Admin mismatch",
    )?;
    let find_one = |v: Value, name: &str| -> Result<Value> {
        let matches: Vec<_> = v["items"]
            .as_array()
            .ok_or("items missing")?
            .iter()
            .filter(|v| v["name"] == name)
            .cloned()
            .collect();
        ensure(matches.len() == 1, "identity ambiguous")?;
        Ok(matches[0].clone())
    };
    let accounts = call(&admin_alias, "account.list", json!({}))?;
    let a = find_one(accounts.clone(), "NotifySwitchA")?;
    let b = find_one(accounts, "NotifySwitchB")?;
    ensure(
        call(&b_alias, "account.status", json!({}))?["id"] == b["id"],
        "B not retained selected account",
    )?;
    let board = find_one(
        call(&admin_alias, "board.list", json!({}))?,
        "NOTIFY_SWITCH_BOARD",
    )?;
    let threads = call(&admin_alias, "thread.list", json!({"board_id":board["id"]}))?;
    let ta = find_one(threads.clone(), "NOTIFY_SWITCH_A")?;
    let tb = find_one(threads, "NOTIFY_SWITCH_B")?;
    let aid = a["id"].as_str().ok_or("A missing")?;
    let bid = b["id"].as_str().ok_or("B missing")?;
    let at = ta["id"].as_str().ok_or("TA missing")?;
    let bt = tb["id"].as_str().ok_or("TB missing")?;
    let targets = [
        (aid, at),
        (aid, at),
        (aid, at),
        (bid, bt),
        (bid, bt),
        (bid, bt),
        (aid, at),
    ];
    let old_a = call(&admin_alias, "post.list", json!({"thread_id":at}))?;
    let old_items = old_a["items"].as_array().ok_or("old items missing")?;
    ensure(
        old_items.len() == 2
            && old_items[0]["body"] == MARKERS[0]
            && old_items[1]["body"] == MARKERS[1]
            && old_items.iter().all(|p| p["revision"] == 1),
        "old A Posts changed",
    )?;
    ensure(
        call(&admin_alias, "post.list", json!({"thread_id":bt}))?["items"] == json!([]),
        "B steps already started",
    )?;
    ensure(
        outbox(aid)? == 0 && outbox(bid)? == 0 && !registration()?.is_registered()?,
        "prior closeout changed",
    )?;
    let before = safe_counts(&targets)?;
    ensure(before == [1, 0, 0, 0, 0, 0, 0], "initial history changed")?;
    save(
        "initial-independent-observation.json",
        &json!({"A_to_B_admission_not_proven":true,"queued_A_marker_absent_after_normal_stop":true,"post_count":2,"history":before,"selected_B_verified":true}),
    )?;
    // The lost generated password is not recovered. Normal synthetic Admin reset
    // prepares A's known login for the unperformed B-to-A case; B stays selected.
    let pa = uuid::Uuid::new_v4().simple().to_string();
    call(
        &admin_alias,
        "account.reset",
        json!({"id":aid,"password":pa}),
    )?;
    save(
        "preparation-reset-a.json",
        &json!({"synthetic_A_only":true,"old_password_not_recovered":true,"selected_B_unchanged":alias()?==b_alias}),
    )?;
    let rules_hash = hash(&fs::read(Path::new(ROOT).join("RULES.md"))?);
    swarm_runtime::notifications::register(
        Path::new(ROOT),
        &Path::new(CANDIDATE).join("swarm-desktop.exe"),
    )?;
    let mut posts = old_items.clone();
    posts.push(call(
        &admin_alias,
        "post.create",
        json!({"thread_id":at,"body":MARKERS[2]}),
    )?);
    posts.push(call(
        &admin_alias,
        "post.create",
        json!({"thread_id":bt,"body":MARKERS[3]}),
    )?);
    save(
        "positive-b.json",
        &json!({"history":wait_positive(&targets,3)?}),
    )?;
    drain(bid)?;
    save(
        "switch-b-to-a-started.json",
        &json!({"scope":"queued-B-to-A","completion_deadline_not_used_as_admission_proof":true}),
    )?;
    let began = Instant::now();
    posts.push(call(
        &admin_alias,
        "post.create",
        json!({"thread_id":bt,"body":MARKERS[4]}),
    )?);
    ensure(outbox(bid)? == 1, "B queue not pending")?;
    let mut h = GuiHost::open(Path::new(ROOT))?;
    h.authenticate("login", "NotifySwitchA", &pa)?;
    let elapsed = began.elapsed().as_millis();
    let a_alias = alias()?;
    ensure(
        call(&a_alias, "account.status", json!({}))?["id"] == a["id"],
        "A not selected",
    )?;
    ensure(
        call(&b_alias, "account.status", json!({}))?["id"] == b["id"],
        "B credential unexpectedly revoked",
    )?;
    posts.push(call(
        &admin_alias,
        "post.create",
        json!({"thread_id":bt,"body":MARKERS[5]}),
    )?);
    posts.push(call(
        &admin_alias,
        "post.create",
        json!({"thread_id":at,"body":MARKERS[6]}),
    )?);
    wait_positive(&targets, 6)?;
    drain(aid)?;
    let counts = observe(&targets, 7)?;
    ensure(
        counts == [1, 0, 0, 1, 0, 0, 1] && outbox(aid)? == 0 && outbox(bid)? == 0,
        "final history/queue mismatch",
    )?;
    for (thread, expected) in [(at, vec![0, 1, 2, 6]), (bt, vec![3, 4, 5])] {
        let listed = call(&admin_alias, "post.list", json!({"thread_id":thread}))?;
        let items = listed["items"].as_array().ok_or("items missing")?;
        ensure(items.len() == expected.len(), "Post count changed")?;
        for (item, index) in items.iter().zip(expected) {
            ensure(
                item["id"] == posts[index]["id"]
                    && item["body"] == MARKERS[index]
                    && item["revision"] == 1,
                "Post identity/body/revision changed",
            )?;
        }
    }
    ensure(
        hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == rules_hash,
        "RULES changed",
    )?;
    let result = json!({"status":"SELECTED_RECIPIENT_B_TO_A_QUALIFIED_PASS","a_id":aid,"b_id":bid,"a_thread":at,"b_thread":bt,"post_ids":posts.iter().map(|p|p["id"].clone()).collect::<Vec<_>>(),"history":counts,"create_to_A_login_return_ms":elapsed,"B_pending_before_login":1,"old_B_credential_valid_after_A_selection":true,"initial_A_to_B_not_accepted":true,"initial_A_to_B_not_replayed":true,"synthetic_A_preparation_reset":true,"server_selection_admission_instrumented":false,"final_selected":"A","all_seven_posts_preserved":true,"activation_targets_checked":true,"raw_xml_saved":false,"native_gui_observed":false,"banner_click_observed":false});
    ensure(
        !serde_json::to_string(&result)?.contains(&pa),
        "safe receipt password match",
    )?;
    drop(h);
    runtime.stop()?;
    registration()?.remove()?;
    ensure(
        !registration()?.is_registered()?,
        "owned registration remains",
    )?;
    save("result.json", &result)?;
    save(
        "continuation-closeout.json",
        &json!({"runtime_pid":runtime.child.id(),"normally_exited":true,"owned_registration_absent":true,"history_retained":true,"closed_stream_bytes":[fs::metadata(Path::new(BASE).join("continuation.stdout.log"))?.len(),fs::metadata(Path::new(BASE).join("continuation.stderr.log"))?.len()]}),
    )?;
    println!("SELECTED_RECIPIENT_B_TO_A_QUALIFIED_PASS");
    Ok(())
}
fn main() {
    let result = match std::env::args().nth(1).as_deref() {
        None => run(),
        Some("--continue-b-to-a") => continue_unexecuted_b_to_a(),
        _ => Err(Box::new(Guard("unknown mode")) as Box<dyn std::error::Error>),
    };
    if let Err(e) = result {
        if let Some(f) = e.downcast_ref::<swarm_core::protocol::Fault>() {
            eprintln!("Product fault code: {}", f.code);
        }
        if let Some(g) = e.downcast_ref::<Guard>() {
            eprintln!("Guard: {}", g.0);
        }
        eprintln!("Switch check failed; preserve root/receipts; no replay");
        std::process::exit(1);
    }
}
