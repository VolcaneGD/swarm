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
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("Swarm-update-check")
        .build()
        .map_err(|_| "Could not prepare update check")?;
    let response = client
        .get(endpoint)
        .send()
        .map_err(|_| "Could not check updates")?
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
