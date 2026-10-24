Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Invoke-Cargo {
    param([string[]]$CargoArgs)
    & cargo @CargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo check failed: $($CargoArgs -join ' ')" }
}

Push-Location (Split-Path $PSScriptRoot -Parent)
$swarmBuildGuard = $null
try {
    . (Join-Path $PSScriptRoot 'frontend_build_guard.ps1')
    $swarmBuildGuard = Open-SwarmFrontendBuildGuard -Root (Get-Location).Path
    $swarmUtf8 = [System.Text.UTF8Encoding]::new($false, $true)
    $swarmFiles = & rg --files -g '*.md' -g '*.rs' -g '*.toml' -g '*.sql' -g '*.ps1' -g '*.ts' -g '*.tsx' -g '*.css' -g '*.html' -g '*.json' -g 'Cargo.lock'
    if ($LASTEXITCODE -ne 0) { throw 'Source file discovery failed' }
    foreach ($swarmFile in $swarmFiles) {
        $swarmText = $swarmUtf8.GetString([System.IO.File]::ReadAllBytes((Join-Path (Get-Location) $swarmFile)))
        if ($swarmText.Contains([char]65533)) { throw "Replacement character found: $swarmFile" }
    }
    Write-Output "UTF-8 verified: $($swarmFiles.Count) files"
    & npm run build --prefix apps/desktop
    if ($LASTEXITCODE -ne 0) { throw 'Desktop frontend build failed' }
    & npm test --prefix apps/desktop
    if ($LASTEXITCODE -ne 0) { throw 'Desktop frontend tests failed' }
    Invoke-Cargo -CargoArgs @('fmt', '--all', '--', '--check')
    Invoke-Cargo -CargoArgs @('clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings')
    Invoke-Cargo -CargoArgs @('test', '--workspace', '--locked')
    Invoke-Cargo -CargoArgs @('build', '-p', 'swarm-desktop', '--features', 'custom-protocol', '--locked')
}
finally {
    if ($null -ne $swarmBuildGuard) { $swarmBuildGuard.Dispose() }
    Pop-Location
}
