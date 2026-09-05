param(
    [ValidateSet('setup', 'dev', 'build', 'spike', 'check', 'native', 'test')]
    [string]$Command = 'dev'
)
$ErrorActionPreference = 'Stop'
$repoPath = Split-Path -Parent $PSScriptRoot
Push-Location $repoPath
try {
    # npm manages this exact Node version in its normal package cache. Nothing in V2 changes.
    & npm.cmd exec --yes --package=node@24.20.0 -- node (Join-Path $PSScriptRoot 'run-desktop.mjs') $Command
    if ($LASTEXITCODE -ne 0) { throw "Desktop command '$Command' failed ($LASTEXITCODE)." }
} finally {
    Pop-Location
}
