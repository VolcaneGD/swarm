#![cfg(windows)]

use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::Read,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Barrier},
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
use swarm_runtime::gui::GuiClient;

fn request(operation: &str, auth: Option<&str>, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: None,
    }
}

struct Runtime {
    child: Child,
    root: PathBuf,
}
impl Runtime {
    fn stop(&mut self) {
        if self.child.try_wait().unwrap().is_some() {
            return;
        }
        let client = GuiClient::open(&self.root).unwrap();
        client
            .issue(
                "fixture-admin",
                &"d".repeat(64),
                request(
                    "account.login",
                    None,
                    json!({"name":"Admin","password":"concurrency-fixture"}),
                ),
            )
            .unwrap();
        client.stop("fixture-admin").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(Instant::now() < deadline, "fixture runtime stop deadline");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        // Only this guard's directly spawned isolated child is eligible for
        // termination, even when an assertion or normal stop has failed.
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn cli_executable() -> PathBuf {
    let Some(directory) = std::env::var_os("SWARM_TEST_CANDIDATE_DIRECTORY") else {
        return PathBuf::from(env!("CARGO_BIN_EXE_swarm"));
    };
    // The override is test-only and restricted to a staged repository candidate.
    // It must never select an arbitrary executable outside the owned workspace.
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap();
    let allowed = workspace
        .join("target/distribution")
        .canonicalize()
        .unwrap();
    let directory = PathBuf::from(directory).canonicalize().unwrap();
    assert!(directory.starts_with(&allowed));
    let executable = directory.join("swarm.exe").canonicalize().unwrap();
    assert!(executable.starts_with(&directory));
    executable
}

fn cli(root: &Path, alias: &str, args: &[String]) -> Value {
    let mut child = Command::new(cli_executable())
        .creation_flags(0x08000000)
        .arg("--data-dir")
        .arg(root)
        .args(["--session", alias, "--format", "json"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Drain both pipes while the process runs. A page exceeding the OS pipe
    // buffer must not make the fixture wait for exit before consuming stdout.
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let read = |reader: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            reader.take(1024 * 1024).read_to_end(&mut bytes).unwrap();
            bytes
        })
    };
    let stdout = read(Box::new(stdout));
    let stderr = read(Box::new(stderr));
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        match child.try_wait().unwrap() {
            Some(status) => break status,
            None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("isolated CLI deadline expired");
            }
        }
    };
    let stdout = stdout.join().unwrap();
    let _stderr = stderr.join().unwrap();
    // Outputs are bounded fixture pages/bodies. Do not echo them on failure.
    let public: Value = serde_json::from_slice(&stdout).unwrap();
    assert!(
        status.success(),
        "CLI exit {:?}, code {:?}",
        status.code(),
        public["error"]["code"]
    );
    assert_eq!(public["schema_version"], 1);
    assert_eq!(public["ok"], true);
    assert!(public["error"].is_null());
    let text = String::from_utf8(stdout).unwrap();
    for private_key in ["session_token", "resume_secret"] {
        assert!(!text.contains(private_key));
    }
    public["data"].clone()
}
fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn twenty_real_cli_processes_preserve_posts_replies_owners_and_retry_keys() {
    let data = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let mut service = Service::open(data.path()).unwrap();
    let admin = service
        .request(request(
            "account.setup",
            None,
            json!({"name":"Admin","password":"concurrency-fixture"}),
        ))
        .unwrap();
    let token = admin["token"].as_str();
    let project = service
        .request(request(
            "project.register",
            token,
            json!({"name":"Concurrent CLI","root":project_root.path()}),
        ))
        .unwrap();
    let board = service
        .request(request(
            "board.create",
            token,
            json!({"name":"Concurrent CLI"}),
        ))
        .unwrap();
    let topic = service
        .request(request(
            "thread.create",
            token,
            json!({"board_id":board["id"],"name":"Concurrent CLI"}),
        ))
        .unwrap();
    let thread_id = topic["id"].as_str().unwrap().to_owned();
    let project_id = project["id"].as_str().unwrap().to_owned();
    service
        .request(request(
            "post.create",
            token,
            json!({"thread_id":thread_id,"body":"Reply target"}),
        ))
        .unwrap();
    std::fs::write(data.path().join("RULES.md"), "~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: isolated-cli-posts\n    effect: allow\n    actions: [post.create, response.create]\naccess:\n  boards: []\n  threads: []\n~~~\n").unwrap();
    drop(service);

    let runtime_exe = cli_executable().parent().unwrap().join("swarm-runtime.exe");
    assert!(
        runtime_exe.is_file(),
        "build swarm-runtime beside swarm.exe before this integration test"
    );
    let child = Command::new(runtime_exe)
        .creation_flags(0x08000000)
        .arg("--data-dir")
        .arg(data.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut runtime = Runtime {
        child,
        root: data.path().to_owned(),
    };
    let name = swarm_runtime::pipe_name(data.path()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(connection) = swarm_platform::pipe::connect(&name) {
            drop(connection);
            break;
        }
        assert!(
            runtime.child.try_wait().unwrap().is_none(),
            "fixture runtime exited before readiness"
        );
        assert!(
            Instant::now() < deadline,
            "fixture runtime readiness deadline"
        );
        std::thread::sleep(Duration::from_millis(10));
    }

    let mut actors = Vec::new();
    for index in 0..20 {
        let alias = format!("worker-{index}");
        let started = cli(
            data.path(),
            &alias,
            &args(&[
                "identity",
                "start",
                "--project-id",
                &project_id,
                "--model-name",
                "Fixture",
            ]),
        );
        actors.push(started["identity"]["id"].as_str().unwrap().to_owned());
    }
    assert_eq!(actors.iter().collect::<BTreeSet<_>>().len(), 20);
    let barrier = Arc::new(Barrier::new(20));
    let workers: Vec<_> = (0..20)
        .map(|index| {
            let root = data.path().to_owned();
            let target = thread_id.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let alias = format!("worker-{index}");
                let body = format!("CLI worker {index}");
                let key = format!("cli-create-{index}");
                let mut command = args(&[
                    if index % 2 == 0 { "post" } else { "response" },
                    "create",
                    "--thread-id",
                    &target,
                    "--body",
                    &body,
                    "--request-id",
                    &key,
                ]);
                if index % 2 != 0 {
                    command.extend(args(&["--to", "1"]));
                }
                barrier.wait();
                let created = cli(&root, &alias, &command);
                let replayed = cli(&root, &alias, &command);
                assert_eq!(created["id"], replayed["id"]);
                assert_eq!(created["no"], replayed["no"]);
                (index, created)
            })
        })
        .collect();
    let completed: Vec<_> = workers.into_iter().map(|worker| worker.join()).collect();
    assert!(
        completed.iter().all(Result::is_ok),
        "concurrent CLI workers failed after all workers joined"
    );
    let results: Vec<_> = completed.into_iter().map(Result::unwrap).collect();
    let numbers: BTreeSet<_> = results
        .iter()
        .map(|(_, result)| result["no"].as_u64().unwrap())
        .collect();
    assert_eq!(numbers, (2..=21).collect());
    let page = cli(
        data.path(),
        "worker-0",
        &args(&["post", "list", "--thread-id", &thread_id]),
    );
    assert_eq!(page["items"].as_array().unwrap().len(), 21);
    let replies = cli(
        data.path(),
        "worker-0",
        &args(&["response", "list", "--thread-id", &thread_id, "--to", "1"]),
    );
    assert_eq!(replies["items"].as_array().unwrap().len(), 10);
    for index in 0..20 {
        cli(
            data.path(),
            &format!("worker-{index}"),
            &args(&["identity", "release"]),
        );
    }
    runtime.stop();

    let mut service = Service::open(data.path()).unwrap();
    let stored = service
        .request(request("post.list", token, json!({"thread_id":thread_id})))
        .unwrap();
    // Human and AI envelopes intentionally differ: AI reads carry their own
    // Thread-rule acknowledgement. Compare the durable records and pagination.
    assert_eq!(stored["items"], page["items"]);
    assert_eq!(stored["next_before_no"], page["next_before_no"]);
    let posts = stored["items"].as_array().unwrap();
    assert_eq!(
        posts
            .iter()
            .map(|post| post["id"].as_str().unwrap())
            .collect::<BTreeSet<_>>()
            .len(),
        21
    );
    for (index, created) in results {
        let post = posts
            .iter()
            .find(|post| post["id"] == created["id"])
            .unwrap();
        assert_eq!(post["body"], format!("CLI worker {index}"));
        assert_eq!(post["author"]["id"], actors[index]);
        assert_eq!(post["thread_id"], thread_id);
        assert_eq!(
            post["reply_to"],
            if index % 2 == 0 {
                Value::Null
            } else {
                json!(1)
            }
        );
    }
    assert_eq!(posts[0]["response_count"], 10);
    let db = rusqlite::Connection::open(data.path().join("swarm.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(
        db.prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none()
    );
}
