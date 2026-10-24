use std::{fs::OpenOptions, io::{Read, Write}, path::Path, time::{Duration, Instant}};
use swarm_platform::{BackupStore, CredentialStore, PlatformError};
const ROOT: &str = "C:/Swarm-Validation/Runs/acl-20261020";
const KEY: &str = "r01_cipher";
fn mark(name: &str) {
    OpenOptions::new().create_new(true).write(true).open(Path::new(ROOT).join(name))
        .expect("exclusive marker").write_all(b"synthetic validation marker").unwrap();
}
fn wait(name: &str) {
    let deadline = Instant::now() + Duration::from_secs(45);
    while !Path::new(ROOT).join(name).is_file() {
        assert!(Instant::now() < deadline, "marker deadline; no retry");
        std::thread::sleep(Duration::from_millis(50));
    }
}
fn main() {
    if std::env::var("COMPUTERNAME").unwrap_or_default() != "DESKTOP-1TAP8BI" {
        eprintln!("guest only; refused before filesystem access");
        std::process::exit(2);
    }
    let root = Path::new(ROOT);
    assert!(root.is_dir(), "observer must exclusively create fixed run root");
    match std::env::args().nth(1).as_deref() {
        Some("first") => {
            assert!(!root.join("credentials").exists() && !root.join("backups").exists(), "preserve prior stores");
            mark("first-started");
            let store = CredentialStore::open(&root.join("credentials")).unwrap();
            store.put(KEY, b"r01 synthetic value").unwrap();
            assert_eq!(store.get(KEY).unwrap().unwrap(), b"r01 synthetic value");
            let backups = BackupStore::open(&root.join("backups")).unwrap();
            backups.write_new("r01", |file| file.write_all(b"r01 synthetic backup").map_err(|_| PlatformError::Io)).unwrap();
            let lock = store.lock("r01_lock").unwrap();
            mark("first-held");
            wait("release-first");
            drop(lock);
            mark("first-released");
        }
        Some("second") => {
            assert!(root.join("first-held").is_file(), "first holder required");
            let store = CredentialStore::open(&root.join("credentials")).unwrap();
            mark("second-started");
            let lock = store.lock("r01_lock").unwrap();
            mark("second-acquired");
            assert_eq!(store.get(KEY).unwrap().unwrap(), b"r01 synthetic value");
            let backups = BackupStore::open(&root.join("backups")).unwrap();
            let mut bytes = Vec::new();
            backups.read("r01").unwrap().read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, b"r01 synthetic backup");
            wait("release-second");
            drop(lock);
            mark("second-released");
        }
        _ => panic!("first or second only"),
    }
    // Never print plaintext, encrypted data, credentials or backup content.
    println!("synthetic store and lock probe complete");
}
