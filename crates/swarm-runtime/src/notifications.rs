//! Trusted runtime delivery; never exposed on either RPC transport.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use swarm_core::{NotificationBatch, protocol::Fault};
use swarm_platform::{
    CredentialStore,
    notifications::{ActivationTarget, Identity, NotificationError, Registration, ToastContent},
};

const WINDOW_MS: u32 = 2000;
const GUI_BINDING: &str = "gui-executable";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    version: u32,
    executable: PathBuf,
}

fn state_error() -> Fault {
    Fault::new(
        "NOTIFICATION_STATE",
        8,
        "Notification registration could not be verified",
    )
}
fn os_error(error: NotificationError) -> Fault {
    Fault::new(
        if error == NotificationError::Disabled {
            "NOTIFICATION_DISABLED"
        } else {
            "NOTIFICATION_OS"
        },
        8,
        "Windows notification submission failed",
    )
}

/// Native desktop/installer initialization only. No caller-supplied renderer path.
/// The runtime never installs or changes Windows registration on its own.
pub fn register(root: &Path, executable: &Path) -> Result<(), Fault> {
    let identity = Identity::new(root, executable).map_err(os_error)?;
    let store =
        CredentialStore::open(&root.join("notification-state")).map_err(|_| state_error())?;
    let _gate = store
        .lock("native-registration")
        .map_err(|_| state_error())?;
    Registration::new(identity)
        .map_err(os_error)?
        .install()
        .map_err(os_error)?;
    let binding = Binding {
        version: 1,
        executable: executable.canonicalize().map_err(|_| state_error())?,
    };
    store
        .put(
            GUI_BINDING,
            &serde_json::to_vec(&binding).map_err(|_| state_error())?,
        )
        .map_err(|_| state_error())
}

fn native_send(root: &Path, batch: &NotificationBatch, silent: bool) -> Result<(), Fault> {
    let store =
        CredentialStore::open(&root.join("notification-state")).map_err(|_| state_error())?;
    let bytes = store
        .get(GUI_BINDING)
        .map_err(|_| state_error())?
        .ok_or_else(state_error)?;
    let binding: Binding = serde_json::from_slice(&bytes).map_err(|_| state_error())?;
    if binding.version != 1 {
        return Err(state_error());
    }
    let identity = Identity::new(root, &binding.executable).map_err(os_error)?;
    let content = content(&identity, batch)?;
    let content = if silent {
        content.without_popup()
    } else {
        content
    };
    Registration::new(identity)
        .map_err(os_error)?
        .show(&content)
        .map_err(os_error)
}

fn content(identity: &Identity, batch: &NotificationBatch) -> Result<ToastContent, Fault> {
    let first = batch.deliveries.first().ok_or_else(state_error)?;
    if batch
        .deliveries
        .iter()
        .any(|item| item.account_id != first.account_id || item.thread_id != first.thread_id)
    {
        return Err(state_error());
    }
    let target = ActivationTarget::new(&first.account_id, &first.thread_id).map_err(os_error)?;
    let posts = batch
        .deliveries
        .iter()
        .filter(|item| item.kind == swarm_core::NotificationKind::Post)
        .count();
    let responses = batch.deliveries.len() - posts;
    // At most one authorized, already bounded excerpt; never concatenate bodies.
    let excerpt = batch
        .deliveries
        .iter()
        .find_map(|item| item.excerpt.as_deref());
    ToastContent::new(
        identity,
        &target,
        &batch.id,
        posts,
        responses,
        excerpt,
        match batch.language {
            Some(swarm_core::DisplayLanguage::Ja) => true,
            Some(swarm_core::DisplayLanguage::En) => false,
            None => swarm_platform::display::japanese(),
        },
    )
    .map_err(os_error)
}

pub(crate) fn native_sink(root: &Path) -> Sink {
    let root = root.to_path_buf();
    Box::new(move |batch| native_send(&root, batch, false))
}

pub(crate) type Sink = Box<dyn FnMut(&NotificationBatch) -> Result<(), Fault> + Send>;

pub(crate) fn run(shared: crate::SharedService, stopping: Arc<AtomicBool>, mut sink: Sink) {
    let mut reported = false;
    let mut retry_after = None;
    while !stopping.load(Ordering::Acquire) {
        if retry_after.is_some_and(|deadline| std::time::Instant::now() < deadline) {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        let result = (|| {
            let mut guard = shared.lock().map_err(|_| state_error())?;
            if stopping.load(Ordering::Acquire) {
                return Ok(false);
            }
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| state_error())?;
            let now = i64::try_from(now.as_millis()).map_err(|_| state_error())?;
            let (service, sessions, _) = &mut *guard;
            crate::restore_apply::blocked(service.data_root())?;
            // The same service mutex serializes permissions, posts and full stop.
            // deliver retains the selection gate through sink and DB receipt.
            sessions.deliver_notification_batch(service, now, WINDOW_MS, &mut sink)
        })();
        match result {
            Ok(true) => reported = false,
            Ok(false) => {}
            Err(error) if error.code == "RUNTIME_STOPPED" => break,
            Err(_) => {
                // Persisted batch backoff is authoritative after restart. Also
                // wait at least five seconds from the actual failed call's return.
                retry_after = Some(std::time::Instant::now() + Duration::from_secs(5));
                if !reported {
                    eprintln!(
                        "Swarm notification delivery is unavailable; pending work is retained"
                    );
                    reported = true;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::sync::{Mutex, mpsc};
    use swarm_core::{Service, protocol::Request};
    use swarm_platform::CredentialStore;

    fn request(operation: &str, args: Value) -> Request {
        Request {
            operation: operation.into(),
            args,
            auth: None,
            context: json!({}),
            request_id: None,
        }
    }
    fn fixture(root: &std::path::Path) -> crate::SharedService {
        let mut service = Service::open(root).unwrap();
        let mut sessions = crate::session::SessionBridge::open(root, &mut service).unwrap();
        sessions
            .gui_issue(
                &mut service,
                "admin",
                &"a".repeat(64),
                request(
                    "account.setup",
                    json!({"name":"Admin","password":"password"}),
                ),
            )
            .unwrap();
        CredentialStore::open(&root.join("gui-state"))
            .unwrap()
            .put("selection", br#"{"alias":"admin"}"#)
            .unwrap();
        sessions
            .synchronize_notification_recipient_locked(&mut service)
            .unwrap();
        let board = sessions
            .gui_request(
                &mut service,
                "admin",
                request("board.create", json!({"name":"Board"})),
            )
            .unwrap();
        let thread = sessions
            .gui_request(
                &mut service,
                "admin",
                request(
                    "thread.create",
                    json!({"name":"Thread","board_id":board["id"]}),
                ),
            )
            .unwrap();
        sessions
            .gui_request(
                &mut service,
                "admin",
                request(
                    "thread.subscribe",
                    json!({"id":thread["id"],"subscribed":true}),
                ),
            )
            .unwrap();
        for body in ["First", "Second"] {
            sessions
                .gui_request(
                    &mut service,
                    "admin",
                    request("post.create", json!({"thread_id":thread["id"],"body":body})),
                )
                .unwrap();
        }
        let access = crate::gui::GuiAccess::open(root).unwrap();
        Arc::new(Mutex::new((service, sessions, access)))
    }

    #[test]
    fn worker_batches_after_two_seconds_retries_same_id_and_keeps_unread() {
        let root = tempfile::tempdir().unwrap();
        let shared = fixture(root.path());
        let stopping = Arc::new(AtomicBool::new(false));
        let (sent, received) = mpsc::channel();
        let worker_shared = shared.clone();
        let worker_stop = stopping.clone();
        let started = std::time::Instant::now();
        let worker = std::thread::spawn(move || {
            let mut first = true;
            run(
                worker_shared,
                worker_stop,
                Box::new(move |batch| {
                    sent.send((batch.id.clone(), batch.deliveries.len(), started.elapsed()))
                        .unwrap();
                    if std::mem::take(&mut first) {
                        Err(Fault::new("NOTIFICATION_OS", 8, "Controlled failure"))
                    } else {
                        Ok(())
                    }
                }),
            );
        });
        let first = received.recv_timeout(Duration::from_secs(5));
        if first.is_err() {
            stopping.store(true, Ordering::Release);
            worker.join().unwrap();
            panic!("runtime worker must deliver committed posts without a GUI request");
        }
        let first =
            first.expect("runtime worker must deliver committed posts without a GUI request");
        assert_eq!(first.1, 2);
        assert!(first.2 >= Duration::from_millis(1700));
        let second = received.recv_timeout(Duration::from_secs(7));
        stopping.store(true, Ordering::Release);
        worker.join().unwrap();
        let second = second.expect("failed OS submission must retry");
        assert_eq!(first.0, second.0);
        assert!(second.2 - first.2 >= Duration::from_secs(5));
        let mut guard = shared.lock().unwrap();
        let (service, sessions, _) = &mut *guard;
        assert!(
            sessions
                .prepare_notifications(service, 200)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            sessions
                .gui_request(service, "admin", request("conversation.unread", json!({})))
                .unwrap()["total"],
            2
        );
    }

    #[test]
    fn failed_notification_retry_rechecks_deleted_content_after_real_worker_restart() {
        // Real time and real persisted Service/bridge state; only the OS sink is
        // replaced. This does not simulate elapsed retention days or OS delivery.
        for scope in ["post", "thread", "board"] {
            let root = tempfile::tempdir().unwrap();
            let shared = fixture(root.path());
            let (thread, board, removed_post) = {
                let mut guard = shared.lock().unwrap();
                let (service, sessions, _) = &mut *guard;
                sessions
                    .gui_request(
                        service,
                        "admin",
                        request(
                            "notification.settings.set",
                            json!({"enabled":true,"include_excerpt":true}),
                        ),
                    )
                    .unwrap();
                let pending = sessions.prepare_notifications(service, 200).unwrap();
                assert_eq!(pending.len(), 2);
                let thread = pending[0].thread_id.clone();
                let removed_post = pending[0].post_id.clone();
                let board = sessions
                    .gui_request(
                        service,
                        "admin",
                        request("thread.get", json!({"id":thread})),
                    )
                    .unwrap()["board_id"]
                    .as_str()
                    .unwrap()
                    .to_owned();
                (thread, board, removed_post)
            };
            let stopping = Arc::new(AtomicBool::new(false));
            let (sent, received) = mpsc::channel();
            let worker_shared = shared.clone();
            let worker_stop = stopping.clone();
            let worker = std::thread::spawn(move || {
                run(
                    worker_shared,
                    worker_stop,
                    Box::new(move |batch| {
                        sent.send(batch.clone()).unwrap();
                        Err(Fault::new(
                            "NOTIFICATION_OS",
                            8,
                            "Controlled failed submission",
                        ))
                    }),
                )
            });
            let failed = received.recv_timeout(Duration::from_secs(6));
            stopping.store(true, Ordering::Release);
            worker.join().unwrap();
            let failed = failed.expect("positive control: real worker reaches the failed sink");
            assert_eq!(failed.deliveries.len(), 2);
            assert!(failed.deliveries.iter().all(|item| item.excerpt.is_some()));

            let expected_post = {
                let mut guard = shared.lock().unwrap();
                let (service, sessions, _) = &mut *guard;
                if scope == "post" {
                    let posts = sessions
                        .gui_request(
                            service,
                            "admin",
                            request("post.page", json!({"thread_id":thread})),
                        )
                        .unwrap();
                    let post = posts["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|post| post["id"] == removed_post)
                        .unwrap();
                    sessions
                        .gui_request(
                            service,
                            "admin",
                            request("post.delete", json!({"thread_id":thread,"no":post["no"]})),
                        )
                        .unwrap();
                    failed
                        .deliveries
                        .iter()
                        .find(|item| item.post_id != removed_post)
                        .unwrap()
                        .post_id
                        .clone()
                } else {
                    let id = if scope == "board" { &board } else { &thread };
                    sessions
                        .gui_request(
                            service,
                            "admin",
                            request(&format!("{scope}.delete"), json!({"id":id})),
                        )
                        .unwrap();
                    sessions
                        .gui_request(
                            service,
                            "admin",
                            request(&format!("{scope}.purge"), json!({"id":id,"confirmed":true})),
                        )
                        .unwrap();
                    let live_board = sessions
                        .gui_request(
                            service,
                            "admin",
                            request("board.create", json!({"name":"Live control"})),
                        )
                        .unwrap();
                    let live_thread = sessions
                        .gui_request(
                            service,
                            "admin",
                            request(
                                "thread.create",
                                json!({"board_id":live_board["id"],"name":"Live control"}),
                            ),
                        )
                        .unwrap();
                    sessions
                        .gui_request(
                            service,
                            "admin",
                            request(
                                "thread.subscribe",
                                json!({"id":live_thread["id"],"subscribed":true}),
                            ),
                        )
                        .unwrap();
                    sessions
                        .gui_request(
                            service,
                            "admin",
                            request(
                                "post.create",
                                json!({"thread_id":live_thread["id"],"body":"Live control body"}),
                            ),
                        )
                        .unwrap()["id"]
                        .as_str()
                        .unwrap()
                        .to_owned()
                }
            };
            drop(shared);
            let mut service = Service::open(root.path()).unwrap();
            let sessions = crate::session::SessionBridge::open(root.path(), &mut service).unwrap();
            let access = crate::gui::GuiAccess::open(root.path()).unwrap();
            let reopened = Arc::new(Mutex::new((service, sessions, access)));
            let stopping = Arc::new(AtomicBool::new(false));
            let worker_stop = stopping.clone();
            let worker_shared = reopened.clone();
            let (sent, received) = mpsc::channel();
            let worker = std::thread::spawn(move || {
                run(
                    worker_shared,
                    worker_stop,
                    Box::new(move |batch| {
                        sent.send(batch.clone()).unwrap();
                        Ok(())
                    }),
                )
            });
            let delivered = received.recv_timeout(Duration::from_secs(8));
            stopping.store(true, Ordering::Release);
            worker.join().unwrap();
            let delivered = delivered.expect("live control is still delivered after reopen");
            assert_eq!(delivered.deliveries.len(), 1, "scope: {scope}");
            assert_eq!(
                delivered.deliveries[0].post_id, expected_post,
                "scope: {scope}"
            );
            assert_ne!(delivered.deliveries[0].post_id, removed_post);
            assert!(received.try_recv().is_err());
            let mut guard = reopened.lock().unwrap();
            let (service, sessions, _) = &mut *guard;
            assert!(
                sessions
                    .prepare_notifications(service, 200)
                    .unwrap()
                    .is_empty()
            );
        }
    }

    #[test]
    fn in_flight_delivery_serializes_real_stop_and_selection_change() {
        use std::sync::atomic::AtomicUsize;
        let root = tempfile::tempdir().unwrap();
        let shared = fixture(root.path());
        let stopping = Arc::new(AtomicBool::new(false));
        let (_observer, stop_read) =
            crate::lifecycle_drain_tests::StopObserver::register(root.path());
        let listener =
            swarm_platform::pipe::Listener::bind(&crate::gui_pipe_name(root.path()).unwrap())
                .unwrap();
        let listener_root = root.path().to_owned();
        let listener_shared = shared.clone();
        let listener_stop = stopping.clone();
        let listener_worker = std::thread::spawn(move || {
            crate::serve_listener(
                listener,
                None,
                listener_shared,
                Arc::new(AtomicUsize::new(0)),
                true,
                listener_stop,
                listener_root,
            )
        });
        let (entered, sending) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let worker_shared = shared.clone();
        let worker_stop = stopping.clone();
        let count = Arc::new(AtomicUsize::new(0));
        let worker_count = count.clone();
        let worker = std::thread::spawn(move || {
            run(
                worker_shared,
                worker_stop,
                Box::new(move |_| {
                    worker_count.fetch_add(1, Ordering::SeqCst);
                    entered.send(()).unwrap();
                    released.recv_timeout(Duration::from_secs(8)).unwrap();
                    Ok(())
                }),
            )
        });
        sending.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            shared.try_lock().is_err(),
            "delivery must hold the service guard"
        );
        let stop_root = root.path().to_owned();
        let (stopped, stop_result) = mpsc::channel();
        let stop_worker = std::thread::spawn(move || {
            let client = crate::gui::GuiClient::open(&stop_root).unwrap();
            stopped.send(client.stop("admin")).unwrap();
        });
        stop_read.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(
            stop_result
                .recv_timeout(Duration::from_millis(100))
                .is_err(),
            "accepted stop must wait for in-flight delivery"
        );
        let state_root = root.path().join("gui-state");
        let (selected, selection_done) = mpsc::channel();
        let selection_worker = std::thread::spawn(move || {
            let store = CredentialStore::open(&state_root).unwrap();
            let _gate = store.lock("notification-selection-gate").unwrap();
            store.remove("selection").unwrap();
            selected.send(()).unwrap();
        });
        assert!(
            selection_done
                .recv_timeout(Duration::from_millis(100))
                .is_err(),
            "selection must wait for in-flight delivery"
        );
        release.send(()).unwrap();
        assert_eq!(
            stop_result
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap()["state"],
            "stopped"
        );
        selection_done.recv_timeout(Duration::from_secs(5)).unwrap();
        stop_worker.join().unwrap();
        selection_worker.join().unwrap();
        listener_worker.join().unwrap().unwrap();
        worker.join().unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(stopping.load(Ordering::Acquire));
        assert_eq!(
            crate::lifecycle::check_running_allowed(root.path())
                .unwrap_err()
                .code,
            "RUNTIME_STOPPED"
        );
    }

    #[test]
    fn absent_or_corrupt_native_binding_does_not_acknowledge_a_batch() {
        let root = tempfile::tempdir().unwrap();
        let shared = fixture(root.path());
        let mut guard = shared.lock().unwrap();
        let (service, sessions, _) = &mut *guard;
        let now = i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap()
            + 3000;
        let mut sink = native_sink(root.path());
        assert_eq!(
            sessions
                .deliver_notification_batch(service, now, WINDOW_MS, &mut sink)
                .unwrap_err()
                .code,
            "NOTIFICATION_STATE"
        );
        let store = CredentialStore::open(&root.path().join("notification-state")).unwrap();
        store.put(GUI_BINDING, b"corrupt binding").unwrap();
        assert_eq!(
            sessions
                .deliver_notification_batch(service, now + 5000, WINDOW_MS, &mut sink)
                .unwrap_err()
                .code,
            "NOTIFICATION_STATE"
        );
        assert_eq!(store.get(GUI_BINDING).unwrap().unwrap(), b"corrupt binding");
        assert_eq!(
            sessions.prepare_notifications(service, 200).unwrap().len(),
            2
        );
        assert_eq!(
            sessions
                .gui_request(service, "admin", request("conversation.unread", json!({})))
                .unwrap()["total"],
            2
        );
    }

    #[test]
    #[ignore = "opt-in isolated Windows registration and silent runtime notification delivery"]
    fn runtime_worker_submits_to_windows_and_receipts_without_a_gui() {
        struct Cleanup(Registration);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                if self
                    .0
                    .is_registered()
                    .expect("verify owned fixture registration")
                {
                    self.0
                        .clear_history()
                        .expect("clear owned silent fixture history");
                }
                self.0
                    .remove()
                    .expect("remove only owned fixture registration");
            }
        }
        let root = tempfile::tempdir().unwrap();
        let shared = fixture(root.path());
        let executable = std::env::current_exe().unwrap();
        let registration =
            Registration::new(Identity::new(root.path(), &executable).unwrap()).unwrap();
        let cleanup = Cleanup(registration);
        register(root.path(), &executable).unwrap();
        assert!(cleanup.0.is_registered().unwrap());
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_stop = stopping.clone();
        let worker_shared = shared.clone();
        let worker_root = root.path().to_owned();
        let (submitted, receipt) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            run(
                worker_shared,
                worker_stop,
                Box::new(move |batch| {
                    let result = native_send(&worker_root, batch, true);
                    submitted.send(result.clone()).unwrap();
                    result
                }),
            )
        });
        let result = receipt.recv_timeout(Duration::from_secs(10));
        stopping.store(true, Ordering::Release);
        worker.join().unwrap();
        result
            .expect("bounded Windows API attempt")
            .expect("Windows history must confirm the runtime batch");
        let mut guard = shared.lock().unwrap();
        let (service, sessions, _) = &mut *guard;
        assert!(
            sessions
                .prepare_notifications(service, 200)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            sessions
                .gui_request(service, "admin", request("conversation.unread", json!({})))
                .unwrap()["total"],
            2
        );
        drop(guard);
        cleanup.0.clear_history().unwrap();
        cleanup.0.remove().unwrap();
        assert!(!cleanup.0.is_registered().unwrap());
    }
}
