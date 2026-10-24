#requires -Version 5.1
param([switch]$ShowPlan,[switch]$ValidateApprovalOnly,[switch]$InspectResources,[switch]$ExecuteApproved,[switch]$Continuation2,[switch]$Continuation3,[switch]$Continuation4,[switch]$Continuation5)
$ErrorActionPreference='Stop'
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1') -ErrorAction Stop
$workspace='C:\Users\Unknown\.gemini\antigravity\scratch\swarm'
$old=Join-Path $workspace 'target\batched-roadmap-20261015-os'
$out=Join-Path $workspace 'target\vm-retained-20261020'
$vmId=[guid]'16360373-4ac4-455f-81a4-0679644b080d'
$guestRun='C:\Swarm-Validation\Runs\os-20261015'
# ASCII source preserves this exact Japanese answer under Windows PowerShell5.1.
$answer=ConvertFrom-Json '"\u65e2\u5b58VM\u306e\u7d99\u7d9a\u691c\u67fb\u3060\u3051\u8ffd\u52a01\u56de\u3092\u8a31\u53ef\u3059\u308b"'
$priorOut=$out
if(@($Continuation2,$Continuation3,$Continuation4,$Continuation5|Where-Object{$_}).Count -gt 1){throw 'Choose one explicit continuation'}
$runningContinuation=$Continuation4 -or $Continuation5
$renewed=$Continuation2 -or $Continuation3 -or $runningContinuation
$extension=if($Continuation5){5}elseif($Continuation4){4}elseif($Continuation3){3}elseif($Continuation2){2}else{1}
if($Continuation2){
 $out=Join-Path $workspace 'target\vm-retained-20261020-extension-2'
 $answer=ConvertFrom-Json '"\u3067\u306f\u3001\u0056\u004d\u691c\u67fb\u3092\u884c\u3063\u3066\u304f\u3060\u3055\u3044\u3002"'
}
if($Continuation3){
 $priorOut=Join-Path $workspace 'target\vm-retained-20261020-extension-2'
 $out=Join-Path $workspace 'target\vm-retained-20261020-extension-3'
 $answer=ConvertFrom-Json '"\u518d\u8a66\u884c\u3057\u3066\u304f\u3060\u3055\u3044\u3002"'
}
if($Continuation4){
 $priorOut=Join-Path $workspace 'target\vm-retained-20261020-extension-3'
 $out=Join-Path $workspace 'target\vm-retained-20261020-extension-4'
 $answer=ConvertFrom-Json '"\u8a31\u53ef\u3057\u307e\u3059\u3002\u7d9a\u3051\u3066\u304f\u3060\u3055\u3044\u3002"'
}
if($Continuation5){
 $priorOut=Join-Path $workspace 'target\vm-retained-20261020-extension-4'
 $out=Join-Path $workspace 'target\vm-retained-20261020-extension-5'
 $answer=ConvertFrom-Json '"\u7d9a\u3051\u3066\u304f\u3060\u3055\u3044\u3002"'
}
function Save-New([string]$name,$value) {
 $bytes=(New-Object Text.UTF8Encoding($false)).GetBytes(($value|ConvertTo-Json -Depth 12))
 $file=[IO.File]::Open((Join-Path $out $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
 try{$file.Write($bytes,0,$bytes.Length)}finally{$file.Dispose()}
}
function Current-Plan {
 $hashes=@{}
 foreach($name in @('Test-Retained-Install.ps1','Invoke-OwnedProcess.ps1','os_probe_owner.exe','vm-ready.json','baseline.json')) {
  $hashes[$name]=(Get-FileHash -LiteralPath (Join-Path $old $name) -Algorithm SHA256).Hash
 }
 $hashes['package-report.json']=(Get-FileHash -LiteralPath (Join-Path $workspace 'target\distribution\candidate-20261014-header-wrap\package-report.json')).Hash
 $hashes['OwnerFixed\swarm-runtime.exe']=(Get-FileHash -LiteralPath (Join-Path $workspace 'target\distribution\candidate-20261015-private-owner\swarm-runtime.exe')).Hash
 if($renewed){foreach($name in @('attempt.json','result.json')){$hashes["prior-$name"]=(Get-FileHash -LiteralPath (Join-Path $priorOut $name)).Hash}}
 if($runningContinuation){$hashes['identity-connection-result']=(Get-FileHash -LiteralPath (Join-Path $workspace 'target\vm-direct-identity-20261020\result.json')).Hash}
 @{incident='vm-os-20261015';vm_id=$vmId.ToString();baseline_id='ab32a58f-bed7-4540-a640-d428c80fe442';guest_run=$guestRun;input_hashes=$hashes;supervisor_sha256=(Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash;attempts=1;retries=0;guest_deadline_seconds=420;guest_uninstall='Normal NSIS self-copy uninstall removes retained Installed only';guest_registration_cleanup='Verify original PATH value/type/existence and only owned PATH/uninstall registrations removed';original_data='Require exact original guest Data count and SHA256 preservation';host_os_mutation=$false;reinstall=$false;checkpoint_restore=$false;network_change=$false;power_fault=$false;native_ui_accepted=$false;independent_acl_accepted=$false}
}
function Get-HostResourceStatus([UInt64]$StartupBytes) {
 $os=Get-CimInstance Win32_OperatingSystem
 $available=[UInt64]$os.FreePhysicalMemory*1KB
 $commit=[UInt64]$os.FreeVirtualMemory*1KB
 $required=$StartupBytes+1GB
 @{startup_bytes=$StartupBytes;host_margin_bytes=1GB;required_available_bytes=$required;available_ram_bytes=$available;free_commit_bytes=$commit;can_start=($available -ge $required -and $commit -ge $required);vm_access=$false;vm_mutation=$false}
}
if($InspectResources -and !$ExecuteApproved){Get-HostResourceStatus (6GB)|ConvertTo-Json;exit 0}
if($ShowPlan -and !$ExecuteApproved){Current-Plan|ConvertTo-Json -Depth 8;exit 0}
if(!$ExecuteApproved -and !$ValidateApprovalOnly){throw 'Use read-only -ShowPlan/-ValidateApprovalOnly or approved execution; no default action'}
# Approval is checked before Hyper-V access, elevation/credentials, or any write.
$approval=Get-Content (Join-Path $out 'approval.json') -Raw -Encoding UTF8|ConvertFrom-Json
if($approval.user_answer -cne $answer -or $approval.incident -cne 'vm-os-20261015' -or $approval.extra_attempts -ne 1 -or $approval.retries -ne 0 -or $approval.vm_id -cne $vmId.ToString() -or $approval.publication_permission -ne $false -or $approval.elapsed_permission -ne $false -or $approval.power_fault_permission -ne $false){throw 'New narrow human approval required; no execution'}
if($renewed){
 if($approval.extension -ne $extension){throw 'Wrong explicit continuation scope'}
 foreach($name in @('attempt.json','result.json')){if((Get-FileHash -LiteralPath (Join-Path $priorOut $name)).Hash -cne $approval.prior_receipt_hashes.$name){throw 'Prior consumed evidence changed; no budget reset'}}
}
if($runningContinuation){
 $identityReceipt=Join-Path $workspace 'target\vm-direct-identity-20261020\result.json'
 if((Get-FileHash -LiteralPath (Join-Path $priorOut 'plan.json')).Hash -cne $approval.prior_plan_sha256){throw 'Prior retained-input plan changed'}
 $priorPlan=Get-Content -LiteralPath (Join-Path $priorOut 'plan.json') -Raw -Encoding UTF8|ConvertFrom-Json
 $freshPlan=Current-Plan
 foreach($name in @('Test-Retained-Install.ps1','Invoke-OwnedProcess.ps1','os_probe_owner.exe','vm-ready.json','baseline.json','package-report.json','OwnerFixed\swarm-runtime.exe')){if($freshPlan.input_hashes[$name] -cne $priorPlan.input_hashes.$name){throw 'Original retained input changed before running continuation'}}
 if((Get-FileHash -LiteralPath $identityReceipt).Hash -cne $approval.identity_connection_sha256){throw 'Identity connection receipt changed'}
 $identity=Get-Content -LiteralPath $identityReceipt -Raw -Encoding UTF8|ConvertFrom-Json
 if($identity.status -cne 'PASS' -or !$identity.session_closed -or $identity.hostname -cne 'DESKTOP-1TAP8BI' -or $identity.whoami -ine 'desktop-1tap8bi\gpt' -or $approval.reinstall_permission -ne $false -or $approval.checkpoint_restore_permission -ne $false -or $approval.running_vm_only -ne $true){throw 'Running-VM continuation scope mismatch'}
}
if($ValidateApprovalOnly -and !$ExecuteApproved){@{approval_matches=$true;vm_access=$false;attempt_consumed=(Test-Path -LiteralPath (Join-Path $out 'attempt.json'))}|ConvertTo-Json;exit 0}
if(Test-Path -LiteralPath (Join-Path $out 'attempt.json')){throw 'VM extension consumed; preserve receipts, no retry'}
$principal=New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Normal administrator elevation required; no execution'}
$planName=if($renewed){'plan.json'}else{'plan-ps51-fixed.json'}
$plan=Get-Content (Join-Path $out $planName) -Raw -Encoding UTF8|ConvertFrom-Json
$current=Current-Plan
if($plan.vm_id -cne $current.vm_id -or $plan.baseline_id -cne $current.baseline_id -or $plan.supervisor_sha256 -cne $current.supervisor_sha256){throw 'Pinned supervisor identity changed'}
foreach($name in $current.input_hashes.Keys){if($plan.input_hashes.$name -cne $current.input_hashes[$name]){throw 'Pinned retained input changed'}}
$inspection=Get-Content (Join-Path $out 'host-inspection.json') -Raw -Encoding UTF8|ConvertFrom-Json
if($inspection.status -ne 'PASS' -or !$inspection.disk_chain_matches -or !$inspection.baseline_present -or !$inspection.network_disconnected){throw 'Verified read-only VM prerequisites required'}
Import-Module Hyper-V
$vm=Get-VM -Id $vmId -ErrorAction Stop
$proof=Get-Content (Join-Path $old 'vm-ready.json') -Raw -Encoding UTF8|ConvertFrom-Json
$disks=@($vm|Get-VMHardDiskDrive)
$expectedState=if($runningContinuation){'Running'}else{'Off'}
if($vm.Name -cne 'Swarm-Validation-Win11' -or $vm.State.ToString() -cne $expectedState -or $disks.Count -ne 1 -or $disks[0].Path -ine $proof.disk_chain[0] -or (Get-VHD $disks[0].Path).ParentPath -ine $proof.disk_chain[1] -or @($vm|Get-VMNetworkAdapter|Where-Object SwitchName).Count -or @($vm|Get-VMSnapshot|Where-Object{$_.Id.ToString() -eq $current.baseline_id}).Count -ne 1){throw 'Live owned VM guard; preserve state'}
if(@($proof.disk_chain).Count -ne 2 -or (Get-VHD -Path $proof.disk_chain[1]).ParentPath){throw 'Expected complete two-node disk chain; extra ancestor refused'}
$memory=Get-VMMemory -VM $vm
$resources=Get-HostResourceStatus ([UInt64]$memory.Startup)
if(!$runningContinuation -and !$resources.can_start){throw 'Host RAM/commit prerequisite unavailable; no VM start or attempt consumption'}
$session=$null;$credential=$null;$job=$null
# This exclusive receipt consumes the added attempt before any VM mutation.
Save-New 'attempt.json' @{incident='vm-os-20261015';extra_attempt=$extension;retries=0;vm_id=$vmId.ToString();at=[DateTimeOffset]::Now.ToString('o')}
try {
 $phase='starting-vm'
 if(!$runningContinuation){Start-VM -VM $vm|Out-Null}
 $phase='awaiting-human-credential'
 Save-New 'awaiting-credential.json' @{vm_id=$vmId.ToString();user_input_required=$true;secret_recorded=$false}
 $credential=Get-Credential -UserName 'DESKTOP-1TAP8BI\GPT' -Message 'Approved one-time retained Swarm VM continuation. Guest password stays in memory; no retries.'
 if(!$credential){throw 'Human credential cancelled; no guest trial or automatic retry'}
 $phase='opening-powershell-direct-session'
 Save-New 'guest-session-opening.json' @{phase=$phase;at=[DateTimeOffset]::Now.ToString('o');secret_recorded=$false}
 $session=New-PSSession -VMId $vmId -Credential $credential
 $credential=$null
 $phase='checking-guest-identity'
 Save-New 'guest-session-opened.json' @{phase=$phase;at=[DateTimeOffset]::Now.ToString('o');secret_recorded=$false}
 $guest=Invoke-Command -Session $session -ArgumentList $guestRun -ScriptBlock {
  param($run)
  if($env:COMPUTERNAME -cne 'DESKTOP-1TAP8BI' -or [Security.Principal.WindowsIdentity]::GetCurrent().Name -cne 'DESKTOP-1TAP8BI\GPT'){throw 'Wrong guest identity'}
  if(Test-Path -LiteralPath "$run\retained-install-result.json"){throw 'Guest continuation already consumed'}
  @{identity_verified=$true;run_exists=(Test-Path -LiteralPath $run)}
 }
 if(!$guest.run_exists){throw 'Retained guest run missing; no substitute fixture'}
 $phase='checking-guest-input-pins'
 $guestInputs=Invoke-Command -Session $session -ArgumentList $guestRun,$current.input_hashes -ScriptBlock {
  param($run,$hashes)
  $observed=@{}
  foreach($name in @('baseline.json','package-report.json','os_probe_owner.exe','OwnerFixed\swarm-runtime.exe')) {
   $actual=(Get-FileHash -LiteralPath (Join-Path $run $name) -Algorithm SHA256).Hash
   if($actual -cne $hashes[$name]){throw 'Guest-controlling retained input differs; no execution'}
   $observed[$name]=$actual
  }
  $observed
 }
 Save-New 'guest-inputs-before-dispatch.json' $guestInputs
 foreach($name in @('Invoke-OwnedProcess.ps1','Test-Retained-Install.ps1')) {
  $destination=Join-Path $guestRun $name
  $expected=$current.input_hashes[$name]
  $remote=Invoke-Command -Session $session -ArgumentList $destination -ScriptBlock {param($path)if(Test-Path -LiteralPath $path){(Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash}}
  if($remote -and $remote -cne $expected){throw 'Preserve mismatching guest script; no overwrite'}
  if(!$remote){Copy-Item -LiteralPath (Join-Path $old $name) -Destination $destination -ToSession $session}
 }
 $phase='dispatching-retained-guest-job'
 $job=Invoke-Command -Session $session -AsJob -ArgumentList $guestRun,$current.input_hashes -ScriptBlock {
  param($run,$hashes)
  foreach($name in @('Invoke-OwnedProcess.ps1','Test-Retained-Install.ps1','baseline.json','package-report.json','os_probe_owner.exe','OwnerFixed\swarm-runtime.exe')){if((Get-FileHash -LiteralPath (Join-Path $run $name)).Hash -cne $hashes[$name]){throw 'Guest input hash mismatch'}}
  if(@(Get-ExecutionPolicy -List|Where-Object{$_.Scope -in @('MachinePolicy','UserPolicy') -and $_.ExecutionPolicy -ne 'Undefined'}).Count){throw 'Managed execution policy; no bypass'}
  $before=Get-ExecutionPolicy -List|ConvertTo-Json -Compress
  # Load the helper inside the already-reviewed process-scoped Bypass child.
  # Dot-sourcing it in this default-policy remoting session can fail before
  # that child starts. Do not change the session or persistent policy.
  $childCommand=@'
$ErrorActionPreference='Stop'
$run='C:\Swarm-Validation\Runs\os-20261015'
. "$run\Invoke-OwnedProcess.ps1"
$result=Invoke-OwnedProcess "$env:WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" @('-NoProfile','-NonInteractive','-ExecutionPolicy','Bypass','-File',"$run\Test-Retained-Install.ps1") -TimeoutMilliseconds 420000
$result|ConvertTo-Json -Depth 6 -Compress
'@
  $encoded=[Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($childCommand))
  $childOutput=& "$env:WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand $encoded
  if($LASTEXITCODE -ne 0){throw 'Retained child launcher failed; no retry'}
  $result=($childOutput|Out-String)|ConvertFrom-Json
  if((Get-ExecutionPolicy -List|ConvertTo-Json -Compress) -cne $before){throw 'Persistent policy changed'}
  @{process=$result;receipt=(Get-Content "$run\retained-install-result.json" -Raw -Encoding UTF8|ConvertFrom-Json)}
 }
 $deadline=[DateTime]::UtcNow.AddSeconds(440)
 while($job.State -notin @('Completed','Failed','Stopped') -and [DateTime]::UtcNow -lt $deadline){$null=Wait-Job $job -Timeout 15}
 if($job.State -ne 'Completed'){throw 'Guest operation incomplete; preserve job/process/VM, do not retry'}
 $result=Receive-Job $job -ErrorAction Stop
 Save-New 'guest-result.json' $result
 Remove-Job $job;$job=$null
 if($result.process.exit_code -ne 0 -or $result.receipt.status -ne 'PASS'){throw 'Guest residual failed; preserve VM and state'}
 # Copy only completed receipts. Never copy a live redirected writer.
 foreach($name in @('retained-install-result.json','retained-runtime-streams.json')){Copy-Item -LiteralPath (Join-Path $guestRun $name) -Destination (Join-Path $out $name) -FromSession $session}
 $active=Invoke-Command -Session $session -ScriptBlock {@(Get-Process -Name swarm,swarm-runtime,swarm-desktop -ErrorAction SilentlyContinue).Count}
 if($active -ne 0){throw 'Guest Swarm still active; preserve VM'}
 Remove-PSSession $session;$session=$null
 Stop-VM -VM (Get-VM -Id $vmId)
 if((Get-VM -Id $vmId).State -ne 'Off'){throw 'Normal shutdown not confirmed'}
 Save-New 'result.json' @{status='PASS';vm_id=$vmId.ToString();final_state='Off';native_ui_accepted=$false;independent_acl_accepted=$false;formal_ac_accepted=$false;at=[DateTimeOffset]::Now.ToString('o')}
}catch{
 Save-New 'result.json' @{status='FAIL';phase=$phase;error=$_.Exception.Message;vm_id=$vmId.ToString();state_preserved=$true;retry_allowed=$false;at=[DateTimeOffset]::Now.ToString('o')}
 exit 1
}finally{
 if($session -and (!$job -or $job.State -eq 'Completed')){Remove-PSSession $session}
 $credential=$null
}
