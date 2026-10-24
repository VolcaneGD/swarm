//! Fixed existing archive baseline. Plan is filesystem-only; execution waits a real day.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use swarm_core::protocol::Request;
use swarm_platform::CredentialStore;
use swarm_runtime::gui::GuiClient;

type Result<T> = std::result::Result<T, String>;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261023-archived-retention";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261023-archived-retention/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const ALIAS: &str = "archived-retention-20261023";
const BASELINE_SHA: &str = "503786a53651801ccd21e72a2199513d7424f5c5235cf73c06016ae17aac7e13";
const RULES_SHA: &str = "acd1096cc313bc24107b64ce3e74364a974b2e4b84306f6e03d4bfaacd355ad5";
const NOT_BEFORE: u64 = 1792815821;

fn require(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(message.into()) }
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| "clock unavailable".into())
}
fn ready(at: u64) -> bool {
    at >= NOT_BEFORE
}
fn read(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    fs::read(path).map_err(|_| "fixed input unavailable".into())
}
fn save(name: &str, value: Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(Path::new(BASE).join(name))
        .map_err(|_| "preserve existing observation receipt")?;
    writeln!(f, "{value}").map_err(|_| "receipt write failed".into())
}
fn inputs() -> Result<Value> {
    for fixed in [BASE, ROOT, CANDIDATE] {
        require(
            fs::canonicalize(fixed)
                .map_err(|_| "fixed directory missing")?
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .replace('\\', "/")
                .eq_ignore_ascii_case(fixed),
            "redirected fixed directory",
        )?;
    }
    let bytes = read(Path::new(BASE).join("baseline.json"))?;
    require(digest(&bytes) == BASELINE_SHA, "baseline changed")?;
    require(
        digest(&read(Path::new(ROOT).join("RULES.md"))?) == RULES_SHA,
        "RULES changed",
    )?;
    for (name, sha) in [
        (
            "swarm-runtime.exe",
            "0cf3ecb753c20830be7edf2c3630b3cc07aac90655d963b1097374f8b2b24210",
        ),
        (
            "swarm.exe",
            "d23b7973d229bea186d4aec95eca3a6f7f2868f7243534156372e5e438ecfd33",
        ),
    ] {
        require(
            digest(&read(Path::new(CANDIDATE).join(name))?) == sha,
            "frozen candidate changed",
        )?;
    }
    let b: Value = serde_json::from_slice(&bytes).map_err(|_| "baseline JSON invalid")?;
    require(
        b["root"] == ROOT
            && b["saved_alias"] == ALIAS
            && b["observe_not_before_unix_seconds"] == NOT_BEFORE,
        "baseline identity/time mismatch",
    )?;
    Ok(b)
}
// Separate transport uncertainty from a definitive NOT_FOUND. Never accept empty history.
fn missing(value: &Result<Value>) -> bool {
    matches!(value, Err(code) if code == "NOT_FOUND")
}
fn live_matches(value: &Value, b: &Value) -> bool {
    value["id"] == b["id"]
        && value["no"] == b["no"]
        && value["revision"] == b["revision"]
        && value["deleted"] == false
        && value["reply_to"] == b["reply_to"]
        && value["body"]
            .as_str()
            .is_some_and(|s| b["body_sha256"] == digest(s.as_bytes()))
}
fn history_matches(value: &Value) -> bool {
    value["items"].as_array().is_some_and(|items| {
        items.len() == 1
            && items[0]["revision"] == 1
            && items[0]["body"].as_str().is_some_and(|s| {
                digest(s.as_bytes()) == digest(b"ARCHIVERETENTIONLIVEORIGINAL20261023")
            })
    })
}
fn private(alias: &str, op: &str, args: Value, deadline: Instant) -> Result<Value> {
    let (tx, rx) = mpsc::channel();
    let alias = alias.to_owned();
    let op = op.to_owned();
    std::thread::spawn(move || {
        let r = GuiClient::open(Path::new(ROOT))
            .map_err(|_| "GUI_OPEN_FAILED".into())
            .and_then(|c| {
                if op == "gui.runtime_stop" {
                    c.stop(&alias)
                } else {
                    c.request(
                        &alias,
                        Request {
                            operation: op,
                            auth: None,
                            args,
                            context: json!({}),
                            request_id: None,
                        },
                    )
                }
                .map_err(|f| f.code.to_string())
            });
        let _ = tx.send(r);
    });
    rx.recv_timeout(
        deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(20)),
    )
    .map_err(|_| "UNCERTAIN_PRIVATE_REQUEST".to_string())?
}
struct Session {
    deadline: Instant,
    uncertain: bool,
    ai_active: bool,
    project: String,
}
impl Session {
    fn cli(&mut self, args: &[&str]) -> Result<Value> {
        require(Instant::now() < self.deadline, "observation deadline")?;
        let mut cmd = Command::new(Path::new(CANDIDATE).join("swarm.exe"));
        cmd.args([
            "--data-dir",
            ROOT,
            "--project",
            &self.project,
            "--session",
            ALIAS,
            "--format",
            "json",
        ])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd.spawn().map_err(|_| "CLI_START_FAILED")?;
        let pid = child.id();
        // Drain both pipes while the process runs, with a strict memory bound.
        let out = child.stdout.take().ok_or("CLI stdout unavailable")?;
        let err = child.stderr.take().ok_or("CLI stderr unavailable")?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut a = Vec::new();
            let mut b = Vec::new();
            let t = std::thread::spawn(move || err.take(65537).read_to_end(&mut b).map(|_| b));
            let r = out
                .take(65537)
                .read_to_end(&mut a)
                .and_then(|_| {
                    t.join()
                        .unwrap_or_else(|_| Err(std::io::Error::other("reader failed")))
                })
                .map(|b| (a, b));
            let _ = tx.send(r);
        });
        let bound = self.deadline.min(Instant::now() + Duration::from_secs(20));
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if Instant::now() < bound => std::thread::sleep(Duration::from_millis(25)),
                _ => {
                    self.uncertain = true;
                    let _ = save(
                        "observation-uncertain-cli.json",
                        json!({"pid":pid,"forced_exit":false}),
                    );
                    return Err("UNCERTAIN_CLI_COMPLETION".into());
                }
            }
        };
        let (stdout, stderr) = rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| {
                self.uncertain = true;
                "UNCERTAIN_CLI_STREAMS"
            })?
            .map_err(|_| "CLI_STREAM_FAILED")?;
        require(
            stdout.len() <= 65536 && stderr.is_empty(),
            "CLI_STREAM_CONTRACT",
        )?;
        let envelope: Value = serde_json::from_slice(&stdout).map_err(|_| "CLI_JSON_INVALID")?;
        require(envelope["schema_version"] == 1, "CLI_SCHEMA_INVALID")?;
        if status.code() == Some(0) && envelope["ok"] == true {
            return Ok(envelope["data"].clone());
        }
        if status.code() == Some(5)
            && envelope["ok"] == false
            && envelope["error"]["code"] == "NOT_FOUND"
        {
            return Err("NOT_FOUND".into());
        }
        Err(format!("CLI_FAILURE exit={:?}", status.code()))
    }
    fn private(&mut self, alias: &str, op: &str, args: Value) -> Result<Value> {
        let r = private(alias, op, args, self.deadline);
        if r.as_ref().is_err_and(|e| e.starts_with("UNCERTAIN_")) {
            self.uncertain = true;
        }
        r
    }
    fn readsets(&mut self, alias: &str, b: &Value) -> Result<()> {
        let a = b["archived_thread"]["id"]
            .as_str()
            .ok_or("archive ID missing")?;
        let l = b["live_thread"]["id"].as_str().ok_or("live ID missing")?;
        require(
            self.private(alias, "thread.get", json!({"id":l}))? == b["live_thread"],
            "private live Thread changed",
        )?;
        require(
            live_matches(
                &self.private(alias, "post.get", json!({"thread_id":l,"no":1}))?,
                &b["live_post"],
            ),
            "private live Post changed",
        )?;
        require(
            history_matches(&self.private(alias, "post.history", json!({"thread_id":l,"no":1}))?),
            "private live history changed",
        )?;
        require(
            missing(&self.private(alias, "thread.get", json!({"id":a}))),
            "private archive Thread must be NOT_FOUND",
        )?;
        require(
            missing(&self.private(alias, "post.list", json!({"thread_id":a}))),
            "private archive list must be NOT_FOUND",
        )?;
        for no in [1, 2, 1000] {
            require(
                missing(&self.private(alias, "post.get", json!({"thread_id":a,"no":no}))),
                "private archive Post must be NOT_FOUND",
            )?;
        }
        require(
            missing(&self.private(alias, "post.history", json!({"thread_id":a,"no":1}))),
            "private archive history must be NOT_FOUND",
        )?;
        save(
            "observation-private.json",
            json!({"thread_list_and_post_sentinels_not_found":true,"sentinel_nos":[1,2,1000],"history_not_found":true,"live_preserved":true}),
        )?;
        let actor = b["actor_id"].as_str().ok_or("actor missing")?;
        self.ai_active = true;
        let resumed = self.cli(&["identity", "resume", "--actor-id", actor])?;
        require(
            resumed["identity"]["id"] == b["actor_id"],
            "resumed a different public actor",
        )?;
        // Identity metadata was checked before launch; resume never registers a new actor.
        require(
            self.cli(&["thread", "get", "--id", l])? == b["live_thread"],
            "public live Thread changed",
        )?;
        require(
            live_matches(
                &self.cli(&["post", "get", "--thread-id", l, "--no", "1"])?,
                &b["live_post"],
            ),
            "public live Post changed",
        )?;
        require(
            history_matches(&self.cli(&["post", "history", "--thread-id", l, "--no", "1"])?),
            "public live history changed",
        )?;
        require(
            missing(&self.cli(&["thread", "get", "--id", a])),
            "public archive Thread must be NOT_FOUND",
        )?;
        require(
            missing(&self.cli(&["post", "list", "--thread-id", a])),
            "public archive list must be NOT_FOUND",
        )?;
        for no in ["1", "2", "1000"] {
            require(
                missing(&self.cli(&["post", "get", "--thread-id", a, "--no", no])),
                "public archive Post must be NOT_FOUND",
            )?;
        }
        require(
            missing(&self.cli(&["post", "history", "--thread-id", a, "--no", "1"])),
            "public archive history must be NOT_FOUND",
        )?;
        save(
            "observation-public.json",
            json!({"same_project_actor_alias":true,"thread_list_and_post_sentinels_not_found":true,"sentinel_nos":[1,2,1000],"history_not_found":true,"live_preserved":true}),
        )?;
        Ok(())
    }
}
fn saved_metadata(b: &Value) -> Result<String> {
    let gui = CredentialStore::open(&Path::new(ROOT).join("gui-state"))
        .map_err(|_| "GUI store unavailable")?;
    for key in ["pending-auth", "pending-create"] {
        require(
            gui.get(key)
                .map_err(|_| "pending state unavailable")?
                .is_none(),
            "pending operation; preserve state",
        )?;
    }
    let selected: Value = serde_json::from_slice(
        &gui.get("selection")
            .map_err(|_| "selection unavailable")?
            .ok_or("saved selection required")?,
    )
    .map_err(|_| "selection invalid")?;
    // Selection contains only an alias; account identity is checked through account.status.
    require(
        selected.as_object().is_some_and(|o| o.len() == 1) && selected["alias"].is_string(),
        "saved selection invalid",
    )?;
    let store = CredentialStore::open(&Path::new(ROOT).join("client-credentials"))
        .map_err(|_| "AI store unavailable")?;
    let saved: Value = serde_json::from_slice(
        &store
            .get(ALIAS)
            .map_err(|_| "alias unavailable")?
            .ok_or("existing alias required")?,
    )
    .map_err(|_| "alias invalid")?;
    require(
        saved["kind"] == "ai"
            && saved["id"] == b["actor_id"]
            && saved["project_id"] == b["project_id"],
        "saved public identity mismatch",
    )?;
    Ok(selected["alias"]
        .as_str()
        .ok_or("human alias missing")?
        .into())
}
fn normal_exit(child: &mut Child) -> Result<i32> {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait().map_err(|_| "runtime status unavailable")? {
            require(status.success(), "runtime exit failed")?;
            return status.code().ok_or("runtime exit code unavailable".into());
        }
        require(
            Instant::now() < end,
            "runtime exit deadline; preserve process",
        )?;
        std::thread::sleep(Duration::from_millis(40));
    }
}
fn observe(b: &Value) -> Result<()> {
    // Caller passed real time and all immutable inputs before any store is opened.
    let alias = saved_metadata(b)?;
    let pipe =
        swarm_runtime::pipe_name(Path::new(ROOT)).map_err(|_| "pipe identity unavailable")?;
    require(
        swarm_platform::pipe::connect(&pipe).is_err(),
        "existing runtime; do not duplicate or claim ownership",
    )?;
    save(
        "observation-start.json",
        json!({"at_unix_seconds":now()?,"baseline_sha256":BASELINE_SHA,"standing_authorization_20261023":true,"reseed":false}),
    )?;
    let mut cmd = Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
    cmd.args(["--data-dir", ROOT, "--human-start"])
        .stdin(Stdio::null());
    for (name, out) in [
        ("observation-runtime.stdout.log", true),
        ("observation-runtime.stderr.log", false),
    ] {
        let f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(Path::new(BASE).join(name))
            .map_err(|_| "preserve stream receipt")?;
        if out {
            cmd.stdout(f);
        } else {
            cmd.stderr(f);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().map_err(|_| "runtime start failed")?;
    save(
        "observation-runtime.json",
        json!({"pid":child.id(),"candidate":CANDIDATE}),
    )?;
    let mut s = Session {
        deadline: Instant::now() + Duration::from_secs(180),
        uncertain: false,
        ai_active: false,
        project: b["project_id"].as_str().ok_or("Project missing")?.into(),
    };
    // Connection readiness only; no observation is replayed.
    let end = Instant::now() + Duration::from_secs(15);
    let checks = (|| {
        loop {
            match s.private(&alias, "account.status", json!({})) {
                Ok(account) => {
                    require(
                        account["id"] == b["account_id"] && account["is_admin"] == true,
                        "baseline Admin mismatch",
                    )?;
                    break;
                }
                Err(e)
                    if !s.uncertain
                        && Instant::now() < end
                        && ["GUI_OPEN_FAILED", "CONNECTION_ERROR"].contains(&e.as_str()) =>
                {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(e) => return Err(e),
            }
        }
        require(
            s.private(&alias, "retention.settings.get", json!({}))? == b["settings"],
            "retention settings changed",
        )?;
        let project = s.private(&alias, "project.get", json!({"id":s.project.clone()}))?;
        require(
            project["name"] == "Archived retention readback"
                && project["canonical_root"].as_str().is_some_and(|p| {
                    p.trim_start_matches(r"\\?\")
                        .replace('\\', "/")
                        .eq_ignore_ascii_case(&format!("{BASE}/PublicReadbackProject"))
                }),
            "registered project changed",
        )?;
        s.readsets(&alias, b)
    })();
    // Definite failures still get a normal release/stop. Uncertain mutations preserve owners.
    s.deadline = Instant::now() + Duration::from_secs(40);
    let mut released = !s.ai_active;
    if s.ai_active && !s.uncertain {
        released = s.cli(&["identity", "release"]).is_ok();
    }
    let stopped = if !s.uncertain && released {
        private(
            &alias,
            "gui.runtime_stop",
            json!({}),
            Instant::now() + Duration::from_secs(20),
        )
        .and_then(|_| normal_exit(&mut child))
    } else {
        Err("uncertain operation or unreleased identity; preserve runtime".into())
    };
    let pins = inputs().is_ok();
    let stream_bytes = ["stdout", "stderr"].map(|n| {
        fs::metadata(Path::new(BASE).join(format!("observation-runtime.{n}.log")))
            .map(|m| m.len())
            .ok()
    });
    let pass = checks.is_ok() && released && stopped.is_ok() && pins;
    save(
        "observation-result.json",
        json!({"status":if pass {"SCOPED_API_PASS_NATIVE_PENDING"}else{"FAILED_OR_PARTIAL"},"checks_error":checks.as_ref().err(),"ai_released":released,"normal_exit_code":stopped.as_ref().ok(),"closeout_error":stopped.as_ref().err(),"baseline_rules_candidate_preserved":pins,"stream_bytes":stream_bytes,"native_gui_observed":false,"formal_ac_accepted":false,"uncertain":s.uncertain,"runtime_pid":child.id()}),
    )?;
    require(
        pass,
        "observation incomplete; inspect phase receipts and preserved state",
    )
}
fn main() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    require(
        args == ["--plan"] || args == ["--observe"],
        "fixed --plan or --observe only; no root/time override",
    )?;
    let b = inputs()?;
    let at = now()?;
    if args == ["--plan"] || !ready(at) {
        println!(
            "{}",
            json!({"status":if ready(at){"READY_FOR_AUTHORIZED_OBSERVATION"}else{"WAIT_REAL_ELAPSED_TIME"},"not_before_unix_seconds":NOT_BEFORE,"runtime_started":false,"data_stores_opened":false,"fixture_modified":false})
        );
        if !ready(at) {
            std::process::exit(2);
        }
        return Ok(());
    }
    observe(&b)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_day_cutoff_is_inclusive_without_early_window() {
        assert!(!ready(NOT_BEFORE - 1));
        assert!(ready(NOT_BEFORE));
    }
    #[test]
    fn empty_history_denial_and_transport_errors_are_not_purge() {
        assert!(missing(&Err("NOT_FOUND".into())));
        for code in [
            "PERMISSION_DENIED",
            "CONNECTION_ERROR",
            "UNCERTAIN_PRIVATE_REQUEST",
        ] {
            assert!(!missing(&Err(code.into())));
        }
        assert!(!missing(&Ok(json!({"items":[]}))));
    }
    #[test]
    fn live_control_requires_identity_body_number_revision_and_not_deleted() {
        let mut p = json!({"id":"live","no":1,"revision":2,"deleted":false,"reply_to":null,"body":"control"});
        let b = json!({"id":"live","no":1,"revision":2,"reply_to":null,"body_sha256":digest(b"control")});
        assert!(live_matches(&p, &b));
        for (key, bad) in [
            ("id", json!("other")),
            ("no", json!(2)),
            ("revision", json!(3)),
            ("deleted", json!(true)),
            ("body", Value::Null),
            ("reply_to", json!(1)),
        ] {
            let old = p[key].clone();
            p[key] = bad;
            assert!(!live_matches(&p, &b));
            p[key] = old;
        }
    }
    #[test]
    fn history_control_rejects_missing_or_extra_or_changed_entries() {
        let mut h = json!({"items":[{"revision":1,"body":"ARCHIVERETENTIONLIVEORIGINAL20261023"}]});
        assert!(history_matches(&h));
        h["items"][0]["revision"] = json!(2);
        assert!(!history_matches(&h));
        h["items"] = json!([]);
        assert!(!history_matches(&h));
    }
}
