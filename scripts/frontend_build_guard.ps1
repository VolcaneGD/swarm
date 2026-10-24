function Open-SwarmFrontendBuildGuard {
    param([Parameter(Mandatory)][string]$Root)
    $target = Join-Path $Root 'target'
    [System.IO.Directory]::CreateDirectory($target) | Out-Null
    $lockPath = Join-Path $target 'frontend-build.lock'
    try {
        return [System.IO.File]::Open($lockPath, [System.IO.FileMode]::OpenOrCreate, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
    }
    catch [System.IO.IOException] {
        throw 'Another frontend build/check owns dist. Wait for it to finish before building a candidate; do not run these pipelines concurrently.'
    }
}
