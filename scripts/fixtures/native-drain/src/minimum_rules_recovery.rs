//! One minimum-beta supported-runtime recovery join; no OS/power fault or old fixture access.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use swarm_core::{CredentialCandidate, Service, protocol::Request};
use swarm_platform::{CredentialStore, pipe};
use swarm_runtime::gui::GuiClient;
type Result<T> = std::result::Result<T, String>;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261024-minimum-rules-recovery";
const EXE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261024-run-path/swarm-runtime.exe";
const PIN: &str = "1ee7838beb28752558ab48a4a7687b2347d6c5309e756f9d6cf9eef2599d03ce";
const WORKER: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/debug/deps/swarm_core-346d19dfdf4bdea5.exe";
const HUMAN: &str = "minimum-rules-admin";
const AI: &str = "minimum-rules-ai";
fn ensure(v: bool, m: &str) -> Result<()> {
    if v { Ok(()) } else { Err(m.into()) }
}
fn fault<T>(v: std::result::Result<T, swarm_core::protocol::Fault>) -> Result<T> {
    v.map_err(|e| e.code)
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
fn hash(v: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(v.as_ref()))
}
fn save(n: &str, v: Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(Path::new(BASE).join(n))
        .map_err(|_| "preserve receipt")?;
    writeln!(f, "{v}").map_err(|_| "receipt write".into())
}
fn launch(exe: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new(exe);
    c.creation_flags(0x08000000);
    c.stdin(Stdio::null());
    c
}
fn spawn(root: &Path, n: usize) -> Result<Child> {
    ensure(
        hash(fs::read(EXE).map_err(|_| "runtime missing")?) == PIN,
        "runtime pin",
    )?;
    let mut c = launch(EXE);
    c.arg("--data-dir").arg(root).arg("--human-start");
    for (name, out) in [("stdout", true), ("stderr", false)] {
        let f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(Path::new(BASE).join(format!("runtime-{n}.{name}.log")))
            .map_err(|_| "preserve runtime streams")?;
        if out {
            c.stdout(f);
        } else {
            c.stderr(f);
        }
    }
    let child = c.spawn().map_err(|_| "runtime spawn")?;
    save(&format!("runtime-{n}.json"), json!({"pid":child.id()}))?;
    Ok(child)
}
fn wait(child: &mut Child) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(s) = child.try_wait().map_err(|_| "child wait")? {
            return ensure(s.success(), "runtime nonzero");
        };
        ensure(
            Instant::now() < end,
            "preserve uncertain child without force kill",
        )?;
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn ready(root: &Path, child: &mut Child, active: &mut Option<GuiClient>) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        ensure(
            child.try_wait().map_err(|_| "child wait")?.is_none(),
            "runtime early exit",
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
fn public(root: &Path, op: &str, args: Value) -> Result<Value> {
    let bytes = CredentialStore::open(&root.join("client-credentials"))
        .and_then(|s| s.get(AI))
        .map_err(|_| "client credential store")?
        .ok_or("AI credential absent")?;
    let candidate: CredentialCandidate =
        serde_json::from_slice(&bytes).map_err(|_| "AI candidate format")?;
    let CredentialCandidate::Ai { session_token, .. } = candidate else {
        return Err("AI candidate required".into());
    };
    let mut r = req(op, args);
    r.auth = Some(session_token);
    let name = fault(swarm_runtime::pipe_name(root))?;
    let mut c = pipe::connect(&name).map_err(|_| "public pipe")?;
    swarm_ipc::write_request(&mut c, &r).map_err(|e| e.code)?;
    let v = swarm_ipc::read_response(&mut c).map_err(|e| e.code)?;
    if v.ok {
        v.data.ok_or("public result missing".into())
    } else {
        Err(v.error.map(|e| e.code).unwrap_or("missing error".into()))
    }
}
fn run() -> Result<()> {
    ensure(
        std::env::args().skip(1).collect::<Vec<_>>() == ["--execute-once"],
        "fixed execute-once only",
    )?;
    ensure(!Path::new(BASE).exists(), "preserve existing attempt")?;
    let worker_pin = hash(fs::read(WORKER).map_err(|_| "existing core crash worker missing")?);
    fs::create_dir(BASE).map_err(|_| "exclusive base")?;
    save(
        "attempt.json",
        json!({"runtime_sha256":PIN,"worker_sha256":worker_pin,"stage":"source_prefix_written","whole_os_fault":false}),
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
        fault(c.issue(
            HUMAN,
            &"a".repeat(64),
            req(
                "account.setup",
                json!({"name":"Minimum Rules Admin","password":"synthetic-rules-only-password"}),
            ),
        ))?;
        let project_root = Path::new(BASE).join("PublicProject");
        fs::create_dir(&project_root).map_err(|_| "project root")?;
        let project = fault(c.request(
            HUMAN,
            req(
                "project.register",
                json!({"name":"Minimum rules project","root":project_root}),
            ),
        ))?;
        let board = fault(c.request(
            HUMAN,
            req("board.create", json!({"name":"Preserve control"})),
        ))?;
        let thread = fault(c.request(
            HUMAN,
            req(
                "thread.create",
                json!({"board_id":board["id"],"name":"Rules recovery control"}),
            ),
        ))?;
        fault(c.request(HUMAN,req("post.create",json!({"thread_id":thread["id"],"body":"Synthetic immutable rules recovery control."}))))?;
        let expected = fault(c.request(
            HUMAN,
            req("post.get", json!({"thread_id":thread["id"],"no":1})),
        ))?;
        let original = fs::read(root.join("RULES.md")).map_err(|_| "original rules")?;
        let mut start = req(
            "session.start",
            json!({"alias":AI,"project_id":project["id"],"model_name":"Rules fixture"}),
        );
        start.auth = Some("b".repeat(64));
        let name = fault(swarm_runtime::pipe_name(&root))?;
        let mut p = pipe::connect(&name).map_err(|_| "session connect")?;
        swarm_ipc::write_request(&mut p, &start).map_err(|e| e.code)?;
        ensure(
            swarm_ipc::read_response(&mut p).map_err(|e| e.code)?.ok,
            "AI start",
        )?;
        drop(p);
        ensure(
            public(&root, "post.get", json!({"thread_id":thread["id"],"no":1}))? == expected,
            "positive control",
        )?;
        ensure(
            public(
                &root,
                "post.create",
                json!({"thread_id":thread["id"],"body":"must remain denied"}),
            )
            .is_err_and(|e| e == "PERMISSION_DENIED"),
            "baseline write denial",
        )?;
        fault(c.stop(HUMAN))?;
        wait(&mut child)?;
        let proposed = "# Interrupted staged policy\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: staged-allow\n    effect: allow\n    actions: [post.create]\naccess:\n  boards: []\n  threads: []\n~~~\n";
        let mut crash = launch(WORKER);
        crash
            .args([
                "--exact",
                "rules_editor::windows_tests::crash_worker",
                "--ignored",
                "--nocapture",
            ])
            .env("SWARM_RULES_CRASH_PATH", root.join("RULES.md"))
            .env("SWARM_RULES_CRASH_STAGE", "source_prefix_written")
            .env("SWARM_RULES_CRASH_SOURCE", proposed);
        let out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(Path::new(BASE).join("crash-worker.stdout.log"))
            .map_err(|_| "worker stdout")?;
        let err = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(Path::new(BASE).join("crash-worker.stderr.log"))
            .map_err(|_| "worker stderr")?;
        let mut crash_child = crash
            .stdout(out)
            .stderr(err)
            .spawn()
            .map_err(|_| "crash worker launch")?;
        save(
            "crash-worker.json",
            json!({"pid":crash_child.id(),"stage":"source_prefix_written","force_kill":false}),
        )?;
        let crash_deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = crash_child.try_wait().map_err(|_| "crash worker wait")? {
                break status;
            }
            ensure(
                Instant::now() < crash_deadline,
                "uncertain crash worker; preserve PID receipt",
            )?;
            std::thread::sleep(Duration::from_millis(25));
        };
        ensure(!status.success(), "worker did not interrupt")?;
        let broken = fs::read(root.join("RULES.md")).map_err(|_| "interrupted artifact")?;
        ensure(broken != original, "artifact not interrupted")?;
        let candidates = fs::read_dir(&root)
            .map_err(|_| "recovery candidates")?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(".RULES.md.recovery-")
            })
            .collect::<Vec<_>>();
        ensure(
            candidates.len() == 1,
            "recovery candidate identity ambiguous",
        )?;
        let recovery = candidates[0].path();
        ensure(
            fs::read(&recovery).map_err(|_| "recovery copy")? == original,
            "original recovery bytes changed",
        )?;
        let filename = recovery
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or("recovery filename")?;
        let id = filename
            .strip_prefix(".RULES.md.recovery-")
            .ok_or("recovery ID")?;
        save(
            "interrupted.json",
            json!({"stage":"source_prefix_written","worker_nonzero":true,"original_sha256":hash(&original),"interrupted_sha256":hash(&broken),"recovery_copy_preserved":true,"recovery_id":id,"whole_os_fault":false}),
        )?;
        child = spawn(&root, 1)?;
        ready(&root, &mut child, &mut active)?;
        let c = active.as_ref().unwrap();
        ensure(
            public(&root, "post.get", json!({"thread_id":thread["id"],"no":1}))
                .is_err_and(|e| e == "RULES_INVALID"),
            "fresh runtime did not deny invalid policy",
        )?;
        ensure(
            public(
                &root,
                "post.create",
                json!({"thread_id":thread["id"],"body":"must not be granted"}),
            )
            .is_err_and(|e| e == "RULES_INVALID"),
            "partial grant escaped",
        )?;
        let inspected = fault(c.request(HUMAN, req("rules.get", json!({"recovery_id":id}))))?;
        ensure(
            inspected["candidate"]["valid"] == true
                && inspected["candidate"]["source"]
                    .as_str()
                    .is_some_and(|s| s.as_bytes() == original),
            "supported recovery inspection mismatch",
        )?;
        fault(c.request(HUMAN,req("rules.save",json!({"source":inspected["candidate"]["source"],"if_revision":inspected["current"]["revision"],"recovery_id":id,"recovery_revision":inspected["candidate"]["revision"]}))))?;
        ensure(
            fs::read(root.join("RULES.md")).map_err(|_| "restored rules")? == original,
            "original policy not restored",
        )?;
        ensure(
            public(&root, "post.get", json!({"thread_id":thread["id"],"no":1}))? == expected,
            "public control changed after recovery",
        )?;
        ensure(
            public(
                &root,
                "post.create",
                json!({"thread_id":thread["id"],"body":"must remain denied"}),
            )
            .is_err_and(|e| e == "PERMISSION_DENIED"),
            "restored policy expanded rights",
        )?;
        ensure(
            fault(c.request(
                HUMAN,
                req("post.list", json!({"thread_id":thread["id"],"limit":10})),
            ))?["items"]
                .as_array()
                .is_some_and(|v| v.len() == 1),
            "extra Post after recovery",
        )?;
        public(&root, "identity.release", json!({}))?;
        fault(c.stop(HUMAN))?;
        wait(&mut child)?;
        save(
            "result.json",
            json!({"status":"MINIMUM_RULES_RESTART_RECOVERY_PASS","fresh_distribution_runtime":true,"invalid_read_and_write_denied":true,"supported_explicit_recovery":true,"original_rules_and_post_preserved":true,"post_count":1,"restored_write_denied":true,"ai_released":true,"normal_exit_code":0,"native_gui":false,"whole_os_powerloss":false,"formal_ac_accepted":false}),
        )
    })();
    if let Err(e) = &result {
        let closed = if child.try_wait().ok().flatten().is_some() {
            true
        } else if let Some(c) = active.as_ref() {
            c.stop(HUMAN).is_ok() && wait(&mut child).is_ok()
        } else {
            false
        };
        let _ = save(
            "failure.json",
            json!({"error":e,"pid":child.id(),"normal_closeout_or_prior_exit":closed,"force_kill":false,"retry":false}),
        );
    }
    result
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1)
    }
    println!("MINIMUM_RULES_RESTART_RECOVERY_PASS");
}
