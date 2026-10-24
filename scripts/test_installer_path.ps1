$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'installer_path.ps1') -Action PlanOnly

function Assert-Equal {
    param([string] $Name, $Expected, $Actual)
    if ($Expected -cne $Actual) {
        throw "$Name`: expected <$Expected>, got <$Actual>"
    }
}

function Assert-True {
    param([string] $Name, [bool] $Value)
    if (-not $Value) { throw "$Name`: expected true" }
}

$install = 'C:\Users\Test User\AppData\Local\Programs\Swarm'

$empty = Get-SwarmPathPlan -Mode Add -CurrentPath '' -InstallDirectory $install
Assert-Equal 'empty PATH add' $install $empty.Path
Assert-True 'new entry ownership' $empty.OwnsEntry

$existing = 'C:\Tools;D:\Other;'
$append = Get-SwarmPathPlan -Mode Add -CurrentPath $existing -InstallDirectory $install
Assert-Equal 'unrelated PATH preservation' ($existing + ';' + $install) $append.Path

$preexisting = 'C:\Tools;' + $install.ToUpperInvariant() + '\;D:\Other'
$keepPreexisting = Get-SwarmPathPlan -Mode Add -CurrentPath $preexisting -InstallDirectory $install
Assert-Equal 'preexisting equivalent path' $preexisting $keepPreexisting.Path
Assert-True 'preexisting path is not owned' (-not $keepPreexisting.OwnsEntry)

$ownedAlready = Get-SwarmPathPlan -Mode Add -CurrentPath ("C:\Tools;$install") -InstallDirectory $install -OwnedEntry $install
Assert-Equal 'idempotent update' ("C:\Tools;$install") $ownedAlready.Path
Assert-True 'idempotent ownership retained' $ownedAlready.OwnsEntry
Assert-True 'idempotent update does not write' (-not $ownedAlready.Changed)

$notOwned = Get-SwarmPathPlan -Mode Remove -CurrentPath ("C:\Tools;$install;D:\Other") -InstallDirectory $install
Assert-Equal 'preexisting entry retained on uninstall' ("C:\Tools;$install;D:\Other") $notOwned.Path

$owned = Get-SwarmPathPlan -Mode Remove -CurrentPath ("C:\Tools;$install;D:\Other") -InstallDirectory $install -OwnedEntry $install
Assert-Equal 'owned entry removal' 'C:\Tools;D:\Other' $owned.Path
Assert-True 'owned entry removed' $owned.Changed

$duplicateOwned = Get-SwarmPathPlan -Mode Remove -CurrentPath ("$install;C:\Tools;$install") -InstallDirectory $install -OwnedEntry $install
Assert-Equal 'remove one owned duplicate only' ("$install;C:\Tools") $duplicateOwned.Path

$mismatchedOwnership = Get-SwarmPathPlan -Mode Remove -CurrentPath ("C:\Tools;$install") -InstallDirectory $install -OwnedEntry 'D:\Other'
Assert-Equal 'unrelated ownership marker cannot remove target' ("C:\Tools;$install") $mismatchedOwnership.Path

$longPath = ('Q' * 40000) + ';C:\Tools'
$longPlan = Get-SwarmPathPlan -Mode Add -CurrentPath $longPath -InstallDirectory $install
Assert-Equal 'PATH beyond NSIS string limit is preserved' ($longPath + ';' + $install) $longPlan.Path

$semicolonRejected = $false
try { Get-SwarmPathPlan -Mode Add -CurrentPath '' -InstallDirectory 'C:\Swarm;Other' | Out-Null }
catch { $semicolonRejected = $true }
Assert-True 'unrepresentable install directory rejected' $semicolonRejected

$driveRelativeRejected = $false
try { Get-SwarmPathPlan -Mode Add -CurrentPath '' -InstallDirectory 'C:relative\Swarm' | Out-Null }
catch { $driveRelativeRejected = $true }
Assert-True 'drive-relative install path rejected' $driveRelativeRejected

$unc = Get-SwarmPathPlan -Mode Add -CurrentPath '' -InstallDirectory '\\server\share\Swarm'
Assert-Equal 'absolute UNC install path accepted' '\\server\share\Swarm' $unc.Path

$overLimitPath = ('P' * 32750)
$overLimitPlan = Get-SwarmPathPlan -Mode Add -CurrentPath $overLimitPath -InstallDirectory $install
$overLimitRejected = $false
try { Assert-SwarmWindowsPathLength $overLimitPlan.Path }
catch { $overLimitRejected = $true }
Assert-True 'oversized PATH write rejected by pure guard' $overLimitRejected

$originalRegistryPath = [pscustomobject]@{
    Exists = $true
    Value = 'C:\Tools;D:\Other'
    Kind = [Microsoft.Win32.RegistryValueKind]::String
}
$writtenRegistryPath = [pscustomobject]@{
    Exists = $true
    Value = $originalRegistryPath.Value + ';' + $install
    Kind = $originalRegistryPath.Kind
}
$mockRegistry = @{
    Exists = $writtenRegistryPath.Exists
    Value = $writtenRegistryPath.Value
    Kind = $writtenRegistryPath.Kind
    Broadcasts = 0
}
$mockSaveOwnerFailure = { throw 'mock owner metadata write failure' }
$mockReadPath = { [pscustomobject]@{ Exists = $mockRegistry.Exists; Value = $mockRegistry.Value; Kind = $mockRegistry.Kind } }
$mockRestorePath = {
    param($state)
    $mockRegistry.Exists = $state.Exists
    $mockRegistry.Value = $state.Value
    $mockRegistry.Kind = $state.Kind
}
$mockBroadcast = { $mockRegistry.Broadcasts++ }
$ownerFailureMessage = $null
try {
    Complete-SwarmPathAddTransaction -OriginalPathState $originalRegistryPath -WrittenPathState $writtenRegistryPath -SaveOwner $mockSaveOwnerFailure -ReadCurrentPath $mockReadPath -RestorePath $mockRestorePath -Broadcast $mockBroadcast
} catch { $ownerFailureMessage = $_.Exception.Message }
Assert-Equal 'owner-save failure is reported after rollback' 'The Swarm PATH entry was not registered because ownership metadata could not be saved; the previous PATH value was restored. Owner error: mock owner metadata write failure' $ownerFailureMessage
Assert-True 'rollback restores exact PATH presence' $mockRegistry.Exists
Assert-Equal 'rollback restores exact PATH text' $originalRegistryPath.Value $mockRegistry.Value
Assert-Equal 'rollback restores original registry kind' $originalRegistryPath.Kind $mockRegistry.Kind
Assert-Equal 'successful rollback broadcasts environment change' 1 $mockRegistry.Broadcasts

$missingPathOriginal = [pscustomobject]@{ Exists = $false; Value = ''; Kind = $null }
$missingPathMock = @{ Exists = $true; Value = $writtenRegistryPath.Value; Kind = $writtenRegistryPath.Kind; Broadcasts = 0 }
$missingPathRead = { [pscustomobject]@{ Exists = $missingPathMock.Exists; Value = $missingPathMock.Value; Kind = $missingPathMock.Kind } }
$missingPathRestore = {
    param($state)
    $missingPathMock.Exists = $state.Exists
    $missingPathMock.Value = $state.Value
    $missingPathMock.Kind = $state.Kind
}
$missingPathBroadcast = { $missingPathMock.Broadcasts++ }
try {
    Complete-SwarmPathAddTransaction -OriginalPathState $missingPathOriginal -WrittenPathState $writtenRegistryPath -SaveOwner $mockSaveOwnerFailure -ReadCurrentPath $missingPathRead -RestorePath $missingPathRestore -Broadcast $missingPathBroadcast
} catch { }
Assert-True 'rollback removes a previously absent PATH value' (-not $missingPathMock.Exists)
Assert-Equal 'absent PATH rollback broadcasts environment change' 1 $missingPathMock.Broadcasts

$concurrentPathMock = @{ Exists = $true; Value = $writtenRegistryPath.Value + ';C:\Concurrent'; Kind = $writtenRegistryPath.Kind; Broadcasts = 0; RestoreCalled = $false }
$concurrentPathRead = { [pscustomobject]@{ Exists = $concurrentPathMock.Exists; Value = $concurrentPathMock.Value; Kind = $concurrentPathMock.Kind } }
$concurrentPathRestore = { $concurrentPathMock.RestoreCalled = $true }
$concurrentPathBroadcast = { $concurrentPathMock.Broadcasts++ }
$concurrentFailureMessage = $null
try {
    Complete-SwarmPathAddTransaction -OriginalPathState $originalRegistryPath -WrittenPathState $writtenRegistryPath -SaveOwner $mockSaveOwnerFailure -ReadCurrentPath $concurrentPathRead -RestorePath $concurrentPathRestore -Broadcast $concurrentPathBroadcast
} catch { $concurrentFailureMessage = $_.Exception.Message }
Assert-True 'concurrent PATH change is reported as partial' ($concurrentFailureMessage.Contains('changed after this installer wrote it'))
Assert-Equal 'concurrent PATH change is preserved' ($writtenRegistryPath.Value + ';C:\Concurrent') $concurrentPathMock.Value
Assert-Equal 'concurrent PATH change is not broadcast by rollback' 0 $concurrentPathMock.Broadcasts
Assert-True 'concurrent PATH is not overwritten' (-not $concurrentPathMock.RestoreCalled)

$successfulOwnerMock = @{ Saves = 0; Restores = 0; Broadcasts = 0 }
Complete-SwarmPathAddTransaction -OriginalPathState $originalRegistryPath -WrittenPathState $writtenRegistryPath -SaveOwner { $successfulOwnerMock.Saves++ } -ReadCurrentPath $concurrentPathRead -RestorePath { $successfulOwnerMock.Restores++ } -Broadcast { $successfulOwnerMock.Broadcasts++ }
Assert-Equal 'successful owner persistence is called once' 1 $successfulOwnerMock.Saves
Assert-Equal 'successful owner persistence needs no rollback' 0 $successfulOwnerMock.Restores
Assert-Equal 'successful owner persistence needs no rollback broadcast' 0 $successfulOwnerMock.Broadcasts

$failedRestoreMock = @{ Exists = $true; Value = $writtenRegistryPath.Value; Kind = $writtenRegistryPath.Kind; Broadcasts = 0 }
$failedRestoreRead = { [pscustomobject]@{ Exists = $failedRestoreMock.Exists; Value = $failedRestoreMock.Value; Kind = $failedRestoreMock.Kind } }
$failedRestore = { throw 'mock rollback failure' }
$failedRestoreMessage = $null
try {
    Complete-SwarmPathAddTransaction -OriginalPathState $originalRegistryPath -WrittenPathState $writtenRegistryPath -SaveOwner $mockSaveOwnerFailure -ReadCurrentPath $failedRestoreRead -RestorePath $failedRestore -Broadcast $mockBroadcast
} catch { $failedRestoreMessage = $_.Exception.Message }
Assert-True 'rollback failure reports possible unowned PATH entry' ($failedRestoreMessage.Contains('restoring the previous PATH also failed'))

$helperPath = Join-Path $PSScriptRoot 'installer_path.ps1'
$hookPath = Join-Path $PSScriptRoot '..\apps\desktop\src-tauri\installer_hooks.nsh'
$configPath = Join-Path $PSScriptRoot '..\apps\desktop\src-tauri\tauri.distribution.conf.json'
$hook = Get-Content -LiteralPath $hookPath -Raw -Encoding utf8
$config = Get-Content -LiteralPath $configPath -Raw -Encoding utf8 | ConvertFrom-Json
$source = Get-Content -LiteralPath $helperPath -Raw -Encoding utf8

foreach ($macro in @('NSIS_HOOK_PREINSTALL', 'NSIS_HOOK_POSTINSTALL', 'NSIS_HOOK_PREUNINSTALL', 'NSIS_HOOK_POSTUNINSTALL')) {
    Assert-True "hook macro $macro" ($hook.Contains("!macro $macro"))
}
Assert-Equal 'configured hook file' 'installer_hooks.nsh' $config.bundle.windows.nsis.installerHooks
Assert-Equal 'bundled helper destination' 'swarm-installer.ps1' $config.bundle.resources.'../../../scripts/installer_path.ps1'
Assert-True 'registry kind read from existing value' ($source.Contains("GetValueKind('Path')"))
Assert-True 'registry kind preserved on write' ($source.Contains('SetValue(''Path'', $plan.Path, $kind)'))
Assert-True 'unexpanded registry value read' ($source.Contains('DoNotExpandEnvironmentNames'))
Assert-True 'environment broadcast is present' ($source.Contains('SendMessageTimeout'))
Assert-True 'runtime guard wired into install and uninstall' (($hook.Split('!insertmacro SWARM_RUN_GUARD').Length - 1) -ge 2)
Assert-True 'preinstall initializes temporary helper path before guard and restores install output path' ($hook.Contains('InitPluginsDir') -and $hook.Contains('SetOutPath "$PLUGINSDIR"') -and $hook.Contains('!insertmacro SWARM_RUN_GUARD "$PLUGINSDIR\swarm-installer.ps1"') -and $hook.Contains('SetOutPath "$INSTDIR"'))
Assert-True 'preuninstall copy failure uses explicit error branch' ($hook -match 'ClearErrors\s+CopyFiles' -and $hook.Contains('${If} ${Errors}') -and $hook.Contains('${EndIf}'))
Assert-True 'update path removal is guarded' ($hook.Contains('$UpdateMode <> 1'))
Assert-True 'runtime is never forcibly stopped' (-not $source.Contains('.Kill(') -and -not $source.Contains('Stop-Process'))
Assert-True 'runtime guard covers all installed executables' ($source.Contains("'swarm-runtime.exe'") -and $source.Contains("'swarm-desktop.exe'") -and $source.Contains("'swarm.exe'"))
Assert-True 'PATH length guard precedes registry write' ($source.IndexOf('Assert-SwarmWindowsPathLength $plan.Path') -lt $source.IndexOf('$environment.SetValue(''Path'', $plan.Path, $kind)'))
Assert-True 'default data deletion requires explicit checkbox' ($hook.Contains('$DeleteAppDataCheckboxState = 1'))
Assert-True 'default data deletion is skipped for updates' ($hook.Contains('${AndIf} $UpdateMode <> 1'))
Assert-True 'only the verified default data root is deleted' ($hook.Contains('RMDir /r "$LOCALAPPDATA\Swarm"'))
Assert-True 'default data deletion failure is surfaced' ($hook.Contains('could not remove all data from $LOCALAPPDATA\Swarm'))
Assert-True 'custom data roots are not dynamically deleted' (-not $hook.Contains('RMDir /r "$INSTDIR'))

$tokens = $null
$errors = $null
[System.Management.Automation.Language.Parser]::ParseFile($helperPath, [ref]$tokens, [ref]$errors) | Out-Null
if ($errors.Count -gt 0) { throw "PowerShell parse errors: $($errors.Message -join '; ')" }

Write-Output 'Installer PATH planner and static hook contracts passed; no registry, process, broadcast, or filesystem mutation was performed.'

# Invoke the real action with no installed executables. StrictMode in Windows
# PowerShell 5.1 must not treat an empty pipeline as an object with .Count.
$emptyInstall = Join-Path ([IO.Path]::GetTempPath()) ('swarm-guard-empty-' + [guid]::NewGuid().ToString('N'))
Assert-True 'empty real process guard allows installation' ((Invoke-SwarmInstallerAction -SelectedAction GuardRunning -SelectedInstallDirectory $emptyInstall) -eq 0)
Write-Output 'Real empty-process GuardRunning action passed without mutations.'
