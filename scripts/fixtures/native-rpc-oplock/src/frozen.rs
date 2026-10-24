//! Exact frozen executable, real human login RPC, protected journal replacement.
use super::{Result, ensure, hash, oplock::ReadHandleOplock, save};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use swarm_core::protocol::Request;
use swarm_gui_host::GuiHost;
use swarm_platform::CredentialStore;
use swarm_runtime::gui::GuiClient;
#[cfg(not(feature = "rollback-fixture"))]
const BASE: &str = "E:/Swarm-Validation-Host/run-20261022-frozen-rpc-oplock";
#[cfg(not(feature = "rollback-fixture"))]
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261022-frozen-rpc-oplock/Data";
#[cfg(feature = "rollback-fixture")]
const BASE: &str = "E:/Swarm-Validation-Host/run-20261022-frozen-rpc-oplock-rollback";
#[cfg(feature = "rollback-fixture")]
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261022-frozen-rpc-oplock-rollback/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const ISSUE_ALIAS: &str = "frozen-precommit-login";
fn now() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}
fn read(name: &str) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(
        Path::new(BASE).join(name),
    )?)?)
}
fn persist(name: &str, v: Value) -> Result<()> {
    save(Path::new(BASE), name, &v)
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
fn fixed() -> Result<()> {
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
            "frozen candidate changed",
        )?;
    }
    Ok(())
}
fn alias() -> Result<String> {
    Ok(read("prepared.json")?["admin_alias"]
        .as_str()
        .ok_or("Admin alias missing")?
        .into())
}
fn normal_stop() -> Result<()> {
    if swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"] != "stopped" {
        GuiClient::open(Path::new(ROOT))?.stop(&alias()?)?;
    }
    Ok(())
}
fn prepare() -> Result<()> {
    fixed()?;
    ensure(!Path::new(BASE).exists(), "existing Data refused")?;
    fs::create_dir(BASE)?;
    fs::create_dir(ROOT)?;
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
    let mut child = command.spawn()?;
    persist(
        "runtime.json",
        json!({"pid":child.id(),"frozen_runtime":true}),
    )?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        ensure(child.try_wait()?.is_none(), "runtime exited at startup")?;
        if GuiClient::open(Path::new(ROOT))
            .and_then(|c| c.setup_status())
            .is_ok()
        {
            break;
        }
        ensure(Instant::now() < deadline, "runtime startup deadline")?;
        std::thread::sleep(Duration::from_millis(30));
    }
    let mut h = GuiHost::open(Path::new(ROOT))?;
    let password = uuid::Uuid::new_v4().simple().to_string();
    CredentialStore::open(&Path::new(BASE).join("SyntheticInputs"))?
        .put("password", password.as_bytes())?;
    let account = h.authenticate("setup", "RPC_Oplock_Admin", &password)?["account"]["id"].clone();
    let selected: Value = serde_json::from_slice(
        &CredentialStore::open(&Path::new(ROOT).join("gui-state"))?
            .get("selection")?
            .ok_or("selection missing")?,
    )?;
    persist(
        "prepared.json",
        json!({"admin_alias":selected["alias"],"account_id":account,"data_root":ROOT}),
    )?;
    h.backup_control(
        account.as_str().ok_or("account missing")?,
        "settings.set",
        json!({"enabled":false,"interval_seconds":86400,"retention":2,"destination":null}),
    )?;
    h.manage(
        "display.settings.set",
        json!({"language":"ja","theme":"light"}),
    )?;
    let board = h.create("board.create", json!({"name":"FROZEN_RPC_OPLOCK_BOARD"}))?;
    h.acknowledge_create()?;
    let thread = h.create(
        "thread.create",
        json!({"name":"FROZEN_RPC_OPLOCK_THREAD","board_id":board["id"]}),
    )?;
    h.acknowledge_create()?;
    let post = h.create(
        "post.create",
        json!({"thread_id":thread["id"],"body":"SYNTHETIC_FROZEN_RPC_OPLOCK_POST"}),
    )?;
    h.acknowledge_create()?;
    persist(
        "baseline.json",
        json!({"thread_id":thread["id"],"post_id":post["id"],"revision":post["revision"],"body_sha256":hash(b"SYNTHETIC_FROZEN_RPC_OPLOCK_POST")}),
    )?;
    drop(h);
    println!("FROZEN_FIXTURE_PREPARED");
    Ok(())
}
fn database() -> Result<rusqlite::Connection> {
    Ok(rusqlite::Connection::open_with_flags(
        Path::new(ROOT).join("swarm.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?)
}
fn writer_busy() -> Result<bool> {
    let db = database()?;
    db.busy_timeout(Duration::ZERO)?;
    match db.execute_batch("BEGIN IMMEDIATE") {
        Ok(()) => {
            db.execute_batch("ROLLBACK")?;
            Ok(false)
        }
        Err(rusqlite::Error::SqliteFailure(f, _))
            if matches!(
                f.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ) =>
        {
            Ok(true)
        }
        Err(e) => Err(e.into()),
    }
}
fn watch_inner() -> Result<()> {
    fixed()?;
    ensure(
        !Path::new(BASE).join("watch-ready.json").exists(),
        "watch already used",
    )?;
    let client = GuiClient::open(Path::new(ROOT))?;
    let account = client.request(&alias()?, req("account.status", json!({})))?;
    ensure(
        account["id"] == read("prepared.json")?["account_id"],
        "wrong live Admin",
    )?;
    // Reconcile through the real broker before installing the filesystem hold.
    let initial_sessions: i64 =
        database()?.query_row("SELECT count(*) FROM human_sessions", [], |r| r.get(0))?;
    persist(
        "watch-ready.json",
        json!({"waiting_for_arm":true,"initial_session_count":initial_sessions,"root":ROOT}),
    )?;
    let deadline = Instant::now() + Duration::from_secs(180);
    while !Path::new(BASE).join("arm.json").exists() {
        ensure(Instant::now() < deadline, "unarmed deadline")?;
        std::thread::sleep(Duration::from_millis(20));
    }
    let journal = Path::new(ROOT)
        .join("credentials")
        .join(format!("{}.bin", hash(b"candidate-journal")));
    // The memo write occurs after candidate staging inside the credential sink.
    // Read sharing permits normal memo load; denying DELETE makes its later
    // atomic replacement fail without changing ACLs or touching prior fixtures.
    let memo_guard = if cfg!(feature = "rollback-fixture") {
        use std::os::windows::fs::OpenOptionsExt;
        Some(
            OpenOptions::new().read(true).share_mode(3).open(
                Path::new(ROOT)
                    .join("gui-credentials")
                    .join(format!("{}.bin", hash(b"human-bridge-memos"))),
            )?,
        )
    } else {
        None
    };
    let mut held = ReadHandleOplock::new(&journal)?;
    let password = String::from_utf8(
        CredentialStore::open(&Path::new(BASE).join("SyntheticInputs"))?
            .get("password")?
            .ok_or("synthetic input missing")?,
    )?;
    let request = req(
        "account.login",
        json!({"name":"RPC_Oplock_Admin","password":password}),
    );
    let (tx, rx) = std::sync::mpsc::channel();
    let rpc = std::thread::spawn(move || {
        let result = client.issue(ISSUE_ALIAS, &"f".repeat(64), request);
        let _ = tx.send(result);
    });
    let observed = held
        .broken(5000)?
        .ok_or("credential replacement not reached")?;
    let break_at = now();
    let busy = writer_busy()?;
    let committed_sessions: i64 =
        database()?.query_row("SELECT count(*) FROM human_sessions", [], |r| r.get(0))?;
    ensure(
        observed.2 & 1 != 0
            && busy
            && committed_sessions == initial_sessions
            && matches!(rx.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)),
        "not an unresolved real SQL credential transaction",
    )?;
    persist(
        "precommit-ready.json",
        json!({"root":ROOT,"break":observed,"unix_ms":break_at,"writer_busy":true,"committed_session_count_unchanged":true,"rpc_pending":true,"frozen_runtime":true,"hold_seconds":8,"direct_sql_updates":false}),
    )?;
    let deadline = Instant::now() + Duration::from_secs(8);
    while !Path::new(BASE).join("release.json").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let before = now();
    let stop_absent = CredentialStore::open(&Path::new(ROOT).join("lifecycle"))?
        .get("full-stop")?
        .is_none();
    let still_pending = matches!(rx.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty));
    let native = Path::new(BASE).join("release.json").exists();
    persist(
        "before-unlock.json",
        json!({"unix_ms":before,"native_release_received":native,"stop_marker_absent":stop_absent,"rpc_pending":still_pending,"writer_busy":writer_busy()?}),
    )?;
    drop(held);
    let result = rx.recv_timeout(Duration::from_secs(15))?;
    rpc.join().map_err(|_| "RPC thread failed")?;
    drop(memo_guard);
    let rollback = cfg!(feature = "rollback-fixture");
    let public = if rollback {
        let fault = result.err().ok_or("memo conflict did not roll back")?;
        ensure(
            fault.code == "CREDENTIAL_STORAGE",
            "unexpected rollback fault",
        )?;
        json!({"error_code":fault.code})
    } else {
        result?
    };
    let text = serde_json::to_string(&public)?;
    ensure(
        !text.contains(&password) && public.get("token").is_none(),
        "private value in public RPC result",
    )?;
    let final_sessions: i64 =
        database()?.query_row("SELECT count(*) FROM human_sessions", [], |r| r.get(0))?;
    ensure(
        final_sessions == initial_sessions + i64::from(!rollback),
        "SQL session outcome does not match commit/rollback",
    )?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"] != "stopped"
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(30));
    }
    let stopped = swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"] == "stopped";
    let native_before_unlock = if native {
        let r = read("release.json")?;
        r["native_click_return_unix_ms"]
            .as_u64()
            .is_some_and(|v| (v as u128) >= break_at && (v as u128) <= before)
    } else {
        false
    };
    persist(
        "result.json",
        json!({"status":if native_before_unlock&&stop_absent&&still_pending&&stopped{"FROZEN_RPC_PRECOMMIT_NATIVE_CLICK_PASS"}else{"NATIVE_STOP_NOT_ESTABLISHED"},"rpc_committed":!rollback,"rollback":rollback,"public_result":public,"session_count_delta":final_sessions-initial_sessions,"native_click_before_unlock":native_before_unlock,"stop_persisted":stopped,"stop_request_admission_instrumented":false,"frozen_runtime":true,"transaction_hook":false,"hold_ms":before-break_at}),
    )?;
    ensure(
        native_before_unlock && stop_absent && still_pending && stopped,
        "native stop not established",
    )?;
    println!("FROZEN_RPC_PRECOMMIT_NATIVE_CLICK_PASS");
    Ok(())
}
fn watch() -> Result<()> {
    let r = watch_inner();
    if r.is_err() {
        let _ = normal_stop();
    }
    r
}
fn arm() -> Result<()> {
    ensure(
        read("watch-ready.json")?["root"] == ROOT,
        "wrong watcher root",
    )?;
    persist("arm.json", json!({"root":ROOT,"unix_ms":now()}))?;
    let deadline = Instant::now() + Duration::from_secs(6);
    while !Path::new(BASE).join("precommit-ready.json").exists() {
        ensure(
            Instant::now() < deadline,
            "precommit readiness deadline; no click",
        )?;
        std::thread::sleep(Duration::from_millis(10));
    }
    ensure(
        read("precommit-ready.json")?["writer_busy"] == true,
        "transaction not held",
    )?;
    println!("REAL_RPC_PRECOMMIT_READY");
    Ok(())
}
fn release(timestamp: &str) -> Result<()> {
    let timestamp: u64 = timestamp.parse()?;
    ensure(
        (timestamp as u128) <= now() && now() - (timestamp as u128) < 5000,
        "stale native observation",
    )?;
    persist(
        "release.json",
        json!({"native_click_return_unix_ms":timestamp,"helper_release_unix_ms":now(),"root":ROOT}),
    )
}
fn verify() -> Result<()> {
    ensure(
        read("result.json")?["status"] == "FROZEN_RPC_PRECOMMIT_NATIVE_CLICK_PASS",
        "not a native joined pass",
    )?;
    let c = GuiClient::open(Path::new(ROOT))?;
    let rollback = cfg!(feature = "rollback-fixture");
    let issued = c.request(ISSUE_ALIAS, req("account.status", json!({})));
    let read_alias = if rollback {
        ensure(
            issued
                .err()
                .is_some_and(|f| f.code == "AUTHENTICATION_REQUIRED"),
            "rolled-back credential accepted",
        )?;
        alias()?
    } else {
        ensure(issued.is_ok(), "committed credential refused")?;
        ISSUE_ALIAS.to_owned()
    };
    let account = c.request(&read_alias, req("account.status", json!({})))?;
    ensure(
        account["id"] == read("prepared.json")?["account_id"],
        "issued credential did not survive",
    )?;
    let baseline = read("baseline.json")?;
    let posts = c.request(
        &read_alias,
        req("post.list", json!({"thread_id":baseline["thread_id"]})),
    )?;
    let items = posts["items"].as_array().ok_or("Posts missing")?;
    ensure(
        items.len() == 1
            && items[0]["id"] == baseline["post_id"]
            && items[0]["revision"] == baseline["revision"]
            && hash(items[0]["body"].as_str().ok_or("body missing")?.as_bytes())
                == baseline["body_sha256"],
        "original Post changed",
    )?;
    persist(
        "verified-after-native-start.json",
        json!({"issued_alias_valid_after_restart":!rollback,"rolled_back_alias_refused":rollback,"original_admin_valid":true,"same_account":true,"original_post_id_body_revision_unchanged":true,"normal_api_readback":true}),
    )?;
    println!("FROZEN_RPC_POST_STOP_READBACK_PASS");
    Ok(())
}
fn cleanup() -> Result<()> {
    normal_stop()?;
    let r = swarm_platform::notifications::Registration::new(
        swarm_platform::notifications::Identity::new(
            Path::new(ROOT),
            &Path::new(CANDIDATE).join("swarm-desktop.exe"),
        )?,
    )?;
    r.remove()?;
    ensure(!r.is_registered()?, "owned registration remains")?;
    persist(
        "registration-closeout.json",
        json!({"owned_registration_absent":true,"data_preserved":true,"normal_stop":true}),
    )
}
pub fn run(args: &[String]) -> Result<()> {
    match args {
        [a] if a == "--prepare" => prepare(),
        [a] if a == "--watch" => watch(),
        [a] if a == "--arm" => arm(),
        [a, t] if a == "--release" => release(t),
        [a] if a == "--verify" => verify(),
        [a] if a == "--cleanup" => cleanup(),
        _ => Err("unknown operation".into()),
    }
}
