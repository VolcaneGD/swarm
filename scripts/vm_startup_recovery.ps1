#requires -Version 5.1
param([switch]$ValidateOnly,[switch]$ExecuteProbe,[switch]$ResumeProbe)
$ErrorActionPreference='Stop'
$workspace='C:\Users\Unknown\.gemini\antigravity\scratch\swarm'
$output=Join-Path $workspace 'target\vm-startup-recovery-20261023'
$vmId=[guid]'16360373-4ac4-455f-81a4-0679644b080d'
$guestStage='C:\Swarm-Validation\Inputs\startup-20261021'
$guestRun='C:\Swarm-Validation\Runs\startup-20261021'
$probe=Join-Path $workspace 'target\vm-startup-helper-repair-20261021\observer-source-repair-not-dispatched.exe'
$expectedProbe='65AC2CE565A0935B1F0EF077A66B926F24538BFA323776910343260E802E28FD'
$runner=Join-Path $workspace 'scripts\fixtures\vm-startup\Invoke-Probe.ps1'
$expectedRunner='E519505A43F2F741F0998FA001515AA7540B65D09785D78E4E9595A063852077'
$continuing=$ResumeProbe.IsPresent
function Save-New($Name,$Value){
 if($continuing){$Name=$Name -replace '^probe-','probe-interactive-'}
 $bytes=(New-Object Text.UTF8Encoding($false)).GetBytes(($Value|ConvertTo-Json -Depth 12))
 $stream=[IO.File]::Open((Join-Path $output $Name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write)
 try{$stream.Write($bytes,0,$bytes.Length)}finally{$stream.Dispose()}
}
if((Get-FileHash -LiteralPath $probe).Hash -cne $expectedProbe -or (Get-FileHash -LiteralPath $runner).Hash -cne $expectedRunner){throw 'Reviewed recovery input changed'}
if($ValidateOnly){@{source_pins_match=$true;vm_access=$false;guest_operation=$false;approval='latest-standing-authorization'}|ConvertTo-Json;exit 0}
if(!$ExecuteProbe -and !$ResumeProbe){throw 'Explicit ValidateOnly, ExecuteProbe or ResumeProbe required'}
$principal=New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Normal Windows elevation required'}
Import-Module Hyper-V
$proof=Get-Content -Raw -Encoding UTF8 "$workspace\target\batched-roadmap-20261015-os\vm-ready.json"|ConvertFrom-Json
$vm=Get-VM -Id $vmId
if($vm.Name -cne 'Swarm-Validation-Win11' -or $vm.Id.ToString() -cne $proof.vm_id){throw 'Expected exact owned VM'}
if($continuing){
 $priorAttempt=Get-Content -Raw -Encoding UTF8 "$output\probe-attempt.json"|ConvertFrom-Json
 $cancelled=Get-Content -Raw -Encoding UTF8 "$output\hidden-credential-cancelled.json"|ConvertFrom-Json
 if($vm.State -ne 'Running' -or $priorAttempt.vm_id -cne $vmId.ToString() -or !$cancelled.no_guest_operation_started -or (Get-Process -Id $cancelled.pid -ErrorAction SilentlyContinue)){throw 'Prior same-VM credential-wait ownership not established'}
}elseif($vm.State -ne 'Off'){throw 'Expected owned Off VM'}
$disks=@($vm|Get-VMHardDiskDrive)
if($disks.Count -ne 1){throw 'Unexpected disk count'}
$disk=Get-VHD -Path $disks[0].Path
if($disk.Path -ine $proof.disk_chain[0] -or $disk.ParentPath -ine $proof.disk_chain[1] -or (Get-VHD -Path $disk.ParentPath).ParentPath){throw 'Disk chain changed'}
if(@($vm|Get-VMNetworkAdapter|Where-Object SwitchName).Count -ne 0){throw 'Network isolation changed'}
if(@($vm|Get-VMSnapshot|Where-Object{$_.Id.ToString() -ceq $proof.baseline_id}).Count -ne 1){throw 'Baseline changed'}
$memory=Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory
$startup=[uint64](Get-VMMemory -VM $vm).Startup
# This operation only reads metadata; it does not start Swarm/GUI or run builds.
# Retain a512MiB host margin here; the full product sequence keeps its1GiB margin.
$required=if($continuing){512MB}else{$startup+512MB}
$resources=@{at=[DateTimeOffset]::Now.ToString('o');vm_startup=$startup;metadata_only_margin=512MB;full_product_sequence=$false;required=$required;physical_free=[uint64]$memory.AvailableMBytes*1MB;commit_free=[uint64]$memory.CommitLimit-[uint64]$memory.CommittedBytes}
Save-New ("probe-resource-preflight-"+[DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfff')+'.json') $resources
if($resources.physical_free -lt $required -or $resources.commit_free -lt $required){throw 'Current RAM/commit margin insufficient; no boot attempted'}
Save-New 'probe-attempt.json' @{incident='vm-os-20261015';continuation='20261023-real-guest-dependency';old_attempts_preserved=$true;original_bootstrap_replayed=$false;vm_id=$vmId.ToString();at=[DateTimeOffset]::Now.ToString('o')}
$session=$null;$connector=$null;$credential=$null;$started=$false;$uncertain=$false;$phase='boot';$job=$null
try{
 if(!$continuing){Start-VM -VM $vm|Out-Null};$started=$true
 Save-New 'probe-booted.json' @{vm_id=$vmId.ToString();state=(Get-VM -Id $vmId).State.ToString();at=[DateTimeOffset]::Now.ToString('o')}
 $deadline=[DateTime]::UtcNow.AddSeconds(90)
 do{
  $heartbeatName=ConvertFrom-Json '"\u30cf\u30fc\u30c8\u30d3\u30fc\u30c8"'
  $normalStatus=ConvertFrom-Json '"\u6b63\u5e38"'
  $heartbeat=(Get-VMIntegrationService -VM (Get-VM -Id $vmId)|Where-Object{$_.Name -in @('Heartbeat',$heartbeatName)})
  if($heartbeat.PrimaryStatusDescription -in @('OK',$normalStatus)){break}
  Start-Sleep -Seconds 2
 }while([DateTime]::UtcNow -lt $deadline)
 $phase='guest-credential'
 Save-New 'probe-awaiting-credential.json' @{username='DESKTOP-1TAP8BI\GPT';secret_recorded=$false;at=[DateTimeOffset]::Now.ToString('o')}
 $credential=Get-Credential -UserName 'DESKTOP-1TAP8BI\GPT' -Message 'Swarm owned VM recovery: existing GPT Windows password stays only in this process memory. Do not put it in chat.'
 if(!$credential){throw 'Guest credential input cancelled'}
 $phase='direct-connect'
 $connector=[powershell]::Create()
 [void]$connector.AddScript('param($id,$cred) $ErrorActionPreference="Stop"; New-PSSession -VMId $id -Credential $cred -ErrorAction Stop').AddArgument($vmId).AddArgument($credential)
 $pending=$connector.BeginInvoke()
 if(!$pending.AsyncWaitHandle.WaitOne(60000)){$connector.Stop();throw 'Direct connection deadline; preserve diagnostic boundary'}
 $items=@($connector.EndInvoke($pending))
 if($connector.HadErrors -or $items.Count -ne 1){throw 'Direct connection failed'}
 $session=$items[0]
 $phase='guard'
 $prior=Invoke-Command -Session $session -ArgumentList $guestStage,$guestRun -ScriptBlock {
  param($stage,$run)
  if($env:COMPUTERNAME -cne 'DESKTOP-1TAP8BI' -or [Security.Principal.WindowsIdentity]::GetCurrent().Name -ine 'DESKTOP-1TAP8BI\GPT'){throw 'Wrong guest identity'}
  if(!(Test-Path -LiteralPath "$run\bootstrap.json") -or !(Test-Path -LiteralPath "$run\cleanup.json")){throw 'Prior completed bootstrap/cleanup absent; no reseed'}
  if((Get-FileHash -LiteralPath "$stage\Guest-Guard.ps1").Hash -cne 'B1F57181C34DC64DA2DFFF8F380537D33856A379D3E6FE0F1B792E9CC01DF69F'){throw 'Guest guard hash mismatch'}
  @{guest=$env:COMPUTERNAME;prior_bootstrap_present=$true;prior_cleanup_present=$true}
 }
 Save-New 'probe-prior.json' $prior
 $phase='stage-repaired-observer'
 $guestProgram="$guestStage\startup-observer-recovery-20261023.exe"
 Invoke-Command -Session $session -ArgumentList $guestProgram -ScriptBlock {param($path)if(Test-Path -LiteralPath $path){throw 'Preserve existing recovery observer'}}
 Copy-Item -LiteralPath $probe -Destination $guestProgram -ToSession $session
 $trace="$guestRun\recovery-20261023"
 Invoke-Command -Session $session -ArgumentList $trace -ScriptBlock {param($path)if(Test-Path -LiteralPath $path){throw 'Preserve existing recovery traces'};New-Item -ItemType Directory -Path $path -ErrorAction Stop|Out-Null}
 . $runner
 $phase='real-guest-metadata'
 $job=Invoke-Command -Session $session -AsJob -ScriptBlock ${function:Invoke-StartupProbe} -ArgumentList $guestProgram,@('--metadata'),$expectedProbe,$trace,'metadata',170000
 if(!(Wait-Job -Job $job -Timeout 180) -or $job.State -ne 'Completed'){$uncertain=$true;throw 'Real guest probe incomplete; retain VM/process state'}
 $result=Receive-Job -Job $job -ErrorAction Stop
 Remove-Job -Job $job;$job=$null
 Save-New 'probe-result.json' @{status='REAL_GUEST_METADATA_RETURNED';observation=$result;startup_acceptance=$false;native_acceptance=$false}
 $phase='export-traces'
 New-Item -ItemType Directory -Path "$output\guest-probe-traces" -ErrorAction Stop|Out-Null
 Copy-Item -Path "$trace\*.json" -Destination "$output\guest-probe-traces" -FromSession $session
}catch{
 Save-New 'probe-failure.json' @{phase=$phase;error=$_.Exception.Message;uncertain=$uncertain;at=[DateTimeOffset]::Now.ToString('o');startup_acceptance=$false}
 throw
}finally{
 if($session){Remove-PSSession -Session $session -ErrorAction SilentlyContinue}
 if($connector){$connector.Dispose()}
 $credential=$null
 if($started -and !$uncertain){
  Stop-VM -VM (Get-VM -Id $vmId) -ErrorAction Continue
  Save-New 'probe-closeout.json' @{vm_id=$vmId.ToString();state=(Get-VM -Id $vmId).State.ToString();normal_shutdown_requested=$true;power_off_forced=$false;at=[DateTimeOffset]::Now.ToString('o')}
 }
}
