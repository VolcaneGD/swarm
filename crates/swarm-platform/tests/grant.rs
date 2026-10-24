#![cfg(windows)]
use serde_json::{Value, json};
use swarm_core::{
    Service,
    protocol::{Fault, Request},
};
use swarm_platform::CredentialStore;

fn request(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}
fn persist(store: &CredentialStore, grant: &Value) -> Result<(), Fault> {
    let bytes = serde_json::to_vec(grant).map_err(|_| {
        Fault::new(
            "CREDENTIAL_STORAGE_FAILED",
            8,
            "Private credential storage failed",
        )
    })?;
    store.put("human-bootstrap", &bytes).map_err(|_| {
        Fault::new(
            "CREDENTIAL_STORAGE_FAILED",
            8,
            "Private credential storage failed",
        )
    })
}
#[test]
fn bootstrap_persists_with_real_dpapi_and_returns_only_public_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let secrets = temp.path().join("credentials");
    let store = CredentialStore::open(&secrets).unwrap();
    let mut service = Service::open(&data).unwrap();
    let public = service
        .request_with_credential_sink(
            request(
                "account.setup",
                None,
                json!({"name":"Admin","password":"fixture-password"}),
            ),
            |grant| persist(&store, grant),
        )
        .unwrap();
    assert!(public.get("token").is_none());
    assert!(public.get("recovery_code").is_none());
    drop(service);
    drop(store);
    let reopened_store = CredentialStore::open(&secrets).unwrap();
    let private: Value =
        serde_json::from_slice(&reopened_store.get("human-bootstrap").unwrap().unwrap()).unwrap();
    let mut reopened = Service::open(&data).unwrap();
    let status = reopened
        .request(request(
            "account.status",
            private["token"].as_str(),
            json!({}),
        ))
        .unwrap();
    assert_eq!(status["id"], public["id"]);
}
#[test]
fn real_store_rejection_rolls_back_bootstrap_without_replacing_saved_credential() {
    let temp = tempfile::tempdir().unwrap();
    let store = CredentialStore::open(&temp.path().join("credentials")).unwrap();
    store.put("human-bootstrap", b"prior-fixture").unwrap();
    let mut service = Service::open(&temp.path().join("data")).unwrap();
    let oversized_name = "A".repeat(64 * 1024);
    let error = service
        .request_with_credential_sink(
            request(
                "account.setup",
                None,
                json!({"name":oversized_name,"password":"fixture-password"}),
            ),
            |grant| persist(&store, grant),
        )
        .unwrap_err();
    assert_eq!(error.code, "CREDENTIAL_STORAGE_FAILED");
    assert!(
        store
            .get("human-bootstrap")
            .unwrap()
            .as_deref()
            .is_some_and(|bytes| bytes == b"prior-fixture")
    );
    assert!(
        service
            .request_with_credential_sink(
                request(
                    "account.setup",
                    None,
                    json!({"name":"Admin","password":"fixture-password"})
                ),
                |grant| persist(&store, grant)
            )
            .is_ok()
    );
}
