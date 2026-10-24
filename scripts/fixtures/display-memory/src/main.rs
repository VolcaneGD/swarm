//! Ordinary display workload, no fault injection, credential invalidation or OS test.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use swarm_core::{CredentialCandidate, Service, protocol::Request};
use swarm_gui_host::GuiHost;
use swarm_runtime::gui::GuiClient;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn ensure(b: bool, m: &str) -> Result<()> {
    if b { Ok(()) } else { Err(m.into()) }
}
fn req(op: &str, args: Value, token: Option<&str>) -> Request {
    Request {
        operation: op.into(),
        args,
        auth: token.map(str::to_owned),
        context: json!({}),
        request_id: None,
    }
}
fn save(base: &Path, name: &str, v: &Value) -> Result<()> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(base.join(name))?;
    writeln!(f, "{v}")?;
    Ok(())
}
fn paths(case: &str) -> Result<(PathBuf, PathBuf, PathBuf)> {
    ensure(
        matches!(
            case,
            "baseline"
                | "candidate"
                | "date-cache"
                | "observer"
                | "poll-stable"
                | "control-50-long"
                | "virtual"
                | "virtual-lazy"
        ),
        "unknown case",
    )?;
    let base = Path::new("E:/Swarm-Validation-Host/run-20261022-display-memory").join(case);
    let root = if matches!(
        case,
        "date-cache" | "observer" | "poll-stable" | "control-50-long" | "virtual" | "virtual-lazy"
    ) {
        base.parent().ok_or("base missing")?.join("candidate/Data")
    } else {
        base.join("Data")
    };
    let name = if case == "baseline" {
        "candidate-20261019-account-feedback"
    } else if case == "candidate" {
        "candidate-20261022-literal-markdown"
    } else if case == "virtual-lazy" {
        "candidate-20261022-virtual-posts-lazy"
    } else if case == "virtual" {
        "candidate-20261022-virtual-posts"
    } else if matches!(case, "poll-stable" | "control-50-long") {
        "candidate-20261022-stable-notifications"
    } else {
        "candidate-20261022-date-cache"
    };
    let candidate =
        Path::new("C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution")
            .join(name);
    Ok((base, root, candidate))
}
fn prepare(case: &str, complete_unstarted: bool) -> Result<()> {
    ensure(
        matches!(case, "baseline" | "candidate"),
        "existing root is reopen-only",
    )?;
    let (base, root, candidate) = paths(case)?;
    ensure(
        if complete_unstarted {
            case == "baseline" && root.is_dir() && !base.join("runtime.json").exists()
        } else {
            !base.exists()
        },
        "existing display fixture refused outside its known preparation boundary",
    )?;
    ensure(
        candidate.join("swarm-desktop.exe").is_file()
            && candidate.join("swarm-runtime.exe").is_file(),
        "candidate missing",
    )?;
    if !complete_unstarted {
        fs::create_dir_all(&base)?;
    }
    let mut service = Service::open(&root)?;
    service.set_startup_preference(false)?;
    let mut token = String::new();
    let operation = if complete_unstarted {
        "account.login"
    } else {
        "account.setup"
    };
    service.request_with_credential_sink(
        req(
            operation,
            json!({"name":"Display Fixture","password":"synthetic-display-only"}),
            None,
        ),
        |grant| {
            if let CredentialCandidate::Human { token: issued, .. } =
                CredentialCandidate::from_grant(operation, grant)?
            {
                token = issued;
            }
            Ok(())
        },
    )?;
    ensure(!token.is_empty(), "synthetic setup failed")?;
    let thread = if complete_unstarted {
        let boards = service.request(req("board.list", json!({}), Some(&token)))?;
        let boards = boards["items"].as_array().ok_or("boards missing")?;
        ensure(
            boards.len() == 1 && boards[0]["name"] == "DISPLAY_MEMORY_BOARD",
            "unexpected seed Board",
        )?;
        let threads = service.request(req(
            "thread.list",
            json!({"board_id":boards[0]["id"]}),
            Some(&token),
        ))?;
        let threads = threads["items"].as_array().ok_or("Threads missing")?;
        ensure(
            threads.len() == 1 && threads[0]["name"] == "DISPLAY_MEMORY_1000",
            "unexpected seed Thread",
        )?;
        let page = service.request(req(
            "post.page",
            json!({"thread_id":threads[0]["id"],"page":1,"page_size":1000}),
            Some(&token),
        ))?;
        ensure(
            page["total_posts"] == 37,
            "not the known unstarted reply boundary",
        )?;
        save(
            &base,
            "preparation-continuation.json",
            &json!({"original_posts_retained":37,"first_missing_no":38,"cause":"formal response.create with to required; reply_to is not a post.create argument","runtime_not_yet_started":true}),
        )?;
        threads[0].clone()
    } else {
        let board = service.request(req(
            "board.create",
            json!({"name":"DISPLAY_MEMORY_BOARD"}),
            Some(&token),
        ))?;
        service.request(req(
            "thread.create",
            json!({"board_id":board["id"],"name":"DISPLAY_MEMORY_1000"}),
            Some(&token),
        ))?
    };
    for no in if complete_unstarted { 38 } else { 1 }..=1000 {
        let body = if no == 54 {
            "D".repeat(100000)
        } else if no == 53 {
            "MEMORY_TARGET_53".into()
        } else {
            format!("Memory sample Post {no}.")
        };
        let mut args = json!({"thread_id":thread["id"],"body":body});
        let operation = if (38..=53).contains(&no) {
            args["to"] = json!(no - 1);
            "response.create"
        } else {
            "post.create"
        };
        service.request(req(operation, args, Some(&token)))?;
    }
    token.clear();
    drop(service);
    launch_runtime(&base, &root, &candidate, case)?;
    finish_display(case)
}
fn reopen(case: &str) -> Result<()> {
    ensure(
        matches!(
            case,
            "date-cache"
                | "observer"
                | "poll-stable"
                | "control-50-long"
                | "virtual"
                | "virtual-lazy"
        ),
        "only fixed display root may reopen",
    )?;
    let (base, root, candidate) = paths(case)?;
    ensure(
        !base.exists() && root.is_dir(),
        "preserve existing observations",
    )?;
    ensure(
        swarm_runtime::lifecycle::status(&root)?["state"] == "stopped",
        "display runtime must be normally stopped",
    )?;
    fs::create_dir(&base)?;
    launch_runtime(&base, &root, &candidate, case)?;
    finish_display(case)
}
fn launch_runtime(base: &Path, root: &Path, candidate: &Path, case: &str) -> Result<()> {
    let mut command = Command::new(candidate.join("swarm-runtime.exe"));
    command
        .args([
            "--data-dir",
            root.to_str().ok_or("root invalid")?,
            "--human-start",
        ])
        .stdin(Stdio::null())
        .stdout(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(base.join("runtime.stdout.log"))?,
        )
        .stderr(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(base.join("runtime.stderr.log"))?,
        );
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn()?;
    save(base, "runtime.json", &json!({"pid":child.id(),"case":case}))?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        ensure(child.try_wait()?.is_none(), "runtime exited")?;
        if GuiClient::open(root).and_then(|c| c.setup_status()).is_ok() {
            break;
        }
        ensure(Instant::now() < deadline, "readiness deadline")?;
        std::thread::sleep(Duration::from_millis(30));
    }
    Ok(())
}
fn finish_display(case: &str) -> Result<()> {
    let (base, root, candidate) = paths(case)?;
    ensure(
        base.join("runtime.json").exists() && !base.join("prepared.json").exists(),
        "display preparation already finished or runtime missing",
    )?;
    let mut h = GuiHost::open(&root)?;
    let snapshot = h.authenticate("login", "Display Fixture", "synthetic-display-only")?;
    h.backup_control(
        snapshot["account"]["id"]
            .as_str()
            .ok_or("account missing")?,
        "settings.set",
        json!({"enabled":false,"interval_seconds":86400,"retention":2,"destination":null}),
    )?;
    h.manage(
        "display.settings.set",
        json!({"language":"ja","theme":"light"}),
    )?;
    let boards = h.read("board.list", json!({}))?;
    let boards = boards["items"].as_array().ok_or("Boards missing")?;
    ensure(
        boards.len() == 1 && boards[0]["name"] == "DISPLAY_MEMORY_BOARD",
        "unexpected Board",
    )?;
    let threads = h.read("thread.list", json!({"board_id":boards[0]["id"]}))?;
    let threads = threads["items"].as_array().ok_or("Threads missing")?;
    ensure(
        threads.len() == 1 && threads[0]["name"] == "DISPLAY_MEMORY_1000",
        "unexpected Thread",
    )?;
    let thread = &threads[0];
    if case == "control-50-long" {
        let account = snapshot["account"]["id"]
            .as_str()
            .ok_or("account missing")?;
        let old = h.ui_preferences(
            account,
            "ui.preferences.get",
            json!({"scope":"thread_scroll","key":thread["id"]}),
        )?;
        ensure(
            old["value"].is_object(),
            "original saved preference required",
        )?;
        save(&base, "original-preference.json", &old)?;
        h.ui_preferences(account, "ui.preferences.set", json!({"scope":"thread_scroll","key":thread["id"],"value":{"version":1,"postNo":53,"offset":0,"scrollTop":0,"atBottom":false,"page":2,"pageSize":50}}))?;
    }
    let page = h.read(
        "post.page",
        json!({"thread_id":thread["id"],"page":1,"page_size":1000}),
    )?;
    ensure(page["total_posts"] == 1000, "seed incomplete")?;
    let items = page["items"].as_array().ok_or("Posts missing")?;
    let post53 = items
        .iter()
        .find(|p| p["no"] == 53)
        .ok_or("target missing")?["id"]
        .clone();
    save(
        &base,
        "prepared.json",
        &json!({"case":case,"root":root,"candidate":candidate,"posts":1000,"long_post_no":54,"long_body_scalars":100000,"target_post_no":53,"target_post_id":post53,"reply_depth":16,"thread_id":thread["id"],"body_template":"Memory sample Post N.; No53 marker; No54 D*100000","gui_sha256":format!("{:x}",Sha256::digest(fs::read(candidate.join("swarm-desktop.exe"))?)),"runtime_sha256":format!("{:x}",Sha256::digest(fs::read(candidate.join("swarm-runtime.exe"))?)),"native_test_accepted":false}),
    )?;
    println!("DISPLAY_FIXTURE_PREPARED");
    Ok(())
}
fn cleanup(case: &str) -> Result<()> {
    let (base, root, candidate) = paths(case)?;
    if case == "control-50-long" {
        let prepared: Value = serde_json::from_slice(&fs::read(base.join("prepared.json"))?)?;
        let old: Value = serde_json::from_slice(&fs::read(base.join("original-preference.json"))?)?;
        let mut h = GuiHost::open(&root)?;
        let account = h.snapshot()?;
        h.ui_preferences(
            account["account"]["id"].as_str().ok_or("account missing")?,
            "ui.preferences.set",
            json!({"scope":"thread_scroll","key":prepared["thread_id"],"value":old["value"]}),
        )?;
        save(
            &base,
            "preference-restored.json",
            &json!({"restored_original":true}),
        )?;
    }
    let raw = swarm_platform::CredentialStore::open(&root.join("gui-state"))?
        .get("selection")?
        .ok_or("selection missing")?;
    let selected: Value = serde_json::from_slice(&raw)?;
    if swarm_runtime::lifecycle::status(&root)?["state"] != "stopped" {
        GuiClient::open(&root)?.stop(selected["alias"].as_str().ok_or("alias missing")?)?;
    }
    let registration = swarm_platform::notifications::Registration::new(
        swarm_platform::notifications::Identity::new(&root, &candidate.join("swarm-desktop.exe"))?,
    )?;
    registration.remove()?;
    ensure(!registration.is_registered()?, "registration remains")?;
    save(
        &base,
        "cleanup.json",
        &json!({"normal_stop":true,"registration_absent":true,"data_preserved":true}),
    )
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.as_slice() {
        [case, op] if op == "reopen" => reopen(case),
        [case, op] if op == "prepare" => prepare(case, false),
        [case, op] if op == "complete-preparation" => prepare(case, true),
        [case, op] if op == "finish-display" => finish_display(case),
        [case, op] if op == "cleanup" => cleanup(case),
        _ => Err("expected case and prepare/cleanup".into()),
    };
    if let Err(e) = r {
        if let Some(f) = e.downcast_ref::<swarm_core::protocol::Fault>() {
            eprintln!("Product fault code: {}", f.code);
        }
        eprintln!("Display fixture failed; preserve all Data and receipts");
        std::process::exit(1)
    }
}
