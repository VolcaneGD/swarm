use std::{io::Read, time::Duration};
const MAX_MANIFEST_BYTES: u64 = 16 * 1024;

pub fn normalize_update_url(raw: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(raw).map_err(|_| "Invalid update URL")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Update URLs must use HTTPS without credentials".into());
    }
    Ok(url.to_string())
}

/// Public metadata only; no private host state or credentials are sent.
pub fn fetch_update_manifest(endpoint: &str) -> Result<serde_json::Value, String> {
    let endpoint = normalize_update_url(endpoint)?;
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let url = attempt.url();
            if attempt.previous().len() >= 4
                || url.scheme() != "https"
                || !matches!(
                    url.host_str(),
                    Some(
                        "github.com"
                            | "release-assets.githubusercontent.com"
                            | "objects.githubusercontent.com"
                    )
                )
            {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .user_agent("Swarm-update-check")
        .build()
        .map_err(|_| "Could not prepare update check")?;
    let response = client
        .get(endpoint)
        .send()
        .map_err(|_| "Could not check updates")?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("UPDATE_UNPUBLISHED".into());
    }
    let response = response
        .error_for_status()
        .map_err(|_| "Update server unavailable")?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MANIFEST_BYTES)
    {
        return Err("Update manifest too large".into());
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read update manifest")?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("Update manifest too large".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid update manifest".into())
}

impl crate::GuiHost {
    /// Protected receipt is staged only after the updater verifies signed bytes.
    pub fn stage_update_receipt(
        &self,
        from: &str,
        version: &str,
        notes: &str,
    ) -> Result<(), swarm_core::protocol::Fault> {
        if from.len() > 128
            || version.is_empty()
            || version.len() > 128
            || notes.len() > 8192
            || from == version
        {
            return Err(crate::invalid());
        }
        let value =
            serde_json::to_vec(&serde_json::json!({"from":from,"version":version,"notes":notes}))
                .map_err(|_| crate::storage())?;
        self.store
            .put("update-receipt", &value)
            .map_err(|_| crate::storage())
    }
    pub fn consume_update_notes(
        &self,
        installed_version: &str,
    ) -> Result<Option<serde_json::Value>, swarm_core::protocol::Fault> {
        let Some(bytes) = self
            .store
            .get("update-receipt")
            .map_err(|_| crate::storage())?
        else {
            return Ok(None);
        };
        let value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| crate::storage())?;
        if value["version"].as_str() != Some(installed_version)
            || value["from"].as_str() == Some(installed_version)
        {
            return Ok(None);
        }
        self.store
            .remove("update-receipt")
            .map_err(|_| crate::storage())?;
        Ok(Some(
            serde_json::json!({"version":installed_version,"notes":value["notes"]}),
        ))
    }
}
