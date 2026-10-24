//! Direct-response-only pending target deletion joined to owned real OS history.
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
const MARKERS: [&str; 5] = [
    "NOTIFY_DIRECT_POS_AK",
    "NOTIFY_DIRECT_LOST_TARGET_AL",
    "NOTIFY_DIRECT_LIVE_TARGET_AM",
    "NOTIFY_DIRECT_RESTORED_TARGET_AN",
    "NOTIFY_DIRECT_PLAIN_NOT_SUBSCRIBED_AO",
];
fn ensure(b: bool, m: &str) -> Result<()> {
    if b { Ok(()) } else { Err(m.into()) }
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn continuing() -> bool {
    matches!(
        std::env::args().nth(1).as_deref(),
        Some("--continue-existing-targets" | "--continue-after-positive-readback")
    )
}
fn after_positive() -> bool {
    std::env::args().nth(1).as_deref() == Some("--continue-after-positive-readback")
}
fn file_name(n: &str) -> String {
    if after_positive() {
        n.replacen("direct-target", "direct-target-after-positive", 1)
    } else if continuing() {
        n.replacen("direct-target", "direct-target-continued", 1)
    } else {
        n.into()
    }
}
fn save(n: &str, v: &Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(Path::new(BASE).join(file_name(n)))?;
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
fn history() -> Result<[usize; 5]> {
    history_for(&MARKERS)
}
fn history_for(markers: &[&str; 5]) -> Result<[usize; 5]> {
    use windows::{
        UI::Notifications::ToastNotificationManager,
        Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
        core::HSTRING,
    };
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    let result = (|| -> Result<[usize; 5]> {
        let identity = Identity::new(
            Path::new(ROOT),
            &Path::new(CANDIDATE).join("swarm-desktop.exe"),
        )?;
        let entries = ToastNotificationManager::History()?
            .GetHistoryWithId(&HSTRING::from(identity.app_id()))?;
        let mut counts = [0; 5];
        // The frozen baseline predates every unexecuted negative/fresh action.
        // Production uses a new UUID batch tag for new work. Keep the positive
        // tag plus all new tags; never swallow an error on an observed tag.
        let baseline: Option<Value> = if after_positive() {
            Some(serde_json::from_slice(&fs::read(
                Path::new(BASE).join("direct-target-os-metadata.json"),
            )?)?)
        } else {
            None
        };
        for entry in entries {
            if let Some(baseline) = &baseline {
                let tag = hash(entry.Tag()?.to_string().as_bytes());
                let group = hash(entry.Group()?.to_string().as_bytes());
                if baseline["entries"]
                    .as_array()
                    .ok_or("baseline entries absent")?
                    .iter()
                    .any(|old| {
                        old["tag_sha256"] == tag
                            && old["group_sha256"] == group
                            && old["content"]["markers"][0] != true
                    })
                {
                    continue;
                }
            }
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
fn wait_history(index: usize) -> Result<[usize; 5]> {
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

fn wait_drained(account: &str) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(2);
    while outbox(account)? != 0 {
        ensure(
            Instant::now() < until,
            "OS history present but outbox not drained",
        )?;
        std::thread::sleep(Duration::from_millis(40));
    }
    Ok(())
}
fn no_lost_for(seconds: u64, expected: [usize; 5]) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < until {
        ensure(
            history()? == expected,
            "ineligible direct or ordinary marker delivered",
        )?;
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}
fn run() -> Result<()> {
    if after_positive() {
        let failure: Value = serde_json::from_slice(&fs::read(
            Path::new(BASE).join("direct-target-continued-failure.json"),
        )?)?;
        ensure(
            failure["error"]
                .as_str()
                .is_some_and(|e| e.contains("0xC00CE558")),
            "prior OS read failure mismatch",
        )?;
        let baseline: Value = serde_json::from_slice(&fs::read(
            Path::new(BASE).join("direct-target-os-metadata.json"),
        )?)?;
        let entries = baseline["entries"].as_array().ok_or("OS baseline absent")?;
        ensure(
            entries
                .iter()
                .all(|e| e["tag_nonempty"] == true && e["group_nonempty"] == true),
            "baseline tag identity absent",
        )?;
        ensure(
            entries
                .iter()
                .filter(|e| e["content"]["markers"][0] == true)
                .count()
                == 1,
            "positive OS tag not unique",
        )?;
        ensure(
            entries
                .iter()
                .all(|e| (1..5).all(|i| e["content"]["markers"][i] != true)),
            "negative/fresh phase already started",
        )?;
        ensure(
            !Path::new(BASE)
                .join("direct-target-continued-positive.json")
                .exists()
                && !Path::new(BASE)
                    .join("direct-target-continued-negative.json")
                    .exists(),
            "prior phase advanced",
        )?;
    }
    if continuing() {
        let failure: Value = serde_json::from_slice(&fs::read(
            Path::new(BASE).join("direct-target-failure.json"),
        )?)?;
        ensure(
            failure["error"] == "targets not owned by selected recipient",
            "initial failure mismatch",
        )?;
        ensure(
            !Path::new(BASE).join("direct-target-prepared.json").exists()
                && !Path::new(BASE).join("direct-target-positive.json").exists(),
            "original notification phase already started",
        )?;
    }
    let previous: Value = serde_json::from_slice(&fs::read(Path::new(BASE).join("result.json"))?)?;
    let prior: Value =
        serde_json::from_slice(&fs::read(Path::new(BASE).join("post-delete-result.json"))?)?;
    ensure(
        previous["status"] == "NORMAL_LOGOUT_NOTIFICATION_PASS"
            && prior["status"] == "SPARSE_POST_DELETE_NOTIFICATION_PASS",
        "prior suffix absent",
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
    ensure(
        history()?
            == if after_positive() {
                [1, 0, 0, 0, 0]
            } else {
                [0; 5]
            },
        "unexpected marker baseline",
    )?;
    save(
        "direct-target-attempt.json",
        &json!({"scope":"direct-response-only pending target deletion","replay_previous":false,"vm":false,"elapsed":false}),
    )?;
    let admin = format!("account-{}", hash(b"NotifyLogoutAdmin"));
    let mut cmd = Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
    cmd.args(["--data-dir", ROOT, "--human-start"])
        .stdin(Stdio::null())
        .stdout(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join(file_name("direct-target.stdout.log")))?,
        )
        .stderr(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join(file_name("direct-target.stderr.log")))?,
        );
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut runtime = Runtime {
        child: cmd.spawn()?,
        admin: Some(admin.clone()),
    };
    save(
        "direct-target-runtime.json",
        &json!({"pid":runtime.child.id(),"candidate":CANDIDATE}),
    )?;
    let until = Instant::now() + Duration::from_secs(15);
    while GuiClient::open(Path::new(ROOT))
        .and_then(|c| c.setup_status())
        .is_err()
    {
        ensure(Instant::now() < until, "runtime readiness deadline")?;
        std::thread::sleep(Duration::from_millis(50));
    }
    let recipient_alias = alias()?;
    let recipient = previous["recipient_id"]
        .as_str()
        .ok_or("recipient absent")?;
    let old_thread = previous["thread_id"].as_str().ok_or("old Thread absent")?;
    let before = call(
        &recipient_alias,
        "post.list",
        json!({"thread_id":old_thread}),
    )?;
    ensure(
        before["items"].as_array().ok_or("old Posts absent")?.len() == 34,
        "old Post count changed",
    )?;
    ensure(outbox(recipient)? == 0, "old pending work")?;
    let rules = hash(&fs::read(Path::new(ROOT).join("RULES.md"))?);
    let subscriptions_before = call(&recipient_alias, "conversation.subscriptions", json!({}))?;
    let board = call(&recipient_alias, "thread.get", json!({"id":old_thread}))?["board_id"].clone();
    let thread = if continuing() {
        let listing = call(&admin, "thread.list", json!({"board_id":board}))?;
        let matches = listing["items"]
            .as_array()
            .ok_or("Threads absent")?
            .iter()
            .filter(|t| t["name"] == "DIRECT_REPLY_NO_SUBSCRIPTION")
            .collect::<Vec<_>>();
        ensure(matches.len() == 1, "unique existing direct Thread absent")?;
        matches[0].clone()
    } else {
        call(
            &admin,
            "thread.create",
            json!({"board_id":board,"name":"DIRECT_REPLY_NO_SUBSCRIPTION"}),
        )?
    };
    let t = thread["id"].as_str().ok_or("new Thread absent")?;
    let not_subscribed = |subscriptions: &Value| -> Result<()> {
        ensure(
            !subscriptions["items"]
                .as_array()
                .ok_or("subscriptions absent")?
                .iter()
                .any(|i| i["id"] == t),
            "new direct-only Thread subscribed",
        )
    };
    not_subscribed(&call(
        &recipient_alias,
        "conversation.subscriptions",
        json!({}),
    )?)?;
    let (a, b) = if continuing() {
        let posts = call(&recipient_alias, "post.list", json!({"thread_id":t}))?;
        let items = posts["items"].as_array().ok_or("existing targets absent")?;
        ensure(
            items.len() == (if after_positive() { 4 } else { 2 })
                && items[0]["body"] == "DIRECT_TARGET_A_MUST_SURVIVE"
                && items[1]["body"] == "DIRECT_TARGET_B_MUST_SURVIVE"
                && items[0]["no"] == 1
                && items[1]["no"] == 2
                && items.iter().take(2).all(|p| {
                    p["revision"] == 1 && p["deleted"] == false && p["reply_to"].is_null()
                }),
            "existing preparation changed; do not recreate",
        )?;
        if after_positive() {
            let prepared: Value = serde_json::from_slice(&fs::read(
                Path::new(BASE).join("direct-target-continued-prepared.json"),
            )?)?;
            ensure(
                prepared["thread_id"] == t
                    && prepared["target_ids"] == json!([items[0]["id"], items[1]["id"]])
                    && items[2]["body"] == MARKERS[4]
                    && items[3]["body"] == MARKERS[0]
                    && items[3]["reply_to"] == items[1]["no"],
                "existing positive preparation mismatch",
            )?;
        }
        (items[0].clone(), items[1].clone())
    } else {
        let a = call(
            &recipient_alias,
            "post.create",
            json!({"thread_id":t,"body":"DIRECT_TARGET_A_MUST_SURVIVE"}),
        )?;
        let b = call(
            &recipient_alias,
            "post.create",
            json!({"thread_id":t,"body":"DIRECT_TARGET_B_MUST_SURVIVE"}),
        )?;
        (a, b)
    };
    ensure(
        a["author"]["id"] == recipient && b["author"]["id"] == recipient,
        "targets not owned by selected recipient",
    )?;
    let plain = if after_positive() {
        call(&recipient_alias, "post.get", json!({"thread_id":t,"no":3}))?
    } else {
        call(
            &admin,
            "post.create",
            json!({"thread_id":t,"body":MARKERS[4]}),
        )?
    };
    ensure(
        outbox(recipient)? == 0,
        "ordinary unsubscribed/self Post incorrectly queued",
    )?;
    save(
        "direct-target-prepared.json",
        &json!({"thread_id":t,"board_id":board,"target_ids":[a["id"],b["id"]],
        "target_nos":[a["no"],b["no"]],"recipient_owns_targets":true,"unsubscribed":true,"plain_post_id":plain["id"],"outbox":0}),
    )?;
    swarm_runtime::notifications::register(
        Path::new(ROOT),
        &Path::new(CANDIDATE).join("swarm-desktop.exe"),
    )?;
    let positive = if after_positive() {
        call(&recipient_alias, "post.get", json!({"thread_id":t,"no":4}))?
    } else {
        call(
            &admin,
            "response.create",
            json!({"thread_id":t,"to":b["no"],"body":MARKERS[0]}),
        )?
    };
    let h = wait_history(0)?;
    ensure(
        h == [1, 0, 0, 0, 0],
        "positive/direct-only control mismatch",
    )?;
    wait_drained(recipient)?;
    save(
        "direct-target-positive.json",
        &json!({"post_id":positive["id"],"history":h,"outbox":0,"unsubscribed":true}),
    )?;
    let began = Instant::now();
    let queued = call(
        &admin,
        "response.create",
        json!({"thread_id":t,"to":a["no"],"body":MARKERS[1]}),
    )?;
    let peer = call(
        &admin,
        "response.create",
        json!({"thread_id":t,"to":b["no"],"body":MARKERS[2]}),
    )?;
    let pending = outbox(recipient)?;
    ensure(
        pending == 2,
        "two direct Responses not pending before target deletion",
    )?;
    let removed = call(&admin, "post.delete", json!({"thread_id":t,"no":a["no"]}))?;
    let change_ms = began.elapsed().as_millis();
    ensure(
        change_ms < 1500,
        "target deletion missed aggregation margin; not accepted",
    )?;
    ensure(
        removed["deleted"] == true && removed["id"] == a["id"] && removed["revision"] == 2,
        "deleted target mismatch",
    )?;
    let still_live = call(
        &recipient_alias,
        "post.get",
        json!({"thread_id":t,"no":queued["no"]}),
    )?;
    ensure(
        still_live["deleted"] == false
            && still_live["body"] == MARKERS[1]
            && still_live["reply_to"] == a["no"],
        "Response itself not live/readable",
    )?;
    not_subscribed(&call(
        &recipient_alias,
        "conversation.subscriptions",
        json!({}),
    )?)?;
    let live_history = wait_history(2)?;
    ensure(
        live_history == [1, 0, 1, 0, 0],
        "lost-target response delivered or live peer missing",
    )?;
    wait_drained(recipient)?;
    no_lost_for(3, [1, 0, 1, 0, 0])?;
    save(
        "direct-target-negative.json",
        &json!({"pending_before":pending,"create_to_target_delete_ms":change_ms,
        "deleted_target_id":a["id"],"withheld_response_id":queued["id"],"live_peer_response_id":peer["id"],
        "withheld_response_itself_live_readable":true,"history":live_history,"outbox":0,"unsubscribed":true}),
    )?;
    let restored = call(&admin, "post.restore", json!({"thread_id":t,"no":a["no"]}))?;
    ensure(
        restored["id"] == a["id"]
            && restored["body"] == a["body"]
            && restored["revision"] == 3
            && restored["deleted"] == false,
        "target restore mismatch",
    )?;
    no_lost_for(3, [1, 0, 1, 0, 0])?;
    let fresh = call(
        &admin,
        "response.create",
        json!({"thread_id":t,"to":a["no"],"body":MARKERS[3]}),
    )?;
    let final_history = wait_history(3)?;
    ensure(
        final_history == [1, 0, 1, 1, 0],
        "restored-target fresh response missing or old/plain marker delivered",
    )?;
    wait_drained(recipient)?;
    ensure(alias()? == recipient_alias, "selection changed")?;
    ensure(
        call(&recipient_alias, "conversation.subscriptions", json!({}))? == subscriptions_before,
        "subscriptions changed",
    )?;
    ensure(
        call(
            &recipient_alias,
            "post.list",
            json!({"thread_id":old_thread}),
        )?["items"]
            == before["items"],
        "old34full Post items changed",
    )?;
    let after = call(&recipient_alias, "post.list", json!({"thread_id":t}))?;
    let items = after["items"].as_array().ok_or("new Posts absent")?;
    ensure(items.len() == 7, "new Thread count changed")?;
    for (index, expected) in [&a, &b, &plain, &positive, &queued, &peer, &fresh]
        .iter()
        .enumerate()
    {
        let item = &items[index];
        ensure(
            item["id"] == expected["id"]
                && item["body"] == expected["body"]
                && item["no"] == expected["no"]
                && item["author"] == expected["author"]
                && item["reply_to"] == expected["reply_to"]
                && item["deleted"] == false
                && item["revision"] == if index == 0 { 3 } else { 1 },
            "new Post identity/body/No/reply/revision mismatch",
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
        "owned registration remains",
    )?;
    save(
        "direct-target-result.json",
        &json!({"status":"DIRECT_REPLY_PENDING_TARGET_DELETE_OS_PASS",
        "runtime_pid":runtime.child.id(),"thread_id":t,"new_post_ids":items.iter().map(|i|i["id"].clone()).collect::<Vec<_>>(),
        "pending_before":pending,"create_to_target_delete_ms":change_ms,"history":final_history,
        "old34full_post_items_unchanged":true,"new7posts_retained":true,"target_a_restored_revision":3,
        "withheld_response_itself_live_readable":true,"unsubscribed_throughout":true,"ordinary_post_not_queued_or_delivered":true,
        "same_alias_selection_subscriptions_rules":true,"normally_exited":true,"owned_registration_absent":true,
        "closed_stream_bytes":[fs::metadata(Path::new(BASE).join(file_name("direct-target.stdout.log")))?.len(),fs::metadata(Path::new(BASE).join(file_name("direct-target.stderr.log")))?.len()],
        "native_gui_observed":false,"banner_click_observed":false,
        "positive_not_reposted":after_positive(),"history_legacy_baseline_excluded_before_content":after_positive(),
        "new_and_positive_tags_must_be_readable":true}),
    )?;
    println!("DIRECT_REPLY_PENDING_TARGET_DELETE_OS_PASS");
    Ok(())
}
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--inspect-history-metadata") {
        use windows::{
            UI::Notifications::ToastNotificationManager,
            Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
            core::HSTRING,
        };
        let result = (|| -> Result<Value> {
            unsafe {
                RoInitialize(RO_INIT_MULTITHREADED)?;
            }
            let identity = Identity::new(
                Path::new(ROOT),
                &Path::new(CANDIDATE).join("swarm-desktop.exe"),
            )?;
            let entries = ToastNotificationManager::History()?
                .GetHistoryWithId(&HSTRING::from(identity.app_id()))?;
            let mut rows = Vec::new();
            for entry in entries {
                let tag = entry.Tag()?.to_string();
                let group = entry.Group()?.to_string();
                let content = match entry.Content() {
                    Err(error) => json!({"stage":"Content","error":error.to_string()}),
                    Ok(doc) => match doc.GetXml() {
                        Err(error) => json!({"stage":"GetXml","error":error.to_string()}),
                        Ok(xml) => {
                            let s = xml.to_string();
                            json!({"stage":"readable","xml_length":s.len(),"markers":MARKERS.map(|m|s.contains(m))})
                        }
                    },
                };
                rows.push(json!({"tag_sha256":hash(tag.as_bytes()),"group_sha256":hash(group.as_bytes()),
                    "tag_nonempty":!tag.is_empty(),"group_nonempty":!group.is_empty(),"content":content}));
            }
            unsafe {
                RoUninitialize();
            }
            Ok(json!({"read_only_own_history":true,"entries":rows}))
        })();
        match result {
            Ok(value) => {
                if let Err(error) = save("direct-target-os-metadata.json", &value) {
                    eprintln!("{error}");
                    std::process::exit(1);
                }
                println!("{value}");
            }
            Err(error) => {
                eprintln!("OS_METADATA_READ_FAILED: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if std::env::args().nth(1).as_deref() == Some("--inspect-os-history") {
        let mut observations = Vec::new();
        for _ in 0..3 {
            observations.push(match history() {
                Ok(counts) => json!({"readable":true,"history":counts}),
                Err(error) => json!({"readable":false,"error":error.to_string()}),
            });
            std::thread::sleep(Duration::from_millis(200));
        }
        let result = json!({"read_only_os_history":true,"observations":observations});
        if let Err(error) = save("direct-target-os-readback.json", &result) {
            eprintln!("OS_READBACK_SAVE_FAILED: {error}");
            std::process::exit(1);
        }
        println!("{result}");
        return;
    }
    if let Err(error) = run() {
        let _ = save(
            "direct-target-failure.json",
            &json!({"status":"NOT_ACCEPTED","error":error.to_string()}),
        );
        eprintln!("DIRECT_REPLY_TARGET_NOT_ACCEPTED: {error}");
        std::process::exit(1);
    }
}
