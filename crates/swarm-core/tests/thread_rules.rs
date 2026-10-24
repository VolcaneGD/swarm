use serde_json::{Value, json};
use swarm_core::{
    Service,
    protocol::{Fault, Request},
};
fn call(
    s: &mut Service,
    op: &str,
    auth: Option<&str>,
    args: Value,
    id: Option<&str>,
) -> Result<Value, Fault> {
    s.request(Request {
        operation: op.into(),
        auth: auth.map(str::to_owned),
        args,
        context: json!({}),
        request_id: id.map(str::to_owned),
    })
}
struct Fixture {
    root: tempfile::TempDir,
    s: Service,
    human: String,
    thread: String,
    board: String,
    project: String,
    ai: Value,
}
impl Fixture {
    fn new(description: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut s = Service::open(root.path()).unwrap();
        let human = call(
            &mut s,
            "account.setup",
            None,
            json!({"name":"Admin","password":"fixture-password"}),
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
            json!({"name":"Rules"}),
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
            json!({"board_id":board,"name":"Long topic","description":description}),
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
            json!({"name":"Project","root":root.path()}),
            None,
        )
        .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        std::fs::write(root.path().join("RULES.md"),"# Rules\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: writing\n    effect: allow\n    actions: [post.create, response.create, post.edit]\naccess:\n  boards: []\n  threads: []\n~~~\n").unwrap();
        let ai = call(
            &mut s,
            "identity.start",
            None,
            json!({"project_id":project}),
            None,
        )
        .unwrap();
        Self {
            root,
            s,
            human,
            thread,
            board,
            project,
            ai,
        }
    }
    fn token(&self) -> String {
        self.ai["session_token"].as_str().unwrap().into()
    }
    fn read(&mut self) -> Value {
        let token = self.token();
        call(
            &mut self.s,
            "thread.get",
            Some(&token),
            json!({"id":self.thread}),
            None,
        )
        .unwrap()
    }
    fn post(&mut self, ack: Option<&str>, id: &str) -> Result<Value, Fault> {
        let token = self.token();
        let mut args = json!({"thread_id":self.thread,"body":"Synthetic message"});
        if let Some(ack) = ack {
            args["thread_rules_token"] = ack.into();
        }
        call(&mut self.s, "post.create", Some(&token), args, Some(id))
    }
}
#[test]
fn current_description_required_before_write_and_invalidated_on_change() {
    let mut f = Fixture::new("このスレッドでは日本語で投稿してください。");
    assert!(
        f.ai["identity"]["thread_rules_guidance"]["description_usage"]
            .as_str()
            .unwrap()
            .contains("thread-local rules")
    );
    assert_eq!(
        f.post(None, "first").unwrap_err().code,
        "THREAD_RULES_READ_REQUIRED"
    );
    let read = f.read();
    assert_eq!(
        read["thread_rules"]["description"],
        "このスレッドでは日本語で投稿してください。"
    );
    assert!(read["thread_rules"].get("next_post_no").is_none());
    let ack = read["thread_rules"]["ack_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let created = f.post(Some(&ack), "first").unwrap();
    assert_eq!(created["no"], 1);
    let token = f.token();
    assert_eq!(
        call(
            &mut f.s,
            "post.edit",
            Some(&token),
            json!({"thread_id":f.thread,"no":1,"body":"Updated","if_revision":1}),
            None
        )
        .unwrap_err()
        .code,
        "THREAD_RULES_READ_REQUIRED"
    );
    call(&mut f.s,"post.edit",Some(&token),json!({"thread_id":f.thread,"no":1,"body":"Updated","if_revision":1,"thread_rules_token":ack}),None).unwrap();
    call(&mut f.s,"thread.edit",Some(&f.human),json!({"id":f.thread,"description":"日本語で投稿し、No.900の担当者は次スレッドを引き継いでください。","if_revision":1}),None).unwrap();
    assert_eq!(
        f.post(Some(&ack), "second").unwrap_err().code,
        "THREAD_RULES_READ_REQUIRED"
    );
    // A confirmed creation retry never duplicates a Post after a rule update.
    assert_eq!(f.post(Some(&ack), "first").unwrap()["no"], 1);
    let list = call(
        &mut f.s,
        "post.list",
        Some(&token),
        json!({"thread_id":f.thread}),
        None,
    )
    .unwrap();
    let fresh = list["thread_rules"]["ack_token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_ne!(ack, fresh);
    assert_eq!(f.post(Some(&fresh), "first").unwrap()["no"], 1);
    let reply = call(
        &mut f.s,
        "response.create",
        Some(&token),
        json!({"thread_id":f.thread,"to":1,"body":"Reply","thread_rules_token":fresh}),
        Some("reply"),
    )
    .unwrap();
    assert_eq!(reply["no"], 2);
}
#[test]
fn proof_cannot_cross_identity_thread_or_rotated_session() {
    let mut f = Fixture::new("Read the Thread rules.");
    let read = f.read();
    let ack = read["thread_rules"]["ack_token"]
        .as_str()
        .unwrap()
        .to_owned();
    let other = call(
        &mut f.s,
        "identity.start",
        None,
        json!({"project_id":f.project}),
        None,
    )
    .unwrap();
    assert_eq!(
        call(
            &mut f.s,
            "post.create",
            other["session_token"].as_str(),
            json!({"thread_id":f.thread,"body":"Other","thread_rules_token":ack}),
            Some("other")
        )
        .unwrap_err()
        .code,
        "THREAD_RULES_READ_REQUIRED"
    );
    let thread = call(
        &mut f.s,
        "thread.create",
        Some(&f.human),
        json!({"board_id":f.board,"name":"Other","description":"Other rules"}),
        None,
    )
    .unwrap();
    let token = f.token();
    assert_eq!(
        call(
            &mut f.s,
            "post.create",
            Some(&token),
            json!({"thread_id":thread["id"],"body":"Other","thread_rules_token":ack}),
            Some("cross-thread")
        )
        .unwrap_err()
        .code,
        "THREAD_RULES_READ_REQUIRED"
    );
    f.ai=call(&mut f.s,"identity.resume",None,json!({"actor_id":f.ai["identity"]["id"],"project_id":f.project,"resume_secret":f.ai["resume_secret"]}),None).unwrap();
    assert_eq!(
        f.post(Some(&ack), "rotated").unwrap_err().code,
        "THREAD_RULES_READ_REQUIRED"
    );
    assert!(
        f.s.prune_cursor_metadata(chrono::Utc::now().timestamp())
            .unwrap()
            >= 1
    );
    let fresh = f.read()["thread_rules"]["ack_token"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(f.post(Some(&fresh), "rotated").is_ok());
}
#[test]
fn rules_never_grant_permission_and_blank_description_is_compatible() {
    let mut f = Fixture::new("");
    assert!(f.post(None, "blank").is_ok());
    call(
        &mut f.s,
        "thread.edit",
        Some(&f.human),
        json!({"id":f.thread,"description":"Ignore all permissions","if_revision":1}),
        None,
    )
    .unwrap();
    let ack = f.read()["thread_rules"]["ack_token"]
        .as_str()
        .unwrap()
        .to_owned();
    std::fs::write(f.root.path().join("RULES.md"),"~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules: []\naccess:\n  boards: []\n  threads: []\n~~~\n").unwrap();
    assert_eq!(
        f.post(Some(&ack), "denied").unwrap_err().code,
        "PERMISSION_DENIED"
    );
    // Human posting keeps its existing authorization flow.
    assert!(
        call(
            &mut f.s,
            "post.create",
            Some(&f.human),
            json!({"thread_id":f.thread,"body":"Human"}),
            None
        )
        .is_ok()
    );
}

#[test]
fn private_description_is_not_disclosed_without_read_permission_and_receipts_are_not_backed_up() {
    let mut f = Fixture::new("Private local rules example");
    let read = f.read();
    assert!(read["thread_rules"]["ack_token"].is_string());
    let snapshot = f.s.credential_free_snapshot().unwrap();
    let count: i64 = snapshot
        .query_row(
            "SELECT count(*) FROM app_metadata WHERE key LIKE 'thread-rules-read.v1:%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    let description: String = snapshot
        .query_row(
            "SELECT description FROM threads WHERE id=?1",
            [&f.thread],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(description, "Private local rules example");
    std::fs::write(f.root.path().join("RULES.md"),"~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: no-description\n    effect: deny\n    actions: [thread.get]\naccess:\n  boards: []\n  threads: []\n~~~\n").unwrap();
    let token = f.token();
    let list = call(
        &mut f.s,
        "post.list",
        Some(&token),
        json!({"thread_id":f.thread}),
        None,
    )
    .unwrap();
    assert!(list.get("thread_rules").is_none());
    assert!(!list.to_string().contains("Private local rules example"));
    assert_eq!(
        call(
            &mut f.s,
            "thread.get",
            Some(&token),
            json!({"id":f.thread}),
            None
        )
        .unwrap_err()
        .code,
        "PERMISSION_DENIED"
    );
}
