#![cfg(windows)]
use serde_json::{Value, json};
use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt, path::Path};
use swarm_core::{
    Service,
    protocol::{Fault, Request},
};

fn call(
    service: &mut Service,
    operation: &str,
    auth: Option<&str>,
    args: Value,
) -> Result<Value, Fault> {
    service.request(Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    })
}

fn valid_source() -> &'static str {
    "# Editor save test\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: locking-test\n    effect: allow\n    actions: [post.create]\naccess:\n  boards: []\n  threads: []\n~~~\n"
}

fn save(service: &mut Service, token: &str, source: &str, revision: &str) -> Result<Value, Fault> {
    call(
        service,
        "rules.save",
        Some(token),
        json!({"source":source,"if_revision":revision}),
    )
}

fn assert_no_recovery_or_temp_files(directory: &Path) {
    for entry in std::fs::read_dir(directory).unwrap().filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(
            !name.contains(".recovery-"),
            "unexpected recovery file: {name}"
        );
        assert!(!name.contains(".tmp-"), "unexpected staged file: {name}");
    }
}

#[test]
fn target_lock_conflicts_stale_revision_and_hardlinks_never_publish() {
    let directory = tempfile::tempdir().unwrap();
    let mut service = Service::open(directory.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"rules-lock-admin-fixture"}),
    )
    .unwrap();
    let token = admin["token"].as_str().unwrap();
    let target = directory.path().join("RULES.md");
    let initial = call(&mut service, "rules.get", Some(token), json!({})).unwrap();
    let old_source = std::fs::read(&target).unwrap();

    for (read, write) in [(true, false), (false, true)] {
        let mut options = OpenOptions::new();
        options
            .read(read)
            .write(write)
            .share_mode(0x0000_0001 | 0x0000_0002 | 0x0000_0004);
        let external = options.open(&target).unwrap();
        let error = save(
            &mut service,
            token,
            valid_source(),
            initial["revision"].as_str().unwrap(),
        )
        .unwrap_err();
        assert_eq!(error.code, "RULES_BUSY");
        assert_eq!(std::fs::read(&target).unwrap(), old_source);
        drop(external);
        assert_no_recovery_or_temp_files(directory.path());
    }

    let changed_source = "# Changed by external editor\n";
    std::fs::write(&target, changed_source).unwrap();
    let stale = save(
        &mut service,
        token,
        valid_source(),
        initial["revision"].as_str().unwrap(),
    )
    .unwrap_err();
    assert_eq!(stale.code, "RULES_CHANGED");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), changed_source);
    assert_no_recovery_or_temp_files(directory.path());

    let current = call(&mut service, "rules.get", Some(token), json!({})).unwrap();
    let hardlink = directory.path().join("external-hardlink.md");
    std::fs::hard_link(&target, &hardlink)
        .expect("Windows test volume must support NTFS-style hard links");
    let linked = save(
        &mut service,
        token,
        valid_source(),
        current["revision"].as_str().unwrap(),
    )
    .unwrap_err();
    assert_eq!(linked.code, "RULES_BUSY");
    assert_eq!(std::fs::read_to_string(&target).unwrap(), changed_source);
    assert_eq!(std::fs::read_to_string(&hardlink).unwrap(), changed_source);
    assert_no_recovery_or_temp_files(directory.path());

    std::fs::remove_file(&hardlink).unwrap();
    let current = call(&mut service, "rules.get", Some(token), json!({})).unwrap();
    let saved = save(
        &mut service,
        token,
        valid_source(),
        current["revision"].as_str().unwrap(),
    )
    .unwrap();
    assert_eq!(saved["source"], valid_source());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), valid_source());
    assert_no_recovery_or_temp_files(directory.path());
}
