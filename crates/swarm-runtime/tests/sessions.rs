#![cfg(windows)]
use serde_json::{Value, json};
use swarm_core::{CredentialCandidate, Service, protocol::Request};
use swarm_platform::CredentialStore;
use swarm_runtime::session::SessionBridge;
fn request(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}
fn fixture() -> (tempfile::TempDir, tempfile::TempDir, Service, String) {
    let root = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = service
        .request(request(
            "account.setup",
            None,
            json!({"name":"Admin","password":"fixture-password"}),
        ))
        .unwrap();
    let project = service
        .request(request(
            "project.register",
            admin["token"].as_str(),
            json!({"name":"Project","root":project_root.path()}),
        ))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    (root, project_root, service, project)
}
fn candidate(root: &std::path::Path, alias: &str) -> CredentialCandidate {
    serde_json::from_slice(
        &CredentialStore::open(&root.join("client-credentials"))
            .unwrap()
            .get(alias)
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
fn resume_args(candidate: &CredentialCandidate) -> Value {
    match candidate {
        CredentialCandidate::Ai {
            id,
            project_id,
            resume_secret,
            ..
        } => {
            json!({"alias":"agent","actor_id":id,"project_id":project_id,"model_name":"Fixture","resume_secret":resume_secret})
        }
        _ => panic!("expected AI"),
    }
}
#[test]
fn committed_start_replays_after_restart_without_creating_a_second_identity() {
    let (root, _project, mut service, project) = fixture();
    let start = request(
        "session.start",
        Some(&"a".repeat(64)),
        json!({"alias":"agent","project_id":project,"model_name":"Fixture"}),
    );
    let mut bridge = SessionBridge::open(root.path(), &mut service).unwrap();
    let first = bridge.dispatch(&mut service, start.clone()).unwrap();
    let private = candidate(root.path(), "agent");
    assert!(first.get("session_token").is_none());
    assert!(first.get("resume_secret").is_none());
    drop(bridge);
    drop(service);
    let mut service = Service::open(root.path()).unwrap();
    let mut bridge = SessionBridge::open(root.path(), &mut service).unwrap();
    assert_eq!(bridge.dispatch(&mut service, start.clone()).unwrap(), first);
    assert_eq!(candidate(root.path(), "agent"), private);
    let mut changed = start.clone();
    changed.args["model_name"] = json!("Different");
    assert_eq!(
        bridge
            .dispatch(&mut service, changed)
            .unwrap_err()
            .exit_code,
        6
    );
    let mut changed_alias = start.clone();
    changed_alias.args["alias"] = json!("other");
    assert_eq!(
        bridge
            .dispatch(&mut service, changed_alias)
            .unwrap_err()
            .exit_code,
        6
    );
    assert!(
        CredentialStore::open(&root.path().join("client-credentials"))
            .unwrap()
            .get("other")
            .unwrap()
            .is_none()
    );
    let mut duplicate = start;
    duplicate.auth = Some("b".repeat(64));
    assert_eq!(
        bridge
            .dispatch(&mut service, duplicate)
            .unwrap_err()
            .exit_code,
        6
    );
}
#[test]
fn client_save_failure_after_resume_commit_is_recoverable_with_the_same_intent() {
    let (root, _project, mut service, project) = fixture();
    let mut bridge = SessionBridge::open(root.path(), &mut service).unwrap();
    bridge
        .dispatch(
            &mut service,
            request(
                "session.start",
                Some(&"a".repeat(64)),
                json!({"alias":"agent","project_id":project,"model_name":"Fixture"}),
            ),
        )
        .unwrap();
    let old = candidate(root.path(), "agent");
    let client_file = std::fs::read_dir(root.path().join("client-credentials"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "bin"))
        .unwrap();
    let alias = root.path().join("external-link");
    std::fs::hard_link(&client_file, &alias).unwrap();
    let resume = request("session.resume", Some(&"b".repeat(64)), resume_args(&old));
    assert_eq!(
        bridge
            .dispatch(&mut service, resume.clone())
            .unwrap_err()
            .exit_code,
        8
    );
    assert!(!service.validate_credential_candidate(&old).unwrap().session);
    std::fs::remove_file(alias).unwrap();
    drop(bridge);
    drop(service);
    let mut service = Service::open(root.path()).unwrap();
    let mut bridge = SessionBridge::open(root.path(), &mut service).unwrap();
    let public = bridge.dispatch(&mut service, resume.clone()).unwrap();
    let new = candidate(root.path(), "agent");
    assert_ne!(new, old);
    assert!(service.validate_credential_candidate(&new).unwrap().session);
    assert_eq!(bridge.dispatch(&mut service, resume).unwrap(), public);
    assert_eq!(candidate(root.path(), "agent"), new);
}
#[test]
fn replay_of_a_released_grant_does_not_reactivate_it_and_bad_resume_keeps_old_login() {
    let (root, _project, mut service, project) = fixture();
    let mut bridge = SessionBridge::open(root.path(), &mut service).unwrap();
    let start = request(
        "session.start",
        Some(&"a".repeat(64)),
        json!({"alias":"agent","project_id":project,"model_name":"Fixture"}),
    );
    bridge.dispatch(&mut service, start.clone()).unwrap();
    let old = candidate(root.path(), "agent");
    let mut args = resume_args(&old);
    args["resume_secret"] = json!("wrong-secret");
    assert_eq!(
        bridge
            .dispatch(
                &mut service,
                request("session.resume", Some(&"b".repeat(64)), args)
            )
            .unwrap_err()
            .exit_code,
        3
    );
    assert_eq!(candidate(root.path(), "agent"), old);
    assert!(service.validate_credential_candidate(&old).unwrap().session);
    if let CredentialCandidate::Ai { session_token, .. } = old {
        service
            .request(request("identity.release", Some(&session_token), json!({})))
            .unwrap();
    }
    assert_eq!(
        bridge.dispatch(&mut service, start).unwrap_err().exit_code,
        3
    );
}

#[test]
fn unknown_model_can_start_and_resume_without_a_model_declaration() {
    let (root, _project, mut service, project) = fixture();
    let mut bridge = SessionBridge::open(root.path(), &mut service).unwrap();
    let public = bridge
        .dispatch(
            &mut service,
            request(
                "session.start",
                Some(&"c".repeat(64)),
                json!({"alias":"agent","project_id":project}),
            ),
        )
        .unwrap();
    assert_eq!(public["identity"]["model_name"], "不明");
    let old = candidate(root.path(), "agent");
    let mut args = resume_args(&old);
    args.as_object_mut().unwrap().remove("model_name");
    let resumed = bridge
        .dispatch(
            &mut service,
            request("session.resume", Some(&"d".repeat(64)), args),
        )
        .unwrap();
    assert_eq!(public["identity"]["id"], resumed["identity"]["id"]);
    assert!(!service.validate_credential_candidate(&old).unwrap().session);
}

#[test]
fn missing_private_memo_does_not_reissue_a_committed_grant() {
    use sha2::{Digest, Sha256};
    let (root, _project, mut service, project) = fixture();
    let mut bridge = SessionBridge::open(root.path(), &mut service).unwrap();
    let intent = "e".repeat(64);
    let start = request(
        "session.start",
        Some(&intent),
        json!({"alias":"agent","project_id":project,"model_name":"Fixture"}),
    );
    bridge.dispatch(&mut service, start.clone()).unwrap();
    let old = candidate(root.path(), "agent");
    let key = format!("issuance-{:x}", Sha256::digest(intent.as_bytes()));
    CredentialStore::open(&root.path().join("credentials"))
        .unwrap()
        .remove(&key)
        .unwrap();
    assert_eq!(
        bridge.dispatch(&mut service, start).unwrap_err().exit_code,
        8
    );
    assert_eq!(candidate(root.path(), "agent"), old);
    assert!(service.validate_credential_candidate(&old).unwrap().session);
}

#[test]
fn private_gui_bootstrap_and_saved_login_do_not_enable_ai_account_management() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut bridge = SessionBridge::open(root.path(), &mut service).unwrap();
    let setup = request(
        "account.setup",
        None,
        json!({"name":"Admin","password":"fixture-password"}),
    );
    assert!(service.setup_required().unwrap());
    let denied = swarm_runtime::dispatch(&mut service, setup.clone());
    assert!(!denied.ok);
    assert_eq!(denied.error.unwrap().exit_code, 4);
    let public = bridge
        .gui_issue(&mut service, "admin", &"a".repeat(64), setup)
        .unwrap();
    assert!(public.get("token").is_none());
    assert!(public.get("recovery_code").is_none());
    assert!(!service.setup_required().unwrap());
    drop(bridge);
    drop(service);
    let mut service = Service::open(root.path()).unwrap();
    let mut bridge = SessionBridge::open(root.path(), &mut service).unwrap();
    let status = bridge
        .gui_request(
            &mut service,
            "admin",
            request("account.status", None, json!({})),
        )
        .unwrap();
    assert_eq!(status["id"], public["id"]);
    assert!(!swarm_runtime::dispatch(&mut service, request("account.status", None, json!({}))).ok);
}
