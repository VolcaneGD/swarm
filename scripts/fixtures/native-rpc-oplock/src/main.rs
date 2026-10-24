//! Frozen-runtime native RPC feasibility and execution are separate phases.
mod frozen;
mod oplock;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    time::Duration,
};
use swarm_platform::CredentialStore;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn ensure(b: bool, m: &str) -> Result<()> {
    if b { Ok(()) } else { Err(m.into()) }
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn save(base: &Path, name: &str, v: &Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(base.join(name))?;
    writeln!(f, "{v}")?;
    Ok(())
}
fn probe() -> Result<()> {
    let base = Path::new("E:/Swarm-Validation-Host/run-20261022-oplock-feasibility");
    ensure(!base.exists(), "existing probe refused")?;
    fs::create_dir(base)?;
    let store_root = base.join("SyntheticStore");
    let store = CredentialStore::open(&store_root)?;
    store.put("probe", b"original synthetic value")?;
    let mut held =
        oplock::ReadHandleOplock::new(&store_root.join(format!("{}.bin", hash(b"probe"))))?;
    ensure(
        store.get("probe")?.as_deref() == Some(b"original synthetic value"),
        "read changed",
    )?;
    ensure(held.broken(100)?.is_none(), "read broke RH oplock")?;
    let (tx, rx) = std::sync::mpsc::channel();
    let writer = std::thread::spawn(move || {
        let r = store.put("probe", b"replacement synthetic value");
        let _ = tx.send(r.is_ok());
    });
    let observed = held.broken(3000)?;
    let pending = matches!(
        rx.recv_timeout(Duration::from_millis(200)),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout)
    );
    save(
        base,
        "before-release.json",
        &json!({"break":observed,"writer_pending":pending,"read_did_not_break":true,"synthetic_store_only":true}),
    )?;
    drop(held);
    let replaced = rx.recv_timeout(Duration::from_secs(5))?;
    writer.join().map_err(|_| "probe writer failed")?;
    ensure(
        observed.is_some_and(|(_, _, flags)| flags & 1 != 0) && pending && replaced,
        "RH write admission was not held until release",
    )?;
    ensure(
        CredentialStore::open(&store_root)?.get("probe")?.as_deref()
            == Some(b"replacement synthetic value"),
        "final value differs",
    )?;
    save(
        base,
        "result.json",
        &json!({"status":"CREDENTIAL_STORE_RH_REPLACEMENT_HOLD_PASS","product_started":false,"original_files_preserved":true,"handle_closed":true}),
    )?;
    println!("CREDENTIAL_STORE_RH_REPLACEMENT_HOLD_PASS");
    Ok(())
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.as_slice() {
        [a] if a == "--probe" => probe(),
        _ => frozen::run(&args),
    };
    if let Err(e) = r {
        if let Some(f) = e.downcast_ref::<swarm_core::protocol::Fault>() {
            eprintln!("Product fault code: {}", f.code);
        }
        eprintln!("Oplock fixture failed; preserve all owned evidence");
        std::process::exit(1)
    }
}
