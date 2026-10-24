#requires -Version 5.1
param([switch]$Prepare,[switch]$ValidateOnly,[switch]$Execute,[switch]$AdoptRunning,[switch]$ResumeStaged,[switch]$AfterManualStart,[switch]$AfterEnabledReboot,[ValidatePattern('^[a-z0-9-]{1,48}$')][string]$ReceiptRevision='initial')
$ErrorActionPreference='Stop'
$workspace='C:\Users\Unknown\.gemini\antigravity\scratch\swarm'
$out=Join-Path $workspace 'target\vm-startup-native-20261023'
if($ReceiptRevision -ne 'initial'){$out=Join-Path $workspace "target\vm-startup-native-20261023-$ReceiptRevision"}
$probe=Join-Path $workspace 'target\fixture-builds\vm-startup\release\swarm-vm-startup-observer.exe'
$runner=Join-Path $workspace 'scripts\fixtures\vm-startup\Invoke-Probe.ps1'
$vmId=[guid]'16360373-4ac4-455f-81a4-0679644b080d'
$stage='C:\Swarm-Validation\Inputs\startup-20261021'
$guestProgram="$stage\startup-observer-native-20261023.exe"
if($AfterManualStart){$guestProgram="$stage\startup-observer-after-manual-start-20261024.exe";if(!$AdoptRunning -or !$ResumeStaged){throw 'Manual-start continuation requires staged running-VM resume'}}
if($AfterEnabledReboot){if(!$AfterManualStart){throw 'Post-reboot continuation requires its prior lineage'};$guestProgram="$stage\startup-observer-postboot-20261024.exe"}
$run='C:\Swarm-Validation\Runs\startup-20261021'
$trace="$run\native-20261023\traces"
function Save-New($name,$value){
 $bytes=(New-Object Text.UTF8Encoding($false)).GetBytes(($value|ConvertTo-Json -Depth 14))
 $f=[IO.File]::Open((Join-Path $out $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write)
 try{$f.Write($bytes,0,$bytes.Length)}finally{$f.Dispose()}
}
function Inputs {
 $files=@{probe=$probe;runner=$runner;source="$workspace\scripts\fixtures\vm-startup\src\main.rs";session="$workspace\scripts\fixtures\vm-startup\session.rs";supervisor=$PSCommandPath;guard="$workspace\scripts\fixtures\vm-startup\Guest-Guard.ps1"}
 $pins=@{};foreach($key in $files.Keys){$pins[$key]=(Get-FileHash -LiteralPath $files[$key]).Hash};$pins
}
$planName=if($AdoptRunning){'plan-adopt-running.json'}else{'plan.json'}
if($Prepare){if(!(Test-Path -LiteralPath $out)){New-Item -ItemType Directory -Path $out|Out-Null};Save-New $planName @{incident='vm-os-20261015';authorization='20261023-standing-continuation';vm_id=$vmId.ToString();adopt_running=$AdoptRunning.IsPresent;original_bootstrap_replayed=$false;receipt_lineage="$run\native-20261023";input_hashes=(Inputs);temporary_startup_memory=4GB;restore_original_memory=$true;native_acceptance=$false};exit 0}
$plan=Get-Content -Raw -Encoding UTF8 (Join-Path $out $planName)|ConvertFrom-Json
$pins=Inputs
foreach($key in $pins.Keys){if($pins[$key] -cne $plan.input_hashes.$key){throw 'Prepared input changed'}}
if($ValidateOnly){@{pins_match=$true;vm_access=$false;guest_operation=$false}|ConvertTo-Json;exit 0}
if(!$Execute){throw 'Explicit Prepare, ValidateOnly or Execute required'}
$principal=New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Normal Windows elevation required'}
Import-Module Hyper-V
$proof=Get-Content -Raw -Encoding UTF8 "$workspace\target\batched-roadmap-20261015-os\vm-ready.json"|ConvertFrom-Json
function Owned-VM {
 $v=Get-VM -Id $vmId
 if($v.Name -cne 'Swarm-Validation-Win11' -or $v.Id.ToString() -cne $proof.vm_id){throw 'Wrong owned VM'}
 $disks=@($v|Get-VMHardDiskDrive);if($disks.Count -ne 1){throw 'Wrong disk count'}
 $d=Get-VHD -Path $disks[0].Path
 if($d.Path -ine $proof.disk_chain[0] -or $d.ParentPath -ine $proof.disk_chain[1] -or (Get-VHD -Path $d.ParentPath).ParentPath){throw 'Disk chain changed'}
 if(@($v|Get-VMNetworkAdapter|Where-Object SwitchName).Count -or @($v|Get-VMSnapshot|Where-Object{$_.Id.ToString() -ceq $proof.baseline_id}).Count -ne 1){throw 'Isolation/baseline changed'}
 $v
}
$vm=Owned-VM
if($vm.State -ne $(if($AdoptRunning){'Running'}else{'Off'})){throw 'Expected reviewed owned VM state'}
$originalMemory=Get-VMMemory -VM $vm
$startup=[uint64]$plan.temporary_startup_memory
if(!$AdoptRunning -and ($originalMemory.Minimum -gt $startup -or $originalMemory.Maximum -lt $startup)){throw 'Temporary startup outside original memory bounds'}
$memory=Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory
$required=if($AdoptRunning){1GB}else{$startup+1GB}
if([uint64]$memory.AvailableMBytes*1MB -lt $required -or [uint64]$memory.CommitLimit-[uint64]$memory.CommittedBytes -lt $required){throw 'Actual memory margin unavailable'}
Save-New 'attempt.json' @{incident='vm-os-20261015';at=[DateTimeOffset]::Now.ToString('o');vm_id=$vmId.ToString();original_memory=@{startup=[uint64]$originalMemory.Startup;minimum=[uint64]$originalMemory.Minimum;maximum=[uint64]$originalMemory.Maximum;dynamic=$originalMemory.DynamicMemoryEnabled};temporary_startup=$startup;old_receipts_preserved=$true}
$credential=$null;$session=$null;$connector=$null;$job=$null;$uncertain=$false;$phase='memory-adjustment';$memoryChanged=$false;$callIndex=0
function Open-Direct {
 $script:callIndex++
 for($connectAttempt=0;$connectAttempt -lt 3;$connectAttempt++){
  $script:connector=[powershell]::Create()
  [void]$script:connector.AddScript('param($id,$cred) $ErrorActionPreference="Stop"; New-PSSession -VMId $id -Credential $cred -ErrorAction Stop').AddArgument($vmId).AddArgument($credential)
  try{
   $p=$script:connector.BeginInvoke()
   if(!$p.AsyncWaitHandle.WaitOne(60000)){$script:connector.Stop();throw 'Direct connection deadline'}
   $items=@($script:connector.EndInvoke($p));if($script:connector.HadErrors -or $items.Count -ne 1){throw 'Direct connection failed'};$script:session=$items[0];return
  }catch{
   Save-New "direct-open-failure-$callIndex-$connectAttempt.json" @{phase=$phase;error=$_.Exception.Message;at=[DateTimeOffset]::Now.ToString('o');guest_operation_sent=$false}
   $script:connector.Dispose();$script:connector=$null
   $credentialError='credential|'+(-join @([char]0x8cc7,[char]0x683c,[char]0x60c5,[char]0x5831))
   if($connectAttempt -ge 2 -or !($phase -in @('enabledboot','disabledboot') -or ($phase -eq 'credential' -and $AfterEnabledReboot)) -or (Owned-VM).State -ne 'Running' -or $_.Exception.Message -match $credentialError){throw}
   Start-Sleep -Seconds 15
  }
 }
}
function Close-Direct {
 if($script:session){Remove-PSSession $script:session -ErrorAction SilentlyContinue;$script:session=$null}
 if($script:connector){$script:connector.Dispose();$script:connector=$null}
}
function Probe([string[]]$arguments,[string]$name){
 $script:job=Invoke-Command -Session $session -AsJob -ScriptBlock ${function:Invoke-StartupProbe} -ArgumentList $guestProgram,$arguments,$pins.probe,$trace,"$ReceiptRevision-$name",170000
 if(!(Wait-Job -Job $job -Timeout 180) -or $job.State -ne 'Completed'){$script:uncertain=$true;throw 'Guest probe incomplete; preserve state'}
 $v=Receive-Job $job -ErrorAction Stop;Remove-Job $job;$script:job=$null;$v
}
try{
 if(!$AdoptRunning){
  Set-VMMemory -VM $vm -StartupBytes $startup;$memoryChanged=$true
  if([uint64](Get-VMMemory -VM (Owned-VM)).Startup -ne $startup){throw 'Temporary memory readback mismatch'}
  Start-VM -VM (Owned-VM)|Out-Null
 }
 Save-New 'booted.json' @{state=(Owned-VM).State.ToString();at=[DateTimeOffset]::Now.ToString('o')}
 $phase='credential'
 Write-Host 'Guest Windows credential is used only for this sequence. Do not enter it in chat.'
 Save-New 'awaiting-credential.json' @{username='DESKTOP-1TAP8BI\GPT';secret_recorded=$false}
 $credential=Get-Credential -UserName 'DESKTOP-1TAP8BI\GPT' -Message 'Existing owned VM. Credential stays only in this sequence memory.'
 if(!$credential){throw 'Credential cancelled'}
 Open-Direct
 $phase='old-data-guard'
 $before=Invoke-Command -Session $session -ArgumentList $stage,$run,$pins.guard -ScriptBlock {
  param($stage,$run,$guardHash)
  if($env:COMPUTERNAME -cne 'DESKTOP-1TAP8BI' -or [Security.Principal.WindowsIdentity]::GetCurrent().Name -ine 'DESKTOP-1TAP8BI\GPT'){throw 'Wrong guest'}
  if(!(Test-Path "$run\bootstrap.json") -or !(Test-Path "$run\cleanup.json")){throw 'Original bootstrap/cleanup proof absent'}
  if((Get-FileHash "$stage\Guest-Guard.ps1").Hash -cne $guardHash){throw 'Original guard changed'}
  $text=& "$env:WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "& '$stage\Guest-Guard.ps1' | ConvertTo-Json -Compress"
  if($LASTEXITCODE -ne 0){throw 'Original Data/PATH guard failed'}
  $v=$text|ConvertFrom-Json
  if($v.old_data_files -ne 22 -or $v.old_data_hashes_match -ne $true -or $v.path_unchanged -ne $true){throw 'Original Data/PATH guard result invalid'};$v
 }
 Save-New 'old-data-before.json' $before
 $phase='stage'
 if($ResumeStaged){
  if(!$AdoptRunning -or $ReceiptRevision -notlike 'native-*'){throw 'Staged resume requires its explicit running-VM profile'}
  if($AfterManualStart){
   Invoke-Command -Session $session -ArgumentList $stage,"$run\native-20261023",$guestProgram,$AfterEnabledReboot.IsPresent -ScriptBlock {param($s,$r,$p,$postboot)if((Get-FileHash "$s\startup-observer-native-20261023.exe").Hash -cne 'EE6AD4A2A7E58C5D38125D6001BC8BB0B9E3B29701772008463DD1813F87055F' -or !(Test-Path "$r\b0-stopped.json") -or !(Test-Path "$r\sameboot-attempt.json") -or (Test-Path $p)){throw 'Prior manual-start continuation boundary mismatch'};if($postboot){if(!(Test-Path "$r\sameboot.json") -or (Test-Path "$r\enabledboot-attempt.json")){throw 'Post-reboot phase boundary mismatch'}}elseif(Test-Path "$r\sameboot.json"){throw 'Completed sameboot must not be replayed'}}
   Copy-Item -LiteralPath $probe -Destination $guestProgram -ToSession $session
  }else{
   Invoke-Command -Session $session -ArgumentList $guestProgram,"$run\native-20261023",$pins.probe -ScriptBlock {param($p,$r,$hash)if(!(Test-Path $r) -or (Get-FileHash $p).Hash -cne $hash -or (Test-Path "$r\b0-running-attempt.json")){throw 'Staged continuation identity or phase mismatch'}}
  }
 }else{
  Invoke-Command -Session $session -ArgumentList $guestProgram,"$run\native-20261023" -ScriptBlock {param($p,$r)if((Test-Path $p) -or (Test-Path $r)){throw 'Existing continuation preserved'}}
  Copy-Item -LiteralPath $probe -Destination $guestProgram -ToSession $session
  Invoke-Command -Session $session -ArgumentList $trace -ScriptBlock {param($trace)New-Item -ItemType Directory -Path $trace -ErrorAction Stop|Out-Null}
 }
 . $runner
 Save-New 'initial-metadata.json' (Probe @('--continue','--metadata') $(if($ResumeStaged){"initial-$ReceiptRevision"}else{'initial'}))
 Close-Direct
 if(!@(Get-Process -Name vmconnect -ErrorAction SilentlyContinue|Where-Object{$_.MainWindowTitle -like '*Swarm-Validation-Win11*'}).Count){Start-Process -FilePath "$env:WINDIR\System32\vmconnect.exe" -ArgumentList @('localhost','Swarm-Validation-Win11') -WindowStyle Normal}
 Save-New 'waiting-native.json' @{next=$(if($AfterEnabledReboot){'enabledboot'}elseif($AfterManualStart){'signout'}else{'b0-running'});guest_shortcut="$stage\Open Swarm.lnk";guest_signin_required=$true;deadline_minutes=45}
 $deadline=[DateTime]::UtcNow.AddMinutes(45)
 $steps=if($AfterEnabledReboot){@('enabledboot','disabled-set','reboot-disabled','disabledboot','cleanup')}elseif($AfterManualStart){@('signout','sameboot','reboot-enabled','enabledboot','disabled-set','reboot-disabled','disabledboot','cleanup')}else{@('b0-running','b0-stopped','signout','sameboot','reboot-enabled','enabledboot','disabled-set','reboot-disabled','disabledboot','cleanup')}
 foreach($step in $steps){
  $phase=$step;$request="$out\request-$step.json"
  while(!(Test-Path $request)){
   if(Test-Path "$out\request-abort.json"){
    $abort=Get-Content -Raw -Encoding UTF8 "$out\request-abort.json"|ConvertFrom-Json
    if($abort.continue -ne $true -or $abort.mode -cne 'normal-cleanup'){throw 'Invalid scoped abort request'}
    Open-Direct
    $writers=Invoke-Command -Session $session -ArgumentList $stage -ScriptBlock {param($stage)@(Get-CimInstance Win32_Process|Where-Object{$_.ExecutablePath -ieq "$stage\swarm-desktop.exe"}).Count}
    if($writers -ne 0){throw 'Close native GUI before scoped cleanup'}
    Save-New 'abort-cleanup-product.json' (Probe @('--continue','--cleanup') 'abort-cleanup')
    $guard=Invoke-Command -Session $session -ArgumentList $stage -ScriptBlock {param($stage)$text=& "$env:WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "& '$stage\Guest-Guard.ps1' | ConvertTo-Json -Compress";if($LASTEXITCODE -ne 0){throw 'Abort old Data guard failed'};$text|ConvertFrom-Json}
    Save-New 'old-data-after.json' $guard
    New-Item -ItemType Directory -Path "$out\guest-receipts" -ErrorAction Stop|Out-Null
    Copy-Item -Path "$run\native-20261023\*.json" -Destination "$out\guest-receipts" -FromSession $session
    Close-Direct;Stop-VM -VM (Owned-VM)
    if((Owned-VM).State -ne 'Off'){throw 'Normal abort shutdown not established'}
    Save-New 'abort-result.json' @{normal_cleanup_complete=$true;phase_not_accepted=$step;formal_ac_accepted=$false};return
   }
   $diagnostic="$out\request-diagnostic-$step.json"
   if((Test-Path $diagnostic) -and !(Test-Path "$out\diagnostic-$step.json")){
    $inspect=Get-Content -Raw -Encoding UTF8 $diagnostic|ConvertFrom-Json
    if($inspect.step -cne $step -or $inspect.continue -ne $true){throw 'Invalid read-only diagnostic request'}
    Open-Direct;Save-New "diagnostic-$step.json" (Probe @('--continue','--metadata') "diagnostic-$step");Close-Direct
   }
   if([DateTime]::UtcNow -ge $deadline){throw 'Native interaction deadline; preserve state'};Start-Sleep -Seconds 1
  }
  $r=Get-Content -Raw -Encoding UTF8 $request|ConvertFrom-Json
  if($r.step -cne $step -or $r.continue -ne $true){throw 'Wrong phase request'}
  Save-New "dispatch-$step.json" @{at=[DateTimeOffset]::Now.ToString('o');step=$step}
  Open-Direct
  if($step -in @('signout','reboot-enabled','reboot-disabled')){
   $m=Probe @('--continue','--metadata') "before-$step";Save-New "before-$step.json" $m
   if($m.product.interactive.active -ne $true){throw 'Actual active GPT session required'}
   if($step -eq 'reboot-disabled' -and ($m.product.runtime.state -ne 'stopped' -or $m.product.startup.enabled -ne $false)){throw 'Native full-stop plus disabled startup required'}
   Invoke-Command -Session $session -ArgumentList $step,$m.product.interactive.session_id -ScriptBlock {param($step,$id)if($step -eq 'signout'){& "$env:WINDIR\System32\logoff.exe" $id;if($LASTEXITCODE -ne 0){throw 'Normal signout failed'}}else{Start-Process "$env:WINDIR\System32\shutdown.exe" -ArgumentList @('/r','/t','5') -WindowStyle Hidden}}
   Save-New "result-$step.json" @{normal_transition_requested=$true;acceptance_pending_after_readback=$true}
  }elseif($step -eq 'cleanup'){
   Save-New 'cleanup-product.json' (Probe @('--continue','--cleanup') 'cleanup')
   $after=Invoke-Command -Session $session -ArgumentList $stage -ScriptBlock {param($stage)$text=& "$env:WINDIR\System32\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "& '$stage\Guest-Guard.ps1' | ConvertTo-Json -Compress";if($LASTEXITCODE -ne 0){throw 'Final old Data/PATH guard failed'};$v=$text|ConvertFrom-Json;if($v.old_data_files -ne 22 -or $v.old_data_hashes_match -ne $true -or $v.path_unchanged -ne $true){throw 'Final old Data/PATH guard result invalid'};$v}
   Save-New 'old-data-after.json' $after
   New-Item -ItemType Directory -Path "$out\guest-receipts" -ErrorAction Stop|Out-Null
   Copy-Item -Path "$run\native-20261023\*.json" -Destination "$out\guest-receipts" -FromSession $session
  }else{
   $arguments=if($AfterManualStart -and $step -eq 'sameboot'){@('--continue','--recover-sameboot')}else{@('--continue','--observe',$step)}
   $v=Probe $arguments $step
   $runtime=@($v.processes|Where-Object Name -eq 'swarm-runtime.exe');$gui=@($v.processes|Where-Object Name -eq 'swarm-desktop.exe')
   if($gui.Count -ne 0 -or ($step -in @('b0-running','enabledboot','disabled-set') -and $runtime.Count -ne 1) -or ($step -in @('b0-stopped','sameboot','disabledboot') -and $runtime.Count -ne 0)){throw 'Actual owned process state mismatch'}
   Save-New "result-$step.json" $v
  }
  Close-Direct
 }
 $phase='shutdown';Stop-VM -VM (Owned-VM)
 if((Owned-VM).State -ne 'Off'){throw 'Normal shutdown not established'}
 Save-New 'sequence-result.json' @{status='SCOPED_PRODUCT_SEQUENCE_COMPLETE';formal_ac_accepted=$false;native_evidence_separate=$true}
}catch{
 Save-New 'sequence-failure.json' @{phase=$phase;uncertain=$uncertain;error=$_.Exception.Message;at=[DateTimeOffset]::Now.ToString('o');evidence_preserved=$true}
 if($session){
  try{
   $failureDir=Join-Path $out 'failure-guest-json';New-Item -ItemType Directory -Path $failureDir -ErrorAction Stop|Out-Null
   Copy-Item -Path "$run\native-20261023\*.json" -Destination $failureDir -FromSession $session -ErrorAction Stop
   Copy-Item -Path "$trace\$ReceiptRevision-*.json" -Destination $failureDir -FromSession $session -ErrorAction Stop
  }catch{Save-New 'failure-export-incomplete.json' @{evidence_preserved_in_guest=$true;at=[DateTimeOffset]::Now.ToString('o')}}
 }
 throw
}finally{
 Close-Direct;$credential=$null
 if((Owned-VM).State -eq 'Off'){
  if($memoryChanged){Set-VMMemory -VM (Owned-VM) -StartupBytes ([uint64]$originalMemory.Startup)}
  $final=Get-VMMemory -VM (Owned-VM)
  if([uint64]$final.Startup -ne [uint64]$originalMemory.Startup -or [uint64]$final.Minimum -ne [uint64]$originalMemory.Minimum -or [uint64]$final.Maximum -ne [uint64]$originalMemory.Maximum -or $final.DynamicMemoryEnabled -ne $originalMemory.DynamicMemoryEnabled){throw 'Original VM memory restoration not verified'}
  Save-New 'host-closeout.json' @{vm_state='Off';original_memory_restored=$true;chain_baseline_network_verified=$true;at=[DateTimeOffset]::Now.ToString('o')}
 }
}
