#requires -Version 5.1
# Read-only inspection of the existing guest launch failure. Never launches the GUI.
param([switch]$Execute,[switch]$LockDetails,[switch]$StartupSequenceDetails)
$ErrorActionPreference='Stop'
$root='C:\Users\Unknown\.gemini\antigravity\scratch\swarm'
$out=Join-Path $root 'target\vm-gui-launch-diagnostic-20261023'
if($LockDetails){$out=Join-Path $root 'target\vm-lock-details-20261023'}
if($StartupSequenceDetails){$out=Join-Path $root 'target\vm-sameboot-diagnostic-20261024'}
if(!$Execute){throw 'Explicit Execute required'}
$principal=New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if(!$principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){throw 'Normal elevation required'}
New-Item -ItemType Directory -Path $out -ErrorAction Stop|Out-Null
function Save-New($name,$value){
 $bytes=[Text.UTF8Encoding]::new($false).GetBytes(($value|ConvertTo-Json -Depth 12))
 $file=[IO.File]::Open((Join-Path $out $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write)
 try{$file.Write($bytes,0,$bytes.Length)}finally{$file.Dispose()}
}
$session=$null;$connector=$null;$credential=$null
try{
 Import-Module Hyper-V
 $id=[guid]'16360373-4ac4-455f-81a4-0679644b080d'
 $vm=Get-VM -Id $id
 $proof=Get-Content -Raw -Encoding UTF8 "$root\target\batched-roadmap-20261015-os\vm-ready.json"|ConvertFrom-Json
 $disk=@($vm|Get-VMHardDiskDrive)
 if($vm.Name -cne 'Swarm-Validation-Win11' -or $vm.State -ne 'Running' -or $disk.Count -ne 1){throw 'Owned running VM required'}
 $vhd=Get-VHD -Path $disk[0].Path
 if($vhd.Path -ine $proof.disk_chain[0] -or $vhd.ParentPath -ine $proof.disk_chain[1] -or (Get-VHD $vhd.ParentPath).ParentPath){throw 'Unexpected disk chain'}
 if(@($vm|Get-VMNetworkAdapter|Where-Object SwitchName).Count -or @($vm|Get-VMSnapshot|Where-Object{$_.Id.ToString() -ceq $proof.baseline_id}).Count -ne 1){throw 'Isolation/baseline changed'}
 Save-New 'attempt.json' @{at=[DateTimeOffset]::Now.ToString('o');read_only=$true;vm_id=$id.ToString();gui_launch=$false;credential_recorded=$false}
 $credential=Get-Credential -UserName 'DESKTOP-1TAP8BI\GPT' -Message 'Read-only guest GUI launch diagnosis. Do not enter the password in chat.'
 if(!$credential){throw 'Credential cancelled'}
 $connector=[powershell]::Create()
 [void]$connector.AddScript('param($id,$credential) $ErrorActionPreference="Stop"; New-PSSession -VMId $id -Credential $credential -ErrorAction Stop').AddArgument($id).AddArgument($credential)
 $pending=$connector.BeginInvoke()
 if(!$pending.AsyncWaitHandle.WaitOne(60000)){$connector.Stop();throw 'Direct connection deadline'}
 $items=@($connector.EndInvoke($pending))
 if($connector.HadErrors -or $items.Count -ne 1){throw 'Direct connection failed'}
 $session=$items[0]
 $result=Invoke-Command -Session $session -ArgumentList $LockDetails.IsPresent,$StartupSequenceDetails.IsPresent -ScriptBlock {
  param($lockDetails,$startupDetails)
  $ErrorActionPreference='Stop'
  if($env:COMPUTERNAME -cne 'DESKTOP-1TAP8BI' -or [Security.Principal.WindowsIdentity]::GetCurrent().Name -ine 'DESKTOP-1TAP8BI\GPT'){throw 'Wrong guest'}
  $stage='C:\Swarm-Validation\Inputs\startup-20261021'
  if($startupDetails){
   $receipt='C:\Swarm-Validation\Runs\startup-20261021\native-20261023'
   $program="$stage\startup-observer-native-20261023.exe"
   if((Get-FileHash $program).Hash -cne 'EE6AD4A2A7E58C5D38125D6001BC8BB0B9E3B29701772008463DD1813F87055F'){throw 'Observer pin changed'}
   $trace=@{};foreach($file in @(Get-ChildItem -LiteralPath "$receipt\traces" -File -Filter 'native-20261024-trace-sameboot-*.json')){$trace[$file.Name]=Get-Content -Raw -Encoding UTF8 $file.FullName|ConvertFrom-Json}
   $phases=@{};foreach($name in @('b0-running.json','b0-stopped.json','sameboot-attempt.json','sameboot.json','cli-timeout.json')){if(Test-Path -LiteralPath "$receipt\$name"){$phases[$name]=Get-Content -Raw -Encoding UTF8 "$receipt\$name"|ConvertFrom-Json}}
   $metadata=& $program --continue --metadata 2>$null;$code=$LASTEXITCODE
   $value=if($code -eq 0){$metadata|ConvertFrom-Json}else{$null}
   return @{at=[DateTimeOffset]::Now.ToString('o');read_only=$true;trace=$trace;phases=$phases;metadata_exit=$code;metadata=$value;processes=@(Get-CimInstance Win32_Process|Where-Object{$_.ExecutablePath -in @("$stage\swarm-desktop.exe","$stage\swarm-runtime.exe")}|Select-Object Name,ProcessId,SessionId)}
  }
  if($lockDetails){
   $data='C:\Swarm-Validation\Runs\startup-20261021\Data';$sid=[Security.Principal.WindowsIdentity]::GetCurrent().User.Value
   $files=@(Get-ChildItem -LiteralPath $data -Force -File)+@(Get-Item -LiteralPath "$data\gui-state\.host.lock","$data\lifecycle\runtime.lock","$data\lifecycle\startup.lock","$data\lifecycle\launch.lock")
   $acls=@();foreach($file in $files){$acl=Get-Acl -LiteralPath $file.FullName;$owner=$acl.GetOwner([Security.Principal.SecurityIdentifier]).Value;$rules=@($acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier])|ForEach-Object{@{principal=$(if($_.IdentityReference.Value -ceq $sid){'current_user'}elseif($_.IdentityReference.Value -ceq 'S-1-5-32-544'){'administrators'}elseif($_.IdentityReference.Value -ceq 'S-1-5-18'){'system'}else{'other'});rights=$_.FileSystemRights.ToString();inherited=$_.IsInherited;type=$_.AccessControlType.ToString()}});$acls+=@{relative=$file.FullName.Substring($data.Length);length=$file.Length;owner_matches=($owner -ceq $sid);owner_admin=($owner -ceq 'S-1-5-32-544');protected=$acl.AreAccessRulesProtected;rules=$rules}}
   return @{at=[DateTimeOffset]::Now.ToString('o');read_only=$true;file_acls=$acls;processes=@(Get-CimInstance Win32_Process|Where-Object{$_.ExecutablePath -in @("$stage\swarm-desktop.exe","$stage\swarm-runtime.exe")}|Select-Object Name,ProcessId,SessionId)}
  }
  $shell=New-Object -ComObject WScript.Shell
  $link=$shell.CreateShortcut("$stage\Open Swarm.lnk")
  $shortcut=@{target=$link.TargetPath;arguments=$link.Arguments;working_directory=$link.WorkingDirectory;target_exists=(Test-Path -LiteralPath $link.TargetPath)}
  $hashes=@{};foreach($name in @('swarm-desktop.exe','swarm-runtime.exe')){$hashes[$name]=(Get-FileHash -LiteralPath "$stage\$name").Hash}
  $processes=@(Get-CimInstance Win32_Process|Where-Object{$_.ExecutablePath -in @("$stage\swarm-desktop.exe","$stage\swarm-runtime.exe")}|Select-Object Name,ProcessId,SessionId,CreationDate)
  $webview=@();foreach($path in @('HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}','HKCU:\Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}')){
   $item=Get-ItemProperty -LiteralPath $path -ErrorAction SilentlyContinue
   $webview+=@{registry_path=$path;present=($null -ne $item);version=$item.pv}
  }
  $webviewFiles=@();foreach($path in @("${env:ProgramFiles(x86)}\Microsoft\EdgeWebView\Application","$env:LOCALAPPDATA\Microsoft\EdgeWebView\Application")){
   if(Test-Path -LiteralPath $path){$webviewFiles+=@(Get-ChildItem -LiteralPath $path -Directory|Where-Object {Test-Path -LiteralPath (Join-Path $_.FullName 'msedgewebview2.exe')}|Select-Object -ExpandProperty Name)}
  }
  $events=@();$eventQuery='complete'
  try{
   $records=@(Get-WinEvent -FilterHashtable @{LogName='Application';Id=1000,1001;StartTime=[datetime]'2026-10-23T22:40:00'} -MaxEvents 40 -ErrorAction Stop)
   foreach($record in $records){
    [xml]$xml=$record.ToXml();$data=@{};foreach($item in $xml.Event.EventData.Data){if($item.Name){$data[[string]$item.Name]=[string]$item.'#text'}}
    if(@($data.Values|Where-Object {$_ -match '(?i)swarm-(desktop|runtime)\.exe'}).Count){
     $selected=@{};foreach($key in @('AppName','AppVersion','AppPath','ModuleName','ModuleVersion','ModulePath','ExceptionCode','FaultingOffset','P1','P2','P4','P5','P7','P8')){if($data.ContainsKey($key)){$selected[$key]=$data[$key]}}
     $events+=@{time=$record.TimeCreated.ToString('o');id=$record.Id;fields=$selected}
    }
   }
  }catch{if($_.FullyQualifiedErrorId -like 'NoMatchingEventsFound*'){$eventQuery='no_matching_events'}else{$eventQuery='query_failed'}}
  @{at=[DateTimeOffset]::Now.ToString('o');shortcut=$shortcut;hashes=$hashes;processes=$processes;webview_registry=$webview;webview_file_versions=$webviewFiles;swarm_crash_events=$events;event_query=$eventQuery;read_only=$true;gui_launched=$false}
 }
 Save-New 'result.json' $result
}catch{Save-New 'failure.json' @{at=[DateTimeOffset]::Now.ToString('o');error=$_.Exception.Message;read_only=$true};throw}
finally{if($session){Remove-PSSession $session -ErrorAction SilentlyContinue};if($connector){$connector.Dispose()};$credential=$null}
