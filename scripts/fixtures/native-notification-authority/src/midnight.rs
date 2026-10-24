//! Fixed, isolated real-midnight identity/ownership/direct-response observation.
//! Plan never opens stores. Prepare and observe are exclusive, bounded phases.
use chrono::{Datelike, Local};
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
const BASE: &str = "E:/Swarm-Validation-Host/run-20261023-midnight-identity";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261023-midnight-identity/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const BEFORE: &str = "MIDNIGHT_IDENTITY_BEFORE_REPLY";
const AFTER: &str = "MIDNIGHT_IDENTITY_AFTER_REPLY";
const TARGET: &str = "MIDNIGHT_IDENTITY_OWNED_TARGET";

fn require(ok: bool, message: &'static str) -> Result<()> {
    if ok { Ok(()) } else { Err(message.into()) }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn save(name: &str, value: &Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(BASE).join(name))?;
    writeln!(f, "{value}")?;
    Ok(())
}
fn read(name: &str) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(
        Path::new(BASE).join(name),
    )?)?)
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
fn selected() -> Result<String> {
    let value: Value = serde_json::from_slice(
        &CredentialStore::open(&Path::new(ROOT).join("gui-state"))?
            .get("selection")?
            .ok_or("selection absent")?,
    )?;
    Ok(value["alias"].as_str().ok_or("alias absent")?.into())
}
fn create(host: &mut GuiHost, op: &str, args: Value) -> Result<Value> {
    let value = host.create(op, args)?;
    host.acknowledge_create()?;
    Ok(value)
}
fn registration() -> Result<Registration> {
    Ok(Registration::new(Identity::new(
        Path::new(ROOT),
        &Path::new(CANDIDATE).join("swarm-desktop.exe"),
    )?)?)
}
fn candidate() -> Result<()> {
    for fixed in ["E:/Swarm-Validation-Host", CANDIDATE] {
        require(
            fs::canonicalize(fixed)?
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .replace('\\', "/")
                .eq_ignore_ascii_case(fixed),
            "redirected fixed directory",
        )?;
    }
    if Path::new(ROOT).exists() {
        require(
            fs::canonicalize(ROOT)?
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .replace('\\', "/")
                .eq_ignore_ascii_case(ROOT),
            "redirected Data directory",
        )?;
    }
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
        require(
            hash(&fs::read(Path::new(CANDIDATE).join(name))?) == expected,
            "candidate changed",
        )?;
    }
    Ok(())
}
fn original_fields(post: &Value) -> Value {
    json!({"id":post["id"],"no":post["no"],"revision":post["revision"],"reply_to":post["reply_to"],"author":post["author"],"body_sha256":hash(post["body"].as_str().unwrap_or("").as_bytes())})
}
fn valid_display(post: &Value) -> Result<&str> {
    let display = post["author"]["display_id"]
        .as_str()
        .ok_or("display id absent")?;
    require(
        display.len() == 10 && display.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid USER display id",
    )?;
    require(
        post["author"]["model_name"] == "USER",
        "not a USER snapshot",
    )?;
    Ok(display)
}
fn pending(account: &str) -> Result<i64> {
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
fn history() -> Result<[usize; 2]> {
    use windows::{
        UI::Notifications::ToastNotificationManager,
        Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
        core::HSTRING,
    };
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    let result = (|| -> Result<[usize; 2]> {
        let identity = Identity::new(
            Path::new(ROOT),
            &Path::new(CANDIDATE).join("swarm-desktop.exe"),
        )?;
        let entries = ToastNotificationManager::History()?
            .GetHistoryWithId(&HSTRING::from(identity.app_id()))?;
        let mut counts = [0; 2];
        for entry in entries {
            let xml = entry.Content()?.GetXml()?.to_string();
            for (i, marker) in [BEFORE, AFTER].iter().enumerate() {
                counts[i] += usize::from(xml.contains(marker));
            }
        }
        Ok(counts)
    })();
    unsafe {
        RoUninitialize();
    }
    result
}
fn delivered(expected: [usize; 2], account: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if history()? == expected && pending(account)? == 0 {
            return Ok(());
        }
        require(
            Instant::now() < deadline,
            "OS delivery/control or drain not established",
        )?;
        std::thread::sleep(Duration::from_millis(100));
    }
}
struct Runtime {
    child: Child,
    admin: Option<String>,
}
impl Runtime {
    fn start(phase: &str, admin: Option<String>) -> Result<Self> {
        require(
            GuiClient::open(Path::new(ROOT))
                .and_then(|c| c.setup_status())
                .is_err(),
            "existing runtime refused",
        )?;
        let mut command = Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
        command
            .args(["--data-dir", ROOT, "--human-start"])
            .stdin(Stdio::null());
        for (suffix, stream) in [("stdout", true), ("stderr", false)] {
            let f = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(Path::new(BASE).join(format!("{phase}.{suffix}.log")))?;
            if stream {
                command.stdout(f);
            } else {
                command.stderr(f);
            }
        }
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut runtime = Self {
            child: command.spawn()?,
            admin,
        };
        save(
            &format!("{phase}-runtime.json"),
            &json!({"pid":runtime.child.id(),"candidate":CANDIDATE}),
        )?;
        let deadline = Instant::now() + Duration::from_secs(20);
        while GuiClient::open(Path::new(ROOT))
            .and_then(|c| c.setup_status())
            .is_err()
        {
            require(
                runtime.child.try_wait()?.is_none(),
                "runtime exited before ready",
            )?;
            require(Instant::now() < deadline, "runtime readiness deadline")?;
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(runtime)
    }
    fn stop(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait()? {
            require(status.success(), "runtime nonzero exit")?;
            return Ok(());
        }
        let admin = match &self.admin {
            Some(a) => a.clone(),
            None => selected()?,
        };
        GuiClient::open(Path::new(ROOT))?.stop(&admin)?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(status) = self.child.try_wait()? {
                require(status.success(), "runtime nonzero exit")?;
                return Ok(());
            }
            require(
                Instant::now() < deadline,
                "normal stop timeout; preserve process",
            )?;
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
fn close(runtime: &mut Runtime, phase: &str) -> Result<()> {
    runtime.stop()?;
    registration()?.remove()?;
    require(
        !registration()?.is_registered()?,
        "owned registration retained",
    )?;
    save(
        &format!("{phase}-closeout.json"),
        &json!({"runtime_pid":runtime.child.id(),"normal_exit":true,"owned_registration_absent":true,"streams":[fs::metadata(Path::new(BASE).join(format!("{phase}.stdout.log")))?.len(),fs::metadata(Path::new(BASE).join(format!("{phase}.stderr.log")))?.len()]}),
    )
}
fn clock_date() -> Result<String> {
    let at = Local::now();
    require(
        at.offset().local_minus_utc() == 9 * 3600,
        "expected Windows JST timezone",
    )?;
    Ok(format!(
        "{:04}-{:02}-{:02}",
        at.year(),
        at.month(),
        at.day()
    ))
}
fn observe_allowed(date: &str) -> bool {
    date == "2026-10-24"
}
fn within_first_day(now: i64, prepared: i64) -> bool {
    prepared > 0 && now >= prepared && now - prepared < 86_400
}
fn prepare() -> Result<()> {
    require(
        clock_date()? == "2026-10-23",
        "preparation requires real October 23 JST",
    )?;
    candidate()?;
    require(
        !Path::new(BASE).exists(),
        "existing baseline refused; no reseed",
    )?;
    fs::create_dir(BASE)?;
    fs::create_dir(ROOT)?;
    save(
        "prepare-attempt.json",
        &json!({"date":clock_date()?,"scope":"real-midnight identity ownership direct-response","clock_changed":false}),
    )?;
    swarm_runtime::startup_preference_set(Path::new(ROOT), false)?;
    let mut runtime = Runtime::start("prepare", None)?;
    let work = (|| -> Result<Value> {
        let mut host = GuiHost::open(Path::new(ROOT))?;
        let password = uuid::Uuid::new_v4().simple().to_string();
        let admin =
            host.authenticate("setup", "MidnightAdmin", &password)?["account"]["id"].clone();
        let admin_alias = selected()?;
        runtime.admin = Some(admin_alias.clone());
        host.backup_control(
            admin.as_str().ok_or("admin absent")?,
            "settings.set",
            json!({"enabled":false,"interval_seconds":86400,"retention":2,"destination":null}),
        )?;
        let board = create(
            &mut host,
            "board.create",
            json!({"name":"MIDNIGHT_IDENTITY_BOARD"}),
        )?;
        let thread = create(
            &mut host,
            "thread.create",
            json!({"board_id":board["id"],"name":"MIDNIGHT_IDENTITY_THREAD"}),
        )?["id"]
            .clone();
        let password2 = uuid::Uuid::new_v4().simple().to_string();
        let account = host.manage_account(
            "account.create",
            json!({"name":"MidnightRecipient","password":password2}),
        )?["id"]
            .clone();
        host.authenticate("login", "MidnightRecipient", &password2)?;
        let recipient_alias = selected()?;
        let target = create(
            &mut host,
            "post.create",
            json!({"thread_id":thread,"body":TARGET}),
        )?;
        require(
            target["author"]["id"] == account
                && target["no"] == 1
                && target["revision"] == 1
                && target["body"] == TARGET,
            "target immutable owner mismatch",
        )?;
        valid_display(&target)?;
        require(
            call(&recipient_alias, "conversation.subscriptions", json!({}))?["items"]
                .as_array()
                .ok_or("subscriptions missing")?
                .is_empty(),
            "unexpected subscription",
        )?;
        call(
            &recipient_alias,
            "notification.settings.set",
            json!({"enabled":true,"include_excerpt":true}),
        )?;
        require(
            !registration()?.is_registered()? && history()? == [0, 0],
            "owned identity not fresh",
        )?;
        swarm_runtime::notifications::register(
            Path::new(ROOT),
            &Path::new(CANDIDATE).join("swarm-desktop.exe"),
        )?;
        let reply = call(
            &admin_alias,
            "response.create",
            json!({"thread_id":thread,"to":1,"body":BEFORE}),
        )?;
        require(
            reply["no"] == 2
                && reply["reply_to"] == 1
                && reply["body"] == BEFORE
                && reply["author"]["id"] == admin,
            "preparation Response mismatch",
        )?;
        delivered([1, 0], account.as_str().ok_or("recipient absent")?)?;
        require(
            clock_date()? == "2026-10-23",
            "preparation crossed date; preserve uncertain evidence",
        )?;
        let prepared_at = chrono::DateTime::parse_from_rfc3339(
            target["created_at"].as_str().ok_or("target time missing")?,
        )?
        .timestamp();
        let baseline = json!({"root":ROOT,"date":"2026-10-23","prepared_unix_seconds":prepared_at,"not_before":"2026-10-24T00:00:00+09:00","admin_alias":admin_alias,"recipient_alias":recipient_alias,"recipient_id":account,"thread_id":thread,"target":original_fields(&target),"reply":original_fields(&reply),"history":[1,0],"rules_sha256":hash(&fs::read(Path::new(ROOT).join("RULES.md"))?)});
        let text = serde_json::to_string(&baseline)?;
        require(
            !text.contains(&password) && !text.contains(&password2),
            "receipt secret canary",
        )?;
        Ok(baseline)
    })();
    let cleanup = close(&mut runtime, "prepare");
    let baseline = work?;
    cleanup?;
    save("baseline.json", &baseline)?;
    save(
        "baseline-digest.json",
        &json!({"sha256":hash(&fs::read(Path::new(BASE).join("baseline.json"))?)}),
    )?;
    println!("MIDNIGHT_BASELINE_PREPARED_REAL_DAY_OBSERVATION_PENDING");
    Ok(())
}
fn observe() -> Result<()> {
    require(
        observe_allowed(&clock_date()?),
        "wait for real October 24 JST; no stores opened",
    )?;
    candidate()?;
    let baseline = read("baseline.json")?;
    require(
        hash(&fs::read(Path::new(BASE).join("baseline.json"))?)
            == read("baseline-digest.json")?["sha256"],
        "baseline digest changed",
    )?;
    require(
        baseline["root"] == ROOT && baseline["date"] == "2026-10-23",
        "baseline identity mismatch",
    )?;
    require(
        within_first_day(
            Local::now().timestamp(),
            baseline["prepared_unix_seconds"]
                .as_i64()
                .ok_or("preparation time absent")?,
        ),
        "observation must precede 24 elapsed hours to distinguish calendar midnight",
    )?;
    require(
        read("prepare-closeout.json")?["normal_exit"] == true,
        "preparation closeout absent",
    )?;
    require(
        hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == baseline["rules_sha256"],
        "RULES changed",
    )?;
    save(
        "observe-attempt.json",
        &json!({"date":clock_date()?,"baseline_sha256":read("baseline-digest.json")?["sha256"],"reseed":false}),
    )?;
    let admin = baseline["admin_alias"]
        .as_str()
        .ok_or("admin alias missing")?;
    let recipient = baseline["recipient_alias"]
        .as_str()
        .ok_or("recipient alias missing")?;
    let mut runtime = Runtime::start("observe", Some(admin.into()))?;
    let work = (|| -> Result<Value> {
        require(selected()? == recipient, "saved recipient changed")?;
        require(
            call(recipient, "account.status", json!({}))?["id"] == baseline["recipient_id"],
            "old credential identity changed",
        )?;
        let thread = &baseline["thread_id"];
        let old = call(recipient, "post.get", json!({"thread_id":thread,"no":1}))?;
        require(
            original_fields(&old) == baseline["target"],
            "old Post changed before observation",
        )?;
        let prior_reply = call(recipient, "post.get", json!({"thread_id":thread,"no":2}))?;
        require(
            original_fields(&prior_reply) == baseline["reply"],
            "old Response changed",
        )?;
        require(
            call(recipient, "conversation.subscriptions", json!({}))?["items"]
                .as_array()
                .ok_or("subscriptions absent")?
                .is_empty(),
            "subscription confounds direct response",
        )?;
        require(
            !registration()?.is_registered()? && history()? == [1, 0],
            "owned OS history/control changed",
        )?;
        swarm_runtime::notifications::register(
            Path::new(ROOT),
            &Path::new(CANDIDATE).join("swarm-desktop.exe"),
        )?;
        let fresh = call(
            recipient,
            "post.create",
            json!({"thread_id":thread,"body":"MIDNIGHT_IDENTITY_NEW_DAY_POST"}),
        )?;
        require(
            fresh["author"]["id"] == baseline["recipient_id"] && fresh["no"] == 3,
            "immutable owner/number changed",
        )?;
        require(
            valid_display(&fresh)? != valid_display(&old)?,
            "USER display did not rotate at actual midnight",
        )?;
        let response = call(
            admin,
            "response.create",
            json!({"thread_id":thread,"to":1,"body":AFTER}),
        )?;
        require(
            response["no"] == 4 && response["reply_to"] == 1,
            "Response target/number mismatch",
        )?;
        delivered(
            [1, 1],
            baseline["recipient_id"]
                .as_str()
                .ok_or("recipient missing")?,
        )?;
        let edited = call(
            recipient,
            "post.edit",
            json!({"thread_id":thread,"no":1,"if_revision":1,"body":"MIDNIGHT_IDENTITY_EDITED_AFTER_DATE"}),
        )?;
        require(
            edited["author"] == old["author"]
                && edited["revision"] == 2
                && edited["edited_by"] == baseline["recipient_id"]
                && edited["editor_display_id"] == fresh["author"]["display_id"],
            "old ownership/editor snapshot mismatch",
        )?;
        require(
            hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == baseline["rules_sha256"],
            "RULES changed during observation",
        )?;
        let posts = call(recipient, "post.list", json!({"thread_id":thread}))?;
        require(
            posts["items"].as_array().ok_or("Posts absent")?.len() == 4,
            "unexpected Post count",
        )?;
        Ok(
            json!({"status":"REAL_MIDNIGHT_SCOPED_PASS","date":clock_date()?,"recipient_id":baseline["recipient_id"],"display_before":valid_display(&old)?,"display_after":valid_display(&fresh)?,"old_target_id":old["id"],"fresh_post_id":fresh["id"],"reply_id":response["id"],"old_alias_retained":true,"unsubscribed_direct_reply_delivered":true,"history":[1,1],"old_post_author_snapshot_retained":true,"new_day_editor_snapshot":true,"native_gui":false,"clock_changed":false,"whole_R07_accepted":false}),
        )
    })();
    let cleanup = close(&mut runtime, "observe");
    let result = work?;
    cleanup?;
    save("result.json", &result)?;
    println!("REAL_MIDNIGHT_SCOPED_PASS");
    Ok(())
}
// Continue only the unperformed edit after the preserved four-Post observation.
// The original executable, attempt and baseline remain immutable.
fn recover_edit() -> Result<()> {
    require(
        observe_allowed(&clock_date()?),
        "recovery requires real October 24",
    )?;
    candidate()?;
    let b = read("baseline.json")?;
    require(
        hash(&fs::read(Path::new(BASE).join("baseline.json"))?)
            == "a096c2b7e5c2f7610700105c8f0ba28f65d5e49a76b1bcc1579f2a9954692e15",
        "original baseline changed",
    )?;
    require(
        within_first_day(
            Local::now().timestamp(),
            b["prepared_unix_seconds"]
                .as_i64()
                .ok_or("baseline time absent")?,
        ),
        "original window expired",
    )?;
    require(
        read("observe-closeout.json")?["normal_exit"] == true
            && !Path::new(BASE).join("result.json").exists(),
        "original failure closeout required",
    )?;
    let admin = b["admin_alias"].as_str().ok_or("admin absent")?;
    let recipient = b["recipient_alias"].as_str().ok_or("recipient absent")?;
    require(selected()? == recipient, "saved recipient changed")?;
    require(
        !registration()?.is_registered()? && history()? == [1, 1],
        "actual delivered history required; never resend",
    )?;
    let db = rusqlite::Connection::open_with_flags(
        Path::new(ROOT).join("swarm.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let grants: i64 = db.query_row(
        "SELECT COUNT(*) FROM account_permissions WHERE account_id=?1 AND operation='post.edit'",
        [b["recipient_id"].as_str().ok_or("recipient id absent")?],
        |r| r.get(0),
    )?;
    require(grants == 0, "unexpected preexisting edit grant")?;
    drop(db);
    save(
        "recover-edit-attempt.json",
        &json!({"date":clock_date()?,"original_attempt_preserved":true,"resend":false,"reseed":false,"history":[1,1],"prior_edit_grants":0}),
    )?;
    let mut runtime = Runtime::start("recover-edit", Some(admin.into()))?;
    let mut granted = false;
    let work = (|| -> Result<Value> {
        let thread = &b["thread_id"];
        require(
            call(recipient, "account.status", json!({}))?["id"] == b["recipient_id"],
            "recipient changed",
        )?;
        let old = call(recipient, "post.get", json!({"thread_id":thread,"no":1}))?;
        let prior = call(recipient, "post.get", json!({"thread_id":thread,"no":2}))?;
        require(
            original_fields(&old) == b["target"] && original_fields(&prior) == b["reply"],
            "old snapshots changed",
        )?;
        let fresh = call(recipient, "post.get", json!({"thread_id":thread,"no":3}))?;
        let response = call(recipient, "post.get", json!({"thread_id":thread,"no":4}))?;
        require(
            fresh["author"]["id"] == b["recipient_id"]
                && fresh["revision"] == 1
                && fresh["body"] == "MIDNIGHT_IDENTITY_NEW_DAY_POST"
                && valid_display(&fresh)? != valid_display(&old)?,
            "preserved new-day Post mismatch",
        )?;
        require(
            response["reply_to"] == 1
                && response["body"] == AFTER
                && response["author"]["id"] == prior["author"]["id"],
            "preserved direct Response mismatch",
        )?;
        require(
            call(recipient, "conversation.subscriptions", json!({}))?["items"]
                .as_array()
                .ok_or("subscriptions absent")?
                .is_empty()
                && pending(b["recipient_id"].as_str().ok_or("recipient absent")?)? == 0,
            "direct delivery confounded",
        )?;
        // Default human permissions omit post.edit. Grant only this fixture Thread,
        // then restore inherit even when the edit/assertion fails.
        let permission = |decision| json!({"subject_type":"account","subject_id":b["recipient_id"],"operation":"post.edit","scope":"thread","target_id":thread,"decision":decision});
        call(admin, "permission.set", permission("allow"))?;
        granted = true;
        save(
            "recover-edit-grant.json",
            &json!({"operation":"post.edit","scope":"thread","temporary":true}),
        )?;
        let edited = call(
            recipient,
            "post.edit",
            json!({"thread_id":thread,"no":1,"if_revision":1,"body":"MIDNIGHT_IDENTITY_EDITED_AFTER_DATE"}),
        )?;
        require(
            edited["author"] == old["author"]
                && edited["revision"] == 2
                && edited["edited_by"] == b["recipient_id"]
                && edited["editor_display_id"] == fresh["author"]["display_id"],
            "old ownership/editor snapshot mismatch",
        )?;
        require(
            hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == b["rules_sha256"],
            "RULES changed",
        )?;
        require(
            call(recipient, "post.list", json!({"thread_id":thread}))?["items"]
                .as_array()
                .ok_or("Posts absent")?
                .len()
                == 4,
            "unexpected Post count",
        )?;
        Ok(
            json!({"status":"REAL_MIDNIGHT_SCOPED_PASS_AFTER_FIXTURE_GRANT_RECOVERY","date":clock_date()?,"recipient_id":b["recipient_id"],"display_before":valid_display(&old)?,"display_after":valid_display(&fresh)?,"history":[1,1],"old_post_author_snapshot_retained":true,"new_day_editor_snapshot":true,"original_failed_attempt_preserved":true,"no_posts_resent":true,"native_gui":false,"clock_changed":false,"whole_R07_accepted":false}),
        )
    })();
    let restore = if granted {
        call(admin,"permission.set",json!({"subject_type":"account","subject_id":b["recipient_id"],"operation":"post.edit","scope":"thread","target_id":b["thread_id"],"decision":"inherit"})).map(|_| ())
    } else {
        Ok(())
    };
    if restore.is_ok() && granted {
        save(
            "recover-edit-permission-restored.json",
            &json!({"decision":"inherit","restored":true}),
        )?;
    }
    let cleanup = close(&mut runtime, "recover-edit");
    let result = work?;
    restore?;
    cleanup?;
    save("recover-edit-result.json", &result)?;
    println!("REAL_MIDNIGHT_SCOPED_PASS_AFTER_FIXTURE_GRANT_RECOVERY");
    Ok(())
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result=match args.as_slice() {
        [mode] if mode=="--plan" => clock_date().map(|date|println!("{}",json!({"date":date,"date_ready":observe_allowed(&date),"stores_opened":false,"runtime_started":false,"not_before":"2026-10-24T00:00:00+09:00","additional_guard":"baseline age must be under 24 hours"}))),
        [mode] if mode=="--prepare" => prepare(),
        [mode] if mode=="--observe" => observe(),
        [mode] if mode=="--recover-edit" => recover_edit(),
        _ => Err("fixed --plan/--prepare/--observe only".into()),
    };
    if result.is_err() {
        // Never emit backend bodies, arguments or generated credentials on failure.
        eprintln!("MIDNIGHT_NOT_ACCEPTED; preserve phase receipts, Data and process uncertainty");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observation_requires_the_exact_next_real_date() {
        assert!(!observe_allowed("2026-10-23"));
        assert!(observe_allowed("2026-10-24"));
        assert!(!observe_allowed("2026-10-25"));
    }
    #[test]
    fn first_day_guard_excludes_a_24_hour_rotation_explanation() {
        assert!(!within_first_day(99, 100));
        assert!(within_first_day(100, 100));
        assert!(within_first_day(86_499, 100));
        assert!(!within_first_day(86_500, 100));
    }
    #[test]
    fn preserved_fields_detect_ownership_number_and_snapshot_changes() {
        let old = json!({"id":"p","no":1,"revision":1,"reply_to":null,"body":"secret fixture body","author":{"id":"immutable","display_id":"0123456789"}});
        for key in ["id", "no", "revision", "reply_to", "author", "body"] {
            let mut changed = old.clone();
            changed[key] = json!("different");
            assert_ne!(original_fields(&old), original_fields(&changed));
        }
        assert!(
            !original_fields(&old)
                .to_string()
                .contains("secret fixture body")
        );
    }
}
