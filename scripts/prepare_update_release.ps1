#requires -Version 7.0
param(
    [Parameter(Mandatory)][string] $Installer,
    [Parameter(Mandatory)][ValidatePattern('^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$')][string] $Version,
    [Parameter(Mandatory)][string] $NotesFile,
    [Parameter(Mandatory)][string] $SigningKeyPath,
    [Parameter(Mandatory)][string] $OutputDirectory
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$swarmRoot = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent))
$outputRoot = [IO.Path]::GetFullPath($OutputDirectory)
$allowedRoot = [IO.Path]::Combine($swarmRoot, 'target', 'distribution') + [IO.Path]::DirectorySeparatorChar
if (!$outputRoot.StartsWith($allowedRoot, [StringComparison]::OrdinalIgnoreCase) -or (Test-Path -LiteralPath $outputRoot)) {
    throw 'Use a new output directory under target/distribution.'
}
$keyPath = (Resolve-Path -LiteralPath $SigningKeyPath).Path
if ($keyPath.StartsWith($swarmRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Keep the private signing key outside the source repository.'
}
$installerPath = (Resolve-Path -LiteralPath $Installer).Path
$trustedKey = (Get-Content -Raw -Encoding UTF8 (Join-Path $swarmRoot 'apps/desktop/src-tauri/tauri.conf.json') | ConvertFrom-Json).plugins.updater.pubkey
if (!(Test-Path -LiteralPath ($keyPath + '.pub')) -or [IO.File]::ReadAllText($keyPath + '.pub').Trim() -cne $trustedKey) {
    throw 'Signing key does not match the public key trusted by Swarm.'
}
$name = "Swarm_${Version}_x64-setup.exe"
if ([IO.Path]::GetFileName($installerPath) -cne $name) { throw 'Installer filename must match the release version.' }
$notes = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $NotesFile).Path, [Text.UTF8Encoding]::new($false, $true))
if ([Text.Encoding]::UTF8.GetByteCount($notes) -gt 8192) { throw 'Release notes exceed 8192 UTF-8 bytes.' }
$stream = [IO.File]::OpenRead($installerPath)
try { if ($stream.ReadByte() -ne 77 -or $stream.ReadByte() -ne 90) { throw 'Expected a Windows installer.' } }
finally { $stream.Dispose() }
New-Item -ItemType Directory -Path $outputRoot | Out-Null
$outputInstaller = Join-Path $outputRoot $name
Copy-Item -LiteralPath $installerPath -Destination $outputInstaller
$oldPassword = $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD
try {
    if ($null -eq $oldPassword) { $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '' }
    $signerArgs = @('signer', 'sign', '--private-key-path', $keyPath, '--app-version', $Version)
    if ([string]::IsNullOrEmpty($oldPassword)) { $signerArgs += '--password=' }
    $signerArgs += $outputInstaller
    & node (Join-Path $swarmRoot 'apps/desktop/node_modules/@tauri-apps/cli/tauri.js') @signerArgs
    if ($LASTEXITCODE -ne 0) { throw 'Updater artifact signing failed.' }
}
finally { $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = $oldPassword }
$signature = [IO.File]::ReadAllText($outputInstaller + '.sig').Trim()
$manifest = [ordered]@{
    version = $Version
    notes = $notes
    pub_date = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = @{ 'windows-x86_64' = @{
        url = "https://github.com/VolcaneGD/swarm/releases/download/v$Version/$name"
        signature = $signature
    } }
}
$utf8 = [Text.UTF8Encoding]::new($false)
$json = ($manifest | ConvertTo-Json -Depth 6) + "`n"
if ($utf8.GetByteCount($json) -gt 16384) { throw 'Update manifest exceeds 16 KiB.' }
[IO.File]::WriteAllText((Join-Path $outputRoot 'latest.json'), $json, $utf8)
$hash = (Get-FileHash -LiteralPath $outputInstaller -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText((Join-Path $outputRoot 'SHA256SUMS.txt'), "$hash  $name`n", $utf8)
Write-Output "Prepared signed update assets in $outputRoot. Nothing was uploaded or published."
