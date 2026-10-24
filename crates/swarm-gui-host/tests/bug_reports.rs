#![cfg(windows)]
use serde_json::json;
use std::time::{Duration, Instant};
use swarm_core::{Service, protocol::Request};
use swarm_gui_host::GuiHost;

#[test]
fn consent_requires_current_authenticated_admin_and_preserves_full_stop() {
    let root = tempfile::tempdir().unwrap();
    let mut service = Service::open(root.path()).unwrap();
    service
        .request(Request {
            operation: "account.setup".into(),
            auth: None,
            args: json!({"name":"Admin","password":"synthetic-password"}),
            context: json!({}),
            request_id: None,
        })
        .unwrap();
    drop(service);
    let path = root.path().to_owned();
    let server =
        std::thread::spawn(move || swarm_runtime::serve_with_gui(&path, None, None).unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    while swarm_platform::pipe::connect(&swarm_runtime::gui_pipe_name(root.path()).unwrap())
        .is_err()
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut host = GuiHost::open(root.path()).unwrap();
    assert!(host.bug_report_consent("unknown", true).is_err());
    host.authenticate("login", "Admin", "synthetic-password")
        .unwrap();
    let id = host.snapshot().unwrap()["account"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(host.bug_report_consent("different-account", true).is_err());
    assert_eq!(host.bug_report_consent(&id, true).unwrap()["enabled"], true);
    assert_eq!(
        host.bug_report_consent(&id, false).unwrap()["enabled"],
        false
    );
    host.stop_runtime().unwrap();
    server.join().unwrap();
    assert!(swarm_runtime::lifecycle::check_running_allowed(root.path()).is_err());
    assert_eq!(swarm_reports::flush(root.path()).unwrap()["enabled"], false);
}
