//! One fresh notification-authority slice. No elapsed/VM/old-root operations.
use serde_json::{Value,json};
use sha2::{Digest,Sha256};
use std::{fs::{self,OpenOptions},io::Write,path::Path,process::{Command,Stdio},time::{Duration,Instant,SystemTime,UNIX_EPOCH}};
use swarm_core::protocol::Request;
use swarm_gui_host::GuiHost;
use swarm_platform::{CredentialStore,notifications::{Identity,Registration}};
use swarm_runtime::gui::GuiClient;
const BASE:&str="E:/Swarm-Validation-Host/run-20261021-native-notification-authority";
const ROOT:&str="E:/Swarm-Validation-Host/run-20261021-native-notification-authority/Data";
const CANDIDATE:&str="C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
const POSITIVE:&str="NATIVE_NOTIFY_AUTH_POSITIVE_20261021";
const REVOKED:&str="NATIVE_NOTIFY_AUTH_REVOKED_20261021";
fn sha(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
fn now()->u64{SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()}
fn save(name:&str,value:&Value)->Result<(),Box<dyn std::error::Error>>{let mut f=OpenOptions::new().write(true).create_new(true).open(Path::new(BASE).join(name))?;writeln!(f,"{}",value)?;Ok(())}
fn read(name:&str)->Result<Value,Box<dyn std::error::Error>>{Ok(serde_json::from_slice(&fs::read(Path::new(BASE).join(name))?)?)}
fn request(op:&str,args:Value)->Request{Request{operation:op.into(),auth:None,args,context:json!({}),request_id:None}}
fn fixed_inputs()->Result<(),Box<dyn std::error::Error>>{
 for (name,hash) in [("swarm-desktop.exe","ffbc41afad0c2a29246b7471b95b1068a40b3052d65ae3944e1547693340209a"),("swarm-runtime.exe","0cf3ecb753c20830be7edf2c3630b3cc07aac90655d963b1097374f8b2b24210")]{assert_eq!(sha(&fs::read(Path::new(CANDIDATE).join(name))?),hash,"candidate changed");}
 Ok(())
}
fn fixed_root()->Result<(),Box<dyn std::error::Error>>{assert_eq!(fs::canonicalize(ROOT)?.to_string_lossy().trim_start_matches(r"\\?\").replace('\\',"/"),ROOT);Ok(())}
fn alias()->Result<String,Box<dyn std::error::Error>>{let bytes=CredentialStore::open(&Path::new(ROOT).join("gui-state"))?.get("selection")?.ok_or("selection missing")?;let v:Value=serde_json::from_slice(&bytes)?;Ok(v["alias"].as_str().ok_or("alias missing")?.into())}
fn prepare()->Result<(),Box<dyn std::error::Error>>{
 fixed_inputs()?;
 assert_eq!(fs::canonicalize("E:/Swarm-Validation-Host")?.to_string_lossy().trim_start_matches(r"\\?\").replace('\\',"/"),"E:/Swarm-Validation-Host");
 fs::create_dir(BASE)?; // Existing base refuses; never rename to reset an attempt.
 save("attempt.json",&json!({"scope":"native-current-permission-notification","at_unix_seconds":now(),"attempts":1,"automatic_retries":0}))?;
 fs::create_dir(ROOT)?;fixed_root()?;
 let out=OpenOptions::new().write(true).create_new(true).open(Path::new(BASE).join("runtime.stdout.log"))?;
 let err=OpenOptions::new().write(true).create_new(true).open(Path::new(BASE).join("runtime.stderr.log"))?;
 let mut command=Command::new(Path::new(CANDIDATE).join("swarm-runtime.exe"));
 command.args(["--data-dir",ROOT,"--human-start"]).stdin(Stdio::null()).stdout(out).stderr(err);
 #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
 let child=command.spawn()?;
 save("runtime-start.json",&json!({"pid":child.id(),"fixed_exe":true,"root":ROOT}))?;
 let until=Instant::now()+Duration::from_secs(20);
 loop {if GuiClient::open(Path::new(ROOT)).and_then(|c|c.setup_status()).is_ok(){break}if Instant::now()>=until{return Err("readiness timeout; preserve fixture, no retry".into())}std::thread::sleep(Duration::from_millis(100));}
 let mut host=GuiHost::open(Path::new(ROOT))?;
 let admin_password=uuid::Uuid::new_v4().simple().to_string();
 host.authenticate("setup","NOTIFY_AUTH_ADMIN",&admin_password)?;
 let admin_alias=alias()?;
 let admin=host.snapshot()?["account"].clone();
 save("admin-stage.json",&json!({"admin_alias":admin_alias,"admin_id":admin["id"],"runtime_pid":child.id()}))?;
 finish_unstarted_steps(host,admin_alias,admin,child.id())
}
fn finish_unstarted_steps(mut host:GuiHost,admin_alias:String,admin:Value,runtime_pid:u32)->Result<(),Box<dyn std::error::Error>>{
 let recipient_password=uuid::Uuid::new_v4().simple().to_string();
 let recipient=host.manage_account("account.create",json!({"name":"NOTIFY_AUTH_RECIPIENT","password":recipient_password}))?;
 host.manage_authorization("permission.set",json!({"subject_type":"account","subject_id":recipient["id"],"operation":"read","scope":"global","target_id":"","decision":"allow"}))?;
 let board=host.create("board.create",json!({"name":"NATIVE_NOTIFY_AUTH_BOARD"}))?;host.acknowledge_create()?;
 let thread=host.create("thread.create",json!({"name":"NATIVE_NOTIFY_AUTH_THREAD","board_id":board["id"]}))?;host.acknowledge_create()?;
 host.authenticate("login","NOTIFY_AUTH_RECIPIENT",&recipient_password)?;
 let recipient_alias=alias()?;
 // Fixture setup only; the Thread subscription itself MUST be a separately
 // action-time-confirmed native click. No subscription is created here.
 host.manage_notifications(recipient["id"].as_str().ok_or("recipient id missing")?,"notification.settings.set",json!({"enabled":true,"include_excerpt":true}))?;
 let snapshot=host.snapshot()?;assert_eq!(snapshot["account"]["id"],recipient["id"]);
 let ready=json!({"root":ROOT,"runtime_pid":runtime_pid,"admin_alias":admin_alias,"recipient_alias":recipient_alias,"admin_id":admin["id"],"recipient_id":recipient["id"],"board_id":board["id"],"thread_id":thread["id"],"posts":0,"subscription_created":false,"native_action_time_confirmation_required":true,"fixture_rules_sha256":sha(&fs::read(Path::new(ROOT).join("RULES.md"))?)});
 save("ready.json",&ready)?;println!("Fixture prepared; no subscription/post/delivery yet. Native confirmation required.");Ok(())
}
fn complete_unstarted()->Result<(),Box<dyn std::error::Error>>{
 fixed_inputs()?;fixed_root()?;
 assert!(!Path::new(BASE).join("ready.json").exists());
 let started=read("runtime-start.json")?;
 let admin_alias=alias()?;let client=GuiClient::open(Path::new(ROOT))?;
 let admin=product(&client,&admin_alias,"account.status",json!({}))?;
 assert_eq!(admin["name"],"NOTIFY_AUTH_ADMIN");assert_eq!(admin["is_admin"],true);
 let accounts=product(&client,&admin_alias,"account.list",json!({}))?;
 assert_eq!(accounts["items"].as_array().ok_or("account list missing")?.len(),1,"recipient steps already started; do not replay");
 let boards=product(&client,&admin_alias,"board.list",json!({}))?;
 assert_eq!(boards["items"],json!([]),"Board steps already started; do not replay");
 save("unstarted-steps-continuation.json",&json!({"original_attempt_preserved":true,"cause":"GuiHost.read account.status unavailable; use saved snapshot/private status","runtime_restarted":false,"admin_recreated":false,"unexecuted_steps_only":true,"automatic_retries":0,"at_unix_seconds":now()}))?;
 let pid=u32::try_from(started["pid"].as_u64().ok_or("runtime PID missing")?)?;
 save("admin-stage.json",&json!({"admin_alias":admin_alias,"admin_id":admin["id"],"runtime_pid":pid}))?;
 finish_unstarted_steps(GuiHost::open(Path::new(ROOT))?,admin_alias,admin,pid)
}
fn product(client:&GuiClient,alias:&str,op:&str,args:Value)->Result<Value,Box<dyn std::error::Error>>{Ok(client.request(alias,request(op,args))?)}
fn history()->Result<Value,Box<dyn std::error::Error>>{
 use windows::{UI::Notifications::ToastNotificationManager,Win32::System::WinRT::{RoInitialize,RoUninitialize,RO_INIT_MULTITHREADED},core::HSTRING};
 let identity=Identity::new(Path::new(ROOT),&Path::new(CANDIDATE).join("swarm-desktop.exe"))?;
 unsafe{RoInitialize(RO_INIT_MULTITHREADED)?;}
 let result=(||->Result<Value,Box<dyn std::error::Error>>{
 let entries=ToastNotificationManager::History()?.GetHistoryWithId(&HSTRING::from(identity.app_id()))?;
 let mut positive=0;let mut revoked=0;let mut count=0;
 for entry in entries {let xml=entry.Content()?.GetXml()?.to_string();count+=1;positive+=usize::from(xml.contains(POSITIVE));revoked+=usize::from(xml.contains(REVOKED));}
 Ok(json!({"only_owned_app_id":true,"entries":count,"positive_marker_entries":positive,"revoked_marker_entries":revoked,"at_unix_seconds":now()}))})();
 unsafe{RoUninitialize();}result
}
fn main()->Result<(),Box<dyn std::error::Error>>{
 let args:Vec<_>=std::env::args().skip(1).collect();assert_eq!(args.len(),1,"one explicit mode required");
 if args[0]=="--prepare"{return prepare()}
 if args[0]=="--complete-unstarted-steps-once"{return complete_unstarted()}
 fixed_inputs()?;fixed_root()?;let ready=if args[0]=="--closeout" && !Path::new(BASE).join("ready.json").exists(){read("admin-stage.json")?}else{read("ready.json")?};
 let admin=ready["admin_alias"].as_str().ok_or("admin alias missing")?;
 let recipient=ready["recipient_alias"].as_str().unwrap_or("");
 let thread=ready["thread_id"].as_str().unwrap_or("");
 if args[0]!="--closeout"{assert!(!recipient.is_empty()&&!thread.is_empty(),"ready fixture required");}
 let client=GuiClient::open(Path::new(ROOT))?;
 match args[0].as_str(){
  "--positive-once"=>{
   let approval=read("subscription-native-confirmed.json")?;assert_eq!(approval["native_subscription_enabled"],true);assert_eq!(approval["thread_id"],thread);
   assert_eq!(alias()?,recipient);
   let selected=product(&client,recipient,"account.status",json!({}))?;assert_eq!(selected["id"],ready["recipient_id"]);
   let subscriptions=product(&client,recipient,"conversation.subscriptions",json!({}))?;
   assert!(subscriptions["items"].as_array().ok_or("subscriptions missing")?.iter().any(|i|i["id"]==thread));
   save("positive-attempt.json",&json!({"at_unix_seconds":now(),"retry_allowed":false}))?;
   let post=product(&client,admin,"post.create",json!({"thread_id":thread,"body":POSITIVE}))?;
   save("positive-post.json",&json!({"post_id":post["id"],"no":post["no"],"revision":post["revision"],"body_sha256":sha(POSITIVE.as_bytes()),"at_unix_seconds":now()}))?;
   println!("Positive second-author event created once; observe actual native toast now.");
  }
  "--history-readonly"=>println!("{}",history()?),
  "--revoke-and-post-once"=>{
   let positive=read("positive-native-observed.json")?;assert_eq!(positive["actual_native_positive_observed"],true);
   assert!(history()?["positive_marker_entries"].as_u64().unwrap_or(0)>0,"positive OS history missing; no negative trial");
   save("revoked-attempt.json",&json!({"at_unix_seconds":now(),"retry_allowed":false}))?;
   product(&client,admin,"permission.set",json!({"subject_type":"account","subject_id":ready["recipient_id"],"operation":"read","scope":"thread","target_id":thread,"decision":"deny"}))?;
   let denial=client.request(recipient,request("thread.get",json!({"id":thread}))).unwrap_err();assert!(denial.code=="PERMISSION_DENIED"||denial.code=="NOT_FOUND");
   let post=product(&client,admin,"post.create",json!({"thread_id":thread,"body":REVOKED}))?;
   save("revoked-post.json",&json!({"post_id":post["id"],"no":post["no"],"revision":post["revision"],"body_sha256":sha(REVOKED.as_bytes()),"old_recipient_denial":denial.code,"at_unix_seconds":now()}))?;
   println!("Same recipient denied; revoked second-author event created once. Observe bounded native/history interval.");
  }
  "--settled-readonly"=>{
   let revoked=read("revoked-post.json")?;assert!(now()>=revoked["at_unix_seconds"].as_u64().ok_or("revoked time missing")?+10,"normal delivery interval not elapsed");
   let denial=client.request(recipient,request("thread.get",json!({"id":thread}))).unwrap_err();assert!(denial.code=="PERMISSION_DENIED"||denial.code=="NOT_FOUND");
   let db=rusqlite::Connection::open_with_flags(Path::new(ROOT).join("swarm.sqlite3"),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
   let pending:i64=db.query_row("SELECT COUNT(*) FROM notification_outbox WHERE account_id=?1",[ready["recipient_id"].as_str().ok_or("recipient id missing")?],|r|r.get(0))?;
   let h=history()?;assert_eq!(h["revoked_marker_entries"],0);assert_eq!(pending,0);
   save("settled-product.json",&json!({"old_recipient_denial":denial.code,"owned_history":h,"pending_outbox":pending,"same_recipient_alias":alias()?==recipient,"native_negative_separate":true,"formal_ac_accepted":false}))?;
   println!("Current denial, owned OS history absence and zero pending outbox confirmed; native observation remains separate.");
  }
  "--closeout"=>{
   let mut output=OpenOptions::new().write(true).create_new(true).open(Path::new(BASE).join("closeout.json"))?;
   let account=product(&client,admin,"account.status",json!({}))?;assert_eq!(account["id"],ready["admin_id"]);assert_eq!(account["is_admin"],true);
   client.stop(admin)?;
   let identity=Identity::new(Path::new(ROOT),&Path::new(CANDIDATE).join("swarm-desktop.exe"))?;
   let registration=Registration::new(identity)?;
   // Preserve native entries for review; remove only this fixture registration.
   registration.remove()?;assert!(!registration.is_registered()?);
   writeln!(output,"{}",json!({"normal_stop_accepted":true,"owned_registration_absent":true,"history_not_cleared":true,"retry_allowed":false}))?;
   println!("Owned normal stop accepted and registration absent; verify process exits before reading streams.");
  }
  _=>return Err("unknown mode; refused".into())
 }Ok(())
}
