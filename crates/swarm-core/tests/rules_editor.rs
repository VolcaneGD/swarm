use serde_json::{Value, json};
use std::{
    path::Path,
    sync::{Arc, Barrier},
};
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

fn fixture() -> (tempfile::TempDir, Service, String) {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"rules-admin-fixture"}),
    )
    .unwrap();
    (root, service, admin["token"].as_str().unwrap().to_owned())
}

fn valid_rules(action: &str, project: Option<&str>) -> String {
    let project_subject = project
        .map(|id| format!("\n    subjects:\n      projects: [{id}]"))
        .unwrap_or_default();
    format!(
        "# Keep this comment\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: editor-test\n    effect: allow{project_subject}\n    actions: [{action}]\naccess:\n  boards: []\n  threads: []\n~~~\n"
    )
}

#[test]
fn account_ui_preferences_are_human_scoped_validated_and_backed_up() {
    let (_root, mut service, admin) = fixture();
    let account = call(&mut service, "account.status", Some(&admin), json!({})).unwrap();
    let account_id = account["id"].as_str().unwrap();
    let layout = json!({"version":1,"treeWidth":320,"composerHeight":260});
    assert_eq!(
        call(
            &mut service,
            "ui.preferences.set",
            Some(&admin),
            json!({
                "expected_account_id":account_id,"scope":"layout","value":layout
            })
        )
        .unwrap(),
        json!({"saved":true})
    );
    assert_eq!(
        call(
            &mut service,
            "ui.preferences.get",
            Some(&admin),
            json!({
                "expected_account_id":account_id,"scope":"layout"
            })
        )
        .unwrap(),
        json!({"value":layout,"migration_allowed":true})
    );

    let view = json!("threaded");
    call(
        &mut service,
        "ui.preferences.set",
        Some(&admin),
        json!({
            "expected_account_id":account_id,"scope":"conversation_view","value":view
        }),
    )
    .unwrap();
    let thread_id = "00000000-0000-4000-8000-000000000001";
    let scroll = json!({"version":1,"postNo":42,"offset":2,"scrollTop":1250,"atBottom":false,"page":2,"pageSize":100});
    call(
        &mut service,
        "ui.preferences.set",
        Some(&admin),
        json!({
            "expected_account_id":account_id,"scope":"thread_scroll","key":thread_id,"value":scroll
        }),
    )
    .unwrap();
    assert_eq!(
        call(
            &mut service,
            "ui.preferences.get",
            Some(&admin),
            json!({
                "expected_account_id":account_id,"scope":"thread_scroll","key":thread_id
            })
        )
        .unwrap(),
        json!({"value":scroll,"migration_allowed":true})
    );

    assert_eq!(
        call(
            &mut service,
            "ui.preferences.get",
            Some(&admin),
            json!({
                "expected_account_id":"00000000-0000-4000-8000-000000000099","scope":"layout"
            })
        )
        .unwrap_err()
        .code,
        "ACCOUNT_CHANGED"
    );
    assert_eq!(call(&mut service,"ui.preferences.set",Some(&admin),json!({
        "expected_account_id":account_id,"scope":"layout","value":{"version":1,"treeWidth":999,"composerHeight":260}
    })).unwrap_err().code,"INVALID_INPUT");
    assert_eq!(
        call(
            &mut service,
            "ui.preferences.get",
            Some(&admin),
            json!({
                "expected_account_id":account_id,"scope":"layout","key":"extra"
            })
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );

    let snapshot = service.credential_free_snapshot().unwrap();
    let serialized: String = snapshot
        .query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [format!("ui-preference-v1:{account_id}:layout")],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(&serialized).unwrap(), layout);
    service.set_startup_preference(false).unwrap();
    let snapshot = service.credential_free_snapshot().unwrap();
    let startup: String = snapshot
        .query_row(
            "SELECT value FROM app_metadata WHERE key='startup-desired-v1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&startup).unwrap(),
        json!({"version":1,"enabled":false})
    );
}

#[test]
fn window_geometry_is_trusted_global_storage_and_survives_backup() {
    let (_root, mut service, _admin) = fixture();
    let geometry = swarm_core::WindowGeometry {
        version: 1,
        x: -120,
        y: 40,
        width: 1200,
        height: 800,
        maximized: false,
    };
    service.window_geometry_set(&geometry).unwrap();
    assert_eq!(service.window_geometry_get().unwrap(), Some(geometry));
    let snapshot = service.credential_free_snapshot().unwrap();
    let saved: String = snapshot
        .query_row(
            "SELECT value FROM app_metadata WHERE key='window-geometry-v1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&saved).unwrap()["width"],
        1200
    );
}

#[test]
fn rules_editor_permissions_validation_and_exact_source_round_trip() {
    let (_root, mut service, admin) = fixture();
    let member = call(
        &mut service,
        "account.create",
        Some(&admin),
        json!({"name":"RulesMember","password":"rules-member-fixture"}),
    )
    .unwrap();
    let member_login = call(
        &mut service,
        "account.login",
        None,
        json!({"name":"RulesMember","password":"rules-member-fixture"}),
    )
    .unwrap();
    let token = member_login["token"].as_str().unwrap();

    assert_eq!(
        call(&mut service, "rules.get", Some(token), json!({}))
            .unwrap_err()
            .code,
        "PERMISSION_DENIED"
    );
    let capabilities = call(&mut service, "rules.capabilities", Some(token), json!({})).unwrap();
    assert_eq!(
        capabilities["global"],
        json!({"read":false,"check":false,"validate":false,"save":false})
    );

    for operation in ["rules.get", "rules.validate", "rules.save"] {
        call(
            &mut service,
            "permission.set",
            Some(&admin),
            json!({
                "subject_type":"account",
                "subject_id":member["id"],
                "operation":operation,
                "scope":"global",
                "target_id":"",
                "decision":"allow"
            }),
        )
        .unwrap();
    }
    let current = call(&mut service, "rules.get", Some(token), json!({})).unwrap();
    let source = valid_rules("post.create", None);
    let validation = call(
        &mut service,
        "rules.validate",
        Some(token),
        json!({"source":source}),
    )
    .unwrap();
    assert_eq!(validation["valid"], true);
    assert_eq!(validation["actions"], json!(["post.create"]));
    let invalid = call(
        &mut service,
        "rules.validate",
        Some(token),
        json!({"source":"~~~swarm-rules\nnot: [valid\n~~~"}),
    )
    .unwrap();
    assert_eq!(invalid["valid"], false);

    let saved = call(
        &mut service,
        "rules.save",
        Some(token),
        json!({"source":source,"if_revision":current["revision"]}),
    )
    .unwrap();
    assert_eq!(saved["source"], source);
    assert_eq!(saved["scope"], "global");
    let reread = call(&mut service, "rules.get", Some(token), json!({})).unwrap();
    assert_eq!(reread["source"], source);
    assert!(source.starts_with("# Keep this comment"));

    let ai_project_root = tempfile::tempdir().unwrap();
    let ai_project = call(
        &mut service,
        "project.register",
        Some(&admin),
        json!({"name":"AI editor guard project","root":ai_project_root.path()}),
    )
    .unwrap();
    let ai = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":ai_project["id"]}),
    )
    .unwrap();
    for operation in [
        "rules.capabilities",
        "rules.get",
        "rules.validate",
        "rules.save",
        "ui.preferences.get",
        "ui.preferences.set",
    ] {
        assert_eq!(
            call(
                &mut service,
                operation,
                Some(ai["session_token"].as_str().unwrap()),
                json!({})
            )
            .unwrap_err()
            .code,
            "PERMISSION_DENIED",
            "AI must not call {operation}"
        );
    }
    assert_eq!(
        call(
            &mut service,
            "rules.get",
            Some(ai["session_token"].as_str().unwrap()),
            json!({"recovery_id":"0123456789abcdef0123456789abcdef"})
        )
        .unwrap_err()
        .code,
        "PERMISSION_DENIED"
    );
    assert_eq!(
        call(
            &mut service,
            "rules.save",
            Some(ai["session_token"].as_str().unwrap()),
            json!({
                "source":valid_rules("post.create", None),
                "if_revision":"missing:v1",
                "recovery_id":"0123456789abcdef0123456789abcdef",
                "recovery_revision":"sha256:unknown"
            })
        )
        .unwrap_err()
        .code,
        "PERMISSION_DENIED"
    );
}

#[test]
fn external_revision_change_is_reported_and_preserved() {
    let (root, mut service, admin) = fixture();
    let current = call(&mut service, "rules.get", Some(&admin), json!({})).unwrap();
    let external = "# External edit\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules: []\naccess:\n  boards: []\n  threads: []\n~~~\n";
    std::fs::write(root.path().join("RULES.md"), external).unwrap();
    let error = call(
        &mut service,
        "rules.save",
        Some(&admin),
        json!({"source":valid_rules("post.create", None),"if_revision":current["revision"]}),
    )
    .unwrap_err();
    assert_eq!(error.code, "RULES_CHANGED");
    assert_eq!(
        std::fs::read_to_string(root.path().join("RULES.md")).unwrap(),
        external
    );
}

#[test]
fn concurrent_internal_saves_with_same_revision_have_one_winner() {
    let (root, mut service, admin) = fixture();
    let initial = call(&mut service, "rules.get", Some(&admin), json!({})).unwrap();
    let revision = initial["revision"].as_str().unwrap().to_owned();
    let root_path = root.path().to_owned();
    let barrier = Arc::new(Barrier::new(2));
    let workers = ["post.create", "post.edit"].map(|action| {
        let root_path = root_path.clone();
        let revision = revision.clone();
        let barrier = barrier.clone();
        let admin = admin.clone();
        std::thread::spawn(move || {
            let mut service = Service::open(&root_path).unwrap();
            let source = valid_rules(action, None);
            barrier.wait();
            call(
                &mut service,
                "rules.save",
                Some(&admin),
                json!({"source":source,"if_revision":revision}),
            )
        })
    });
    let results = workers.map(|worker| worker.join().unwrap());
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .next()
            .unwrap()
            .code,
        "RULES_CHANGED"
    );
}

#[test]
fn project_rules_require_current_project_identity_and_root() {
    let (root, mut service, admin) = fixture();
    let project_root = tempfile::tempdir().unwrap();
    let project = call(
        &mut service,
        "project.register",
        Some(&admin),
        json!({"name":"Local rules project","root":project_root.path()}),
    )
    .unwrap();
    let project_id = project["id"].as_str().unwrap();
    let path = Path::new(project["canonical_root"].as_str().unwrap()).join("RULES.md");
    let source = valid_rules("post.create", Some(project_id));
    let missing = call(
        &mut service,
        "rules.get",
        Some(&admin),
        json!({"project_id":project_id}),
    )
    .unwrap();
    assert_eq!(missing["source"], "");
    let saved = call(
        &mut service,
        "rules.save",
        Some(&admin),
        json!({"project_id":project_id,"source":source,"if_revision":missing["revision"]}),
    )
    .unwrap();
    assert_eq!(saved["project_id"], project_id);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), source);

    std::fs::remove_dir_all(project_root.path()).unwrap();
    let unavailable = call(
        &mut service,
        "rules.get",
        Some(&admin),
        json!({"project_id":project_id}),
    )
    .unwrap_err();
    assert_eq!(unavailable.code, "PROJECT_ROOT_UNAVAILABLE");
    assert!(
        !unavailable
            .message
            .contains(project_root.path().to_string_lossy().as_ref())
    );

    let changed_project_id = call(
        &mut service,
        "rules.get",
        Some(&admin),
        json!({"project_id":null}),
    )
    .unwrap_err();
    assert_eq!(changed_project_id.code, "INVALID_INPUT");
    let _ = root;
}

fn raw_revision(bytes: Option<&[u8]>) -> String {
    use sha2::{Digest, Sha256};
    match bytes {
        Some(bytes) => format!("sha256:{:x}", Sha256::digest(bytes)),
        None => "missing:v1".to_owned(),
    }
}

#[test]
fn recovery_inspection_identifies_raw_current_and_candidate_and_applies_explicitly() {
    let (root, mut service, admin) = fixture();
    let candidate = valid_rules("post.create", None);
    let recovery_id = "0123456789abcdef0123456789abcdef";
    let current = b"\xffincomplete policy";
    let current_path = root.path().join("RULES.md");
    let recovery_path = root
        .path()
        .join(format!(".RULES.md.recovery-{recovery_id}"));
    std::fs::write(&current_path, current).unwrap();
    std::fs::write(&recovery_path, candidate.as_bytes()).unwrap();

    let snapshot = call(
        &mut service,
        "rules.get",
        Some(&admin),
        json!({"recovery_id":recovery_id}),
    )
    .unwrap();
    assert_eq!(snapshot["scope"], "global");
    assert_eq!(snapshot["project_id"], Value::Null);
    assert_eq!(snapshot["recovery_id"], recovery_id);
    assert_eq!(snapshot["current"]["source"], Value::Null);
    assert_eq!(snapshot["current"]["revision"], raw_revision(Some(current)));
    assert_eq!(snapshot["current"]["utf8"], false);
    assert_eq!(snapshot["current"]["exists"], true);
    assert_eq!(snapshot["candidate"]["source"], candidate);
    assert_eq!(
        snapshot["candidate"]["revision"],
        raw_revision(Some(candidate.as_bytes()))
    );
    assert_eq!(snapshot["candidate"]["valid"], true);

    let saved = call(
        &mut service,
        "rules.save",
        Some(&admin),
        json!({
            "source":candidate,
            "if_revision":snapshot["current"]["revision"],
            "recovery_id":recovery_id,
            "recovery_revision":snapshot["candidate"]["revision"]
        }),
    )
    .unwrap();
    assert_eq!(saved["source"], candidate);
    assert_eq!(std::fs::read_to_string(&current_path).unwrap(), candidate);
    assert_eq!(std::fs::read_to_string(&recovery_path).unwrap(), candidate);

    std::fs::remove_file(&current_path).unwrap();
    let missing_current = call(
        &mut service,
        "rules.get",
        Some(&admin),
        json!({"recovery_id":recovery_id}),
    )
    .unwrap();
    assert_eq!(missing_current["current"]["source"], Value::Null);
    assert_eq!(missing_current["current"]["revision"], "missing:v1");
    assert_eq!(missing_current["current"]["exists"], false);
    assert_eq!(
        call(&mut service, "rules.get", Some(&admin), json!({}))
            .unwrap_err()
            .code,
        "RULES_INVALID"
    );
}

#[test]
fn recovery_inspection_requires_exact_candidate_and_apply_rejects_changed_bytes() {
    let (root, mut service, admin) = fixture();
    let candidate = valid_rules("post.create", None);
    let recovery_id = "fedcba9876543210fedcba9876543210";
    let current = valid_rules("post.edit", None);
    let current_path = root.path().join("RULES.md");
    let recovery_path = root
        .path()
        .join(format!(".RULES.md.recovery-{recovery_id}"));
    std::fs::write(&current_path, &current).unwrap();
    std::fs::write(&recovery_path, &candidate).unwrap();
    let snapshot = call(
        &mut service,
        "rules.get",
        Some(&admin),
        json!({"recovery_id":recovery_id}),
    )
    .unwrap();

    std::fs::write(&current_path, "external current edit").unwrap();
    let stale_current = call(
        &mut service,
        "rules.save",
        Some(&admin),
        json!({
            "source":candidate,
            "if_revision":snapshot["current"]["revision"],
            "recovery_id":recovery_id,
            "recovery_revision":snapshot["candidate"]["revision"]
        }),
    )
    .unwrap_err();
    assert_eq!(stale_current.code, "RULES_CHANGED");
    assert_eq!(
        std::fs::read_to_string(&current_path).unwrap(),
        "external current edit"
    );

    std::fs::write(&recovery_path, "changed candidate").unwrap();
    let stale_candidate = call(
        &mut service,
        "rules.save",
        Some(&admin),
        json!({
            "source":candidate,
            "if_revision":raw_revision(Some(b"external current edit")),
            "recovery_id":recovery_id,
            "recovery_revision":snapshot["candidate"]["revision"]
        }),
    )
    .unwrap_err();
    assert_eq!(stale_candidate.code, "RULES_CHANGED");
    assert!(!stale_candidate.message.contains("RULES.md"));

    std::fs::write(&recovery_path, &candidate).unwrap();
    let mismatched_source = call(
        &mut service,
        "rules.save",
        Some(&admin),
        json!({
            "source":valid_rules("post.delete", None),
            "if_revision":raw_revision(Some(b"external current edit")),
            "recovery_id":recovery_id,
            "recovery_revision":raw_revision(Some(candidate.as_bytes()))
        }),
    )
    .unwrap_err();
    assert_eq!(mismatched_source.code, "RULES_CHANGED");

    std::fs::remove_file(&recovery_path).unwrap();
    let deleted_candidate = call(
        &mut service,
        "rules.get",
        Some(&admin),
        json!({"recovery_id":recovery_id}),
    )
    .unwrap_err();
    assert_eq!(deleted_candidate.code, "RULES_CHANGED");
    assert!(
        !deleted_candidate
            .message
            .contains(root.path().to_string_lossy().as_ref())
    );
}

#[test]
fn recovery_arguments_are_strict_and_existing_permissions_apply() {
    let (root, mut service, admin) = fixture();
    for args in [
        json!({"recovery_id":"../RULES.md"}),
        json!({"recovery_id":"0123456789abcdef0123456789abcdef","unexpected":true}),
    ] {
        assert_eq!(
            call(&mut service, "rules.get", Some(&admin), args)
                .unwrap_err()
                .code,
            "INVALID_INPUT"
        );
    }
    let unpaired = call(
        &mut service,
        "rules.save",
        Some(&admin),
        json!({
            "source":valid_rules("post.create", None),
            "if_revision":raw_revision(Some(std::fs::read(root.path().join("RULES.md")).unwrap().as_slice())),
            "recovery_id":"0123456789abcdef0123456789abcdef"
        }),
    )
    .unwrap_err();
    assert_eq!(unpaired.code, "INVALID_INPUT");
}

#[test]
fn recovery_requires_existing_read_and_save_permissions() {
    let (root, mut service, admin) = fixture();
    let member = call(
        &mut service,
        "account.create",
        Some(&admin),
        json!({"name":"RecoveryReader","password":"recovery-reader-fixture"}),
    )
    .unwrap();
    let member_login = call(
        &mut service,
        "account.login",
        None,
        json!({"name":"RecoveryReader","password":"recovery-reader-fixture"}),
    )
    .unwrap();
    let token = member_login["token"].as_str().unwrap();
    let recovery_id = "0123456789abcdef0123456789abcdef";
    let candidate = valid_rules("post.create", None);
    let current = valid_rules("post.edit", None);
    let current_path = root.path().join("RULES.md");
    std::fs::write(&current_path, &current).unwrap();
    std::fs::write(
        root.path()
            .join(format!(".RULES.md.recovery-{recovery_id}")),
        &candidate,
    )
    .unwrap();

    for operation in ["rules.get", "rules.save"] {
        call(
            &mut service,
            "permission.set",
            Some(&admin),
            json!({
                "subject_type":"account",
                "subject_id":member["id"],
                "operation":operation,
                "scope":"global",
                "target_id":"",
                "decision":"allow"
            }),
        )
        .unwrap();
    }
    let snapshot = call(
        &mut service,
        "rules.get",
        Some(token),
        json!({"recovery_id":recovery_id}),
    )
    .unwrap();
    call(
        &mut service,
        "permission.set",
        Some(&admin),
        json!({
            "subject_type":"account",
            "subject_id":member["id"],
            "operation":"rules.get",
            "scope":"global",
            "target_id":"",
            "decision":"deny"
        }),
    )
    .unwrap();
    let read_denied_apply = call(
        &mut service,
        "rules.save",
        Some(token),
        json!({
            "source":candidate,
            "if_revision":snapshot["current"]["revision"],
            "recovery_id":recovery_id,
            "recovery_revision":snapshot["candidate"]["revision"]
        }),
    )
    .unwrap_err();
    assert_eq!(read_denied_apply.code, "PERMISSION_DENIED");

    call(
        &mut service,
        "permission.set",
        Some(&admin),
        json!({
            "subject_type":"account",
            "subject_id":member["id"],
            "operation":"rules.get",
            "scope":"global",
            "target_id":"",
            "decision":"allow"
        }),
    )
    .unwrap();
    call(
        &mut service,
        "permission.set",
        Some(&admin),
        json!({
            "subject_type":"account",
            "subject_id":member["id"],
            "operation":"rules.save",
            "scope":"global",
            "target_id":"",
            "decision":"deny"
        }),
    )
    .unwrap();
    let save_denied = call(
        &mut service,
        "rules.save",
        Some(token),
        json!({
            "source":candidate,
            "if_revision":snapshot["current"]["revision"],
            "recovery_id":recovery_id,
            "recovery_revision":snapshot["candidate"]["revision"]
        }),
    )
    .unwrap_err();
    assert_eq!(save_denied.code, "PERMISSION_DENIED");

    call(
        &mut service,
        "permission.set",
        Some(&admin),
        json!({
            "subject_type":"account",
            "subject_id":member["id"],
            "operation":"rules.get",
            "scope":"global",
            "target_id":"",
            "decision":"deny"
        }),
    )
    .unwrap();
    let read_denied = call(
        &mut service,
        "rules.get",
        Some(token),
        json!({"recovery_id":recovery_id}),
    )
    .unwrap_err();
    assert_eq!(read_denied.code, "PERMISSION_DENIED");
}

#[test]
fn invalid_missing_oversized_and_project_recovery_candidates_are_scoped() {
    let (root, mut service, admin) = fixture();
    let recovery_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let invalid_candidate = "not a valid RULES source";
    let global_path = root
        .path()
        .join(format!(".RULES.md.recovery-{recovery_id}"));
    let invalid_utf8_id = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    std::fs::write(
        root.path()
            .join(format!(".RULES.md.recovery-{invalid_utf8_id}")),
        [0xff, 0xfe],
    )
    .unwrap();
    let invalid_utf8 = call(
        &mut service,
        "rules.get",
        Some(&admin),
        json!({"recovery_id":invalid_utf8_id}),
    )
    .unwrap_err();
    assert_eq!(invalid_utf8.code, "RULES_INVALID");
    assert_eq!(
        invalid_utf8.message,
        "RULES recovery copy is not valid UTF-8"
    );

    std::fs::write(&global_path, invalid_candidate).unwrap();
    let hardlink_id = "dddddddddddddddddddddddddddddddd";
    let hardlink_source = root.path().join(".RULES.md.external");
    std::fs::write(&hardlink_source, invalid_candidate).unwrap();
    std::fs::hard_link(
        &hardlink_source,
        root.path()
            .join(format!(".RULES.md.recovery-{hardlink_id}")),
    )
    .unwrap();
    assert_eq!(
        call(
            &mut service,
            "rules.get",
            Some(&admin),
            json!({"recovery_id":hardlink_id})
        )
        .unwrap_err()
        .code,
        "INTERNAL_ERROR"
    );
    let invalid = call(
        &mut service,
        "rules.get",
        Some(&admin),
        json!({"recovery_id":recovery_id}),
    )
    .unwrap();
    assert_eq!(invalid["candidate"]["valid"], false);
    assert_eq!(invalid["candidate"]["source"], invalid_candidate);
    assert_eq!(
        call(
            &mut service,
            "rules.save",
            Some(&admin),
            json!({
                "source":invalid_candidate,
                "if_revision":invalid["current"]["revision"],
                "recovery_id":recovery_id,
                "recovery_revision":invalid["candidate"]["revision"]
            })
        )
        .unwrap_err()
        .code,
        "RULES_INVALID"
    );

    let missing = call(
        &mut service,
        "rules.get",
        Some(&admin),
        json!({"recovery_id":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}),
    )
    .unwrap_err();
    assert_eq!(missing.code, "RULES_CHANGED");
    assert!(
        !missing
            .message
            .contains(root.path().to_string_lossy().as_ref())
    );

    let oversized_id = "cccccccccccccccccccccccccccccccc";
    std::fs::write(
        root.path()
            .join(format!(".RULES.md.recovery-{oversized_id}")),
        vec![b'x'; 1024 * 1024 + 1],
    )
    .unwrap();
    assert_eq!(
        call(
            &mut service,
            "rules.get",
            Some(&admin),
            json!({"recovery_id":oversized_id})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );

    let project_root = tempfile::tempdir().unwrap();
    let project = call(
        &mut service,
        "project.register",
        Some(&admin),
        json!({"name":"Recovery isolation","root":project_root.path()}),
    )
    .unwrap();
    let project_id = project["id"].as_str().unwrap();
    let project_candidate = valid_rules("post.edit", Some(project_id));
    std::fs::write(
        project_root
            .path()
            .join(format!(".RULES.md.recovery-{recovery_id}")),
        &project_candidate,
    )
    .unwrap();
    let project_snapshot = call(
        &mut service,
        "rules.get",
        Some(&admin),
        json!({"project_id":project_id,"recovery_id":recovery_id}),
    )
    .unwrap();
    assert_eq!(project_snapshot["project_id"], project_id);
    assert_eq!(project_snapshot["candidate"]["source"], project_candidate);
    assert_ne!(project_snapshot["candidate"]["source"], invalid_candidate);
}
