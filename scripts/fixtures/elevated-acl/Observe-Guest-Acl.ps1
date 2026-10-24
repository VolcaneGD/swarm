#requires -Version 5.1
#requires -RunAsAdministrator
$ErrorActionPreference='Stop'
Import-Module "$PSHOME\Modules\Microsoft.PowerShell.Security\Microsoft.PowerShell.Security.psd1"
Import-Module "$PSHOME\Modules\Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1"
$root='C:\Swarm-Validation\Runs\acl-20261020'
$old='C:\Swarm-Validation\Runs\os-20261015'
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
if($env:COMPUTERNAME -cne 'DESKTOP-1TAP8BI' -or $identity.Name -ine 'DESKTOP-1TAP8BI\GPT'){throw 'Guest identity mismatch'}
if(Test-Path -LiteralPath $root){throw 'Fixed ACL run already exists; no retry or alternate root'}
if(!(Test-Path -LiteralPath "$PSScriptRoot\acl_probe.exe")){throw 'Pinned adjacent probe missing'}
function Assert-OriginalData {
 $baseline=Get-Content -LiteralPath "$old\baseline.json" -Raw -Encoding UTF8|ConvertFrom-Json
 $data=Join-Path $env:LOCALAPPDATA 'Swarm'
 $items=@(Get-ChildItem -LiteralPath $data -Recurse -Force)
 if(@($items|Where-Object{$_.Attributes -band [IO.FileAttributes]::ReparsePoint}).Count){throw 'Linked original Data refused'}
 $files=@($items|Where-Object{!$_.PSIsContainer})
 if($files.Count -ne @($baseline.data_hashes.PSObject.Properties).Count){throw 'Original Data count differs'}
 foreach($file in $files){if((Get-FileHash -LiteralPath $file.FullName).Hash -cne $baseline.data_hashes.($file.FullName.Substring($data.Length))){throw 'Original Data hash differs'}}
}
Assert-OriginalData
function Save-New($name,$value){
 $bytes=(New-Object Text.UTF8Encoding($false)).GetBytes(($value|ConvertTo-Json -Depth 9))
 $file=[IO.File]::Open((Join-Path $root $name),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
 try{$file.Write($bytes,0,$bytes.Length)}finally{$file.Dispose()}
}
function Wait-Marker($name){
 $deadline=[DateTime]::UtcNow.AddSeconds(35)
 while(!(Test-Path -LiteralPath (Join-Path $root $name))){if([DateTime]::UtcNow -ge $deadline){throw 'Observer marker deadline; preserve state'};Start-Sleep -Milliseconds 50}
}
function Read-PrivateAcl($path){
 $item=Get-Item -LiteralPath $path -Force
 if($item.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Linked fixture refused'}
 $acl=Microsoft.PowerShell.Security\Get-Acl -LiteralPath $path
 $owner=$acl.GetOwner([Security.Principal.SecurityIdentifier]).Value
 $rules=@($acl.GetAccessRules($true,$true,[Security.Principal.SecurityIdentifier]))
 if($owner -cne $identity.User.Value -or !$acl.AreAccessRulesProtected -or $rules.Count -ne 1){throw 'Independent owner/protected DACL/count mismatch'}
 $ace=$rules[0]
 if($ace.IdentityReference.Value -cne $identity.User.Value -or $ace.AccessControlType.ToString() -cne 'Allow' -or $ace.IsInherited -or [int]$ace.FileSystemRights -ne 2032127 -or [int]$ace.InheritanceFlags -ne 0 -or [int]$ace.PropagationFlags -ne 0){throw 'Independent private ACE mismatch'}
 @{relative=$item.FullName.Substring($root.Length);owner_sid=$owner;protected=$acl.AreAccessRulesProtected;ace_count=$rules.Count;ace_sid=$ace.IdentityReference.Value;rights=[int]$ace.FileSystemRights;inherited=$ace.IsInherited;inheritance_flags=[int]$ace.InheritanceFlags;propagation_flags=[int]$ace.PropagationFlags;sddl=$acl.Sddl}
}
New-Item -ItemType Directory -Path $root -ErrorAction Stop|Out-Null
Save-New 'attempt.json' @{at=[DateTimeOffset]::Now.ToString('o');elevated=$true;user_sid=$identity.User.Value;automatic_retries=0;existing_data_mutation=$false}
$first=$null;$second=$null
try {
 . "$old\Invoke-OwnedProcess.ps1"
 $first=Start-OwnedProcess "$PSScriptRoot\acl_probe.exe" @('first')
 Wait-Marker 'first-held'
 $cipher=@(Get-ChildItem -LiteralPath "$root\credentials" -Filter '*.bin' -File)
 $locks=@(Get-ChildItem -LiteralPath "$root\credentials" -Filter '*.lock' -File)
 $backup=@(Get-ChildItem -LiteralPath "$root\backups" -Filter '*.backup' -File)
 if($cipher.Count -ne 1 -or $locks.Count -ne 1 -or $backup.Count -ne 1){throw 'Exact new file matrix missing'}
 $paths=@("$root\credentials","$root\backups",$cipher[0].FullName,$backup[0].FullName,$locks[0].FullName)
 $before=@($paths|ForEach-Object{Read-PrivateAcl $_})
 Save-New 'acl-while-first-held.json' $before
 $second=Start-OwnedProcess "$PSScriptRoot\acl_probe.exe" @('second')
 Wait-Marker 'second-started'
 Start-Sleep -Milliseconds 750
 if((Test-Path -LiteralPath "$root\second-acquired") -or $second.Process.HasExited){throw 'Lock contender did not remain blocked'}
 Save-New 'contention-observed.json' @{second_waiting=$true;first_alive=(!$first.Process.HasExited);observation_ms=750}
 if($first.Process.HasExited){throw 'First holder exited before release'}
 Save-New 'release-first' @{release=$true}
 Wait-Marker 'second-acquired'
 $after=@($paths|ForEach-Object{Read-PrivateAcl $_})
 Save-New 'acl-while-second-held.json' $after
 if(($before|ConvertTo-Json -Depth 8 -Compress) -cne ($after|ConvertTo-Json -Depth 8 -Compress)){throw 'ACL changed after lock reacquisition'}
 Save-New 'release-second' @{release=$true}
 $one=Complete-OwnedProcess -Owned $first -TimeoutMilliseconds 10000
 $two=Complete-OwnedProcess -Owned $second -TimeoutMilliseconds 10000
 foreach($processResult in @($one,$two)){if($processResult.exit_code -ne 0 -or $processResult.stderr.Length -or $processResult.stdout.Trim() -cne 'synthetic store and lock probe complete'){throw 'Probe exit/stream mismatch'}}
 Wait-Marker 'first-released';Wait-Marker 'second-released'
 Assert-OriginalData
 Save-New 'result.json' @{status='PASS';independent_metadata_observations=10;new_ciphertext=$true;new_backup=$true;new_lock=$true;cross_process_contention=$true;lock_reacquired=$true;elevated=$true;processes_exited=$true;at=[DateTimeOffset]::Now.ToString('o');human_runtime_stop='Separate retained product PASS 2026-10-20, not executed by this probe'}
}catch{
 Save-New 'result.json' @{status='FAIL';error=$_.Exception.Message;retry_allowed=$false;state_preserved=$true;at=[DateTimeOffset]::Now.ToString('o')}
 exit 1
}finally{
 # Probe deadlines release only their own handles; never kill a runtime/VM or delete fixtures.
 if($first){$first.Process.Dispose()};if($second){$second.Process.Dispose()}
}
