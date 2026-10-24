use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
use swarm_gui_host::GuiHost;
fn request(op: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: op.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}
fn main() {
    let base = PathBuf::from(std::env::args().nth(1).unwrap());
    let workspace = PathBuf::from(std::env::args().nth(2).unwrap());
    assert!(!base.exists(), "exclusive new fixture required");
    let data = base.join("Data");
    let mut service = Service::open(&data).unwrap();
    let setup = service
        .request(request(
            "account.setup",
            None,
            json!({"name":"Context Matrix Admin", "password":""}),
        ))
        .unwrap();
    let token = setup["token"].as_str().unwrap();
    let mut projects = vec![];
    for name in ["A", "B"] {
        let root = base.join(format!("Project{name}"));
        std::fs::create_dir_all(&root).unwrap();
        let project = service
            .request(request(
                "project.register",
                Some(token),
                json!({"name":format!("Context {name}"), "root":root}),
            ))
            .unwrap();
        projects.push(json!({"id":project["id"], "root":root}));
    }
    drop(service);
    let mut child = Command::new(workspace.join("target/debug/swarm-runtime.exe"))
        .args(["--data-dir", data.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let host = loop {
        assert!(child.try_wait().unwrap().is_none());
        if let Ok(mut host) = GuiHost::open(&data) {
            if host
                .authenticate("login", "Context Matrix Admin", "")
                .is_ok()
            {
                break host;
            }
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(100));
    };
    std::fs::write(
        base.join("ready.json"),
        serde_json::to_vec_pretty(&json!({
            "data_root":data, "projects":projects, "runtime_pid":child.id(),
            "fixture_kind":"developer Service seed; no native GUI onboarding claim"
        }))
        .unwrap(),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(360);
    while !base.join("stop.request").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let accepted = host.stop_runtime().is_ok();
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let graceful = child.try_wait().unwrap().is_some();
    if !graceful {
        let _ = child.kill();
        let _ = child.wait();
    }
    std::fs::write(
        base.join("stopped.json"),
        serde_json::to_vec_pretty(&json!({
            "human_stop_accepted":accepted, "graceful_exit":graceful
        }))
        .unwrap(),
    )
    .unwrap();
}
