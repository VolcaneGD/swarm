//! Interactive native OAuth flow; credentials never enter the renderer.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};
use url::Url;

fn google_fault() -> Fault {
    Fault::new(
        "GOOGLE_AUTH_FAILED",
        3,
        "Google authentication did not complete. Retry explicitly.",
    )
}
fn require_google_available() -> Result<(), Fault> {
    if swarm_core::google_recovery_available() {
        Ok(())
    } else {
        Err(Fault::new(
            "FEATURE_UNAVAILABLE",
            2,
            "Google linking and recovery are unavailable in this beta",
        ))
    }
}
fn random() -> String {
    format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn callback(target: &str, state: &str) -> Result<String, Fault> {
    let url = Url::parse(&format!("http://127.0.0.1{target}")).map_err(|_| google_fault())?;
    if url.path() != "/google-callback" || url.fragment().is_some() {
        return Err(google_fault());
    }
    let pairs: Vec<_> = url.query_pairs().collect();
    if pairs.iter().filter(|(k, _)| k == "state").count() != 1
        || pairs
            .iter()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.as_ref())
            != Some(state)
    {
        return Err(google_fault());
    }
    if pairs.iter().any(|(key, _)| key == "error") {
        return Err(Fault::new(
            "GOOGLE_CANCELLED",
            3,
            "Google authentication was cancelled",
        ));
    }
    if pairs.iter().filter(|(key, _)| key == "code").count() != 1 {
        return Err(google_fault());
    }
    pairs
        .iter()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.to_string())
        .filter(|v| !v.is_empty() && v.len() <= 4096)
        .ok_or_else(google_fault)
}
fn read_callback(stream: &mut TcpStream, state: &str) -> Result<String, Fault> {
    stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .map_err(|_| google_fault())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(1)))
        .map_err(|_| google_fault())?;
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 1024];
    while bytes.len() < 8192 {
        let count = stream.read(&mut chunk).map_err(|_| google_fault())?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.windows(4).any(|v| v == b"\r\n\r\n") {
            break;
        }
    }
    let request = std::str::from_utf8(&bytes).map_err(|_| google_fault())?;
    let fields: Vec<_> = request
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    let result = if fields.len() == 3
        && fields[0] == "GET"
        && fields[2] == "HTTP/1.1"
        && bytes.len() <= 8192
    {
        callback(fields[1], state)
    } else {
        Err(google_fault())
    };
    let status = if result.is_ok() {
        "200 OK"
    } else {
        "400 Bad Request"
    };
    let body = if result.is_ok() {
        "Google authentication received. Return to Swarm. / Swarmに戻ってください。"
    } else {
        "Invalid callback."
    };
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    result
}
fn authorize(nonce: &str, open: impl FnOnce(&str) -> Result<(), Fault>) -> Result<String, Fault> {
    require_google_available()?;
    if nonce.len() != 64 {
        return Err(google_fault());
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| google_fault())?;
    listener.set_nonblocking(true).map_err(|_| google_fault())?;
    let redirect = format!(
        "http://127.0.0.1:{}/google-callback",
        listener.local_addr().map_err(|_| google_fault())?.port()
    );
    let state = random();
    let verifier = random();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url =
        Url::parse("https://accounts.google.com/o/oauth2/v2/auth").map_err(|_| google_fault())?;
    url.query_pairs_mut().extend_pairs([
        ("client_id", swarm_core::google_client_id()),
        ("redirect_uri", redirect.as_str()),
        ("response_type", "code"),
        ("scope", "openid email"),
        ("state", state.as_str()),
        ("nonce", nonce),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
        ("prompt", "select_account"),
        ("access_type", "online"),
    ]);
    open(url.as_str())?;
    let deadline = Instant::now() + Duration::from_secs(240);
    let code = loop {
        if Instant::now() >= deadline {
            return Err(Fault::new(
                "GOOGLE_TIMEOUT",
                3,
                "Google authentication timed out",
            ));
        }
        match listener.accept() {
            Ok((mut stream, _)) => match read_callback(&mut stream, &state) {
                Ok(code) => break code,
                Err(error) if error.code == "GOOGLE_CANCELLED" => return Err(error),
                Err(_) => {}
            },
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100))
            }
            Err(_) => return Err(google_fault()),
        }
    };
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| google_fault())?;
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", swarm_core::google_client_id()),
            ("code", code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|_| google_fault())?;
    let mut body = Vec::new();
    response
        .take(65537)
        .read_to_end(&mut body)
        .map_err(|_| google_fault())?;
    if body.len() > 65536 {
        return Err(google_fault());
    }
    let token: Value = serde_json::from_slice(&body).map_err(|_| google_fault())?;
    token["id_token"]
        .as_str()
        .filter(|v| !v.is_empty() && v.len() <= 16384)
        .map(str::to_owned)
        .ok_or_else(google_fault)
}
impl GuiHost {
    pub fn google_link(
        &mut self,
        expected: &str,
        current_password: &str,
        open: impl FnOnce(&str) -> Result<(), Fault>,
    ) -> Result<Value, Fault> {
        require_google_available()?;
        if current_password.len() > 4096 {
            return Err(invalid());
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        let account = client.request(&selected.alias, request("account.status", json!({})))?;
        if account["id"].as_str() != Some(expected) {
            return Err(authentication());
        }
        let challenge = client.request(&selected.alias,request("account.google.begin",json!({"name":account["name"],"purpose":"link","current_password":current_password})))?;
        let nonce = challenge["nonce"].as_str().ok_or_else(google_fault)?;
        let token = authorize(nonce, open)?;
        client.request(
            &selected.alias,
            request(
                "account.google.link",
                json!({"challenge":nonce,"id_token":token}),
            ),
        )?;
        self.snapshot()
    }
    pub fn google_unlink(
        &mut self,
        expected: &str,
        current_password: &str,
    ) -> Result<Value, Fault> {
        require_google_available()?;
        if current_password.len() > 4096 {
            return Err(invalid());
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        let account = client.request(&selected.alias, request("account.status", json!({})))?;
        if account["id"].as_str() != Some(expected) {
            return Err(authentication());
        }
        client.request(
            &selected.alias,
            request(
                "account.google.unlink",
                json!({"current_password":current_password}),
            ),
        )?;
        self.snapshot()
    }
    pub fn google_recover(
        &mut self,
        name: &str,
        password: &str,
        open: impl FnOnce(&str) -> Result<(), Fault>,
    ) -> Result<Value, Fault> {
        require_google_available()?;
        if name.trim().is_empty()
            || name.len() > 4096
            || password.trim().is_empty()
            || password.len() > 4096
        {
            return Err(invalid());
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let challenge = GuiClient::open(&self.root)?.google_begin_recovery(name)?;
        let nonce = challenge["nonce"].as_str().ok_or_else(google_fault)?;
        let token = authorize(nonce, open)?;
        let pending = Pending {
            version: 1,
            intent: random(),
            alias: alias(name),
            request: request(
                "account.google.recover",
                json!({"name":name,"password":password,"challenge":nonce,"id_token":token}),
            ),
        };
        let bytes = serde_json::to_vec(&pending).map_err(|_| storage())?;
        let gate = self.notification_selection_gate()?;
        self.store
            .put("pending-auth", &bytes)
            .map_err(|_| storage())?;
        drop(gate);
        self.retry_pending()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn callback_rejects_wrong_state_duplicates_errors_and_other_paths() {
        assert_eq!(
            callback("/google-callback?code=abc&state=expected", "expected").unwrap(),
            "abc"
        );
        for target in [
            "/google-callback?code=abc&state=other",
            "/google-callback?code=abc&state=expected&state=expected",
            "/google-callback?code=abc&code=def&state=expected",
            "/other?code=abc&state=expected",
            "/google-callback?error=access_denied&state=expected",
            "/google-callback?code=&state=expected",
        ] {
            assert!(callback(target, "expected").is_err());
        }
    }
}
