#![windows_subsystem = "windows"]
use std::{ffi::c_void,fs::{self,OpenOptions},io::Write,mem::{size_of,zeroed},os::windows::ffi::OsStrExt,path::Path};
use windows_sys::Win32::{Foundation::{CloseHandle,LocalFree},Security::{Authorization::{GetNamedSecurityInfoW,SE_FILE_OBJECT},GetTokenInformation,TokenUser,TOKEN_QUERY,TOKEN_USER,OWNER_SECURITY_INFORMATION,DACL_SECURITY_INFORMATION,GetSecurityDescriptorControl,SE_DACL_PROTECTED,GetAclInformation,ACL_SIZE_INFORMATION,AclSizeInformation,EqualSid,GetAce,ACCESS_ALLOWED_ACE},System::Threading::{GetCurrentProcess,OpenProcessToken}};
#[link(name="user32")] unsafe extern "system" {fn MessageBoxW(w:isize,t:*const u16,c:*const u16,k:u32)->i32;}
fn message(text:&str){let text:Vec<u16>=text.encode_utf16().chain(Some(0)).collect();let title:Vec<u16>="Swarm native access diagnosis".encode_utf16().chain(Some(0)).collect();unsafe{MessageBoxW(0,text.as_ptr(),title.as_ptr(),0);}}
fn acl(path:&Path,sid:*mut c_void)->String{
 let wide:Vec<u16>=path.as_os_str().encode_wide().chain(Some(0)).collect();let mut owner=std::ptr::null_mut();let mut dacl=std::ptr::null_mut();let mut sd=std::ptr::null_mut();
 let code=unsafe{GetNamedSecurityInfoW(wide.as_ptr(),SE_FILE_OBJECT,OWNER_SECURITY_INFORMATION|DACL_SECURITY_INFORMATION,&mut owner,std::ptr::null_mut(),&mut dacl,std::ptr::null_mut(),&mut sd)};
 if code!=0{return format!("acl_error={code}")}
 let mut control=0u16;let mut revision=0;unsafe{GetSecurityDescriptorControl(sd,&mut control,&mut revision)};
 let owner_matches=!owner.is_null()&&unsafe{EqualSid(owner,sid)}!=0;
 let mut info:ACL_SIZE_INFORMATION=unsafe{zeroed()};let info_ok=!dacl.is_null()&&unsafe{GetAclInformation(dacl,(&mut info as *mut ACL_SIZE_INFORMATION).cast(),size_of::<ACL_SIZE_INFORMATION>() as u32,AclSizeInformation)}!=0;
 let mut ace=std::ptr::null_mut();let mut detail=String::new();
 if info_ok&&info.AceCount==1&&unsafe{GetAce(dacl,0,&mut ace)}!=0&&!ace.is_null(){let ace=unsafe{&*ace.cast::<ACCESS_ALLOWED_ACE>()};detail=format!(" ace_flags={} ace_mask={:#x} ace_user_matches={}",ace.Header.AceFlags,ace.Mask,unsafe{EqualSid(std::ptr::addr_of!(ace.SidStart).cast_mut().cast(),sid)}!=0)}
 unsafe{LocalFree(sd)};
 format!("owner_matches={owner_matches} protected={} ace_count={}{}",control&SE_DACL_PROTECTED!=0,if info_ok{info.AceCount}else{u32::MAX},detail)
}
fn run()->Result<String,Box<dyn std::error::Error>>{
 if std::env::var("COMPUTERNAME")?.to_uppercase()!="DESKTOP-1TAP8BI"||std::env::var("USERNAME")?.to_uppercase()!="GPT"{return Err("Wrong guest".into())}
 let root=Path::new("C:/Swarm-Validation/Runs/startup-20261021/Data");
 let mut file=OpenOptions::new().create_new(true).write(true).open(root.parent().ok_or("No parent")?.join("native-20261023/gui-access-diagnostic.txt"))?;
 let mut token=std::ptr::null_mut();if unsafe{OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY,&mut token)}==0{return Err("Cannot inspect own token".into())}
 let mut needed=0;unsafe{GetTokenInformation(token,TokenUser,std::ptr::null_mut(),0,&mut needed)};
 let mut buffer=vec![0usize;(needed as usize).div_ceil(size_of::<usize>())];
 if unsafe{GetTokenInformation(token,TokenUser,buffer.as_mut_ptr().cast(),needed,&mut needed)}==0{unsafe{CloseHandle(token)};return Err("Cannot inspect own user".into())}
 let sid=unsafe{(*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid};
 let mut lines=vec!["Read-only native access checks; no ACL changes; no credential contents read.".to_owned()];
 for name in ["gui-state","lifecycle"]{
  let path=root.join(name);
  let exists=path.is_dir();let open=if exists{format!("{:?}",swarm_platform::CredentialStore::open(&path).err())}else{"missing; not created".into()};
  lines.push(format!("{name}: store_error={open}; {}",acl(&path,sid)));
 }
 for name in ["gui-state/.host.lock","lifecycle/runtime.lock","lifecycle/startup.lock"]{let result=OpenOptions::new().read(true).write(true).open(root.join(name));lines.push(format!("{name}: open_error={:?}",result.err().map(|e|e.raw_os_error())))}
 let mut cursor=Some(root);while let Some(path)=cursor{lines.push(format!("ancestor {}: metadata_error={:?}",path.display(),fs::metadata(path).err().map(|e|e.raw_os_error())));cursor=path.parent();}
 unsafe{CloseHandle(token)};
 let text=lines.join("\n");writeln!(file,"{text}")?;Ok(text)
}
fn main(){match run(){Ok(text)=>message(&text),Err(error)=>message(&format!("Access diagnosis failed: {error}"))}}
