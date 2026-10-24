#![windows_subsystem = "windows"]
//! Read-only diagnosis; never runs a product phase or edits Data/receipts.
use serde_json::{Value, json};
use std::{fs, path::Path};
#[path = "session.rs"]
mod session;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const ROOT: &str = "C:/Swarm-Validation/Runs/startup-20261021/Data";
const RECEIPTS: &str = "C:/Swarm-Validation/Runs/startup-20261021/fixed-20261024";
#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(window: isize, text: *const u16, title: *const u16, kind: u32) -> i32;
}
fn run() -> Result<Value> {
    if std::env::var("COMPUTERNAME")?.to_uppercase() != "DESKTOP-1TAP8BI"
        || std::env::var("USERNAME")?.to_uppercase() != "GPT"
    {
        return Err("Wrong guest".into());
    }
    let mut receipts = json!({});
    for name in [
        "b0-running.json",
        "b0-stopped-attempt.json",
        "b0-stopped.json",
        "manual-b0-stopped-result.json",
        "sameboot-attempt.json",
        "sameboot.json",
        "manual-sameboot-attempt.json",
        "manual-sameboot-result.json",
    ] {
        let path = Path::new(RECEIPTS).join(name);
        if path.is_file() {
            let v: Value = serde_json::from_slice(&fs::read(path)?)?;
            let p = if v.get("product").is_some() {
                &v["product"]
            } else {
                &v
            };
            receipts[name] = json!({"exists":true,"boot_id":p["boot_id"],"session":p["interactive"]["session_id"],"runtime":p["runtime"]["state"],"startup":p["startup"],"version":p["stop_record_version"],"os":v["os"]});
        } else {
            receipts[name] = json!({"exists":false});
        }
    }
    let sessions = session::query()?;
    let interactive = session::select(&sessions).map(|s| json!({"id":s.id,"active":true}));
    let runtime = swarm_runtime::lifecycle::status(Path::new(ROOT))
        .map_or_else(|e| json!({"error":e.code}), |v| v);
    let startup = swarm_runtime::startup::status(
        Path::new(ROOT),
        Path::new("C:/Swarm-Validation/Inputs/startup-fixed-20261024/swarm-runtime.exe"),
    )
    .map_or_else(|e| json!({"error":e.code}), |v| v);
    Ok(
        json!({"read_only":true,"receipts":receipts,"now":{"boot":swarm_platform::lifecycle::boot_id()?,"interactive":interactive,"runtime":runtime,"startup":startup},"no_phase_replay":true}),
    )
}
fn main() {
    let message = match run() {
        Ok(v) => {
            let mut text=format!("READ_ONLY_DIAGNOSTIC; NO PHASE REPLAY\nCurrent boot: {}\nCurrent interactive: {}\nCurrent runtime: {}\nCurrent startup: {}\n",v["now"]["boot"],v["now"]["interactive"],v["now"]["runtime"],v["now"]["startup"]);
            if let Some(entries)=v["receipts"].as_object(){for (name,r) in entries {text.push_str(&format!("\n{name}: exists={}\nboot={} session={} runtime={} version={}\n",r["exists"],r["boot_id"],r["session"],r["runtime"],r["version"]));}}
            text
        },
        Err(e) => format!("DIAGNOSTIC_FAILED\n{e}"),
    };
    let text: Vec<_> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<_> = "Swarm fixed read-only diagnostic"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe { MessageBoxW(0, text.as_ptr(), title.as_ptr(), 0) };
}
