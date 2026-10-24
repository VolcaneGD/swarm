#![windows_subsystem = "windows"]
use std::{fs::{self,OpenOptions},io::Write,path::Path,process::{Command,Stdio},time::{Duration,Instant}};
use std::os::windows::process::CommandExt;
#[link(name="user32")]
unsafe extern "system" { fn MessageBoxW(window:isize,text:*const u16,title:*const u16,kind:u32)->i32; }
const STAGE:&str="C:/Swarm-Validation/Inputs/startup-20261021";
const BASE:&str="C:/Swarm-Validation/Runs/startup-20261021";
fn message(text:&str){let text:Vec<u16>=text.encode_utf16().chain(Some(0)).collect();let title:Vec<u16>="Swarm runtime launch diagnosis".encode_utf16().chain(Some(0)).collect();unsafe{MessageBoxW(0,text.as_ptr(),title.as_ptr(),0);}}
fn run()->Result<String,Box<dyn std::error::Error>>{
 if std::env::var("COMPUTERNAME")?.to_uppercase()!="DESKTOP-1TAP8BI" || std::env::var("USERNAME")?.to_uppercase()!="GPT" {return Err("Wrong guest identity".into())}
 if std::env::current_exe()?.parent().ok_or("No executable directory")?.canonicalize()?!=Path::new(STAGE).canonicalize()? {return Err("Wrong diagnostic location".into())}
 for file in ["bootstrap.json","cleanup.json"]{if !Path::new(BASE).join(file).is_file(){return Err("Original fixture receipt absent".into())}}
 let dir=Path::new(BASE).join("native-20261023");
 let mut receipt=OpenOptions::new().create_new(true).write(true).open(dir.join("runtime-launch-diagnostic.txt"))?;
 let stdout=OpenOptions::new().create_new(true).write(true).open(dir.join("runtime-launch.stdout.log"))?;
 let stderr=OpenOptions::new().create_new(true).write(true).open(dir.join("runtime-launch.stderr.log"))?;
 writeln!(receipt,"explicit_fixture_root=true\nnative_acceptance=false\nlauncher_pid={}",std::process::id())?;
 let spawn=Command::new(Path::new(STAGE).join("swarm-runtime.exe")).args(["--data-dir",&format!("{BASE}/Data")]).current_dir(STAGE).stdin(Stdio::null()).stdout(stdout).stderr(stderr).creation_flags(0x08000000).spawn();
 let mut child=match spawn{Ok(child)=>child,Err(error)=>{writeln!(receipt,"spawn_error_code={:?}",error.raw_os_error())?;return Ok(format!("GUI could not start. Windows error: {:?}",error.raw_os_error()))}};
 writeln!(receipt,"gui_pid={}",child.id())?;
 let deadline=Instant::now()+Duration::from_secs(20);
 loop{
  if let Some(status)=child.try_wait()?{
   writeln!(receipt,"gui_exit_code={:?}",status.code())?;
   let error_path=dir.join("runtime-launch.stderr.log");
   let raw=if fs::metadata(&error_path)?.len()<=65536{fs::read_to_string(error_path).unwrap_or_default()}else{String::new()};
   let safe:Vec<_>=raw.lines().filter(|line|line.len()<200&&line.split_once(':').is_some_and(|(code,_)|!code.is_empty()&&code.bytes().all(|c|c.is_ascii_uppercase()||c==b'_'))).take(5).collect();
   return Ok(format!("Runtime exited. Exit code: {:?}\n{}\nEvidence: native-20261023/runtime-launch-diagnostic.txt",status.code(),safe.join("\n")))
  }
  if Instant::now()>=deadline{writeln!(receipt,"runtime_running_after_20s=true")?;return Ok(format!("Runtime remains running (PID {}). This is diagnosis only. Return to Swarm and retry its connection.",child.id()))}
  std::thread::sleep(Duration::from_millis(100));
 }
}
fn main(){match run(){Ok(result)=>message(&result),Err(error)=>message(&format!("Diagnostic did not complete: {error}"))}}
