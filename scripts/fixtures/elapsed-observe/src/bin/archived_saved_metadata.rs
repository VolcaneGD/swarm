//! Read only the existing released AI's public identity metadata. No runtime/IPC.
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};
use swarm_platform::CredentialStore;
fn main() -> Result<(), String> {
    const BASE: &str = "E:/Swarm-Validation-Host/run-20261023-archived-retention";
    if std::env::args().len() != 1 {
        return Err("fixed metadata read only".into());
    }
    let b: Value = serde_json::from_slice(
        &fs::read(Path::new(BASE).join("baseline-pending-closeout.json"))
            .map_err(|_| "prepared metadata missing")?,
    )
    .map_err(|_| "prepared JSON invalid")?;
    let store = CredentialStore::open(&Path::new(BASE).join("Data/client-credentials"))
        .map_err(|_| "saved identity store unavailable")?;
    let bytes = store
        .get("archived-retention-20261023")
        .map_err(|_| "saved alias unavailable")?
        .ok_or("same saved alias required")?;
    let identity: Value = serde_json::from_slice(&bytes).map_err(|_| "saved identity invalid")?;
    if identity["kind"] != "ai"
        || identity["project_id"] != b["project_id"]
        || !identity["id"]
            .as_str()
            .is_some_and(|s| s.len() == 36 && s.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-'))
    {
        return Err("saved public metadata mismatch".into());
    }
    let v = json!({"actor_id":identity["id"],"project_id":identity["project_id"],"alias":"archived-retention-20261023","same_existing_identity":true,"runtime_started":false,"ipc_opened":false,"credentials_emitted":false});
    let mut out = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(Path::new(BASE).join("public-identity-metadata.json"))
        .map_err(|_| "preserve existing metadata receipt")?;
    writeln!(out, "{v}").map_err(|_| "metadata receipt write failed")?;
    println!("SAVED_PUBLIC_IDENTITY_METADATA_CONFIRMED");
    Ok(())
}
