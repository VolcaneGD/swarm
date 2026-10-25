param(
    [ValidateSet('Capture', 'Apply', 'PlanOnly')][string] $Action = 'PlanOnly',
    [string] $InstallDirectory,
    [string] $StateFile,
    [string] $DesktopShortcut,
    [string] $StartMenuShortcut,
    [string] $FlatStartMenuShortcut
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-SwarmDataRoot {
    param([string] $Root)
    if ([string]::IsNullOrWhiteSpace($Root) -or $Root.Length -gt 8192 -or
        $Root -match '["\x00-\x1f]' -or
        ($Root -notmatch '^[A-Za-z]:[\\/]' -and $Root -notmatch '^\\\\[^\\/]+[\\/][^\\/]+') -or
        -not [IO.Directory]::Exists($Root)) {
        throw 'The existing Swarm Data directory is invalid or unavailable. No Data was changed.'
    }
    return [IO.Path]::GetFullPath($Root)
}

function Read-SwarmLaunchBinding {
    param([string] $Path)
    if (-not [IO.File]::Exists($Path)) { return $null }
    $item = Get-Item -LiteralPath $Path
    if ($item.Length -gt 16384 -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'Invalid Swarm launch settings. No Data was changed.'
    }
    $value = [IO.File]::ReadAllText($Path, [Text.UTF8Encoding]::new($false, $true)) | ConvertFrom-Json
    if ($value.schema_version -ne 1) { throw 'Unsupported Swarm launch settings.' }
    return Assert-SwarmDataRoot $value.data_dir
}

function Get-SwarmShortcutRoot {
    param($Shortcut)
    if ([IO.Path]::GetFileName($Shortcut.TargetPath) -ine 'swarm-desktop.exe') { return $null }
    if ([string]::IsNullOrWhiteSpace($Shortcut.Arguments)) { return $null }
    # Preserve a single explicit Data selection, never arbitrary shortcut commands.
    if ($Shortcut.Arguments -notmatch '^\s*--data-dir\s+(?:"([^"\r\n]+)"|([^"\s]+))\s*$') {
        throw 'Swarm shortcut arguments need repair before installing. No Data was changed.'
    }
    $root = if ($Matches[1]) { $Matches[1] } else { $Matches[2] }
    return Assert-SwarmDataRoot $root
}

function Save-SwarmBindingJson {
    param([string] $Path, $Value)
    $json = ($Value | ConvertTo-Json -Depth 4) + "`n"
    $temporary = $Path + '.' + [guid]::NewGuid().ToString('N') + '.tmp'
    try {
        [IO.File]::WriteAllText($temporary, $json, [Text.UTF8Encoding]::new($false))
        if ([IO.File]::Exists($Path)) {
            # Windows PowerShell5.1 binds a null backup name as an invalid empty
            # path. A private sibling backup keeps replacement atomic there too.
            $backup = $temporary + '.previous'
            [IO.File]::Replace($temporary, $Path, $backup)
            [IO.File]::Delete($backup)
        }
        else { [IO.File]::Move($temporary, $Path) }
    } finally {
        if ([IO.File]::Exists($temporary)) { [IO.File]::Delete($temporary) }
    }
}

function Capture-SwarmLaunchBinding {
    param([string] $Directory, [string] $State, [string[]] $Shortcuts)
    $roots = [Collections.Generic.List[string]]::new()
    # GUI initialization captures before the old uninstaller removes shortcuts.
    # PREINSTALL merges that capture with the final selected installation path.
    if ([IO.File]::Exists($State)) {
        $previous = [IO.File]::ReadAllText($State) | ConvertFrom-Json
        if ($previous.schema_version -ne 1) { throw 'Invalid captured Swarm launch settings.' }
        if ($null -ne $previous.data_dir) { $roots.Add((Assert-SwarmDataRoot $previous.data_dir)) }
    }
    $saved = Read-SwarmLaunchBinding (Join-Path $Directory 'swarm-launch.json')
    if ($saved) { $roots.Add($saved) }
    $shell = New-Object -ComObject WScript.Shell
    foreach ($path in @($Shortcuts | Where-Object { $_ } | Select-Object -Unique)) {
        if (-not [IO.File]::Exists($path)) { continue }
        $root = Get-SwarmShortcutRoot ($shell.CreateShortcut($path))
        if ($root) { $roots.Add($root) }
    }
    $distinct = @($roots | ForEach-Object { $_.TrimEnd('\', '/').ToUpperInvariant() } | Select-Object -Unique)
    if ($distinct.Count -gt 1) {
        throw 'Swarm shortcuts select different Data directories. Resolve the selection before installing; no Data was changed.'
    }
    $selected = if ($roots.Count -gt 0) { $roots[0] } else { $null }
    Save-SwarmBindingJson $State @{ schema_version = 1; data_dir = $selected }
}

function Apply-SwarmLaunchBinding {
    param([string] $Directory, [string] $State, [string[]] $Shortcuts)
    $value = [IO.File]::ReadAllText($State, [Text.UTF8Encoding]::new($false, $true)) | ConvertFrom-Json
    if ($value.schema_version -ne 1) { throw 'Invalid captured Swarm launch settings.' }
    if ($null -eq $value.data_dir) { return }
    $root = Assert-SwarmDataRoot $value.data_dir
    Save-SwarmBindingJson (Join-Path $Directory 'swarm-launch.json') @{ schema_version = 1; data_dir = $root }
    $shell = New-Object -ComObject WScript.Shell
    foreach ($path in @($Shortcuts | Where-Object { $_ } | Select-Object -Unique)) {
        if (-not [IO.File]::Exists($path)) { continue }
        $shortcut = $shell.CreateShortcut($path)
        if ([IO.Path]::GetFileName($shortcut.TargetPath) -ine 'swarm-desktop.exe') { continue }
        $shortcut.TargetPath = Join-Path $Directory 'swarm-desktop.exe'
        $shortcut.WorkingDirectory = $Directory
        $shortcut.Arguments = '--data-dir "' + $root + '"'
        $shortcut.Save()
    }
}

if ($MyInvocation.InvocationName -ne '.') {
    try {
        $paths = @($DesktopShortcut, $StartMenuShortcut, $FlatStartMenuShortcut)
        switch ($Action) {
            Capture { Capture-SwarmLaunchBinding $InstallDirectory $StateFile $paths }
            Apply { Apply-SwarmLaunchBinding $InstallDirectory $StateFile $paths }
            PlanOnly { }
        }
        exit 0
    } catch {
        [Console]::Error.WriteLine($_.Exception.Message)
        exit 1
    }
}
