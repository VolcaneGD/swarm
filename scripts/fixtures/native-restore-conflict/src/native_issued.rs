//! R18: issued human2/AI/cursors joined to a native post-active rollback.
//! Fixed synthetic root; no OS fault, direct SQL, injected product endpoint or old-root mutation.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use swarm_core::{CredentialCandidate, protocol::Request};
use swarm_gui_host::GuiHost;
use swarm_ipc::RpcResponse;
use swarm_platform::{
    CredentialStore,
    notifications::{Identity, Registration},
    pipe,
};
use swarm_runtime::gui::GuiClient;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261022-native-issued-rollback";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261022-native-issued-rollback/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const NAME: &str = "IssuedRollbackFixture";
const HUMAN2: &str = "issued-rollback-human2";
const AI_ALIAS: &str = "issued-rollback-ai";
const CANARY: &str = "issued-rollback-unused-delete-canary";
const RULES: &str = "# Isolated issued rollback\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: explicit-changes\n    effect: allow\n    actions: [changes.get, changes.wait]\naccess:\n  boards: []\n  threads: []\n~~~\n";
fn ensure(b: bool, message: &str) -> Result<()> {
    if b { Ok(()) } else { Err(message.into()) }
}
fn root() -> &'static Path {
    Path::new(ROOT)
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn save(name: &str, value: &Value) -> Result<()> {
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(Path::new(BASE).join(name))?;
    writeln!(file, "{value}")?;
    Ok(())
}
fn read(name: &str) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(
        Path::new(BASE).join(name),
    )?)?)
}
fn private() -> Result<CredentialStore> {
    Ok(CredentialStore::open(
        &Path::new(BASE).join("PrivateEvidence"),
    )?)
}
fn selected() -> Result<String> {
    let v: Value = serde_json::from_slice(
        &CredentialStore::open(&root().join("gui-state"))?
            .get("selection")?
            .ok_or("selection absent")?,
    )?;
    Ok(v["alias"].as_str().ok_or("alias absent")?.into())
}
fn req(op: &str, args: Value, auth: Option<String>) -> Request {
    Request {
        operation: op.into(),
        args,
        auth,
        context: json!({}),
        request_id: None,
    }
}
fn human(alias: &str, op: &str, args: Value) -> Result<Value> {
    Ok(GuiClient::open(root())?.request(alias, req(op, args, None))?)
}
fn rpc(op: &str, args: Value, auth: Option<String>) -> Result<RpcResponse> {
    let mut connection = pipe::connect(&swarm_runtime::pipe_name(root())?)?;
    connection.set_timeout(Duration::from_secs(8));
    swarm_ipc::write_request(&mut connection, &req(op, args, auth))?;
    Ok(swarm_ipc::read_response(&mut connection)?)
}
fn positive(response: RpcResponse) -> Result<Value> {
    ensure(response.ok, "public positive RPC refused")?;
    Ok(response.data.ok_or("RPC data absent")?)
}
fn refused(response: RpcResponse, code: &str) -> Result<()> {
    ensure(!response.ok, "old credential unexpectedly accepted")?;
    ensure(
        response.error.ok_or("RPC fault absent")?.code == code,
        "unexpected safe RPC fault code",
    )
}
fn human_refused(alias: &str) -> Result<String> {
    let error = GuiClient::open(root())?
        .request(alias, req("account.status", json!({}), None))
        .err()
        .ok_or("old human alias accepted")?;
    ensure(
        error.code == "AUTHENTICATION_REQUIRED",
        "old human fault unexpected",
    )?;
    Ok(error.code)
}
fn check_candidate() -> Result<()> {
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
    Ok(())
}
fn spawn(name: &str, executable: &str) -> Result<u32> {
    let mut command = Command::new(Path::new(CANDIDATE).join(executable));
    command.args(["--data-dir", ROOT]);
    if executable == "swarm-runtime.exe" {
        command.arg("--human-start");
    }
    command
        .stdin(Stdio::null())
        .stdout(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join(format!("{name}.stdout.log")))?,
        )
        .stderr(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(BASE).join(format!("{name}.stderr.log")))?,
        );
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let child = command.spawn()?;
    let pid = child.id();
    save(
        &format!("{name}.json"),
        &json!({"pid":pid,"candidate":CANDIDATE,"data_root":ROOT}),
    )?;
    Ok(pid)
}
fn ready() -> Result<()> {
    let until = Instant::now() + Duration::from_secs(15);
    while GuiClient::open(root())
        .and_then(|c| c.setup_status())
        .is_err()
    {
        ensure(
            Instant::now() < until,
            "readiness deadline; preserve owned process",
        )?;
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}
fn created(h: &mut GuiHost, op: &str, args: Value) -> Result<Value> {
    let v = h.create(op, args)?;
    h.acknowledge_create()?;
    Ok(v)
}
fn post_summary(p: &Value) -> Result<Value> {
    Ok(
        json!({"id":p["id"],"no":p["no"],"revision":p["revision"],"body_sha256":hash(p["body"].as_str().ok_or("body absent")?.as_bytes())}),
    )
}
fn compare_posts(alias: &str, p: &Value) -> Result<()> {
    let actual = human(alias, "post.list", json!({"thread_id":p["thread"]}))?;
    let items = actual["items"].as_array().ok_or("Posts absent")?;
    ensure(items.len() == 2, "original Post count changed")?;
    for expected in p["posts"].as_array().ok_or("expected Posts absent")? {
        let item = items
            .iter()
            .find(|v| v["id"] == expected["id"])
            .ok_or("Post id lost")?;
        ensure(post_summary(item)? == *expected, "original Post changed")?;
    }
    Ok(())
}
fn ai_candidate(alias: &str) -> Result<CredentialCandidate> {
    Ok(serde_json::from_slice(
        &CredentialStore::open(&root().join("client-credentials"))?
            .get(alias)?
            .ok_or("AI candidate absent")?,
    )?)
}
fn ai_fields(c: &CredentialCandidate) -> Result<(&str, &str, &str)> {
    match c {
        CredentialCandidate::Ai {
            id,
            session_token,
            resume_secret,
            ..
        } => Ok((id, session_token, resume_secret)),
        _ => Err("AI candidate type mismatch".into()),
    }
}
fn prepare() -> Result<()> {
    check_candidate()?;
    ensure(
        !Path::new(BASE).exists(),
        "fixed root already exists; no reseed",
    )?;
    fs::create_dir(BASE)?;
    fs::create_dir(ROOT)?;
    save(
        "preparation-attempt.json",
        &json!({"scope":"new-native-issued-credentials-rollback-join","not_repeat_old_fixture":true,"vm":false,"elapsed":false,"publication":false}),
    )?;
    swarm_runtime::startup_preference_set(root(), false)?;
    fs::write(root().join("RULES.md"), RULES)?;
    spawn("runtime", "swarm-runtime.exe")?;
    ready()?;
    let mut h = GuiHost::open(root())?;
    let account = h.authenticate("setup", NAME, "")?["account"]["id"].clone();
    let aid = account.as_str().ok_or("account absent")?;
    h.backup_control(
        aid,
        "settings.set",
        json!({"enabled":false,"interval_seconds":86400,"retention":2,"destination":null}),
    )?;
    let project_root = Path::new(BASE).join("Project");
    fs::create_dir(&project_root)?;
    fs::write(project_root.join("RULES.md"), RULES)?;
    let project = h.project_control(
        aid,
        "project.register",
        json!({"name":"Issued native rollback","root":project_root}),
    )?["id"]
        .clone();
    let board = created(
        &mut h,
        "board.create",
        json!({"name":"ISSUED_ROLLBACK_BOARD"}),
    )?;
    let thread = created(
        &mut h,
        "thread.create",
        json!({"board_id":board["id"],"name":"ISSUED_LATEST_STATE_MUST_SURVIVE"}),
    )?;
    created(
        &mut h,
        "post.create",
        json!({"thread_id":thread["id"],"body":"ISSUED_ARCHIVED_SYNTHETIC"}),
    )?;
    let archive = h.backup_control(aid, "manual.create", json!({}))?;
    h.backup_control(aid, "manual.ack", json!({"intent":archive["intent"]}))?;
    let live=h.edit(json!({"thread_id":thread["id"],"no":1,"if_revision":1,"body":"ISSUED_LIVE_POST1_MUST_SURVIVE"}))?;
    let extra = created(
        &mut h,
        "post.create",
        json!({"thread_id":thread["id"],"body":"ISSUED_LIVE_ONLY_POST2"}),
    )?;
    let primary = selected()?;
    let client = GuiClient::open(root())?;
    let second = client.issue(
        HUMAN2,
        &format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        ),
        req("account.login", json!({"name":NAME,"password":""}), None),
    )?;
    ensure(second["id"] == account, "second human account mismatch")?;
    for alias in [&primary, HUMAN2] {
        ensure(
            human(alias, "account.status", json!({}))?["id"] == account,
            "human positive mismatch",
        )?;
    }
    positive(rpc(
        "session.start",
        json!({"alias":AI_ALIAS,"project_id":project}),
        Some(format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        )),
    )?)?;
    let ai = ai_candidate(AI_ALIAS)?;
    let (_, token, _) = ai_fields(&ai)?;
    let a = positive(rpc(
        "post.list",
        json!({"thread_id":thread["id"]}),
        Some(token.into()),
    )?)?;
    ensure(
        a["items"].as_array().ok_or("AI positive absent")?.len() == 2,
        "AI positive count mismatch",
    )?;
    let c1 =
        human(&primary, "changes.get", json!({"thread_id":thread["id"]}))?["change_cursor"].clone();
    let c2 =
        human(HUMAN2, "changes.get", json!({"thread_id":thread["id"]}))?["change_cursor"].clone();
    let ca = positive(rpc(
        "changes.get",
        json!({"thread_id":thread["id"]}),
        Some(token.into()),
    )?)?["change_cursor"]
        .clone();
    ensure(
        c1.is_string() && c2.is_string() && ca.is_string(),
        "positive cursor missing",
    )?;
    for (key, value) in [
        ("human1-cursor", &c1),
        ("human2-cursor", &c2),
        ("ai-cursor", &ca),
    ] {
        private()?.put(key, &serde_json::to_vec(value)?)?;
    }
    private()?.put("issued-ai", &serde_json::to_vec(&ai)?)?;
    save(
        "issued-positive.json",
        &json!({"human_sessions":2,"same_account":true,"ai_session_and_resume_issued":true,"post_items":2,"human_cursors_issued":2,"human_cursors_equal":c1==c2,"ai_cursor_issued":true,"secret_values_emitted":false}),
    )?;
    let archive_path = root().join("backups").join(format!(
        "{}.backup",
        archive["key"].as_str().ok_or("archive key absent")?
    ));
    let preview = h.select_restore_backup(aid, &archive_path)?;
    let canonical_base = fs::canonicalize(BASE)?;
    for t in preview["preview"]["targets"]
        .as_array()
        .ok_or("restore targets absent")?
    {
        ensure(
            Path::new(t["target_path"].as_str().ok_or("restore target absent")?)
                .starts_with(&canonical_base),
            "restore target outside fixed fixture",
        )?;
    }
    let prepared = h.prepare_restore(aid, true)?;
    ensure(prepared["state"] == "prepared", "restore not prepared")?;
    let p = json!({"account":account,"project":project,"thread":thread["id"],"posts":[post_summary(&live)?,post_summary(&extra)?],"primary_alias":primary,"second_alias":HUMAN2,"prepared":prepared,"archive_sha256":hash(&fs::read(&archive_path)?),"rules_sha256":hash(&fs::read(root().join("RULES.md"))?),"known_unset_password":true,"native_apply_not_executed":true});
    compare_posts(&primary, &p)?;
    save("prepared.json", &p)?;
    drop(h);
    spawn("desktop", "swarm-desktop.exe")?;
    println!("ISSUED_NATIVE_PREPARED_WAIT_ACTION_TIME_CONFIRMATION");
    Ok(())
}
fn arm() -> Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    let p = read("prepared.json")?;
    let approval = read("native-apply-approval.json")?;
    ensure(
        approval["approved"] == true
            && approval["data_root"] == ROOT
            && approval["archive_sha256"] == p["archive_sha256"],
        "exact native apply approval absent",
    )?;
    save(
        "apply-attempt.json",
        &json!({"scope":"same-native-issued-rollback","old_fixture_replay":false}),
    )?;
    let store = CredentialStore::open(&root().join("credentials"))?;
    store.put(CANARY, b"synthetic-unused-record")?;
    let file = root()
        .join("credentials")
        .join(format!("{}.bin", hash(CANARY.as_bytes())));
    let lease = OpenOptions::new().read(true).share_mode(1).open(file)?;
    ensure(
        store.get(CANARY)?.as_deref() == Some(b"synthetic-unused-record"),
        "synthetic record not readable",
    )?;
    save(
        "armed.json",
        &json!({"read_allowed":true,"delete_blocked":true,"deadline_seconds":180,"auth_token_file":false}),
    )?;
    let until = Instant::now() + Duration::from_secs(180);
    while !Path::new(BASE).join("release-lock").exists() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
    }
    let timed_out = !Path::new(BASE).join("release-lock").exists();
    drop(lease);
    save(
        "lock-released.json",
        &json!({"released":true,"timed_out":timed_out}),
    )?;
    Ok(())
}
fn inspect() -> Result<()> {
    let p = read("prepared.json")?;
    let journal = CredentialStore::open(&root().join("restore-transactions"))?;
    let t: Value = serde_json::from_slice(
        &journal
            .get("active")?
            .ok_or("no active transaction; not accepted")?,
    )?;
    ensure(
        t["state"] == "active" && t["intent"] == p["prepared"]["preparation"]["intent"],
        "active mismatch",
    )?;
    let terminal_absent = journal
        .get(&format!(
            "result-{}",
            t["intent"].as_str().ok_or("intent absent")?
        ))?
        .is_none();
    ensure(terminal_absent, "terminal unexpectedly present")?;
    let error = GuiClient::open(root())?
        .request(
            p["primary_alias"].as_str().ok_or("alias absent")?,
            req("account.status", json!({}), None),
        )
        .err()
        .ok_or("active operation not fenced")?;
    ensure(
        error.code == "RESTORE_RECOVERY_REQUIRED"
            || (error.code == "CONNECTION_ERROR"
                && swarm_runtime::lifecycle::status(root())?["state"] == "not_running"),
        "unexpected failure/endpoint state",
    )?;
    save(
        "native-failure-observed.json",
        &json!({"active_retained":true,"terminal_absent":terminal_absent,"ordinary_request_code":error.code,"safety_sha256":t["safety_sha256"],"exact_intent":true}),
    )?;
    Ok(())
}
fn recover() -> Result<()> {
    check_candidate()?;
    ensure(
        read("lock-released.json")?["released"] == true,
        "lease not released",
    )?;
    ensure(
        read("native-failure-observed.json")?["active_retained"] == true,
        "native failure absent",
    )?;
    ensure(
        swarm_runtime::lifecycle::status(root())?["state"] == "not_running",
        "old runtime not normally drained",
    )?;
    spawn("recovered-runtime", "swarm-runtime.exe")?;
    ready()?;
    let p = read("prepared.json")?;
    let journal = CredentialStore::open(&root().join("restore-transactions"))?;
    ensure(journal.get("active")?.is_none(), "recovery active remains")?;
    let intent = p["prepared"]["preparation"]["intent"]
        .as_str()
        .ok_or("prepared intent absent")?;
    let t: Value = serde_json::from_slice(
        &journal
            .get(&format!("result-{intent}"))?
            .ok_or("terminal absent")?,
    )?;
    ensure(t["state"] == "rolled_back", "not rolled back")?;
    let a1 = human_refused(p["primary_alias"].as_str().ok_or("primary alias absent")?)?;
    let a2 = human_refused(HUMAN2)?;
    let ai: CredentialCandidate =
        serde_json::from_slice(&private()?.get("issued-ai")?.ok_or("issued AI absent")?)?;
    let (id, token, resume) = ai_fields(&ai)?;
    refused(
        rpc("board.list", json!({}), Some(token.into()))?,
        "AUTHENTICATION_REQUIRED",
    )?;
    refused(
        rpc(
            "session.resume",
            json!({"alias":"old-resume-denied","project_id":p["project"],"actor_id":id,"resume_secret":resume}),
            Some(format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            )),
        )?,
        "AUTHENTICATION_REQUIRED",
    )?;
    save(
        "old-issued-refused.json",
        &json!({"terminal":"rolled_back","active_absent":true,"human1":a1,"human2":a2,"ai_token":"AUTHENTICATION_REQUIRED","ai_resume":"AUTHENTICATION_REQUIRED","positive_before_same_native_failure":true,"reopened_broker_only":true,"old_runtime_drained":true}),
    )?;
    spawn("recovered-desktop", "swarm-desktop.exe")?;
    println!("ISSUED_ROLLBACK_OLD_CREDENTIALS_REFUSED_WAIT_USER_NATIVE_LOGIN");
    Ok(())
}
fn verify() -> Result<()> {
    ensure(
        read("native-user-login.json")?["user_reported_login"] == true,
        "native user login confirmation absent",
    )?;
    let p = read("prepared.json")?;
    let alias = selected()?;
    ensure(
        human(&alias, "account.status", json!({}))?["id"] == p["account"],
        "native account mismatch",
    )?;
    compare_posts(&alias, &p)?;
    for name in ["human1-cursor", "human2-cursor"] {
        let cursor: Value =
            serde_json::from_slice(&private()?.get(name)?.ok_or("human cursor absent")?)?;
        let error = GuiClient::open(root())?
            .request(
                &alias,
                req(
                    "changes.get",
                    json!({"thread_id":p["thread"],"after":cursor}),
                    None,
                ),
            )
            .err()
            .ok_or("old human cursor accepted")?;
        ensure(
            error.code == "RESYNC_REQUIRED",
            "old human cursor unexpected code",
        )?;
    }
    positive(rpc(
        "session.start",
        json!({"alias":"fresh-issued-ai","project_id":p["project"]}),
        Some(format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        )),
    )?)?;
    let fresh = ai_candidate("fresh-issued-ai")?;
    let (_, fresh_token, _) = ai_fields(&fresh)?;
    let old: Value =
        serde_json::from_slice(&private()?.get("ai-cursor")?.ok_or("AI cursor absent")?)?;
    refused(
        rpc(
            "changes.get",
            json!({"thread_id":p["thread"],"after":old}),
            Some(fresh_token.into()),
        )?,
        "RESYNC_REQUIRED",
    )?;
    let fresh_cursor = positive(rpc(
        "changes.get",
        json!({"thread_id":p["thread"]}),
        Some(fresh_token.into()),
    )?)?["change_cursor"]
        .clone();
    let client = GuiClient::open(root())?;
    let terminal = client.backup(
        &alias,
        p["account"].as_str().ok_or("account absent")?,
        "restore.status",
        json!({"intent":p["prepared"]["preparation"]["intent"]}),
    )?;
    ensure(
        terminal["state"] == "rolled_back",
        "terminal reread mismatch",
    )?;
    ensure(
        human(&alias, "account.status", json!({}))?["id"] == p["account"],
        "terminal reread expired new human",
    )?;
    positive(rpc(
        "changes.get",
        json!({"thread_id":p["thread"],"after":fresh_cursor}),
        Some(fresh_token.into()),
    )?)?;
    ensure(
        hash(&fs::read(root().join("RULES.md"))?) == p["rules_sha256"],
        "RULES changed",
    )?;
    save(
        "verified.json",
        &json!({"status":"ISSUED_CREDENTIALS_SAME_NATIVE_ROLLBACK_JOIN_PASS","human_sessions_refused":2,"ai_session_and_resume_refused":true,"old_human_cursors_refused":2,"old_ai_cursor_refused":true,"fresh_human_and_ai_valid":true,"terminal_reread_preserves_fresh_credentials_and_cursor":true,"same_latest_posts":true,"rules_unchanged":true,"native_human_login":true,"old_live_broker_not_observed":true,"full_fault_os_matrix":false}),
    )?;
    println!("ISSUED_CREDENTIALS_SAME_NATIVE_ROLLBACK_JOIN_PASS");
    Ok(())
}
fn stop() -> Result<()> {
    GuiClient::open(root())?.stop(&selected()?)?;
    let until = Instant::now() + Duration::from_secs(15);
    while swarm_runtime::lifecycle::status(root())?["state"] != "stopped" {
        ensure(Instant::now() < until, "normal stop deadline")?;
        std::thread::sleep(Duration::from_millis(60));
    }
    let r = Registration::new(Identity::new(
        root(),
        &Path::new(CANDIDATE).join("swarm-desktop.exe"),
    )?)?;
    r.remove()?;
    ensure(!r.is_registered()?, "owned registration remains")?;
    save(
        "closeout.json",
        &json!({"normally_stopped":true,"owned_registration_absent":true}),
    )?;
    Ok(())
}
fn main() {
    let result = match std::env::args().nth(1).as_deref() {
        Some("--prepare") => prepare(),
        Some("--arm") => arm(),
        Some("--inspect") => inspect(),
        Some("--recover") => recover(),
        Some("--verify") => verify(),
        Some("--stop") => stop(),
        _ => Err("choose a fixed phase".into()),
    };
    if let Err(error) = result {
        eprintln!("ISSUED_NATIVE_NOT_ACCEPTED: {error}");
        std::process::exit(1);
    }
}
