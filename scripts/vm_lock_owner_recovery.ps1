#requires -Version 5.1
# Repair only confirmed Administrators-owned, empty fixture lease files.
param([switch]$Execute,[switch]$LaunchLockOnly,[switch]$BrokerLockOnly)
$ErrorActionPreference='Stop'
$root='C:\Users\Unknown\.gemini\antigravity\scratch\swarm'
$out=Join-Path $root 'target\vm-lock-owner-recovery-20261023-hardlink-guard'
if($LaunchLockOnly){$out=Join-Path $root 'target\vm-launch-lock-owner-recovery-20261023'}
if($BrokerLockOnly){if($LaunchLockOnly){throw 'One explicit lock scope required'};$out=Join-Path $root 'target\vm-broker-lock-owner-recovery-20261023'}
if(!$Execute){throw 'Explicit Execute required'}
$principal=New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Normal elevation required'}
New-Item -ItemType Directory -Path $out -ErrorAction Stop|Out-Null
function Save-New($name,$value){$bytes=[Text.UTF8Encoding]::new($false).GetBytes(($value|ConvertTo-Json -Depth 12));$f=[IO.File]::Open((Join-Path $out $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write);try{$f.Write($bytes,0,$bytes.Length)}finally{$f.Dispose()}}
$credential=$null;$session=$null;$connector=$null
try{
 Import-Module Hyper-V
 $id=[guid]'16360373-4ac4-455f-81a4-0679644b080d';$vm=Get-VM -Id $id
 $proof=Get-Content -Raw -Encoding UTF8 "$root\target\batched-roadmap-20261015-os\vm-ready.json"|ConvertFrom-Json
 if($vm.Name -cne 'Swarm-Validation-Win11' -or $vm.State -ne 'Running'){throw 'Wrong VM/state'}
 $disks=@($vm|Get-VMHardDiskDrive);if($disks.Count -ne 1){throw 'Unexpected disks'};$disk=Get-VHD $disks[0].Path
 if($disk.Path -ine $proof.disk_chain[0] -or $disk.ParentPath -ine $proof.disk_chain[1] -or (Get-VHD $disk.ParentPath).ParentPath){throw 'Unexpected chain'}
 if(@($vm|Get-VMNetworkAdapter|Where-Object SwitchName).Count -or @($vm|Get-VMSnapshot|Where-Object{$_.Id.ToString() -ceq $proof.baseline_id}).Count -ne 1){throw 'Isolation/baseline changed'}
 Save-New 'attempt.json' @{at=[DateTimeOffset]::Now.ToString('o');scope=$(if($LaunchLockOnly){'one empty fixture launch.lock only'}else{'three empty task fixture locks only'});original_user_data_touched=$false}
 $credential=Get-Credential -UserName 'DESKTOP-1TAP8BI\GPT' -Message 'Inspect and repair only the identified empty task-fixture lease files. No original user Data changes.'
 if(!$credential){throw 'Credential cancelled'}
 $connector=[powershell]::Create();[void]$connector.AddScript('param($id,$cred) $ErrorActionPreference="Stop"; New-PSSession -VMId $id -Credential $cred -ErrorAction Stop').AddArgument($id).AddArgument($credential)
 $pending=$connector.BeginInvoke();if(!$pending.AsyncWaitHandle.WaitOne(60000)){$connector.Stop();throw 'Direct deadline'};$items=@($connector.EndInvoke($pending));if($connector.HadErrors -or $items.Count -ne 1){throw 'Direct failed'};$session=$items[0]
 $result=Invoke-Command -Session $session -ArgumentList $LaunchLockOnly.IsPresent,$BrokerLockOnly.IsPresent -ScriptBlock {
  param($launchOnly,$brokerOnly)
  $ErrorActionPreference='Stop'
  $identity=[Security.Principal.WindowsIdentity]::GetCurrent();if($env:COMPUTERNAME -cne 'DESKTOP-1TAP8BI' -or $identity.Name -ine 'DESKTOP-1TAP8BI\GPT'){throw 'Wrong guest'}
  $stage='C:\Swarm-Validation\Inputs\startup-20261021';$base='C:\Swarm-Validation\Runs\startup-20261021';$data="$base\Data";$receipt="$base\native-20261023\lock-owner-recovery.json"
  if($launchOnly){$receipt="$base\native-20261023\launch-lock-owner-recovery.json"}
  if($brokerOnly){$receipt="$base\native-20261023\broker-lock-owner-recovery.json"}
  if((Test-Path $receipt) -or !(Test-Path "$base\bootstrap.json") -or !(Test-Path "$base\cleanup.json")){throw 'Receipt/fixture precondition failed'}
  function Writers { @(Get-CimInstance Win32_Process|Where-Object{$_.ExecutablePath -in @("$stage\swarm-desktop.exe","$stage\swarm-runtime.exe")}) }
  if($launchOnly -or $brokerOnly){$deadline=[DateTime]::UtcNow.AddSeconds(60);while((Writers).Count -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Seconds 1}}
  if((Writers).Count){throw 'Product writer remains'}
  $sid=$identity.User;$adminSid='S-1-5-32-544';$paths=@("$data\gui-state\.host.lock","$data\lifecycle\runtime.lock","$data\lifecycle\startup.lock")
  if($launchOnly){$paths=@("$data\lifecycle\launch.lock")}
  if($brokerOnly){$paths=@("$data\credentials\.broker.lock")}
  $before=@();$handles=@()
  try{
   foreach($path in $paths){
    $item=Get-Item -LiteralPath $path -Force;if($item.PSIsContainer -or $item.Length -ne 0 -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)){throw 'Only existing empty regular locks allowed'}
    $links=@(& "$env:WINDIR\System32\fsutil.exe" hardlink list $path)
    if($LASTEXITCODE -ne 0 -or @($links|Where-Object{$_.Trim()}).Count -ne 1){throw 'Single-link regular lock required before ACL changes'}
    $acl=Get-Acl -LiteralPath $path;$owner=$acl.GetOwner([Security.Principal.SecurityIdentifier]).Value
    $before+=@{path=$path;length=$item.Length;sha256=(Get-FileHash $path).Hash;sddl=$acl.Sddl;owner_is_current_user=($owner -ceq $sid.Value);owner_is_administrators=($owner -ceq $adminSid)}
   }
   $snapshot=@{at=[DateTimeOffset]::Now.ToString('o');before=$before;repair_started=$false;original_user_data_touched=$false}
   [IO.File]::WriteAllText($receipt,($snapshot|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
   if(@($before|Where-Object {!$_.owner_is_administrators}).Count){throw 'Owner is not the predicted Administrators SID; do not repair speculatively'}
   foreach($path in $paths){$handles+=([IO.File]::Open($path,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None))}
   if((Writers).Count){throw 'Product writer appeared'}
   foreach($handle in $handles){$handle.Dispose()};$handles=@()
   $after=@()
   foreach($path in $paths){
    $acl=New-Object Security.AccessControl.FileSecurity
    $acl.SetOwner($sid);$acl.SetAccessRuleProtection($true,$false)
    $rule=New-Object Security.AccessControl.FileSystemAccessRule($sid,[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.AccessControlType]::Allow)
    $acl.AddAccessRule($rule);Set-Acl -LiteralPath $path -AclObject $acl
    $readback=Get-Acl -LiteralPath $path;$rules=@($readback.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]))
    if($readback.GetOwner([Security.Principal.SecurityIdentifier]).Value -cne $sid.Value -or !$readback.AreAccessRulesProtected -or $rules.Count -ne 1 -or $rules[0].IdentityReference.Value -cne $sid.Value -or $rules[0].IsInherited -or $rules[0].FileSystemRights -ne [Security.AccessControl.FileSystemRights]::FullControl){throw 'Private owner/ACL readback mismatch'}
    if((Get-Item $path -Force).Length -ne 0 -or (Get-FileHash $path).Hash -cne $before[0].sha256){throw 'Empty lock content changed'}
    $after+=@{path=$path;length=0;sha256=(Get-FileHash $path).Hash;owner_matches=$true;protected=$true;current_user_only_ace=$true}
   }
   $snapshot.repair_started=$true;$snapshot.after=$after;$snapshot.native_confirmation_pending=$true
   [IO.File]::WriteAllText($receipt,($snapshot|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false));$snapshot
  }finally{foreach($handle in $handles){$handle.Dispose()}}
 }
 Save-New 'result.json' $result
}catch{Save-New 'failure.json' @{at=[DateTimeOffset]::Now.ToString('o');error=$_.Exception.Message;partial_repair_not_assumed_absent=$true};throw}
finally{if($session){Remove-PSSession $session -ErrorAction SilentlyContinue};if($connector){$connector.Dispose()};$credential=$null}
