#![windows_subsystem = "windows"]
//! One scoped Run-dispatch diagnostic; never launches product or alters existing values.
use serde_json::{Value, json};
use std::{fs, fs::OpenOptions, io::Write, os::windows::ffi::OsStrExt, path::Path};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, RegCloseKey, RegDeleteValueW,
    RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
};
const ROOT: &str = "C:/Swarm-Validation/Runs/startup-20261021/autorun-canary-fixed-20261024";
const DATA: &str = "C:/Swarm-Validation/Runs/startup-20261021/Data";
const STAGE: &str = "C:\\Swarm-Validation\\Inputs\\startup-fixed-20261024";
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(window: isize, text: *const u16, title: *const u16, kind: u32) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentProcessId() -> u32;
    fn ProcessIdToSessionId(pid: u32, session: *mut u32) -> i32;
}
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}
fn key() -> Result<Key> {
    let mut key = std::ptr::null_mut();
    let path = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    if unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            path.as_ptr(),
            0,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            &mut key,
        )
    } != 0
    {
        return Err("Run key open failed".into());
    }
    Ok(Key(key))
}
fn query(key: HKEY, name: &str) -> Result<Option<Vec<u8>>> {
    let name = wide(name);
    let mut kind = 0;
    let mut length = 0;
    let status = unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null(),
            &mut kind,
            std::ptr::null_mut(),
            &mut length,
        )
    };
    if status == 2 {
        return Ok(None);
    }
    if status != 0 || kind != REG_SZ || length > 65536 {
        return Err("Owned canary value invalid".into());
    }
    let mut value = vec![0; length as usize];
    if unsafe {
        RegQueryValueExW(
            key,
            name.as_ptr(),
            std::ptr::null(),
            &mut kind,
            value.as_mut_ptr(),
            &mut length,
        )
    } != 0
    {
        return Err("Owned canary read failed".into());
    }
    Ok(Some(value))
}
fn items() -> Result<Vec<(String, Vec<u8>)>> {
    let target = Path::new(STAGE).join("fixed-autorun-canary.exe");
    let mut items = Vec::new();
    for (mode, path) in [
        ("ordinary", target.clone()),
        ("extended", target.canonicalize()?),
    ] {
        let mut command = vec![b'"' as u16];
        command.extend(path.as_os_str().encode_wide());
        command.push(b'"' as u16);
        command.extend(format!(" --{mode}").encode_utf16());
        command.push(0);
        let data = command.into_iter().flat_map(u16::to_le_bytes).collect();
        items.push((format!("Swarm-Validation-20261024-{mode}"), data));
    }
    Ok(items)
}
fn save(name: &str, value: &Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(Path::new(ROOT).join(name))?;
    f.write_all(serde_json::to_string_pretty(value)?.as_bytes())?;
    Ok(())
}
fn metadata() -> Result<Value> {
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) } == 0 {
        return Err("Current session query failed".into());
    }
    Ok(
        json!({"boot_id":swarm_platform::lifecycle::boot_id()?,"session_id":session,"pid":std::process::id(),"at_unix_millis":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis(),"runtime":swarm_runtime::lifecycle::status(Path::new(DATA)).unwrap_or_else(|e|json!({"error":e.code}))}),
    )
}
fn run() -> Result<Value> {
    if std::env::var("COMPUTERNAME")?.to_uppercase() != "DESKTOP-1TAP8BI"
        || std::env::var("USERNAME")?.to_uppercase() != "GPT"
    {
        return Err("Wrong guest".into());
    }
    let exe = std::env::current_exe()?;
    let mode = exe
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("Missing fixed mode")?;
    if mode == "fixed-autorun-canary" {
        let argument = std::env::args().nth(1).ok_or("Missing fixed canary flag")?;
        let name = match argument.as_str() {
            "--ordinary" => "ordinary",
            "--extended" => "extended",
            _ => return Err("Unknown canary flag".into()),
        };
        save(
            &format!("entry-{name}.json"),
            &json!({"pid":std::process::id(),"at_unix_millis":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis()}),
        )?;
        let value = metadata().inspect_err(|_| {
            let _ = save(
                &format!("failure-{name}.json"),
                &json!({"stage":"metadata","data_written":false}),
            );
        })?;
        let setup: Value =
            serde_json::from_slice(&fs::read(Path::new(ROOT).join("setup-result.json"))?)?;
        if value["boot_id"] == setup["initial"]["boot_id"] {
            return Err("Real new boot required for canary record".into());
        }
        save(&format!("autorun-{name}.json"), &value)?;
        return Ok(value);
    }
    match mode {
        "fixed-autorun-setup" => {
            if Path::new(ROOT).exists() {
                return Err("Preserve prior canary root".into());
            }
            let _root = swarm_platform::CredentialStore::open(Path::new(ROOT))?;
            let key = key()?;
            let items = items()?;
            for (name, _) in &items {
                if query(key.0, name)?.is_some() {
                    return Err("Existing canary name refused".into());
                }
            }
            save("setup-attempt.json", &metadata()?)?;
            for (name, data) in &items {
                if unsafe {
                    RegSetValueExW(
                        key.0,
                        wide(name).as_ptr(),
                        0,
                        REG_SZ,
                        data.as_ptr(),
                        data.len() as u32,
                    )
                } != 0
                {
                    return Err("Canary registration failed; inspect partial state".into());
                }
                if query(key.0, name)?.as_deref() != Some(data.as_slice()) {
                    return Err("Canary registration readback failed".into());
                }
            }
            let result = json!({"registered_canaries":2,"existing_swarm_registration_changed":false,"data_changed":false,"initial":metadata()?});
            save("setup-result.json", &result)?;
            Ok(result)
        }
        "fixed-autorun-inspect" => {
            let mut result = json!({"current":metadata()?,"ordinary":null,"extended":null,"diagnostic_only":true});
            for mode in ["ordinary", "extended"] {
                result[format!("{mode}_entry")] =
                    json!(Path::new(ROOT).join(format!("entry-{mode}.json")).is_file());
                result[format!("{mode}_metadata_failure")] = json!(
                    Path::new(ROOT)
                        .join(format!("failure-{mode}.json"))
                        .is_file()
                );
                let path = Path::new(ROOT).join(format!("autorun-{mode}.json"));
                if path.exists() {
                    result[mode] = serde_json::from_slice(&fs::read(path)?)?;
                }
            }
            save("inspect-result.json", &result)?;
            Ok(result)
        }
        "fixed-autorun-cleanup" => {
            let key = key()?;
            for (name, data) in items()? {
                match query(key.0, &name)? {
                    None => {}
                    Some(v) if v == data => {
                        if unsafe { RegDeleteValueW(key.0, wide(&name).as_ptr()) } != 0 {
                            return Err("Canary removal failed".into());
                        }
                    }
                    Some(_) => return Err("Changed canary registration refused".into()),
                }
                if query(key.0, &name)?.is_some() {
                    return Err("Canary removal readback failed".into());
                }
            }
            let result =
                json!({"owned_canaries_removed":2,"existing_swarm_registration_changed":false});
            save("cleanup-result.json", &result)?;
            Ok(result)
        }
        _ => Err("Fixed canary mode required".into()),
    }
}
fn main() {
    let automatic = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s == "fixed-autorun-canary"))
        .unwrap_or(false);
    let result = run();
    if automatic {
        return;
    }
    let message = match result {
        Ok(v) => format!("AUTORUN_DIAGNOSTIC\n{v:#}"),
        Err(e) => format!("AUTORUN_DIAGNOSTIC_FAILED\n{e}\nPreserve partial state"),
    };
    let text: Vec<_> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<_> = "Swarm autorun canary result"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe { MessageBoxW(0, text.as_ptr(), title.as_ptr(), 0) };
}
