#![cfg(windows)]

use serde_json::{Value, json};
use std::{
    path::Path,
    process::{Command, Output},
    time::{Duration, Instant},
};
use swarm_core::{CredentialCandidate, Service, protocol::Request};

fn request(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}

fn rules(projects: &[&str], allow_registration: bool) -> String {
    let rule = if allow_registration {
        let ids = projects
            .iter()
            .map(|id| format!("        - \"{id}\"\n"))
            .collect::<String>();
        format!(
            "rules:\n  - id: register-projects\n    effect: allow\n    subjects:\n      projects:\n{ids}    actions: [project.register]\n"
        )
    } else {
        "rules: []\n".to_owned()
    };
    format!(
        "~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\n{rule}access:\n  boards: []\n  threads: []\n~~~\n"
    )
}

fn local_allow(project_id: &str) -> String {
    format!(
        "~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: local-only\n    effect: allow\n    subjects:\n      projects: [\"{project_id}\"]\n    actions: [project.register]\naccess:\n  boards: []\n  threads: []\n~~~\n"
    )
}

fn run_register(
    data: &Path,
    alias: &str,
    name: &str,
    root: &Path,
    request_id: &str,
    extra: &[&str],
) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swarm"))
        .arg("--data-dir")
        .arg(data)
        .args([
            "--session",
            alias,
            "--format",
            "json",
            "--name",
            name,
            "--root",
        ])
        .arg(root)
        .args(["--request-id", request_id])
        .args(extra)
        .args(["project", "register"])
        .output()
        .unwrap()
}

fn run_raw(data: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swarm"))
        .arg("--data-dir")
        .arg(data)
        .args(args)
        .output()
        .unwrap()
}

fn envelope(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "stdout: {}; stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn wait_for_pipe(root: &Path) {
    let name = swarm_runtime::pipe_name(root).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(connection) = swarm_platform::pipe::connect(&name) {
            drop(connection);
            return;
        }
        assert!(
            Instant::now() < deadline,
            "runtime Named Pipe did not become ready"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn real_cli_registers_only_with_global_allow_and_hides_other_projects() {
    let data = tempfile::tempdir().unwrap();
    let project_a = tempfile::tempdir().unwrap();
    let project_b = tempfile::tempdir().unwrap();
    let target_a = tempfile::tempdir().unwrap();
    let target_b = tempfile::tempdir().unwrap();
    let target_c = tempfile::tempdir().unwrap();
    let missing_root = data.path().join("missing-registration-root");

    let mut service = Service::open(data.path()).unwrap();
    let admin = service
        .request(request(
            "account.setup",
            None,
            json!({"name":"Admin","password":"fixture-password"}),
        ))
        .unwrap();
    let first_project = service
        .request(request(
            "project.register",
            admin["token"].as_str(),
            json!({"name":"Project A","root":project_a.path()}),
        ))
        .unwrap();
    let second_project = service
        .request(request(
            "project.register",
            admin["token"].as_str(),
            json!({"name":"Project B","root":project_b.path()}),
        ))
        .unwrap();
    let first_ai = service
        .request(request(
            "identity.start",
            None,
            json!({"project_id":first_project["id"],"model_name":"Fixture One"}),
        ))
        .unwrap();
    let second_ai = service
        .request(request(
            "identity.start",
            None,
            json!({"project_id":second_project["id"],"model_name":"Fixture Two"}),
        ))
        .unwrap();
    let credentials =
        swarm_platform::CredentialStore::open(&data.path().join("client-credentials")).unwrap();
    for (alias, grant) in [("first", &first_ai), ("second", &second_ai)] {
        let private = CredentialCandidate::from_grant("identity.start", grant).unwrap();
        credentials
            .put(alias, &serde_json::to_vec(&private).unwrap())
            .unwrap();
    }
    let human = CredentialCandidate::from_grant("account.setup", &admin).unwrap();
    credentials
        .put("human", &serde_json::to_vec(&human).unwrap())
        .unwrap();
    let first_id = first_project["id"].as_str().unwrap().to_owned();
    let second_id = second_project["id"].as_str().unwrap().to_owned();
    std::fs::write(
        data.path().join("RULES.md"),
        rules(&[&first_id, &second_id], true),
    )
    .unwrap();
    drop(service);

    let server_root = data.path().to_owned();
    let server = std::thread::spawn(move || swarm_runtime::serve(&server_root, Some(9)).unwrap());
    wait_for_pipe(data.path());

    let first = run_register(
        data.path(),
        "first",
        "Registered A",
        target_a.path(),
        "register-a",
        &[],
    );
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let first_response = envelope(&first);
    let registered = &first_response["data"];
    assert_eq!(first_response["ok"], true);
    assert_eq!(registered["name"], "Registered A");
    let canonical_a = std::fs::canonicalize(target_a.path()).unwrap();
    assert_eq!(registered["canonical_root"], canonical_a.to_str().unwrap());
    assert!(registered["id"].as_str().is_some());
    let public_keys = registered.as_object().unwrap().keys().collect::<Vec<_>>();
    assert_eq!(public_keys.len(), 3);
    assert!(
        public_keys
            .iter()
            .all(|key| ["id", "name", "canonical_root", "created_at"].contains(&key.as_str()))
    );
    assert!(!String::from_utf8_lossy(&first.stdout).contains("Project B"));

    let same_retry = run_register(
        data.path(),
        "first",
        "Registered A",
        target_a.path(),
        "register-a",
        &[],
    );
    assert!(
        same_retry.status.success(),
        "{}",
        String::from_utf8_lossy(&same_retry.stderr)
    );
    assert_eq!(envelope(&same_retry)["data"]["id"], registered["id"]);

    let changed_retry = run_register(
        data.path(),
        "first",
        "Changed",
        target_b.path(),
        "register-a",
        &[],
    );
    assert_eq!(changed_retry.status.code(), Some(6));
    assert_eq!(
        envelope(&changed_retry)["error"]["code"],
        "REQUEST_CONFLICT"
    );
    assert!(
        !String::from_utf8_lossy(&changed_retry.stdout)
            .contains(registered["id"].as_str().unwrap())
    );
    assert!(
        !String::from_utf8_lossy(&changed_retry.stderr).contains(target_a.path().to_str().unwrap())
    );

    let other_actor_duplicate = run_register(
        data.path(),
        "second",
        "Hidden project",
        target_a.path(),
        "register-b",
        &[],
    );
    assert_eq!(other_actor_duplicate.status.code(), Some(6));
    let duplicate_response = envelope(&other_actor_duplicate);
    assert_eq!(duplicate_response["error"]["code"], "PROJECT_PATH_CONFLICT");
    for hidden in [
        registered["id"].as_str().unwrap(),
        "Registered A",
        target_a.path().to_str().unwrap(),
    ] {
        assert!(!String::from_utf8_lossy(&other_actor_duplicate.stdout).contains(hidden));
        assert!(!String::from_utf8_lossy(&other_actor_duplicate.stderr).contains(hidden));
    }

    // A local allow cannot grant authority to register a new project.
    std::fs::write(data.path().join("RULES.md"), rules(&[], false)).unwrap();
    std::fs::write(project_a.path().join("RULES.md"), local_allow(&first_id)).unwrap();
    let local_only = run_register(
        data.path(),
        "first",
        "Local grant",
        target_b.path(),
        "local-only",
        &[],
    );
    assert_eq!(local_only.status.code(), Some(4));
    assert_eq!(envelope(&local_only)["error"]["code"], "PERMISSION_DENIED");
    assert!(
        !String::from_utf8_lossy(&local_only.stdout).contains(target_b.path().to_str().unwrap())
    );

    std::fs::remove_file(project_a.path().join("RULES.md")).unwrap();
    let default_deny = run_register(
        data.path(),
        "first",
        "Denied",
        target_c.path(),
        "default-deny",
        &[],
    );
    assert_eq!(default_deny.status.code(), Some(4));
    assert_eq!(
        envelope(&default_deny)["error"]["code"],
        "PERMISSION_DENIED"
    );

    std::fs::write(
        data.path().join("RULES.md"),
        rules(&[&first_id, &second_id], true),
    )
    .unwrap();
    let long_name = run_register(
        data.path(),
        "first",
        &"N".repeat(513),
        target_c.path(),
        "long-name",
        &[],
    );
    assert_eq!(long_name.status.code(), Some(2));
    assert_eq!(envelope(&long_name)["error"]["code"], "INVALID_INPUT");
    let invalid_root = run_register(
        data.path(),
        "first",
        "Missing",
        &missing_root,
        "missing-root",
        &[],
    );
    assert_eq!(invalid_root.status.code(), Some(2));
    assert_eq!(envelope(&invalid_root)["error"]["code"], "INVALID_INPUT");
    server.join().unwrap();

    let no_session = run_raw(
        data.path(),
        &[
            "--format",
            "json",
            "--name",
            "No bootstrap",
            "--root",
            target_c.path().to_str().unwrap(),
            "--request-id",
            "no-session",
            "project",
            "register",
        ],
    );
    assert_eq!(no_session.status.code(), Some(3));
    assert_eq!(
        envelope(&no_session)["error"]["code"],
        "AUTHENTICATION_REQUIRED"
    );
    let human_session = run_register(
        data.path(),
        "human",
        "No Admin CLI registration",
        target_c.path(),
        "human-session",
        &[],
    );
    assert_eq!(human_session.status.code(), Some(3));
    assert_eq!(
        envelope(&human_session)["error"]["code"],
        "AUTHENTICATION_REQUIRED"
    );

    for extra in [["--alias", "other"], ["--path", "C:\\elsewhere"]] {
        let invalid = run_register(
            data.path(),
            "first",
            "Invalid",
            target_c.path(),
            "invalid-option",
            &extra,
        );
        assert_eq!(invalid.status.code(), Some(2));
        assert_eq!(envelope(&invalid)["error"]["code"], "INVALID_INPUT");
    }
    for action in ["alias", "root"] {
        let invalid = run_raw(
            data.path(),
            &["--session", "first", "--format", "json", "project", action],
        );
        assert_eq!(invalid.status.code(), Some(2));
        assert_eq!(envelope(&invalid)["error"]["code"], "INVALID_INPUT");
    }
}
