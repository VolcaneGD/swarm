//! Fixed disposable-guest probe. Does not reboot, simulate boot, or accept native UI.
use serde_json::{Value,json};
use std::{fs::{self,OpenOptions},io::Write,path::Path,process::{Command,Stdio},time::{Duration,Instant}};
use sha2::{Digest,Sha256};
#[path="../session.rs"] mod session;
const BASE:&str="C:/Swarm-Validation/Runs/startup-20261021";
const ROOT:&str="C:/Swarm-Validation/Runs/startup-20261021/Data";
const STAGE:&str="C:/Swarm-Validation/Inputs/startup-20261021";
const CONTINUATION:&str="C:/Swarm-Validation/Runs/startup-20261021/native-20261023";
type Result<T> = std::result::Result<T,Box<dyn std::error::Error>>;
fn require(ok:bool,msg:&str)->Result<()>{if !ok {return Err(msg.into())}Ok(())}
fn continuing()->bool{std::env::args().nth(1).as_deref()==Some("--continue")}
fn save(name:&str,v:&Value)->Result<()>{let mut f=OpenOptions::new().write(true).create_new(true).open(Path::new(if continuing(){CONTINUATION}else{BASE}).join(name))?;writeln!(f,"{v}")?;Ok(())}
fn read(name:&str)->Result<Value>{let root=if continuing()&&name!="bootstrap.json"{CONTINUATION}else{BASE};Ok(serde_json::from_slice(&fs::read(Path::new(root).join(name))?)?)}
fn inputs()->Result<()>{
 require(std::env::var("COMPUTERNAME")?.eq_ignore_ascii_case("DESKTOP-1TAP8BI"),"Wrong guest hostname")?;
 require(std::env::var("USERNAME")?.eq_ignore_ascii_case("GPT"),"Wrong guest username")?;
 for(name,hash)in[("swarm-desktop.exe","ffbc41afad0c2a29246b7471b95b1068a40b3052d65ae3944e1547693340209a"),("swarm-runtime.exe","0cf3ecb753c20830be7edf2c3630b3cc07aac90655d963b1097374f8b2b24210"),("swarm.exe","d23b7973d229bea186d4aec95eca3a6f7f2868f7243534156372e5e438ecfd33")]{require(format!("{:x}",Sha256::digest(fs::read(Path::new(STAGE).join(name))?))==hash,"Candidate pin mismatch")?;}Ok(())
}
#[link(name="kernel32")]unsafe extern "system"{fn WTSGetActiveConsoleSessionId()->u32;}
fn metadata()->Result<Value>{
 let sessions=session::query()?;
 let interactive=session::select(&sessions).map(|s|json!({"session_id":s.id,"username":s.user,"domain":s.domain,"protocol":s.protocol,"active":true}));
 Ok(json!({"boot_id":swarm_platform::lifecycle::boot_id()?,"physical_console_id":unsafe{WTSGetActiveConsoleSessionId()},"interactive":interactive,"runtime":swarm_runtime::lifecycle::status(Path::new(ROOT))?,"startup":swarm_runtime::startup::status(Path::new(ROOT),&Path::new(STAGE).join("swarm-runtime.exe"))?,"native_accepted":false}))
}
fn progress(stage:&str)->Result<()>{save(&format!("bootstrap-stage-{stage}.json"),&json!({"stage":stage,"at_unix_millis":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis(),"native_accepted":false}))}
fn bootstrap()->Result<Value>{
 require(!Path::new(BASE).exists(),"Existing run refused")?;fs::create_dir(BASE)?;save("bootstrap-attempt.json",&json!({"automatic_retries":0}))?;fs::create_dir(ROOT)?;
 let out=OpenOptions::new().write(true).create_new(true).open(Path::new(BASE).join("bootstrap-runtime.stdout.log"))?;let err=OpenOptions::new().write(true).create_new(true).open(Path::new(BASE).join("bootstrap-runtime.stderr.log"))?;
 let mut c=Command::new(Path::new(STAGE).join("swarm-runtime.exe"));c.args(["--data-dir",ROOT,"--human-start"]).stdin(Stdio::null()).stdout(out).stderr(err);
 {use std::os::windows::process::CommandExt;c.creation_flags(0x08000000);}let child=c.spawn()?;save("bootstrap-runtime.json",&json!({"pid":child.id()}))?;
 progress("01-runtime-spawned")?;
 let deadline=Instant::now()+Duration::from_secs(20);loop{if swarm_runtime::gui::GuiClient::open(Path::new(ROOT)).and_then(|c|c.setup_status()).is_ok(){break}require(Instant::now()<deadline,"Bootstrap runtime deadline")?;std::thread::sleep(Duration::from_millis(100));}
 progress("02-runtime-ready")?;let mut host=swarm_gui_host::GuiHost::open(Path::new(ROOT))?;
 host.authenticate("setup","STARTUP_OS_ADMIN",&uuid::Uuid::new_v4().simple().to_string())?;
 progress("03-admin-created")?;
 let admin=host.snapshot()?["account"].clone();fs::create_dir(Path::new(BASE).join("Project"))?;
 let project=host.project_control(admin["id"].as_str().ok_or("Admin ID missing")?,"project.register",json!({"name":"Startup OS refusal fixture","root":format!("{BASE}/Project")}))?;
 progress("04-project-created")?;
 swarm_runtime::startup::initialize(Path::new(ROOT),&Path::new(STAGE).join("swarm-runtime.exe"))?;
 progress("05-startup-configured")?;
 let v=json!({"admin_id":admin["id"],"project_id":project["id"],"initial":metadata()?,"native_accepted":false});save("bootstrap.json",&v)?;Ok(v)
}
fn cli_stopped()->Result<Value>{
 let project=read("bootstrap.json")?["project_id"].as_str().ok_or("Project ID missing")?.to_owned();
 let mut c=Command::new(Path::new(STAGE).join("swarm.exe"));c.args(["--data-dir",ROOT,"--project",&project,"--session","startup-os-refusal","--format","json","identity","start"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
 {use std::os::windows::process::CommandExt;c.creation_flags(0x08000000);}let mut child=c.spawn()?;let deadline=Instant::now()+Duration::from_secs(20);
 loop{if child.try_wait()?.is_some(){break}
 if Instant::now()>=deadline{save("cli-timeout.json",&json!({"pid":child.id(),"retry_allowed":false}))?;return Err("CLI timeout; preserve child, no retry".into())}std::thread::sleep(Duration::from_millis(50));}
 let o=child.wait_with_output()?;let stdout=serde_json::from_slice::<Value>(&o.stdout).ok();let stderr=serde_json::from_slice::<Value>(&o.stderr).ok();
 let code=stdout.as_ref().or(stderr.as_ref()).and_then(|v|v.get("error")).and_then(|v|v.get("code")).and_then(Value::as_str).unwrap_or("").to_owned();
 require(o.status.code()==Some(7)&&code=="RUNTIME_STOPPED","CLI did not preserve full stop")?;
 Ok(json!({"exit_code":7,"error_code":code,"stdout_bytes":o.stdout.len(),"stderr_bytes":o.stderr.len(),"identity_created":false}))
}
fn observe(phase:&str,recover_manual_start:bool)->Result<Value>{
 let previous=match phase{"b0-running"=>"bootstrap.json","b0-stopped"=>"b0-running.json","sameboot"=>"b0-stopped.json","enabledboot"=>"sameboot.json","disabled-set"=>"enabledboot.json","disabledboot"=>"disabled-set.json",_=>return Err("Unknown phase".into())};let prior=read(previous)?;
 let attempt=if recover_manual_start{"sameboot-after-manual-start-attempt.json".to_owned()}else{format!("{phase}-attempt.json")};
 save(&attempt,&json!({"automatic_retries":0,"manual_start_confounded_prior_observation":recover_manual_start}))?;
 let mut v=metadata()?;require(v["interactive"]["active"]==true,"Exactly one active guest GPT session required")?;
 let stopped=matches!(phase,"b0-stopped"|"sameboot"|"disabledboot");require(v["runtime"]["state"]==if stopped{"stopped"}else{"running"},"Runtime state differs from contract")?;
 let enabled=!matches!(phase,"disabled-set"|"disabledboot");require(v["startup"]["enabled"]==enabled&&v["startup"]["registered"]==enabled,"Desired/actual startup mismatch")?;
 if phase=="sameboot"{require(v["boot_id"]==prior["boot_id"],"Same-boot logon changed boot GUID")?;}
 if matches!(phase,"enabledboot"|"disabledboot"){require(v["boot_id"]!=prior["boot_id"],"Actual reboot GUID change required")?;}
 if matches!(phase,"b0-stopped"|"disabled-set"){require(v["boot_id"]==prior["boot_id"],"Unexpected reboot between native actions")?;}
 if matches!(phase,"sameboot"|"disabledboot"){v["cli"]=cli_stopped()?;require(swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"]=="stopped","CLI restarted runtime")?;}
 if !stopped{require(swarm_runtime::gui::GuiClient::open(Path::new(ROOT))?.setup_status()?["setup_required"]==false,"Running endpoint/setup response mismatch")?;}
 v["phase"]=json!(phase);v["product_checks_passed"]=json!(true);save(&format!("{phase}.json"),&v)?;Ok(v)
}
fn cleanup()->Result<Value>{
 save("cleanup-attempt.json",&json!({"automatic_retries":0}))?;
 swarm_runtime::startup::configure(Path::new(ROOT),&Path::new(STAGE).join("swarm-runtime.exe"),false)?;
 if swarm_runtime::lifecycle::status(Path::new(ROOT))?["state"]=="running"{swarm_gui_host::GuiHost::open(Path::new(ROOT))?.stop_runtime()?;}
 let identity=swarm_platform::notifications::Identity::new(Path::new(ROOT),&Path::new(STAGE).join("swarm-desktop.exe"))?;swarm_platform::notifications::Registration::new(identity)?.remove()?;
 let v=metadata()?;save("cleanup.json",&v)?;Ok(v)
}
fn run()->Result<Value>{let mut args:Vec<_>=std::env::args().skip(1).collect();inputs()?;if continuing(){args.remove(0);require(args.first().is_some_and(|m|m!="--bootstrap"),"Never replay completed bootstrap")?;require(Path::new(BASE).join("bootstrap.json").exists()&&Path::new(BASE).join("cleanup.json").exists(),"Prior bootstrap and cleanup proof required")?;if !Path::new(CONTINUATION).exists(){fs::create_dir(CONTINUATION)?;}}match args.as_slice(){[m]if m=="--bootstrap"=>bootstrap(),[m]if m=="--metadata"=>metadata(),[m,p]if m=="--observe"=>observe(p,false),[m]if m=="--recover-sameboot"=>{require(continuing()&&Path::new(CONTINUATION).join("sameboot-attempt.json").exists()&&!Path::new(CONTINUATION).join("sameboot.json").exists(),"Prior failed sameboot observation required")?;observe("sameboot",true)},[m]if m=="--cleanup"=>cleanup(),_=>Err("Explicit fixed mode required".into())}}
fn main(){match run(){Ok(v)=>println!("{v}"),Err(_)=>{eprintln!("Fixed startup observer failed; preserve receipts and state; no retry.");std::process::exit(1)}}}
