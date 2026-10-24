/// Build-time public manifest locator; never accept a renderer-supplied endpoint.
const MANIFEST_URL: &str = match option_env!("SWARM_UPDATE_MANIFEST_URL") {
    Some(url) => url,
    None => "https://example.com/swarm/latest.json",
};

pub(crate) fn parse_manifest(
    value: serde_json::Value,
    current: &tauri::utils::PackageInfo,
) -> Result<Option<(String, String)>, String> {
    let version = value["version"].as_str().ok_or("Missing update version")?;
    // Use the same SemVer type/parser as Tauri's installed package identity.
    let mut latest = current.clone();
    latest.version = version.parse().map_err(|_| "Invalid update version")?;
    if latest.version <= current.version || !latest.version.pre.is_empty() {
        return Ok(None);
    }
    let download = swarm_gui_host::normalize_update_url(
        value["windows_x64_url"]
            .as_str()
            .ok_or("Missing Windows update URL")?,
    )?;
    Ok(Some((latest.version.to_string(), download)))
}

pub(crate) fn check(
    current: &tauri::utils::PackageInfo,
) -> Result<Option<(String, String)>, String> {
    parse_manifest(
        swarm_gui_host::fetch_update_manifest(MANIFEST_URL)?,
        current,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn only_newer_stable_versions_offer_an_https_windows_download() {
        let current = tauri::utils::PackageInfo {
            name: "Swarm".into(),
            version: "0.9.0".parse().unwrap(),
            authors: "",
            description: "",
            crate_name: "swarm-desktop",
        };
        assert!(
            parse_manifest(
                json!({"version":"0.10.0","windows_x64_url":"https://example.com/setup.exe"}),
                &current
            )
            .unwrap()
            .is_some()
        );
        for version in ["0.9.0", "0.8.0", "1.0.0-beta.1"] {
            assert!(
                parse_manifest(json!({"version":version}), &current)
                    .unwrap()
                    .is_none()
            );
        }
        for url in [
            "http://example.com/setup.exe",
            "file:///setup.exe",
            "https://user:secret@example.com/setup.exe",
        ] {
            assert!(
                parse_manifest(json!({"version":"1.0.0","windows_x64_url":url}), &current).is_err()
            );
        }
        assert!(parse_manifest(json!({"version":"broken"}), &current).is_err());
        assert!(parse_manifest(json!({"version":"1.0.0+build-with-hyphen","windows_x64_url":"https://example.com/setup.exe"}), &current).unwrap().is_some());
    }
}
