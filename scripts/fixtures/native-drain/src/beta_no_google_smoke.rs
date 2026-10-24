//! Final staged-binary beta check. No external auth, GUI input, install or old fixture access.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
use swarm_runtime::gui::GuiClient;
type Result<T> = std::result::Result<T, String>;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261024-beta-no-google-smoke";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261024-beta-no-google";
const ALIAS: &str = "beta-no-google-admin";
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
fn save(n: &str, v: Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(BASE).join(n))
        .map_err(|_| "preserve receipt")?;
    writeln!(f, "{v}").map_err(|_| "receipt write".into())
}
fn wait(child: &mut Child) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(s) = child.try_wait().map_err(|_| "child wait")? {
            return ensure(s.success(), "nonzero runtime");
        };
        ensure(
            Instant::now() < end,
            "uncertain child; preserve without force kill",
        )?;
        std::thread::sleep(Duration::from_millis(25));
    }
}
fn run() -> Result<()> {
    ensure(
        std::env::args().skip(1).collect::<Vec<_>>() == ["--execute-once"],
        "fixed execute-once only",
    )?;
    ensure(!Path::new(BASE).exists(), "preserve existing attempt")?;
    let report: Value = serde_json::from_slice(
        &fs::read(Path::new(CANDIDATE).join("package-report.json"))
            .map_err(|_| "candidate report")?,
    )
    .map_err(|_| "report format")?;
    ensure(
        report["candidate_only"] == true && report["release_approved"] == false,
        "candidate publication flags",
    )?;
    let expected = report["files"]
        .as_array()
        .ok_or("file records")?
        .iter()
        .find(|r| r["path"] == "swarm-runtime.exe")
        .and_then(|r| r["sha256"].as_str())
        .ok_or("runtime pin")?;
    let executable = Path::new(CANDIDATE).join("swarm-runtime.exe");
    ensure(
        format!(
            "{:x}",
            Sha256::digest(fs::read(&executable).map_err(|_| "runtime missing")?)
        ) == expected,
        "runtime changed",
    )?;
    fs::create_dir(BASE).map_err(|_| "exclusive base")?;
    save(
        "attempt.json",
        json!({"candidate":CANDIDATE,"runtime_sha256":expected,"external_auth":false,"native_gui":false}),
    )?;
    let root = Path::new(BASE).join("Data");
    {
        let mut s = fault(Service::open(&root))?;
        fault(s.set_startup_preference(false))?;
    }
    let mut c = Command::new(&executable);
    c.arg("--data-dir")
        .arg(&root)
        .arg("--human-start")
        .stdin(Stdio::null());
    use std::os::windows::process::CommandExt;
    c.creation_flags(0x08000000);
    for (name, out) in [("stdout", true), ("stderr", false)] {
        let f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(Path::new(BASE).join(format!("runtime.{name}.log")))
            .map_err(|_| "preserve streams")?;
        if out {
            c.stdout(f);
        } else {
            c.stderr(f);
        }
    }
    let mut child = c.spawn().map_err(|_| "runtime launch")?;
    save("runtime.json", json!({"pid":child.id()}))?;
    let mut active = None;
    let result = (|| {
        let end = Instant::now() + Duration::from_secs(15);
        loop {
            ensure(
                child.try_wait().map_err(|_| "child wait")?.is_none(),
                "runtime early exit",
            )?;
            if let Ok(client) = GuiClient::open(&root) {
                let ready = client.setup_status().is_ok();
                active = Some(client);
                if ready {
                    break;
                }
            }
            ensure(Instant::now() < end, "readiness deadline")?;
            std::thread::sleep(Duration::from_millis(25));
        }
        let client = active.as_ref().unwrap();
        let account = fault(client.issue(
            ALIAS,
            &"a".repeat(64),
            req(
                "account.setup",
                json!({"name":"Beta Smoke Admin","password":"fixture-beta-password"}),
            ),
        ))?;
        let before = fault(client.request(ALIAS, req("account.status", json!({}))))?;
        for (op, args) in [
            (
                "account.google.begin",
                json!({"name":"Beta Smoke Admin","purpose":"link","current_password":"fixture-beta-password"}),
            ),
            (
                "account.google.link",
                json!({"challenge":"a".repeat(64),"id_token":"synthetic-never-exchanged"}),
            ),
            (
                "account.google.unlink",
                json!({"current_password":"fixture-beta-password"}),
            ),
        ] {
            ensure(
                client
                    .request(ALIAS, req(op, args))
                    .is_err_and(|e| e.code == "FEATURE_UNAVAILABLE"),
                "staged Google operation not disabled",
            )?;
        }
        ensure(client.issue(ALIAS,&"b".repeat(64),req("account.google.recover",json!({"name":"Beta Smoke Admin","password":"must-not-change","challenge":"a".repeat(64),"id_token":"synthetic-never-exchanged"}))).is_err_and(|e|e.code=="FEATURE_UNAVAILABLE"),"staged Google recovery not disabled")?;
        ensure(
            client
                .google_begin_recovery("Beta Smoke Admin")
                .is_err_and(|e| e.code == "FEATURE_UNAVAILABLE"),
            "private recovery begin not disabled",
        )?;
        ensure(
            fault(client.request(ALIAS, req("account.status", json!({}))))? == before,
            "Google denial changed account",
        )?;
        let backup = fault(client.backup(
            ALIAS,
            account["id"].as_str().ok_or("account ID")?,
            "settings.get",
            json!({}),
        ))?;
        ensure(
            backup["enabled"].is_boolean(),
            "backup settings unavailable",
        )?;
        let code = fault(client.reveal_recovery_code(ALIAS))?["recovery_code"]
            .as_str()
            .ok_or("recovery code absent")?
            .to_owned();
        let recovered=fault(client.issue(ALIAS,&"c".repeat(64),req("account.recover",json!({"name":"Beta Smoke Admin","password":"fixture-beta-recovered","recovery_code":code}))))?;
        ensure(
            recovered["id"] == account["id"],
            "ordinary code recovery changed account",
        )?;
        fault(client.request(ALIAS, req("account.logout", json!({}))))?;
        let login = fault(client.issue(
            ALIAS,
            &"d".repeat(64),
            req(
                "account.login",
                json!({"name":"Beta Smoke Admin","password":"fixture-beta-recovered"}),
            ),
        ))?;
        ensure(
            login["id"] == account["id"],
            "ordinary login changed account",
        )?;
        fault(client.stop(ALIAS))?;
        wait(&mut child)?;
        ensure(
            !swarm_platform::startup::Registration::new(&executable, &root)
                .and_then(|r| r.is_registered())
                .map_err(|_| "startup inspection")?,
            "startup registered",
        )?;
        save(
            "result.json",
            json!({"status":"STAGED_BETA_NO_GOOGLE_PASS","google_account_operations_denied":4,"private_recovery_begin_denied":true,"account_unchanged_by_denials":true,"normal_password_login":true,"normal_recovery_code":true,"backup_settings_accessible":true,"normal_exit_code":0,"startup_registered":false,"external_google_request":false,"native_gui":false,"formal_ac_accepted":false}),
        )
    })();
    if let Err(e) = &result {
        let closed = if child.try_wait().ok().flatten().is_some() {
            true
        } else if let Some(c) = active.as_ref() {
            c.stop(ALIAS).is_ok() && wait(&mut child).is_ok()
        } else {
            false
        };
        let _ = save(
            "failure.json",
            json!({"error":e,"pid":child.id(),"normal_closeout_or_prior_exit":closed,"force_kill":false}),
        );
    }
    result
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1)
    }
    println!("STAGED_BETA_NO_GOOGLE_PASS");
}
