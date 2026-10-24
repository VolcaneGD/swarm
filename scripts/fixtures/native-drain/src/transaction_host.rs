//! Fixed native pre-commit fixture orchestration. No H1/old response-drain access.
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use swarm_gui_host::GuiHost;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261022-native-transaction-drain";
const GUI: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback/swarm-desktop.exe";
fn ensure(b: bool, m: &str) -> Result<()> {
    if b { Ok(()) } else { Err(m.into()) }
}
fn save(base: &Path, n: &str, v: &Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(base.join(n))?;
    writeln!(f, "{v}")?;
    Ok(())
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ensure(
        args.len() == 2
            && matches!(
                args[0].as_str(),
                "commit" | "rollback" | "rollback-diagnostic"
            ),
        "mode must be an explicit native transaction case",
    )?;
    let base = Path::new(BASE).join(&args[0]);
    let root = base.join("Data");
    ensure(
        root.is_dir() && base.join("server-ready.json").exists(),
        "native worker not ready",
    )?;
    let rollback = args[0] != "commit";
    match args[1].as_str() {
        "login" => {
            ensure(
                !base.join("arm-native.json").exists() && !base.join("host-login.json").exists(),
                "already armed/logged in",
            )?;
            let mut h = GuiHost::open(&root)?;
            let v = h.authenticate("login", "Drain Fixture Admin", "fixture-password-only")?;
            ensure(
                v["account"]["id"].as_str().is_some(),
                "account not selected",
            )?;
            h.manage(
                "display.settings.set",
                json!({"language":"ja","theme":"light"}),
            )?;
            save(
                &base,
                "host-login.json",
                &json!({"normal_api_login":true,"native_auth_input":false,"account_id":v["account"]["id"],"root":root}),
            )?;
            println!("NORMAL_HOST_LOGIN_READY");
        }
        "arm" => {
            ensure(
                base.join("host-login.json").exists()
                    && !base.join("transaction-entered.json").exists(),
                "not in pre-arm state",
            )?;
            save(
                &base,
                "arm-native.json",
                &json!({"root":root,"rollback":rollback}),
            )?;
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                if base.join("transaction-entered.json").exists() {
                    break;
                }
                ensure(
                    Instant::now() < until,
                    "transaction did not enter; no native click",
                )?;
                std::thread::sleep(Duration::from_millis(10));
            }
            let v: Value =
                serde_json::from_slice(&fs::read(base.join("transaction-entered.json"))?)?;
            ensure(
                v["pre_commit"] == true
                    && v["rollback"] == rollback
                    && v["stop_marker_present"] == false,
                "entry receipt mismatch",
            )?;
            println!("NATIVE_PRECOMMIT_READY");
        }
        "verify" => {
            let v: Value = serde_json::from_slice(&fs::read(base.join("result.json"))?)?;
            let before: Value =
                serde_json::from_slice(&fs::read(base.join("stop-before-release.json"))?)?;
            ensure(
                v["status"] == "NATIVE_PRECOMMIT_STOP_PASS"
                    && v["rollback"] == rollback
                    && v["credential_valid_after_reopen"] == !rollback
                    && v["stop_persisted"] == true
                    && v["listeners_drained"] == true,
                "native transaction result mismatch",
            )?;
            ensure(
                before["native_stop_received"] == true
                    && before["stop_marker_present"] == false
                    && before["transaction_still_owns_gate"] == true,
                "stop did not precede transaction release",
            )?;
            ensure(
                swarm_runtime::lifecycle::status(&root)?["state"] == "stopped",
                "human stop not retained",
            )?;
            save(
                &base,
                "host-verified.json",
                &json!({"status":"NATIVE_PRECOMMIT_STOP_PASS","rollback":rollback,"native_stop_before_release":true,"reopened_candidate_valid":!rollback,"stopped":true,"frozen_runtime_executable":false}),
            )?;
            println!("NATIVE_PRECOMMIT_STOP_PASS");
        }
        "cleanup-registration" => {
            let r = swarm_platform::notifications::Registration::new(
                swarm_platform::notifications::Identity::new(&root, &PathBuf::from(GUI))?,
            )?;
            r.remove()?;
            ensure(!r.is_registered()?, "registration remains")?;
            save(
                &base,
                "registration-closeout.json",
                &json!({"owned_registration_absent":true,"data_preserved":true}),
            )?;
        }
        _ => return Err("unknown phase".into()),
    }
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        if let Some(f) = e.downcast_ref::<swarm_core::protocol::Fault>() {
            eprintln!("Product fault code: {}", f.code);
        }
        eprintln!("Native transaction host guard failed; preserve all owned evidence");
        std::process::exit(1);
    }
}
