#![cfg(windows)]
//! Real executable/public AI and private human operation matrix in isolated roots.
use serde_json::{Value, json};
use std::{
    io::Read,
    os::windows::process::CommandExt,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use swarm_core::{Service, protocol::Request};
use swarm_runtime::gui::GuiClient;

fn req(operation: &str, args: Value) -> Request {
    Request {
        operation: operation.into(),
        auth: None,
        args,
        context: json!({}),
        request_id: None,
    }
}
fn drain(stream: impl Read + Send + 'static) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stream
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        assert!(
            bytes.len() <= 4 * 1024 * 1024,
            "fixture public-output bound exceeded"
        );
        bytes
    })
}
struct Runtime {
    child: Child,
}
impl Drop for Runtime {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
fn bounded_wait(child: &mut Child) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("owned fixture process deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn process_memory(pid: u32) -> Value {
    // Read counters only for the directly owned isolated runtime, never discover
    // a production process by name. Emit no command line, path or user data.
    let code = format!(
        "$swarmObservedProcess = Get-Process -Id {pid}; $swarmObservedProcess | Select-Object WorkingSet64,PrivateMemorySize64,PeakWorkingSet64 | ConvertTo-Json -Compress"
    );
    let mut child = Command::new("powershell.exe")
        .creation_flags(0x08000000)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &code,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = drain(child.stdout.take().unwrap());
    let stderr = drain(child.stderr.take().unwrap());
    assert!(
        bounded_wait(&mut child).success(),
        "owned runtime counter read failed"
    );
    let bytes = stdout.join().unwrap();
    let _ = stderr.join().unwrap();
    let memory: Value = serde_json::from_slice(&bytes).unwrap();
    for key in ["WorkingSet64", "PrivateMemorySize64", "PeakWorkingSet64"] {
        assert!(memory[key].as_u64().is_some());
    }
    memory
}
fn no_secrets(bytes: &[u8], secrets: &[String]) {
    let text = String::from_utf8_lossy(bytes);
    for secret in secrets {
        assert!(
            !text.contains(secret),
            "protected fixture secret appeared in public bytes"
        );
    }
    for key in ["session_token", "resume_secret", "recovery_code"] {
        assert!(
            !text.contains(key),
            "private grant field appeared in public bytes"
        );
    }
}
fn cli(root: &Path, alias: &str, words: &[&str], exit: i32, secrets: &[String]) -> Value {
    let stdout = cli_bytes(root, alias, words, exit, secrets, "json");
    let public: Value = serde_json::from_slice(&stdout).unwrap();
    assert_eq!(public["ok"], exit == 0);
    public
}
fn cli_bytes(
    root: &Path,
    alias: &str,
    words: &[&str],
    exit: i32,
    secrets: &[String],
    format: &str,
) -> Vec<u8> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_swarm"));
    command
        .creation_flags(0x08000000)
        .arg("--data-dir")
        .arg(root)
        .args(["--format", format]);
    if words.first() != Some(&"runtime") {
        command.args(["--session", alias]);
    }
    let mut child = command
        .args(words)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = drain(child.stdout.take().unwrap());
    let stderr = drain(child.stderr.take().unwrap());
    let status = bounded_wait(&mut child);
    let stdout = stdout.join().unwrap();
    let stderr = stderr.join().unwrap();
    no_secrets(&stdout, secrets);
    no_secrets(&stderr, secrets);
    assert_eq!(
        status.code(),
        Some(exit),
        "unexpected CLI exit for {}",
        words[0]
    );
    stdout
}
fn gui(client: &GuiClient, alias: &str, operation: &str, args: Value) -> Value {
    client.request(alias, req(operation, args)).unwrap()
}
fn public_fault(
    root: &Path,
    auth: Option<&str>,
    operation: &str,
    args: Value,
    exit: i32,
    secrets: &[String],
) {
    let name = swarm_runtime::pipe_name(root).unwrap();
    let mut connection = swarm_platform::pipe::connect(&name).unwrap();
    let mut request = req(operation, args);
    request.auth = auth.map(str::to_owned);
    swarm_ipc::write_request(&mut connection, &request).unwrap();
    let response = swarm_ipc::read_response(&mut connection).unwrap();
    no_secrets(&serde_json::to_vec(&response).unwrap(), secrets);
    assert!(!response.ok);
    assert_eq!(response.error.unwrap().exit_code, exit);
}
fn permission(client: &GuiClient, kind: &str, id: &str, operation: &str, decision: &str) {
    gui(
        client,
        "admin",
        "permission.set",
        json!({"subject_type":kind,"subject_id":id,"operation":operation,"scope":"global","target_id":"","decision":decision}),
    );
}
fn scan_private_files(path: &Path, secrets: &[String]) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            scan_private_files(&path, secrets);
        } else {
            let bytes = std::fs::read(&path).unwrap();
            for secret in secrets {
                assert!(
                    !bytes
                        .windows(secret.len())
                        .any(|part| part == secret.as_bytes()),
                    "plaintext fixture credential on disk"
                );
            }
        }
    }
}

#[test]
fn standalone_runtime_subjects_recheck_role_denials_ownership_deletion_and_revocation_without_secret_output()
 {
    let data = tempfile::tempdir().unwrap();
    let project_root = tempfile::tempdir().unwrap();
    let password = "private-matrix-password-marker";
    let mut service = Service::open(data.path()).unwrap();
    let grant = service
        .request(req(
            "account.setup",
            json!({"name":"Admin","password":password}),
        ))
        .unwrap();
    let mut secrets = vec![
        password.to_owned(),
        grant["token"].as_str().unwrap().to_owned(),
        grant["recovery_code"].as_str().unwrap().to_owned(),
    ];
    let seed = |service: &mut Service, op: &str, args: Value| {
        let mut request = req(op, args);
        request.auth = grant["token"].as_str().map(str::to_owned);
        service.request(request).unwrap()
    };
    let project = seed(
        &mut service,
        "project.register",
        json!({"name":"Matrix","root":project_root.path()}),
    );
    let board = seed(&mut service, "board.create", json!({"name":"Matrix"}));
    let thread = seed(
        &mut service,
        "thread.create",
        json!({"board_id":board["id"],"name":"Matrix"}),
    );
    for name in ["Member", "Editor"] {
        seed(
            &mut service,
            "account.create",
            json!({"name":name,"password":password}),
        );
    }
    std::fs::write(data.path().join("RULES.md"),"~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: matrix\n    effect: allow\n    actions: [post.create, response.create, post.edit, post.history, changes.get]\naccess:\n  boards: []\n  threads: []\n~~~\n").unwrap();
    drop(service);
    let runtime_exe = Path::new(env!("CARGO_BIN_EXE_swarm"))
        .parent()
        .unwrap()
        .join("swarm-runtime.exe");
    assert!(runtime_exe.is_file(), "build sibling runtime first");
    let mut runtime = Runtime {
        child: Command::new(runtime_exe)
            .creation_flags(0x08000000)
            .arg("--data-dir")
            .arg(data.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    };
    let out = drain(runtime.child.stdout.take().unwrap());
    let err = drain(runtime.child.stderr.take().unwrap());
    for pipe in [
        swarm_runtime::pipe_name(data.path()).unwrap(),
        swarm_runtime::gui_pipe_name(data.path()).unwrap(),
    ] {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(connection) = swarm_platform::pipe::connect(&pipe) {
                drop(connection);
                break;
            }
            assert!(runtime.child.try_wait().unwrap().is_none());
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    let client = GuiClient::open(data.path()).unwrap();
    let startup_memory = process_memory(runtime.child.id());
    let mut accounts = Vec::new();
    for (index, (alias, name)) in [
        ("admin", "Admin"),
        ("member", "Member"),
        ("editor", "Editor"),
    ]
    .iter()
    .enumerate()
    {
        let account = client
            .issue(
                alias,
                &format!("{index:064x}"),
                req("account.login", json!({"name":name,"password":password})),
            )
            .unwrap();
        no_secrets(&serde_json::to_vec(&account).unwrap(), &secrets);
        accounts.push(account["id"].as_str().unwrap().to_owned());
    }
    let human_store =
        swarm_platform::CredentialStore::open(&data.path().join("credentials")).unwrap();
    let journal: Value =
        serde_json::from_slice(&human_store.get("candidate-journal").unwrap().unwrap()).unwrap();
    for candidates in journal["slots"].as_object().unwrap().values() {
        for candidate in candidates.as_array().unwrap() {
            if let Some(token) = candidate["token"].as_str() {
                secrets.push(token.to_owned());
            }
        }
    }
    let thread_id = thread["id"].as_str().unwrap();
    let human = gui(
        &client,
        "admin",
        "post.create",
        json!({"thread_id":thread_id,"body":"retained-original-marker"}),
    );
    let no = human["no"].as_i64().unwrap();
    for alias in ["admin", "member", "editor"] {
        assert_eq!(
            gui(
                &client,
                alias,
                "post.get",
                json!({"thread_id":thread_id,"no":no})
            )["body"],
            "retained-original-marker"
        );
        let created = gui(
            &client,
            alias,
            "post.create",
            json!({"thread_id":thread_id,"body":format!("{alias} body")}),
        );
        assert!(created["no"].as_i64().unwrap() > no);
        assert_eq!(
            gui(
                &client,
                alias,
                "response.create",
                json!({"thread_id":thread_id,"to":no,"body":"reply"})
            )["reply_to"],
            no
        );
    }
    for alias in ["member", "editor"] {
        assert_eq!(
            client
                .request(
                    alias,
                    req(
                        "post.edit",
                        json!({"thread_id":thread_id,"no":no,"body":"unauthorized","if_revision":1})
                    )
                )
                .unwrap_err()
                .exit_code,
            4
        );
        for operation in ["post.delete", "post.restore", "post.deleted_body"] {
            assert_eq!(
                client
                    .request(
                        alias,
                        req(operation, json!({"thread_id":thread_id,"no":no}))
                    )
                    .unwrap_err()
                    .exit_code,
                4
            );
        }
    }
    let role = gui(&client, "admin", "role.create", json!({"name":"Editors"}));
    let role_id = role["id"].as_str().unwrap();
    gui(
        &client,
        "admin",
        "role.assign",
        json!({"account_id":accounts[2],"role_id":role_id,"assigned":true}),
    );
    permission(&client, "role", role_id, "post.edit", "allow");
    gui(
        &client,
        "editor",
        "post.edit",
        json!({"thread_id":thread_id,"no":no,"body":"retained-current-marker","if_revision":1}),
    );
    permission(&client, "account", &accounts[2], "post.edit", "deny");
    assert_eq!(
        client
            .request(
                "editor",
                req(
                    "post.edit",
                    json!({"thread_id":thread_id,"no":no,"body":"denied","if_revision":2})
                )
            )
            .unwrap_err()
            .exit_code,
        4
    );
    let project_id = project["id"].as_str().unwrap();
    let human_no = no.to_string();
    let mut actors = Vec::new();
    for alias in ["ai-a", "ai-b"] {
        let started = cli(
            data.path(),
            alias,
            &["identity", "start", "--project-id", project_id],
            0,
            &secrets,
        );
        actors.push(
            started["data"]["identity"]["id"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
        let store =
            swarm_platform::CredentialStore::open(&data.path().join("client-credentials")).unwrap();
        let saved: Value = serde_json::from_slice(&store.get(alias).unwrap().unwrap()).unwrap();
        for key in ["session_token", "resume_secret"] {
            secrets.push(saved[key].as_str().unwrap().to_owned());
        }
        assert_eq!(
            cli(
                data.path(),
                alias,
                &["post", "get", "--thread-id", thread_id, "--no", &human_no],
                0,
                &secrets
            )["data"]["body"],
            "retained-current-marker"
        );
        assert_eq!(
            cli(
                data.path(),
                alias,
                &[
                    "response",
                    "create",
                    "--thread-id",
                    thread_id,
                    "--to",
                    &human_no,
                    "--body",
                    "AI reply"
                ],
                0,
                &secrets
            )["data"]["reply_to"],
            no
        );
    }
    let own = cli(
        data.path(),
        "ai-a",
        &[
            "post",
            "create",
            "--thread-id",
            thread_id,
            "--body",
            "own-ai-body",
        ],
        0,
        &secrets,
    );
    let own_no = own["data"]["no"].as_i64().unwrap().to_string();
    cli(
        data.path(),
        "ai-a",
        &[
            "post",
            "edit",
            "--thread-id",
            thread_id,
            "--no",
            &own_no,
            "--body",
            "own-ai-revision",
            "--if-revision",
            "1",
        ],
        0,
        &secrets,
    );
    for (alias, target) in [("ai-a", human_no.as_str()), ("ai-b", own_no.as_str())] {
        cli(
            data.path(),
            alias,
            &[
                "post",
                "edit",
                "--thread-id",
                thread_id,
                "--no",
                target,
                "--body",
                "denied",
                "--if-revision",
                "1",
            ],
            4,
            &secrets,
        );
    }
    // A stale revision must fail through both real public output formats and
    // leave the current body intact. Error arguments must not become logs.
    for format in ["json", "markdown"] {
        cli_bytes(
            data.path(),
            "ai-a",
            &[
                "post",
                "edit",
                "--thread-id",
                thread_id,
                "--no",
                &own_no,
                "--body",
                "rejected-draft-marker",
                "--if-revision",
                "1",
            ],
            6,
            &[secrets.clone(), vec!["rejected-draft-marker".into()]].concat(),
            format,
        );
    }
    let markdown = cli_bytes(
        data.path(),
        "ai-a",
        &["post", "get", "--thread-id", thread_id, "--no", &own_no],
        0,
        &secrets,
        "markdown",
    );
    assert!(
        String::from_utf8(markdown)
            .unwrap()
            .contains("own-ai-revision")
    );
    let store =
        swarm_platform::CredentialStore::open(&data.path().join("client-credentials")).unwrap();
    let before: Value = serde_json::from_slice(&store.get("ai-a").unwrap().unwrap()).unwrap();
    let old_token = before["session_token"].as_str().unwrap().to_owned();
    for operation in ["post.delete", "post.restore", "post.deleted_body"] {
        public_fault(
            data.path(),
            Some(&old_token),
            operation,
            json!({"thread_id":thread_id,"no":no}),
            4,
            &secrets,
        );
    }
    public_fault(
        data.path(),
        Some(grant["token"].as_str().unwrap()),
        "board.list",
        json!({}),
        3,
        &secrets,
    );
    public_fault(data.path(), None, "board.list", json!({}), 3, &secrets);
    public_fault(
        data.path(),
        Some(&old_token),
        "account.list",
        json!({}),
        4,
        &secrets,
    );
    cli(
        data.path(),
        "ai-a",
        &[
            "identity",
            "resume",
            "--actor-id",
            &actors[0],
            "--project-id",
            project_id,
        ],
        0,
        &secrets,
    );
    let resumed: Value = serde_json::from_slice(&store.get("ai-a").unwrap().unwrap()).unwrap();
    for key in ["session_token", "resume_secret"] {
        secrets.push(resumed[key].as_str().unwrap().to_owned());
    }
    // Replay the exact old token on the actual public endpoint after rotation.
    let pipe = swarm_runtime::pipe_name(data.path()).unwrap();
    let mut connection = swarm_platform::pipe::connect(&pipe).unwrap();
    let mut stale = req("board.list", json!({}));
    stale.auth = Some(old_token);
    swarm_ipc::write_request(&mut connection, &stale).unwrap();
    assert_eq!(
        swarm_ipc::read_response(&mut connection)
            .unwrap()
            .error
            .unwrap()
            .exit_code,
        3
    );
    let baseline = cli(
        data.path(),
        "ai-a",
        &["changes", "get", "--thread-id", thread_id],
        0,
        &secrets,
    );
    assert_eq!(baseline["data"]["items"], json!([]));
    let change_cursor = baseline["data"]["change_cursor"].as_str().unwrap();
    gui(
        &client,
        "admin",
        "post.delete",
        json!({"thread_id":thread_id,"no":no}),
    );
    let deleted_secrets = [
        secrets.clone(),
        vec![
            "retained-original-marker".into(),
            "retained-current-marker".into(),
        ],
    ]
    .concat();
    for words in [
        vec!["post", "get", "--thread-id", thread_id, "--no", &human_no],
        vec![
            "post",
            "history",
            "--thread-id",
            thread_id,
            "--no",
            &human_no,
        ],
        vec!["post", "list", "--thread-id", thread_id],
        vec!["search", "--query", "retained", "--scope", "body"],
    ] {
        cli(data.path(), "ai-a", &words, 0, &deleted_secrets);
        cli_bytes(data.path(), "ai-a", &words, 0, &deleted_secrets, "markdown");
    }
    let changes = cli(
        data.path(),
        "ai-a",
        &[
            "changes",
            "get",
            "--thread-id",
            thread_id,
            "--after",
            change_cursor,
        ],
        0,
        &deleted_secrets,
    );
    assert!(!changes["data"]["items"].as_array().unwrap().is_empty());
    cli_bytes(
        data.path(),
        "ai-a",
        &[
            "changes",
            "get",
            "--thread-id",
            thread_id,
            "--after",
            change_cursor,
        ],
        0,
        &deleted_secrets,
        "markdown",
    );
    for format in ["json", "markdown"] {
        cli_bytes(
            data.path(),
            "ai-a",
            &[
                "rules",
                "check",
                "--operation",
                "post.history",
                "--thread-id",
                thread_id,
                "--post-no",
                &human_no,
            ],
            0,
            &deleted_secrets,
            format,
        );
    }
    assert_eq!(
        client
            .request(
                "member",
                req("post.deleted_body", json!({"thread_id":thread_id,"no":no}))
            )
            .unwrap_err()
            .exit_code,
        4
    );
    assert_eq!(
        gui(
            &client,
            "admin",
            "post.deleted_body",
            json!({"thread_id":thread_id,"no":no})
        )["body"],
        "retained-current-marker"
    );
    gui(
        &client,
        "admin",
        "post.restore",
        json!({"thread_id":thread_id,"no":no}),
    );
    for (operation, scope, target) in [
        ("read", "board", board["id"].as_str().unwrap()),
        ("read", "thread", thread_id),
    ] {
        gui(
            &client,
            "admin",
            "permission.set",
            json!({"subject_type":"account","subject_id":accounts[1],"operation":operation,"scope":scope,"target_id":target,"decision":"deny"}),
        );
    }
    for operation in ["board.list", "thread.list", "search"] {
        let args = if operation == "search" {
            json!({"query":"retained","scope":"body"})
        } else {
            json!({})
        };
        let filtered = gui(&client, "member", operation, args);
        assert_eq!(filtered["items"], json!([]));
        if operation == "search" {
            assert_eq!(filtered["total_count"], 0);
        }
        let encoded = serde_json::to_vec(&filtered).unwrap();
        no_secrets(&encoded, &deleted_secrets);
        let encoded = String::from_utf8(encoded).unwrap();
        assert!(!encoded.contains(thread_id));
        assert!(!encoded.contains(board["id"].as_str().unwrap()));
    }
    for operation in ["post.get", "post.history"] {
        assert_eq!(
            client
                .request(
                    "member",
                    req(operation, json!({"thread_id":thread_id,"no":no}))
                )
                .unwrap_err()
                .exit_code,
            4
        );
    }
    gui(
        &client,
        "admin",
        "account.disable",
        json!({"id":accounts[1],"disabled":true}),
    );
    assert_eq!(
        client
            .request("member", req("board.list", json!({})))
            .unwrap_err()
            .exit_code,
        3
    );
    gui(
        &client,
        "admin",
        "account.disable",
        json!({"id":accounts[1],"disabled":false}),
    );
    assert_eq!(
        client
            .request("member", req("board.list", json!({})))
            .unwrap_err()
            .exit_code,
        3
    );
    for alias in ["ai-a", "ai-b"] {
        cli(data.path(), alias, &["identity", "release"], 0, &secrets);
    }
    let mut status_times = Vec::new();
    for _ in 0..5 {
        let started = Instant::now();
        cli(
            data.path(),
            "unused-status-alias",
            &["runtime", "status"],
            0,
            &secrets,
        );
        status_times.push(started.elapsed().as_millis());
    }
    let post_workload_memory = process_memory(runtime.child.id());
    eprintln!(
        "isolated_runtime_observation={}",
        json!({"configuration":"debug standalone runtime; small subject fixture; no native GUI","startup_memory":startup_memory,"post_workload_memory":post_workload_memory,"runtime_status_cli_wall_ms":status_times})
    );
    client.stop("admin").unwrap();
    assert!(bounded_wait(&mut runtime.child).success());
    no_secrets(&out.join().unwrap(), &deleted_secrets);
    no_secrets(&err.join().unwrap(), &deleted_secrets);
    scan_private_files(data.path(), &secrets);
}
