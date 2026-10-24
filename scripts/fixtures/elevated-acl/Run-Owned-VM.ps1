#requires -Version 5.1
param([switch]$ShowPlan,[switch]$ValidateApprovalOnly,[switch]$ExecuteApproved)
$ErrorActionPreference='Stop'
Import-Module "$PSHOME\Modules\Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1"
$workspace='C:\Users\Unknown\.gemini\antigravity\scratch\swarm'
$out=Join-Path $workspace 'target\vm-acl-20261020'
$vmId=[guid]'16360373-4ac4-455f-81a4-0679644b080d'
$guestRoot='C:\Swarm-Validation\Runs\acl-20261020'
$guestStage='C:\Swarm-Validation\Inputs\acl-20261020'
$source=Join-Path $workspace 'scripts\fixtures\elevated-acl'
function Save-New($name,$value){
 $bytes=(New-Object Text.UTF8Encoding($false)).GetBytes(($value|ConvertTo-Json -Depth 12))
 $file=[IO.File]::Open((Join-Path $out $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
 try{$file.Write($bytes,0,$bytes.Length)}finally{$file.Dispose()}
}
function Get-Plan {
 $inputs=@{}
 foreach($item in @(@('probe',"$out\acl_probe.exe"),@('observer',"$source\Observe-Guest-Acl.ps1"),@('probe-source',"$source\probe.rs"),@('platform-library',"$workspace\target\release\deps\libswarm_platform-3012bce3dbf0a04d.rlib"),@('prior-success',"$workspace\target\vm-retained-20261020-extension-5\result.json"),@('baseline',"$workspace\target\batched-roadmap-20261015-os\baseline.json"),@('owned-process-helper',"$workspace\target\batched-roadmap-20261015-os\Invoke-OwnedProcess.ps1"),@('vm-proof',"$workspace\target\batched-roadmap-20261015-os\vm-ready.json"),@('inspector',"$out\Inspect-Owned-VM.ps1"))){$inputs[$item[0]]=(Get-FileHash -LiteralPath $item[1]).Hash}
 @{incident='vm-os-20261015';scope='R01-independent-elevated-acl-once';vm_id=$vmId.ToString();baseline_id='ab32a58f-bed7-4540-a640-d428c80fe442';guest_root=$guestRoot;guest_stage=$guestStage;attempts=1;automatic_retries=0;guest_deadline_seconds=180;boot_wait_seconds=90;input_hashes=$inputs;supervisor_sha256=(Get-FileHash -LiteralPath $PSCommandPath).Hash;publication=$false;reinstall=$false;checkpoint_restore=$false;network_change=$false;existing_data_change=$false;acl_repair=$false;elapsed_seed=$false}
}
if($ShowPlan -and !$ExecuteApproved){Get-Plan|ConvertTo-Json -Depth 8;exit 0}
if(!$ExecuteApproved -and !$ValidateApprovalOnly){throw 'No default action; explicit plan or approved execution only'}
$approval=Get-Content -LiteralPath "$out\approval.json" -Raw -Encoding UTF8|ConvertFrom-Json
$answer=ConvertFrom-Json '"R01\u306e\u72ec\u7acbACL\u691c\u67fb\u3092\u8ffd\u52a0\u0031\u56de\u8a31\u53ef\u3059\u308b"'
if($approval.user_answer -cne $answer -or $approval.scope -cne 'R01-independent-elevated-acl-once' -or $approval.incident -cne 'vm-os-20261015' -or $approval.attempts -ne 1 -or $approval.automatic_retries -ne 0 -or $approval.vm_id -cne $vmId.ToString()){throw 'New scoped R01 human approval required'}
foreach($field in @('publication','reinstall','checkpoint_restore','network_change','existing_data_change','acl_repair','elapsed_seed')){if($approval.$field -ne $false){throw 'Scope exclusion missing'}}
if($ValidateApprovalOnly -and !$ExecuteApproved){@{approval_matches=$true;vm_access=$false;attempt_consumed=(Test-Path -LiteralPath "$out\attempt.json")}|ConvertTo-Json;exit 0}
if(Test-Path -LiteralPath "$out\attempt.json"){throw 'R01 added attempt consumed; no retry'}
$principal=New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Normal UAC elevation required'}
$plan=Get-Content -LiteralPath "$out\plan.json" -Raw -Encoding UTF8|ConvertFrom-Json
$current=Get-Plan
if($plan.supervisor_sha256 -cne $current.supervisor_sha256){throw 'Pinned supervisor changed'}
foreach($name in $current.input_hashes.Keys){if($plan.input_hashes.$name -cne $current.input_hashes[$name]){throw 'Pinned R01 input changed'}}
$prior=Get-Content -LiteralPath "$workspace\target\vm-retained-20261020-extension-5\result.json" -Raw -Encoding UTF8|ConvertFrom-Json
if($prior.status -cne 'PASS' -or $prior.final_state -cne 'Off'){throw 'Preserve prior successful closeout'}
& "$out\Inspect-Owned-VM.ps1"
$inspection=Get-Content -LiteralPath "$out\host-inspection.json" -Raw -Encoding UTF8|ConvertFrom-Json
if($inspection.status -cne 'PASS' -or $inspection.vm.state -cne 'Off' -or !$inspection.disk_chain_matches -or !$inspection.baseline_present -or !$inspection.network_disconnected -or $inspection.volume.health -cne 'Healthy'){throw 'Fixed VM prerequisites failed'}
Import-Module Hyper-V
$vm=Get-VM -Id $vmId
$proof=Get-Content -LiteralPath "$workspace\target\batched-roadmap-20261015-os\vm-ready.json" -Raw -Encoding UTF8|ConvertFrom-Json
if($proof.baseline_id -cne $current.baseline_id -or $vm.Name -cne 'Swarm-Validation-Win11' -or $vm.State -ne 'Off' -or (Get-VHD -Path $inspection.disk_chain[0]).Attached){throw 'Off/detached fixed VM guard'}
$os=Get-CimInstance Win32_OperatingSystem
$required=[UInt64](Get-VMMemory -VM $vm).Startup+1GB
if([UInt64]$os.FreePhysicalMemory*1KB -lt $required -or [UInt64]$os.FreeVirtualMemory*1KB -lt $required){throw 'Physical/commit margin unavailable; no attempt consumed'}
$session=$null;$credential=$null;$job=$null;$phase='starting-vm'
Save-New 'attempt.json' @{incident=$current.incident;scope=$current.scope;at=[DateTimeOffset]::Now.ToString('o');automatic_retries=0;vm_id=$vmId.ToString()}
try {
 Start-VM -VM $vm|Out-Null
 $phase='waiting-for-completed-boot'
 for($i=0;$i -lt 6;$i++){Start-Sleep -Seconds 15}
 # Boot wait is not readiness proof. Exactly one connection follows local input.
 $phase='awaiting-human-credential'
 Save-New 'awaiting-credential.json' @{phase=$phase;secret_recorded=$false;at=[DateTimeOffset]::Now.ToString('o')}
 $credential=Get-Credential -UserName 'DESKTOP-1TAP8BI\GPT' -Message 'R01 independent synthetic ACL observation only; one attempt, no retry.'
 if(!$credential){throw 'Local credential cancelled'}
 $phase='opening-direct-session'
 $session=New-PSSession -VMId $vmId -Credential $credential
 $credential=$null
 $phase='checking-guest-inputs'
 Invoke-Command -Session $session -ArgumentList $guestRoot,$guestStage,$current.input_hashes -ScriptBlock {
  param($root,$stage,$hashes)
  $ErrorActionPreference='Stop'
  $identity=[Security.Principal.WindowsIdentity]::GetCurrent()
  $principal=New-Object Security.Principal.WindowsPrincipal($identity)
  if($env:COMPUTERNAME -cne 'DESKTOP-1TAP8BI' -or $identity.Name -ine 'DESKTOP-1TAP8BI\GPT' -or !$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Elevated guest identity required'}
  if((Test-Path -LiteralPath $root) -or (Test-Path -LiteralPath $stage)){throw 'Preserve existing R01 roots; no alternate root'}
  foreach($pair in @(@('baseline','baseline.json'),@('owned-process-helper','Invoke-OwnedProcess.ps1'))){if((Get-FileHash -LiteralPath (Join-Path 'C:\Swarm-Validation\Runs\os-20261015' $pair[1])).Hash -cne $hashes[$pair[0]]){throw 'Original guest control input changed'}}
  New-Item -ItemType Directory -Path $stage -ErrorAction Stop|Out-Null
 }
 Copy-Item -LiteralPath "$out\acl_probe.exe" -Destination "$guestStage\acl_probe.exe" -ToSession $session
 Copy-Item -LiteralPath "$source\Observe-Guest-Acl.ps1" -Destination "$guestStage\Observe-Guest-Acl.ps1" -ToSession $session
 $phase='dispatching-r01-observer'
 $job=Invoke-Command -Session $session -AsJob -ArgumentList $guestStage,$guestRoot,$current.input_hashes -ScriptBlock {
  param($stage,$root,$hashes)
  $ErrorActionPreference='Stop'
  foreach($pair in @(@('probe','acl_probe.exe'),@('observer','Observe-Guest-Acl.ps1'))){if((Get-FileHash -LiteralPath (Join-Path $stage $pair[1])).Hash -cne $hashes[$pair[0]]){throw 'Transferred probe input mismatch'}}
  if(@(Get-ExecutionPolicy -List|Where-Object{$_.Scope -in @('MachinePolicy','UserPolicy') -and $_.ExecutionPolicy -ne 'Undefined'}).Count){throw 'Managed policy; no bypass'}
  $before=Get-ExecutionPolicy -List|ConvertTo-Json -Compress
  & "$env:WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$stage\Observe-Guest-Acl.ps1"
  $code=$LASTEXITCODE
  if((Get-ExecutionPolicy -List|ConvertTo-Json -Compress) -cne $before){throw 'Parent policy changed'}
  @{exit_code=$code;receipt=(Get-Content -LiteralPath "$root\result.json" -Raw -Encoding UTF8|ConvertFrom-Json)}
 }
 if(!(Wait-Job -Job $job -Timeout 180) -or $job.State -ne 'Completed'){throw 'R01 guest deadline/job failure; preserve state'}
 $result=Receive-Job -Job $job -ErrorAction Stop
 Save-New 'dispatch-result.json' $result
 if($result.exit_code -ne 0 -or $result.receipt.status -cne 'PASS'){throw 'R01 observer failed; no retry'}
 foreach($name in @('attempt.json','acl-while-first-held.json','contention-observed.json','acl-while-second-held.json','result.json')){
  $destination="$out\guest-$name"
  if(Test-Path -LiteralPath $destination){throw 'Preserve existing completed guest receipt'}
  Copy-Item -LiteralPath "$guestRoot\$name" -Destination $destination -FromSession $session
 }
 Remove-Job -Job $job;$job=$null
 Remove-PSSession -Session $session;$session=$null
 $phase='normal-shutdown'
 Stop-VM -VM (Get-VM -Id $vmId)
 & "$out\Inspect-Owned-VM.ps1" -ReceiptName 'host-after-attempt.json'
 $after=Get-Content -LiteralPath "$out\host-after-attempt.json" -Raw -Encoding UTF8|ConvertFrom-Json
 if($after.status -cne 'PASS' -or $after.vm.state -cne 'Off' -or !$after.disk_chain_matches -or !$after.baseline_present -or !$after.network_disconnected){throw 'Normal closeout not confirmed'}
 Save-New 'result.json' @{status='PASS';final_state='Off';scope=$current.scope;at=[DateTimeOffset]::Now.ToString('o');automatic_retries=0;native_ui_accepted=$false;formal_ac_accepted=$false}
}catch{
 Save-New 'result.json' @{status='FAIL';phase=$phase;error=$_.Exception.Message;state_preserved=$true;retry_allowed=$false;at=[DateTimeOffset]::Now.ToString('o')}
 exit 1
}finally{if($session -and (!$job -or $job.State -eq 'Completed')){Remove-PSSession -Session $session};$credential=$null}
