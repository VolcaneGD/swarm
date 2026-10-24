param(
    [ValidateSet('PlanOnly', 'AddPath', 'RemovePath', 'GuardRunning')]
    [string] $Action = 'PlanOnly',
    [string] $InstallDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:SwarmPathOwnersKey = 'Software\Swarm\Installer\PathOwners'
$script:SwarmWindowsPathValueLimit = 32767

function Assert-SwarmAbsoluteWindowsPath {
    param([Parameter(Mandatory)][string] $Path)

    if ($Path -notmatch '^[A-Za-z]:[\\/]' -and $Path -notmatch '^\\\\[^\\/]+[\\/][^\\/]+') {
        throw 'The install directory must be an absolute drive or UNC path.'
    }
}

function Assert-SwarmWindowsPathLength {
    param([Parameter(Mandatory)][string] $PathValue)

    if ($PathValue.Length -gt $script:SwarmWindowsPathValueLimit) {
        throw "The current-user PATH would exceed the Windows $script:SwarmWindowsPathValueLimit-character limit; no PATH changes were made."
    }
}

function ConvertTo-SwarmComparablePath {
    param([AllowNull()][string] $Path)

    if ([string]::IsNullOrEmpty($Path)) { return $null }
    $expanded = [Environment]::ExpandEnvironmentVariables($Path)
    $expanded = $expanded.Trim()
    if ($expanded.Length -ge 2 -and $expanded[0] -eq '"' -and $expanded[$expanded.Length - 1] -eq '"') {
        $expanded = $expanded.Substring(1, $expanded.Length - 2)
    }
    if (-not [IO.Path]::IsPathRooted($expanded)) { return $null }

    try {
        $full = [IO.Path]::GetFullPath($expanded)
    } catch {
        return $null
    }

    $root = [IO.Path]::GetPathRoot($full)
    if ($full.Length -gt $root.Length) { $full = $full.TrimEnd('\', '/') }
    return $full.ToUpperInvariant()
}

function Get-SwarmPathOwnerKeyName {
    param([Parameter(Mandatory)][string] $InstallDirectory)

    $comparable = ConvertTo-SwarmComparablePath $InstallDirectory
    Assert-SwarmAbsoluteWindowsPath $InstallDirectory
    if (-not $comparable) { throw 'The install directory must be an absolute Windows path.' }
    $bytes = [Text.Encoding]::UTF8.GetBytes($comparable)
    $sha256 = [Security.Cryptography.SHA256]::Create()
    try { $hash = $sha256.ComputeHash($bytes) }
    finally { $sha256.Dispose() }
    return ([BitConverter]::ToString($hash)).Replace('-', '')
}

function Get-SwarmPathPlan {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][ValidateSet('Add', 'Remove')][string] $Mode,
        [AllowNull()][AllowEmptyString()][string] $CurrentPath,
        [Parameter(Mandatory)][string] $InstallDirectory,
        [AllowNull()][string] $OwnedEntry
    )

    Assert-SwarmAbsoluteWindowsPath $InstallDirectory
    $target = [IO.Path]::GetFullPath($InstallDirectory)
    if ($target.Contains(';')) { throw 'An install directory containing a semicolon cannot be represented in PATH.' }

    $current = if ($null -eq $CurrentPath) { '' } else { $CurrentPath }
    $entries = [Collections.Generic.List[string]]::new()
    foreach ($entry in $current.Split([char]';')) { $entries.Add($entry) }

    $targetComparable = ConvertTo-SwarmComparablePath $target
    if ($Mode -eq 'Add') {
        $present = $false
        foreach ($entry in $entries) {
            if ((ConvertTo-SwarmComparablePath $entry) -eq $targetComparable) {
                $present = $true
                break
            }
        }

        if ($present) {
            return [pscustomobject]@{
                Path = $current
                Changed = $false
                OwnsEntry = [bool]$OwnedEntry
                OwnedEntry = $OwnedEntry
            }
        }

        $updated = if ($current.Length -eq 0) { $target } else { $current + ';' + $target }
        return [pscustomobject]@{
            Path = $updated
            Changed = $true
            OwnsEntry = $true
            OwnedEntry = $target
        }
    }

    if ([string]::IsNullOrEmpty($OwnedEntry) -or
        (ConvertTo-SwarmComparablePath $OwnedEntry) -ne $targetComparable) {
        return [pscustomobject]@{
            Path = $current
            Changed = $false
            OwnsEntry = $false
            OwnedEntry = $null
        }
    }

    # Remove one exact lexical entry, choosing the last match because Add appends.
    # Other duplicates and all unrelated PATH text remain untouched.
    $removeIndex = -1
    for ($i = $entries.Count - 1; $i -ge 0; $i--) {
        if ([string]::Equals($entries[$i], $OwnedEntry, [StringComparison]::OrdinalIgnoreCase)) {
            $removeIndex = $i
            break
        }
    }
    if ($removeIndex -lt 0) {
        return [pscustomobject]@{
            Path = $current
            Changed = $false
            OwnsEntry = $false
            OwnedEntry = $null
        }
    }

    $entries.RemoveAt($removeIndex)
    return [pscustomobject]@{
        Path = [string]::Join(';', $entries)
        Changed = $true
        OwnsEntry = $false
        OwnedEntry = $null
    }
}

function Get-SwarmPathStateKeyPath {
    param([Parameter(Mandatory)][string] $InstallDirectory)
    return "$script:SwarmPathOwnersKey\$(Get-SwarmPathOwnerKeyName $InstallDirectory)"
}

function Complete-SwarmPathAddTransaction {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][psobject] $OriginalPathState,
        [Parameter(Mandatory)][psobject] $WrittenPathState,
        [Parameter(Mandatory)][scriptblock] $SaveOwner,
        [Parameter(Mandatory)][scriptblock] $ReadCurrentPath,
        [Parameter(Mandatory)][scriptblock] $RestorePath,
        [Parameter(Mandatory)][scriptblock] $Broadcast
    )

    try {
        & $SaveOwner
        return
    } catch {
        $ownerFailure = $_.Exception.Message
    }

    try {
        $currentState = & $ReadCurrentPath
    } catch {
        throw "The Swarm PATH entry may remain added without ownership metadata because PATH could not be re-read after the ownership write failed. Owner error: $ownerFailure Read error: $($_.Exception.Message)"
    }

    $stillOurWrite = $currentState.Exists -and
        $currentState.Kind -eq $WrittenPathState.Kind -and
        [string]::Equals([string]$currentState.Value, [string]$WrittenPathState.Value, [StringComparison]::Ordinal)
    if (-not $stillOurWrite) {
        throw "The Swarm PATH entry may remain added without ownership metadata because PATH changed after this installer wrote it; the concurrent value was preserved. Owner error: $ownerFailure"
    }

    try {
        & $RestorePath $OriginalPathState
    } catch {
        throw "The Swarm PATH entry may remain added without ownership metadata because restoring the previous PATH also failed. Owner error: $ownerFailure Restore error: $($_.Exception.Message)"
    }

    try {
        & $Broadcast
    } catch {
        throw "The previous PATH value was restored after ownership metadata failed, but the environment-change notification failed. Owner error: $ownerFailure Notification error: $($_.Exception.Message)"
    }

    throw "The Swarm PATH entry was not registered because ownership metadata could not be saved; the previous PATH value was restored. Owner error: $ownerFailure"
}

function Invoke-SwarmPathMutation {
    param(
        [Parameter(Mandatory)][ValidateSet('Add', 'Remove')][string] $Mode,
        [Parameter(Mandatory)][string] $InstallDirectory
    )

    Assert-SwarmAbsoluteWindowsPath $InstallDirectory
    $installPath = [IO.Path]::GetFullPath($InstallDirectory)
    if ($installPath.Contains(';')) { throw 'An install directory containing a semicolon cannot be represented in PATH.' }

    $mutexName = 'Local\SwarmInstallerPath-' + [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $mutex = [Threading.Mutex]::new($false, $mutexName)
    $locked = $false
    try {
        try { $locked = $mutex.WaitOne([TimeSpan]::FromSeconds(10)) } catch [Threading.AbandonedMutexException] { $locked = $true }
        if (-not $locked) { throw 'Another Swarm installer PATH operation is still active.' }

        $environment = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
        if ($null -eq $environment) { $environment = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment', $true) }
        if ($null -eq $environment) { throw 'Could not open the current-user Environment registry key.' }
        try {
            $hasPath = $environment.GetValueNames() -contains 'Path'
            $kind = if ($hasPath) { $environment.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
            if ($kind -notin @([Microsoft.Win32.RegistryValueKind]::String, [Microsoft.Win32.RegistryValueKind]::ExpandString)) {
                throw 'The current-user PATH registry value has an unsupported type.'
            }
            $current = if ($hasPath) {
                [string]$environment.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            } else { '' }

            $ownerKeyPath = Get-SwarmPathStateKeyPath $installPath
            $ownerKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($ownerKeyPath, $true)
            $ownedEntry = $null
            if ($null -ne $ownerKey) {
                try { $ownedEntry = [string]$ownerKey.GetValue('PathEntry', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
                finally { $ownerKey.Dispose() }
            }

            $plan = Get-SwarmPathPlan -Mode $Mode -CurrentPath $current -InstallDirectory $installPath -OwnedEntry $ownedEntry
            $originalPathState = [pscustomobject]@{
                Exists = $hasPath
                Value = $current
                Kind = if ($hasPath) { $kind } else { $null }
            }
            if ($plan.Changed) {
                Assert-SwarmWindowsPathLength $plan.Path
                $environment.SetValue('Path', $plan.Path, $kind)
            }

            if ($Mode -eq 'Add' -and $plan.OwnsEntry) {
                $saveOwner = {
                    $ownerKey = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey($ownerKeyPath, $true)
                    if ($null -eq $ownerKey) { throw 'Could not save Swarm PATH ownership state.' }
                    try { $ownerKey.SetValue('PathEntry', $plan.OwnedEntry, [Microsoft.Win32.RegistryValueKind]::String) }
                    finally { $ownerKey.Dispose() }
                }
                if ($plan.Changed) {
                    $writtenPathState = [pscustomobject]@{
                        Exists = $true
                        Value = $plan.Path
                        Kind = $kind
                    }
                    Complete-SwarmPathAddTransaction -OriginalPathState $originalPathState -WrittenPathState $writtenPathState -SaveOwner $saveOwner -ReadCurrentPath {
                        $latestExists = $environment.GetValueNames() -contains 'Path'
                        $latestKind = if ($latestExists) { $environment.GetValueKind('Path') } else { $null }
                        $latestValue = if ($latestExists) { [string]$environment.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) } else { $null }
                        [pscustomobject]@{ Exists = $latestExists; Value = $latestValue; Kind = $latestKind }
                    } -RestorePath {
                        param($state)
                        if ($state.Exists) {
                            $environment.SetValue('Path', [string]$state.Value, $state.Kind)
                        } else {
                            $environment.DeleteValue('Path', $false)
                        }
                    } -Broadcast { Send-SwarmEnvironmentChange }
                } else {
                    & $saveOwner
                }
            } elseif ($Mode -eq 'Remove' -or ($Mode -eq 'Add' -and -not $plan.OwnsEntry -and $null -ne $ownedEntry)) {
                $ownerKey = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($ownerKeyPath, $true)
                if ($null -ne $ownerKey) {
                    try { $ownerKey.DeleteValue('PathEntry', $false) }
                    finally { $ownerKey.Dispose() }
                    try { [Microsoft.Win32.Registry]::CurrentUser.DeleteSubKey($ownerKeyPath, $false) } catch { }
                }
            }

            if ($plan.Changed) { Send-SwarmEnvironmentChange }
        } finally {
            $environment.Dispose()
        }
    } finally {
        if ($locked) { $mutex.ReleaseMutex() }
        $mutex.Dispose()
    }
}

function Send-SwarmEnvironmentChange {
    if (-not ('SwarmInstaller.NativeMethods' -as [type])) {
        Add-Type -Namespace SwarmInstaller -Name NativeMethods -MemberDefinition @'
            [System.Runtime.InteropServices.DllImport("user32.dll", CharSet = System.Runtime.InteropServices.CharSet.Unicode, SetLastError = true)]
            public static extern System.IntPtr SendMessageTimeout(
                System.IntPtr hWnd, uint Msg, System.UIntPtr wParam, string lParam,
                uint fuFlags, uint uTimeout, out System.UIntPtr lpdwResult);
'@
    }
    $ignored = [UIntPtr]::Zero
    [void][SwarmInstaller.NativeMethods]::SendMessageTimeout(
        [IntPtr]0xffff, 0x001A, [UIntPtr]::Zero, 'Environment', 0x0002, 5000, [ref]$ignored)
}

function Test-SwarmProcessMatchesInstall {
    param([Parameter(Mandatory)][Diagnostics.Process] $Process, [Parameter(Mandatory)][string] $ExpectedPath)

    $expectedName = [IO.Path]::GetFileNameWithoutExtension($ExpectedPath)
    if (-not [string]::Equals($Process.ProcessName, $expectedName, [StringComparison]::OrdinalIgnoreCase)) { return $false }
    try {
        $actualPath = $Process.MainModule.FileName
        return (ConvertTo-SwarmComparablePath $actualPath) -eq (ConvertTo-SwarmComparablePath $ExpectedPath)
    } catch {
        # If a same-named process cannot be inspected, fail closed rather than risk
        # replacing an executable that may belong to this current-user install.
        return $true
    }
}

function Get-SwarmBlockingProcesses {
    param([Parameter(Mandatory)][string] $InstallDirectory)

    $paths = @(
        [IO.Path]::Combine($InstallDirectory, 'swarm-runtime.exe'),
        [IO.Path]::Combine($InstallDirectory, 'swarm-desktop.exe'),
        [IO.Path]::Combine($InstallDirectory, 'swarm.exe')
    )
    $found = [Collections.Generic.List[string]]::new()
    foreach ($path in $paths) {
        foreach ($process in [Diagnostics.Process]::GetProcessesByName([IO.Path]::GetFileNameWithoutExtension($path))) {
            try {
                if (Test-SwarmProcessMatchesInstall -Process $process -ExpectedPath $path) { $found.Add($path) }
            } finally { $process.Dispose() }
        }
    }
    return @($found | Select-Object -Unique)
}

function Invoke-SwarmInstallerAction {
    param([Parameter(Mandatory)][string] $SelectedAction, [AllowNull()][string] $SelectedInstallDirectory)

    switch ($SelectedAction) {
        'AddPath' {
            if ([string]::IsNullOrWhiteSpace($SelectedInstallDirectory)) { throw 'InstallDirectory is required.' }
            Invoke-SwarmPathMutation -Mode Add -InstallDirectory $SelectedInstallDirectory
        }
        'RemovePath' {
            if ([string]::IsNullOrWhiteSpace($SelectedInstallDirectory)) { throw 'InstallDirectory is required.' }
            Invoke-SwarmPathMutation -Mode Remove -InstallDirectory $SelectedInstallDirectory
        }
        'GuardRunning' {
            if ([string]::IsNullOrWhiteSpace($SelectedInstallDirectory)) { throw 'InstallDirectory is required.' }
            if (@(Get-SwarmBlockingProcesses $SelectedInstallDirectory).Count -gt 0) { return 23 }
            return 0
        }
        'PlanOnly' { return 0 }
        default { throw 'Unsupported installer action.' }
    }
    return 0
}

if ($MyInvocation.InvocationName -ne '.') {
    try {
        $result = Invoke-SwarmInstallerAction -SelectedAction $Action -SelectedInstallDirectory $InstallDirectory
        if ($null -eq $result) { exit 0 }
        exit [int]$result
    } catch {
        [Console]::Error.WriteLine($_.Exception.Message)
        exit 1
    }
}
