#requires -Version 5.1
$ErrorActionPreference='Stop'
if($env:COMPUTERNAME -cne 'DESKTOP-1TAP8BI' -or [Security.Principal.WindowsIdentity]::GetCurrent().Name -ine 'DESKTOP-1TAP8BI\GPT'){throw 'Wrong guest'}
$data='C:\Swarm-Validation\Runs\startup-20261021\Data'
if(@(Get-CimInstance Win32_Process|Where-Object{$_.Name -in @('swarm-desktop.exe','swarm-runtime.exe')}).Count){throw 'Writers must be absent'}
foreach($path in @('C:\Swarm-Validation','C:\Swarm-Validation\Runs','C:\Swarm-Validation\Runs\startup-20261021',$data,"$data\lifecycle","$data\lifecycle\launch.lock")){
 $item=Get-Item -LiteralPath $path -Force
 if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Links refused'}
}
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class SwarmLeaseLinks {
 [StructLayout(LayoutKind.Sequential)] public struct Info {
  public uint Attributes; public uint CreationLow; public uint CreationHigh; public uint AccessLow; public uint AccessHigh; public uint WriteLow; public uint WriteHigh;
  public uint Volume; public uint SizeHigh; public uint SizeLow; public uint Links; public uint IndexHigh; public uint IndexLow;
 }
 [DllImport("kernel32.dll",SetLastError=true)] public static extern bool GetFileInformationByHandle(SafeFileHandle h,out Info i);
}
'@
if([Runtime.InteropServices.Marshal]::SizeOf([type][SwarmLeaseLinks+Info]) -ne 52){throw 'Native layout mismatch'}
$rights=[Security.AccessControl.FileSystemRights]::Read -bor [Security.AccessControl.FileSystemRights]::Write -bor [Security.AccessControl.FileSystemRights]::ChangePermissions
$file=[IO.FileStream]::new("$data\lifecycle\launch.lock",[IO.FileMode]::Open,$rights,[IO.FileShare]::None,4096,[IO.FileOptions]::None)
try{
 $info=New-Object SwarmLeaseLinks+Info
 if(![SwarmLeaseLinks]::GetFileInformationByHandle($file.SafeFileHandle,[ref]$info) -or $info.Links -ne 1 -or $file.Length -ne 0){throw 'Single empty lease required'}
 $sid=[Security.Principal.WindowsIdentity]::GetCurrent().User
 $acl=$file.GetAccessControl()
 if($acl.GetOwner([Security.Principal.SecurityIdentifier]).Value -cne $sid.Value){throw 'Retained owner differs'}
 $before=$acl.GetSecurityDescriptorSddlForm([Security.AccessControl.AccessControlSections]::Owner -bor [Security.AccessControl.AccessControlSections]::Access)
 $rules=@($acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]))
 if($acl.AreAccessRulesProtected -and $rules.Count -eq 1 -and $rules[0].IdentityReference.Value -ceq $sid.Value -and $rules[0].FileSystemRights -eq [Security.AccessControl.FileSystemRights]::FullControl){
  $repaired=$false
 }else{
  if($acl.AreAccessRulesProtected -or $rules.Count -ne 3 -or @($rules|Where-Object{$_.AccessControlType -ne 'Allow'}).Count){throw 'Unexpected retained ACL'}
  if(@($rules|Where-Object{$_.IdentityReference.Value -ceq $sid.Value -and $_.FileSystemRights -eq [Security.AccessControl.FileSystemRights]::FullControl}).Count -ne 1 -or @($rules|Where-Object{$_.IdentityReference.Value -ceq 'S-1-5-18' -and $_.FileSystemRights -eq [Security.AccessControl.FileSystemRights]::FullControl}).Count -ne 1 -or @($rules|Where-Object{$_.IdentityReference.Value -match '^S-1-5-5-\d+-\d+$' -and [int]$_.FileSystemRights -eq 1179817}).Count -ne 1){throw 'Expected retained user/system/logon ACL required'}
  $private=New-Object Security.AccessControl.FileSecurity
  $private.SetAccessRuleProtection($true,$false)
  $private.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid,[Security.AccessControl.FileSystemRights]::FullControl,[Security.AccessControl.AccessControlType]::Allow))
  $file.SetAccessControl($private);$repaired=$true
 }
 $after=$file.GetAccessControl()
 $afterRules=@($after.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]))
 if(!$after.AreAccessRulesProtected -or $after.GetOwner([Security.Principal.SecurityIdentifier]).Value -cne $sid.Value -or $afterRules.Count -ne 1 -or $afterRules[0].IdentityReference.Value -cne $sid.Value -or $afterRules[0].IsInherited -or $afterRules[0].FileSystemRights -ne [Security.AccessControl.FileSystemRights]::FullControl -or $file.Length -ne 0){throw 'Private lease readback failed'}
 $result=@{single_empty_file=$true;before_sddl=$before;repaired=$repaired;owner_preserved=$true;protected_single_current_user_ace=$true;content_changed=$false}
}finally{$file.Dispose()}
$shortcutPath='C:\Swarm-Validation\Inputs\startup-fixed-20261024\Open Fixed Swarm.lnk'
if(Test-Path -LiteralPath $shortcutPath){throw 'Preserve existing shortcut'}
$shell=New-Object -ComObject WScript.Shell
$shortcut=$shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath='C:\Swarm-Validation\Inputs\startup-fixed-20261024\swarm-desktop.exe'
$shortcut.Arguments='--data-dir "C:\Swarm-Validation\Runs\startup-20261021\Data"'
$shortcut.WorkingDirectory='C:\Swarm-Validation\Inputs\startup-fixed-20261024'
$shortcut.Save()
$result.fixed_data_shortcut_created=$true
$result|ConvertTo-Json -Compress
