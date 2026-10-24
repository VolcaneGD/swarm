//! New soft-deletion/public-history contract only. No VM, elapsed root, or GUI.
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::{fs::{self,OpenOptions},io::Write,path::Path,process::{Command,Stdio,Child},time::{Duration,Instant},sync::mpsc};
use swarm_core::protocol::Request;
use swarm_runtime::gui::GuiClient;
const BASE:&str="E:/Swarm-Validation-Host/run-20261021-public-history-deletion";
const ROOT:&str="E:/Swarm-Validation-Host/run-20261021-public-history-deletion/Data";
const CANDIDATE:&str="C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const ALIAS:&str="public-history-deletion-20261021";
const ORIGINAL:&str="SYNTHETIC_HISTORY_ORIGINAL_20261021";
const CURRENT:&str="SYNTHETIC_HISTORY_CURRENT_20261021";
type Result<T>=std::result::Result<T,Box<dyn std::error::Error>>;
fn ensure(ok:bool,message:&str)->Result<()>{if ok{Ok(())}else{Err(message.into())}}
fn hash(b:&[u8])->String{format!("{:x}",Sha256::digest(b))}
fn save(name:&str,v:&Value)->Result<()>{let mut f=OpenOptions::new().create_new(true).write(true).open(Path::new(BASE).join(name))?;writeln!(f,"{v}")?;Ok(())}
fn inputs()->Result<()>{for(n,h)in[("swarm-runtime.exe","0cf3ecb753c20830be7edf2c3630b3cc07aac90655d963b1097374f8b2b24210"),("swarm.exe","d23b7973d229bea186d4aec95eca3a6f7f2868f7243534156372e5e438ecfd33")]{ensure(hash(&fs::read(Path::new(CANDIDATE).join(n))?)==h,"Frozen candidate changed")?}Ok(())}
fn call(alias:&str,op:&str,args:Value)->Result<Value>{
 let(tx,rx)=mpsc::channel();let alias=alias.to_owned();let op=op.to_owned();
 std::thread::spawn(move||{let r=GuiClient::open(Path::new(ROOT)).and_then(|c|c.request(&alias,Request{operation:op,auth:None,args,context:json!({}),request_id:None})).map_err(|e|e.code.to_string());let _=tx.send(r);});
 Ok(rx.recv_timeout(Duration::from_secs(20)).map_err(|_|"Uncertain private request; preserve state, no retry")?.map_err(|e|format!("Private operation refused: {e}"))?)
}
fn selected()->Result<String>{let bytes=swarm_platform::CredentialStore::open(&Path::new(ROOT).join("gui-state"))?.get("selection")?.ok_or("Selected alias missing")?;let v:Value=serde_json::from_slice(&bytes)?;Ok(v["alias"].as_str().ok_or("Alias missing")?.into())}
fn cli(project:&str,args:&[&str],expected:i32,label:&str)->Result<Value>{
 let mut c=Command::new(Path::new(CANDIDATE).join("swarm.exe"));c.args(["--data-dir",ROOT,"--project",project,"--session",ALIAS,"--format","json"]).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
 {use std::os::windows::process::CommandExt;c.creation_flags(0x08000000);}let mut child=c.spawn()?;let until=Instant::now()+Duration::from_secs(20);
 loop{if child.try_wait()?.is_some(){break}if Instant::now()>=until{save("uncertain-cli.json",&json!({"operation":label,"pid":child.id(),"retry_allowed":false}))?;return Err("CLI deadline; preserve child and state".into())}std::thread::sleep(Duration::from_millis(20));}
 let o=child.wait_with_output()?;let v:Value=serde_json::from_slice(&o.stdout).unwrap_or(Value::Null);
 save(&format!("cli-{label}.json"),&json!({"operation":label,"expected_exit":expected,"actual_exit":o.status.code(),"stdout_bytes":o.stdout.len(),"stderr_bytes":o.stderr.len(),"error_code":v["error"]["code"],"items_count":v["data"]["items"].as_array().map(Vec::len),"raw_output_saved":false}))?;
 ensure(o.status.code()==Some(expected)&&o.stderr.is_empty()&&v["schema_version"]==1&&v["ok"]==(expected==0),"Public CLI contract mismatch; see safe receipt")?;
 Ok(if expected==0{v["data"].clone()}else{v["error"].clone()})
}
fn history_ok(v:&Value)->bool{v["items"].as_array().is_some_and(|a|a.len()==1&&a[0]["revision"]==1&&a[0]["body"].as_str().is_some_and(|b|hash(b.as_bytes())==hash(ORIGINAL.as_bytes())))}
fn scenario()->Result<()>{
 inputs()?;fs::create_dir(BASE)?;save("attempt.json",&json!({"scope":"public-soft-deleted-history-explicit-grant","automatic_retries":0,"vm":false,"elapsed":false,"publication":false}))?;
 fs::create_dir(ROOT)?;
 // Only this fresh root; prevent accidental startup registration, no clock/settings seed.
 swarm_runtime::startup_preference_set(Path::new(ROOT),false)?;
 let out=OpenOptions::new().create_new(true).write(true).open(Path::new(BASE).join("runtime.stdout.log"))?;let err=OpenOptions::new().create_new(true).write(true).open(Path::new(BASE).join("runtime.stderr.log"))?;
 let mut c=Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));c.args(["--data-dir",ROOT,"--human-start"]).stdin(Stdio::null()).stdout(out).stderr(err);{use std::os::windows::process::CommandExt;c.creation_flags(0x08000000);}
 let mut runtime=c.spawn()?;save("runtime.json",&json!({"pid":runtime.id()}))?;
 let until=Instant::now()+Duration::from_secs(20);loop{if GuiClient::open(Path::new(ROOT)).and_then(|c|c.setup_status()).is_ok(){break}ensure(Instant::now()<until,"Runtime readiness deadline")?;std::thread::sleep(Duration::from_millis(50));}
 let mut host=swarm_gui_host::GuiHost::open(Path::new(ROOT))?;
 host.authenticate("setup","PUBLIC_HISTORY_FIXTURE_ADMIN",&uuid::Uuid::new_v4().simple().to_string())?;
 let admin=selected()?;let board=host.create("board.create",json!({"name":"PUBLIC_HISTORY_BOARD"}))?;host.acknowledge_create()?;
 let thread=host.create("thread.create",json!({"name":"PUBLIC_HISTORY_THREAD","board_id":board["id"]}))?;host.acknowledge_create()?;
 let post=host.create("post.create",json!({"thread_id":thread["id"],"body":ORIGINAL}))?;host.acknowledge_create()?;
 let t=thread["id"].as_str().ok_or("Thread ID missing")?;let no=post["no"].as_i64().ok_or("Post No missing")?;let n=no.to_string();
 call(&admin,"post.edit",json!({"thread_id":t,"no":no,"body":CURRENT,"if_revision":1}))?;
 let rules=format!("# Synthetic history permission fixture\n\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: only-fixture-history\n    effect: allow\n    actions: [post.history]\n    scope:\n      threads: [{t}]\naccess:\n  boards: []\n  threads: []\n~~~\n");
 fs::write(Path::new(ROOT).join("RULES.md"),&rules)?;
 fs::create_dir(Path::new(BASE).join("Project"))?;
 let project=call(&admin,"project.register",json!({"name":"Public history deletion","root":format!("{BASE}/Project")}))?;let p=project["id"].as_str().ok_or("Project ID missing")?;
 save("fixture.json",&json!({"thread_id":t,"post_id":post["id"],"post_no":no,"project_id":p,"rules_sha256":hash(rules.as_bytes()),"retention_settings_changed":false,"clock_changed":false}))?;
 cli(p,&["identity","start"],0,"identity-start")?;
 ensure(history_ok(&cli(p,&["post","history","--thread-id",t,"--no",&n],0,"live-history")?),"Positive public history mismatch")?;
 call(&admin,"post.delete",json!({"thread_id":t,"no":no}))?;
 ensure(cli(p,&["post","history","--thread-id",t,"--no",&n],0,"deleted-history")?["items"]==json!([]),"Deleted public history not empty")?;
 let deleted=cli(p,&["post","get","--thread-id",t,"--no",&n],0,"deleted-skeleton")?;
 ensure(deleted["deleted"]==true&&deleted["body"].is_null()&&deleted["restore_available"]==true&&deleted["id"]==post["id"],"Soft-deleted public skeleton mismatch")?;
 ensure(history_ok(&call(&admin,"post.history",json!({"thread_id":t,"no":no}))?),"Private retained history control failed")?;
 call(&admin,"post.restore",json!({"thread_id":t,"no":no}))?;
 ensure(history_ok(&cli(p,&["post","history","--thread-id",t,"--no",&n],0,"restored-history")?),"Restored public history mismatch")?;
 cli(p,&["identity","release"],0,"identity-release")?;
 GuiClient::open(Path::new(ROOT))?.stop(&admin)?;
 wait_exit(&mut runtime)?;
 let streams=["runtime.stdout.log","runtime.stderr.log"].map(|s|fs::metadata(Path::new(BASE).join(s)).map(|m|m.len())).into_iter().collect::<std::io::Result<Vec<_>>>()?;
 ensure(streams==[0,0],"Closed runtime stream nonempty")?;
 save("result.json",&json!({"status":"PUBLIC_SOFT_DELETE_HISTORY_PASS_NOT_NATIVE_OR_ELAPSED","same_identity_all_history_checks":true,"live_history_items":1,"deleted_history_exit":0,"deleted_history_items":0,"private_deleted_history_items":1,"restored_history_items":1,"identity_released":true,"runtime_exited":true,"runtime_exit":0,"closed_stream_bytes":streams,"state":swarm_runtime::lifecycle::status(Path::new(ROOT))?,"native_accepted":false,"elapsed_accepted":false}))?;Ok(())
}
fn wait_exit(child:&mut Child)->Result<()>{let until=Instant::now()+Duration::from_secs(20);loop{if let Some(s)=child.try_wait()?{return ensure(s.success(),"Runtime exit failure")}ensure(Instant::now()<until,"Runtime stop deadline; no force")?;std::thread::sleep(Duration::from_millis(50));}}
fn closeout()->Result<()>{
 inputs()?;ensure(Path::new(BASE).join("failure.json").is_file(),"Failed fixture required for closeout")?;
 let fixture:Value=serde_json::from_slice(&fs::read(Path::new(BASE).join("fixture.json"))?)?;let p=fixture["project_id"].as_str().ok_or("Existing project missing")?;
 save("closeout-attempt.json",&json!({"acceptance_retry":false,"automatic_retries":0,"restoration":false}))?;
 cli(p,&["identity","release"],0,"closeout-identity-release")?;
 GuiClient::open(Path::new(ROOT))?.stop(&selected()?)?;
 save("closeout-stop.json",&json!({"identity_released":true,"normal_stop_accepted":true,"state":swarm_runtime::lifecycle::status(Path::new(ROOT))?,"runtime_exit_separate":true}))?;Ok(())
}
#[link(name="kernel32")]unsafe extern "system"{fn OpenProcess(access:u32,inherit:i32,pid:u32)->isize;fn WaitForSingleObject(handle:isize,millis:u32)->u32;fn GetExitCodeProcess(handle:isize,code:*mut u32)->i32;fn CloseHandle(handle:isize)->i32;}
struct OwnedRuntime(isize);impl Drop for OwnedRuntime{fn drop(&mut self){unsafe{CloseHandle(self.0);}}}
impl OwnedRuntime{fn open(pid:u32)->Result<Self>{let h=unsafe{OpenProcess(0x00101000,0,pid)};ensure(h!=0,"Existing owned runtime handle unavailable")?;Ok(Self(h))}fn exited(&self)->Result<()>{ensure(unsafe{WaitForSingleObject(self.0,20000)}==0,"Owned runtime exit deadline; no force")?;let mut code=0;ensure(unsafe{GetExitCodeProcess(self.0,&mut code)}!=0&&code==0,"Runtime normal exit not confirmed")}}
fn retry_corrected()->Result<()>{
 let resume_argument_repair=std::env::args().any(|s|s=="--retry-resume-arguments");
 let continue_unstarted=resume_argument_repair||std::env::args().any(|s|s=="--retry-continue-unstarted");
 inputs()?;
 let approval:Value=serde_json::from_slice(&fs::read(Path::new(BASE).join("retry-1-approval.json"))?)?;
 ensure(approval["user_answer"]=="再実行してください。"&&approval["scope"]=="same-fixture-public-history-retry-1"&&approval["vm"]==false&&approval["elapsed"]==false,"Exact current public-history retry authority missing")?;
 let before:Value=serde_json::from_slice(&fs::read(Path::new(BASE).join("host-closeout.json"))?)?;
 ensure(before["runtime_exit_observed_by_host"]==true&&before["identity_released"]==true,"Prior closeout not confirmed")?;
 let f:Value=serde_json::from_slice(&fs::read(Path::new(BASE).join("fixture.json"))?)?;
 ensure(hash(&fs::read(Path::new(ROOT).join("RULES.md"))?)==f["rules_sha256"],"Existing scoped RULES changed")?;
 ensure(swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"]==if continue_unstarted{"running"}else{"stopped"},"Unexpected existing runtime state")?;
 let p=f["project_id"].as_str().ok_or("Existing Project missing")?;let t=f["thread_id"].as_str().ok_or("Thread missing")?;let no=f["post_no"].as_i64().ok_or("Post No missing")?;let n=no.to_string();
 let store=swarm_platform::CredentialStore::open(&Path::new(ROOT).join("client-credentials"))?;
 let saved:Value=serde_json::from_slice(&store.get(ALIAS)?.ok_or("Existing alias missing")?)?;
 ensure(saved["project_id"]==f["project_id"],"Saved alias Project mismatch")?;let actor=saved["id"].clone();drop(store);
 let mut runtime=None;
 if continue_unstarted{
  let fail:Value=serde_json::from_slice(&fs::read(Path::new(BASE).join("retry-1-failure.json"))?)?;
  ensure(fail["reason"]=="Original soft-deleted Post state mismatch","Different failure cannot use unstarted continuation")?;
  for name in ["retry-1-resumed.json","cli-retry1-deleted-history.json","retry-1-result.json"]{ensure(!Path::new(BASE).join(name).exists(),"Public read steps already started; no replay")?;}
  if resume_argument_repair{
   let parse:Value=serde_json::from_slice(&fs::read(Path::new(BASE).join("cli-retry1-resume.json"))?)?;
   ensure(parse["actual_exit"]==2&&parse["error_code"]=="INVALID_INPUT","Resume did not fail in argument validation")?;
   save("retry-1-resume-argument-repair.json",&json!({"missing_argument":"--actor-id","same_runtime":true,"new_attempt":false,"public_read_steps_previously_executed":false}))?;
  }else{
   ensure(!Path::new(BASE).join("cli-retry1-resume.json").exists(),"Resume already attempted")?;
   save("retry-1-unstarted-continuation.json",&json!({"reason":"Post delete increments revision; remove incorrect edit-only revision assumption","runtime_restarted":false,"fixture_recreated":false,"public_steps_previously_executed":false}))?;
  }
 }else{
  save("retry-1-attempt.json",&json!({"user_answer":"再実行してください。","existing_data":true,"existing_project_alias":true,"automatic_retries":0,"vm":false,"elapsed":false}))?;
  let out=OpenOptions::new().create_new(true).write(true).open(Path::new(BASE).join("retry-1-runtime.stdout.log"))?;let err=OpenOptions::new().create_new(true).write(true).open(Path::new(BASE).join("retry-1-runtime.stderr.log"))?;
  let mut c=Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));c.args(["--data-dir",ROOT,"--human-start"]).stdin(Stdio::null()).stdout(out).stderr(err);{use std::os::windows::process::CommandExt;c.creation_flags(0x08000000);}let child=c.spawn()?;
  save("retry-1-runtime.json",&json!({"pid":child.id()}))?;runtime=Some(child);
 }
 let started:Value=serde_json::from_slice(&fs::read(Path::new(BASE).join("retry-1-runtime.json"))?)?;let owned=OwnedRuntime::open(started["pid"].as_u64().ok_or("Owned PID missing")? as u32)?;
 let until=Instant::now()+Duration::from_secs(20);loop{if GuiClient::open(Path::new(ROOT)).and_then(|c|c.setup_status()).is_ok(){break}ensure(Instant::now()<until,"Retry runtime readiness deadline")?;std::thread::sleep(Duration::from_millis(50));}
 let admin=selected()?;let account=call(&admin,"account.status",json!({}))?;ensure(account["name"]=="PUBLIC_HISTORY_FIXTURE_ADMIN"&&account["is_admin"]==true,"Selected original Admin mismatch")?;
 let deleted=call(&admin,"post.get",json!({"thread_id":t,"no":no}))?;
 ensure(deleted["id"]==f["post_id"]&&deleted["deleted"]==true&&deleted["restore_available"]==true,"Original soft-deleted Post state mismatch")?;
 ensure(call(&admin,"post.deleted_body",json!({"thread_id":t,"no":no}))?["body"].as_str().is_some_and(|s|hash(s.as_bytes())==hash(CURRENT.as_bytes())),"Original current body hash changed")?;
 ensure(history_ok(&call(&admin,"post.history",json!({"thread_id":t,"no":no}))?),"Retained private history control failed")?;
 cli(p,&["identity","resume","--actor-id",actor.as_str().ok_or("Actor ID missing")?],0,if resume_argument_repair{"retry1-resume-corrected-args"}else{"retry1-resume"})?;
 let store=swarm_platform::CredentialStore::open(&Path::new(ROOT).join("client-credentials"))?;let resumed:Value=serde_json::from_slice(&store.get(ALIAS)?.ok_or("Resumed alias missing")?)?;ensure(resumed["id"]==actor&&resumed["project_id"]==f["project_id"],"Immutable AI/Project changed during resume")?;drop(store);
 save("retry-1-resumed.json",&json!({"same_ai_id":true,"same_project_alias":true,"new_token_expected":true}))?;
 ensure(cli(p,&["post","history","--thread-id",t,"--no",&n],0,"retry1-deleted-history")?["items"]==json!([]),"AI deleted history not empty")?;
 let skeleton=cli(p,&["post","get","--thread-id",t,"--no",&n],0,"retry1-skeleton")?;
 ensure(skeleton["deleted"]==true&&skeleton["body"].is_null()&&skeleton["restore_available"]==true&&skeleton["id"]==f["post_id"],"Public skeleton/retained state mismatch")?;
 call(&admin,"post.restore",json!({"thread_id":t,"no":no}))?;
 ensure(history_ok(&cli(p,&["post","history","--thread-id",t,"--no",&n],0,"retry1-restored-history")?),"Restored AI history not preserved")?;
 cli(p,&["identity","release"],0,"retry1-release")?;GuiClient::open(Path::new(ROOT))?.stop(&admin)?;owned.exited()?;if let Some(ref mut child)=runtime{wait_exit(child)?;}
 let lengths=[fs::metadata(Path::new(BASE).join("retry-1-runtime.stdout.log"))?.len(),fs::metadata(Path::new(BASE).join("retry-1-runtime.stderr.log"))?.len()];ensure(lengths==[0,0],"Closed retry runtime stream nonempty")?;
 save("retry-1-result.json",&json!({"status":"PUBLIC_SOFT_DELETED_HISTORY_PASS_NOT_ELAPSED_OR_NATIVE","same_project_ai_alias":true,"private_retained_history_items":1,"public_deleted_history_items":0,"public_restored_history_items":1,"post_restored_same_id":true,"identity_released":true,"runtime_exited":true,"closed_stream_bytes":lengths,"rules_sha256_unchanged":hash(&fs::read(Path::new(ROOT).join("RULES.md"))?)==f["rules_sha256"],"state":swarm_runtime::lifecycle::status(Path::new(ROOT))?}))?;Ok(())
}
#[cfg(test)]mod contract_tests{
 use super::*;
 #[test]fn resume_cli_contract_requires_explicit_existing_actor(){
  let args=["--data-dir",ROOT,"--project","0c5749ad-5af7-4840-bb94-f3ea6ad5dd5f","--session",ALIAS,"identity","resume"];
  let incomplete:Vec<std::ffi::OsString>=args.into_iter().map(Into::into).collect();
  assert!(swarm_cli::parse_args(&incomplete,&mut std::io::empty()).is_err());
  let mut complete=incomplete;complete.extend(["--actor-id".into(),"48a29a1d-fb6c-49d4-949e-336549b604ef".into()]);
  assert!(swarm_cli::parse_args(&complete,&mut std::io::empty()).is_ok());
 }
 #[test]fn human_retained_and_public_ai_deleted_history_have_different_contracts(){
  let dir=tempfile::tempdir().unwrap();let project=tempfile::tempdir().unwrap();
  fs::write(dir.path().join("RULES.md"),"# fixture\n~~~swarm-rules\nschema_version: 1\ndefaults:\n  ai_read: true\nrules:\n  - id: explicit-history\n    effect: allow\n    actions: [post.history]\naccess:\n  boards: []\n  threads: []\n~~~\n").unwrap();
  let mut s=swarm_core::Service::open(dir.path()).unwrap();
  fn req(s:&mut swarm_core::Service,op:&str,token:Option<&str>,args:Value)->Value{s.request(Request{operation:op.into(),auth:token.map(str::to_owned),args,context:json!({}),request_id:None}).unwrap()}
  let a=req(&mut s,"account.setup",None,json!({"name":"ContractAdmin","password":""}))["token"].as_str().unwrap().to_owned();
  let b=req(&mut s,"board.create",Some(&a),json!({"name":"B"}));let t=req(&mut s,"thread.create",Some(&a),json!({"name":"T","board_id":b["id"]}));
  let post=req(&mut s,"post.create",Some(&a),json!({"thread_id":t["id"],"body":ORIGINAL}));let args=json!({"thread_id":t["id"],"no":post["no"]});
  req(&mut s,"post.edit",Some(&a),json!({"thread_id":t["id"],"no":post["no"],"body":CURRENT,"if_revision":1}));
  let p=req(&mut s,"project.register",Some(&a),json!({"name":"P","root":project.path()}));let ai=req(&mut s,"identity.start",None,json!({"project_id":p["id"]}))["session_token"].as_str().unwrap().to_owned();
  assert!(history_ok(&req(&mut s,"post.history",Some(&ai),args.clone())));
  assert_eq!(req(&mut s,"post.delete",Some(&a),args.clone())["revision"],3);
  assert_eq!(req(&mut s,"post.history",Some(&ai),args.clone())["items"],json!([]));
  assert!(history_ok(&req(&mut s,"post.history",Some(&a),args.clone())));
  req(&mut s,"post.restore",Some(&a),args.clone());
  assert!(history_ok(&req(&mut s,"post.history",Some(&ai),args)));
 }
}
fn main(){let args:Vec<_>=std::env::args().skip(1).collect();let cleanup=args==["--closeout-only"];let repairing=args==["--retry-resume-arguments"];let continuing=args==["--retry-continue-unstarted"]||repairing;let retry=args==["--retry-approved-once"]||continuing;if !cleanup&&!retry&&args!=["--execute-once"]{eprintln!("Explicit single mode required");std::process::exit(2)}if !cleanup&&!retry&&Path::new(BASE).exists(){eprintln!("Existing fixture; no retry");std::process::exit(1)}let deadline=Instant::now()+Duration::from_secs(180);let(tx,rx)=mpsc::channel();std::thread::spawn(move||{let r=(if cleanup{closeout()}else if retry{retry_corrected()}else{scenario()}).map_err(|e|e.to_string());let _=tx.send(r);});let result=rx.recv_timeout(deadline.saturating_duration_since(Instant::now()));match result{Ok(Ok(()))=>println!("Fixed fixture operation completed; inspect scoped result/closeout."),other=>{let reason=match other{Ok(Err(s))=>s,_=>"Overall deadline; preserve child/state, no retry".into()};if Path::new(BASE).is_dir(){let _=save(if cleanup{"closeout-failure.json"}else if repairing{"retry-1-resume-arguments-failure.json"}else if continuing{"retry-1-continuation-failure.json"}else if retry{"retry-1-failure.json"}else{"failure.json"},&json!({"reason":reason,"retry_allowed":false}));}eprintln!("Public-history fixture stopped; see safe failure receipt, no retry.");std::process::exit(1)}}}
