//! Fixed disposable-guest probe. Does not reboot, simulate boot, or accept native UI.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[path = "session.rs"]
mod session;
const BASE: &str = "C:/Swarm-Validation/Runs/startup-20261021";
const ROOT: &str = "C:/Swarm-Validation/Runs/startup-20261021/Data";
const STAGE: &str = "C:/Swarm-Validation/Inputs/startup-fixed-20261024";
const CONTINUATION: &str = "C:/Swarm-Validation/Runs/startup-20261021/fixed-20261024";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn require(ok: bool, msg: &str) -> Result<()> {
    if !ok {
        return Err(msg.into());
    }
    Ok(())
}
fn continuing() -> bool {
    std::env::args().nth(1).as_deref() == Some("--continue")
}
fn save(name: &str, v: &Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(if continuing() { CONTINUATION } else { BASE }).join(name))?;
    writeln!(f, "{v}")?;
    Ok(())
}
fn read(name: &str) -> Result<Value> {
    let root = if continuing() && name != "bootstrap.json" {
        CONTINUATION
    } else {
        BASE
    };
    Ok(serde_json::from_slice(&fs::read(
        Path::new(root).join(name),
    )?)?)
}
fn inputs() -> Result<()> {
    require(
        std::env::var("COMPUTERNAME")?.eq_ignore_ascii_case("DESKTOP-1TAP8BI"),
        "Wrong guest hostname",
    )?;
    require(
        std::env::var("USERNAME")?.eq_ignore_ascii_case("GPT"),
        "Wrong guest username",
    )?;
    for (name, hash) in [
        (
            "swarm-desktop.exe",
            "b19d914abf2e4b40652a7cf83064e3a651c903ec9012734b9f920856296ff273",
        ),
        (
            "swarm-runtime.exe",
            "c7b93c9f57314a233e59ff523b03aa0626034bf30f680ac0d16e3bb234325a57",
        ),
        (
            "swarm.exe",
            "ff9a8c4ca4f1f7ae0a83ca78940c5d5aae585e5703b33c87a24270ebc06b5391",
        ),
    ] {
        require(
            format!(
                "{:x}",
                Sha256::digest(fs::read(Path::new(STAGE).join(name))?)
            ) == hash,
            "Candidate pin mismatch",
        )?;
    }
    Ok(())
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn WTSGetActiveConsoleSessionId() -> u32;
}
fn logon_session() -> Result<String> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_STATISTICS, TokenStatistics},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    let mut token = std::ptr::null_mut();
    require(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } != 0,
        "Token query open failed",
    )?;
    let mut statistics: TOKEN_STATISTICS = unsafe { std::mem::zeroed() };
    let mut length = 0;
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenStatistics,
            (&mut statistics as *mut TOKEN_STATISTICS).cast(),
            std::mem::size_of::<TOKEN_STATISTICS>() as u32,
            &mut length,
        )
    };
    unsafe { CloseHandle(token) };
    require(
        ok != 0 && length as usize == std::mem::size_of::<TOKEN_STATISTICS>(),
        "Token statistics query failed",
    )?;
    Ok(format!(
        "{:08x}{:08x}",
        statistics.AuthenticationId.HighPart as u32, statistics.AuthenticationId.LowPart
    ))
}
fn metadata() -> Result<Value> {
    let sessions = session::query()?;
    let interactive=session::select(&sessions).map(|s|json!({"session_id":s.id,"username":s.user,"domain":s.domain,"protocol":s.protocol,"active":true}));
    let logon_session = logon_session()?;
    Ok(
        json!({"boot_id":swarm_platform::lifecycle::boot_id()?,"logon_session":logon_session,"physical_console_id":unsafe{WTSGetActiveConsoleSessionId()},"interactive":interactive,"runtime":swarm_runtime::lifecycle::status(Path::new(ROOT))?,"startup":swarm_runtime::startup::status(Path::new(ROOT),&Path::new(STAGE).join("swarm-runtime.exe"))?,"native_accepted":false}),
    )
}
fn stop_version() -> Result<u8> {
    let bytes = swarm_platform::CredentialStore::open(&Path::new(ROOT).join("lifecycle"))?
        .get("full-stop")?;
    Ok(match bytes.as_deref() {
        None => 0,
        Some(b) if b.starts_with(b"1:") => 1,
        Some(b) if b.starts_with(b"2:") => 2,
        _ => return Err("Unknown stop record".into()),
    })
}
fn baseline() -> Result<Value> {
    let mut value = metadata()?;
    require(
        value["runtime"]["state"] == "stopped"
            && value["startup"]["enabled"] == false
            && value["startup"]["registered"] == false,
        "Retained stopped/disabled baseline required",
    )?;
    require(
        stop_version()? == 1,
        "Retained legacy v1 stop required before explicit human Start",
    )?;
    value["cli"] = cli_stopped()?;
    require(stop_version()? == 1, "CLI changed legacy stop")?;
    for (dir, name) in [
        ("gui-state", ".host.lock"),
        ("credentials", ".broker.lock"),
        ("lifecycle", "runtime.lock"),
        ("lifecycle", "startup.lock"),
        ("lifecycle", "launch.lock"),
    ] {
        let _lease = swarm_platform::CredentialStore::open(&Path::new(ROOT).join(dir))?
            .private_lease(name)?;
    }
    value["legacy_stop_preserved"] = json!(true);
    value["private_existing_leases"] = json!(5);
    value["product_checks_passed"] = json!(true);
    save("baseline.json", &value)?;
    Ok(value)
}
fn cli_stopped() -> Result<Value> {
    let project = read("bootstrap.json")?["project_id"]
        .as_str()
        .ok_or("Project ID missing")?
        .to_owned();
    let mut c = Command::new(Path::new(STAGE).join("swarm.exe"));
    c.args([
        "--data-dir",
        ROOT,
        "--project",
        &project,
        "--session",
        "startup-os-refusal",
        "--format",
        "json",
        "identity",
        "start",
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    let mut child = c.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            save(
                "cli-timeout.json",
                &json!({"pid":child.id(),"retry_allowed":false}),
            )?;
            return Err("CLI timeout; preserve child, no retry".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let o = child.wait_with_output()?;
    let stdout = serde_json::from_slice::<Value>(&o.stdout).ok();
    let stderr = serde_json::from_slice::<Value>(&o.stderr).ok();
    let code = stdout
        .as_ref()
        .or(stderr.as_ref())
        .and_then(|v| v.get("error"))
        .and_then(|v| v.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    require(
        o.status.code() == Some(7) && code == "RUNTIME_STOPPED",
        "CLI did not preserve full stop",
    )?;
    Ok(
        json!({"exit_code":7,"error_code":code,"stdout_bytes":o.stdout.len(),"stderr_bytes":o.stderr.len(),"identity_created":false}),
    )
}
fn observe(phase: &str, recover_manual_start: bool) -> Result<Value> {
    let previous = match phase {
        "b0-running" => "baseline.json",
        "b0-stopped" => "b0-running.json",
        "sameboot" => "b0-stopped.json",
        "enabledboot" => "sameboot.json",
        "disabled-set" => "enabledboot.json",
        "disabledboot" => "disabled-set.json",
        _ => return Err("Unknown phase".into()),
    };
    let prior = read(if recover_manual_start {
        "sameboot-corrected-baseline.json"
    } else {
        previous
    })?;
    let attempt = if recover_manual_start {
        "sameboot-corrected-signout-attempt.json".to_owned()
    } else {
        format!("{phase}-attempt.json")
    };
    save(
        &attempt,
        &json!({"automatic_retries":0,"manual_start_confounded_prior_observation":recover_manual_start}),
    )?;
    let mut v = metadata()?;
    require(
        v["interactive"]["active"] == true,
        "Exactly one active guest GPT session required",
    )?;
    let stopped = matches!(phase, "b0-stopped" | "sameboot" | "disabledboot");
    require(
        v["runtime"]["state"] == if stopped { "stopped" } else { "running" },
        "Runtime state differs from contract",
    )?;
    let enabled = !matches!(phase, "disabled-set" | "disabledboot");
    require(
        v["startup"]["enabled"] == enabled && v["startup"]["registered"] == enabled,
        "Desired/actual startup mismatch",
    )?;
    if phase == "sameboot" {
        require(
            v["boot_id"] == prior["boot_id"],
            "Same-boot logon changed boot GUID",
        )?;
    }
    if matches!(phase, "enabledboot" | "disabledboot") {
        require(
            v["boot_id"] != prior["boot_id"],
            "Actual reboot GUID change required",
        )?;
    }
    if matches!(phase, "b0-stopped" | "disabled-set") {
        require(
            v["boot_id"] == prior["boot_id"],
            "Unexpected reboot between native actions",
        )?;
    }
    if matches!(phase, "sameboot" | "disabledboot") {
        v["cli"] = cli_stopped()?;
        require(
            swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"] == "stopped",
            "CLI restarted runtime",
        )?;
    }
    if !stopped {
        require(
            swarm_runtime::gui::GuiClient::open(Path::new(ROOT))?.setup_status()?["setup_required"]
                == false,
            "Running endpoint/setup response mismatch",
        )?;
    }
    if phase == "b0-running" {
        require(
            stop_version()? == 0,
            "Explicit human Start did not clear legacy record",
        )?;
    }
    if phase == "b0-stopped" {
        require(stop_version()? == 2, "New full stop must persist v2")?;
    }
    if phase == "sameboot" {
        require(
            v["logon_session"].is_string()
                && prior["logon_session"].is_string()
                && v["logon_session"] != prior["logon_session"],
            "Actual new authentication logon session required",
        )?;
    }
    v["stop_record_version"] = json!(stop_version()?);
    v["phase"] = json!(phase);
    v["product_checks_passed"] = json!(true);
    save(&format!("{phase}.json"), &v)?;
    Ok(v)
}
fn cleanup() -> Result<Value> {
    save("cleanup-attempt.json", &json!({"automatic_retries":0}))?;
    swarm_runtime::startup::configure(
        Path::new(ROOT),
        &Path::new(STAGE).join("swarm-runtime.exe"),
        false,
    )?;
    if swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"] == "running" {
        swarm_gui_host::GuiHost::open(Path::new(ROOT))?.stop_runtime()?;
    }
    let identity = swarm_platform::notifications::Identity::new(
        Path::new(ROOT),
        &Path::new(STAGE).join("swarm-desktop.exe"),
    )?;
    swarm_platform::notifications::Registration::new(identity)?.remove()?;
    let v = metadata()?;
    save("cleanup.json", &v)?;
    Ok(v)
}
fn run() -> Result<Value> {
    let mut args: Vec<_> = std::env::args().skip(1).collect();
    inputs()?;
    require(
        continuing(),
        "Fixed observer requires explicit continuation; no bootstrap",
    )?;
    if continuing() {
        args.remove(0);
        require(
            args.first().is_some_and(|m| m != "--bootstrap"),
            "Never replay completed bootstrap",
        )?;
        require(
            Path::new(BASE).join("bootstrap.json").exists()
                && Path::new(BASE).join("cleanup.json").exists(),
            "Prior bootstrap and cleanup proof required",
        )?;
        if !Path::new(CONTINUATION).exists() {
            fs::create_dir(CONTINUATION)?;
        }
    }
    match args.as_slice() {
        [m] if m == "--baseline" => baseline(),
        [m] if m == "--metadata" => metadata(),
        [m, p] if m == "--observe" => observe(p, false),
        [m] if m == "--correct-signout-baseline" => {
            require(
                Path::new(CONTINUATION)
                    .join("sameboot-attempt.json")
                    .is_file()
                    && !Path::new(CONTINUATION).join("sameboot.json").exists(),
                "Prior wrong-boundary failure required",
            )?;
            let mut v = metadata()?;
            require(
                v["runtime"]["state"] == "stopped"
                    && v["startup"]["enabled"] == true
                    && v["startup"]["registered"] == true,
                "Stopped/enabled prerequisite required",
            )?;
            let marker = swarm_platform::CredentialStore::open(&Path::new(ROOT).join("lifecycle"))?
                .get("full-stop")?
                .ok_or("Missing stop")?;
            require(
                marker == format!("2:{}", v["boot_id"].as_str().ok_or("Missing boot")?).as_bytes(),
                "Fresh native full stop on this boot required",
            )?;
            v["product_checks_passed"] = json!(true);
            v["prerequisite_only"] = json!(true);
            v["stop_record_version"] = json!(2);
            save("sameboot-corrected-baseline.json", &v)?;
            Ok(v)
        }
        [m] if m == "--recover-corrected-signout" => {
            require(
                continuing()
                    && Path::new(CONTINUATION)
                        .join("sameboot-attempt.json")
                        .exists()
                    && !Path::new(CONTINUATION).join("sameboot.json").exists(),
                "Prior failed sameboot observation required",
            )?;
            observe("sameboot", true)
        }
        [m] if m == "--cleanup" => cleanup(),
        _ => Err("Explicit fixed mode required".into()),
    }
}
fn main() {
    match run() {
        Ok(v) => println!("{v}"),
        Err(error) => {
            let message = error.to_string();
            let code = match message.as_str() {
                "Same-boot logon changed boot GUID" => "wrong_os_boundary",
                "Actual new authentication logon session required" => "new_logon_not_proven",
                "Runtime state differs from contract" => "runtime_state_contract",
                "Desired/actual startup mismatch" => "startup_state_contract",
                "Actual reboot GUID change required" => "reboot_not_proven",
                "Fresh native full stop on this boot required" => "fresh_stop_prerequisite",
                "Exactly one active guest GPT session required" => "active_session_prerequisite",
                _ => "observer_dependency_or_storage",
            };
            eprintln!(
                "{}",
                json!({"guard_code":code,"io_kind":error.downcast_ref::<std::io::Error>().map(|e|format!("{:?}",e.kind()))})
            );
            std::process::exit(1)
        }
    }
}
