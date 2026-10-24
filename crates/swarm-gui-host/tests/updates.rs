#![cfg(windows)]
use swarm_gui_host::GuiHost;

#[test]
fn update_notes_require_the_installed_version_and_are_consumed_once() {
    let root = tempfile::tempdir().unwrap();
    let host = GuiHost::open(root.path()).unwrap();
    assert!(host.consume_update_notes("0.1.0").unwrap().is_none());
    host.stage_update_receipt("0.1.0", "0.2.0", "Fixed search\nImproved updates")
        .unwrap();
    // Download/cancel/failure/ordinary restart must never claim an installed update.
    assert!(host.consume_update_notes("0.1.0").unwrap().is_none());
    drop(host);
    let host = GuiHost::open(root.path()).unwrap();
    assert!(host.consume_update_notes("0.3.0").unwrap().is_none());
    let notes = host.consume_update_notes("0.2.0").unwrap().unwrap();
    assert_eq!(notes["version"], "0.2.0");
    assert_eq!(notes["notes"], "Fixed search\nImproved updates");
    assert!(host.consume_update_notes("0.2.0").unwrap().is_none());
    assert!(
        host.stage_update_receipt("0.2.0", "0.2.0", "same version")
            .is_err()
    );
    assert!(
        host.stage_update_receipt("0.1.0", "0.2.0", &"x".repeat(8193))
            .is_err()
    );
}
