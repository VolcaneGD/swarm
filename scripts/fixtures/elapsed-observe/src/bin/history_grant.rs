//! One newly authorized physical-expiry/history join. Never replay old readsets.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
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
use swarm_core::protocol::Request;
use swarm_platform::{
    CredentialStore,
    notifications::{Identity, Registration},
};
use swarm_runtime::gui::GuiClient;

type Result<T> = std::result::Result<T, String>;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261019-elapsed-approved-extension-2";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261019-elapsed-approved-extension-2/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const ARTIFACT: &str =
    "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/expiry-history-grant-20261023";
const THREAD: &str = "bdd8bf0b-0033-432f-b9de-0f4ca0b86965";
const SESSION: &str = "elapsed-followup-20261020";
const BASELINE_SHA: &str = "1d68c9a0106becf65940dd645c758de3a31fc9588d6dc1d5b4de1342d5c4c644";
const RULES_SHA: &str = "cac66eb9923ff8701603f1dff96019146b703d3f7071775ab01f2c3b9887d3a0";
const PROPOSED_SHA: &str = "853f0d8946f7d5c3864453c8b91093af2d353f68e134c7428e5b9ab0f5cdbd5b";
const ORIGINAL: &str = "SYNTHETIC_EXPIRY_HISTORY_POSITIVE_ORIGINAL_20261023";
const CURRENT: &str = "SYNTHETIC_EXPIRY_HISTORY_POSITIVE_CURRENT_20261023";

fn ensure(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(message.into()) }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    fs::read(path).map_err(|_| "fixed input unavailable".into())
}
fn text(path: impl AsRef<Path>) -> Result<String> {
    String::from_utf8(read(path)?).map_err(|_| "fixed input UTF8 invalid".into())
}
fn json_read(path: impl AsRef<Path>) -> Result<Value> {
    serde_json::from_slice(&read(path)?).map_err(|_| "fixed JSON invalid".into())
}
fn save(name: &str, value: Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(Path::new(BASE).join(format!("history-grant-20261023-{name}.json")))
        .map_err(|_| "preserve existing receipt".to_string())?;
    writeln!(f, "{value}").map_err(|_| "receipt write failed".into())
}
fn inputs() -> Result<Value> {
    for p in [BASE, ROOT, CANDIDATE, ARTIFACT] {
        let canonical = fs::canonicalize(p).map_err(|_| "fixed path missing")?;
        ensure(
            canonical
                .to_string_lossy()
                .trim_start_matches(r"\\?\")
                .replace('\\', "/")
                .eq_ignore_ascii_case(p),
            "redirected fixed path refused",
        )?;
    }
    ensure(
        hash(&read(Path::new(BASE).join("baseline.json"))?) == BASELINE_SHA,
        "baseline changed",
    )?;
    ensure(
        hash(&read(Path::new(ROOT).join("RULES.md"))?) == RULES_SHA,
        "original RULES changed",
    )?;
    ensure(
        hash(&read(Path::new(ARTIFACT).join("proposed-RULES.md"))?) == PROPOSED_SHA,
        "proposal changed",
    )?;
    for (file, sha) in [
        (
            "swarm-runtime.exe",
            "0cf3ecb753c20830be7edf2c3630b3cc07aac90655d963b1097374f8b2b24210",
        ),
        (
            "swarm.exe",
            "d23b7973d229bea186d4aec95eca3a6f7f2868f7243534156372e5e438ecfd33",
        ),
        (
            "swarm-desktop.exe",
            "ffbc41afad0c2a29246b7471b95b1068a40b3052d65ae3944e1547693340209a",
        ),
    ] {
        ensure(
            hash(&read(Path::new(CANDIDATE).join(file))?) == sha,
            "frozen candidate changed",
        )?;
    }
    let b = json_read(Path::new(BASE).join("baseline.json"))?;
    ensure(
        b["live_thread"]["id"] == THREAD
            && b["live_control"]["no"] == 1
            && b["deleted_post"]["no"] == 2,
        "fixed targets changed",
    )?;
    Ok(b)
}
fn approval_ok(a: &Value, self_sha: &str) -> bool {
    a["scope"] == "elapsed-explicit-history-20261023"
        && a["approved"] == true
        && a["standing_authorization"] == true
        && a["executable_sha256"] == self_sha
        && a["baseline_sha256"] == BASELINE_SHA
        && a["original_rules_sha256"] == RULES_SHA
        && a["proposed_rules_sha256"] == PROPOSED_SHA
        && a["data_root"] == ROOT
        && a["temporary_rules_change"] == true
        && a["positive_posts"] == 1
        && a["positive_edits"] == 1
        && a["attempts"] == 1
        && a["automatic_retries"] == 0
        && a["reuse_project_and_alias"] == true
        && a["reseed"] == false
        && a["vm"] == false
        && a["publication"] == false
}
struct Calls {
    deadline: Instant,
    uncertain: Arc<AtomicBool>,
}
impl Calls {
    fn ready(&self) -> Result<()> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(15);
            loop {
                if GuiClient::open(Path::new(ROOT))
                    .and_then(|v| v.setup_status())
                    .is_ok()
                {
                    let _ = tx.send(Ok(()));
                    return;
                }
                if Instant::now() >= until {
                    let _ = tx.send(Err("runtime readiness deadline".to_string()));
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        rx.recv_timeout(Duration::from_secs(16)).map_err(|_| {
            self.uncertain.store(true, Ordering::SeqCst);
            "uncertain readiness IPC; preserve process".to_string()
        })?
    }
    fn remaining(&self) -> Result<Duration> {
        let d = self
            .deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(20));
        ensure(!d.is_zero(), "execution deadline")?;
        Ok(d)
    }
    fn request(&self, alias: &str, op: &str, args: Value) -> Result<Value> {
        let duration = self.remaining()?;
        let (tx, rx) = mpsc::channel();
        let alias = alias.to_owned();
        let op = op.to_owned();
        std::thread::spawn(move || {
            let r = GuiClient::open(Path::new(ROOT))
                .and_then(|c| {
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
                })
                .map_err(|f| f.code.to_string());
            let _ = tx.send(r);
        });
        rx.recv_timeout(duration).map_err(|_| {
            self.uncertain.store(true, Ordering::SeqCst);
            "uncertain private request; preserve process".to_string()
        })?
    }
    fn cli(&self, project: &str, args: &[&str]) -> Result<Value> {
        let duration = self.remaining()?;
        let mut c = Command::new(Path::new(CANDIDATE).join("swarm.exe"));
        c.args([
            "--data-dir",
            ROOT,
            "--project",
            project,
            "--session",
            SESSION,
            "--format",
            "json",
        ])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x08000000);
        }
        let mut child = c.spawn().map_err(|_| "CLI launch failed")?;
        let until = Instant::now() + duration;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(25)),
                _ => {
                    self.uncertain.store(true, Ordering::SeqCst);
                    let _ = save(
                        "uncertain-cli",
                        json!({"pid":child.id(),"operation":args.iter().take(2).copied().collect::<Vec<_>>(),"forced_exit":false}),
                    );
                    return Err("uncertain CLI; preserve process".into());
                }
            }
        }
        let output = child
            .wait_with_output()
            .map_err(|_| "CLI streams unavailable")?;
        let value: Value =
            serde_json::from_slice(&output.stdout).map_err(|_| "CLI JSON invalid")?;
        let code = value["error"]["code"]
            .as_str()
            .filter(|s| s.len() <= 64 && s.bytes().all(|b| b.is_ascii_uppercase() || b == b'_'))
            .unwrap_or("UNAVAILABLE");
        ensure(
            output.status.success()
                && output.stderr.is_empty()
                && output.stdout.len() <= 65536
                && value["schema_version"] == 1
                && value["ok"] == true,
            &format!(
                "CLI completed failure; code {code}; stderr bytes {}",
                output.stderr.len()
            ),
        )?;
        Ok(value["data"].clone())
    }
    fn stop(&self, alias: &str) -> Result<()> {
        let (tx, rx) = mpsc::channel();
        let alias = alias.to_owned();
        std::thread::spawn(move || {
            let r = GuiClient::open(Path::new(ROOT))
                .and_then(|c| c.stop(&alias))
                .map_err(|f| f.code.to_string());
            let _ = tx.send(r);
        });
        rx.recv_timeout(Duration::from_secs(15)).map_err(|_| {
            self.uncertain.store(true, Ordering::SeqCst);
            "uncertain normal stop; preserve process".to_string()
        })??;
        Ok(())
    }
}
fn history_ok(v: &Value) -> bool {
    v["items"].as_array().is_some_and(|a| {
        a.len() == 1
            && a[0]["revision"] == 1
            && a[0]["body"]
                .as_str()
                .is_some_and(|b| hash(b.as_bytes()) == hash(ORIGINAL.as_bytes()))
    })
}
fn original_live(v: &Value, b: &Value) -> bool {
    v["id"] == b["id"]
        && v["no"] == b["no"]
        && v["revision"] == b["revision"]
        && v["body"]
            .as_str()
            .is_some_and(|s| hash(s.as_bytes()) == b["body_sha256"])
}
struct State {
    alias: String,
    project: String,
    ai_active: bool,
    applied: bool,
}
fn selected() -> Result<String> {
    let s = CredentialStore::open(&Path::new(ROOT).join("gui-state"))
        .map_err(|_| "GUI selection store unavailable")?;
    for key in ["pending-auth", "pending-create"] {
        ensure(
            s.get(key)
                .map_err(|_| "pending state unavailable")?
                .is_none(),
            "pending GUI operation",
        )?;
    }
    let bytes = s
        .get("selection")
        .map_err(|_| "selection unavailable")?
        .ok_or("saved selection required")?;
    let v: Value = serde_json::from_slice(&bytes).map_err(|_| "selection JSON invalid")?;
    Ok(v["alias"].as_str().ok_or("saved alias missing")?.into())
}
fn observe(c: &Calls, s: &mut State, b: &Value, original: &str, proposed: &str) -> Result<Value> {
    let account = c.request(&s.alias, "account.status", json!({}))?;
    ensure(
        account["id"] == b["account_id"] && account["is_admin"] == true,
        "baseline Admin not selected",
    )?;
    let store = CredentialStore::open(&Path::new(ROOT).join("client-credentials"))
        .map_err(|_| "saved CLI store unavailable")?;
    let bytes = store
        .get(SESSION)
        .map_err(|_| "saved identity unavailable")?
        .ok_or("existing identity required; no start")?;
    let saved: Value = serde_json::from_slice(&bytes).map_err(|_| "saved identity invalid")?;
    ensure(saved["kind"] == "ai", "saved identity is not AI")?;
    s.project = saved["project_id"]
        .as_str()
        .ok_or("saved Project missing")?
        .into();
    let actor = saved["id"].as_str().ok_or("saved actor missing")?;
    let metadata = c.request(&s.alias, "project.get", json!({"id":s.project}))?;
    let canonical = metadata["canonical_root"]
        .as_str()
        .ok_or("Project root missing")?
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    ensure(
        canonical.eq_ignore_ascii_case(&format!("{BASE}/PublicReadbackProject"))
            && metadata["name"] == "Elapsed retention readback",
        "different saved Project",
    )?;
    let live = c.request(&s.alias, "post.get", json!({"thread_id":THREAD,"no":1}))?;
    ensure(
        original_live(&live, &b["live_control"]),
        "original live Post changed",
    )?;
    let expired = c.request(&s.alias, "post.get", json!({"thread_id":THREAD,"no":2}))?;
    ensure(
        expired["id"] == b["deleted_post"]["id"]
            && expired["deleted"] == true
            && expired["body"].is_null()
            && expired["restore_available"] == false,
        "expired target is not purged skeleton",
    )?;
    let rules = c.request(&s.alias, "rules.get", json!({}))?;
    ensure(
        rules["source"] == original && rules["revision"] == format!("sha256:{RULES_SHA}"),
        "original rules API mismatch",
    )?;
    ensure(
        c.request(&s.alias, "rules.validate", json!({"source":proposed}))?["valid"] == true,
        "proposed rules invalid",
    )?;
    let applied = c.request(
        &s.alias,
        "rules.save",
        json!({"source":proposed,"if_revision":rules["revision"]}),
    )?;
    s.applied = true;
    ensure(
        applied["revision"] == format!("sha256:{PROPOSED_SHA}"),
        "applied rules revision mismatch",
    )?;
    save(
        "rules-applied",
        json!({"original_sha256":RULES_SHA,"applied_sha256":PROPOSED_SHA,"thread_id":THREAD}),
    )?;
    let created = c.request(
        &s.alias,
        "post.create",
        json!({"thread_id":THREAD,"body":ORIGINAL}),
    )?;
    let no = created["no"]
        .as_i64()
        .filter(|n| *n > 2)
        .ok_or("new positive No invalid")?;
    ensure(
        created["revision"] == 1,
        "positive create revision mismatch",
    )?;
    save(
        "positive-created",
        json!({"id":created["id"],"no":no,"revision":1,"body_sha256":hash(ORIGINAL.as_bytes())}),
    )?;
    let edited = c.request(
        &s.alias,
        "post.edit",
        json!({"thread_id":THREAD,"no":no,"body":CURRENT,"if_revision":1}),
    )?;
    ensure(
        edited["revision"] == 2 && edited["id"] == created["id"],
        "positive edit mismatch",
    )?;
    save(
        "positive-edited",
        json!({"id":created["id"],"no":no,"revision":2,"body_sha256":hash(CURRENT.as_bytes())}),
    )?;
    s.ai_active = true;
    c.cli(&s.project, &["identity", "resume", "--actor-id", actor])?;
    let positive = c.cli(
        &s.project,
        &[
            "post",
            "history",
            "--thread-id",
            THREAD,
            "--no",
            &no.to_string(),
        ],
    )?;
    ensure(history_ok(&positive), "positive public history mismatch")?;
    save(
        "positive-history",
        json!({"public_history_count":1,"original_body_sha256":hash(ORIGINAL.as_bytes()),"post_id":created["id"],"post_no":no}),
    )?;
    let expired_history = c.cli(
        &s.project,
        &["post", "history", "--thread-id", THREAD, "--no", "2"],
    )?;
    ensure(
        expired_history["items"] == json!([]),
        "expired public history not empty",
    )?;
    save(
        "expired-history",
        json!({"post_id":b["deleted_post"]["id"],"post_no":2,"public_history_count":0,"cli_success":true,"explicit_grant":true}),
    )?;
    ensure(
        original_live(
            &c.request(&s.alias, "post.get", json!({"thread_id":THREAD,"no":1}))?,
            &b["live_control"],
        ),
        "final original live changed",
    )?;
    ensure(
        c.request(&s.alias, "post.get", json!({"thread_id":THREAD,"no":2}))? == expired,
        "expired preservation mismatch",
    )?;
    Ok(
        json!({"status":"EXPLICIT_HISTORY_EXPIRY_PASS_PENDING_CLOSEOUT","positive_post_id":created["id"],"positive_post_no":no,"positive_history_items":1,"expired_history_items":0,"explicit_grant":true,"original_live_preserved":true,"expired_id_preserved":true,"new_positive_retained":true,"native_gui_observed":false,"formal_ac_accepted":false}),
    )
}
fn normal_exit(child: &mut Child) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().map_err(|_| "runtime status unavailable")? {
            return ensure(status.success(), "runtime nonzero exit");
        }
        ensure(
            Instant::now() < until,
            "normal exit deadline; preserve runtime",
        )?;
        std::thread::sleep(Duration::from_millis(40));
    }
}
fn execute(b: Value) -> Result<()> {
    let original = text(Path::new(ROOT).join("RULES.md"))?;
    let proposed = text(Path::new(ARTIFACT).join("proposed-RULES.md"))?;
    let approval = json_read(Path::new(ARTIFACT).join("approval.json"))?;
    let own = std::env::current_exe().map_err(|_| "self path unavailable")?;
    ensure(
        approval_ok(&approval, &hash(&read(own)?)),
        "new standing approval/self pin required",
    )?;
    let alias = selected()?;
    save(
        "attempt",
        json!({"scope":"elapsed-explicit-history-20261023","approved":true,"at_unix_seconds":SystemTime::now().duration_since(UNIX_EPOCH).map_err(|_| "clock unavailable")?.as_secs(),"automatic_retries":0,"baseline_sha256":BASELINE_SHA}),
    )?;
    let mut command = Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
    command
        .args(["--data-dir", ROOT, "--human-start"])
        .stdin(Stdio::null());
    for (suffix, output) in [("stdout", true), ("stderr", false)] {
        let f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(Path::new(BASE).join(format!("history-grant-20261023-runtime.{suffix}.log")))
            .map_err(|_| "preserve runtime stream")?;
        if output {
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
    save("runtime", json!({"pid":child.id(),"candidate":CANDIDATE}))?;
    let c = Calls {
        deadline: Instant::now() + Duration::from_secs(180),
        uncertain: Arc::new(AtomicBool::new(false)),
    };
    let mut s = State {
        alias,
        project: String::new(),
        ai_active: false,
        applied: false,
    };
    let result = (|| {
        c.ready()?;
        ensure(
            child
                .try_wait()
                .map_err(|_| "runtime state unavailable")?
                .is_none(),
            "owned runtime exited during startup",
        )?;
        observe(&c, &mut s, &b, &original, &proposed)
    })();
    if c.uncertain.load(Ordering::SeqCst) {
        save(
            "result",
            json!({"status":"UNCERTAIN_NOT_ACCEPTED","runtime_pid":child.id(),"normal_cleanup_attempted":false,"rules_may_be_applied":s.applied,"ai_may_be_active":s.ai_active}),
        )?;
        return Err("uncertain operation; preserve runtime and do not race cleanup".into());
    }
    let cleanup: Result<()> = (|| {
        let mut errors = Vec::new();
        if s.ai_active {
            match c.cli(&s.project, &["identity", "release"]) {
                Ok(_) => s.ai_active = false,
                Err(e) => errors.push(e),
            }
        }
        ensure(
            !c.uncertain.load(Ordering::SeqCst),
            "uncertain release; preserve state",
        )?;
        if s.applied {
            let restored = c.request(
                &s.alias,
                "rules.save",
                json!({"source":original,"if_revision":format!("sha256:{PROPOSED_SHA}")}),
            );
            match restored {
                Ok(v) if v["revision"] == format!("sha256:{RULES_SHA}") => s.applied = false,
                Ok(_) => errors.push("RULES restore revision mismatch".into()),
                Err(e) => errors.push(e),
            }
        }
        ensure(
            !c.uncertain.load(Ordering::SeqCst),
            "uncertain RULES restore; preserve state",
        )?;
        if let Err(e) = ensure(
            hash(&read(Path::new(ROOT).join("RULES.md"))?) == RULES_SHA,
            "RULES restore bytes mismatch",
        ) {
            errors.push(e);
        }
        if let Err(e) = ensure(
            hash(&read(Path::new(BASE).join("baseline.json"))?) == BASELINE_SHA,
            "baseline changed",
        ) {
            errors.push(e);
        }
        c.stop(&s.alias)?;
        normal_exit(&mut child)?;
        let registration = Registration::new(
            Identity::new(
                Path::new(ROOT),
                &Path::new(CANDIDATE).join("swarm-desktop.exe"),
            )
            .map_err(|_| "notification identity unavailable")?,
        )
        .map_err(|_| "registration unavailable")?;
        if registration
            .is_registered()
            .map_err(|_| "registration status unavailable")?
        {
            registration
                .remove()
                .map_err(|_| "registration removal failed")?;
        }
        ensure(
            !registration
                .is_registered()
                .map_err(|_| "final registration status unavailable")?,
            "registration remains",
        )?;
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    })();
    save(
        "closeout",
        json!({"runtime_pid":child.id(),"normal_closeout":cleanup.is_ok(),"rules_restored":!s.applied,"ai_released":!s.ai_active,"baseline_sha256":BASELINE_SHA,"error":cleanup.as_ref().err()}),
    )?;
    let mut receipt = match &result {
        Ok(v) => v.clone(),
        Err(e) => json!({"status":"NOT_ACCEPTED","error":e}),
    };
    receipt["normal_closeout"] = json!(cleanup.is_ok());
    if result.is_ok() && cleanup.is_ok() {
        receipt["status"] = json!("EXPLICIT_HISTORY_EXPIRY_PASS");
    }
    save("result", receipt)?;
    result?;
    cleanup?;
    println!("EXPLICIT_HISTORY_EXPIRY_PASS");
    Ok(())
}
fn main() -> Result<()> {
    let mode = std::env::args().skip(1).collect::<Vec<_>>();
    ensure(
        mode.len() == 1 && matches!(mode[0].as_str(), "--plan" | "--execute-approved"),
        "one fixed explicit mode required",
    )?;
    let b = inputs()?;
    if mode[0] == "--plan" {
        println!(
            "{}",
            json!({"status":"INPUTS_READY_ONLY","credential_store_opened":false,"runtime_started":false,"fixture_modified":false})
        );
        Ok(())
    } else {
        execute(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_old_read_only_authority_and_scope_expansion() {
        let mut a = json!({"scope":"elapsed-explicit-history-20261023","approved":true,"standing_authorization":true,"executable_sha256":"self","baseline_sha256":BASELINE_SHA,"original_rules_sha256":RULES_SHA,"proposed_rules_sha256":PROPOSED_SHA,"data_root":ROOT,"temporary_rules_change":true,"positive_posts":1,"positive_edits":1,"attempts":1,"automatic_retries":0,"reuse_project_and_alias":true,"reseed":false,"vm":false,"publication":false});
        assert!(approval_ok(&a, "self"));
        assert!(!approval_ok(&a, "different-executable"));
        for key in [
            "approved",
            "standing_authorization",
            "temporary_rules_change",
            "reuse_project_and_alias",
        ] {
            let previous = a[key].clone();
            a[key] = json!(false);
            assert!(!approval_ok(&a, "self"));
            a[key] = previous;
        }
        for key in ["reseed", "vm", "publication"] {
            a[key] = json!(true);
            assert!(!approval_ok(&a, "self"));
            a[key] = json!(false);
        }
        a["scope"] = json!("elapsed-public-readback-extension-1");
        assert!(!approval_ok(&a, "self"));
    }
    #[test]
    fn empty_and_wrong_revision_cannot_establish_positive_history() {
        assert!(!history_ok(&json!({"items":[]})));
        assert!(!history_ok(
            &json!({"items":[{"revision":2,"body":ORIGINAL}]})
        ));
        assert!(!history_ok(
            &json!({"items":[{"revision":1,"body":CURRENT}]})
        ));
        assert!(history_ok(
            &json!({"items":[{"revision":1,"body":ORIGINAL}]})
        ));
    }
}
