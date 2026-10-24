use rusqlite::Connection;
use swarm_core::Service;

#[test]
fn captures_only_global_and_registered_local_rules_and_marks_missing_local() {
    let data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let absent = tempfile::tempdir().unwrap();
    let service = Service::open(data.path()).unwrap();
    std::fs::write(project.path().join("RULES.md"), "# local rules\n").unwrap();
    std::fs::write(
        project.path().join("private-source.rs"),
        "EXCLUDED_SOURCE_SENTINEL",
    )
    .unwrap();
    let db = Connection::open(data.path().join("swarm.sqlite3")).unwrap();
    for (id, root) in [("present", project.path()), ("absent", absent.path())] {
        db.execute(
            "INSERT INTO projects VALUES(?1,?1,?2,'now')",
            rusqlite::params![id, root.to_str().unwrap()],
        )
        .unwrap();
    }
    let prepared = service.prepare_backup().unwrap();
    assert_eq!(prepared.rules().len(), 3);
    assert!(prepared.rules()[0].project_id.is_none());
    assert!(
        prepared.rules()[0]
            .content
            .as_ref()
            .unwrap()
            .contains("swarm-rules")
    );
    let local = prepared
        .rules()
        .iter()
        .find(|r| r.project_id.as_deref() == Some("present"))
        .unwrap();
    assert_eq!(local.content.as_deref(), Some("# local rules\n"));
    assert!(local.revision.starts_with("sha256:"));
    let missing = prepared
        .rules()
        .iter()
        .find(|r| r.project_id.as_deref() == Some("absent"))
        .unwrap();
    assert!(missing.content.is_none());
    assert_eq!(missing.revision, "missing:v1");
    assert!(!prepared.rules().iter().any(|r| {
        r.content
            .as_deref()
            .unwrap_or("")
            .contains("EXCLUDED_SOURCE_SENTINEL")
    }));
}

#[test]
fn missing_global_or_unavailable_registered_root_prevents_backup() {
    let data = tempfile::tempdir().unwrap();
    let service = Service::open(data.path()).unwrap();
    std::fs::remove_file(data.path().join("RULES.md")).unwrap();
    assert!(service.prepare_backup().is_err());
    std::fs::write(data.path().join("RULES.md"), "# global").unwrap();
    let db = Connection::open(data.path().join("swarm.sqlite3")).unwrap();
    db.execute(
        "INSERT INTO projects VALUES('missing','missing',?1,'now')",
        [data.path().join("not-present").to_str().unwrap()],
    )
    .unwrap();
    assert!(service.prepare_backup().is_err());
}

#[test]
fn invalid_utf8_or_oversized_rules_fail_without_changing_original() {
    let data = tempfile::tempdir().unwrap();
    let service = Service::open(data.path()).unwrap();
    let rules = data.path().join("RULES.md");
    std::fs::write(&rules, [0xff, 0xfe]).unwrap();
    assert!(service.prepare_backup().is_err());
    assert_eq!(std::fs::read(&rules).unwrap(), [0xff, 0xfe]);
    std::fs::write(&rules, vec![b'x'; 1024 * 1024 + 1]).unwrap();
    assert!(service.prepare_backup().is_err());
    assert_eq!(std::fs::metadata(rules).unwrap().len(), 1024 * 1024 + 1);
}

#[cfg(windows)]
#[test]
fn hard_linked_rules_are_rejected_without_changing_external_file() {
    let data = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let service = Service::open(data.path()).unwrap();
    let link = outside.path().join("alias");
    std::fs::hard_link(data.path().join("RULES.md"), &link).unwrap();
    let before = std::fs::read(&link).unwrap();
    assert!(service.prepare_backup().is_err());
    assert_eq!(std::fs::read(&link).unwrap(), before);
}

#[cfg(windows)]
#[test]
fn registered_root_replaced_by_junction_is_not_treated_as_missing_local_rules() {
    let data = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let service = Service::open(data.path()).unwrap();
    let junction = data.path().join("project-junction");
    let status = std::process::Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command",
        "New-Item -ItemType Junction -Path $env:SWARM_BACKUP_JUNCTION -Target $env:SWARM_BACKUP_TARGET | Out-Null"])
        .env("SWARM_BACKUP_JUNCTION",&junction).env("SWARM_BACKUP_TARGET",target.path()).output().unwrap();
    assert!(
        status.status.success(),
        "temporary junction creation failed"
    );
    let db = Connection::open(data.path().join("swarm.sqlite3")).unwrap();
    db.execute(
        "INSERT INTO projects VALUES('alias','alias',?1,'now')",
        [junction.to_str().unwrap()],
    )
    .unwrap();
    assert!(service.prepare_backup().is_err());
    std::fs::remove_dir(junction).unwrap();
    assert!(!target.path().join("RULES.md").exists());
}
