use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
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
    request_id: Option<&str>,
) -> Result<Value, Fault> {
    service.request(Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: request_id.map(str::to_owned),
    })
}

fn rules(rules: &str) -> String {
    format!(
        "# Registration fixture\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n{rules}access:\n  boards: []\n  threads: []\n~~~\n"
    )
}

fn rule(id: &str, effect: &str, action: &str) -> String {
    format!("  - id: {id}\n    effect: {effect}\n    actions: [{action}]\n")
}

fn register_args(name: &str, root: &std::path::Path) -> Value {
    json!({"name":name,"root":root})
}

struct Fixture {
    data: tempfile::TempDir,
    service: Service,
    admin: String,
    current_project: String,
    current_root: PathBuf,
    ai: String,
}

impl Fixture {
    fn new() -> Self {
        let data = tempfile::tempdir().unwrap();
        let mut service = Service::open(data.path()).unwrap();
        let admin = call(
            &mut service,
            "account.setup",
            None,
            json!({"name":"Admin","password":"project-registration-admin-fixture"}),
            None,
        )
        .unwrap();
        let admin = admin["token"].as_str().unwrap().to_owned();
        let current_root = data.path().join("projects/current");
        fs::create_dir_all(&current_root).unwrap();
        let current = call(
            &mut service,
            "project.register",
            Some(&admin),
            register_args("Admin-registered current project", &current_root),
            None,
        )
        .unwrap();
        let current_project = current["id"].as_str().unwrap().to_owned();
        let identity = call(
            &mut service,
            "identity.start",
            None,
            json!({"project_id":current_project,"model_name":"RegistrationFixture"}),
            None,
        )
        .unwrap();
        let ai = identity["session_token"].as_str().unwrap().to_owned();
        Self {
            data,
            service,
            admin,
            current_project,
            current_root,
            ai,
        }
    }

    fn global_rules(&self, source: &str) {
        fs::write(self.data.path().join("RULES.md"), source).unwrap();
    }

    fn local_rules(&self, source: &str) {
        fs::write(self.current_root.join("RULES.md"), source).unwrap();
    }

    fn register(
        &mut self,
        name: &str,
        root: &std::path::Path,
        request_id: Option<&str>,
    ) -> Result<Value, Fault> {
        call(
            &mut self.service,
            "project.register",
            Some(&self.ai),
            register_args(name, root),
            request_id,
        )
    }

    fn admin_projects(&mut self) -> Value {
        call(
            &mut self.service,
            "project.list",
            Some(&self.admin),
            json!({}),
            None,
        )
        .unwrap()
    }
}

fn new_root(fixture: &Fixture, name: &str) -> PathBuf {
    let path = fixture.data.path().join("projects").join(name);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn first_project_bootstrap_stays_with_admin_and_registration_requires_auth_and_request_id() {
    let mut fixture = Fixture::new();
    let missing_project = call(
        &mut fixture.service,
        "identity.start",
        None,
        json!({"project_id":"not-registered","model_name":"Fixture"}),
        None,
    )
    .unwrap_err();
    assert_eq!(missing_project.code, "PROJECT_NOT_REGISTERED");

    let target = new_root(&fixture, "auth-target");
    fixture.global_rules(&rules(&rule("register", "allow", "project.register")));
    let unauthenticated = call(
        &mut fixture.service,
        "project.register",
        None,
        register_args("Unauthenticated", &target),
        Some("no-auth-request"),
    )
    .unwrap_err();
    assert_eq!(unauthenticated.code, "AUTHENTICATION_REQUIRED");

    let missing_request_id = fixture.register("Missing key", &target, None).unwrap_err();
    assert_eq!(missing_request_id.code, "INVALID_INPUT");

    for forged in [
        json!({"name":"Forged actor","root":target,"actor_id":"someone-else"}),
        json!({"name":"Forged project","root":target,"project_id":"another-project"}),
    ] {
        let error = call(
            &mut fixture.service,
            "project.register",
            Some(&fixture.ai),
            forged,
            Some("forged-request"),
        )
        .unwrap_err();
        assert_eq!(error.code, "INVALID_INPUT");
    }
    assert_eq!(
        fixture.admin_projects()["items"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn global_allow_is_required_and_current_local_policy_can_only_narrow_it() {
    let mut fixture = Fixture::new();
    let target = new_root(&fixture, "policy-target");
    fixture.global_rules(&rules(""));
    fixture.local_rules(&rules(&rule("local-only", "allow", "project.register")));
    let local_only = fixture
        .register("Local cannot grant", &target, Some("local-only"))
        .unwrap_err();
    assert_eq!(local_only.code, "PERMISSION_DENIED");

    fixture.global_rules(&rules(&rule("global-deny", "deny", "project.register")));
    fixture.local_rules(&rules(&rule("local-allow", "allow", "project.register")));
    let global_deny = fixture
        .register("Global deny wins", &target, Some("global-deny"))
        .unwrap_err();
    assert_eq!(global_deny.code, "PERMISSION_DENIED");

    fixture.global_rules(&rules(&rule("global-allow", "allow", "project.register")));
    fixture.local_rules(&rules(&rule("local-deny", "deny", "project.register")));
    let local_deny = fixture
        .register("Local deny narrows", &target, Some("local-deny"))
        .unwrap_err();
    assert_eq!(local_deny.code, "PERMISSION_DENIED");

    fixture.global_rules(&rules(""));
    fixture.local_rules(&rules(&rule(
        "local-allow-again",
        "allow",
        "project.register",
    )));
    let no_global_grant = fixture
        .register("No global allow", &target, Some("no-global-grant"))
        .unwrap_err();
    assert_eq!(no_global_grant.code, "PERMISSION_DENIED");
    assert_eq!(
        fixture.admin_projects()["items"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn successful_registration_returns_only_new_project_and_does_not_renew_or_write_rules() {
    let mut fixture = Fixture::new();
    let target = new_root(&fixture, "success-target");
    fixture.global_rules(&rules(&rule("register", "allow", "project.register")));
    let global_before = fs::read(fixture.data.path().join("RULES.md")).unwrap();
    let local_source = rules(&rule("local-allow", "allow", "project.register"));
    fixture.local_rules(&local_source);
    let local_before = fs::read(fixture.current_root.join("RULES.md")).unwrap();
    let status_before = call(
        &mut fixture.service,
        "identity.status",
        Some(&fixture.ai),
        json!({}),
        None,
    )
    .unwrap();

    let result = fixture
        .register("New project", &target, Some("new-project-receipt"))
        .unwrap();
    assert_eq!(
        result
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        ["canonical_root", "id", "name"]
    );
    assert_eq!(result["name"], "New project");
    assert_eq!(
        result["canonical_root"],
        target.canonicalize().unwrap().to_str().unwrap()
    );
    assert_ne!(result["id"], fixture.current_project);
    assert_eq!(
        fs::read(fixture.data.path().join("RULES.md")).unwrap(),
        global_before
    );
    assert_eq!(
        fs::read(fixture.current_root.join("RULES.md")).unwrap(),
        local_before
    );
    assert!(!target.join("RULES.md").exists());
    let status_after = call(
        &mut fixture.service,
        "identity.status",
        Some(&fixture.ai),
        json!({}),
        None,
    )
    .unwrap();
    assert_eq!(status_after["expires_at"], status_before["expires_at"]);
    assert_eq!(status_after["project_id"], fixture.current_project);
}

#[test]
fn exact_receipt_replays_but_changed_payload_cross_operation_and_revoked_policy_do_not() {
    let mut fixture = Fixture::new();
    let target = new_root(&fixture, "receipt-target");
    let source = format!(
        "{}{}{}",
        rule("register", "allow", "project.register"),
        rule("board-create", "allow", "board.create"),
        rule("board-read", "allow", "board.get")
    );
    fixture.global_rules(&rules(&source));
    let first = fixture
        .register("Receipt project", &target, Some("shared-request-key"))
        .unwrap();
    let replay = fixture
        .register("Receipt project", &target, Some("shared-request-key"))
        .unwrap();
    assert_eq!(first, replay);

    let changed_payload = fixture
        .register("Changed name", &target, Some("shared-request-key"))
        .unwrap_err();
    assert_eq!(changed_payload.code, "REQUEST_CONFLICT");

    let reused_for_board = call(
        &mut fixture.service,
        "board.create",
        Some(&fixture.ai),
        json!({"name":"Different operation"}),
        Some("shared-request-key"),
    )
    .unwrap_err();
    assert_eq!(reused_for_board.code, "REQUEST_CONFLICT");

    let board_first = call(
        &mut fixture.service,
        "board.create",
        Some(&fixture.ai),
        json!({"name":"Board first"}),
        Some("board-first-key"),
    )
    .unwrap();
    assert!(board_first["id"].is_string());
    let reused_for_project = fixture
        .register(
            "Cross operation",
            &new_root(&fixture, "cross-op-target"),
            Some("board-first-key"),
        )
        .unwrap_err();
    assert_eq!(reused_for_project.code, "REQUEST_CONFLICT");

    fixture.global_rules(&rules(&rule("deny-register", "deny", "project.register")));
    let after_revocation = fixture
        .register("Receipt project", &target, Some("shared-request-key"))
        .unwrap_err();
    assert_eq!(after_revocation.code, "PERMISSION_DENIED");
    assert_eq!(
        fixture.admin_projects()["items"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn exact_receipt_replays_after_root_is_removed_and_changed_raw_input_conflicts() {
    let mut fixture = Fixture::new();
    fixture.global_rules(&rules(&rule("register", "allow", "project.register")));
    let target = new_root(&fixture, "removed-receipt-target");
    let first = fixture
        .register("Removed target", &target, Some("removed-target-receipt"))
        .unwrap();

    fs::remove_dir_all(&target).unwrap();
    let replay = fixture
        .register("Removed target", &target, Some("removed-target-receipt"))
        .unwrap();
    assert_eq!(replay, first);

    // Receipt lookup must compare the original request before touching the now
    // missing filesystem path. Reusing the ID with changed raw input conflicts.
    let changed_raw_root = target.join(".");
    let changed = fixture
        .register(
            "Removed target",
            &changed_raw_root,
            Some("removed-target-receipt"),
        )
        .unwrap_err();
    assert_eq!(changed.code, "REQUEST_CONFLICT");
}

#[test]
fn receipt_conflicts_after_admin_changes_canonical_root_without_revealing_it() {
    let mut fixture = Fixture::new();
    fixture.global_rules(&rules(&rule("register", "allow", "project.register")));
    let original = new_root(&fixture, "original-receipt-root");
    let first = fixture
        .register("Movable receipt", &original, Some("moved-root-receipt"))
        .unwrap();
    let moved = new_root(&fixture, "admin-moved-root");

    call(
        &mut fixture.service,
        "project.root",
        Some(&fixture.admin),
        json!({"project_id":first["id"],"path":moved}),
        None,
    )
    .unwrap();

    let retry = fixture
        .register("Movable receipt", &original, Some("moved-root-receipt"))
        .unwrap_err();
    assert_eq!(retry.code, "PROJECT_PATH_CONFLICT");
    assert!(!retry.message.contains(moved.to_string_lossy().as_ref()));
    assert!(!retry.message.contains(first["id"].as_str().unwrap()));
}

#[test]
fn canonical_root_and_private_alias_conflicts_disclose_no_existing_project() {
    let mut fixture = Fixture::new();
    fixture.global_rules(&rules(&rule("register", "allow", "project.register")));
    let secret_root = new_root(&fixture, "private-project-root");
    let secret = call(
        &mut fixture.service,
        "project.register",
        Some(&fixture.admin),
        register_args("Private project name", &secret_root),
        None,
    )
    .unwrap();
    let alias_root = new_root(&fixture, "private-project-alias");
    call(
        &mut fixture.service,
        "project.alias",
        Some(&fixture.admin),
        json!({"project_id":secret["id"],"path":alias_root}),
        None,
    )
    .unwrap();

    for (index, path) in [secret_root.join("."), alias_root].into_iter().enumerate() {
        let error = fixture
            .register(
                "Attempt duplicate",
                &path,
                Some(&format!("duplicate-{index}")),
            )
            .unwrap_err();
        assert_eq!(error.code, "PROJECT_PATH_CONFLICT");
        assert!(!error.message.contains("Private project name"));
        assert!(!error.message.contains(secret["id"].as_str().unwrap()));
        assert!(!error.message.contains(secret_root.to_str().unwrap()));
    }
    assert_eq!(
        fixture.admin_projects()["items"].as_array().unwrap().len(),
        2
    );
}

#[test]
fn concurrent_registration_of_one_canonical_root_has_one_winner() {
    let mut fixture = Fixture::new();
    fixture.global_rules(&rules(&rule("register", "allow", "project.register")));
    let target = new_root(&fixture, "concurrent-target");
    let root = fixture.data.path().to_owned();
    let auth = fixture.ai.clone();
    let barrier = Arc::new(Barrier::new(2));
    let attempts = ["concurrent-one", "concurrent-two"].map(|request_id| {
        let root = root.clone();
        let path = target.clone();
        let auth = auth.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            let mut service = Service::open(&root).unwrap();
            barrier.wait();
            call(
                &mut service,
                "project.register",
                Some(&auth),
                register_args("Concurrent project", &path),
                Some(request_id),
            )
        })
    });
    let results = attempts.map(|attempt| attempt.join().unwrap());
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    let errors = results
        .iter()
        .filter_map(|result| result.as_ref().err())
        .collect::<Vec<_>>();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, "PROJECT_PATH_CONFLICT");
    assert_eq!(
        fixture.admin_projects()["items"].as_array().unwrap().len(),
        2
    );
}
