//! New archived-Thread real-day baseline; does not reuse an old elapsed seed.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use swarm_gui_host::GuiHost;
use swarm_runtime::gui::GuiClient;

type Result<T> = std::result::Result<T, String>;
type Task = Box<dyn FnOnce(&mut GuiHost) -> Result<Value> + Send>;
type Message = (Task, mpsc::Sender<Result<Value>>);
const BASE: &str = "E:/Swarm-Validation-Host/run-20261023-archived-retention";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261023-archived-retention/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const ALIAS: &str = "archived-retention-20261023";
const ORIGINAL: &str = "ARCHIVERETENTIONTARGETORIGINAL20261023";
const CURRENT: &str = "ARCHIVERETENTIONTARGETCURRENT20261023";
const LIVE_ORIGINAL: &str = "ARCHIVERETENTIONLIVEORIGINAL20261023";
const LIVE_CURRENT: &str = "ARCHIVERETENTIONLIVECURRENT20261023";
const FILLER: &str = "ARCHIVERETENTIONFILLER20261023";

fn ensure(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(message.into()) }
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| "system time unavailable".into())
}
fn save(name: &str, v: &Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(Path::new(BASE).join(name))
        .map_err(|_| "preserve existing fixed-root receipt".to_string())?;
    writeln!(f, "{v}").map_err(|_| "receipt write failed".into())
}
fn pins() -> Result<()> {
    for (file, sha) in [
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
            hash(&fs::read(Path::new(CANDIDATE).join(file)).map_err(|_| "candidate unavailable")?)
                == sha,
            "frozen candidate changed",
        )?;
    }
    Ok(())
}
fn post_meta(v: &Value) -> Value {
    json!({"id":v["id"],"no":v["no"],"revision":v["revision"],"body_sha256":v["body"].as_str().map(|b|hash(b.as_bytes())),"reply_to":v["reply_to"],"deleted":v["deleted"]})
}
struct Host {
    tx: mpsc::Sender<Message>,
    deadline: Instant,
    uncertain: Arc<AtomicBool>,
}
impl Host {
    fn open(uncertain: Arc<AtomicBool>) -> Result<Self> {
        let (tx, rx) = mpsc::channel::<Message>();
        let (ready_tx, ready_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let host = GuiHost::open(Path::new(ROOT));
            match host {
                Ok(mut host) => {
                    let _ = ready_tx.send(Ok(()));
                    while let Ok((job, reply)) = rx.recv() {
                        let _ = reply.send(job(&mut host));
                    }
                }
                Err(f) => {
                    let _ = ready_tx.send(Err(f.code.to_string()));
                }
            }
        });
        ready_rx
            .recv_timeout(Duration::from_secs(15))
            .map_err(|_| "host startup deadline")??;
        Ok(Self {
            tx,
            deadline: Instant::now() + Duration::from_secs(900),
            uncertain,
        })
    }
    fn invoke(
        &self,
        f: impl FnOnce(&mut GuiHost) -> Result<Value> + Send + 'static,
    ) -> Result<Value> {
        let remaining = self
            .deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(20));
        ensure(!remaining.is_zero(), "preparation deadline")?;
        let (tx, rx) = mpsc::channel();
        self.tx
            .send((Box::new(f), tx))
            .map_err(|_| "host worker unavailable")?;
        rx.recv_timeout(remaining).map_err(|_| {
            self.uncertain.store(true, Ordering::SeqCst);
            "uncertain host request; preserve runtime, no replay".to_string()
        })?
    }
    fn create(&self, op: &'static str, args: Value) -> Result<Value> {
        self.invoke(move |h| {
            let v = h.create(op, args).map_err(|f| f.code.to_string())?;
            h.acknowledge_create().map_err(|f| f.code.to_string())?;
            Ok(v)
        })
    }
    fn read(&self, op: &'static str, args: Value) -> Result<Value> {
        self.invoke(move |h| h.read(op, args).map_err(|f| f.code.to_string()))
    }
}
fn cli(project: &str, args: &[&str], uncertain: &AtomicBool) -> Result<Value> {
    let mut command = Command::new(Path::new(CANDIDATE).join("swarm.exe"));
    command
        .args([
            "--data-dir",
            ROOT,
            "--project",
            project,
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
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|_| "public CLI launch failed")?;
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(25)),
            _ => {
                uncertain.store(true, Ordering::SeqCst);
                let _ = save(
                    "uncertain-cli.json",
                    &json!({"pid":child.id(),"forced_exit":false}),
                );
                return Err("uncertain CLI; preserve runtime".into());
            }
        }
    }
    let out = child
        .wait_with_output()
        .map_err(|_| "CLI output unavailable")?;
    ensure(
        out.status.success() && out.stderr.is_empty() && out.stdout.len() < 65536,
        "public CLI failed; no raw response saved",
    )?;
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| "CLI JSON invalid")?;
    ensure(
        v["schema_version"] == 1 && v["ok"] == true,
        "CLI response rejected",
    )?;
    Ok(v["data"].clone())
}
fn history_matches(v: &Value, body: &str) -> bool {
    v["items"].as_array().is_some_and(|a| {
        a.len() == 1
            && a[0]["revision"] == 1
            && a[0]["body"]
                .as_str()
                .is_some_and(|s| hash(s.as_bytes()) == hash(body.as_bytes()))
    })
}
fn stop(host: &Host, child: &mut Child) -> Result<()> {
    host.invoke(|h| h.stop_runtime().map_err(|f| f.code.to_string()))?;
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().map_err(|_| "runtime status unavailable")? {
            return ensure(status.success(), "runtime exit failed");
        }
        ensure(
            Instant::now() < until,
            "normal exit deadline; preserve runtime",
        )?;
        std::thread::sleep(Duration::from_millis(40));
    }
}
fn prepare(
    host: &Host,
    project: &mut Option<String>,
    ai_active: &mut bool,
    resuming: bool,
) -> Result<Value> {
    let (account, board, archived, live) = if resuming {
        let status = host.invoke(|h| h.creation_status().map_err(|f| f.code.to_string()))?;
        if status["pending"] == true {
            ensure(
                status["operation"] == "post.create" && status["expired"] == false,
                "unexpected pending create",
            )?;
            let recovered = host.invoke(|h| {
                let value = h.retry_create().map_err(|f| f.code.to_string())?;
                h.acknowledge_create().map_err(|f| f.code.to_string())?;
                Ok(value)
            })?;
            save(
                "pending-create-recovery.json",
                &json!({"operation":"post.create","normal_same_intent_retry":true,"post":post_meta(&recovered)}),
            )?;
        }
        let targets: Value = serde_json::from_slice(
            &fs::read(Path::new(BASE).join("targets.json"))
                .map_err(|_| "same targets unavailable")?,
        )
        .map_err(|_| "targets invalid")?;
        let account = targets["account_id"]
            .as_str()
            .ok_or("same account missing")?
            .to_owned();
        let expected = account.clone();
        host.invoke(move |h| {
            h.retention_control(&expected, "retention.settings.get", json!({}))
                .map_err(|f| f.code.to_string())
        })?;
        ensure(
            targets["archived_thread"]["name"] == "ARCHIVED_RETENTION_TARGET"
                && targets["live_thread"]["name"] == "ARCHIVED_RETENTION_LIVE",
            "different preparation targets",
        )?;
        (
            account,
            targets["board"].clone(),
            targets["archived_thread"].clone(),
            targets["live_thread"].clone(),
        )
    } else {
        let auth = host.invoke(|h| {
            h.authenticate("setup", "ArchivedRetentionFixture", "")
                .map_err(|f| f.code.to_string())
        })?;
        let account = auth["account"]["id"]
            .as_str()
            .ok_or("setup account missing")?
            .to_owned();
        let account_for_backup = account.clone();
        host.invoke(move |h| {
            h.backup_control(
                &account_for_backup,
                "settings.set",
                json!({"enabled":false,"interval_seconds":86400,"retention":2,"destination":null}),
            )
            .map_err(|f| f.code.to_string())
        })?;
        let board = host.create("board.create", json!({"name":"ARCHIVED_RETENTION_BOARD"}))?;
        let archived = host.create(
            "thread.create",
            json!({"name":"ARCHIVED_RETENTION_TARGET","board_id":board["id"]}),
        )?;
        let live = host.create(
            "thread.create",
            json!({"name":"ARCHIVED_RETENTION_LIVE","board_id":board["id"]}),
        )?;
        save(
            "targets.json",
            &json!({"account_id":account,"board":board,"archived_thread":archived,"live_thread":live}),
        )?;
        (account, board, archived, live)
    };
    let a = archived["id"]
        .as_str()
        .ok_or("archived Thread id missing")?
        .to_owned();
    let l = live["id"]
        .as_str()
        .ok_or("live Thread id missing")?
        .to_owned();
    let start_no = if resuming {
        let status = host.invoke(|h| h.creation_status().map_err(|f| f.code.to_string()))?;
        ensure(status["pending"] == false, "pending create remains")?;
        let page = host.read(
            "post.page",
            json!({"thread_id":a,"page":1,"page_size":1000}),
        )?;
        let count = page["total_posts"]
            .as_i64()
            .filter(|n| *n >= 2 && *n < 1000)
            .ok_or("unfinished archived count required")?;
        let items = page["items"].as_array().ok_or("partial posts missing")?;
        ensure(items.len() == count as usize, "partial page incomplete")?;
        for (i, p) in items.iter().enumerate() {
            ensure(
                p["no"] == i + 1 && p["body"] == if i == 0 { CURRENT } else { FILLER },
                "partial posts changed",
            )?;
        }
        let live_now = host.read("post.get", json!({"thread_id":l,"no":1}))?;
        ensure(
            live_now["body"] == LIVE_CURRENT && live_now["revision"] == 2,
            "partial live control changed",
        )?;
        save(
            "continuation-prefix.json",
            &json!({"existing_count":count,"existing_posts":items.iter().map(post_meta).collect::<Vec<_>>(),"pending_create_recovered":status["pending"] == true,"setup_repeated":false,"edits_repeated":false}),
        )?;
        count + 1
    } else {
        let first = host.create("post.create", json!({"thread_id":a,"body":ORIGINAL}))?;
        ensure(first["no"] == 1, "first archived No mismatch")?;
        let edit_a = a.clone();
        host.invoke(move |h| {
            h.edit(json!({"thread_id":edit_a,"no":1,"body":CURRENT,"if_revision":1}))
                .map_err(|f| f.code.to_string())
        })?;
        let first_live = host.create("post.create", json!({"thread_id":l,"body":LIVE_ORIGINAL}))?;
        ensure(first_live["no"] == 1, "live control No mismatch")?;
        let edit_l = l.clone();
        host.invoke(move |h| {
            h.edit(json!({"thread_id":edit_l,"no":1,"body":LIVE_CURRENT,"if_revision":1}))
                .map_err(|f| f.code.to_string())
        })?;
        2
    };
    for no in start_no..=1000 {
        let v = if no == 2 {
            host.create(
                "response.create",
                json!({"thread_id":a,"body":FILLER,"to":1}),
            )?
        } else {
            host.create("post.create", json!({"thread_id":a,"body":FILLER}))?
        };
        ensure(
            v["no"] == no && v["revision"] == 1,
            "archived fill numbering mismatch",
        )?;
        if no % 100 == 0 {
            save(
                &format!("progress-{no}.json"),
                &json!({"highest_no":no,"last_id":v["id"]}),
            )?;
            println!("ARCHIVED_BASELINE_PROGRESS {no}/1000");
        }
    }
    let archived = host.read("thread.get", json!({"id":a}))?;
    ensure(
        archived["archived"] == true
            && archived["archived_at"].is_string()
            && archived["deleted_at"].is_null(),
        "Thread not normally archived",
    )?;
    let page = host.read(
        "post.page",
        json!({"thread_id":a,"page":1,"page_size":1000}),
    )?;
    ensure(
        page["total_posts"] == 1000 && page["items"].as_array().is_some_and(|v| v.len() == 1000),
        "archived page baseline incomplete",
    )?;
    let posts = page["items"].as_array().ok_or("posts missing")?;
    let mut ids = BTreeSet::new();
    for (i, post) in posts.iter().enumerate() {
        ensure(
            post["no"] == (i + 1)
                && post["id"]
                    .as_str()
                    .is_some_and(|s| ids.insert(s.to_owned())),
            "archived IDs/numbers not unique",
        )?;
        ensure(
            post["body"] == if i == 0 { CURRENT } else { FILLER } && post["deleted"] == false,
            "archived body baseline mismatch",
        )?;
    }
    ensure(
        posts[0]["revision"] == 2 && posts[1]["reply_to"] == 1,
        "edited history or formal reply baseline mismatch",
    )?;
    ensure(
        history_matches(
            &host.read("post.history", json!({"thread_id":a,"no":1}))?,
            ORIGINAL,
        ),
        "archived history baseline missing",
    )?;
    ensure(
        history_matches(
            &host.read("post.history", json!({"thread_id":l,"no":1}))?,
            LIVE_ORIGINAL,
        ),
        "live history baseline missing",
    )?;
    let rules = format!(
        "# Isolated archived retention fixture\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: archived-retention-history\n    effect: allow\n    actions: [post.history]\n    scope:\n      threads: [{a}, {l}]\naccess:\n  boards: []\n  threads: []\n~~~\n"
    );
    let old = host.read("rules.get", json!({}))?;
    let candidate = rules.clone();
    ensure(
        host.invoke(move |h| {
            h.manage_rules("rules.validate", json!({"source":candidate}))
                .map_err(|f| f.code.to_string())
        })?["valid"]
            == true,
        "fixture rules invalid",
    )?;
    let candidate = rules.clone();
    let old_revision = old["revision"].clone();
    host.invoke(move |h| {
        h.manage_rules(
            "rules.save",
            json!({"source":candidate,"if_revision":old_revision}),
        )
        .map_err(|f| f.code.to_string())
    })?;
    let account_for_settings = account.clone();
    let settings = host.invoke(move |h| h.retention_control(&account_for_settings,"retention.settings.set",json!({"history_days":null,"deleted_days":null,"deleted_board_days":null,"deleted_thread_days":null,"archived_thread_days":1})).map_err(|f|f.code.to_string()))?;
    ensure(
        settings["archived_thread_days"] == 1
            && settings["history_days"].is_null()
            && settings["deleted_days"].is_null(),
        "retention setting mismatch",
    )?;
    fs::create_dir(Path::new(BASE).join("PublicReadbackProject"))
        .map_err(|_| "preserve Project folder")?;
    let account_for_project = account.clone();
    let p = host.invoke(move |h| h.project_control(&account_for_project,"project.register",json!({"name":"Archived retention readback","root":format!("{BASE}/PublicReadbackProject")})).map_err(|f|f.code.to_string()))?;
    let p = p["id"].as_str().ok_or("Project id missing")?.to_owned();
    *project = Some(p.clone());
    *ai_active = true;
    let ai = cli(&p, &["identity", "start"], &host.uncertain)?;
    ensure(
        ai["identity"]["id"].is_string(),
        "public identity envelope metadata missing",
    )?;
    let public_archived = cli(&p, &["thread", "get", "--id", &a], &host.uncertain)?;
    ensure(
        public_archived["id"] == a && public_archived["archived"] == true,
        "public archived baseline mismatch",
    )?;
    ensure(
        history_matches(
            &cli(
                &p,
                &["post", "history", "--thread-id", &a, "--no", "1"],
                &host.uncertain,
            )?,
            ORIGINAL,
        ),
        "public archived history baseline mismatch",
    )?;
    ensure(
        history_matches(
            &cli(
                &p,
                &["post", "history", "--thread-id", &l, "--no", "1"],
                &host.uncertain,
            )?,
            LIVE_ORIGINAL,
        ),
        "public live history baseline mismatch",
    )?;
    cli(&p, &["identity", "release"], &host.uncertain)?;
    *ai_active = false;
    let current_live = host.read("post.get", json!({"thread_id":l,"no":1}))?;
    ensure(
        current_live["body"] == LIVE_CURRENT && current_live["revision"] == 2,
        "live baseline mismatch",
    )?;
    let at = now()?;
    Ok(
        json!({"scope":"archived-thread-real-day","root":ROOT,"timestamp_source":"actual product clock","baseline_unix_seconds":at,"observe_not_before_unix_seconds":at+86400,"account_id":account,"board":board,"archived_thread":archived,"archived_posts":posts.iter().map(post_meta).collect::<Vec<_>>(),"archived_history_count":1,"live_thread":live,"live_post":post_meta(&current_live),"live_history_count":1,"settings":settings,"rules_sha256":hash(rules.as_bytes()),"project_id":p,"actor_id":ai["identity"]["id"],"saved_alias":ALIAS,"public_positive_archived_history":true,"public_positive_live_history":true,"ai_released":true,"elapsed_day_accepted":false,"native_gui_observed":false,"background_watcher":false,"reseed":false}),
    )
}
fn continue_existing() -> Result<()> {
    pins()?;
    ensure(
        Path::new(ROOT).is_dir() && !Path::new(BASE).join("baseline.json").exists(),
        "same unfinished root required",
    )?;
    let failed: Value = serde_json::from_slice(
        &fs::read(Path::new(BASE).join("failure.json"))
            .map_err(|_| "prior failure receipt required")?,
    )
    .map_err(|_| "failure receipt invalid")?;
    ensure(
        failed["status"] == "UNCERTAIN_NOT_ACCEPTED",
        "unexpected prior phase",
    )?;
    save(
        "continuation-attempt.json",
        &json!({"scope":"same-partial-archived-preparation-unused-suffix","standing_authorization_20261023":true,"at_unix_seconds":now()?,"setup_repeated":false,"reseed":false}),
    )?;
    let uncertain = Arc::new(AtomicBool::new(false));
    let host = Host::open(uncertain.clone())?;
    let mut project = None;
    let mut active = false;
    let outcome = prepare(&host, &mut project, &mut active, true);
    if uncertain.load(Ordering::SeqCst) {
        save(
            "continuation-failure.json",
            &json!({"status":"UNCERTAIN_NOT_ACCEPTED","normal_cleanup_attempted":false}),
        )?;
        return Err("uncertain continuation; preserve runtime".into());
    }
    let release = if active {
        cli(
            project.as_deref().ok_or("active Project missing")?,
            &["identity", "release"],
            &uncertain,
        )
        .map(|_| ())
    } else {
        Ok(())
    };
    if uncertain.load(Ordering::SeqCst) {
        return Err("uncertain release; preserve runtime".into());
    }
    let stopped = host.invoke(|h| h.stop_runtime().map_err(|f| f.code.to_string()));
    save(
        "continuation-closeout.json",
        &json!({"preparation_succeeded":outcome.is_ok(),"normal_stop_accepted":stopped.is_ok(),"ai_released":release.is_ok(),"preparation_error":outcome.as_ref().err(),"stop_error":stopped.as_ref().err(),"external_process_exit_check_required":true,"background_watcher":false}),
    )?;
    release?;
    stopped?;
    save("baseline-pending-closeout.json", &outcome?)?;
    println!("ARCHIVED_BASELINE_READY_PENDING_EXTERNAL_PROCESS_CLOSEOUT");
    Ok(())
}
fn run() -> Result<()> {
    if std::env::args().skip(1).collect::<Vec<_>>() == ["--continue-existing"] {
        return continue_existing();
    }
    ensure(
        std::env::args().skip(1).collect::<Vec<_>>() == ["--prepare"],
        "fixed prepare only; no time/root override",
    )?;
    pins()?;
    ensure(
        !Path::new(BASE).exists(),
        "fixed archived baseline root already exists; never reseed",
    )?;
    fs::create_dir(BASE).map_err(|_| "fixed root exists or cannot be created")?;
    fs::create_dir(ROOT).map_err(|_| "Data creation failed")?;
    save(
        "attempt.json",
        &json!({"scope":"new-archived-thread-real-day-baseline","standing_user_authorization_20261023":true,"not_old_elapsed_retry":true,"at_unix_seconds":now()?}),
    )?;
    swarm_runtime::startup_preference_set(Path::new(ROOT), false)
        .map_err(|f| f.code.to_string())?;
    let mut command = Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
    command
        .args(["--data-dir", ROOT, "--human-start"])
        .stdin(Stdio::null());
    for (suffix, is_out) in [("stdout", true), ("stderr", false)] {
        let f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(Path::new(BASE).join(format!("runtime.{suffix}.log")))
            .map_err(|_| "runtime stream unavailable")?;
        if is_out {
            command.stdout(f);
        } else {
            command.stderr(f);
        }
    }
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|_| "runtime launch failed")?;
    save(
        "runtime.json",
        &json!({"pid":child.id(),"candidate":CANDIDATE}),
    )?;
    let uncertain = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(15);
        loop {
            if GuiClient::open(Path::new(ROOT))
                .and_then(|c| c.setup_status())
                .is_ok()
            {
                let _ = tx.send(true);
                return;
            }
            if Instant::now() >= until {
                let _ = tx.send(false);
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    });
    ensure(
        rx.recv_timeout(Duration::from_secs(16)).unwrap_or(false),
        "runtime readiness unavailable; preserve process",
    )?;
    let host = Host::open(uncertain.clone())?;
    let mut project = None;
    let mut active = false;
    let outcome = prepare(&host, &mut project, &mut active, false);
    if uncertain.load(Ordering::SeqCst) {
        save(
            "failure.json",
            &json!({"status":"UNCERTAIN_NOT_ACCEPTED","runtime_pid":child.id(),"normal_cleanup_attempted":false}),
        )?;
        return Err("uncertain operation; preserve runtime and pending state".into());
    }
    let release = if active {
        cli(
            project.as_deref().ok_or("active Project missing")?,
            &["identity", "release"],
            &uncertain,
        )
        .map(|_| ())
    } else {
        Ok(())
    };
    if uncertain.load(Ordering::SeqCst) {
        return Err("uncertain release; preserve process".into());
    }
    let stopped = stop(&host, &mut child);
    save(
        "closeout.json",
        &json!({"preparation_succeeded":outcome.is_ok(),"ai_released":release.is_ok(),"normal_stop_and_exit":stopped.is_ok(),"runtime_pid":child.id(),"preparation_error":outcome.as_ref().err(),"stop_error":stopped.as_ref().err(),"background_watcher":false}),
    )?;
    release?;
    stopped?;
    let baseline = outcome?;
    save("baseline.json", &baseline)?;
    println!("ARCHIVED_REAL_DAY_BASELINE_READY_NOT_ACCEPTED");
    Ok(())
}
fn main() -> Result<()> {
    run()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn metadata_omits_bodies_and_history_requires_exact_positive() {
        let v = json!({"id":"post","no":1,"revision":2,"body":CURRENT,"token":"never-save"});
        let encoded = post_meta(&v).to_string();
        assert!(!encoded.contains(CURRENT));
        assert!(!encoded.contains("never-save"));
        assert!(!history_matches(&json!({"items":[]}), ORIGINAL));
        assert!(history_matches(
            &json!({"items":[{"revision":1,"body":ORIGINAL}]}),
            ORIGINAL
        ));
        assert!(!history_matches(
            &json!({"items":[{"revision":1,"body":CURRENT}]}),
            ORIGINAL
        ));
    }
}
