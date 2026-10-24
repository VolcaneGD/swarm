#![windows_subsystem = "windows"]
//! Scoped failed-startup inspection and separately selected direct invocation.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::os::windows::process::CommandExt;
use std::{
    fs,
    fs::OpenOptions,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const DATA: &str = "C:/Swarm-Validation/Runs/startup-20261021/Data";
const RECEIPTS: &str = "C:/Swarm-Validation/Runs/startup-20261021/fixed-20261024";
const STAGE: &str = "C:/Swarm-Validation/Inputs/startup-fixed-20261024";
#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(window: isize, text: *const u16, title: *const u16, kind: u32) -> i32;
}
fn save(name: &str, value: &Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(RECEIPTS).join(name))?;
    f.write_all(serde_json::to_string_pretty(value)?.as_bytes())?;
    Ok(())
}
fn metadata() -> Result<Value> {
    Ok(
        json!({"boot_id":swarm_platform::lifecycle::boot_id()?,"runtime":swarm_runtime::lifecycle::status(Path::new(DATA))?,"startup":swarm_runtime::startup::status(Path::new(DATA),&Path::new(STAGE).join("swarm-runtime.exe"))?}),
    )
}
fn registry() -> Result<Value> {
    let script = r#"$ErrorActionPreference='Stop';$k=Get-Item 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run';$matches=@();foreach($n in $k.GetValueNames()){if($n -like 'Swarm-*'){$v=$k.GetValue($n);if($v -is [string] -and $v.Contains('startup-fixed-20261024')){$matches+=@{name=$n;command=$v}}}};if($matches.Count -ne 1){throw 'One task-owned startup registration required'};$name=$matches[0].name;$a=Get-Item 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run' -ErrorAction SilentlyContinue;$bytes=if($a){$a.GetValue($name)}else{$null};@{registration=$matches[0];startup_approved_hex=if($bytes){[BitConverter]::ToString($bytes)}else{$null};last_boot_utc=(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o')}|ConvertTo-Json -Compress -Depth 5"#;
    let mut child = Command::new("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000)
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while child.try_wait()?.is_none() {
        if Instant::now() > deadline {
            return Err("Registry read timeout; preserve process".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output()?;
    if !output.status.success() || !output.stderr.is_empty() || output.stdout.len() > 65536 {
        return Err("Scoped registry read failed".into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
fn run() -> Result<Value> {
    if std::env::var("COMPUTERNAME")?.to_uppercase() != "DESKTOP-1TAP8BI"
        || std::env::var("USERNAME")?.to_uppercase() != "GPT"
    {
        return Err("Wrong guest".into());
    }
    let exe = std::env::current_exe()?;
    if exe.parent().ok_or("Missing stage")?.canonicalize()? != Path::new(STAGE).canonicalize()? {
        return Err("Wrong stage".into());
    }
    if !Path::new(RECEIPTS)
        .join("enabledboot-attempt.json")
        .is_file()
        || Path::new(RECEIPTS).join("enabledboot.json").exists()
    {
        return Err("Failed enabledboot boundary required".into());
    }
    let runtime = Path::new(STAGE).join("swarm-runtime.exe");
    if format!("{:x}", Sha256::digest(fs::read(&runtime)?))
        != "c7b93c9f57314a233e59ff523b03aa0626034bf30f680ac0d16e3bb234325a57"
    {
        return Err("Candidate pin mismatch".into());
    }
    let before = metadata()?;
    match exe.file_stem().and_then(|s| s.to_str()) {
        Some("fixed-startup-inspect") => {
            let value = json!({"product_read_only":true,"before":before,"windows":registry()?,"invocation_performed":false});
            save("startup-inspection.json", &value)?;
            Ok(value)
        }
        Some("fixed-startup-invoke") => {
            if !Path::new(RECEIPTS)
                .join("startup-inspection.json")
                .is_file()
                || before["runtime"]["state"] == "running"
                || before["startup"]["enabled"] != true
                || before["startup"]["registered"] != true
            {
                return Err(
                    "Inspect first; stopped/not-running plus exact enabled registration required"
                        .into(),
                );
            }
            save(
                "startup-direct-invocation-attempt.json",
                &json!({"before":before,"human_start":false,"actual_windows_startup_flag":true}),
            )?;
            let mut child = Command::new(&runtime)
                .args(["--data-dir", DATA, "--windows-startup"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .creation_flags(0x08000000)
                .spawn()?;
            let pid = child.id();
            let deadline = Instant::now() + Duration::from_secs(15);
            while child.try_wait()?.is_none() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(50));
            }
            let value = if child.try_wait()?.is_some() {
                let output = child.wait_with_output()?;
                let text = String::from_utf8_lossy(&output.stderr);
                let code = [
                    "RUNTIME_STARTUP_STATE",
                    "RUNTIME_STARTUP_DISABLED",
                    "RUNTIME_STOPPED",
                    "RUNTIME_STATE",
                    "CREDENTIAL_STORAGE",
                    "CONNECTION_ERROR",
                ]
                .into_iter()
                .find(|code| text.contains(code));
                json!({"manual_diagnostic_only":true,"exit_code":output.status.code(),"stderr_bytes":output.stderr.len(),"safe_error_code":code,"after":metadata()?,"automatic_startup_accepted":false})
            } else {
                json!({"manual_diagnostic_only":true,"child_alive":true,"pid":pid,"after":metadata()?,"automatic_startup_accepted":false})
            };
            save("startup-direct-invocation-result.json", &value)?;
            Ok(value)
        }
        _ => Err("Fixed mode required".into()),
    }
}
fn main() {
    let message = match run() {
        Ok(v) => format!("STARTUP_DIAGNOSTIC; NOT ACCEPTANCE\n{v:#}"),
        Err(e) => format!("DIAGNOSTIC_FAILED\n{e}\nPreserve receipts"),
    };
    let text: Vec<_> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<_> = "Swarm startup diagnostic"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe { MessageBoxW(0, text.as_ptr(), title.as_ptr(), 0) };
}
