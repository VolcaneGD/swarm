//! Successful started credential RPCs joined to human stop. No rollback/fault injection.
#[path = "../../native-rpc-oplock/src/oplock.rs"]
mod oplock;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    os::windows::io::AsRawHandle,
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use swarm_core::{CredentialCandidate, Service, protocol::Request};
use swarm_platform::{CredentialStore, pipe};
use swarm_runtime::gui::GuiClient;
use windows_sys::Win32::{
    Storage::FileSystem::FlushFileBuffers, System::Pipes::GetNamedPipeServerProcessId,
};
type Result<T> = std::result::Result<T, String>;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261024-frozen-started-rpc";
const EXE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261024-run-path/swarm-runtime.exe";
const PIN: &str = "1ee7838beb28752558ab48a4a7687b2347d6c5309e756f9d6cf9eef2599d03ce";
const HUMAN: &str = "started-rpc-admin";
const AI: &str = "started-rpc-ai";
const OLD: &str = "synthetic-initial-password-only";
const NEW: &str = "synthetic-rotated-password-only";
fn ensure(ok: bool, msg: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(msg.into()) }
}
fn fault<T>(r: std::result::Result<T, swarm_core::protocol::Fault>) -> Result<T> {
    r.map_err(|e| e.code)
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
fn now() -> Result<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_millis())
        .map_err(|_| "clock".into())
}
fn digest(v: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(v.as_ref()))
}
fn save(name: &str, v: Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(Path::new(BASE).join(name))
        .map_err(|_| "preserve receipt")?;
    writeln!(f, "{v}").map_err(|_| "receipt write".into())
}
fn pins() -> Result<()> {
    ensure(
        digest(fs::read(EXE).map_err(|_| "candidate missing")?) == PIN,
        "candidate changed",
    )
}
fn spawn(root: &Path, n: usize) -> Result<Child> {
    pins()?;
    let out = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(Path::new(BASE).join(format!("runtime-{n}.stdout.log")))
        .map_err(|_| "preserve stdout")?;
    let err = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(Path::new(BASE).join(format!("runtime-{n}.stderr.log")))
        .map_err(|_| "preserve stderr")?;
    let mut c = Command::new(EXE);
    c.arg("--data-dir")
        .arg(root)
        .arg("--human-start")
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    use std::os::windows::process::CommandExt;
    c.creation_flags(0x08000000);
    let child = c.spawn().map_err(|_| "runtime spawn")?;
    save(
        &format!("runtime-{n}.json"),
        json!({"pid":child.id(),"candidate":EXE}),
    )?;
    Ok(child)
}
fn ready(root: &Path, child: &mut Child, active: &mut Option<GuiClient>) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        ensure(
            child.try_wait().map_err(|_| "child wait")?.is_none(),
            "child exited before readiness",
        )?;
        if let Ok(c) = GuiClient::open(root) {
            let ok = c.setup_status().is_ok();
            *active = Some(c);
            if ok {
                return Ok(());
            }
        }
        ensure(Instant::now() < end, "readiness deadline")?;
        std::thread::sleep(Duration::from_millis(30));
    }
}
fn wait(child: &mut Child) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(s) = child.try_wait().map_err(|_| "child wait")? {
            return ensure(s.success(), "runtime nonzero");
        }
        ensure(
            Instant::now() < end,
            "uncertain runtime; preserve without force kill",
        )?;
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn snapshot(root: &Path) -> Result<String> {
    let db = rusqlite::Connection::open_with_flags(
        root.join("swarm.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(|_| "snapshot open")?;
    let password: String = db
        .query_row("SELECT password_hash FROM accounts", [], |r| r.get(0))
        .map_err(|_| "password snapshot")?;
    let sessions:String=db.query_row("SELECT coalesce(group_concat(token_hash,','),'') FROM (SELECT token_hash FROM ai_sessions ORDER BY token_hash)",[],|r|r.get(0)).map_err(|_|"session snapshot")?;
    // Private verifier only: no password hash, token hash or digest is saved/output.
    Ok(digest(format!("{password}\0{sessions}")))
}
fn writer_busy(root: &Path) -> Result<bool> {
    let db = rusqlite::Connection::open_with_flags(
        root.join("swarm.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .map_err(|_| "probe open")?;
    db.busy_timeout(Duration::ZERO)
        .map_err(|_| "probe timeout")?;
    match db.execute_batch("BEGIN IMMEDIATE") {
        Ok(()) => {
            db.execute_batch("ROLLBACK").map_err(|_| "probe rollback")?;
            Ok(false)
        }
        Err(rusqlite::Error::SqliteFailure(e, _))
            if matches!(
                e.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            ) =>
        {
            Ok(true)
        }
        Err(_) => Err("writer probe".into()),
    }
}
fn full_stop_absent(root: &Path) -> Result<bool> {
    CredentialStore::open(&root.join("lifecycle"))
        .and_then(|s| s.get("full-stop"))
        .map(|v| v.is_none())
        .map_err(|_| "stop store".into())
}
fn raw(root: &Path, request: Request) -> Result<Value> {
    let name = fault(swarm_runtime::pipe_name(root))?;
    let mut c = pipe::connect(&name).map_err(|_| "public connect")?;
    c.set_timeout(Duration::from_secs(20));
    swarm_ipc::write_request(&mut c, &request).map_err(|e| e.code)?;
    let r = swarm_ipc::read_response(&mut c).map_err(|e| e.code)?;
    if r.ok {
        r.data.ok_or("public result missing".into())
    } else {
        Err(r
            .error
            .map(|e| e.code)
            .unwrap_or("public fault missing".into()))
    }
}
fn ai_candidate(root: &Path) -> Result<CredentialCandidate> {
    let bytes = CredentialStore::open(&root.join("client-credentials"))
        .and_then(|s| s.get(AI))
        .map_err(|_| "client store")?
        .ok_or("client absent")?;
    serde_json::from_slice(&bytes).map_err(|_| "candidate format".into())
}
fn ai_status(root: &Path, candidate: &CredentialCandidate, operation: &str) -> Result<Value> {
    let CredentialCandidate::Ai { session_token, .. } = candidate else {
        return Err("not AI".into());
    };
    let mut r = req(operation, json!({}));
    r.auth = Some(session_token.clone());
    raw(root, r)
}
fn control(c: &GuiClient, expected: &Value) -> Result<()> {
    ensure(
        fault(c.request(
            HUMAN,
            req(
                "post.get",
                json!({"thread_id":expected["thread_id"],"no":1}),
            ),
        ))? == *expected,
        "control Post changed",
    )
}
fn stop_file(root: &Path, pid: u32) -> Result<File> {
    let name = fault(swarm_runtime::gui_pipe_name(root))?;
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .open(format!(r"\\.\pipe\{name}"))
        .map_err(|_| "stop pipe open")?;
    let mut server = 0;
    ensure(
        unsafe { GetNamedPipeServerProcessId(f.as_raw_handle(), &mut server) } != 0
            && server == pid,
        "stop pipe is not owned runtime",
    )?;
    Ok(f)
}
fn started(root: &Path, child: &mut Child, number: usize, request: Request) -> Result<()> {
    let before = snapshot(root)?;
    let journal = root
        .join("credentials")
        .join(format!("{}.bin", digest("candidate-journal")));
    let mut held =
        oplock::ReadHandleOplock::new(&journal).map_err(|_| "journal coordination unavailable")?;
    let mut stop = stop_file(root, child.id())?;
    let cap = CredentialStore::open(&root.join("gui-host"))
        .and_then(|s| s.get("transport-capability"))
        .map_err(|_| "capability read")?
        .ok_or("capability absent")?;
    let cap = String::from_utf8(cap).map_err(|_| "capability invalid")?;
    let (update_tx, update_rx) = mpsc::channel();
    let rpc_root = root.to_path_buf();
    let op = request.operation.clone();
    let update = std::thread::spawn(move || {
        let r = if number == 0 {
            fault(GuiClient::open(&rpc_root))
                .and_then(|c| fault(c.issue(HUMAN, &"c".repeat(64), request)))
        } else {
            raw(&rpc_root, request)
        };
        let _ = update_tx.send(r);
    });
    let observed = held
        .broken(5000)
        .map_err(|_| "oplock observation failed")?
        .ok_or("RPC did not reach credential staging")?;
    ensure(
        observed.2 & 1 != 0
            && writer_busy(root)?
            && snapshot(root)? == before
            && matches!(update_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "not a pending started SQL mutation",
    )?;
    let entry = now()?;
    save(
        &format!("case-{number}-entered.json"),
        json!({"operation":op,"at_unix_ms":entry,"writer_busy":true,"committed_snapshot_unchanged":true,"rpc_pending":true,"credential_staging_oplock_break":true,"fault_injection":false}),
    )?;
    let (read_tx, read_rx) = mpsc::channel();
    let (stop_tx, stop_rx) = mpsc::channel();
    let stopper = std::thread::spawn(move || {
        let result = (|| {
            let mut r = req("gui.runtime_stop", json!({"alias":HUMAN}));
            r.auth = Some(cap);
            swarm_ipc::write_request(&mut stop, &r).map_err(|e| e.code)?;
            // Same Windows primitive used by .NET PipeStream.WaitForPipeDrain.
            ensure(
                unsafe { FlushFileBuffers(stop.as_raw_handle()) } != 0,
                "stop pipe read confirmation failed",
            )?;
            let _ = read_tx.send(now()?);
            let r = swarm_ipc::read_response(&mut stop).map_err(|e| e.code)?;
            if r.ok {
                r.data.ok_or("stop result absent".into())
            } else {
                Err(r
                    .error
                    .map(|e| e.code)
                    .unwrap_or("stop fault absent".into()))
            }
        })();
        let _ = stop_tx.send(result);
    });
    // Always release the normal file hold before propagating an observation error.
    let observed = (|| {
        let read_at = read_rx
            .recv_timeout(Duration::from_secs(2))
            .map_err(|_| "stop bytes were not confirmed read")?;
        ensure(
            writer_busy(root)?
                && snapshot(root)? == before
                && full_stop_absent(root)?
                && matches!(update_rx.try_recv(), Err(mpsc::TryRecvError::Empty))
                && matches!(stop_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "stop overtook pending transaction",
        )?;
        let release = now()?;
        save(
            &format!("case-{number}-before-release.json"),
            json!({"stop_bytes_consumed_unix_ms":read_at,"before_release_unix_ms":release,"transaction_owns_writer":true,"committed_snapshot_unchanged":true,"rpc_pending":true,"stop_response_pending":true,"stop_marker_absent":true,"server_dispatch_hook":false}),
        )
    })();
    drop(held);
    let result = update_rx
        .recv_timeout(Duration::from_secs(20))
        .map_err(|_| "uncertain update; preserve runtime")?;
    let stop_result = stop_rx
        .recv_timeout(Duration::from_secs(15))
        .map_err(|_| "uncertain stop; preserve runtime")?;
    update.join().map_err(|_| "update thread failed")?;
    stopper.join().map_err(|_| "stop thread failed")?;
    wait(child)?;
    observed?;
    let public = result?;
    ensure(
        public.get("token").is_none()
            && public.get("resume_secret").is_none()
            && public.get("session_token").is_none(),
        "private grant returned",
    )?;
    ensure(stop_result? == json!({"state":"stopped"}), "stop failed")?;
    ensure(
        snapshot(root)? != before && !full_stop_absent(root)?,
        "mutation/stop not durable",
    )?;
    save(
        &format!("case-{number}-result.json"),
        json!({"operation":op,"status":"FROZEN_STARTED_RPC_COMMIT_STOP_PASS","normal_exit_code":0,"sql_snapshot_changed_after_release":true,"stop_persisted":true,"rollback":false,"native_gui_observed":false}),
    )
}
fn run() -> Result<()> {
    ensure(
        std::env::args().skip(1).collect::<Vec<_>>() == ["--execute-once"],
        "fixed execute-once only",
    )?;
    pins()?;
    ensure(
        !Path::new(BASE).exists(),
        "preserve fixed attempt; no replay",
    )?;
    fs::create_dir(BASE).map_err(|_| "exclusive base creation")?;
    save(
        "attempt.json",
        json!({"at_unix_ms":now()?,"candidate":EXE,"sha256":PIN,"fault_injection":false,"old_fixture_access":false}),
    )?;
    let root = Path::new(BASE).join("Data");
    {
        let mut s = fault(Service::open(&root))?;
        fault(s.set_startup_preference(false))?;
    }
    let mut child = spawn(&root, 0)?;
    let mut active = None;
    let result = (|| {
        ready(&root, &mut child, &mut active)?;
        let c = active.as_ref().unwrap();
        let account = fault(c.issue(
            HUMAN,
            &"a".repeat(64),
            req(
                "account.setup",
                json!({"name":"Started RPC Admin","password":OLD}),
            ),
        ))?;
        ensure(account["id"].as_str().is_some(), "account ID absent")?;
        let project_root = Path::new(BASE).join("PublicProject");
        fs::create_dir(&project_root).map_err(|_| "project root")?;
        let project = fault(c.request(
            HUMAN,
            req(
                "project.register",
                json!({"name":"Started RPC Project","root":project_root}),
            ),
        ))?;
        let board = fault(c.request(
            HUMAN,
            req("board.create", json!({"name":"Started RPC control"})),
        ))?;
        let thread = fault(c.request(
            HUMAN,
            req(
                "thread.create",
                json!({"board_id":board["id"],"name":"Preserve control"}),
            ),
        ))?;
        fault(c.request(
            HUMAN,
            req(
                "post.create",
                json!({"thread_id":thread["id"],"body":"Synthetic unchanged started-RPC control."}),
            ),
        ))?;
        let expected = fault(c.request(
            HUMAN,
            req("post.get", json!({"thread_id":thread["id"],"no":1})),
        ))?;
        started(
            &root,
            &mut child,
            0,
            req(
                "account.password",
                json!({"password":NEW,"current_password":OLD,"name":"Started RPC Admin"}),
            ),
        )?;
        child = spawn(&root, 1)?;
        ready(&root, &mut child, &mut active)?;
        let c = active.as_ref().unwrap();
        control(c, &expected)?;
        let rejected = c
            .issue(
                "verify-old",
                &"d".repeat(64),
                req(
                    "account.login",
                    json!({"name":"Started RPC Admin","password":OLD}),
                ),
            )
            .is_err_and(|e| e.exit_code == 3);
        ensure(rejected, "old password not rejected")?;
        let logged = fault(c.issue(
            "verify-new",
            &"e".repeat(64),
            req(
                "account.login",
                json!({"name":"Started RPC Admin","password":NEW}),
            ),
        ))?;
        ensure(
            logged["id"] == account["id"],
            "rotated password account mismatch",
        )?;
        save(
            "password-readback.json",
            json!({"old_password_rejected":true,"new_password_same_account":true,"control_unchanged":true}),
        )?;
        let mut start = req(
            "session.start",
            json!({"alias":AI,"project_id":project["id"],"model_name":"Synthetic fixture"}),
        );
        start.auth = Some("f".repeat(64));
        started(&root, &mut child, 1, start)?;
        child = spawn(&root, 2)?;
        ready(&root, &mut child, &mut active)?;
        control(active.as_ref().unwrap(), &expected)?;
        let old = ai_candidate(&root)?;
        let status = ai_status(&root, &old, "identity.status")?;
        let CredentialCandidate::Ai {
            id,
            project_id,
            resume_secret,
            ..
        } = &old
        else {
            return Err("AI candidate missing".into());
        };
        let actor = id.clone();
        ensure(status["id"] == *id, "started actor readback mismatch")?;
        let mut resume = req(
            "session.resume",
            json!({"alias":AI,"actor_id":id,"project_id":project_id,"resume_secret":resume_secret,"model_name":"Synthetic fixture"}),
        );
        resume.auth = Some("b".repeat(64));
        started(&root, &mut child, 2, resume)?;
        child = spawn(&root, 3)?;
        ready(&root, &mut child, &mut active)?;
        let c = active.as_ref().unwrap();
        control(c, &expected)?;
        let new = ai_candidate(&root)?;
        ensure(old != new, "resume did not rotate candidate")?;
        ensure(
            ai_status(&root, &old, "identity.status")
                .is_err_and(|e| e == "AUTHENTICATION_REQUIRED"),
            "old AI session not rejected",
        )?;
        ensure(
            ai_status(&root, &new, "identity.status")?["id"] == actor,
            "resumed actor changed",
        )?;
        ai_status(&root, &new, "identity.release")?;
        ensure(
            fault(c.startup_preference_get())? == Some(false),
            "startup preference changed",
        )?;
        fault(c.stop(HUMAN))?;
        wait(&mut child)?;
        pins()?;
        let registered = swarm_platform::startup::Registration::new(Path::new(EXE), &root)
            .and_then(|r| r.is_registered())
            .map_err(|_| "startup inspect")?;
        ensure(!registered, "startup registered")?;
        save(
            "result.json",
            json!({"status":"FROZEN_STARTED_RPC_COMMIT_STOP_PASS","cases":3,"password_rotation_readback":true,"ai_start_and_resume_readback":true,"old_ai_session_rejected":true,"same_actor":true,"ai_released":true,"control_post_unchanged":true,"normal_exit_code":0,"startup_enabled":false,"startup_registered":false,"candidate_preserved":true,"native_gui_observed":false,"formal_ac_accepted":false,"rollback":false}),
        )
    })();
    if let Err(e) = &result {
        let stopped = if child.try_wait().ok().flatten().is_some() {
            true
        } else if swarm_runtime::lifecycle::status(&root).is_ok_and(|v| v["state"] == "stopped") {
            wait(&mut child).is_ok()
        } else if let Some(c) = active.as_ref() {
            c.stop(HUMAN).is_ok() && wait(&mut child).is_ok()
        } else {
            false
        };
        let _ = save(
            "failure.json",
            json!({"error":e,"pid":child.id(),"normal_closeout_or_prior_exit":stopped,"force_kill":false,"retry":false}),
        );
    }
    result
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1)
    }
    println!("FROZEN_STARTED_RPC_COMMIT_STOP_PASS");
}
