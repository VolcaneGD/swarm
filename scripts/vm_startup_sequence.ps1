#requires -Version 5.1
param([switch]$Prepare,[switch]$ValidateOnly,[switch]$ExecuteApproved)
$ErrorActionPreference='Stop'
$workspace='C:\Users\Unknown\.gemini\antigravity\scratch\swarm'
$out=Join-Path $workspace 'target\vm-startup-20261021'
$source=Join-Path $workspace 'scripts\fixtures\vm-startup'
$candidate=Join-Path $workspace 'target\distribution\candidate-20261019-account-feedback'
$vmId=[guid]'16360373-4ac4-455f-81a4-0679644b080d'
$stage='C:\Swarm-Validation\Inputs\startup-20261021'
$run='C:\Swarm-Validation\Runs\startup-20261021'
$scope='minimum-startup-os-sequence-once'
# Source repairs never reopen this consumed incident, including preparation mode.
if(Test-Path -LiteralPath "$out\attempt.json"){throw 'Added sequence consumed; no retry or new plan'}
function Save-New($name,$value){
 $bytes=(New-Object Text.UTF8Encoding($false)).GetBytes(($value|ConvertTo-Json -Depth 15))
 $stream=[IO.File]::Open((Join-Path $out $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
 try{$stream.Write($bytes,0,$bytes.Length)}finally{$stream.Dispose()}
}
function Get-Inputs {
 $paths=@{supervisor=$PSCommandPath;probe="$out\startup-observer.exe";probe_source="$source\src\main.rs";session_source="$source\session.rs";runner="$source\Invoke-Probe.ps1";guest_guard="$source\Guest-Guard.ps1";inspector="$out\Inspect-Owned-VM.ps1";prior_consumed="$workspace\target\vm-acl-20261020\result.json";prior_attempt="$workspace\target\vm-acl-20261020\attempt.json";baseline="$workspace\target\batched-roadmap-20261015-os\baseline.json";vm_proof="$workspace\target\batched-roadmap-20261015-os\vm-ready.json"}
 foreach($name in @('swarm-desktop.exe','swarm-runtime.exe','swarm.exe')){$paths[$name]=Join-Path $candidate $name}
 $pins=@{};foreach($name in $paths.Keys){$pins[$name]=(Get-FileHash -LiteralPath $paths[$name]).Hash};$pins
}
if($Prepare){Save-New 'plan.json' @{incident='vm-os-20261015';scope=$scope;vm_id=$vmId.ToString();attempts=1;automatic_retries=0;initial_boots=1;normal_signouts=1;normal_reboots=2;shutdown='normal';guest_run=$run;guest_stage=$stage;input_hashes=(Get-Inputs);sequence_minutes=30};exit 0}
if(!$ValidateOnly -and !$ExecuteApproved){throw 'Explicit mode required'}
$approval=Get-Content -LiteralPath "$out\approval.json" -Raw -Encoding UTF8|ConvertFrom-Json
$plan=Get-Content -LiteralPath "$out\plan.json" -Raw -Encoding UTF8|ConvertFrom-Json
# ASCII-only source is safe under Windows PowerShell5.1.
$answer=ConvertFrom-Json '"\u3053\u306e\u6700\u5c0fOS\u691c\u67fb\u3092\u8ffd\u52a01sequence\u8a31\u53ef\u3059\u308b"'
if($approval.user_answer -cne $answer -or $approval.incident -cne 'vm-os-20261015' -or $approval.scope -cne $scope -or $approval.vm_id -cne $vmId.ToString() -or $approval.attempts -ne 1 -or $approval.automatic_retries -ne 0){throw 'Exact added scope required'}
foreach($key in @('publication','reinstall','checkpoint_restore','power_fault','elapsed_seed','network_change','old_data_change')){if($approval.$key -ne $false){throw 'Excluded action not false'}}
$currentInputs=Get-Inputs
foreach($key in $currentInputs.Keys){if($currentInputs[$key] -cne $plan.input_hashes.$key){throw 'Pinned input changed'}}
if(Test-Path -LiteralPath "$out\attempt.json"){throw 'Added sequence consumed; no retry'}
if($ValidateOnly){@{approval_matches=$true;input_pins_match=$true;vm_access=$false;attempt_consumed=$false}|ConvertTo-Json;exit 0}
$principal=New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Normal host UAC required'}
Import-Module Hyper-V
& "$out\Inspect-Owned-VM.ps1"
$inspection=Get-Content -LiteralPath "$out\host-inspection.json" -Raw -Encoding UTF8|ConvertFrom-Json
if($inspection.status -cne 'PASS' -or $inspection.vm.state -cne 'Off' -or !$inspection.disk_chain_matches -or !$inspection.baseline_present -or !$inspection.network_disconnected -or $inspection.volume.health -cne 'Healthy'){throw 'Live VM guards failed; no attempt consumed'}
$vm=Get-VM -Id $vmId
$os=Get-CimInstance Win32_OperatingSystem
$required=[UInt64](Get-VMMemory -VM $vm).Startup+1GB
$resources=@{required_bytes=$required;physical_free_bytes=[UInt64]$os.FreePhysicalMemory*1KB;commit_free_bytes=[UInt64]$os.FreeVirtualMemory*1KB}
Save-New 'resource-preflight.json' $resources
if($resources.physical_free_bytes -lt $required -or $resources.commit_free_bytes -lt $required){throw 'RAM/commit margin unavailable; no attempt consumed'}
if((Get-VHD $inspection.disk_chain[0]).Attached){throw 'Expected detached disk before boot'}
$credential=$null;$session=$null;$connector=$null;$phase='boot';$job=$null
function Open-Direct {
 # VMId parameter set has no SessionOption/OpenTimeout. Bound its pipeline instead.
 $script:connector=[powershell]::Create()
 [void]$script:connector.AddScript('param($id,$cred) $ErrorActionPreference="Stop"; New-PSSession -VMId $id -Credential $cred -ErrorAction Stop').AddArgument($vmId).AddArgument($credential)
 $pending=$script:connector.BeginInvoke()
 if(!$pending.AsyncWaitHandle.WaitOne(60000)){$script:connector.Stop();throw 'Direct connection exceeded60seconds; no retry'}
 $items=@($script:connector.EndInvoke($pending))
 if($script:connector.HadErrors -or $items.Count -ne 1){throw 'Direct connection failed; no retry'}
 $script:session=$items[0]
}
function Close-Direct {
 if($script:session){Remove-PSSession -Session $script:session -ErrorAction SilentlyContinue;$script:session=$null}
 if($script:connector){$script:connector.Dispose();$script:connector=$null}
}
function Run-Guest($mode,$phaseName){
 if($mode -eq '--observe'){$arguments=@($mode,$phaseName);$traceName=$phaseName}else{$arguments=@($mode);$traceName=$mode.TrimStart('-');if($mode -eq '--metadata'){$traceName="metadata-$phase"}}
 $trace="$stage\probe-traces"
 Invoke-Command -Session $session -ArgumentList $trace,$stage,$plan.input_hashes.guest_guard -ScriptBlock {param($trace,$stage,$hash)if((Get-FileHash -LiteralPath "$stage\Guest-Guard.ps1").Hash -cne $hash){throw 'Guest guard changed'};if(!(Test-Path -LiteralPath $trace)){New-Item -ItemType Directory -Path $trace -ErrorAction Stop|Out-Null}}
 # Explicit .NET parent-process wait; no native PowerShell invocation tree wait.
 $script:job=Invoke-Command -Session $session -AsJob -ScriptBlock ${function:Invoke-StartupProbe} -ArgumentList "$stage\startup-observer.exe",$arguments,$plan.input_hashes.probe,$trace,$traceName,170000
 if(!(Wait-Job -Job $job -Timeout 180) -or $job.State -ne 'Completed'){throw 'Guest observation deadline/failure; no retry'}
 $value=Receive-Job -Job $job -ErrorAction Stop
 Remove-Job -Job $job;$script:job=$null
 $value
}
Save-New 'attempt.json' @{incident='vm-os-20261015';scope=$scope;automatic_retries=0;vm_id=$vmId.ToString();at=[DateTimeOffset]::Now.ToString('o')}
try{
 Start-VM -VM $vm|Out-Null
 # Fixed wait is not acceptance/readiness evidence. Connection follows local input.
 for($i=0;$i -lt 6;$i++){Start-Sleep -Seconds 15}
 $phase='human-credential'
 Save-New 'awaiting-credential.json' @{user='DESKTOP-1TAP8BI\GPT';secret_recorded=$false;at=[DateTimeOffset]::Now.ToString('o')}
 $credential=Get-Credential -UserName 'DESKTOP-1TAP8BI\GPT' -Message 'Approved minimum startup sequence. Password stays only in this process memory.'
 if(!$credential){throw 'Human credential cancelled'}
 $phase='staging'
 . "$source\Invoke-Probe.ps1"
 Open-Direct
 Invoke-Command -Session $session -ArgumentList $stage,$run,$plan.input_hashes.baseline -ScriptBlock {
  param($stage,$run,$baselineHash)
  $ErrorActionPreference='Stop'
  if($env:COMPUTERNAME -cne 'DESKTOP-1TAP8BI' -or [Security.Principal.WindowsIdentity]::GetCurrent().Name -ine 'DESKTOP-1TAP8BI\GPT'){throw 'Wrong guest identity'}
  if((Test-Path -LiteralPath $stage) -or (Test-Path -LiteralPath $run)){throw 'Existing startup root refused; no alternate path'}
  if((Get-FileHash 'C:\Swarm-Validation\Runs\os-20261015\baseline.json').Hash -cne $baselineHash){throw 'Original guest baseline mismatch'}
  New-Item -ItemType Directory -Path $stage -ErrorAction Stop|Out-Null
 }
 foreach($name in @('swarm-desktop.exe','swarm-runtime.exe','swarm.exe')){Copy-Item -LiteralPath "$candidate\$name" -Destination "$stage\$name" -ToSession $session}
 Copy-Item -LiteralPath "$out\startup-observer.exe" -Destination "$stage\startup-observer.exe" -ToSession $session
 Copy-Item -LiteralPath "$source\Guest-Guard.ps1" -Destination "$stage\Guest-Guard.ps1" -ToSession $session
 $guard=Invoke-Command -Session $session -ArgumentList $stage -ScriptBlock {
  param($stage)
  if(@(Get-ExecutionPolicy -List|Where-Object{$_.Scope -in @('MachinePolicy','UserPolicy') -and $_.ExecutionPolicy -ne 'Undefined'}).Count){throw 'Managed policy refused'}
  $raw=& "$env:WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$stage\Guest-Guard.ps1"
  if($LASTEXITCODE -ne 0){throw 'Old Data/PATH guard failed'}
  $shell=New-Object -ComObject WScript.Shell
  $shortcut=$shell.CreateShortcut("$stage\Open Swarm.lnk");$shortcut.TargetPath="$stage\swarm-desktop.exe";$shortcut.Arguments='--data-dir "C:\Swarm-Validation\Runs\startup-20261021\Data"';$shortcut.WorkingDirectory=$stage;$shortcut.Save()
  @{old_guard_passed=$true;shortcut_created=$true;credential_persisted=$false}
 }
 Save-New 'old-data-before.json' $guard
 Save-New 'bootstrap.json' (Run-Guest '--bootstrap' '')
 Close-Direct
 $phase='native-sequence';$deadline=[DateTime]::UtcNow.AddMinutes(30)
 Save-New 'waiting-native.json' @{shortcut="$stage\Open Swarm.lnk";next='b0-running';at=[DateTimeOffset]::Now.ToString('o');native_accepted=$false}
 $steps=@('b0-running','b0-stopped','signout','sameboot','reboot-enabled','enabledboot','disabled-set','reboot-disabled','disabledboot','cleanup')
 for($index=0;$index -lt $steps.Count;$index++){
  $step=$steps[$index];$phase=$step;$request="$out\request-$step.json"
  while(!(Test-Path -LiteralPath $request)){if([DateTime]::UtcNow -ge $deadline){throw 'Sequence allowance expired; preserve state, no retry'};Start-Sleep -Seconds 1}
  $gate=Get-Content -LiteralPath $request -Raw -Encoding UTF8|ConvertFrom-Json
  if($gate.step -cne $step -or $gate.scope -cne $scope -or $gate.continue -ne $true){throw 'Wrong explicit phase request'}
  Save-New "dispatch-$step.json" @{step=$step;automatic_retries=0;at=[DateTimeOffset]::Now.ToString('o')}
  Open-Direct
  if($step -in @('signout','reboot-enabled','reboot-disabled')){
   $before=Run-Guest '--metadata' ''
   if($before.product.interactive.active -ne $true -or $before.product.interactive.username -ine 'GPT' -or $before.product.interactive.domain -ine 'DESKTOP-1TAP8BI'){throw 'Exactly one active guest GPT session required for transition'}
   Save-New "before-$step.json" $before
   Invoke-Command -Session $session -ArgumentList $step,$before.product.interactive.session_id -ScriptBlock {
    param($step,$consoleId)
    if($step -eq 'signout'){& "$env:WINDIR\System32\logoff.exe" $consoleId;if($LASTEXITCODE -ne 0){throw 'Normal console signout failed'}}
    else{Start-Process -FilePath "$env:WINDIR\System32\shutdown.exe" -ArgumentList @('/r','/t','0') -WindowStyle Hidden}
   }
   Save-New "result-$step.json" @{transition_requested=$true;native_accepted=$false;human_signin_required=$true}
  }elseif($step -eq 'cleanup'){
   Save-New 'cleanup-product.json' (Run-Guest '--cleanup' '')
   $final=Invoke-Command -Session $session -ArgumentList $stage -ScriptBlock {param($stage)$text=& "$env:WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$stage\Guest-Guard.ps1";if($LASTEXITCODE -ne 0){throw 'Final old Data/PATH guard failed'};@{old_guard_passed=$true}}
   Save-New 'old-data-after.json' $final
   New-Item -ItemType Directory -Path "$out\guest-receipts" -ErrorAction Stop|Out-Null
   $receiptNames=Invoke-Command -Session $session -ArgumentList $run -ScriptBlock {param($run)@(Get-ChildItem -LiteralPath $run -Filter '*.json' -File|Select-Object -ExpandProperty Name)}
   foreach($name in $receiptNames){Copy-Item -LiteralPath "$run\$name" -Destination "$out\guest-receipts\$name" -FromSession $session}
  }else{
   $v=Run-Guest '--observe' $step
   $runtime=@($v.processes|Where-Object Name -eq 'swarm-runtime.exe');$gui=@($v.processes|Where-Object Name -eq 'swarm-desktop.exe')
   if($step -in @('b0-running','enabledboot')){if($runtime.Count -ne 1 -or $gui.Count -ne 0){throw 'Expected background runtime only'}}
   if($step -in @('b0-stopped','sameboot','disabledboot')){if($runtime.Count -ne 0 -or $gui.Count -ne 0){throw 'Stopped phase has owned runtime/GUI'}}
   Save-New "result-$step.json" $v
  }
  Close-Direct
 }
 $phase='normal-shutdown';Stop-VM -VM (Get-VM -Id $vmId)
 & "$out\Inspect-Owned-VM.ps1" -ReceiptName 'host-after-attempt.json'
 $after=Get-Content -LiteralPath "$out\host-after-attempt.json" -Raw -Encoding UTF8|ConvertFrom-Json
 if($after.vm.state -cne 'Off' -or !$after.disk_chain_matches -or !$after.baseline_present -or !$after.network_disconnected){throw 'Final VM guard failed'}
 Save-New 'result.json' @{status='PRODUCT_SEQUENCE_COMPLETE_NATIVE_EVIDENCE_SEPARATE';final_state='Off';automatic_retries=0;formal_ac_accepted=$false}
}catch{
 Save-New 'result.json' @{status='FAIL';phase=$phase;error=$_.Exception.Message;state_preserved=$true;automatic_retries=0;at=[DateTimeOffset]::Now.ToString('o')}
 exit 1
}finally{Close-Direct;$credential=$null;if($job -and $job.State -eq 'Completed'){Remove-Job -Job $job}}
