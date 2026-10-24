//! Fixed-baseline follow-up only. No seed, runtime launch, clock override or watcher.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs::{self, OpenOptions}, io::Write, path::Path, process::{Command, Stdio}, sync::mpsc, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use swarm_core::protocol::Request;
use swarm_platform::CredentialStore;
use swarm_runtime::gui::GuiClient;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261019-elapsed-approved-extension-2";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261019-elapsed-approved-extension-2/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const APPROVAL: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/elapsed-followup-20261020/approval.json";
const PUBLIC_APPROVAL: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/elapsed-followup-20261020/public-extension-1-approval.json";
const RULES_SHA: &str = "cac66eb9923ff8701603f1dff96019146b703d3f7071775ab01f2c3b9887d3a0";
const BASELINE_SHA: &str = "1d68c9a0106becf65940dd645c758de3a31fc9588d6dc1d5b4de1342d5c4c644";
const RUNTIME_SHA: &str = "0cf3ecb753c20830be7edf2c3630b3cc07aac90655d963b1097374f8b2b24210";
const CLI_SHA: &str = "d23b7973d229bea186d4aec95eca3a6f7f2868f7243534156372e5e438ecfd33";
const NOT_BEFORE: u64 = 1792505888;
fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() }
fn save(name: &str, value: Value) -> Result<(), String> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(Path::new(BASE).join(name)).map_err(|_| "preserve existing follow-up receipt")?;
    file.write_all(serde_json::to_string_pretty(&value).unwrap().as_bytes()).map_err(|_| "receipt write failed".into())
}
fn require(condition: bool, message: &str) -> Result<(), String> { if condition { Ok(()) } else { Err(message.into()) } }
fn time_ready(at: u64) -> bool { at >= NOT_BEFORE }
fn approval_ready(a: &Value, at: u64) -> bool {
    time_ready(at) && a["scope"] == "elapsed-product-followup-once"
        && a["user_answer"] == "既存baselineの保持期間観測を1回許可する"
        && a["approved_at_unix_seconds"].as_u64().is_some_and(|t| t >= NOT_BEFORE && t <= at)
        && a["attempts"] == 1 && a["automatic_retries"] == 0 && a["native_gui_observed"] == true
        && a["reseed"] == false && a["publication"] == false && a["vm"] == false
}
fn public_approval_ready(a: &Value, at: u64, prior_attempt_at: u64) -> bool {
    a["scope"] == "elapsed-public-readback-extension-1"
        && a["user_answer"] == "保持期間の公開AI読取だけ追加1回を許可する"
        && a["approved_at_unix_seconds"].as_u64().is_some_and(|t| t > prior_attempt_at && t <= at)
        && a["attempts"] == 1 && a["automatic_retries"] == 0
        && a["reseed"] == false && a["publication"] == false && a["vm"] == false
        && a["rules_change"] == false && a["reuse_registered_project_and_alias"] == true
        && a["normal_gui_start_and_stop"] == true && time_ready(at)
}
fn inputs() -> Result<Value, String> {
    for fixed in [BASE, ROOT] {
        let canonical = fs::canonicalize(fixed).map_err(|_| "fixed path missing")?;
        require(canonical.to_string_lossy().trim_start_matches(r"\\?\").replace('\\', "/").eq_ignore_ascii_case(fixed), "redirected fixture refused")?;
    }
    let bytes = fs::read(Path::new(BASE).join("baseline.json")).map_err(|_| "baseline missing")?;
    require(digest(&bytes) == BASELINE_SHA, "baseline changed; stop")?;
    let baseline: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid baseline")?;
    require(baseline["observe_not_before_unix_seconds"] == NOT_BEFORE, "time pin changed")?;
    let closeout: Value = serde_json::from_slice(&fs::read(Path::new(BASE).join("closeout.json")).map_err(|_| "preparation closeout missing")?).map_err(|_| "invalid preparation closeout")?;
    for field in ["preparation_succeeded", "normal_stop_accepted", "runtime_exited"] { require(closeout[field] == true, "preparation closeout failed")?; }
    require(closeout["retry_allowed"] == false, "unexpected preparation retry state")?;
    for (name, sha) in [("swarm-runtime.exe", RUNTIME_SHA), ("swarm.exe", CLI_SHA)] {
        require(digest(&fs::read(Path::new(CANDIDATE).join(name)).map_err(|_| "frozen binary missing")?) == sha, "frozen binary changed")?;
    }
    Ok(baseline)
}
fn call(alias: &str, operation: &str, args: Value, deadline: Instant) -> Result<Value, String> {
    let remaining = deadline.saturating_duration_since(Instant::now()).min(Duration::from_secs(20));
    require(!remaining.is_zero(), "follow-up deadline")?;
    let (tx, rx) = mpsc::channel();
    let alias = alias.to_owned(); let operation = operation.to_owned();
    std::thread::spawn(move || {
        let result = GuiClient::open(Path::new(ROOT)).map_err(|_| "GUI pipe unavailable".to_owned()).and_then(|client| client.request(&alias, Request { operation, auth: None, args, context: json!({}), request_id: None }).map_err(|f| f.code.to_string()));
        let _ = tx.send(result);
    });
    rx.recv_timeout(remaining).map_err(|_| "uncertain product request; preserve state, no retry".to_owned())?
}
fn cli(project: &str, args: &[String], expected: i32, deadline: Instant) -> Result<Value, String> {
    let bound = deadline.min(Instant::now() + Duration::from_secs(20));
    let mut child = Command::new(Path::new(CANDIDATE).join("swarm.exe"))
        .args(["--data-dir", ROOT, "--project", project, "--session", "elapsed-followup-20261020", "--format", "json"])
        .args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|_| "public CLI start failed")?;
    while child.try_wait().map_err(|_| "CLI status unavailable")?.is_none() {
        if Instant::now() >= bound { return Err(format!("owned CLI deadline; preserve PID {}, no retry", child.id())); }
        std::thread::sleep(Duration::from_millis(25));
    }
    let output = child.wait_with_output().map_err(|_| "CLI streams unavailable")?;
    if output.status.code() != Some(expected) || !output.stderr.is_empty() || output.stdout.len() > 65536 {
        // Preserve only bounded nonsecret diagnostics; never copy CLI bodies or credentials.
        let parsed: Value = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
        let code = parsed["error"]["code"].as_str().filter(|s| s.len() <= 64 && s.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')).unwrap_or("UNAVAILABLE");
        let operation = args.iter().take(2).map(String::as_str).collect::<Vec<_>>().join(" ");
        let selector = args.windows(2).find(|pair| matches!(pair[0].as_str(), "--no" | "--id") && pair[1].len() <= 128 && pair[1].bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')).map(|pair|format!(" {} {}",pair[0],pair[1])).unwrap_or_default();
        return Err(format!("public CLI {operation}{selector}: expected exit {expected}, actual {:?}, stdout bytes {}, stderr bytes {}, error code {code}", output.status.code(), output.stdout.len(), output.stderr.len()));
    }
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|_| "invalid public CLI JSON")?;
    require(value["schema_version"] == 1 && value["ok"] == (expected == 0), "public CLI envelope mismatch")?;
    if expected == 0 { Ok(value["data"].clone()) } else { Ok(value["error"].clone()) }
}
fn strings(args: &[&str]) -> Vec<String> { args.iter().map(|s| s.to_string()).collect() }
fn public_readset(b: &Value, project_id: &str, deadline: Instant) -> Result<(), String> {
    let thread = b["live_thread"]["id"].as_str().ok_or("thread id missing")?;
    let live_no = b["live_control"]["no"].to_string();
    let deleted_no = b["deleted_post"]["no"].to_string();
    require(live_matches(&cli(project_id, &strings(&["post","get","--thread-id",thread,"--no",&live_no]),0,deadline)?, &b["live_control"]), "public AI live control mismatch")?;
    require(purged_skeleton(&cli(project_id, &strings(&["post","get","--thread-id",thread,"--no",&deleted_no]),0,deadline)?, &b["deleted_post"]), "public AI purged skeleton mismatch")?;
    require(cli(project_id, &strings(&["post","history","--thread-id",thread,"--no",&deleted_no]),4,deadline)?["code"] == "PERMISSION_DENIED", "public AI history must remain denied under fixture defaults")?;
    require(only_live_search(&cli(project_id,&strings(&["search","run","--query","ELAPSED","--scope","body"]),0,deadline)?,&b["live_control"]), "public AI search leaks expired content or lost live control")?;
    for (kind, id) in [("board", &b["deleted_board"]["id"]), ("thread", &b["deleted_thread"]["id"]), ("thread", &b["child_thread"]["id"])] {
        require(cli(project_id, &strings(&[kind,"get","--id",id.as_str().ok_or("deleted id missing")?]),5,deadline)?["code"] == "NOT_FOUND", "public AI deleted parent/child still visible")?;
    }
    Ok(())
}
fn public_extension(b: &Value) -> Result<Value, String> {
    let deadline = Instant::now() + Duration::from_secs(180);
    let store = CredentialStore::open(&Path::new(ROOT).join("client-credentials")).map_err(|_| "saved CLI credentials unavailable")?;
    let bytes = store.get("elapsed-followup-20261020").map_err(|_| "saved alias unavailable")?.ok_or("existing alias required; no identity start")?;
    let saved: Value = serde_json::from_slice(&bytes).map_err(|_| "saved candidate invalid")?;
    require(saved["kind"] == "ai", "saved alias is not AI")?;
    let project = saved["project_id"].as_str().ok_or("saved project missing")?;
    let actor = saved["id"].as_str().ok_or("saved actor missing")?;
    let gui_store = CredentialStore::open(&Path::new(ROOT).join("gui-state")).map_err(|_| "saved GUI selection unavailable")?;
    let selection: Value = serde_json::from_slice(&gui_store.get("selection").map_err(|_| "GUI selection unavailable")?.ok_or("saved human selection required")?).map_err(|_| "invalid GUI selection")?;
    let alias = selection["alias"].as_str().ok_or("human alias missing")?;
    let account = call(alias, "account.status", json!({}), deadline)?;
    require(account["id"] == b["account_id"] && account["is_admin"] == true, "baseline Admin not selected")?;
    let metadata = call(alias, "project.get", json!({"id":project}), deadline)?;
    let canonical = metadata["canonical_root"].as_str().ok_or("registered project root missing")?.trim_start_matches(r"\\?\").replace('\\',"/");
    require(canonical.eq_ignore_ascii_case(&format!("{BASE}/PublicReadbackProject")) && metadata["name"] == "Elapsed retention readback", "saved alias points to a different project; no resume")?;
    // Resume only the previously released alias/project. No registration or new alias.
    cli(project, &strings(&["identity","resume","--actor-id",actor]),0,deadline)?;
    let readback = public_readset(b, project, deadline);
    if let Err(error) = &readback
        && ["owned CLI deadline", "CLI status unavailable", "CLI streams unavailable"].iter().any(|marker| error.contains(marker))
    {
        return Err(format!("{error}; release not attempted while CLI completion is uncertain"));
    }
    // One normal release also on a definite read failure, without retrying a read.
    let release = cli(project, &strings(&["identity","release"]),0,deadline);
    if let Err(error) = readback {
        return Err(format!("{error}; normal release succeeded: {}", release.is_ok()));
    }
    release?;
    require(digest(&fs::read(Path::new(BASE).join("baseline.json")).map_err(|_| "baseline unavailable")?) == BASELINE_SHA, "baseline modified")?;
    Ok(json!({"status":"PUBLIC_READBACK_PASS_NATIVE_CLOSEOUT_SEPARATE","public_live_control_preserved":true,"public_purged_skeleton":true,"public_history_default_denied":true,"public_history_with_explicit_grant_tested":false,"public_search_live_only":true,"public_deleted_parents_child_not_found":true,"public_ai_released":true,"baseline_preserved":true,"formal_ac_accepted":false,"runtime_stopped":false,"rules_changed":false,"retry_allowed":false}))
}
fn live_matches(post: &Value, baseline: &Value) -> bool {
    post["id"] == baseline["id"] && post["no"] == baseline["no"] && post["revision"] == baseline["revision"]
        && post["body"].as_str().is_some_and(|body| baseline["body_sha256"] == digest(body.as_bytes()))
}
fn purged_skeleton(post: &Value, baseline: &Value) -> bool {
    post["id"] == baseline["id"] && post["no"] == baseline["no"] && post["deleted"] == true
        && post["body"].is_null() && post["restore_available"] == false
}
fn only_live_search(result: &Value, live: &Value) -> bool {
    result["items"].as_array().is_some_and(|items| items.len() == 1 && items[0]["id"] == live["id"] && items[0]["kind"] == "post")
        && result["total_count"] == 1
}
fn observe(b: &Value) -> Result<Value, String> {
    let deadline = Instant::now() + Duration::from_secs(180);
    let store = CredentialStore::open(&Path::new(ROOT).join("gui-state")).map_err(|_| "saved GUI state unavailable")?;
    for key in ["pending-auth", "pending-create"] { require(store.get(key).map_err(|_| "pending state unavailable")?.is_none(), "pending GUI operation; stop")?; }
    let selected: Value = serde_json::from_slice(&store.get("selection").map_err(|_| "selection unavailable")?.ok_or("selected account missing")?).map_err(|_| "invalid selection")?;
    let alias = selected["alias"].as_str().ok_or("selection alias missing")?;
    let account = call(alias, "account.status", json!({}), deadline)?;
    require(account["id"] == b["account_id"] && account["is_admin"] == true, "baseline Admin not selected")?;
    let post_args = |post: &Value| json!({"thread_id":b["live_thread"]["id"],"no":post["no"]});
    require(live_matches(&call(alias, "post.get", post_args(&b["live_control"]), deadline)?, &b["live_control"]), "live control changed")?;
    require(purged_skeleton(&call(alias, "post.get", post_args(&b["deleted_post"]), deadline)?, &b["deleted_post"]), "expired Post not purged; do not restore or replay")?;
    require(call(alias, "post.history", post_args(&b["deleted_post"]), deadline)?["items"] == json!([]), "expired history remains")?;
    let deleted = call(alias, "conversation.deleted", json!({}), deadline)?;
    require(deleted["boards"] == json!([]) && deleted["threads"] == json!([]), "expired deleted inventory remains")?;
    require(only_live_search(&call(alias,"search",json!({"query":"ELAPSED","scope":"body"}),deadline)?,&b["live_control"]), "private search leaks expired content or lost live control")?;
    for (op, args, code) in [
        ("board.get", json!({"id":b["deleted_board"]["id"]}), "NOT_FOUND"),
        ("thread.get", json!({"id":b["deleted_thread"]["id"]}), "NOT_FOUND"),
        ("thread.get", json!({"id":b["child_thread"]["id"]}), "NOT_FOUND"),
        ("post.deleted_body", post_args(&b["deleted_post"]), "CONTENT_PURGED"),
        ("post.restore", post_args(&b["deleted_post"]), "CONTENT_PURGED"),
        ("thread.restore", json!({"id":b["deleted_thread"]["id"]}), "NOT_FOUND"),
        ("board.restore", json!({"id":b["deleted_board"]["id"]}), "NOT_FOUND"),
    ] { require(call(alias, op, args, deadline).err().as_deref() == Some(code), "expected purge/non-revival refusal absent; stop")?; }
    // Public AI bootstrap is new follow-up metadata, never a repeat of baseline seed.
    let project_root = Path::new(BASE).join("PublicReadbackProject");
    fs::create_dir(&project_root).map_err(|_| "preserve existing public-readback project")?;
    let project = call(alias, "project.register", json!({"name":"Elapsed retention readback","root":project_root}), deadline)?;
    let project_id = project["id"].as_str().ok_or("project id missing")?;
    cli(project_id, &strings(&["identity", "start"]), 0, deadline)?;
    // The fixture keeps default read-only RULES. post.history requires an explicit
    // operation grant even after content purges; do not widen rules to make a test pass.
    public_readset(b, project_id, deadline)?;
    cli(project_id, &strings(&["identity","release"]),0,deadline)?;
    require(live_matches(&call(alias,"post.get",post_args(&b["live_control"]),deadline)?,&b["live_control"]), "final live control changed")?;
    require(digest(&fs::read(Path::new(BASE).join("baseline.json")).map_err(|_| "baseline unavailable")?) == BASELINE_SHA, "baseline modified")?;
    Ok(json!({"status":"PRODUCT_READBACK_PASS_NATIVE_AND_CLOSEOUT_SEPARATE","baseline_sha256":BASELINE_SHA,"live_control_preserved":true,"history_expired":true,"public_history_default_denied":true,"public_history_with_explicit_grant_tested":false,"deleted_post_skeleton_only":true,"deleted_board_thread_child_not_found":true,"restore_non_revival":true,"private_and_public_search_live_only":true,"public_ai_readback":true,"public_ai_released":true,"archived_thread_case_prepared":false,"native_acceptance":false,"formal_ac_accepted":false,"runtime_stopped":false,"watcher":false}))
}
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    require(args.len() == 1 && matches!(args[0].as_str(), "--plan" | "--observe-approved-once" | "--public-extension-approved-once"), "one explicit mode only; no root/time overrides")?;
    let baseline = inputs()?; let at = now();
    if !time_ready(at) {
        println!("{}",json!({"status":"WAIT_REAL_ELAPSED_TIME","not_before_jst":"2026-10-20T23:18:08+09:00","remaining_seconds":NOT_BEFORE-at,"data_stores_opened":false,"product_started":false,"attempt_consumed":false}));
        std::process::exit(2);
    }
    if args[0] == "--plan" { println!("{}",json!({"status":"READY_ONLY","product_started":false,"reseed":false,"native_gui_and_new_scoped_authority_required":true})); return Ok(()); }
    if args[0] == "--public-extension-approved-once" {
        let prior: Value = serde_json::from_slice(&fs::read(Path::new(BASE).join("followup-attempt.json")).map_err(|_| "prior attempt required")?).map_err(|_| "invalid prior attempt")?;
        let failure: Value = serde_json::from_slice(&fs::read(Path::new(BASE).join("followup-product-result.json")).map_err(|_| "prior failure required")?).map_err(|_| "invalid prior failure")?;
        require(failure["status"] == "FAIL" && failure["error"] == "public CLI status/stream mismatch", "different incident; extension refused")?;
        let released: Value = serde_json::from_slice(&fs::read(Path::new(BASE).join("followup-release.json")).map_err(|_| "normal prior release required")?).map_err(|_| "invalid prior release")?;
        require(released["ok"] == true && released["exit_code"] == 0, "prior normal release not confirmed")?;
        let authority: Value = serde_json::from_slice(&fs::read(PUBLIC_APPROVAL).map_err(|_| "new explicit public-only extension approval required")?).map_err(|_| "invalid public authority")?;
        require(public_approval_ready(&authority,at,prior["at_unix_seconds"].as_u64().ok_or("prior attempt time missing")?), "public extension authority mismatch")?;
        require(authority["observer_sha256"] == digest(&fs::read(std::env::current_exe().map_err(|_| "observer path unavailable")?).map_err(|_| "observer hash unavailable")?), "public extension approved hash mismatch")?;
        require(digest(&fs::read(Path::new(ROOT).join("RULES.md")).map_err(|_| "fixture rules unavailable")?) == RULES_SHA, "fixture rules changed; do not widen permissions")?;
        save("followup-public-extension-1-attempt.json", json!({"scope":"elapsed-public-readback-extension-1","at_unix_seconds":at,"prior_attempt_at":prior["at_unix_seconds"],"automatic_retries":0,"same_incident":true,"rules_changed":false}))?;
        let receipt = match public_extension(&baseline) {
            Ok(value) => value,
            Err(error) => json!({"status":"FAIL","error":error,"retry_allowed":false,"preserve_runtime_and_fixture":true}),
        };
        let ok = receipt["status"] == "PUBLIC_READBACK_PASS_NATIVE_CLOSEOUT_SEPARATE";
        save("followup-public-extension-1-result.json", receipt)?;
        require(ok, "public extension failed; preserve state, no retry")?;
        println!("Public extension passed; normal native closeout remains separate.");
        return Ok(());
    }
    let approval: Value = serde_json::from_slice(&fs::read(APPROVAL).map_err(|_| "new follow-up approval required; preparation approval does not apply")?).map_err(|_| "invalid follow-up approval")?;
    require(approval_ready(&approval,at), "follow-up scope/time/native observation authority mismatch")?;
    require(approval["observer_sha256"] == digest(&fs::read(std::env::current_exe().map_err(|_| "observer path unavailable")?).map_err(|_| "observer hash unavailable")?), "approved observer hash mismatch")?;
    save("followup-attempt.json",json!({"scope":"elapsed-product-followup-once","at_unix_seconds":at,"baseline_sha256":BASELINE_SHA,"automatic_retries":0,"no_runtime_start":true}))?;
    match observe(&baseline) {
        Ok(receipt) => { save("followup-product-result.json",receipt)?; println!("Product readback passed; native acceptance and normal closeout remain separate."); Ok(()) }
        Err(error) => { save("followup-product-result.json",json!({"status":"FAIL","error":error,"retry_allowed":false,"preserve_runtime_and_fixture":true}))?; Err("follow-up failed; preserve state, no retry".into()) }
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn public_history_default_rule_contract() {
        // New ephemeral Service only: no baseline, GUI, runtime, CLI or clock changes.
        let root = tempfile::tempdir().unwrap();
        let mut service = swarm_core::Service::open(root.path()).unwrap();
        assert_eq!(digest(&fs::read(root.path().join("RULES.md")).unwrap()), RULES_SHA);
        let mut request = |op: &str, auth: Option<&str>, args: Value| service.request(Request {
            operation: op.into(), auth: auth.map(str::to_owned), args,
            context: json!({}), request_id: None,
        });
        let setup = request("account.setup", None, json!({"name":"ContractAdmin","password":"contract-test-password"})).unwrap();
        let admin = setup["token"].as_str().unwrap();
        let project = request("project.register", Some(admin), json!({"name":"ContractProject","root":root.path()})).unwrap();
        let board = request("board.create", Some(admin), json!({"name":"ContractBoard"})).unwrap();
        let thread = request("thread.create", Some(admin), json!({"board_id":board["id"],"name":"ContractThread"})).unwrap();
        let post = request("post.create", Some(admin), json!({"thread_id":thread["id"],"body":"synthetic contract"})).unwrap();
        let identity = request("identity.start", None, json!({"project_id":project["id"]})).unwrap();
        let token = identity["session_token"].as_str().unwrap();
        let args = json!({"thread_id":thread["id"],"no":post["no"]});
        assert!(request("post.get", Some(token), args.clone()).is_ok(), "default read control must work");
        let denied = request("post.history", Some(token), args).unwrap_err();
        assert_eq!((denied.code.as_str(),denied.exit_code),("PERMISSION_DENIED",4), "helper must not expect history success without a rule grant");
    }
    #[test] fn public_extension_rejects_previous_grant_and_any_scope_expansion() {
        let prior = NOT_BEFORE + 10;
        let mut a = json!({"scope":"elapsed-public-readback-extension-1","user_answer":"保持期間の公開AI読取だけ追加1回を許可する","approved_at_unix_seconds":prior+1,"attempts":1,"automatic_retries":0,"reseed":false,"publication":false,"vm":false,"rules_change":false,"reuse_registered_project_and_alias":true,"normal_gui_start_and_stop":true});
        assert!(public_approval_ready(&a,prior+1,prior));
        a["user_answer"] = json!("既存baselineの保持期間観測を1回許可する");
        assert!(!public_approval_ready(&a,prior+1,prior));
        a["user_answer"] = json!("保持期間の公開AI読取だけ追加1回を許可する");
        a["approved_at_unix_seconds"] = json!(prior);
        assert!(!public_approval_ready(&a,prior+1,prior));
        a["approved_at_unix_seconds"] = json!(prior+1);
        for field in ["reseed","publication","vm","rules_change"] {
            a[field] = json!(true); assert!(!public_approval_ready(&a,prior+1,prior)); a[field] = json!(false);
        }
        a["automatic_retries"] = json!(1); assert!(!public_approval_ready(&a,prior+1,prior));
    }
    #[test] fn actual_boundary_has_no_early_ready_window() { assert!(!time_ready(NOT_BEFORE-1)); assert!(time_ready(NOT_BEFORE)); }
    #[test] fn preparation_or_early_authority_cannot_authorize_observation() {
        let mut a=json!({"scope":"elapsed-product-followup-once","user_answer":"既存baselineの保持期間観測を1回許可する","approved_at_unix_seconds":NOT_BEFORE,"attempts":1,"automatic_retries":0,"native_gui_observed":true,"reseed":false,"publication":false,"vm":false});
        assert!(approval_ready(&a,NOT_BEFORE)); a["approved_at_unix_seconds"]=json!(NOT_BEFORE-1); assert!(!approval_ready(&a,NOT_BEFORE));
        a["approved_at_unix_seconds"]=json!(NOT_BEFORE); a["scope"]=json!("elapsed-seed-20261016"); assert!(!approval_ready(&a,NOT_BEFORE));
    }
    #[test] fn only_an_expired_skeleton_is_safe_to_test_restore_refusal() {
        let b=json!({"id":"post","no":2}); let mut p=json!({"id":"post","no":2,"body":null,"deleted":true,"restore_available":false});
        assert!(purged_skeleton(&p,&b)); p["restore_available"]=json!(true); assert!(!purged_skeleton(&p,&b));
    }
    #[test] fn search_must_preserve_live_control_without_any_expired_hit() {
        let live=json!({"id":"live"}); let mut result=json!({"items":[{"id":"live","kind":"post"}],"total_count":1});
        assert!(only_live_search(&result,&live)); result["items"][0]["id"]=json!("expired"); assert!(!only_live_search(&result,&live));
        result["items"]=json!([]); result["total_count"]=json!(0); assert!(!only_live_search(&result,&live));
    }
}
