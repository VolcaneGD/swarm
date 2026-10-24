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
const STAGE: &str = "C:/Swarm-Validation/Inputs/startup-fixed-20261024";
const RECEIPTS: &str = "C:/Swarm-Validation/Runs/startup-20261021/fixed-20261024";
const OBSERVER: &str = "fixed-observer-corrected-signout.exe";
const OBSERVER_HASH: &str = "720f5ec54f9b2a3233f614fed4024a8a87f5ec46182b0123d57edbb863ad9275";
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
        if program.file_name().and_then(|s| s.to_str()) == Some(OBSERVER)
            && let Ok(v) = serde_json::from_slice::<Value>(&output.stderr)
            && let Some(code) = v["guard_code"]
                .as_str()
                .filter(|c| c.len() < 80 && c.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
        {
            return Err(format!("Observer guard: {code}; IO kind: {}", v["io_kind"]).into());
        }
        return Err("Fixed child returned nonzero; preserve receipts, no retry".into());
    }
    if output.stdout.len() > 65536 || !output.stderr.is_empty() {
        return Err("Unexpected child output".into());
    }
    Ok(output.stdout)
}
fn inventory() -> Result<Value> {
    let script = r#"$ErrorActionPreference='Stop';$s='C:\Swarm-Validation\Inputs\startup-fixed-20261024';$old='C:\Swarm-Validation\Inputs\startup-20261021';$p=@(Get-CimInstance Win32_Process|Where-Object{$_.ExecutablePath -in @("$s\swarm-desktop.exe","$s\swarm-runtime.exe","$old\swarm-desktop.exe","$old\swarm-runtime.exe")});@{runtime=@($p|Where-Object Name -eq 'swarm-runtime.exe').Count;gui=@($p|Where-Object Name -eq 'swarm-desktop.exe').Count;legacy=@($p|Where-Object{$_.ExecutablePath -like "$old\*"}).Count;processes=@($p|Select-Object Name,ProcessId,SessionId)}|ConvertTo-Json -Compress -Depth 4"#;
    Ok(serde_json::from_slice(&bounded(
        Path::new("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"),
        &["-NoProfile", "-NonInteractive", "-Command", script],
    )?)?)
}
fn guard() -> Result<Value> {
    if format!(
        "{:x}",
        Sha256::digest(fs::read(
            "C:/Swarm-Validation/Inputs/startup-20261021/Guest-Guard.ps1"
        )?)
    ) != "b1f57181c34dc64da2dfff8f380537d33856a379d3e6fe0f1b792e9cc01df69f"
    {
        return Err("Original guard pin mismatch".into());
    }
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
        .strip_prefix("fixed-check-v4-")
        .ok_or("Fixed phase name required")?;
    if !matches!(
        phase,
        "baseline"
            | "corrected-baseline"
            | "b0-running"
            | "b0-stopped"
            | "sameboot"
            | "enabledboot"
            | "disabled-set"
            | "disabledboot"
            | "cleanup"
            | "closeout"
            | "inspect"
    ) {
        return Err("Unknown phase".into());
    }
    let observer = Path::new(STAGE).join(OBSERVER);
    if format!("{:x}", Sha256::digest(fs::read(&observer)?)) != OBSERVER_HASH {
        return Err("Observer pin mismatch".into());
    }
    if phase == "baseline" {
        fs::create_dir_all(RECEIPTS)?;
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
        for prior in [
            "manual-enabledboot-result.json",
            "manual-disabled-set-result.json",
            "manual-disabledboot-result.json",
        ] {
            if !Path::new(RECEIPTS).join(prior).is_file() {
                return Err("Complete prior phases before cleanup".into());
            }
        }
    }
    let old_data = guard()?;
    if phase == "baseline" {
        if before["runtime"] != 0 {
            return Err("No runtime allowed before migration".into());
        }
        if format!(
            "{:x}",
            Sha256::digest(fs::read(
                "C:/Swarm-Validation/Inputs/startup-fixed-20261024/Repair-Retained-Launch.ps1"
            )?)
        ) != "628cc9ccf48a1b3e23f09f89a27ce537bd83e039dd92c329eae34d51fbc2a3fc"
        {
            return Err("Repair script pin mismatch".into());
        }
        exclusive("lease-repair-attempt.json", b"{}")?;
        let repair = bounded(
            Path::new("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"),
            &[
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
                "C:/Swarm-Validation/Inputs/startup-fixed-20261024/Repair-Retained-Launch.ps1",
            ],
        )?;
        let value: Value = serde_json::from_slice(&repair)?;
        exclusive(
            "lease-repair-result.json",
            serde_json::to_string(&value)?.as_bytes(),
        )?;
    }
    let attempt = if phase == "sameboot" {
        "manual-sameboot-corrected-signout-attempt.json".to_owned()
    } else {
        format!("manual-{phase}-attempt.json")
    };
    exclusive(&attempt,serde_json::to_string(&json!({"phase":phase,"before":before,"old_data_before":old_data,"human_desktop_launch":true,"password_requested":false}))?.as_bytes())?;
    let args = if phase == "corrected-baseline" {
        vec!["--continue", "--correct-signout-baseline"]
    } else if phase == "sameboot" {
        vec!["--continue", "--recover-corrected-signout"]
    } else if phase == "baseline" {
        vec!["--continue", "--baseline"]
    } else if matches!(phase, "cleanup" | "closeout") {
        vec!["--continue", "--cleanup"]
    } else {
        vec!["--continue", "--observe", phase]
    };
    let value: Value = serde_json::from_slice(&bounded(&observer, &args)?)?;
    let after = inventory()?;
    let expected = if matches!(phase, "b0-running" | "enabledboot" | "disabled-set") {
        1
    } else {
        0
    };
    if after["gui"] != 0 || after["runtime"] != expected || after["legacy"] != 0 {
        return Err("Actual process state mismatch".into());
    }
    if !matches!(phase, "cleanup" | "closeout") && value["product_checks_passed"] != true {
        return Err("Product checks incomplete".into());
    }
    let old_after = guard()?;
    let os: Value = serde_json::from_slice(&bounded(
        Path::new("C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe"),
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            r#"@{last_boot_utc=(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o');elevated=([Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)}|ConvertTo-Json -Compress"#,
        ],
    )?)?;
    let prior_name = match phase {
        "sameboot" => "corrected-baseline",
        "enabledboot" => "sameboot",
        "disabledboot" => "disabled-set",
        _ => "",
    };
    if !prior_name.is_empty() {
        let prior: Value = serde_json::from_slice(&fs::read(
            Path::new(RECEIPTS).join(format!("manual-{prior_name}-result.json")),
        )?)?;
        if (os["last_boot_utc"] == prior["os"]["last_boot_utc"]) != (phase == "sameboot") {
            return Err("Independent OS boot-time boundary mismatch".into());
        }
    }
    let result = json!({"phase":phase,"os":os,"product":value,"processes":after,"old_data_after":old_after,"formal_ac_accepted":false});
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
