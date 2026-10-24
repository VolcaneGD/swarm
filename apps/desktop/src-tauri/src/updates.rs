use serde_json::Value;
use tauri_plugin_updater::{Update, UpdaterExt};

pub(crate) const MANIFEST_URL: &str =
    "https://github.com/VolcaneGD/swarm/releases/latest/download/latest.json";

/// Download locators are never accepted from the renderer or another repository.
pub(crate) fn validate_manifest(
    value: &Value,
    current: &tauri::utils::PackageInfo,
) -> Result<bool, String> {
    let version = value["version"].as_str().ok_or("Missing update version")?;
    let mut latest = current.clone();
    latest.version = version.parse().map_err(|_| "Invalid update version")?;
    if latest.version <= current.version || !latest.version.pre.is_empty() {
        return Ok(false);
    }
    if latest.version.to_string() != version {
        return Err("Invalid update version".into());
    }
    let platform = &value["platforms"]["windows-x86_64"];
    let url = platform["url"]
        .as_str()
        .ok_or("Missing Windows update URL")?;
    let expected = format!(
        "https://github.com/VolcaneGD/swarm/releases/download/v{version}/Swarm_{version}_x64-setup.exe"
    );
    if url != expected {
        return Err("Unexpected Windows update source".into());
    }
    let signature = platform["signature"]
        .as_str()
        .ok_or("Missing update signature")?;
    if signature.is_empty() || signature.len() > 2048 {
        return Err("Invalid update signature".into());
    }
    if value["notes"].as_str().unwrap_or_default().len() > 8192 {
        return Err("Update notes too large".into());
    }
    Ok(true)
}

pub(crate) async fn check(app: &tauri::AppHandle) -> Result<Option<Update>, String> {
    let value = tauri::async_runtime::spawn_blocking(|| {
        swarm_gui_host::fetch_update_manifest(MANIFEST_URL)
    })
    .await
    .map_err(|_| "Update check failed")??;
    if !validate_manifest(&value, app.package_info())? {
        return Ok(None);
    }
    // restore_update requires a signature binding the bytes to the announced version.
    let updater = app
        .updater_builder()
        .timeout(std::time::Duration::from_secs(300))
        .version_comparator(|current, release| {
            release.version > current && release.version.pre.is_empty()
        })
        .build()
        .map_err(|_| "Could not prepare updater")?;
    let mut update = updater
        .restore_update(value)
        .map_err(|_| "Invalid update manifest")?;
    if let Some(update) = &mut update {
        update.timeout = Some(std::time::Duration::from_secs(300));
    }
    Ok(update)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn native_updater_rejects_corruption_wrong_key_and_version_relabeling() {
        // Public test fixture; private test key is not included or needed.
        const PUBLIC_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDI2ODZFRDEzRTFBRTQ4RkIKUldUN1NLN2hFKzJHSmx2RnVYb3dpYVBpdWkzVW1abENKMWkwRjlwOTlmRWtoQUxodEVWR0tnbE8K";
        const SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUN1NLN2hFKzJHSmxDaVNBMEw0VlNrV0hsVjVPV05VSU0vRmNmeWVHd0MrS2J1akQzWFVCZ2dmdGJTSERCK3RXbU5qOGxlaStHV29vSm5jM2dmcDdhT1lrYzlKRTJ1K3djPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkyODUwMTYxCWZpbGU6cGF5bG9hZC5iaW4JdmVyc2lvbjo5LjkuOQpVYVpUMkVtWFA1QktuNjBEU2FVTVRVWm5wNmJodlM0THJCdlJjVitaOTBHN2J1aksxR0J1a2FzdjloNXBCZlBabDdEZ1V1dEY3aVhtd1JqQWZJeEJDQT09Cg==";
        const PAYLOAD: &[u8] = b"MZ Swarm updater signature regression fixture\n";
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context.config_mut().plugins.0.insert(
            "updater".into(),
            json!({
                "pubkey":PUBLIC_KEY, "endpoints":[MANIFEST_URL], "requireSignedVersion":true
            }),
        );
        let app = tauri::test::mock_builder()
            .plugin(
                tauri_plugin_updater::Builder::new()
                    .pubkey(PUBLIC_KEY)
                    .build(),
            )
            .build(context)
            .unwrap();
        let updater = app
            .updater_builder()
            .endpoints(vec![MANIFEST_URL.parse().unwrap()])
            .unwrap()
            .build()
            .unwrap();
        let mut value = json!({"version":"9.9.9","platforms":{"windows-x86_64":{
            "url":"https://github.com/VolcaneGD/swarm/releases/download/v9.9.9/Swarm_9.9.9_x64-setup.exe", "signature":SIGNATURE}}});
        let update = updater.restore_update(value.clone()).unwrap().unwrap();
        update.verify(PAYLOAD).unwrap();
        assert!(update.verify(b"tampered").is_err());
        value["version"] = json!("10.0.0");
        assert!(
            updater
                .restore_update(value)
                .unwrap()
                .unwrap()
                .verify(PAYLOAD)
                .is_err()
        );
        let wrong = app
            .updater_builder()
            .pubkey("invalid key")
            .endpoints(vec![MANIFEST_URL.parse().unwrap()])
            .unwrap()
            .build()
            .unwrap();
        assert!(
            wrong
                .restore_update(update.raw_json.clone())
                .unwrap()
                .unwrap()
                .verify(PAYLOAD)
                .is_err()
        );
    }
    fn current() -> tauri::utils::PackageInfo {
        tauri::utils::PackageInfo {
            name: "Swarm".into(),
            version: "0.1.0".parse().unwrap(),
            authors: "",
            description: "",
            crate_name: "swarm-desktop",
        }
    }
    #[test]
    #[ignore = "Explicit local artifact verification; set SWARM_UPDATE_ASSET_DIR"]
    fn verifies_prepared_release_assets_without_installing() {
        let directory = std::path::PathBuf::from(
            std::env::var_os("SWARM_UPDATE_ASSET_DIR").expect("asset directory"),
        );
        let value: Value =
            serde_json::from_slice(&std::fs::read(directory.join("latest.json")).unwrap()).unwrap();
        let mut installed = current();
        installed.version = "0.0.0".parse().unwrap();
        assert!(validate_manifest(&value, &installed).unwrap());
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let mut context = tauri::test::mock_context(tauri::test::noop_assets());
        context
            .config_mut()
            .plugins
            .0
            .insert("updater".into(), config["plugins"]["updater"].clone());
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(context)
            .unwrap();
        // Verification of the current release artifact, not a production downgrade.
        let updater = app
            .updater_builder()
            .version_comparator(|_, _| true)
            .build()
            .unwrap();
        let update = updater.restore_update(value).unwrap().unwrap();
        let filename = format!("Swarm_{}_x64-setup.exe", update.version);
        let mut bytes = std::fs::read(directory.join(&filename)).unwrap();
        let signature = std::fs::read_to_string(directory.join(format!("{filename}.sig"))).unwrap();
        assert_eq!(signature.trim(), update.signature);
        update.verify(&bytes).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        assert!(update.verify(&bytes).is_err());
    }
    fn manifest() -> Value {
        json!({"version":"0.2.0","notes":"New version", "platforms":{"windows-x86_64":{
            "url":"https://github.com/VolcaneGD/swarm/releases/download/v0.2.0/Swarm_0.2.0_x64-setup.exe","signature":"signed-artifact"}}})
    }
    #[test]
    fn stable_upgrade_requires_our_exact_versioned_release_and_signature() {
        let mut value = manifest();
        assert!(validate_manifest(&value, &current()).unwrap());
        for version in ["0.1.0", "0.0.9", "0.3.0-beta.1"] {
            value["version"] = json!(version);
            assert!(!validate_manifest(&value, &current()).unwrap());
        }
        for url in [
            "https://example.com/setup.exe",
            "http://github.com/VolcaneGD/swarm/releases/download/v0.2.0/Swarm_0.2.0_x64-setup.exe",
            "https://github.com/another/swarm/releases/download/v0.2.0/Swarm_0.2.0_x64-setup.exe",
            "https://github.com/VolcaneGD/swarm/releases/download/v0.1.0/Swarm_0.1.0_x64-setup.exe",
        ] {
            value = manifest();
            value["platforms"]["windows-x86_64"]["url"] = json!(url);
            assert!(validate_manifest(&value, &current()).is_err());
        }
        value = manifest();
        value["platforms"]["windows-x86_64"]["signature"] = json!("");
        assert!(validate_manifest(&value, &current()).is_err());
        value = manifest();
        value["version"] = json!("broken");
        assert!(validate_manifest(&value, &current()).is_err());
    }
}
