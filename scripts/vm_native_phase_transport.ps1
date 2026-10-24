#requires -Version 5.1
param([switch]$Stage,[switch]$Inspect,[switch]$Closeout,[switch]$Shutdown)
$ErrorActionPreference='Stop'
if(@(@($Stage.IsPresent,$Inspect.IsPresent,$Closeout.IsPresent,$Shutdown.IsPresent)|Where-Object{$_}).Count -ne 1){throw 'Choose one fixed operation'}
$root='C:\Users\Unknown\.gemini\antigravity\scratch\swarm'
$out=Join-Path $root 'target\vm-native-phases-20261024'
if(!(Test-Path $out)){New-Item -ItemType Directory -Path $out|Out-Null}
function Save-New($name,$value){$b=[Text.UTF8Encoding]::new($false).GetBytes(($value|ConvertTo-Json -Depth 8));$f=[IO.File]::Open((Join-Path $out $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write);try{$f.Write($b,0,$b.Length)}finally{$f.Dispose()}}
Import-Module Hyper-V
$proof=Get-Content -Raw -Encoding UTF8 "$root\target\batched-roadmap-20261015-os\vm-ready.json"|ConvertFrom-Json
$vm=Get-VM -Id ([guid]'16360373-4ac4-455f-81a4-0679644b080d')
if($vm.Name -cne 'Swarm-Validation-Win11' -or $vm.State -ne 'Running'){throw 'Owned running VM required'}
$disks=@($vm|Get-VMHardDiskDrive);if($disks.Count -ne 1){throw 'Unexpected disks'}
$disk=Get-VHD $disks[0].Path
if($disk.Path -ine $proof.disk_chain[0] -or $disk.ParentPath -ine $proof.disk_chain[1] -or (Get-VHD $disk.ParentPath).ParentPath){throw 'Unexpected chain'}
if(@($vm|Get-VMNetworkAdapter|Where-Object SwitchName).Count -or @($vm|Get-VMSnapshot|Where-Object{$_.Id.ToString() -ceq $proof.baseline_id}).Count -ne 1){throw 'Isolation/baseline changed'}
if($Shutdown){
 Save-New 'shutdown-attempt.json' @{vm_id=$vm.Id.ToString();normal_shutdown_only=$true;at=[DateTimeOffset]::Now.ToString('o')}
 $job=Stop-VM -VM $vm -AsJob
 if(!(Wait-Job $job -Timeout 60) -or $job.State -ne 'Completed'){Save-New 'shutdown-incomplete.json' @{force_used=$false;preserve_vm=$true};throw 'Normal shutdown incomplete'}
 Receive-Job $job -ErrorAction Stop|Out-Null;Remove-Job $job
 $current=Get-VM -Id $vm.Id;$memory=Get-VMMemory -VM $current
 if($current.State -ne 'Off' -or $memory.Startup -ne 6GB -or $memory.DynamicMemoryEnabled){throw 'Off/original memory not established'}
 Save-New 'host-closeout.json' @{vm_state='Off';original_memory_restored=$true;chain_baseline_network_verified=$true;at=[DateTimeOffset]::Now.ToString('o')};exit 0
}
$observer="$root\target\fixture-builds\vm-startup\release\swarm-vm-startup-observer.exe"
$launcher="$root\target\fixture-builds\vm-startup\release\swarm-vm-native-phase.exe"
if((Get-FileHash $observer).Hash -cne '3BCBAC7A49409EBB101702B7029385626E08E4CA4DE239308BB242B698E4DF0E' -or (Get-FileHash $launcher).Hash -cne '35A6A6560DB646789B670968449D523A84443BCC7FEB710C5033F60BBC0D6782'){throw 'Fixed binary pin mismatch'}
$guestWord=-join ([char[]]@(0x30B2,0x30B9,0x30C8))
$services=@($vm|Get-VMIntegrationService|Where-Object{$_.Name -like '*Guest Service*' -or $_.Name -like "*$guestWord*"})
if($services.Count -ne 1){throw 'Guest copy service unresolved'}
$service=$services[0];$original=$service.Enabled;$changed=$false
$result=@{at=[DateTimeOffset]::Now.ToString('o');copied=@();guest_password_requested=$false;original_enabled=$original}
try{
 if(!$original){Enable-VMIntegrationService -VMIntegrationService $service;$changed=$true;Start-Sleep -Seconds 2}
 $destination='C:\Swarm-Validation\Inputs\startup-20261021'
 if($Stage){Copy-VMFile -VM $vm -SourcePath $observer -DestinationPath "$destination\startup-observer-native-manual-20261024.exe" -FileSource Host -ErrorAction Stop;$result.copied+=@('startup-observer-native-manual-20261024.exe')}
 $phases=if($Closeout){@('closeout')}elseif($Inspect){@('inspect')}else{@('enabledboot','disabled-set','disabledboot','cleanup')}
 foreach($phase in $phases){Copy-VMFile -VM $vm -SourcePath $launcher -DestinationPath "$destination\startup-check-$phase.exe" -FileSource Host -ErrorAction Stop;$result.copied+=@("startup-check-$phase.exe")}
}catch{$result.error=$_.Exception.Message}
finally{
 if($changed){Disable-VMIntegrationService -VMIntegrationService $service}
 $result.original_service_state_restored=((Get-VMIntegrationService -VM $vm|Where-Object{$_.Name -ceq $service.Name}).Enabled -eq $original)
 Save-New $(if($Closeout){'closeout-staging.json'}elseif($Inspect){'inspect-staging.json'}else{'staging.json'}) $result
}
if($result.copied.Count -ne $(if($Stage){5}else{1}) -or !$result.original_service_state_restored){exit 1}
