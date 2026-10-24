use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub operation: String,
    #[serde(default)]
    pub auth: Option<String>,
    #[serde(default)]
    pub args: Value,
    #[serde(default)]
    pub context: Value,
    #[serde(default)]
    pub request_id: Option<String>,
}
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Request")
            .field("operation", &self.operation)
            .field("payload", &"[redacted]")
            .finish()
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Fault {
    pub code: String,
    pub exit_code: i32,
    pub message: String,
}
impl Fault {
    pub(crate) fn internal() -> Self {
        Self::new("INTERNAL_ERROR", 8, "Storage or internal operation failed")
    }
    pub(crate) fn authentication() -> Self {
        Self::new("AUTHENTICATION_REQUIRED", 3, "A valid login is required")
    }
    pub fn new(code: &str, exit_code: i32, message: &str) -> Self {
        Self {
            code: code.into(),
            exit_code,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for Fault {}
impl From<rusqlite::Error> for Fault {
    fn from(_: rusqlite::Error) -> Self {
        Self::internal()
    }
}
