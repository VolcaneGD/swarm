#![windows_subsystem = "windows"]
//! Human-launched continuation for the causally repaired Run command only.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    fs::OpenOptions,
    io::Write,
    os::windows::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
mod run_path_pins;
#[path = "session.rs"]
mod session;
const STAGE: &str = "C:/Swarm-Validation/Inputs/startup-run-path-20261024";
const DATA: &str = "C:/Swarm-Validation/Runs/startup-20261021/Data";
const RECEIPTS: &str = "C:/Swarm-Validation/Runs/startup-20261021/run-path-fixed-20261024";
const PS: &str = "C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(window: isize, text: *const u16, title: *const u16, kind: u32) -> i32;
}
fn require(ok: bool, message: &str) -> Result<()> {
    if !ok {
        return Err(message.into());
    }
    Ok(())
}
fn save(name: &str, value: &Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(RECEIPTS).join(name))?;
    f.write_all(serde_json::to_string_pretty(value)?.as_bytes())?;
    Ok(())
}
fn prior(name: &str) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(
        Path::new(RECEIPTS).join(format!("{name}.json")),
    )?)?)
}
fn child(program: &Path, args: &[&str]) -> Result<std::process::Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x08000000)
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    while child.try_wait()?.is_none() {
        require(
            Instant::now() < deadline,
            "Child deadline; preserve owned process and receipts",
        )?;
        std::thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output()?;
    require(
        output.stdout.len() <= 65536 && output.stderr.len() <= 65536,
        "Unexpected child output size",
    )?;
    Ok(output)
}
fn ps(script: &str) -> Result<Value> {
    let output = child(
        Path::new(PS),
        &[
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ],
    )?;
    if !output.status.success() || !output.stderr.is_empty() {
        let policy = String::from_utf8_lossy(&output.stderr).contains("PSSecurityException");
        return Err(format!("Read-only Windows observation failed; guard={}; script_policy_error={policy}; exit={:?}",script.contains("Guest-Guard.ps1"),output.status.code()).into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
fn guard() -> Result<Value> {
    let path = "C:/Swarm-Validation/Inputs/startup-20261021/Guest-Guard.ps1";
    require(
        format!("{:x}", Sha256::digest(fs::read(path)?))
            == "b1f57181c34dc64da2dfff8f380537d33856a379d3e6fe0f1b792e9cc01df69f",
        "Original guard pin mismatch",
    )?;
    let value = ps(
        r#"$ErrorActionPreference='Stop'; & 'C:\Swarm-Validation\Inputs\startup-20261021\Guest-Guard.ps1' | ConvertTo-Json -Compress"#,
    )?;
    require(
        value["old_data_files"] == 22
            && value["old_data_hashes_match"] == true
            && value["path_unchanged"] == true
            && value["old_installed_absent"] == true,
        "Original Data/PATH guard failed",
    )?;
    Ok(value)
}
fn inventory() -> Result<Value> {
    ps(
        r#"$ErrorActionPreference='Stop';$s='C:\Swarm-Validation\Inputs\startup-run-path-20261024';$old=@('C:\Swarm-Validation\Inputs\startup-fixed-20261024','C:\Swarm-Validation\Inputs\startup-20261021');$p=@(Get-CimInstance Win32_Process|Where-Object{$_.Name -in @('swarm-runtime.exe','swarm-desktop.exe')});$unknown=@($p|Where-Object{!$_.ExecutablePath -or ((Split-Path $_.ExecutablePath -Parent) -ne $s -and (Split-Path $_.ExecutablePath -Parent) -notin $old)});if($unknown.Count){throw 'Unknown Swarm writer; preserve it'};$current=@($p|Where-Object{(Split-Path $_.ExecutablePath -Parent) -eq $s});@{runtime=@($current|Where-Object Name -eq 'swarm-runtime.exe').Count;gui=@($current|Where-Object Name -eq 'swarm-desktop.exe').Count;legacy=@($p|Where-Object{(Split-Path $_.ExecutablePath -Parent) -in $old}).Count;processes=@($p|Select-Object Name,ProcessId,SessionId,ExecutablePath);last_boot_utc=(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().ToString('o');elevated=([Security.Principal.WindowsPrincipal]::new([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)}|ConvertTo-Json -Compress -Depth 4"#,
    )
}
fn observe_inventory() -> Result<Value> {
    let result = inventory()?;
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    let path = Path::new("C:/Swarm-Validation/Runs/startup-20261021/fixed-20261024").join(format!(
        "run-path-inventory-{at}-{}.json",
        std::process::id()
    ));
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(serde_json::to_string_pretty(&result)?.as_bytes())?;
    Ok(result)
}
fn observe_state() -> Result<Value> {
    let boot = swarm_platform::lifecycle::boot_id()?;
    let marker = stop()?;
    let (version, record_boot) = match marker.as_deref() {
        None => (0, None),
        Some(bytes) => {
            let text = std::str::from_utf8(bytes)?;
            let (version, id) = text.split_once(':').ok_or("Invalid stop format")?;
            require(
                matches!(version, "1" | "2") && uuid::Uuid::parse_str(id).is_ok(),
                "Invalid stop format",
            )?;
            (version.parse::<u8>()?, Some(id.to_owned()))
        }
    };
    let runtime = swarm_runtime::lifecycle::status(Path::new(DATA))
        .unwrap_or_else(|e| json!({"error":e.code}));
    let startup = swarm_runtime::startup::status(
        Path::new(DATA),
        &Path::new(STAGE).join("swarm-runtime.exe"),
    )
    .unwrap_or_else(|e| json!({"error":e.code}));
    let mut history = json!({});
    for name in [
        "prepare-attempt.json",
        "prepare.json",
        "prepare-registration-recovery-attempt.json",
        "prepare-registration-recovery.json",
    ] {
        let path = Path::new(RECEIPTS).join(name);
        history[name] = if path.is_file() {
            let value: Value = serde_json::from_slice(&fs::read(path)?)?;
            json!({"exists":true,"boot":value["boot_id"],"os_boot":value["processes"]["last_boot_utc"]})
        } else {
            json!({"exists":false})
        };
    }
    let result = json!({"read_only":true,"current_boot":boot,"stop_version":version,"record_boot":record_boot,"stop_matches_current":version==2 && record_boot.as_deref()==Some(boot.as_str()),"runtime":runtime,"startup":startup,"windows":inventory()?,"history":history});
    let at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    let mut file = OpenOptions::new().write(true).create_new(true).open(
        Path::new("C:/Swarm-Validation/Runs/startup-20261021/fixed-20261024")
            .join(format!("run-path-state-{at}-{}.json", std::process::id())),
    )?;
    file.write_all(serde_json::to_string_pretty(&result)?.as_bytes())?;
    Ok(result)
}
fn inspect_notification_registration() -> Result<Value> {
    let root = Path::new(DATA);
    let mut candidates = json!({});
    for (label, path, hash) in [
        (
            "current",
            Path::new(STAGE).join("swarm-desktop.exe"),
            run_path_pins::PINS[0].1,
        ),
        (
            "fixed",
            std::path::PathBuf::from(
                "C:/Swarm-Validation/Inputs/startup-fixed-20261024/swarm-desktop.exe",
            ),
            "b19d914abf2e4b40652a7cf83064e3a651c903ec9012734b9f920856296ff273",
        ),
    ] {
        require(
            format!("{:x}", Sha256::digest(fs::read(&path)?)) == hash,
            "Known notification executable pin mismatch",
        )?;
        let identity = swarm_platform::notifications::Identity::new(root, &path)?;
        candidates[label] =
            match swarm_platform::notifications::Registration::new(identity)?.is_registered() {
                Ok(v) => json!({"registered":v}),
                Err(e) => json!({"error":format!("{e:?}")}),
            };
    }
    let identity = swarm_platform::notifications::Identity::new(
        root,
        &Path::new(STAGE).join("swarm-desktop.exe"),
    )?;
    let scheme = identity.scheme();
    let app = identity.app_id();
    let suffix = scheme
        .strip_prefix("swarm-")
        .ok_or("Invalid owned scheme")?;
    require(
        suffix.len() == 64 && suffix.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid owned suffix",
    )?;
    let script = format!(
        r#"$ErrorActionPreference='Stop';$p=Get-Item 'HKCU:\Software\Classes\{scheme}' -ErrorAction SilentlyContinue;$a=Get-Item 'HKCU:\Software\Classes\AppUserModelId\{app}' -ErrorAction SilentlyContinue;$s=Join-Path ([Environment]::GetFolderPath('Programs')) 'Swarm-{suffix}.lnk';$target=$null;$arguments=$null;if(Test-Path -LiteralPath $s){{$link=(New-Object -ComObject WScript.Shell).CreateShortcut($s);$target=$link.TargetPath;$arguments=$link.Arguments}};@{{protocol_owner=if($p){{$p.GetValue('SwarmOwner')}}else{{$null}};app_owner=if($a){{$a.GetValue('SwarmOwner')}}else{{$null}};shortcut_target=$target;shortcut_args=$arguments}}|ConvertTo-Json -Compress"#
    );
    let result = json!({"read_only":true,"candidates":candidates,"owned":ps(&script)?});
    let mut file=OpenOptions::new().write(true).create_new(true).open("C:/Swarm-Validation/Runs/startup-20261021/run-path-fixed-20261024/notification-inspection.json")?;
    file.write_all(serde_json::to_string_pretty(&result)?.as_bytes())?;
    Ok(result)
}
fn stop() -> Result<Option<Vec<u8>>> {
    Ok(
        swarm_platform::CredentialStore::open(&Path::new(DATA).join("lifecycle"))?
            .get("full-stop")?,
    )
}
fn inspect_registration() -> Result<Value> {
    use std::os::windows::ffi::OsStrExt;
    let root = Path::new(DATA);
    let runtime = Path::new(STAGE).join("swarm-runtime.exe");
    let registration = match swarm_platform::startup::Registration::new(&runtime, root) {
        Ok(r) => match r.is_registered() {
            Ok(v) => json!({"constructed":true,"registered":v}),
            Err(e) => json!({"constructed":true,"read_error":format!("{e:?}")}),
        },
        Err(e) => json!({"constructed":false,"constructor_error":format!("{e:?}")}),
    };
    let canonical = root.canonicalize()?;
    let digest = Sha256::digest(
        canonical
            .as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    );
    let name = format!("Swarm-{digest:x}");
    let script = format!(
        r#"$ErrorActionPreference='Stop';$k=Get-Item 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -ErrorAction SilentlyContinue;@{{value=if($k){{$k.GetValue('{name}')}}else{{$null}};value_name='{name}'}}|ConvertTo-Json -Compress"#
    );
    let mut paths = Vec::new();
    for path in [&runtime, root] {
        let canonical = path.canonicalize()?;
        let text = canonical.to_str().ok_or("Non-Unicode diagnostic path")?;
        let ordinary = if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{unc}")
        } else {
            text.strip_prefix(r"\\?\").unwrap_or(text).to_owned()
        };
        let roundtrip = Path::new(&ordinary).canonicalize();
        paths.push(json!({"same_canonical":roundtrip.as_ref().is_ok_and(|p|p==&canonical),"canonical_units":text.encode_utf16().count(),"exists":path.exists()}));
    }
    let status = match swarm_runtime::startup::status(root, &runtime) {
        Ok(v) => v,
        Err(e) => json!({"error_code":e.code}),
    };
    let result = json!({"read_only":true,"registration":registration,"startup":status,"paths":paths,"owned_run":ps(&script)?,"processes":inventory()?});
    let bytes = serde_json::to_vec_pretty(&result)?;
    let mut file = OpenOptions::new().write(true).create_new(true).open(
        "C:/Swarm-Validation/Runs/startup-20261021/fixed-20261024/run-path-setting-inspection.json",
    )?;
    file.write_all(&bytes)?;
    Ok(result)
}
fn retire_known_old_registration() -> Result<Value> {
    let observed: Value = serde_json::from_slice(&fs::read(
        "C:/Swarm-Validation/Runs/startup-20261021/fixed-20261024/run-path-setting-inspection.json",
    )?)?;
    require(
        observed["registration"]["constructed"] == true
            && observed["registration"]["read_error"] == "PlatformError(InvalidData)",
        "Observed registration conflict required",
    )?;
    require(
        inventory()?["legacy"] == 0,
        "Legacy writers must remain closed",
    )?;
    let old_runtime =
        Path::new("C:/Swarm-Validation/Inputs/startup-fixed-20261024/swarm-runtime.exe");
    require(
        format!("{:x}", Sha256::digest(fs::read(old_runtime)?))
            == "c7b93c9f57314a233e59ff523b03aa0626034bf30f680ac0d16e3bb234325a57",
        "Known old runtime pin mismatch",
    )?;
    let path = Path::new(
        "C:/Swarm-Validation/Runs/startup-20261021/fixed-20261024/run-path-retire-old-attempt.json",
    );
    let mut attempt = OpenOptions::new().write(true).create_new(true).open(path)?;
    attempt.write_all(br#"{"known_old_candidate_only":true,"data_changed":false}"#)?;
    // The public adapter refuses any value other than this exact executable/root
    // command (current or known legacy spelling); no broad registry deletion.
    let registration = swarm_platform::startup::Registration::new(old_runtime, Path::new(DATA))?;
    registration.set_enabled(false)?;
    require(
        !swarm_platform::startup::Registration::new(
            &Path::new(STAGE).join("swarm-runtime.exe"),
            Path::new(DATA),
        )?
        .is_registered()?,
        "Root registration not retired",
    )?;
    let result = json!({"known_old_registration_retired":true,"data_changed":false,"other_roots_untouched":true,"new_registered":false,"diagnostic_only":true});
    let mut file = OpenOptions::new().write(true).create_new(true).open(
        "C:/Swarm-Validation/Runs/startup-20261021/fixed-20261024/run-path-retire-old-result.json",
    )?;
    file.write_all(serde_json::to_string_pretty(&result)?.as_bytes())?;
    Ok(result)
}
fn cli_stopped() -> Result<Value> {
    let bootstrap: Value = serde_json::from_slice(&fs::read(
        "C:/Swarm-Validation/Runs/startup-20261021/bootstrap.json",
    )?)?;
    let project = bootstrap["project_id"]
        .as_str()
        .ok_or("Missing original Project locator")?;
    let output = child(
        &Path::new(STAGE).join("swarm.exe"),
        &[
            "--data-dir",
            DATA,
            "--project",
            project,
            "--session",
            "startup-os-refusal",
            "--format",
            "json",
            "identity",
            "start",
        ],
    )?;
    require(output.status.code() == Some(7), "Stopped CLI exit mismatch")?;
    let fault: Value = serde_json::from_slice(&output.stdout)
        .or_else(|_| serde_json::from_slice(&output.stderr))?;
    require(
        fault["error"]["code"] == "RUNTIME_STOPPED",
        "Stopped CLI fault mismatch",
    )?;
    Ok(json!({"exit":7,"code":"RUNTIME_STOPPED","identity_created":false}))
}
fn run() -> Result<Value> {
    require(
        std::env::var("COMPUTERNAME")?.eq_ignore_ascii_case("DESKTOP-1TAP8BI")
            && std::env::var("USERNAME")?.eq_ignore_ascii_case("GPT"),
        "Wrong guest identity",
    )?;
    let exe = std::env::current_exe()?;
    require(
        exe.parent().ok_or("Missing stage")?.canonicalize()? == Path::new(STAGE).canonicalize()?,
        "Wrong stage",
    )?;
    let phase = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| {
            s.strip_prefix("runfix-v2-")
                .or_else(|| s.strip_prefix("runfix-"))
        })
        .ok_or("Fixed phase required")?;
    let current_register = phase == "current-register";
    let preregister = phase == "preregister" || current_register;
    let current_prepare = phase == "current-prepare" || current_register;
    let finish_cleanup = phase == "finish-cleanup";
    let phase = if preregister || current_prepare {
        "prepare"
    } else if finish_cleanup {
        "cleanup"
    } else {
        phase
    };
    require(
        matches!(
            phase,
            "prepare"
                | "enabledboot"
                | "disabled-set"
                | "disabledboot"
                | "cleanup"
                | "closeout"
                | "inspect"
                | "retire-old"
                | "inventory"
                | "state"
                | "notification-inspect"
        ),
        "Unknown phase",
    )?;
    for (name, hash) in run_path_pins::PINS {
        require(
            format!(
                "{:x}",
                Sha256::digest(fs::read(Path::new(STAGE).join(name))?)
            ) == hash,
            "Candidate pin mismatch",
        )?;
    }
    if phase == "inspect" {
        return inspect_registration();
    }
    if phase == "retire-old" {
        return retire_known_old_registration();
    }
    if phase == "inventory" {
        return observe_inventory();
    }
    if phase == "state" {
        return observe_state();
    }
    if phase == "notification-inspect" {
        return inspect_notification_registration();
    }
    if finish_cleanup {
        require(
            Path::new(RECEIPTS).join("cleanup-attempt.json").is_file()
                && !Path::new(RECEIPTS).join("cleanup.json").exists(),
            "Retained failed cleanup required",
        )?;
        let observation: Value = serde_json::from_slice(&fs::read(
            Path::new(RECEIPTS).join("notification-inspection.json"),
        )?)?;
        require(
            observation["candidates"]["fixed"]["registered"] == true
                && observation["candidates"]["current"]["error"] == "Conflict",
            "Observed known-fixed notification ownership required",
        )?;
    }
    let recovering_empty_prepare = phase == "prepare" && Path::new(RECEIPTS).exists();
    require(
        !(preregister || current_prepare) || recovering_empty_prepare,
        "Retained failed prepare required for registration recovery",
    )?;
    if phase == "prepare" {
        if recovering_empty_prepare {
            if preregister || current_prepare {
                require(
                    Path::new(RECEIPTS).join("prepare-attempt.json").is_file()
                        && !Path::new(RECEIPTS).join("prepare.json").exists(),
                    "Known failed prepare without success required",
                )?;
            } else {
                require(
                    exe.file_stem().and_then(|s| s.to_str()) == Some("runfix-v2-prepare"),
                    "Original prepare cannot repeat",
                )?;
                require(
                    fs::read_dir(RECEIPTS)?.next().is_none(),
                    "Preserve existing nonempty continuation; no phase replay",
                )?;
            }
        }
        let old: Value = serde_json::from_slice(&fs::read(
            "C:/Swarm-Validation/Runs/startup-20261021/fixed-20261024/manual-closeout-result.json",
        )?)?;
        require(
            old["processes"]["runtime"] == 0 && old["product"]["startup"]["registered"] == false,
            "Prior normal closeout required",
        )?;
        let _private = swarm_platform::CredentialStore::open(Path::new(RECEIPTS))?;
    }
    let before = guard()?;
    let processes = inventory()?;
    if current_prepare {
        let old: Value =
            serde_json::from_slice(&fs::read(Path::new(RECEIPTS).join("prepare-attempt.json"))?)?;
        require(
            old["processes"]["last_boot_utc"].is_string()
                && old["processes"]["last_boot_utc"] != processes["last_boot_utc"],
            "Actual intervening boot required for new preparation",
        )?;
        if current_register {
            let latest: Value = serde_json::from_slice(&fs::read(
                Path::new(RECEIPTS).join("prepare-current-boot-attempt.json"),
            )?)?;
            require(
                latest["processes"]["last_boot_utc"] == processes["last_boot_utc"],
                "Current failed prepare OS boot must remain unchanged",
            )?;
        }
    }
    if processes["legacy"] != 0 || processes["gui"] != 0 || processes["elevated"] != false {
        return Err(format!(
            "Observer precondition: gui={}, legacy={}, elevated={}",
            processes["gui"], processes["legacy"], processes["elevated"]
        )
        .into());
    }
    let sessions = session::query()?;
    let active = session::select(&sessions).ok_or("Active guest session required")?;
    require(
        active.user.eq_ignore_ascii_case("GPT"),
        "Wrong active session",
    )?;
    if recovering_empty_prepare && !preregister && !current_prepare {
        save(
            "prepare-wrapper-recovery.json",
            &json!({"prior_empty_root_preserved":true,"original_guard_process_condition_restored":true,"product_operations_replayed":false}),
        )?;
    }
    save(
        &if finish_cleanup {
            "cleanup-known-fixed-recovery-attempt.json".to_owned()
        } else if current_register {
            "prepare-current-register-attempt.json".to_owned()
        } else if current_prepare {
            "prepare-current-boot-attempt.json".to_owned()
        } else if preregister {
            "prepare-registration-recovery-attempt.json".to_owned()
        } else {
            format!("{phase}-attempt.json")
        },
        &json!({"before":before,"processes":processes,"automatic_retries":0}),
    )?;
    let root = Path::new(DATA);
    let runtime = Path::new(STAGE).join("swarm-runtime.exe");
    let mut fixture_startup_setup = Value::Null;
    if preregister {
        let boot = swarm_platform::lifecycle::boot_id()?;
        let marker = stop()?;
        require(
            processes["runtime"] == 0
                && swarm_runtime::lifecycle::status(root)?["state"] == "stopped",
            "Stopped runtime required before fixture registration",
        )?;
        require(
            marker.as_deref() == Some(format!("2:{boot}").as_bytes()),
            "Preserve actual current-boot v2 stop before registration",
        )?;
        let before = swarm_runtime::startup::status(root, &runtime)?;
        swarm_runtime::startup::configure(root, &runtime, true)?;
        require(stop()? == marker, "Startup configuration changed full stop")?;
        fixture_startup_setup = json!({"before":before,"shared_host_api_enabled":true,"full_stop_preserved":true,"native_checkbox_accepted":false});
        save(
            if current_register {
                "prepare-current-register-setup.json"
            } else {
                "prepare-registration-recovery.json"
            },
            &fixture_startup_setup,
        )?;
    }
    if matches!(phase, "cleanup" | "closeout") {
        if phase == "cleanup" {
            prior("disabledboot")?;
        }
        swarm_runtime::startup::configure(root, &runtime, false)?;
        if swarm_runtime::lifecycle::status(root)?["state"] == "running" {
            swarm_gui_host::GuiHost::open(root)?.stop_runtime()?;
        }
        if finish_cleanup {
            let old =
                Path::new("C:/Swarm-Validation/Inputs/startup-fixed-20261024/swarm-desktop.exe");
            require(
                format!("{:x}", Sha256::digest(fs::read(old)?))
                    == "b19d914abf2e4b40652a7cf83064e3a651c903ec9012734b9f920856296ff273",
                "Known fixed GUI pin mismatch",
            )?;
            let registration = swarm_platform::notifications::Registration::new(
                swarm_platform::notifications::Identity::new(root, old)?,
            )?;
            require(
                registration.is_registered()?,
                "Fresh exact old notification ownership required",
            )?;
            registration.remove()?;
            require(
                !registration.is_registered()?,
                "Old notification registration remains",
            )?;
            save(
                "cleanup-retired-fixed-notification.json",
                &json!({"known_pinned_fixed_removed":true,"foreign_values_removed":false}),
            )?;
        }
        let identity = swarm_platform::notifications::Identity::new(
            root,
            &Path::new(STAGE).join("swarm-desktop.exe"),
        )?;
        swarm_platform::notifications::Registration::new(identity)?.remove()?;
    }
    let boot = swarm_platform::lifecycle::boot_id()?;
    let startup = swarm_runtime::startup::status(root, &runtime)?;
    let state = swarm_runtime::lifecycle::status(root)?;
    let processes = inventory()?;
    let running = matches!(phase, "enabledboot" | "disabled-set");
    let enabled = matches!(phase, "prepare" | "enabledboot");
    require(
        processes["runtime"] == if running { 1 } else { 0 }
            && processes["gui"] == 0
            && processes["legacy"] == 0,
        "Actual process state mismatch",
    )?;
    require(
        state["state"] == if running { "running" } else { "stopped" },
        "Runtime state mismatch",
    )?;
    if startup["enabled"] != enabled
        || startup["registered"] != enabled
        || startup["configured"] != true
    {
        return Err(format!("Startup state mismatch: {startup}").into());
    }
    let marker = stop()?;
    let mut cli = Value::Null;
    match phase {
        "prepare" => {
            require(
                marker.as_deref() == Some(format!("2:{boot}").as_bytes()),
                "Current boot v2 full stop required",
            )?;
            cli = cli_stopped()?;
        }
        "enabledboot" => {
            let old = prior("prepare")?;
            require(
                old["boot_id"] != boot
                    && old["processes"]["last_boot_utc"] != processes["last_boot_utc"],
                "Real changed boot required",
            )?;
            require(marker.is_none(), "Enabled new boot did not clear stop")?;
        }
        "disabled-set" => {
            let old = prior("enabledboot")?;
            require(
                old["boot_id"] == boot
                    && old["processes"]["last_boot_utc"] == processes["last_boot_utc"],
                "Disable must follow accepted enabled boot",
            )?;
        }
        "disabledboot" => {
            let old = prior("disabled-set")?;
            require(
                old["boot_id"] != boot
                    && old["processes"]["last_boot_utc"] != processes["last_boot_utc"],
                "Real changed boot required",
            )?;
            let previous = old["boot_id"].as_str().ok_or("Missing prior boot")?;
            require(
                marker.as_deref() == Some(format!("2:{previous}").as_bytes()),
                "Disabled reboot must retain prior full stop",
            )?;
            cli = cli_stopped()?;
        }
        _ => {}
    }
    require(stop()? == marker, "Observer/CLI changed stop marker")?;
    let after = guard()?;
    let result = json!({"phase":phase,"boot_id":boot,"startup":startup,"runtime":state,"processes":processes,"session_id":active.id,"cli":cli,"original_guard":after,"fixture_startup_setup":fixture_startup_setup,"known_fixed_notification_retired":finish_cleanup,"formal_ac_accepted":false,"scoped_phase_pass":!matches!(phase,"closeout")});
    save(&format!("{phase}.json"), &result)?;
    Ok(result)
}
fn main() {
    let message = match run() {
        Ok(v) => format!("RUN_PATH_PHASE_RESULT\n{v:#}"),
        Err(e) => format!("PHASE_NOT_ACCEPTED\n{e}\nPreserve receipts; do not relaunch."),
    };
    let text: Vec<_> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<_> = "Swarm Run path validation"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe { MessageBoxW(0, text.as_ptr(), title.as_ptr(), 0) };
}
