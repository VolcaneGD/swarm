use serde_json::Value;
use std::path::PathBuf;
use swarm_core::protocol::Fault;
impl crate::GuiHost {
    pub fn bug_report_root(&self) -> PathBuf {
        self.root.clone()
    }
    pub fn bug_report_consent(
        &mut self,
        expected_account_id: &str,
        enabled: bool,
    ) -> Result<Value, Fault> {
        let snapshot = self.snapshot()?;
        if snapshot["account"]["id"] != expected_account_id
            || snapshot["account"]["is_admin"] != true
            || snapshot["pendingIssue"] != false
        {
            return Err(Fault::new(
                "PERMISSION_DENIED",
                4,
                "Only the current human Admin can change bug-report consent",
            ));
        }
        swarm_reports::set_consent(&self.root, enabled)
            .map_err(|_| Fault::new("REPORT_STORAGE", 8, "Could not save bug-report consent"))
    }
}
