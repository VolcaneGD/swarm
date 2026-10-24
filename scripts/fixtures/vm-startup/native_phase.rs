#![windows_subsystem = "windows"]
//! Human-launched fixed phase observer, using the already authenticated desktop.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::os::windows::process::CommandExt;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const STAGE: &str = "C:/Swarm-Validation/Inputs/startup-20261021";
const RECEIPTS: &str = "C:/Swarm-Validation/Runs/startup-20261021/native-20261023";
const OBSERVER: &str = "startup-observer-native-manual-20261024.exe";
const OBSERVER_HASH: &str = "3bcbac7a49409ebb101702b7029385626e08e4ca4de239308bb242b698e4df0e";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(window: isize, text: *const u16, title: *const u16, kind: u32) -> i32;
}
fn show(text: &str) {
    let t: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "Swarm native phase result"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        MessageBoxW(0, t.as_ptr(), title.as_ptr(), 0);
    }
}
fn exclusive(name: &str, bytes: &[u8]) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(RECEIPTS).join(name))?;
    f.write_all(bytes)?;
    Ok(())
}
fn bounded(program: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000)
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            return Err("Child timeout; preserve process and receipts".into());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err("Fixed child returned nonzero; preserve receipts, no retry".into());
    }
    if output.stdout.len() > 65536 || !output.stderr.is_empty() {
        return Err("Unexpected child output".into());
    }
    Ok(output.stdout)
}
fn inventory() -> Result<Value> {
    let script = r#"$ErrorActionPreference='Stop';$s='C:\Swarm-Validation\Inputs\startup-20261021';$p=@(Get-CimInstance Win32_Process|Where-Object{$_.ExecutablePath -in @("$s\swarm-desktop.exe","$s\swarm-runtime.exe")});@{runtime=@($p|Where-Object Name -eq 'swarm-runtime.exe').Count;gui=@($p|Where-Object Name -eq 'swarm-desktop.exe').Count;processes=@($p|Select-Object Name,ProcessId,SessionId)}|ConvertTo-Json -Compress -Depth 4"#;
    Ok(serde_json::from_slice(&bounded(
        Path::new("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"),
        &["-NoProfile", "-NonInteractive", "-Command", script],
    )?)?)
}
fn guard() -> Result<Value> {
    let script = r#"$ErrorActionPreference='Stop'; & 'C:\Swarm-Validation\Inputs\startup-20261021\Guest-Guard.ps1' | ConvertTo-Json -Compress"#;
    let value: Value = serde_json::from_slice(&bounded(
        Path::new("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"),
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
    )?)?;
    if value["old_data_files"] != 22
        || value["old_data_hashes_match"] != true
        || value["path_unchanged"] != true
    {
        return Err("Original Data/PATH guard not established".into());
    }
    Ok(value)
}
fn run() -> Result<String> {
    if std::env::var("COMPUTERNAME")?.to_uppercase() != "DESKTOP-1TAP8BI"
        || std::env::var("USERNAME")?.to_uppercase() != "GPT"
    {
        return Err("Wrong guest user".into());
    }
    let exe = std::env::current_exe()?;
    if exe.parent().ok_or("No parent")?.canonicalize()? != Path::new(STAGE).canonicalize()? {
        return Err("Wrong stage".into());
    }
    let stem = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("No phase name")?;
    let phase = stem
        .strip_prefix("startup-check-")
        .ok_or("Fixed phase name required")?;
    if !matches!(
        phase,
        "enabledboot" | "disabled-set" | "disabledboot" | "cleanup" | "closeout" | "inspect"
    ) {
        return Err("Unknown phase".into());
    }
    let observer = Path::new(STAGE).join(OBSERVER);
    if format!("{:x}", Sha256::digest(fs::read(&observer)?)) != OBSERVER_HASH {
        return Err("Observer pin mismatch".into());
    }
    let before = inventory()?;
    if phase == "inspect" {
        let metadata: Value =
            serde_json::from_slice(&bounded(&observer, &["--continue", "--metadata"])?)?;
        let value = json!({"read_only":true,"metadata":metadata,"processes":before,"guard":guard()?,"enabledboot_attempt_exists":Path::new(RECEIPTS).join("enabledboot-attempt.json").exists(),"enabledboot_success_exists":Path::new(RECEIPTS).join("enabledboot.json").exists()});
        exclusive(
            "manual-inspect-result.json",
            serde_json::to_string(&value)?.as_bytes(),
        )?;
        return Ok(format!(
            "READ_ONLY_DIAGNOSTIC\n{}",
            serde_json::to_string_pretty(&value)?
        ));
    }
    if before["gui"] != 0 {
        return Err("Close Swarm GUI before observation".into());
    }
    if phase == "cleanup" {
        for prior in ["enabledboot.json", "disabled-set.json", "disabledboot.json"] {
            if !Path::new(RECEIPTS).join(prior).is_file() {
                return Err("Complete prior phases before cleanup".into());
            }
        }
    }
    let old_data = guard()?;
    exclusive(&format!("manual-{phase}-attempt.json"),serde_json::to_string(&json!({"phase":phase,"before":before,"old_data_before":old_data,"human_desktop_launch":true,"password_requested":false}))?.as_bytes())?;
    let args = if matches!(phase, "cleanup" | "closeout") {
        vec!["--continue", "--cleanup"]
    } else {
        vec!["--continue", "--observe", phase]
    };
    let value: Value = serde_json::from_slice(&bounded(&observer, &args)?)?;
    let after = inventory()?;
    let expected = if matches!(phase, "enabledboot" | "disabled-set") {
        1
    } else {
        0
    };
    if after["gui"] != 0 || after["runtime"] != expected {
        return Err("Actual process state mismatch".into());
    }
    if !matches!(phase, "cleanup" | "closeout") && value["product_checks_passed"] != true {
        return Err("Product checks incomplete".into());
    }
    let old_after = guard()?;
    let result = json!({"phase":phase,"product":value,"processes":after,"old_data_after":old_after,"formal_ac_accepted":false});
    exclusive(
        &format!("manual-{phase}-result.json"),
        serde_json::to_string(&result)?.as_bytes(),
    )?;
    Ok(format!(
        "{}\n{}",
        if phase == "closeout" {
            "NORMAL_CLOSEOUT_ONLY; INCOMPLETE_SEQUENCE"
        } else {
            "SCOPED_PHASE_PASS"
        },
        serde_json::to_string_pretty(&result)?
    ))
}
fn main() {
    match run() {
        Ok(v) => show(&v),
        Err(e) => show(&format!(
            "PHASE_NOT_ACCEPTED\n{e}\nPreserve receipts; do not relaunch."
        )),
    }
}
