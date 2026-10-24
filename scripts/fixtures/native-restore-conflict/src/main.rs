//! R18 cause-specific filesystem failure. Production runtime library, finite
//! listener budget, no observer/fault endpoint and no process termination.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use swarm_core::protocol::Request;
use swarm_gui_host::GuiHost;
use swarm_platform::{CredentialStore, pipe};
use swarm_runtime::gui::GuiClient;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261021-native-restore-conflict";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261021-native-restore-conflict/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const CANARY: &str = "restore-delete-conflict-canary";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn ensure(b: bool, m: &str) -> Result<()> {
    if b { Ok(()) } else { Err(m.into()) }
}
fn root() -> &'static Path {
    Path::new(ROOT)
}
fn save(n: &str, v: &Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(Path::new(BASE).join(n))?;
    writeln!(f, "{v}")?;
    Ok(())
}
fn load() -> Result<Value> {
    if Path::new(BASE).join("frozen-prepared.json").exists() {
        return Ok(serde_json::from_slice(&fs::read(
            Path::new(BASE).join("frozen-prepared.json"),
        )?)?);
    }
    Ok(serde_json::from_slice(&fs::read(
        Path::new(BASE).join("prepared.json"),
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
fn created(h: &mut GuiHost, op: &str, args: Value) -> Result<Value> {
    let v = h.create(op, args)?;
    h.acknowledge_create()?;
    Ok(v)
}
fn selected() -> Result<String> {
    let v: Value = serde_json::from_slice(
        &CredentialStore::open(&root().join("gui-state"))?
            .get("selection")?
            .ok_or("selection absent")?,
    )?;
    Ok(v["alias"].as_str().ok_or("alias absent")?.into())
}
fn wait_ready() -> Result<()> {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        if GuiClient::open(root())
            .and_then(|c| c.setup_status())
            .is_ok()
        {
            return Ok(());
        }
        ensure(Instant::now() < until, "readiness deadline")?;
        std::thread::sleep(Duration::from_millis(50));
    }
}
fn prepare(continuation: bool) -> Result<()> {
    if continuation {
        ensure(
            root().is_dir()
                && !Path::new(BASE).join("prepared.json").exists()
                && !Path::new(BASE).join("attempt.json").exists(),
            "continuation refused",
        )?;
    } else {
        ensure(
            !Path::new(BASE).exists(),
            "existing root refused; preserve all evidence",
        )?;
    }
    let exe = Path::new(CANDIDATE).join("swarm-desktop.exe");
    ensure(
        format!("{:x}", Sha256::digest(fs::read(&exe)?))
            == "ffbc41afad0c2a29246b7471b95b1068a40b3052d65ae3944e1547693340209a",
        "frozen GUI changed",
    )?;
    if !continuation {
        fs::create_dir(BASE)?;
        fs::create_dir(ROOT)?;
        swarm_runtime::startup_preference_set(root(), false)?;
    }
    let prefix = if continuation {
        "runtime-repaired"
    } else {
        "runtime"
    };
    let runtime_exe = Path::new(BASE).join(format!("{prefix}.exe"));
    ensure(!runtime_exe.exists(), "runtime snapshot exists")?;
    fs::copy(std::env::current_exe()?, &runtime_exe)?;
    let mut cmd = Command::new(runtime_exe);
    cmd.arg("--serve")
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
        cmd.creation_flags(0x08000000);
    }
    let child = cmd.spawn()?;
    save(
        &format!("{prefix}.json"),
        &json!({"pid":child.id(),"production_library":true,"connections_per_listener":256,"frozen_runtime_executable":false}),
    )?;
    wait_ready()?;
    let mut h = GuiHost::open(root())?;
    let account = h.authenticate("setup", "RestoreConflictFixture", "")?["account"]["id"].clone();
    let aid = account.as_str().ok_or("id absent")?;
    h.backup_control(
        aid,
        "settings.set",
        json!({"enabled":false,"interval_seconds":86400,"retention":2,"destination":null}),
    )?;
    h.manage(
        "display.settings.set",
        json!({"language":"ja","theme":"light"}),
    )?;
    let board = created(
        &mut h,
        "board.create",
        json!({"name":"R18_FILESYSTEM_CONFLICT_BOARD"}),
    )?;
    let thread = created(
        &mut h,
        "thread.create",
        json!({"board_id":board["id"],"name":"R18_LATEST_STATE_MUST_SURVIVE"}),
    )?;
    let archived = created(
        &mut h,
        "post.create",
        json!({"thread_id":thread["id"],"body":"R18_ARCHIVED_SYNTHETIC"}),
    )?;
    let archive = h.backup_control(aid, "manual.create", json!({}))?;
    h.backup_control(aid, "manual.ack", json!({"intent":archive["intent"]}))?;
    let live=h.edit(json!({"thread_id":thread["id"],"no":1,"if_revision":1,"body":"R18_LIVE_SYNTHETIC_MUST_SURVIVE"}))?;
    let extra = created(
        &mut h,
        "post.create",
        json!({"thread_id":thread["id"],"body":"R18_LIVE_ONLY_POST2"}),
    )?;
    let alias = selected()?;
    let c = GuiClient::open(root())?;
    let cursor = c.request(&alias, req("changes.get", json!({})))?["change_cursor"].clone();
    CredentialStore::open(&Path::new(BASE).join("PrivateEvidence"))?
        .put("pre-restore-cursor", &serde_json::to_vec(&cursor)?)?;
    let path = root().join("backups").join(format!(
        "{}.backup",
        archive["key"].as_str().ok_or("archive key absent")?
    ));
    let selection = h.select_restore_backup(aid, &path)?;
    for t in selection["preview"]["targets"]
        .as_array()
        .ok_or("targets absent")?
    {
        ensure(
            Path::new(t["target_path"].as_str().ok_or("target absent")?)
                .starts_with(fs::canonicalize(BASE)?),
            "non-owned restore target",
        )?;
    }
    let prepared = h.prepare_restore(aid, true)?;
    ensure(prepared["state"] == "prepared", "restore not prepared")?;
    save(
        "prepared.json",
        &json!({"account":account,"thread":thread["id"],"live":live,"extra":extra,"archived":archived,"prepared":prepared,"old_alias":alias,"same_incident":"native-restore-20261016","next_attempt":3,"attempt_consumed":false,"vm":false,"elapsed":false,"publication":false}),
    )?;
    println!("PREPARED: owned restore destinations, latest two Posts retained, no apply/fault yet");
    Ok(())
}
fn arm() -> Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    let p = load()?;
    ensure(p["prepared"]["state"] == "prepared", "not prepared")?;
    let continued = Path::new(BASE).join("listener-repair.json").exists();
    let (armed, release, released) = if Path::new(BASE).join("frozen-prepared.json").exists() {
        (
            "armed-frozen.json",
            "release-lock-frozen",
            "lock-released-frozen.json",
        )
    } else if continued {
        (
            "armed-after-listener-repair.json",
            "release-lock-after-listener-repair",
            "lock-released-after-listener-repair.json",
        )
    } else {
        ("armed.json", "release-lock", "lock-released.json")
    };
    ensure(
        Path::new(BASE).join("attempt.json").exists() && !Path::new(BASE).join(armed).exists(),
        "missing attempt record or lock already armed",
    )?;
    let store = CredentialStore::open(&root().join("credentials"))?;
    store.put(CANARY, b"synthetic")?;
    let path = root()
        .join("credentials")
        .join(format!("{:x}.bin", Sha256::digest(CANARY.as_bytes())));
    let lease = OpenOptions::new().read(true).share_mode(1).open(path)?;
    ensure(
        store.get(CANARY)?.as_deref() == Some(b"synthetic"),
        "canary read failed",
    )?;
    save(
        armed,
        &json!({"read_allowed":true,"delete_blocked":true,"deadline_seconds":180,"fault_endpoint":false,"active_injection":false}),
    )?;
    let until = Instant::now() + Duration::from_secs(180);
    while !Path::new(BASE).join(release).exists() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    let timed_out = !Path::new(BASE).join(release).exists();
    drop(lease);
    save(released, &json!({"released":true,"timed_out":timed_out}))?;
    Ok(())
}
fn inspect() -> Result<()> {
    let p = load()?;
    let journal = CredentialStore::open(&root().join("restore-transactions"))?;
    let bytes = journal
        .get("active")?
        .ok_or("no active record; not failure evidence")?;
    let t: Value = serde_json::from_slice(&bytes)?;
    ensure(
        t["intent"] == p["prepared"]["preparation"]["intent"] && t["state"] == "active",
        "wrong active transaction",
    )?;
    let e = GuiClient::open(root())?
        .request(
            p["old_alias"].as_str().ok_or("alias absent")?,
            req("post.list", json!({"thread_id":p["thread"]})),
        )
        .unwrap_err();
    ensure(
        e.code == "RESTORE_RECOVERY_REQUIRED"
            || (e.code == "CONNECTION_ERROR"
                && swarm_runtime::lifecycle::status(root())?["state"] == "not_running"),
        "ordinary operation not fenced or drained",
    )?;
    save(
        if Path::new(BASE).join("frozen-prepared.json").exists() {
            "frozen-failure-observed.json"
        } else {
            "failure-observed.json"
        },
        &json!({"active_retained":true,"ordinary_request_error":e.code,"terminal_absent":journal.get(&format!("result-{}",t["intent"].as_str().ok_or("intent absent")?))?.is_none(),"safety_hash":t["safety_sha256"]}),
    )?;
    Ok(())
}
fn drain() -> Result<()> {
    ensure(
        Path::new(BASE).join("lock-released.json").exists(),
        "lock still held",
    )?;
    // Empty connections are ordinary EOF, not stop requests or force. Each
    // production listener exits naturally at its configured connection limit.
    let names = [
        swarm_runtime::pipe_name(root())?,
        swarm_runtime::gui_pipe_name(root())?,
    ];
    let workers = names
        .into_iter()
        .map(|name| {
            std::thread::spawn(move || {
                let start = Instant::now();
                let mut count = 0;
                while let Ok(c) = pipe::connect(&name) {
                    drop(c);
                    count += 1;
                    if start.elapsed() > Duration::from_secs(55) {
                        return None;
                    }
                    std::thread::sleep(Duration::from_millis(1));
                }
                Some(count)
            })
        })
        .collect::<Vec<_>>();
    let counts = workers
        .into_iter()
        .map(|w| w.join().ok().flatten().ok_or("drain deadline"))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    save(
        if Path::new(BASE).join("listeners-drained.json").exists() {
            "listeners-drained-after-failure.json"
        } else {
            "listeners-drained.json"
        },
        &json!({"empty_connections":counts,"no_termination":true}),
    )?;
    Ok(())
}
fn verify() -> Result<()> {
    let p = load()?;
    wait_ready()?;
    let j = CredentialStore::open(&root().join("restore-transactions"))?;
    ensure(j.get("active")?.is_none(), "active still present")?;
    let key = format!(
        "result-{}",
        p["prepared"]["preparation"]["intent"]
            .as_str()
            .ok_or("intent absent")?
    );
    let t: Value = serde_json::from_slice(&j.get(&key)?.ok_or("no terminal")?)?;
    ensure(t["state"] == "rolled_back", "not rolled back")?;
    let c = GuiClient::open(root())?;
    ensure(
        c.request(
            p["old_alias"].as_str().ok_or("alias absent")?,
            req("account.status", json!({})),
        )
        .unwrap_err()
        .code
            == "AUTHENTICATION_REQUIRED",
        "old alias accepted",
    )?;
    let mut h = GuiHost::open(root())?;
    h.authenticate("login", "RestoreConflictFixture", "")?;
    let posts = h.read("post.list", json!({"thread_id":p["thread"]}))?;
    let items = posts["items"].as_array().ok_or("posts absent")?;
    ensure(items.len() == 2, "post count changed")?;
    for expected in [&p["live"], &p["extra"]] {
        let actual = items
            .iter()
            .find(|x| x["id"] == expected["id"])
            .ok_or("live id lost")?;
        for field in ["id", "body", "revision", "no"] {
            ensure(actual[field] == expected[field], "live post changed")?;
        }
    }
    let cursor: Value = serde_json::from_slice(
        &CredentialStore::open(&Path::new(BASE).join("PrivateEvidence"))?
            .get("pre-restore-cursor")?
            .ok_or("cursor absent")?,
    )?;
    ensure(
        c.request(&selected()?, req("changes.get", json!({"after":cursor})))
            .unwrap_err()
            .code
            == "RESYNC_REQUIRED",
        "old cursor accepted",
    )?;
    save(
        if Path::new(BASE).join("frozen-prepared.json").exists() {
            "frozen-recovered.json"
        } else {
            "recovered.json"
        },
        &json!({"state":"rolled_back","active_removed":true,"two_live_posts_unchanged":true,"old_human_alias_rejected":true,"old_cursor_rejected":true,"reauthentication_via_api":true,"native_relogin_not_claimed":true}),
    )?;
    Ok(())
}
fn run() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--serve") => {
            swarm_runtime::serve_with_gui(root(), Some(4096), Some(4096))?;
            Ok(())
        }
        Some("--prepare") => prepare(false),
        Some("--prepare-frozen") => {
            ensure(
                Path::new(BASE).join("recovered.json").exists()
                    && !Path::new(BASE).join("frozen-prepared.json").exists(),
                "first recovery not verified or frozen already prepared",
            )?;
            let p = load()?;
            let mut h = GuiHost::open(root())?;
            let aid = p["account"].as_str().ok_or("account absent")?;
            ensure(
                h.restore_status(aid)?["state"] == "rolled_back",
                "previous terminal mismatch",
            )?;
            h.clear_restore_selection(aid)?;
            h.select_restore_backup(
                aid,
                Path::new(
                    p["prepared"]["path"]
                        .as_str()
                        .ok_or("archive path absent")?,
                ),
            )?;
            let prepared = h.prepare_restore(aid, true)?;
            ensure(prepared["state"] == "prepared", "not prepared")?;
            let cursor = GuiClient::open(root())?
                .request(&selected()?, req("changes.get", json!({})))?["change_cursor"]
                .clone();
            CredentialStore::open(&Path::new(BASE).join("PrivateEvidence"))?
                .put("pre-restore-cursor", &serde_json::to_vec(&cursor)?)?;
            let mut next = p;
            next["prepared"] = prepared;
            next["old_alias"] = json!(selected()?);
            next["frozen_runtime_executable"] = json!(true);
            save("frozen-prepared.json", &next)?;
            println!("FROZEN PREPARED: same archive/Data, new intent, no apply");
            Ok(())
        }
        Some("--restart-unapplied-listener") => {
            ensure(
                Path::new(BASE).join("listeners-drained.json").exists(),
                "old listeners not drained",
            )?;
            let j = CredentialStore::open(&root().join("restore-transactions"))?;
            ensure(j.get("active")?.is_none(), "apply actually started")?;
            let p = load()?;
            ensure(
                j.get(&format!(
                    "result-{}",
                    p["prepared"]["preparation"]["intent"]
                        .as_str()
                        .ok_or("intent absent")?
                ))?
                .is_none(),
                "apply completed",
            )?;
            let runtime = Path::new(BASE).join("runtime-listener-repair.exe");
            ensure(!runtime.exists(), "repair already dispatched")?;
            fs::copy(std::env::current_exe()?, &runtime)?;
            let mut c = Command::new(runtime);
            c.arg("--serve")
                .stdin(Stdio::null())
                .stdout(
                    OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(Path::new(BASE).join("runtime-listener-repair.stdout.log"))?,
                )
                .stderr(
                    OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(Path::new(BASE).join("runtime-listener-repair.stderr.log"))?,
                );
            {
                use std::os::windows::process::CommandExt;
                c.creation_flags(0x08000000);
            }
            let child = c.spawn()?;
            save(
                "listener-repair.json",
                &json!({"pid":child.id(),"connections_per_listener":4096,"prior_active_absent":true,"prior_terminal_absent":true,"same_data":true,"new_fault_attempt":false}),
            )?;
            wait_ready()?;
            Ok(())
        }
        Some("--diagnose") => {
            let mut h = GuiHost::open(root())?;
            let s = h.snapshot()?;
            let aid = s["account"]["id"].as_str();
            println!(
                "{}",
                json!({"account_selected":aid.is_some(),"setup":GuiClient::open(root())?.setup_status()?,"boards":h.read("board.list",json!({})).ok().map(|v|v["items"].as_array().map(Vec::len)),"restore":aid.and_then(|a|h.restore_status(a).ok()).map(|v|v["state"].clone())})
            );
            Ok(())
        }
        Some("--finish-unprepared") => {
            ensure(
                !Path::new(BASE).join("prepared.json").exists()
                    && !Path::new(BASE).join("attempt.json").exists(),
                "already prepared/applied",
            )?;
            let mut h = GuiHost::open(root())?;
            let s = h.snapshot()?;
            let account = s["account"]["id"].clone();
            let aid = account.as_str().ok_or("no account")?;
            let boards = h.read("board.list", json!({}))?;
            let b = &boards["items"][0];
            ensure(
                boards["items"].as_array().is_some_and(|v| v.len() == 1)
                    && b["name"] == "R18_FILESYSTEM_CONFLICT_BOARD",
                "unexpected board",
            )?;
            let threads = h.read("thread.list", json!({"board_id":b["id"]}))?;
            let t = &threads["items"][0];
            ensure(
                threads["items"].as_array().is_some_and(|v| v.len() == 1)
                    && t["name"] == "R18_LATEST_STATE_MUST_SURVIVE",
                "unexpected thread",
            )?;
            let posts = h.read("post.list", json!({"thread_id":t["id"]}))?;
            let items = posts["items"].as_array().ok_or("posts absent")?;
            ensure(items.len() == 2, "unexpected posts")?;
            let live = items.iter().find(|v| v["no"] == 1).ok_or("live absent")?;
            let extra = items.iter().find(|v| v["no"] == 2).ok_or("extra absent")?;
            ensure(
                live["revision"] == 2
                    && live["body"] == "R18_LIVE_SYNTHETIC_MUST_SURVIVE"
                    && extra["body"] == "R18_LIVE_ONLY_POST2",
                "live differs",
            )?;
            let selection = h.restore_status(aid)?;
            ensure(selection["state"] == "selected", "not unprepared")?;
            for target in selection["preview"]["targets"]
                .as_array()
                .ok_or("targets absent")?
            {
                ensure(
                    Path::new(target["target_path"].as_str().ok_or("path absent")?)
                        .starts_with(fs::canonicalize(BASE)?),
                    "non-owned target",
                )?;
            }
            let prepared = h.prepare_restore(aid, true)?;
            ensure(prepared["state"] == "prepared", "not prepared")?;
            save(
                "prepared.json",
                &json!({"account":account,"thread":t["id"],"live":live,"extra":extra,"prepared":prepared,"old_alias":selected()?,"same_incident":"native-restore-20261016","next_attempt":3,"attempt_consumed":false,"vm":false,"elapsed":false,"publication":false,"prior_preparation_failures_preserved":true}),
            )?;
            println!(
                "PREPARED: existing two Posts and archive reused; no repeat setup/edit/backup"
            );
            Ok(())
        }
        Some("--continue-unstarted") => prepare(true),
        Some("--drain-unstarted") => {
            ensure(
                !Path::new(BASE).join("attempt.json").exists()
                    && !Path::new(BASE).join("prepared.json").exists(),
                "unstarted guard",
            )?;
            let name = swarm_runtime::pipe_name(root())?;
            let until = Instant::now() + Duration::from_secs(20);
            let mut count = 0;
            while let Ok(c) = pipe::connect(&name) {
                drop(c);
                count += 1;
                ensure(Instant::now() < until, "drain deadline")?;
                std::thread::sleep(Duration::from_millis(5));
            }
            save(
                "unstarted-drained.json",
                &json!({"connections":count,"force":false,"fault_attempt":false}),
            )
        }
        Some("--arm") => arm(),
        Some("--inspect") => inspect(),
        Some("--drain") => drain(),
        Some("--verify") => verify(),
        Some("--record-apply") => save(
            "attempt.json",
            &json!({"same_incident":"native-restore-20261016","attempt":3,"mechanism":"readable-credential-delete-conflict","vm":false,"elapsed":false}),
        ),
        Some("--stop") => {
            GuiClient::open(root())?.stop(&selected()?)?;
            Ok(())
        }
        Some("--cleanup-registration") => {
            let r = swarm_platform::notifications::Registration::new(
                swarm_platform::notifications::Identity::new(
                    root(),
                    &Path::new(CANDIDATE).join("swarm-desktop.exe"),
                )?,
            )?;
            r.remove()?;
            ensure(!r.is_registered()?, "registration remains")
        }
        _ => Err("unknown mode; no operation".into()),
    }
}
fn main() {
    if let Err(e) = run() {
        if let Some(f) = e.downcast_ref::<swarm_core::protocol::Fault>() {
            eprintln!("product fault code={}", f.code);
        }
        eprintln!(
            "R18 helper failed; preserve receipts and inspect source/state before continuing"
        );
        std::process::exit(1);
    }
}
