//! Fixed isolated frozen-runtime stop fence. No transaction fault injection/native input.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use swarm_core::{Service, protocol::Request};
use swarm_platform::{CredentialStore, pipe};
use swarm_runtime::gui::GuiClient;

type Result<T> = std::result::Result<T, String>;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261024-frozen-queued-stop";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261024-run-path/swarm-runtime.exe";
const PIN: &str = "1ee7838beb28752558ab48a4a7687b2347d6c5309e756f9d6cf9eef2599d03ce";
const ALIAS: &str = "queued-stop-admin";
// Explicit mutation/control opcode cases. Empty inner arguments intentionally
// establish rejection before validation, not successful execution of each opcode.
const CORE: &[&str] = &[
    "account.setup",
    "account.login",
    "account.create",
    "account.recover",
    "account.google.begin",
    "account.google.link",
    "account.google.unlink",
    "account.google.recover",
    "account.reset",
    "account.password",
    "account.disable",
    "account.logout",
    "role.create",
    "role.assign",
    "permission.set",
    "rules.save",
    "project.register",
    "project.alias",
    "project.root",
    "identity.start",
    "identity.resume",
    "identity.release",
    "identity.model",
    "board.create",
    "board.edit",
    "board.delete",
    "board.restore",
    "board.purge",
    "thread.create",
    "thread.edit",
    "thread.delete",
    "thread.restore",
    "thread.purge",
    "conversation.batch",
    "thread.pin",
    "thread.subscribe",
    "post.read.mark",
    "notification.settings.set",
    "retention.settings.set",
    "display.settings.set",
    "ui.preferences.set",
    "post.create",
    "response.create",
    "post.edit",
    "post.delete",
    "post.restore",
];
const OUTER: &[&str] = &[
    "gui.issue",
    "gui.request",
    "gui.window_geometry_set",
    "gui.startup_preference_set",
    "gui.reveal_recovery_code",
    "gui.runtime_stop",
    "gui.google_begin_recovery",
];
const BACKUP: &[&str] = &[
    "settings.set",
    "manual.create",
    "archives.delete",
    "restore.prepare",
    "restore.apply",
    "restore.cleanup",
];

fn require(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(message.into()) }
}
fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .map_err(|_| "clock unavailable".into())
}
fn save(name: &str, value: Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(BASE).join(name))
        .map_err(|_| "preserve existing receipt")?;
    writeln!(f, "{value}").map_err(|_| "receipt write failed".into())
}
fn request(op: &str, args: Value) -> Request {
    Request {
        operation: op.into(),
        args,
        auth: None,
        context: json!({}),
        request_id: None,
    }
}
fn fault<T>(r: std::result::Result<T, swarm_core::protocol::Fault>) -> Result<T> {
    r.map_err(|e| e.code)
}
fn pins() -> Result<()> {
    let bytes = fs::read(CANDIDATE).map_err(|_| "candidate missing")?;
    require(
        format!("{:x}", Sha256::digest(bytes)) == PIN,
        "candidate changed",
    )
}
fn spawn(root: &Path, phase: usize) -> Result<Child> {
    pins()?;
    let out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(BASE).join(format!("phase-{phase}-stdout.log")))
        .map_err(|_| "preserve stdout")?;
    let err = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(BASE).join(format!("phase-{phase}-stderr.log")))
        .map_err(|_| "preserve stderr")?;
    let mut cmd = Command::new(CANDIDATE);
    cmd.args(["--data-dir"])
        .arg(root)
        .arg("--human-start")
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.spawn().map_err(|_| "runtime spawn failed".into())
}
fn ready(root: &Path, child: &mut Child, active: &mut Option<GuiClient>) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        require(
            child.try_wait().map_err(|_| "child wait failed")?.is_none(),
            "runtime exited before ready",
        )?;
        if let Ok(client) = GuiClient::open(root) {
            let available = client.setup_status().is_ok();
            // Retain the current owner's handle even if readiness fails, so
            // normal cleanup can still be attempted. Never force a stuck child.
            *active = Some(client);
            if available {
                return Ok(());
            }
        }
        require(Instant::now() < end, "readiness deadline")?;
        std::thread::sleep(Duration::from_millis(40));
    }
}
fn wait(child: &mut Child) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(code) = child.try_wait().map_err(|_| "child wait failed")? {
            return require(code.success(), "runtime nonzero exit");
        }
        require(
            Instant::now() < end,
            "runtime uncertain; preserve process, no force kill",
        )?;
        std::thread::sleep(Duration::from_millis(30));
    }
}
struct Case {
    label: String,
    gui: bool,
    request: Request,
}
fn cases(capability: &str, account: &Value) -> Vec<Case> {
    let mut result = Vec::new();
    for op in CORE {
        result.push(Case {
            label: format!("public:{op}"),
            gui: false,
            request: request(op, json!({})),
        });
        let mut outer = request(
            "gui.request",
            json!({"alias":ALIAS,"request":request(op,json!({}))}),
        );
        outer.auth = Some(capability.into());
        result.push(Case {
            label: format!("human:{op}"),
            gui: true,
            request: outer,
        });
    }
    for op in ["session.start", "session.resume"] {
        result.push(Case {
            label: format!("public:{op}"),
            gui: false,
            request: request(op, json!({})),
        });
    }
    for op in OUTER {
        let mut r = request(op, json!({}));
        r.auth = Some(capability.into());
        result.push(Case {
            label: format!("outer:{op}"),
            gui: true,
            request: r,
        });
    }
    for op in BACKUP {
        let mut r = request(
            "gui.backup",
            json!({"alias":ALIAS,"account_id":account,"operation":op,"args":{}}),
        );
        r.auth = Some(capability.into());
        result.push(Case {
            label: format!("backup:{op}"),
            gui: true,
            request: r,
        });
    }
    result
}
fn positive(client: &GuiClient, expected: &Value) -> Result<()> {
    let p = fault(client.request(
        ALIAS,
        request(
            "post.get",
            json!({"thread_id":expected["thread_id"],"no":1}),
        ),
    ))?;
    require(p == *expected, "original control Post changed")
}
fn phase(
    root: &Path,
    client: &GuiClient,
    batch: &[Case],
    number: usize,
    child: &mut Child,
) -> Result<()> {
    let mut pending = Vec::new();
    for case in batch {
        let name = if case.gui {
            fault(swarm_runtime::gui_pipe_name(root))?
        } else {
            fault(swarm_runtime::pipe_name(root))?
        };
        let connection = pipe::connect(&name).map_err(|_| "queue connect failed")?;
        // Connecting to the successor instance proves the prior connection was
        // handed to a runtime worker. Do not confuse kernel connect with admission.
        let successor = pipe::connect(&name).map_err(|_| "successor admission proof failed")?;
        drop(successor);
        pending.push((case, connection));
    }
    save(
        &format!("phase-{number}-admitted.json"),
        json!({"count":pending.len(),"at":now()?,"request_payloads_sent":false,"successor_admission_proof":true}),
    )?;
    require(
        fault(client.stop(ALIAS))? == json!({"state":"stopped"}),
        "stop not acknowledged",
    )?;
    require(
        fault(swarm_runtime::lifecycle::status(root))?["state"] == "stopped",
        "stop marker absent",
    )?;
    require(
        child.try_wait().map_err(|_| "child wait failed")?.is_none(),
        "runtime failed to retain pending connections",
    )?;
    for (case, mut connection) in pending {
        swarm_ipc::write_request(&mut connection, &case.request).map_err(|e| e.code)?;
        let response = swarm_ipc::read_response(&mut connection).map_err(|e| e.code)?;
        require(
            !response.ok
                && response
                    .error
                    .as_ref()
                    .is_some_and(|e| e.code == "RUNTIME_STOPPED"),
            "pending opcode not rejected by stop fence",
        )?;
        save(
            &format!("case-{}.json", case.label.replace(':', "-")),
            json!({"case":case.label,"code":"RUNTIME_STOPPED","accepted_connection_before_stop":true,"payload_sent_after_stop_ack":true,"inner_arguments_validated":false}),
        )?;
    }
    wait(child)?;
    save(
        &format!("phase-{number}-result.json"),
        json!({"status":"FROZEN_PENDING_OPCODE_STOP_PASS","count":batch.len(),"normal_exit_code":0,"stop_persisted":true,"native_gui_observed":false}),
    )
}
fn run() -> Result<()> {
    require(
        std::env::args().skip(1).collect::<Vec<_>>() == ["--execute-once"],
        "fixed execute-once only",
    )?;
    pins()?;
    require(
        !Path::new(BASE).exists(),
        "preserve existing fixture/attempt; no replay",
    )?;
    fs::create_dir(BASE).map_err(|_| "exclusive base creation failed")?;
    save(
        "attempt.json",
        json!({"at":now()?,"candidate":CANDIDATE,"sha256":PIN,"native_gui":false,"existing_fixtures_touched":false}),
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
        let client = active.as_ref().unwrap();
        let setup = fault(client.issue(
            ALIAS,
            &"a".repeat(64),
            request(
                "account.setup",
                json!({"name":"Queued Stop Admin","password":"fixture-only-password"}),
            ),
        ))?;
        let board = fault(client.request(
            ALIAS,
            request("board.create", json!({"name":"Stop fence control"})),
        ))?;
        let thread = fault(client.request(
            ALIAS,
            request(
                "thread.create",
                json!({"board_id":board["id"],"name":"Preserve this control"}),
            ),
        ))?;
        let post = fault(client.request(ALIAS,request("post.create",json!({"thread_id":thread["id"],"body":"Synthetic stop-fence control; retain unchanged."}))))?;
        let expected = fault(client.request(
            ALIAS,
            request("post.get", json!({"thread_id":thread["id"],"no":1})),
        ))?;
        require(post["id"] == expected["id"], "control creation mismatch")?;
        let cap = CredentialStore::open(&root.join("gui-host"))
            .map_err(|_| "capability store unavailable")?
            .get("transport-capability")
            .map_err(|_| "capability read failed")?
            .ok_or("capability absent")?;
        let cap = std::str::from_utf8(&cap).map_err(|_| "invalid capability")?;
        require(setup["id"].as_str().is_some(), "setup account ID absent")?;
        let cases = cases(cap, &setup["id"]);
        require(
            cases
                .iter()
                .map(|c| &c.label)
                .collect::<std::collections::HashSet<_>>()
                .len()
                == cases.len(),
            "duplicate case",
        )?;
        save(
            "scope.json",
            json!({"case_count":cases.len(),"operations":cases.iter().map(|c| &c.label).collect::<Vec<_>>(),"scope":"opcode-independent pending connection rejection, not active transaction/all valid invocations"}),
        )?;
        for (number, batch) in cases.chunks(16).enumerate() {
            if number > 0 {
                child = spawn(&root, number)?;
                ready(&root, &mut child, &mut active)?;
            }
            let client = active.as_ref().unwrap();
            positive(client, &expected)?;
            phase(&root, client, batch, number, &mut child)?;
        }
        let final_phase = cases.len().div_ceil(16);
        child = spawn(&root, final_phase)?;
        ready(&root, &mut child, &mut active)?;
        let client = active.as_ref().unwrap();
        positive(client, &expected)?;
        require(
            fault(client.startup_preference_get())? == Some(false),
            "fixture startup preference changed",
        )?;
        require(
            fault(client.request(
                ALIAS,
                request("post.list", json!({"thread_id":thread["id"],"limit":10})),
            ))?["items"]
                .as_array()
                .is_some_and(|a| a.len() == 1),
            "extra control Posts",
        )?;
        fault(client.stop(ALIAS))?;
        wait(&mut child)?;
        pins()?;
        let registered = swarm_platform::startup::Registration::new(Path::new(CANDIDATE), &root)
            .and_then(|r| r.is_registered())
            .map_err(|_| "startup registration inspection failed")?;
        require(!registered, "fixture startup registration remains")?;
        save(
            "result.json",
            json!({"status":"FROZEN_PENDING_OPCODE_STOP_PASS","cases":cases.len(),"control_post_unchanged":true,"post_count":1,"normal_exit_code":0,"startup_enabled":false,"startup_registered":registered,"candidate_preserved":true,"native_gui_observed":false,"formal_ac_accepted":false,"started_transaction_observed":false}),
        )
    })();
    if let Err(error) = &result {
        let already_exited = child.try_wait().ok().flatten().is_some();
        let stopped = if already_exited {
            true
        } else if swarm_runtime::lifecycle::status(&root).is_ok_and(|v| v["state"] == "stopped") {
            wait(&mut child).is_ok()
        } else if let Some(c) = active.as_ref() {
            c.stop(ALIAS).is_ok() && wait(&mut child).is_ok()
        } else {
            false
        };
        let _ = save(
            "failure.json",
            json!({"error":error,"pid":child.id(),"normal_closeout_or_prior_exit":stopped,"force_kill":false,"retry":false}),
        );
    }
    result
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
    println!("FROZEN_PENDING_OPCODE_STOP_PASS");
}
