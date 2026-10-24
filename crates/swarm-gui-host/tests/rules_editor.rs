#![cfg(windows)]
use serde_json::{Value, json};
use sha2::Digest;
use std::{
    path::Path,
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
use swarm_gui_host::GuiHost;

fn request(operation: &str, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: None,
        args,
        context: json!({}),
        request_id: None,
    }
}

fn call(
    service: &mut Service,
    token: Option<&str>,
    operation: &str,
    args: Value,
) -> Result<Value, swarm_core::protocol::Fault> {
    let mut request = request(operation, args);
    request.auth = token.map(str::to_owned);
    service.request(request)
}

fn ready(root: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    for endpoint in [
        swarm_runtime::pipe_name(root).unwrap(),
        swarm_runtime::gui_pipe_name(root).unwrap(),
    ] {
        loop {
            if let Ok(pipe) = swarm_platform::pipe::connect(&endpoint) {
                drop(pipe);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "named-pipe server did not become ready"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn host_rules_editor_saves_project_policy_and_policy_effect_is_live() {
    let data = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"rules-host-admin-fixture"}),
    )
    .unwrap();
    let admin_token = admin["token"].as_str().unwrap();
    let project = call(
        &mut service,
        Some(admin_token),
        "project.register",
        json!({"name":"Rules host project","root":project_root.path()}),
    )
    .unwrap();
    let board = call(
        &mut service,
        Some(admin_token),
        "board.create",
        json!({"name":"Rules host board","description":""}),
    )
    .unwrap();
    let thread = call(
        &mut service,
        Some(admin_token),
        "thread.create",
        json!({"board_id":board["id"],"name":"Rules host thread","description":""}),
    )
    .unwrap();
    let member = call(
        &mut service,
        Some(admin_token),
        "account.create",
        json!({"name":"RulesEditor","password":"rules-host-member-fixture"}),
    )
    .unwrap();
    for operation in ["rules.get", "rules.check", "rules.validate", "rules.save"] {
        call(
            &mut service,
            Some(admin_token),
            "permission.set",
            json!({"subject_type":"account","subject_id":member["id"],"operation":operation,"scope":"global","target_id":"","decision":"allow"}),
        ).unwrap();
    }
    let ai = call(
        &mut service,
        None,
        "identity.start",
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    let ai_token = ai["session_token"].as_str().unwrap().to_owned();
    let blocked = call(
        &mut service,
        Some(&ai_token),
        "post.create",
        json!({"thread_id":thread["id"],"body":"should be denied before policy save"}),
    )
    .unwrap_err();
    assert_eq!(blocked.code, "PERMISSION_DENIED");
    drop(service);

    let root = data.path().to_owned();
    // The in-process server uses a finite listener budget; remaining GUI slots
    // are drained after the assertions so the joined server exits cleanly.
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&root, Some(1), Some(64)).unwrap()
    });
    ready(data.path());
    let mut host = GuiHost::open(data.path()).unwrap();
    host.authenticate("login", "RulesEditor", "rules-host-member-fixture")
        .unwrap();

    let hidden = host.read("rules.capabilities", json!({})).unwrap();
    assert_eq!(hidden["global"]["save"], true);
    assert_eq!(hidden["global"]["check"], true);
    assert_eq!(hidden["projects"], json!([]));
    let denied_project = host
        .read("rules.get", json!({"project_id":project["id"]}))
        .unwrap_err();
    assert_eq!(denied_project.code, "PERMISSION_DENIED");
    assert!(!denied_project.message.contains("Rules host project"));

    // The editor can use the registered ID after project.get is granted; the UI
    // never supplies a filesystem path.
    let mut admin_service = Service::open(data.path()).unwrap();
    for operation in ["project.list", "project.get"] {
        call(
            &mut admin_service,
            Some(admin_token),
            "permission.set",
            json!({"subject_type":"account","subject_id":member["id"],"operation":operation,"scope":"global","target_id":"","decision":"allow"}),
        ).unwrap();
    }
    let visible = host.read("rules.capabilities", json!({})).unwrap();
    assert_eq!(visible["projects"][0]["id"], project["id"]);
    assert_eq!(visible["projects"][0]["check"], true);
    let subjects = host
        .read("rules.subjects", json!({"project_id":project["id"]}))
        .unwrap();
    assert_eq!(subjects["items"][0]["id"], ai["identity"]["id"]);
    assert!(subjects["items"][0].get("token").is_none());
    assert!(subjects["items"][0].get("root").is_none());
    let initial = host
        .read("rules.get", json!({"project_id":project["id"]}))
        .unwrap();
    assert_eq!(initial["source"], "");
    let source = format!(
        "# exact comment preserved\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: host-live-test\n    effect: allow\n    subjects:\n      projects: [{}]\n    actions: [post.create]\naccess:\n  boards: []\n  threads: []\n~~~\n",
        project["id"].as_str().unwrap()
    );
    let checked = host
        .manage_rules(
            "rules.validate",
            json!({"project_id":project["id"],"source":source}),
        )
        .unwrap();
    assert_eq!(checked["valid"], true);
    assert_eq!(checked["actions"], json!(["post.create"]));
    let saved = host
        .manage_rules(
            "rules.save",
            json!({"project_id":project["id"],"source":source,"if_revision":initial["revision"]}),
        )
        .unwrap();
    assert_eq!(saved["source"], source);
    assert_eq!(
        std::fs::read_to_string(project_root.path().join("RULES.md")).unwrap(),
        source
    );
    assert_eq!(
        host.check_rules(
            json!({
                "project_id":project["id"],
                "actor_id":ai["identity"]["id"],
                "operation":"post.create",
                "target":{"thread_id":thread["id"]}
            }),
            json!({})
        )
        .unwrap(),
        json!({"allowed":true,"reason":"allowed"})
    );
    assert_eq!(
        host.check_rules(
            json!({
                "project_id":project["id"],
                "actor_id":"forged-ai-identity",
                "operation":"post.create",
                "target":{"thread_id":thread["id"]}
            }),
            json!({})
        )
        .unwrap_err()
        .code,
        "NOT_FOUND"
    );
    assert_eq!(
        host.check_rules(
            json!({
                "project_id":project["id"],
                "actor_id":ai["identity"]["id"],
                "operation":"post.create",
                "target":{"thread_id":thread["id"]}
            }),
            json!([])
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );

    let mut after_save = Service::open(data.path()).unwrap();
    let allowed = call(
        &mut after_save,
        Some(&ai_token),
        "post.create",
        json!({"thread_id":thread["id"],"body":"allowed by saved project rule"}),
    )
    .unwrap();
    assert_eq!(allowed["no"], 1);
    for _ in 0..64 {
        if host.read("rules.capabilities", json!({})).is_err() {
            break;
        }
    }
    drop(host);
    server.join().unwrap();
}

#[test]
fn host_rules_check_refuses_to_run_while_credential_issuance_is_pending() {
    let root = tempfile::tempdir().unwrap();
    let state = root.path().join("gui-state");
    let store = swarm_platform::CredentialStore::open(&state).unwrap();
    let alias = format!("account-{:x}", sha2::Sha256::digest(b"PendingAccount"));
    let pending = json!({
        "version":1,
        "intent":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "alias":alias,
        "request":{
            "operation":"account.login",
            "auth":null,
            "args":{"name":"PendingAccount","password":"pending-secret"},
            "context":{},
            "request_id":null
        }
    });
    store
        .put("pending-auth", &serde_json::to_vec(&pending).unwrap())
        .unwrap();
    let mut host = GuiHost::open(root.path()).unwrap();
    let error = host
        .check_rules(
            json!({"project_id":"project","actor_id":"actor","operation":"post.get","target":{"thread_id":"thread","no":1}}),
            json!({}),
        )
        .unwrap_err();
    assert_eq!(error.code, "CONFLICT");
    assert!(!error.to_string().contains("pending-secret"));
}

#[test]
fn host_recovery_compares_corrupt_rules_and_applies_exact_copy_over_private_pipe() {
    let data = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    call(
        &mut service,
        None,
        "account.setup",
        json!({"name":"Admin","password":"recovery-host-fixture"}),
    )
    .unwrap();
    let target = data.path().join("RULES.md");
    let source = std::fs::read_to_string(&target).unwrap();
    let recovery_id = "abcdef0123456789abcdef0123456789";
    let copy = data
        .path()
        .join(format!(".RULES.md.recovery-{recovery_id}"));
    std::fs::write(&copy, &source).unwrap();
    std::fs::write(&target, [0xff, 0xfe]).unwrap();
    drop(service);
    let root = data.path().to_owned();
    let server = std::thread::spawn(move || {
        swarm_runtime::serve_with_gui(&root, Some(1), Some(24)).unwrap()
    });
    ready(data.path());
    let mut host = GuiHost::open(data.path()).unwrap();
    host.authenticate("login", "Admin", "recovery-host-fixture")
        .unwrap();
    assert_eq!(
        host.manage_rules(
            "rules.recovery.inspect",
            json!({"recovery_id":"../RULES.md"})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );
    let compared = host
        .manage_rules("rules.recovery.inspect", json!({"recovery_id":recovery_id}))
        .unwrap();
    assert_eq!(compared["current"]["utf8"], false);
    assert!(compared["current"]["source"].is_null());
    assert_eq!(compared["candidate"]["source"], source);
    assert_eq!(compared["candidate"]["valid"], true);
    let args = json!({"recovery_id":recovery_id,"recovery_revision":compared["candidate"]["revision"],"source":source,"if_revision":compared["current"]["revision"]});
    std::fs::write(&target, "external edit").unwrap();
    assert_eq!(
        host.manage_rules("rules.recovery.apply", args)
            .unwrap_err()
            .code,
        "RULES_CHANGED"
    );
    let refreshed = host
        .manage_rules("rules.recovery.inspect", json!({"recovery_id":recovery_id}))
        .unwrap();
    let restored = host.manage_rules("rules.recovery.apply", json!({"recovery_id":recovery_id,"recovery_revision":refreshed["candidate"]["revision"],"source":source,"if_revision":refreshed["current"]["revision"]})).unwrap();
    assert_eq!(restored["source"], source);
    assert_eq!(restored["revision"], refreshed["candidate"]["revision"]);
    assert_eq!(std::fs::read_to_string(target).unwrap(), source);
    assert_eq!(std::fs::read_to_string(copy).unwrap(), source);
    for _ in 0..24 {
        if host.read("rules.capabilities", json!({})).is_err() {
            break;
        }
    }
    drop(host);
    server.join().unwrap();
}
