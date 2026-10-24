#requires -Version 7.0
param([switch]$SkipInstaller)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# This builds local unsigned candidates. It does not install, register or publish.
$swarmRoot = Split-Path $PSScriptRoot -Parent
$swarmOldRustFlags = $env:RUSTFLAGS
if ($env:RUSTFLAGS -or $env:CARGO_ENCODED_RUSTFLAGS) {
    throw 'Use a clean build environment without custom Rust flags for this candidate.'
}
if (-not $IsWindows) { throw 'Windows x64 is required.' }
if ([System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture -ne 'X64') {
    throw 'Use a Windows x64 shell/toolchain for this candidate.'
}
$swarmFree = (Get-PSDrive -Name ([System.IO.Path]::GetPathRoot($swarmRoot).TrimEnd('\').TrimEnd(':'))).Free
if ($swarmFree -lt 8GB) { throw 'At least 8 GiB workspace-drive free space is required.' }
Push-Location $swarmRoot
$swarmBuildGuard = $null
try {
    . (Join-Path $PSScriptRoot 'frontend_build_guard.ps1')
    $swarmBuildGuard = Open-SwarmFrontendBuildGuard -Root $swarmRoot
    & python -X utf8 scripts/verify_third_party_resources.py
    if ($LASTEXITCODE -ne 0) { throw 'Third-party resources failed exact verification; no bundling started.' }
    & python -X utf8 scripts/package_windows_candidate.py --check-documents
    if ($LASTEXITCODE -ne 0) { throw 'Candidate document link preflight failed; no build or bundling started.' }
    & npm run build --prefix apps/desktop
    if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed.' }
    # All three binaries use the same static CRT flag; no VC redistributable is
    # silently assumed from this development machine for the CLI or runtime.
    $env:RUSTFLAGS = '-C target-feature=+crt-static'
    & cargo build --release --locked --offline -p swarm-cli -p swarm-runtime -p swarm-desktop --features swarm-desktop/custom-protocol
    if ($LASTEXITCODE -ne 0) { throw 'Matching release builds failed.' }
    if (-not $SkipInstaller) {
        Push-Location (Join-Path $swarmRoot 'apps/desktop')
        try {
            & ./node_modules/.bin/tauri.cmd bundle --config src-tauri/tauri.distribution.conf.json --bundles nsis --features custom-protocol --ci --no-sign --no-binary-patching
            if ($LASTEXITCODE -ne 0) { throw 'NSIS candidate generation failed.' }
        }
        finally { Pop-Location }
    }
}
finally {
    if ($null -ne $swarmBuildGuard) { $swarmBuildGuard.Dispose() }
    $env:RUSTFLAGS = $swarmOldRustFlags
    Pop-Location
}
