param(
    [ValidateSet('setup', 'dev', 'build', 'package', 'spike', 'check', 'quick', 'native', 'test', 'test:watch', 'prune')]
    [string]$Command = 'dev',
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs
)
$ErrorActionPreference = 'Stop'
$repoPath = Split-Path -Parent $PSScriptRoot
Push-Location $repoPath
try {
    $cacheFile = Join-Path $repoPath 'apps\desktop\node_modules\.cache\pinned-node.txt'
    $nodeExe = $null

    if (Test-Path $cacheFile) {
        $candidate = (Get-Content $cacheFile -Raw).Trim()
        if ((Test-Path $candidate) -and (& $candidate -v 2>$null) -eq 'v24.20.0') {
            $nodeExe = $candidate
        }
    }
    if (-not $nodeExe -and (Get-Command node -ErrorAction SilentlyContinue)) {
        if ((& node -v 2>$null) -eq 'v24.20.0') {
            $nodeExe = (Get-Command node).Source
        }
    }
    if (-not $nodeExe) {
        $npxDir = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'npm-cache\_npx'
        if (Test-Path $npxDir) {
            $candidates = Get-ChildItem -Path $npxDir -Filter 'node.exe' -Recurse -Depth 5 -ErrorAction SilentlyContinue
            foreach ($c in $candidates) {
                if ((& $c.FullName -v 2>$null) -eq 'v24.20.0') {
                    $nodeExe = $c.FullName
                    break
                }
            }
        }
    }

    if ($nodeExe) {
        $cacheDir = Split-Path $cacheFile
        if (-not (Test-Path $cacheDir)) { New-Item -ItemType Directory -Path $cacheDir -Force | Out-Null }
        if (-not (Test-Path $cacheFile) -or (Get-Content $cacheFile -Raw).Trim() -ne $nodeExe) {
            Set-Content -Path $cacheFile -Value $nodeExe -NoNewline
        }

        if (-not $env:npm_execpath) {
            $npmCmd = Get-Command npm.cmd -ErrorAction SilentlyContinue
            if ($npmCmd) {
                $cliPath = Join-Path (Split-Path -Parent $npmCmd.Source) 'node_modules\npm\bin\npm-cli.js'
                if (Test-Path $cliPath) {
                    $env:npm_execpath = $cliPath
                }
            }
        }

        & $nodeExe (Join-Path $PSScriptRoot 'run-desktop.mjs') $Command @RemainingArgs
    } else {
        # npm manages this exact Node version in its normal package cache when not pre-resolved.
        & npm.cmd exec --yes --package=node@24.20.0 -- node (Join-Path $PSScriptRoot 'run-desktop.mjs') $Command @RemainingArgs
    }
    if ($LASTEXITCODE -ne 0) { throw "Desktop command '$Command' failed ($LASTEXITCODE)." }
} finally {
    Pop-Location
}
