use super::{GuiHost, authentication, conflict, invalid, request};
use serde_json::{Value, json};
use swarm_core::protocol::Fault;
use swarm_runtime::gui::GuiClient;

impl GuiHost {
    /// Recheck selected account under Tauri's host mutex; core owns permission checks.
    pub fn retention_control(
        &mut self,
        expected_account_id: &str,
        operation: &str,
        args: Value,
    ) -> Result<Value, Fault> {
        let fields: &[&str] = match operation {
            "retention.settings.get" => &[],
            "retention.settings.set" => &[
                "history_days",
                "deleted_days",
                "deleted_board_days",
                "deleted_thread_days",
                "archived_thread_days",
            ],
            _ => return Err(invalid()),
        };
        let object = args.as_object().ok_or_else(invalid)?;
        if expected_account_id.is_empty()
            || expected_account_id.len() > 128
            || (operation == "retention.settings.get" && !object.is_empty())
            || (operation == "retention.settings.set"
                && (!object.contains_key("history_days") || !object.contains_key("deleted_days")))
            || object.keys().any(|key| !fields.contains(&key.as_str()))
        {
            return Err(invalid());
        }
        for value in object.values() {
            if !value.is_null() && value.as_i64().is_none_or(|days| days <= 0) {
                return Err(invalid());
            }
        }
        if self.pending()?.is_some() || self.pending_create()?.is_some() {
            return Err(conflict());
        }
        let selected = self.selection()?.ok_or_else(authentication)?;
        let client = GuiClient::open(&self.root)?;
        let account = client.request(&selected.alias, request("account.status", json!({})))?;
        if account.get("id").and_then(Value::as_str) != Some(expected_account_id) {
            return Err(Fault::new(
                "ACCOUNT_CHANGED",
                6,
                "The selected account changed; refresh before continuing",
            ));
        }
        client.request(&selected.alias, request(operation, args))
    }
}
