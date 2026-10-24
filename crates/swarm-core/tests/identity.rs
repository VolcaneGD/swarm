use serde_json::{Value, json};
#[test]
fn concurrent_resume_has_one_winner_and_renews_the_deadline() {
    let (data, mut service, admin) = setup();
    let project_dir = tempfile::tempdir().unwrap();
    let project = register(&mut service, &admin, project_dir.path());
    let initial = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    let old_deadline = (chrono::Utc::now() + chrono::Duration::hours(1))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let db = rusqlite::Connection::open(data.path().join("swarm.sqlite3")).unwrap();
    db.execute(
        "UPDATE ai_identities SET expires_at=?1 WHERE id=?2",
        rusqlite::params![old_deadline, initial["identity"]["id"].as_str().unwrap()],
    )
    .unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
    let args = json!({"project_id":project["id"],"actor_id":initial["identity"]["id"],"resume_secret":initial["resume_secret"]});
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let root = data.path().to_path_buf();
            let barrier = barrier.clone();
            let args = args.clone();
            std::thread::spawn(move || {
                let mut service = Service::open(&root).unwrap();
                barrier.wait();
                call(&mut service, "identity.resume", None, args)
            })
        })
        .collect();
    let outcomes: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().unwrap())
        .collect();
    assert_eq!(outcomes.iter().filter(|r| r.is_ok()).count(), 1);
    for failure in outcomes.iter().filter_map(|r| r.as_ref().err()) {
        assert_eq!(failure.exit_code, 3);
    }
    let winner = outcomes.into_iter().find_map(Result::ok).unwrap();
    let expires =
        chrono::DateTime::parse_from_rfc3339(winner["identity"]["expires_at"].as_str().unwrap())
            .unwrap();
    assert!(expires > chrono::Utc::now() + chrono::Duration::days(6));
    assert_eq!(
        call(
            &mut service,
            "identity.status",
            initial["session_token"].as_str(),
            json!({})
        )
        .unwrap_err()
        .exit_code,
        3
    );
    assert!(
        call(
            &mut service,
            "identity.status",
            winner["session_token"].as_str(),
            json!({})
        )
        .is_ok()
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM ai_sessions WHERE actor_id=?1",
            [initial["identity"]["id"].as_str().unwrap()],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
#[test]
fn canonical_root_changes_are_explicit_and_cross_project_aliases_are_rejected() {
    let (_data, mut service, admin) = setup();
    let root = tempfile::tempdir().unwrap();
    let replacement = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let project = register(&mut service, &admin, root.path());
    let second = register(&mut service, &admin, other.path());
    let id = project["id"].as_str().unwrap();
    assert_eq!(
        call(
            &mut service,
            "project.alias",
            Some(&admin),
            json!({"project_id":id,"path":other.path()})
        )
        .unwrap_err()
        .exit_code,
        6
    );
    let moved = call(
        &mut service,
        "project.root",
        Some(&admin),
        json!({"project_id":id,"path":replacement.path()}),
    )
    .unwrap();
    assert_eq!(moved["id"], project["id"]);
    assert_ne!(moved["canonical_root"], project["canonical_root"]);
    assert_eq!(moved["paths"].as_array().unwrap().len(), 2);
    assert_eq!(
        call(
            &mut service,
            "project.get",
            Some(&admin),
            json!({"id":second["id"]})
        )
        .unwrap()["canonical_root"],
        second["canonical_root"]
    );
}
#[test]
fn ai_credentials_survive_reopen_without_plaintext_storage() {
    let (data, mut service, admin) = setup();
    let project_dir = tempfile::tempdir().unwrap();
    let project = register(&mut service, &admin, project_dir.path());
    let initial = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    drop(service);
    let mut reopened = Service::open(data.path()).unwrap();
    assert_eq!(
        call(
            &mut reopened,
            "identity.status",
            initial["session_token"].as_str(),
            json!({})
        )
        .unwrap()["id"],
        initial["identity"]["id"]
    );
    let db = rusqlite::Connection::open(data.path().join("swarm.sqlite3")).unwrap();
    let token_hash: String = db
        .query_row("SELECT token_hash FROM ai_sessions", [], |row| row.get(0))
        .unwrap();
    let resume_hash: String = db
        .query_row("SELECT resume_hash FROM ai_identities", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_ne!(token_hash, initial["session_token"]);
    assert_ne!(resume_hash, initial["resume_secret"]);
    assert_eq!(token_hash.len(), 64);
    assert_eq!(resume_hash.len(), 64);
}
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
fn setup() -> (tempfile::TempDir, Service, String) {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let admin = call(
        &mut service,
        "account.setup",
        None,
        json!({"name":"Admin","password":"correct horse battery staple"}),
    )
    .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    (root, service, admin)
}
fn register(service: &mut Service, admin: &str, path: &std::path::Path) -> Value {
    call(
        service,
        "project.register",
        Some(admin),
        json!({"name":"Project","root":path}),
    )
    .unwrap()
}
#[test]
fn project_registration_is_persistent_and_aliases_do_not_switch_the_canonical_root() {
    let (data, mut service, admin) = setup();
    let canonical = tempfile::tempdir().unwrap();
    let alias = tempfile::tempdir().unwrap();
    let project = register(&mut service, &admin, canonical.path());
    let id = project["id"].as_str().unwrap();
    call(
        &mut service,
        "project.alias",
        Some(&admin),
        json!({"project_id":id,"path":alias.path()}),
    )
    .unwrap();
    drop(service);
    let mut reopened = Service::open(data.path()).unwrap();
    let fetched = call(&mut reopened, "project.get", Some(&admin), json!({"id":id})).unwrap();
    assert_eq!(fetched["canonical_root"], project["canonical_root"]);
    assert_eq!(fetched["paths"].as_array().unwrap().len(), 2);
    assert_eq!(register(&mut reopened, &admin, canonical.path())["id"], id);
}
#[test]
fn new_sessions_and_projects_have_distinct_ai_ids_and_unknown_models_are_explicit() {
    let (_data, mut service, admin) = setup();
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let pa = register(&mut service, &admin, a.path());
    let pb = register(&mut service, &admin, b.path());
    let one = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":pa["id"]}),
    )
    .unwrap();
    let two = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":pa["id"]}),
    )
    .unwrap();
    let three = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":pb["id"]}),
    )
    .unwrap();
    assert_ne!(one["identity"]["id"], two["identity"]["id"]);
    assert_ne!(one["identity"]["display_id"], two["identity"]["display_id"]);
    assert_ne!(one["identity"]["id"], three["identity"]["id"]);
    assert_eq!(one["identity"]["model_name"], "不明");
    assert_eq!(one["identity"]["model_source"], "unknown");
    assert_eq!(
        call(
            &mut service,
            "identity.start",
            None,
            json!({"project_id":"unregistered"})
        )
        .unwrap_err()
        .exit_code,
        5
    );
}
#[test]
fn resume_retains_id_rotates_credentials_and_revokes_the_old_session() {
    let (_data, mut service, admin) = setup();
    let project_dir = tempfile::tempdir().unwrap();
    let project = register(&mut service, &admin, project_dir.path());
    let initial = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"],"model_name":"TestModel"}),
    )
    .unwrap();
    let token = initial["session_token"].as_str().unwrap();
    let resumed=call(&mut service,"identity.resume",None,json!({"project_id":project["id"],"actor_id":initial["identity"]["id"],"model_name":"TestModel","resume_secret":initial["resume_secret"]})).unwrap();
    assert_eq!(initial["identity"]["id"], resumed["identity"]["id"]);
    assert_ne!(initial["session_token"], resumed["session_token"]);
    assert_ne!(initial["resume_secret"], resumed["resume_secret"]);
    assert_eq!(
        call(&mut service, "identity.status", Some(token), json!({}))
            .unwrap_err()
            .exit_code,
        3
    );
    let status = call(
        &mut service,
        "identity.status",
        resumed["session_token"].as_str(),
        json!({}),
    )
    .unwrap();
    assert_eq!(status["id"], initial["identity"]["id"]);
    assert!(status.get("session_token").is_none());
    assert!(status.get("resume_secret").is_none());
    assert!(call(&mut service,"identity.resume",None,json!({"project_id":project["id"],"actor_id":initial["identity"]["id"],"model_name":"TestModel","resume_secret":initial["resume_secret"]})).is_err());
}
#[test]
fn wrong_resume_project_model_or_secret_preserves_the_active_session() {
    let (_data, mut service, admin) = setup();
    let project_dir = tempfile::tempdir().unwrap();
    let project = register(&mut service, &admin, project_dir.path());
    let initial = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"],"model_name":"TestModel"}),
    )
    .unwrap();
    let args = json!({"project_id":project["id"],"actor_id":initial["identity"]["id"],"model_name":"TestModel","resume_secret":initial["resume_secret"]});
    for (field, value) in [
        ("project_id", json!("other")),
        ("model_name", json!("OtherModel")),
        ("resume_secret", json!("wrong")),
    ] {
        let mut wrong = args.clone();
        wrong[field] = value;
        assert_eq!(
            call(&mut service, "identity.resume", None, wrong)
                .unwrap_err()
                .exit_code,
            3
        );
        assert!(
            call(
                &mut service,
                "identity.status",
                initial["session_token"].as_str(),
                json!({})
            )
            .is_ok()
        );
    }
}
#[test]
fn name_supplementation_and_status_do_not_renew_the_retention_deadline() {
    let (_data, mut service, admin) = setup();
    let project_dir = tempfile::tempdir().unwrap();
    let project = register(&mut service, &admin, project_dir.path());
    let initial = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    let token = initial["session_token"].as_str().unwrap();
    let named = call(
        &mut service,
        "identity.model",
        Some(token),
        json!({"model_name":"TestModel"}),
    )
    .unwrap();
    assert_eq!(named["id"], initial["identity"]["id"]);
    assert_eq!(named["expires_at"], initial["identity"]["expires_at"]);
    assert_eq!(named["model_source"], "declared");
    let status = call(&mut service, "identity.status", Some(token), json!({})).unwrap();
    assert_eq!(status["expires_at"], initial["identity"]["expires_at"]);
    assert_eq!(
        call(
            &mut service,
            "identity.model",
            Some(token),
            json!({"model_name":"DifferentModel"})
        )
        .unwrap_err()
        .exit_code,
        6
    );
}
#[test]
fn expired_ids_cannot_be_authenticated_or_resumed() {
    let (data, mut service, admin) = setup();
    let project_dir = tempfile::tempdir().unwrap();
    let project = register(&mut service, &admin, project_dir.path());
    let initial = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    let db = rusqlite::Connection::open(data.path().join("swarm.sqlite3")).unwrap();
    db.execute(
        "UPDATE ai_identities SET expires_at='2000-01-01T00:00:00.000Z' WHERE id=?1",
        [initial["identity"]["id"].as_str().unwrap()],
    )
    .unwrap();
    assert_eq!(
        call(
            &mut service,
            "identity.status",
            initial["session_token"].as_str(),
            json!({})
        )
        .unwrap_err()
        .exit_code,
        3
    );
    assert_eq!(call(&mut service,"identity.resume",None,json!({"project_id":project["id"],"actor_id":initial["identity"]["id"],"resume_secret":initial["resume_secret"]})).unwrap_err().exit_code,3);
    let new = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    assert_ne!(initial["identity"]["id"], new["identity"]["id"]);
}
#[test]
fn releasing_a_session_revokes_access_but_allows_explicit_resume() {
    let (_data, mut service, admin) = setup();
    let project_dir = tempfile::tempdir().unwrap();
    let project = register(&mut service, &admin, project_dir.path());
    let initial = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    call(
        &mut service,
        "identity.release",
        initial["session_token"].as_str(),
        json!({}),
    )
    .unwrap();
    assert!(
        call(
            &mut service,
            "identity.status",
            initial["session_token"].as_str(),
            json!({})
        )
        .is_err()
    );
    assert!(call(&mut service,"identity.resume",None,json!({"project_id":project["id"],"actor_id":initial["identity"]["id"],"resume_secret":initial["resume_secret"]})).is_ok());
}
#[test]
fn ai_tokens_cannot_be_used_for_admin_operations() {
    let (_data, mut service, admin) = setup();
    let project_dir = tempfile::tempdir().unwrap();
    let project = register(&mut service, &admin, project_dir.path());
    let initial = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    let token = initial["session_token"].as_str();
    for (operation, args) in [
        ("board.create", json!({"name":"Denied"})),
        (
            "project.register",
            json!({"name":"Other","root":project_dir.path()}),
        ),
        ("account.logout", json!({})),
    ] {
        assert_eq!(
            call(&mut service, operation, token, args)
                .unwrap_err()
                .exit_code,
            4
        );
    }
    let status = call(&mut service, "identity.status", token, json!({})).unwrap();
    assert!(call(&mut service, "board.list", token, json!({})).is_ok());
    assert_eq!(status["expires_at"], initial["identity"]["expires_at"]);
    assert!(call(&mut service, "board.list", Some(&admin), json!({})).is_ok());
}

#[test]
fn client_report_provenance_binds_project_and_preserves_identity_deadline() {
    let (_data, mut service, admin) = setup();
    let project_dir = tempfile::tempdir().unwrap();
    let foreign = tempfile::tempdir().unwrap();
    let project = register(&mut service, &admin, project_dir.path());
    let report = json!({"client":"claude-code","session_id":"session","project_root":project_dir.path(),"model_name":"Reported","basis":"response"});
    let initial = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"]}),
    )
    .unwrap();
    let token = initial["session_token"].as_str();
    let named = call(
        &mut service,
        "identity.model",
        token,
        json!({"model_report":report,"model_name":"Fallback"}),
    )
    .unwrap();
    assert_eq!(named["id"], initial["identity"]["id"]);
    assert_eq!(named["expires_at"], initial["identity"]["expires_at"]);
    assert_eq!(named["model_name"], "Reported");
    assert_eq!(named["model_source"], "automatic");
    let same = call(
        &mut service,
        "identity.model",
        token,
        json!({"model_name":"Reported"}),
    )
    .unwrap();
    assert_eq!(same["model_source"], "automatic");
    let mut wrong = report.clone();
    wrong["project_root"] = json!(foreign.path());
    assert_eq!(
        call(
            &mut service,
            "identity.start",
            None,
            json!({"project_id":project["id"],"model_report":wrong})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );
    let mut changed = report.clone();
    changed["model_name"] = json!("Changed");
    assert_eq!(
        call(
            &mut service,
            "identity.model",
            token,
            json!({"model_report":changed})
        )
        .unwrap_err()
        .code,
        "MODEL_CHANGED"
    );
    let mut synthetic = report.clone();
    synthetic["model_name"] = json!("<synthetic>");
    assert_eq!(
        call(
            &mut service,
            "identity.start",
            None,
            json!({"project_id":project["id"],"model_report":synthetic})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );
    let mut selected = report.clone();
    selected["client"] = json!("codex");
    selected["basis"] = json!("selection");
    assert_eq!(
        call(
            &mut service,
            "identity.start",
            None,
            json!({"project_id":project["id"],"model_report":selected})
        )
        .unwrap()["identity"]["model_source"],
        "automatic"
    );
    for patch in [
        json!({"basis":"invented"}),
        json!({"extra":true}),
        json!({"session_id":""}),
    ] {
        let mut invalid = report.clone();
        invalid
            .as_object_mut()
            .unwrap()
            .extend(patch.as_object().unwrap().clone());
        assert_eq!(
            call(
                &mut service,
                "identity.start",
                None,
                json!({"project_id":project["id"],"model_report":invalid})
            )
            .unwrap_err()
            .code,
            "INVALID_INPUT"
        );
    }
}

#[test]
fn unknown_local_model_label_can_be_supplemented_without_renewal() {
    let (_data, mut service, admin) = setup();
    let dir = tempfile::tempdir().unwrap();
    let project = register(&mut service, &admin, dir.path());
    let initial = call(
        &mut service,
        "identity.start",
        None,
        json!({"project_id":project["id"],"model_local":true}),
    )
    .unwrap();
    assert_eq!(initial["identity"]["model_name"], "ローカルモデル");
    assert_eq!(initial["identity"]["model_source"], "unknown");
    let named = call(
        &mut service,
        "identity.model",
        initial["session_token"].as_str(),
        json!({"model_name":"Named","model_local":true}),
    )
    .unwrap();
    assert_eq!(named["model_name"], "Named");
    assert_eq!(named["id"], initial["identity"]["id"]);
    assert_eq!(named["expires_at"], initial["identity"]["expires_at"]);
    let resumed = call(&mut service,"identity.resume",None,json!({"actor_id":initial["identity"]["id"],"project_id":project["id"],"model_name":"Named","model_local":true,"resume_secret":initial["resume_secret"]})).unwrap();
    assert_eq!(resumed["identity"]["id"], initial["identity"]["id"]);
    assert_eq!(
        call(
            &mut service,
            "identity.start",
            None,
            json!({"project_id":project["id"],"model_local":"true"})
        )
        .unwrap_err()
        .code,
        "INVALID_INPUT"
    );
}
