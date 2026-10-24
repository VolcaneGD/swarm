#requires -Version 5.1
param([switch]$AssertOnly)
$ErrorActionPreference='Stop'
$old='C:\Swarm-Validation\Runs\os-20261015'
$identity=[Security.Principal.WindowsIdentity]::GetCurrent()
if($env:COMPUTERNAME -cne 'DESKTOP-1TAP8BI' -or $identity.Name -ine 'DESKTOP-1TAP8BI\GPT'){throw 'Wrong guest identity'}
$baseline=Get-Content -LiteralPath "$old\baseline.json" -Raw -Encoding UTF8|ConvertFrom-Json
$data=Join-Path $env:LOCALAPPDATA 'Swarm'
$items=@(Get-ChildItem -LiteralPath $data -Recurse -Force)
if(@($items|Where-Object{$_.Attributes -band [IO.FileAttributes]::ReparsePoint}).Count){throw 'Original Data links refused'}
$files=@($items|Where-Object{!$_.PSIsContainer})
if($files.Count -ne 22 -or $files.Count -ne @($baseline.data_hashes.PSObject.Properties).Count){throw 'Original Data count changed'}
foreach($file in $files){if((Get-FileHash -LiteralPath $file.FullName).Hash -cne $baseline.data_hashes.($file.FullName.Substring($data.Length))){throw 'Original Data hash changed'}}
if(Test-Path -LiteralPath "$old\Installed"){throw 'Prior Installed unexpectedly exists'}
$pathKey=Get-Item -LiteralPath HKCU:\Environment
$pathExists=@($pathKey.GetValueNames()) -ccontains 'Path'
if($pathExists -ne $baseline.path.exists){throw 'Original PATH presence changed'}
if($pathExists -and ($pathKey.GetValueKind('Path').ToString() -cne $baseline.path.kind -or $pathKey.GetValue('Path',$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) -cne $baseline.path.value)){throw 'Original PATH value/type changed'}
@{old_data_files=$files.Count;old_data_hashes_match=$true;old_installed_absent=$true;path_unchanged=$true;identity_verified=$true}
