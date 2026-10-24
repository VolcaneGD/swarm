//! Cleanup only for the consumed fixed observation. Never starts or resumes an identity.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs::{self, OpenOptions}, io::Write, path::Path, process::{Command, Stdio}, time::{Duration, Instant}};
use swarm_platform::CredentialStore;
const BASE: &str = "E:/Swarm-Validation-Host/run-20261019-elapsed-approved-extension-2";
const ROOT: &str = "E:/Swarm-Validation-Host/run-20261019-elapsed-approved-extension-2/Data";
const CANDIDATE: &str = "C:/Users/Unknown/.gemini/antigravity/scratch/swarm/target/distribution/candidate-20261019-account-feedback";
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).ok_or("explicit cleanup mode required")?;
    assert_eq!(fs::canonicalize(ROOT)?.to_string_lossy().trim_start_matches(r"\\?\").replace('\\', "/"), ROOT);
    assert_eq!(format!("{:x}",Sha256::digest(fs::read(Path::new(BASE).join("baseline.json"))?)), "1d68c9a0106becf65940dd645c758de3a31fc9588d6dc1d5b4de1342d5c4c644");
    let failed: Value = serde_json::from_slice(&fs::read(Path::new(BASE).join("followup-product-result.json"))?)?;
    assert_eq!(failed["status"], "FAIL");
    let receipt = match mode.as_str() {
        "--release-once" => {
            let mut out = OpenOptions::new().create_new(true).write(true).open(Path::new(BASE).join("followup-release.json"))?;
            let store = CredentialStore::open(&Path::new(ROOT).join("client-credentials"))?;
            let saved = store.get("elapsed-followup-20261020")?;
            let receipt = if let Some(bytes) = saved {
                let candidate: Value = serde_json::from_slice(&bytes)?;
                assert_eq!(candidate["kind"], "ai");
                let project = candidate["project_id"].as_str().ok_or("saved project missing")?;
                let exe = Path::new(CANDIDATE).join("swarm.exe");
                assert_eq!(format!("{:x}",Sha256::digest(fs::read(&exe)?)), "d23b7973d229bea186d4aec95eca3a6f7f2868f7243534156372e5e438ecfd33");
                let mut child = Command::new(exe).args(["--data-dir",ROOT,"--project",project,"--session","elapsed-followup-20261020","--format","json","identity","release"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
                let until = Instant::now()+Duration::from_secs(20);
                while child.try_wait()?.is_none() {
                    if Instant::now()>=until { return Err(format!("cleanup timeout; preserve PID {}, no retry",child.id()).into()); }
                    std::thread::sleep(Duration::from_millis(25));
                }
                let result = child.wait_with_output()?;
                let parsed: Value=serde_json::from_slice(&result.stdout).unwrap_or(Value::Null);
                let receipt=json!({"saved_identity_present":true,"exit_code":result.status.code(),"ok":parsed["ok"],"error_code":parsed["error"]["code"],"stderr_bytes":result.stderr.len(),"retry_allowed":false});
                writeln!(out,"{receipt}")?;
                assert!(result.status.success() && parsed["ok"]==true,"normal release failed; no retry");
                receipt
            } else { let r=json!({"saved_identity_present":false,"release_needed":false,"retry_allowed":false}); writeln!(out,"{r}")?; r };
            receipt
        }
        "--registration-after-stop" | "--public-registration-after-stop" => {
            let name = if mode == "--public-registration-after-stop" {
                let result: Value=serde_json::from_slice(&fs::read(Path::new(BASE).join("followup-public-extension-1-result.json"))?)?;
                assert!(result["status"]=="FAIL" || result["status"]=="PUBLIC_READBACK_PASS_NATIVE_CLOSEOUT_SEPARATE");
                "followup-public-extension-1-closeout.json"
            } else { "followup-closeout.json" };
            let status=swarm_runtime::lifecycle::status(Path::new(ROOT)).map_err(|_|"runtime status unavailable")?;
            // Readback must prove the operator's normal stop before removing this registration.
            assert_eq!(status["state"],"stopped");
            let mut out=OpenOptions::new().create_new(true).write(true).open(Path::new(BASE).join(name))?;
            let exe=Path::new(CANDIDATE).join("swarm-desktop.exe");
            let registration=swarm_platform::notifications::Registration::new(swarm_platform::notifications::Identity::new(Path::new(ROOT),&exe)?)?;
            registration.remove()?;
            assert!(!registration.is_registered()?);
            let r=json!({"runtime_status":status,"owned_registration_absent":true,"baseline_preserved":true,"retry_allowed":false});
            writeln!(out,"{r}")?;
            r
        }
        _=>return Err("cleanup mode refused".into())
    };
    println!("{receipt}");
    Ok(())
}
