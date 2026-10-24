//! Native reauthentication only; never repeats restore/fault or retrieves secrets.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use swarm_gui_host::GuiHost;
use swarm_platform::{
    CredentialStore,
    notifications::{Identity, Registration},
};
use swarm_runtime::gui::GuiClient;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261021-native-restore-conflict";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261021-native-restore-conflict/Data";
const OUT: &str =
    "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/restore-native-reauth-20261022";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
fn ensure(value: bool, message: &str) -> Result<()> {
    if value { Ok(()) } else { Err(message.into()) }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn save(name: &str, value: &Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(Path::new(OUT).join(name))?;
    writeln!(f, "{value}")?;
    Ok(())
}
fn read(name: &str) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(
        Path::new(OUT).join(name),
    )?)?)
}
fn prepared() -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(
        Path::new(BASE).join("frozen-prepared.json"),
    )?)?)
}
fn alias() -> Result<String> {
    let value: Value = serde_json::from_slice(
        &CredentialStore::open(&Path::new(ROOT).join("gui-state"))?
            .get("selection")?
            .ok_or("selection absent")?,
    )?;
    Ok(value["alias"].as_str().ok_or("alias absent")?.into())
}
fn registration() -> Result<Registration> {
    Ok(Registration::new(Identity::new(
        Path::new(ROOT),
        &Path::new(CANDIDATE).join("swarm-desktop.exe"),
    )?)?)
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
            "candidate hash changed",
        )?;
    }
    Ok(())
}
fn compare_posts(h: &mut GuiHost, p: &Value) -> Result<Value> {
    let posts = h.read("post.list", json!({"thread_id":p["thread"]}))?;
    let items = posts["items"].as_array().ok_or("Posts absent")?;
    ensure(items.len() == 2, "expected two original Posts")?;
    let mut safe = Vec::new();
    for expected in [&p["live"], &p["extra"]] {
        let actual = items
            .iter()
            .find(|v| v["id"] == expected["id"])
            .ok_or("original Post missing")?;
        for field in ["id", "body", "revision", "no"] {
            ensure(actual[field] == expected[field], "original Post changed")?;
        }
        safe.push(json!({"id":actual["id"],"revision":actual["revision"],"no":actual["no"],"body_sha256":hash(actual["body"].as_str().ok_or("body absent")?.as_bytes())}));
    }
    Ok(json!(safe))
}
fn spawn(name: &str, executable: &str) -> Result<std::process::Child> {
    let mut command = Command::new(Path::new(CANDIDATE).join(executable));
    command.args(["--data-dir", ROOT]);
    if name == "runtime" {
        command.arg("--human-start");
    }
    command
        .stdin(Stdio::null())
        .stdout(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(OUT).join(format!("{name}.stdout.log")))?,
        )
        .stderr(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(Path::new(OUT).join(format!("{name}.stderr.log")))?,
        );
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    Ok(command.spawn()?)
}
fn prepare() -> Result<()> {
    check_candidate()?;
    ensure(!Path::new(OUT).exists(), "existing reauth run refused")?;
    let prior: Value =
        serde_json::from_slice(&fs::read(Path::new(BASE).join("frozen-recovered.json"))?)?;
    ensure(
        prior["state"] == "rolled_back"
            && prior["active_removed"] == true
            && prior["two_live_posts_unchanged"] == true,
        "prior frozen recovery absent",
    )?;
    ensure(
        swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"] == "stopped",
        "existing runtime not stopped",
    )?;
    ensure(
        !registration()?.is_registered()?,
        "unexpected owned registration",
    )?;
    fs::create_dir(OUT)?;
    save(
        "attempt.json",
        &json!({"scope":"native-reauth-only-on-recovered-data","restore_replayed":false,"fault_replayed":false,"known_empty_synthetic_password":true,"vm":false,"elapsed":false}),
    )?;
    let mut runtime = spawn("runtime", "swarm-runtime.exe")?;
    save(
        "runtime.json",
        &json!({"pid":runtime.id(),"data_root":ROOT,"candidate":CANDIDATE}),
    )?;
    let until = Instant::now() + Duration::from_secs(15);
    while GuiClient::open(Path::new(ROOT))
        .and_then(|c| c.setup_status())
        .is_err()
    {
        ensure(Instant::now() < until, "runtime readiness timeout")?;
        if runtime.try_wait()?.is_some() {
            return Err("owned runtime exited before readiness".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let p = prepared()?;
    let mut h = GuiHost::open(Path::new(ROOT))?;
    let before = compare_posts(&mut h, &p)?;
    ensure(
        h.restore_status(p["account"].as_str().ok_or("account absent")?)?["state"] == "rolled_back",
        "rollback terminal changed",
    )?;
    let rules_hash = hash(&fs::read(Path::new(ROOT).join("RULES.md"))?);
    h.logout()?;
    ensure(
        CredentialStore::open(&Path::new(ROOT).join("gui-state"))?
            .get("selection")?
            .is_none(),
        "normal logout retained selection",
    )?;
    save(
        "ready.json",
        &json!({"runtime_pid":runtime.id(),"account_id":p["account"],"thread_id":p["thread"],"posts":before,"rules_sha256":rules_hash,"selected_alias_absent":true,"state":"WAIT_USER_NATIVE_LOGIN","preparation_logout_only":true}),
    )?;
    drop(h);
    let desktop = spawn("desktop", "swarm-desktop.exe")?;
    save(
        "desktop.json",
        &json!({"pid":desktop.id(),"data_root":ROOT,"candidate":CANDIDATE}),
    )?;
    println!("WAIT_USER_NATIVE_LOGIN");
    Ok(())
}
fn verify() -> Result<()> {
    check_candidate()?;
    let confirmation = read("native-user-confirmation.json")?;
    ensure(
        confirmation["user_reported_login"] == true,
        "native user confirmation absent",
    )?;
    let p = prepared()?;
    let ready = read("ready.json")?;
    let mut h = GuiHost::open(Path::new(ROOT))?;
    ensure(
        h.snapshot()?["account"]["id"] == p["account"],
        "native login account mismatch",
    )?;
    let posts = compare_posts(&mut h, &p)?;
    ensure(posts == ready["posts"], "saved Posts differ")?;
    ensure(
        h.restore_status(p["account"].as_str().ok_or("account absent")?)?["state"] == "rolled_back",
        "rollback terminal changed",
    )?;
    ensure(
        hash(&fs::read(Path::new(ROOT).join("RULES.md"))?) == ready["rules_sha256"],
        "RULES changed",
    )?;
    save(
        "verified.json",
        &json!({"status":"NATIVE_REAUTH_SAME_RECOVERED_DATA_VERIFIED","account_id":p["account"],"thread_id":p["thread"],"posts":posts,"same_rollback_terminal":true,"rules_unchanged":true,"helper_authenticated":false,"preparation_logout_only":true,"restore_fault_not_replayed":true,"native_visual_receipt_required":true}),
    )?;
    println!("NATIVE_REAUTH_SAME_RECOVERED_DATA_VERIFIED");
    Ok(())
}
fn stop() -> Result<()> {
    GuiClient::open(Path::new(ROOT))?.stop(&alias()?)?;
    let until = Instant::now() + Duration::from_secs(15);
    while swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"] != "stopped" {
        ensure(
            Instant::now() < until,
            "normal stop deadline; preserve process",
        )?;
        std::thread::sleep(Duration::from_millis(80));
    }
    registration()?.remove()?;
    ensure(!registration()?.is_registered()?, "registration remains")?;
    save(
        "closeout.json",
        &json!({"normally_stopped":true,"owned_registration_absent":true}),
    )?;
    println!("REAUTH_NORMAL_CLOSEOUT");
    Ok(())
}
fn main() {
    let result = match std::env::args().nth(1).as_deref() {
        Some("--prepare") => prepare(),
        Some("--verify") => verify(),
        Some("--stop") => stop(),
        _ => Err("choose --prepare, --verify or --stop".into()),
    };
    if let Err(error) = result {
        eprintln!("REAUTH_NOT_ACCEPTED: {error}");
        std::process::exit(1);
    }
}
