#![cfg(windows)]
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use swarm_adapter::{CredentialBroker, human::HumanBridge};
use swarm_core::{CredentialCandidate, Service, protocol::Request};

fn req(operation: &str, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: None,
        args,
        context: json!({}),
        request_id: None,
    }
}
fn key(c: char) -> String {
    std::iter::repeat_n(c, 64).collect()
}
fn slot(alias: &str) -> String {
    format!("human-{:x}", Sha256::digest(alias.as_bytes()))
}
fn candidate(
    service: &mut Service,
    broker: &mut CredentialBroker,
    alias: &str,
) -> CredentialCandidate {
    broker.credential(service, &slot(alias)).unwrap().unwrap()
}
fn setup(service: &mut Service, broker: &mut CredentialBroker, bridge: &mut HumanBridge) -> Value {
    bridge
        .issue(
            service,
            broker,
            "admin",
            &key('a'),
            req(
                "account.setup",
                json!({"name":"Admin","password":"fixture-password"}),
            ),
        )
        .unwrap()
}
fn assert_public(value: &Value) {
    let text = value.to_string();
    for field in [
        "\"token\"",
        "recovery_code",
        "password",
        "session_token",
        "resume_secret",
    ] {
        assert!(!text.contains(field));
    }
}

#[test]
fn setup_login_persists_across_reopen_and_logout_requires_password_again() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut broker = CredentialBroker::open(&root.path().join("credentials")).unwrap();
    let mut bridge = HumanBridge::open(root.path()).unwrap();
    assert!(service.setup_required().unwrap());
    let public = setup(&mut service, &mut broker, &mut bridge);
    assert_public(&public);
    assert!(!service.setup_required().unwrap());
    let old = candidate(&mut service, &mut broker, "admin");
    drop(bridge);
    drop(broker);
    drop(service);
    let mut service = Service::open(root.path()).unwrap();
    let mut broker = CredentialBroker::open(&root.path().join("credentials")).unwrap();
    let mut bridge = HumanBridge::open(root.path()).unwrap();
    let status = bridge
        .request(
            &mut service,
            &mut broker,
            "admin",
            req("account.status", json!({})),
        )
        .unwrap();
    assert_eq!(status["id"], public["id"]);
    assert_public(&status);
    bridge
        .request(
            &mut service,
            &mut broker,
            "admin",
            req("account.logout", json!({})),
        )
        .unwrap();
    assert!(!service.validate_credential_candidate(&old).unwrap().session);
    assert_eq!(
        bridge
            .request(
                &mut service,
                &mut broker,
                "admin",
                req("account.status", json!({}))
            )
            .unwrap_err()
            .exit_code,
        3
    );
    assert_eq!(
        bridge
            .issue(
                &mut service,
                &mut broker,
                "admin",
                &key('a'),
                req(
                    "account.setup",
                    json!({"name":"Admin","password":"fixture-password"})
                )
            )
            .unwrap_err()
            .exit_code,
        3
    );
    assert_eq!(
        bridge
            .issue(
                &mut service,
                &mut broker,
                "admin",
                &key('b'),
                req("account.login", json!({"name":"Admin","password":"wrong"}))
            )
            .unwrap_err()
            .exit_code,
        3
    );
    let login = bridge
        .issue(
            &mut service,
            &mut broker,
            "admin",
            &key('c'),
            req(
                "account.login",
                json!({"name":"Admin","password":"fixture-password"}),
            ),
        )
        .unwrap();
    assert_eq!(login["id"], public["id"]);
    assert_public(&login);
}

#[test]
fn password_rotation_replays_once_and_keeps_private_recovery_available() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut broker = CredentialBroker::open(&root.path().join("credentials")).unwrap();
    let mut bridge = HumanBridge::open(root.path()).unwrap();
    setup(&mut service, &mut broker, &mut bridge);
    let old = candidate(&mut service, &mut broker, "admin");
    let change = req(
        "account.password",
        json!({"password":"replacement-password","current_password":"fixture-password"}),
    );
    let public = bridge
        .issue(
            &mut service,
            &mut broker,
            "admin",
            &key('b'),
            change.clone(),
        )
        .unwrap();
    let new = candidate(&mut service, &mut broker, "admin");
    assert_ne!(old, new);
    assert!(!service.validate_credential_candidate(&old).unwrap().session);
    assert_public(&public);
    assert_eq!(
        bridge
            .issue(&mut service, &mut broker, "admin", &key('b'), change)
            .unwrap(),
        public
    );
    assert_eq!(candidate(&mut service, &mut broker, "admin"), new);
    if let (
        CredentialCandidate::Human {
            recovery_code: before,
            ..
        },
        CredentialCandidate::Human {
            recovery_code: after,
            ..
        },
    ) = (old, new)
    {
        assert!(before.is_some());
        assert_eq!(before, after);
    } else {
        panic!("expected human");
    }
    assert_eq!(
        bridge
            .issue(
                &mut service,
                &mut broker,
                "admin",
                &key('b'),
                req("account.password", json!({"password":"different"}))
            )
            .unwrap_err()
            .exit_code,
        6
    );
}

#[test]
fn account_alias_binding_and_permissions_survive_account_switching() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut broker = CredentialBroker::open(&root.path().join("credentials")).unwrap();
    let mut bridge = HumanBridge::open(root.path()).unwrap();
    setup(&mut service, &mut broker, &mut bridge);
    let user = bridge
        .request(
            &mut service,
            &mut broker,
            "admin",
            req(
                "account.create",
                json!({"name":"Member","password":"member-password"}),
            ),
        )
        .unwrap();
    assert_public(&user);
    let old = candidate(&mut service, &mut broker, "admin");
    assert!(
        bridge
            .issue(
                &mut service,
                &mut broker,
                "admin",
                &key('b'),
                req(
                    "account.login",
                    json!({"name":"Member","password":"member-password"})
                )
            )
            .is_err()
    );
    assert_eq!(candidate(&mut service, &mut broker, "admin"), old);
    bridge
        .issue(
            &mut service,
            &mut broker,
            "member",
            &key('c'),
            req(
                "account.login",
                json!({"name":"Member","password":"member-password"}),
            ),
        )
        .unwrap();
    let status = bridge
        .request(
            &mut service,
            &mut broker,
            "member",
            req("account.status", json!({})),
        )
        .unwrap();
    assert_eq!(status["id"], user["id"]);
    assert_eq!(
        bridge
            .request(
                &mut service,
                &mut broker,
                "member",
                req("account.list", json!({}))
            )
            .unwrap_err()
            .exit_code,
        4
    );
    let status = bridge
        .request(
            &mut service,
            &mut broker,
            "admin",
            req("account.status", json!({})),
        )
        .unwrap();
    assert_eq!(
        status["id"],
        match old {
            CredentialCandidate::Human { id, .. } => json!(id),
            _ => panic!("expected human"),
        }
    );
}

#[test]
fn recovery_rotates_login_and_old_recovery_without_public_secrets() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut broker = CredentialBroker::open(&root.path().join("credentials")).unwrap();
    let mut bridge = HumanBridge::open(root.path()).unwrap();
    setup(&mut service, &mut broker, &mut bridge);
    let old = candidate(&mut service, &mut broker, "admin");
    let secret = match &old {
        CredentialCandidate::Human {
            recovery_code: Some(code),
            ..
        } => code.clone(),
        _ => panic!("expected recovery"),
    };
    bridge
        .request(
            &mut service,
            &mut broker,
            "admin",
            req("account.logout", json!({})),
        )
        .unwrap();
    let recover = req(
        "account.recover",
        json!({"name":"Admin","recovery_code":secret,"password":"recovered-password"}),
    );
    let public = bridge
        .issue(
            &mut service,
            &mut broker,
            "admin",
            &key('d'),
            recover.clone(),
        )
        .unwrap();
    assert_public(&public);
    assert!(
        !service
            .validate_credential_candidate(&old)
            .unwrap()
            .recovery
    );
    let new = candidate(&mut service, &mut broker, "admin");
    assert_eq!(
        bridge
            .issue(&mut service, &mut broker, "admin", &key('d'), recover)
            .unwrap(),
        public
    );
    assert_eq!(candidate(&mut service, &mut broker, "admin"), new);
}

#[test]
fn receipt_insert_failure_rolls_back_setup_and_allows_same_intent_retry() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut broker = CredentialBroker::open(&root.path().join("credentials")).unwrap();
    let mut bridge = HumanBridge::open(root.path()).unwrap();
    let db = rusqlite::Connection::open(root.path().join("swarm.sqlite3")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_receipt BEFORE INSERT ON app_metadata WHEN NEW.key LIKE 'credential-receipt:%' BEGIN SELECT RAISE(ABORT,'fixture'); END;").unwrap();
    let request = req(
        "account.setup",
        json!({"name":"Admin","password":"fixture-password"}),
    );
    assert_eq!(
        bridge
            .issue(
                &mut service,
                &mut broker,
                "admin",
                &key('a'),
                request.clone()
            )
            .unwrap_err()
            .exit_code,
        8
    );
    assert!(service.setup_required().unwrap());
    db.execute_batch("DROP TRIGGER fail_receipt;").unwrap();
    let public = bridge
        .issue(&mut service, &mut broker, "admin", &key('a'), request)
        .unwrap();
    assert_public(&public);
    assert!(!service.setup_required().unwrap());
}

#[test]
fn external_auth_and_ai_operations_cannot_enter_human_host() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut broker = CredentialBroker::open(&root.path().join("credentials")).unwrap();
    let mut bridge = HumanBridge::open(root.path()).unwrap();
    setup(&mut service, &mut broker, &mut bridge);
    let mut auth = req("board.list", json!({}));
    auth.auth = Some("injected".into());
    assert_eq!(
        bridge
            .request(&mut service, &mut broker, "admin", auth)
            .unwrap_err()
            .exit_code,
        2
    );
    assert!(
        bridge
            .request(
                &mut service,
                &mut broker,
                "admin",
                req("identity.start", json!({}))
            )
            .is_err()
    );
    assert!(
        bridge
            .request(
                &mut service,
                &mut broker,
                "admin",
                req("account.login", json!({}))
            )
            .is_err()
    );
    assert!(
        bridge
            .issue(
                &mut service,
                &mut broker,
                "admin",
                &key('f'),
                req("identity.start", json!({}))
            )
            .is_err()
    );
    assert!(
        bridge
            .issue(
                &mut service,
                &mut broker,
                "admin",
                "bad-key",
                req("account.login", json!({}))
            )
            .is_err()
    );
}

#[test]
fn valid_ai_candidate_is_rejected_even_in_a_human_alias_slot() {
    let root = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut broker = CredentialBroker::open(&root.path().join("credentials")).unwrap();
    let mut bridge = HumanBridge::open(root.path()).unwrap();
    setup(&mut service, &mut broker, &mut bridge);
    let project = bridge
        .request(
            &mut service,
            &mut broker,
            "admin",
            req(
                "project.register",
                json!({"name":"Project","root":project_root.path()}),
            ),
        )
        .unwrap();
    broker
        .issue(
            &mut service,
            &slot("ai"),
            req("identity.start", json!({"project_id":project["id"]})),
        )
        .unwrap();
    let ai = candidate(&mut service, &mut broker, "ai");
    assert!(service.validate_credential_candidate(&ai).unwrap().session);
    assert_eq!(
        bridge
            .request(
                &mut service,
                &mut broker,
                "ai",
                req("account.status", json!({}))
            )
            .unwrap_err()
            .exit_code,
        3
    );
}

#[test]
fn replay_under_different_alias_does_not_copy_human_login() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut broker = CredentialBroker::open(&root.path().join("credentials")).unwrap();
    let mut bridge = HumanBridge::open(root.path()).unwrap();
    setup(&mut service, &mut broker, &mut bridge);
    assert_eq!(
        bridge
            .issue(
                &mut service,
                &mut broker,
                "other",
                &key('a'),
                req(
                    "account.setup",
                    json!({"name":"Admin","password":"fixture-password"})
                )
            )
            .unwrap_err()
            .exit_code,
        6
    );
    assert!(
        broker
            .credential(&mut service, &slot("other"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn private_memo_capacity_failure_preserves_current_password_and_login() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    let mut broker = CredentialBroker::open(&root.path().join("credentials")).unwrap();
    let mut bridge = HumanBridge::open(root.path()).unwrap();
    setup(&mut service, &mut broker, &mut bridge);
    let old = candidate(&mut service, &mut broker, "admin");
    let store =
        swarm_platform::CredentialStore::open(&root.path().join("gui-credentials")).unwrap();
    let original = store.get("human-bridge-memos").unwrap().unwrap();
    let mut entries = serde_json::Map::new();
    for c in ['1', '2', '3', '4', '5'] {
        entries.insert(key(c), json!({"fingerprint":key(c),"candidate":{"id":"i".repeat(4096),"token":"t".repeat(4096),"recovery_code":"r".repeat(4096)}}));
    }
    entries.insert(
        key('6'),
        json!({"fingerprint":key('6'),"candidate":{"id":"i","token":"t","recovery_code":null}}),
    );
    let mut fixture = json!({"version":1,"entries":entries});
    let baseline = serde_json::to_vec(&fixture).unwrap().len();
    let fill = 65_490 - baseline + 1;
    assert!(fill <= 4096);
    fixture["entries"][key('6')]["candidate"]["id"] = json!("i".repeat(fill));
    let encoded = serde_json::to_vec(&fixture).unwrap();
    assert_eq!(encoded.len(), 65_490);
    store.put("human-bridge-memos", &encoded).unwrap();
    let change = req(
        "account.password",
        json!({"password":"replacement-password","current_password":"fixture-password"}),
    );
    assert_eq!(
        bridge
            .issue(
                &mut service,
                &mut broker,
                "admin",
                &key('b'),
                change.clone()
            )
            .unwrap_err()
            .exit_code,
        8
    );
    assert_eq!(store.get("human-bridge-memos").unwrap().unwrap(), encoded);
    assert!(service.validate_credential_candidate(&old).unwrap().session);
    assert_eq!(candidate(&mut service, &mut broker, "admin"), old);
    store.put("human-bridge-memos", &original).unwrap();
    bridge
        .issue(&mut service, &mut broker, "admin", &key('b'), change)
        .unwrap();
    assert!(!service.validate_credential_candidate(&old).unwrap().session);
}
