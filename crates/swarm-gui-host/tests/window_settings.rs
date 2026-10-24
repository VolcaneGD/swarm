#![cfg(windows)]

use swarm_core::WindowGeometry;
use swarm_gui_host::GuiHost;
use swarm_platform::CredentialStore;

#[test]
fn native_bounds_persist_without_login_or_runtime_and_keep_full_stop() {
    let root = tempfile::tempdir().unwrap();
    let store = CredentialStore::open(&root.path().join("lifecycle")).unwrap();
    let stop = b"1:11111111-1111-4111-8111-111111111111";
    store.put("full-stop", stop).unwrap();
    let geometry = WindowGeometry {
        version: 1,
        x: -1200,
        y: 30,
        width: 880,
        height: 760,
        maximized: true,
    };
    {
        let mut host = GuiHost::open(root.path()).unwrap();
        assert_eq!(host.window_geometry_get().unwrap(), None);
        host.window_geometry_set(&geometry).unwrap();
        assert_eq!(host.window_geometry_get().unwrap(), Some(geometry));
        assert!(!root.path().join("gui-host").exists());
        assert_eq!(store.get("full-stop").unwrap().unwrap(), stop);
    }
    let mut host = GuiHost::open(root.path()).unwrap();
    assert_eq!(host.window_geometry_get().unwrap(), Some(geometry));
    let invalid = WindowGeometry {
        width: 0,
        ..geometry
    };
    assert!(host.window_geometry_set(&invalid).is_err());
    assert_eq!(host.window_geometry_get().unwrap(), Some(geometry));
    let journal = CredentialStore::open(&root.path().join("restore-transactions")).unwrap();
    journal.put("active", b"isolated-active-guard").unwrap();
    assert_eq!(
        host.window_geometry_get().unwrap_err().code,
        "RESTORE_RECOVERY_REQUIRED"
    );
    assert_eq!(
        host.window_geometry_set(&geometry).unwrap_err().code,
        "RESTORE_RECOVERY_REQUIRED"
    );
    assert_eq!(store.get("full-stop").unwrap().unwrap(), stop);
}
