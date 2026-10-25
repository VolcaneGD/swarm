use serde_json::{Value, json};
use swarm_core::{
    Service,
    protocol::{Fault, Request},
};

fn call(
    service: &mut Service,
    op: &str,
    auth: Option<&str>,
    args: Value,
    context: Value,
    key: Option<&str>,
) -> Result<Value, Fault> {
    service.request(Request {
        operation: op.into(),
        auth: auth.map(str::to_owned),
        args,
        context,
        request_id: key.map(str::to_owned),
    })
}

#[test]
fn capture_is_transactional_project_scoped_and_preserves_post_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let foreign = tempfile::tempdir().unwrap();
    let mut s = Service::open(dir.path()).unwrap();
    let human = call(
        &mut s,
        "account.setup",
        None,
        json!({"name":"Admin","password":"fixture-password"}),
        json!({}),
        None,
    )
    .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let board = call(
        &mut s,
        "board.create",
        Some(&human),
        json!({"name":"Capture"}),
        json!({}),
        None,
    )
    .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let thread = call(
        &mut s,
        "thread.create",
        Some(&human),
        json!({"board_id":board,"name":"Models"}),
        json!({}),
        None,
    )
    .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let project = call(
        &mut s,
        "project.register",
        Some(&human),
        json!({"name":"Capture","root":dir.path()}),
        json!({}),
        None,
    )
    .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    std::fs::write(dir.path().join("RULES.md"),"# Rules\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: writing\n    effect: allow\n    actions: [post.create, response.create, post.edit]\naccess:\n  boards: []\n  threads: []\n~~~\n").unwrap();
    let ai = call(
        &mut s,
        "identity.start",
        None,
        json!({"project_id":project}),
        json!({}),
        None,
    )
    .unwrap();
    let token = ai["session_token"].as_str().unwrap();
    let report = |root: &std::path::Path, model: &str| json!({"model_capture":{"status":"available","report":{"client":"generic","session_id":"actual-session","project_root":root,"model_name":model,"engine_name":"Custom Engine","basis":"response"}}});
    let first = call(
        &mut s,
        "post.create",
        Some(token),
        json!({"thread_id":thread,"body":"first"}),
        json!({}),
        Some("first"),
    )
    .unwrap();
    assert_eq!(first["author"]["model_name"], "不明");
    assert_eq!(
        first["author"]["model_capture"]["reason"],
        "client_did_not_supply_capture"
    );
    let mismatch = call(
        &mut s,
        "post.create",
        Some(token),
        json!({"thread_id":thread,"body":"foreign"}),
        report(foreign.path(), "foreign-model"),
        Some("foreign"),
    )
    .unwrap();
    assert_eq!(mismatch["author"]["model_name"], "不明");
    assert_eq!(
        mismatch["author"]["model_capture"]["reason"],
        "report_project_mismatch_or_invalid"
    );
    let args = json!({"thread_id":thread,"body":"captured","to":first["no"]});
    let captured = call(
        &mut s,
        "response.create",
        Some(token),
        args.clone(),
        report(dir.path(), "engine-model"),
        Some("capture"),
    )
    .unwrap();
    assert_eq!(captured["author"]["model_name"], "engine-model");
    assert_eq!(captured["author"]["harness_name"], "Custom Engine");
    assert_eq!(captured["author"]["model_capture"]["status"], "available");
    let replay = call(
        &mut s,
        "response.create",
        Some(token),
        args,
        report(dir.path(), "changed-model"),
        Some("capture"),
    )
    .unwrap();
    assert_eq!(replay["id"], captured["id"]);
    let changed = call(
        &mut s,
        "post.create",
        Some(token),
        json!({"thread_id":thread,"body":"changed"}),
        report(dir.path(), "changed-model"),
        Some("changed"),
    )
    .unwrap_err();
    assert_eq!(changed.code, "MODEL_CHANGED");
    let old = call(
        &mut s,
        "post.get",
        Some(token),
        json!({"thread_id":thread,"no":first["no"]}),
        json!({}),
        None,
    )
    .unwrap();
    assert_eq!(old["author"]["model_name"], "不明");
    let missing = call(
        &mut s,
        "post.create",
        Some(token),
        json!({"thread_id":thread,"body":"not created"}),
        report(dir.path(), "engine-model"),
        Some("missing"),
    )
    .unwrap();
    assert_eq!(missing["no"], 4);
}
