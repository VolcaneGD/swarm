#![windows_subsystem = "windows"]
//! New-implementation elevated creator -> normal-token acquisition, isolated from Data.
use serde_json::{Value, json};
use std::{fs, fs::OpenOptions, io::Write, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const ROOT: &str = "C:/Swarm-Validation/Runs/startup-20261021/token-fixed-20261024";
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcess() -> isize;
    fn CloseHandle(handle: isize) -> i32;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(process: isize, access: u32, token: *mut isize) -> i32;
    fn GetTokenInformation(
        token: isize,
        class: u32,
        info: *mut u32,
        size: u32,
        returned: *mut u32,
    ) -> i32;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(window: isize, text: *const u16, title: *const u16, kind: u32) -> i32;
}
fn elevated() -> Result<bool> {
    let mut token = 0;
    if unsafe { OpenProcessToken(GetCurrentProcess(), 8, &mut token) } == 0 {
        return Err("Token open failed".into());
    }
    let mut value = 0;
    let mut length = 0;
    let ok = unsafe { GetTokenInformation(token, 20, &mut value, 4, &mut length) };
    unsafe { CloseHandle(token) };
    if ok == 0 || length != 4 || value > 1 {
        return Err("Elevation query failed".into());
    }
    Ok(value == 1)
}
fn save(name: &str, value: &Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(ROOT).join(name))?;
    f.write_all(serde_json::to_string_pretty(value)?.as_bytes())?;
    Ok(())
}
fn run() -> Result<Value> {
    if std::env::var("COMPUTERNAME")?.to_uppercase() != "DESKTOP-1TAP8BI"
        || std::env::var("USERNAME")?.to_uppercase() != "GPT"
    {
        return Err("Wrong guest user".into());
    }
    let exe = std::env::current_exe()?;
    if exe.parent().ok_or("Missing stage")?.canonicalize()?
        != Path::new("C:/Swarm-Validation/Inputs/startup-fixed-20261024").canonicalize()?
    {
        return Err("Wrong stage".into());
    }
    let stem = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Missing mode")?;
    let create = match stem {
        "fixed-token-elevated" => true,
        "fixed-token-normal" => false,
        _ => return Err("Fixed mode required".into()),
    };
    let is_elevated = elevated()?;
    if is_elevated != create {
        return Err("Wrong token for this fixed phase".into());
    }
    if create {
        if Path::new(ROOT).exists() {
            return Err("Preserve existing token fixture".into());
        }
        let _root_store = swarm_platform::CredentialStore::open(Path::new(ROOT))?;
    } else {
        let prior: Value =
            serde_json::from_slice(&fs::read(Path::new(ROOT).join("elevated-result.json"))?)?;
        if prior["elevated"] != true || prior["leases"] != 5 {
            return Err("Actual elevated creation proof required".into());
        }
    }
    let mode = if create { "elevated" } else { "normal" };
    save(
        &format!("{mode}-attempt.json"),
        &json!({"elevated":is_elevated}),
    )?;
    let mut leases = Vec::new();
    for (dir, name) in [
        ("gui-state", ".host.lock"),
        ("credentials", ".broker.lock"),
        ("lifecycle", "runtime.lock"),
        ("lifecycle", "startup.lock"),
        ("lifecycle", "launch.lock"),
    ] {
        let path = Path::new(ROOT).join(dir);
        if !create && !path.join(name).is_file() {
            return Err("Creator's exact lease missing".into());
        }
        let store = swarm_platform::CredentialStore::open(&path)?;
        leases.push(store.private_lease(name)?);
    }
    let result = json!({"elevated":is_elevated,"leases":leases.len(),"private_owner_dacl_handle_validation":true,"same_exact_paths":true,"actual_normal_acquisition":!create,"data_touched":false,"formal_ac_accepted":false});
    save(&format!("{mode}-result.json"), &result)?;
    Ok(result)
}
fn main() {
    let result = match run() {
        Ok(v) => format!("TOKEN_PHASE_PASS\n{v:#}"),
        Err(e) => format!("TOKEN_PHASE_NOT_ACCEPTED\n{e}\nPreserve receipts; no repeat."),
    };
    let text: Vec<_> = result.encode_utf16().chain(Some(0)).collect();
    let title: Vec<_> = "Swarm token transition result"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe { MessageBoxW(0, text.as_ptr(), title.as_ptr(), 0) };
}
