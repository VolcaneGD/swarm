use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};
use swarm_core::protocol::Request;
use swarm_platform::CredentialStore;
use swarm_runtime::gui::GuiClient;

const ROOT: &str = "E:/Swarm-Validation-Host/run-20261018-h1-auth/Data";
const OUT: &str = "E:/Swarm-Validation-Host/run-20261018-h1-auth/deleted-bulk-stage.jsonl";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).ok_or("mode required")?;
    if ![
        "--stage-once",
        "--stage-mixed-once",
        "--stage-fixed-once",
        "--stage-cascade-once",
        "--verify-cascade",
        "--stage-parent-restore-once",
        "--verify-parent-restore",
        "--read",
    ]
    .contains(&mode.as_str())
    {
        return Err("unsupported mode".into());
    }
    let root = Path::new(ROOT);
    if root
        .canonicalize()?
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/")
        != ROOT
    {
        return Err("fixed root mismatch".into());
    }
    let state = CredentialStore::open(&root.join("gui-state"))?;
    for key in ["pending-auth", "pending-create"] {
        if state.get(key)?.is_some() {
            return Err("pending operation; stop".into());
        }
    }
    let selection: Value = serde_json::from_slice(&state.get("selection")?.ok_or("no selection")?)?;
    let alias = selection["alias"].as_str().ok_or("no alias")?;
    let client = GuiClient::open(root)?;
    let call = |operation: &str, args: Value| {
        client
            .request(
                alias,
                Request {
                    operation: operation.into(),
                    auth: None,
                    args,
                    context: json!({}),
                    request_id: None,
                },
            )
            .map_err(|_| "product request failed; inspect saved receipt, do not replay")
    };
    let account = call("account.status", json!({}))?;
    if account["name"] != "H1_TEST_ADMIN" || account["is_admin"] != true {
        return Err("wrong account".into());
    }
    let original: Value =
        serde_json::from_slice(&fs::read(root.parent().unwrap().join("thread-stage.json"))?)?;
    let post = call(
        "post.get",
        json!({"thread_id":original["thread_id"],"no":1}),
    )?;
    if post["id"] != original["post_id"]
        || post["revision"] != 1
        || post["body"] != "H1_PERMISSION_PRIVATE_BODY"
    {
        return Err("protected original mismatch".into());
    }
    if mode == "--read" {
        println!("{}", call("conversation.deleted", json!({}))?);
        println!("{}", call("board.list", json!({}))?);
        println!(
            "{}",
            json!({"original_preserved":true,"account":"H1_TEST_ADMIN"})
        );
        return Ok(());
    }
    let cascade_path = root
        .parent()
        .unwrap()
        .join(if mode.contains("parent-restore") {
            "deleted-parent-restore-stage.jsonl"
        } else {
            "deleted-cascade-stage.jsonl"
        });
    if mode == "--verify-parent-restore" {
        let entries: Vec<Value> = fs::read_to_string(&cascade_path)?
            .lines()
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?;
        let fixture = entries
            .last()
            .filter(|v| v["stage"] == "ready")
            .ok_or("restore fixture not ready")?;
        let boards = call("board.list", json!({}))?;
        if !boards["items"]
            .as_array()
            .ok_or("board list shape")?
            .iter()
            .any(|b| b["id"] == fixture["board_id"])
        {
            return Err("restored parent absent".into());
        }
        let mut checked = Vec::new();
        for thread in fixture["threads"]
            .as_array()
            .ok_or("thread metadata absent")?
        {
            let restored = call("thread.get", json!({"id":thread["id"]}))?;
            let post = call("post.get", json!({"thread_id":thread["id"],"no":1}))?;
            let reply = call("post.get", json!({"thread_id":thread["id"],"no":2}))?;
            let history = call("post.history", json!({"thread_id":thread["id"],"no":1}))?;
            if restored["id"] != thread["id"]
                || post["id"] != thread["post_id"]
                || post["revision"] != 2
                || post["body"] != "SYNTHETIC_CASCADE_EDITED"
                || reply["id"] != thread["reply_id"]
                || reply["reply_to"] != 1
                || reply["revision"] != 1
                || reply["body"] != "SYNTHETIC_CASCADE_REPLY"
                || history["items"].as_array().map(Vec::len) != Some(1)
            {
                return Err("restored content metadata/body/history mismatch".into());
            }
            checked.push(
                json!({"thread_id":thread["id"],"post_id":post["id"],"post_revision":2,
                "reply_id":reply["id"],"reply_to":1,"body_matches":true,"history_count":1}),
            );
        }
        let inventory = call("conversation.deleted", json!({}))?;
        if inventory["boards"].as_array().is_none_or(|v| !v.is_empty())
            || inventory["threads"]
                .as_array()
                .is_none_or(|v| !v.is_empty())
        {
            return Err("restored deleted inventory not empty".into());
        }
        let mut out = OpenOptions::new().write(true).create_new(true).open(
            root.parent()
                .unwrap()
                .join("deleted-parent-restore-verified.json"),
        )?;
        writeln!(
            out,
            "{}",
            json!({"original_preserved":true,"board_id":fixture["board_id"],"checks":checked,"deleted_inventory_empty":true})
        )?;
        println!(
            "PASS: parent and both children restored with exact post/reply IDs, revisions, bodies and history count; original preserved"
        );
        return Ok(());
    }
    if mode == "--verify-cascade" {
        let entries: Vec<Value> = fs::read_to_string(&cascade_path)?
            .lines()
            .map(serde_json::from_str)
            .collect::<Result<_, _>>()?;
        let fixture = entries
            .last()
            .filter(|v| v["stage"] == "ready")
            .ok_or("cascade not ready")?;
        let mut checks = Vec::new();
        let mut requests = vec![("board.restore", json!({"id":fixture["board_id"]}))];
        for thread in fixture["threads"]
            .as_array()
            .ok_or("thread metadata absent")?
        {
            requests.extend([
                ("thread.get", json!({"id":thread["id"]})),
                ("thread.restore", json!({"id":thread["id"]})),
                ("post.get", json!({"thread_id":thread["id"],"no":1})),
                ("post.history", json!({"thread_id":thread["id"],"no":1})),
                ("response.list", json!({"thread_id":thread["id"],"to":1})),
            ]);
        }
        for (operation, args) in requests {
            let fault = client
                .request(
                    alias,
                    Request {
                        operation: operation.into(),
                        auth: None,
                        args,
                        context: json!({}),
                        request_id: None,
                    },
                )
                .err()
                .ok_or("purged object unexpectedly available")?;
            if fault.code != "NOT_FOUND" || fault.exit_code != 5 {
                return Err("unexpected purge readback fault".into());
            }
            checks
                .push(json!({"operation":operation,"code":fault.code,"exit_code":fault.exit_code}));
        }
        let inventory = call("conversation.deleted", json!({}))?;
        if inventory["boards"].as_array().is_none_or(|v| !v.is_empty())
            || inventory["threads"]
                .as_array()
                .is_none_or(|v| !v.is_empty())
        {
            return Err("deleted inventory not empty".into());
        }
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.parent().unwrap().join("deleted-cascade-verified.json"))?;
        writeln!(
            out,
            "{}",
            json!({"original_preserved":true,"checks":checks,"deleted_inventory_empty":true,
            "native_evidence_separate":true,"public_ai_readback":false})
        )?;
        println!("PASS: cascade11 NOT_FOUND checks, empty inventory, protected original preserved");
        return Ok(());
    }
    let inventory = call("conversation.deleted", json!({}))?;
    if inventory["boards"].as_array().is_none_or(|v| !v.is_empty())
        || inventory["threads"]
            .as_array()
            .is_none_or(|v| !v.is_empty())
    {
        return Err("existing deleted inventory; preserve and stop".into());
    }
    if mode == "--stage-cascade-once" || mode == "--stage-parent-restore-once" {
        let restore_case = mode == "--stage-parent-restore-once";
        let mut out = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(cascade_path)?;
        writeln!(
            out,
            "{}",
            json!({"stage":"started","root":ROOT,"retries":0})
        )?;
        out.sync_all()?;
        let board = call(
            "board.create",
            json!({"name":if restore_case { "H1_PARENT_RESTORE_BOARD" } else { "H1_CASCADE_PURGE_BOARD" }}),
        )?;
        writeln!(out, "{}", json!({"stage":"board_created","id":board["id"]}))?;
        out.sync_all()?;
        let mut threads = Vec::new();
        for index in 1..=2 {
            let thread = call(
                "thread.create",
                json!({"board_id":board["id"],"name":format!("{}_{index}",if restore_case { "H1_PARENT_RESTORE_CHILD" } else { "H1_CASCADE_CHILD" })}),
            )?;
            writeln!(
                out,
                "{}",
                json!({"stage":"thread_created","id":thread["id"]})
            )?;
            out.sync_all()?;
            let post = call(
                "post.create",
                json!({"thread_id":thread["id"],"body":"SYNTHETIC_CASCADE_ORIGINAL"}),
            )?;
            call(
                "post.edit",
                json!({"thread_id":thread["id"],"no":1,"if_revision":1,"body":"SYNTHETIC_CASCADE_EDITED"}),
            )?;
            let reply = call(
                "response.create",
                json!({"thread_id":thread["id"],"to":1,"body":"SYNTHETIC_CASCADE_REPLY"}),
            )?;
            let history = call("post.history", json!({"thread_id":thread["id"],"no":1}))?;
            if history["items"].as_array().map(Vec::len) != Some(1)
                || reply["no"] != 2
                || reply["reply_to"] != 1
            {
                return Err("cascade content baseline mismatch; do not replay".into());
            }
            threads.push(json!({"id":thread["id"],"post_id":post["id"],"reply_id":reply["id"],"history_count":1,"post_revision":2,"reply_to":1}));
            if index == 2 {
                call("thread.delete", json!({"id":thread["id"]}))?;
            }
        }
        call("board.delete", json!({"id":board["id"]}))?;
        writeln!(
            out,
            "{}",
            json!({"stage":"ready","board_id":board["id"],"threads":threads,
            "independent_deleted_child":threads[1]["id"],"original_preserved":true})
        )?;
        out.sync_all()?;
        println!("{}", call("conversation.deleted", json!({}))?);
        println!("cascade fixture ready; native acceptance NOT_RUN; no replay");
        return Ok(());
    }
    if mode == "--stage-mixed-once" {
        let saved = fs::read_to_string(OUT)?;
        let created: Vec<Value> = saved
            .lines()
            .map(serde_json::from_str::<Value>)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|v| v["stage"] == "created")
            .collect();
        if created.len() != 2 {
            return Err("two retained Boards required".into());
        }
        let live = call("board.list", json!({}))?;
        let items = live["items"].as_array().ok_or("Board list shape")?;
        for board in &created {
            if !items
                .iter()
                .any(|v| v["id"] == board["board_id"] && v["name"] == board["name"])
            {
                return Err("retained restored Board mismatch".into());
            }
        }
        let mut mixed = OpenOptions::new().write(true).create_new(true).open(
            root.parent()
                .unwrap()
                .join("deleted-bulk-mixed-stage.jsonl"),
        )?;
        writeln!(mixed, "{}", json!({"stage":"started","retries":0}))?;
        mixed.sync_all()?;
        for index in 1..=2 {
            let thread = call(
                "thread.create",
                json!({"board_id":created[0]["board_id"],"name":format!("H1_BULK_THREAD_{index}")}),
            )?;
            writeln!(
                mixed,
                "{}",
                json!({"stage":"created","thread_id":thread["id"],"name":thread["name"]})
            )?;
            mixed.sync_all()?;
            call("thread.delete", json!({"id":thread["id"]}))?;
            writeln!(
                mixed,
                "{}",
                json!({"stage":"deleted","thread_id":thread["id"]})
            )?;
            mixed.sync_all()?;
        }
        call("board.delete", json!({"id":created[1]["board_id"]}))?;
        writeln!(
            mixed,
            "{}",
            json!({"stage":"deleted","board_id":created[1]["board_id"]})
        )?;
        mixed.sync_all()?;
        println!("{}", call("conversation.deleted", json!({}))?);
        return Ok(());
    }
    let output = if mode == "--stage-fixed-once" {
        root.parent()
            .unwrap()
            .join("deleted-bulk-fixed-stage.jsonl")
    } else {
        Path::new(OUT).to_path_buf()
    };
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    writeln!(
        out,
        "{}",
        json!({"stage":"started","root":ROOT,"retries":0,"original_preserved":true})
    )?;
    out.sync_all()?;
    // Product API fixture preparation, never direct database writes or native purge.
    for index in 1..=2 {
        let label = if mode == "--stage-fixed-once" {
            "H1_BULK_FIXED"
        } else {
            "H1_BULK_RESTORE"
        };
        let board = call("board.create", json!({"name":format!("{label}_{index}")}))?;
        writeln!(
            out,
            "{}",
            json!({"stage":"created","board_id":board["id"],"name":board["name"]})
        )?;
        out.sync_all()?;
        call("board.delete", json!({"id":board["id"]}))?;
        writeln!(out, "{}", json!({"stage":"deleted","board_id":board["id"]}))?;
        out.sync_all()?;
    }
    println!("{}", call("conversation.deleted", json!({}))?);
    println!("two synthetic Boards staged once; original preserved; no native acceptance claimed");
    Ok(())
}
