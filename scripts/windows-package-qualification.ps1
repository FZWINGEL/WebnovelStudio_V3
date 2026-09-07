<#
    Hosted Windows package smoke harness.

    This script deliberately requires GitHub Actions and a fresh RUNNER_TEMP
    root. It only exercises the release installer with synthetic data and
    never removes or reuses a host author-data directory.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$InstallerPath,

    [Parameter(Mandatory = $false)]
    [string]$QualificationRoot,

    [Parameter(Mandatory = $false)]
    [string]$OriginalBuildMetadataPath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Resolve-FullPath {
    param([Parameter(Mandatory = $true)][string]$Path)
    return [IO.Path]::GetFullPath($Path)
}

function Assert-ContainedPath {
    param(
        [Parameter(Mandatory = $true)][string]$Candidate,
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $candidateFull = (Resolve-FullPath $Candidate).TrimEnd('\')
    $rootFull = (Resolve-FullPath $Root).TrimEnd('\')
    $prefix = "$rootFull\"
    if (-not $candidateFull.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw [System.InvalidOperationException]::new(("{0} must stay below {1}: {2}" -f $Label, $rootFull, $candidateFull))
    }
    return $candidateFull
}

function Read-ExpectedProductVersion {
    param(
        [Parameter(Mandatory = $true)][string]$WorkspaceRoot
    )
    $packagePath = Join-Path $WorkspaceRoot 'apps/desktop/package.json'
    if (-not (Test-Path -LiteralPath $packagePath -PathType Leaf)) {
        throw [System.IO.FileNotFoundException]::new("The checked-out frontend package manifest was not found: $packagePath")
    }
    try {
        $package = Get-Content -LiteralPath $packagePath -Raw | ConvertFrom-Json
    } catch {
        throw [System.InvalidOperationException]::new("The checked-out frontend package manifest is not valid JSON: $packagePath")
    }
    $version = [string]$package.version
    if ([string]::IsNullOrWhiteSpace($version)) {
        throw [System.InvalidOperationException]::new("The checked-out frontend package manifest has no version: $packagePath")
    }
    return $version.Trim()
}

function Assert-InstalledProductVersion {
    param(
        [Parameter(Mandatory = $true)][string]$ExpectedVersion,
        [Parameter(Mandatory = $true)][string]$InstalledVersion,
        [Parameter(Mandatory = $true)][string]$ExecutablePath
    )
    $acceptedVersions = @($ExpectedVersion)
    if ($acceptedVersions -notcontains $InstalledVersion) {
        throw [System.InvalidOperationException]::new(("Installed executable ProductVersion '{0}' does not exactly match checked-out package version '{1}': {2}" -f $InstalledVersion, $ExpectedVersion, $ExecutablePath))
    }
}

if (-not [String]::Equals($env:GITHUB_ACTIONS, 'true', [StringComparison]::OrdinalIgnoreCase)) {
    throw [System.InvalidOperationException]::new('This tracked qualification harness requires GITHUB_ACTIONS=true.')
}
if ([String]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) {
    throw [System.InvalidOperationException]::new('RUNNER_TEMP is required for the isolated qualification root.')
}
if ([String]::IsNullOrWhiteSpace($env:GITHUB_WORKSPACE)) {
    throw [System.InvalidOperationException]::new('GITHUB_WORKSPACE is required for installer identity checks.')
}

$RunnerTempRoot = Resolve-FullPath $env:RUNNER_TEMP
$WorkspaceRoot = Resolve-FullPath $env:GITHUB_WORKSPACE
$ExpectedProductVersion = Read-ExpectedProductVersion $WorkspaceRoot
$script:originalBuildMetadata = $null
if (-not [String]::IsNullOrWhiteSpace($OriginalBuildMetadataPath)) {
    $OriginalBuildMetadataPath = Assert-ContainedPath $OriginalBuildMetadataPath $RunnerTempRoot 'OriginalBuildMetadataPath'
    if (-not (Test-Path -LiteralPath $OriginalBuildMetadataPath -PathType Leaf)) {
        throw [System.IO.FileNotFoundException]::new("OriginalBuildMetadataPath was not found: $OriginalBuildMetadataPath")
    }
    try {
        $script:originalBuildMetadata = Get-Content -LiteralPath $OriginalBuildMetadataPath -Raw | ConvertFrom-Json
    } catch {
        throw [System.InvalidOperationException]::new("OriginalBuildMetadataPath is not valid JSON: $OriginalBuildMetadataPath")
    }
    if ($null -eq $script:originalBuildMetadata.github -or $null -eq $script:originalBuildMetadata.source -or $null -eq $script:originalBuildMetadata.installer) {
        throw [System.InvalidOperationException]::new('OriginalBuildMetadataPath must contain github, source, and installer records.')
    }
}
if ([String]::IsNullOrWhiteSpace($QualificationRoot)) {
    $QualificationRoot = Join-Path $RunnerTempRoot 'webnovel-package-qualification'
}
$QualificationRoot = Assert-ContainedPath $QualificationRoot $RunnerTempRoot 'QualificationRoot'
$InputRoot = Join-Path $QualificationRoot 'input'
$OutputRoot = Join-Path $QualificationRoot 'output'
$InstallRoot = Join-Path $QualificationRoot 'app'
if (Test-Path -LiteralPath $QualificationRoot) {
    throw [System.InvalidOperationException]::new("QualificationRoot must be fresh and absent: $QualificationRoot")
}
if ([IO.Path]::GetExtension($InstallerPath) -ine '.exe') {
    throw [System.InvalidOperationException]::new("InstallerPath must identify an .exe: $InstallerPath")
}
$InstallerPath = Assert-ContainedPath $InstallerPath $WorkspaceRoot 'InstallerPath'
if (-not (Test-Path -LiteralPath $InstallerPath -PathType Leaf)) {
    throw [System.IO.FileNotFoundException]::new("InstallerPath was not found: $InstallerPath")
}
$ReleaseDataRoot = Join-Path $env:LOCALAPPDATA 'com.webnovelstudio.v3'
if (Test-Path -LiteralPath $ReleaseDataRoot) {
    throw [System.InvalidOperationException]::new("Release data root already exists on the runner; refusing to touch it: $ReleaseDataRoot")
}
New-Item -ItemType Directory -Force -Path $InputRoot, $OutputRoot | Out-Null
$stagedInstallerPath = Join-Path $InputRoot ([IO.Path]::GetFileName($InstallerPath))
Copy-Item -LiteralPath $InstallerPath -Destination $stagedInstallerPath -Force
Set-ItemProperty -LiteralPath $stagedInstallerPath -Name IsReadOnly -Value $true
$installerFile = Get-Item -LiteralPath $stagedInstallerPath
$InstallerName = $installerFile.Name

$startedAt = [DateTime]::UtcNow
$runId = $startedAt.ToString('yyyyMMdd-HHmmss-fff')
$runRoot = Join-Path $OutputRoot ("run-{0}" -f $runId)
New-Item -ItemType Directory -Force -Path $runRoot | Out-Null
$logPath = Join-Path $runRoot 'qualification.log'
$resultPath = Join-Path $runRoot 'result.json'

$script:events = [System.Collections.Generic.List[object]]::new()
$script:blocked = $false
$script:ownedAppProcesses = [System.Collections.Generic.List[object]]::new()
$script:result = [ordered]@{
    schemaVersion = 1
    status = 'running'
    startedAtUtc = $startedAt.ToString('o')
    completedAtUtc = $null
    runId = $runId
    installer = [ordered]@{
        name = $null
        sha256 = $null
        sourcePath = $null
        exitCode = $null
        elapsedSeconds = $null
    }
    install = [ordered]@{
        expectedRoot = $InstallRoot
        expectedVersion = $ExpectedProductVersion
        executable = $null
        productName = $null
        version = $null
    }
    firstLaunch = [ordered]@{
        pid = $null
        uiAutomationAvailable = $false
        libraryVisible = $false
        projectCreated = $false
        documentCreated = $false
        textEntryMethod = $null
        textEntered = $false
        textReadback = $false
        reopened = $false
        screenshot = $null
    }
    sameVersionReinstall = [ordered]@{
        attempted = $false
        installerExitCode = $null
        libraryVisible = $false
        projectRetained = $false
        documentRetained = $false
        textRetained = $false
        defaultUninstallAttempted = $false
        defaultUninstallExitCode = $null
        normalCloseAttempted = $false
        normalCloseSucceeded = $false
        forcedProcessStop = $false
        screenshot = $null
    }
    claims = [ordered]@{
        sameVersionReinstallOnly = $true
        upgradeQualification = $false
    }
    diagnostics = [ordered]@{
        ownedProcessId = $null
        failureScreenshot = $null
        uiAutomationEntries = @()
        alertTexts = @()
    }
    errors = [System.Collections.Generic.List[string]]::new()
    events = $null
}

# Keep the failure path safe when installation or launch fails before $app is
# assigned. The diagnostics below are only collected for this owned process.
$app = $null

function Get-FileSha256 {
    param([Parameter(Mandatory = $true)][string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-WebViewRuntimeInfo {
    $roots = @(
        (Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Microsoft\EdgeWebView\Application'),
        (Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'Microsoft\EdgeWebView\Application')
    )
    $binaries = @()
    foreach ($root in $roots) {
        if (Test-Path -LiteralPath $root -PathType Container) {
            $binaries += @(Get-ChildItem -LiteralPath $root -Filter 'msedgewebview2.exe' -File -Recurse -ErrorAction SilentlyContinue)
        }
    }
    $selected = $binaries | Sort-Object FullName -Descending | Select-Object -First 1
    if ($null -eq $selected) { return $null }
    return [ordered]@{
        path = $selected.FullName
        fileVersion = $selected.VersionInfo.FileVersion
        productVersion = $selected.VersionInfo.ProductVersion
    }
}

function Write-BuildMetadata {
    $gitSha = (& git -C $WorkspaceRoot rev-parse HEAD 2>$null).Trim()
    if ([String]::IsNullOrWhiteSpace($gitSha)) {
        throw [System.InvalidOperationException]::new('Could not resolve the checkout commit with git.')
    }
    $dirty = (& git -C $WorkspaceRoot status --porcelain=v1 --untracked-files=all 2>$null | Out-String).Trim()
    $currentGithub = [ordered]@{
        repository = $env:GITHUB_REPOSITORY
        runId = $env:GITHUB_RUN_ID
        sha = $env:GITHUB_SHA
        ref = $env:GITHUB_REF
    }
    $currentSource = [ordered]@{
        workspace = $WorkspaceRoot
        gitSha = $gitSha
        dirtyStatus = $dirty
        cargoLockSha256 = Get-FileSha256 (Join-Path $WorkspaceRoot 'Cargo.lock')
        packageLockSha256 = Get-FileSha256 (Join-Path $WorkspaceRoot 'apps/desktop/package-lock.json')
        tauriConfigSha256 = Get-FileSha256 (Join-Path $WorkspaceRoot 'apps/desktop/src-tauri/tauri.conf.json')
    }
    $currentInstaller = [ordered]@{
        sourcePath = $InstallerPath
        stagedPath = $stagedInstallerPath
        name = $installerFile.Name
        sha256 = Get-FileSha256 $installerFile.FullName
        productName = $installerFile.VersionInfo.ProductName
        productVersion = $installerFile.VersionInfo.ProductVersion
    }
    $buildGithub = $currentGithub
    $buildSource = $currentSource
    $buildInstaller = $currentInstaller
    if ($null -ne $script:originalBuildMetadata) {
        $buildGithub = $script:originalBuildMetadata.github
        $buildSource = $script:originalBuildMetadata.source
        $buildInstaller = $script:originalBuildMetadata.installer
    }
    $installerBuild = [ordered]@{
        repository = [string]$buildGithub.repository
        runId = [string]$buildGithub.runId
        sha = [string]$buildGithub.sha
        ref = [string]$buildGithub.ref
        gitSha = [string]$buildSource.gitSha
        dirtyStatus = [string]$buildSource.dirtyStatus
        installerName = [string]$buildInstaller.name
        installerSha256 = [string]$buildInstaller.sha256
        productVersion = [string]$buildInstaller.productVersion
        github = $buildGithub
        source = $buildSource
        installer = $buildInstaller
    }
    $qualificationSource = [ordered]@{
        repository = [string]$currentGithub.repository
        runId = [string]$currentGithub.runId
        sha = [string]$currentGithub.sha
        ref = [string]$currentGithub.ref
        gitSha = [string]$currentSource.gitSha
        dirtyStatus = [string]$currentSource.dirtyStatus
        harnessSha256 = Get-FileSha256 (Join-Path $WorkspaceRoot 'scripts/windows-package-qualification.ps1')
        cargoLockSha256 = [string]$currentSource.cargoLockSha256
        packageLockSha256 = [string]$currentSource.packageLockSha256
        tauriConfigSha256 = [string]$currentSource.tauriConfigSha256
    }
    $metadata = [ordered]@{
        schemaVersion = if ($null -ne $script:originalBuildMetadata) { 2 } else { 1 }
        capturedAtUtc = [DateTime]::UtcNow.ToString('o')
        github = $buildGithub
        source = $buildSource
        installer = $buildInstaller
        installerBuild = $installerBuild
        qualificationSource = $qualificationSource
        runner = [ordered]@{
            os = Get-CimInstance Win32_OperatingSystem | Select-Object Caption, Version, BuildNumber, OSArchitecture
            webViewRuntime = Get-WebViewRuntimeInfo
        }
        qualification = [ordered]@{
            root = $QualificationRoot
            installRoot = $InstallRoot
            claims = @('installed-release', 'synthetic-write-reopen', 'same-version-uninstall-reinstall-retention')
            excludes = @('offline-no-runtime', 'true-upgrade', 'live-provider', 'author-data')
        }
    }
    $metadataPath = Join-Path $OutputRoot 'build-metadata.json'
    $metadata | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $metadataPath -Encoding UTF8
}

function Write-Event {
    param(
        [Parameter(Mandatory = $true)][string]$Stage,
        [Parameter(Mandatory = $true)][string]$Message,
        [ValidateSet('info', 'warning', 'error')][string]$Level = 'info'
    )
    $entry = [ordered]@{
        atUtc = [DateTime]::UtcNow.ToString('o')
        stage = $Stage
        level = $Level
        message = $Message
    }
    $script:events.Add($entry)
    Add-Content -LiteralPath $logPath -Value ((ConvertTo-Json -InputObject $entry -Compress -Depth 5)) -Encoding UTF8
}

function Save-Result {
    $script:result.completedAtUtc = [DateTime]::UtcNow.ToString('o')
    $script:result.events = @($script:events)
    $script:result.errors = @($script:result.errors)
    $script:result | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $resultPath -Encoding UTF8
}

function Fail-Blocked {
    param([string]$Message)
    $script:blocked = $true
    throw [System.InvalidOperationException]::new($Message)
}

function Wait-Until {
    param(
        [Parameter(Mandatory = $true)][scriptblock]$Condition,
        [Parameter(Mandatory = $true)][int]$TimeoutSeconds,
        [Parameter(Mandatory = $true)][string]$Description
    )
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSeconds)
    do {
        try {
            $value = & $Condition
            if ($null -ne $value -and $false -ne $value) {
                return $value
            }
        } catch {
            # The UI tree and processes change while the app starts. Retry until the deadline.
        }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    throw [System.TimeoutException]::new("Timed out waiting for $Description ($TimeoutSeconds seconds).")
}

function Resolve-Installer {
    if (-not (Test-Path -LiteralPath $InputRoot -PathType Container)) {
        throw [System.InvalidOperationException]::new("Qualification input folder is unavailable: $InputRoot")
    }
    $candidate = Join-Path $InputRoot $InstallerName
    if ([IO.Path]::GetExtension($candidate) -ine '.exe' -or -not (Test-Path -LiteralPath $candidate -PathType Leaf)) {
        throw [System.IO.FileNotFoundException]::new("Staged installer does not identify an .exe in the qualification input folder: $InstallerName")
    }
    return Get-Item -LiteralPath $candidate
}

function Install-Silently {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Stage,
        [Parameter(Mandatory = $false)][string]$Arguments = '/S'
    )
    Write-Event $Stage ("Starting the bounded installer operation with arguments: {0}" -f $Arguments)
    $watch = [Diagnostics.Stopwatch]::StartNew()
    $startInfo = [Diagnostics.ProcessStartInfo]::new()
    $startInfo.FileName = $Path
    $startInfo.Arguments = $Arguments
    $startInfo.UseShellExecute = $true
    $startInfo.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $startInfo
    if (-not $process.Start()) {
        throw [System.InvalidOperationException]::new("Could not start installer: $Path")
    }
    $installerPid = $process.Id
    $installerStartTime = $null
    try { $installerStartTime = $process.StartTime } catch { }
    $installerRecord = [ordered]@{ pid = $installerPid; path = $Path; startTime = $installerStartTime }
    $deadline = [DateTime]::UtcNow.AddSeconds(120)
    while (-not $process.HasExited -and [DateTime]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 250
    }
    if (-not $process.HasExited) {
        try {
            Stop-VerifiedOwnedProcessTree $installerRecord
        } catch {
            throw [System.TimeoutException]::new(("Installer PID {0} timed out and verified process-tree cleanup failed: {1}" -f $installerPid, $_.Exception.Message))
        } finally {
            $process.Dispose()
        }
        throw [System.TimeoutException]::new("Installer PID $installerPid did not exit within 120 seconds.")
    }
    $exitCode = $process.ExitCode
    $watch.Stop()
    $process.Dispose()
    if ($Stage -eq 'install') {
        $script:result.installer.exitCode = $exitCode
        $script:result.installer.elapsedSeconds = [Math]::Round($watch.Elapsed.TotalSeconds, 3)
    } elseif ($Stage -eq 'default-uninstall') {
        $script:result.sameVersionReinstall.defaultUninstallExitCode = $exitCode
    } else {
        $script:result.sameVersionReinstall.installerExitCode = $exitCode
    }
    Write-Event $Stage ("Installer PID {0} exited with code {1} after {2:N1}s." -f $installerPid, $exitCode, $watch.Elapsed.TotalSeconds)
    if ($exitCode -ne 0) {
        throw [System.InvalidOperationException]::new("Installer returned non-zero exit code $exitCode.")
    }
    return $exitCode
}

function Find-InstalledExecutable {
    $executablePath = Join-Path $InstallRoot 'webnovel-desktop.exe'
    $script:result.install.expectedRoot = $InstallRoot
    if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) {
        throw [System.IO.FileNotFoundException]::new("Installer exited successfully but expected executable was not found: $executablePath")
    }
    $selected = Get-Item -LiteralPath $executablePath
    $info = $selected.VersionInfo
    if ($info.ProductName -ne 'WebnovelStudio V3') {
        throw [System.InvalidOperationException]::new(("Unexpected installed executable ProductName '{0}' at {1}; expected WebnovelStudio V3." -f $info.ProductName, $selected.FullName))
    }
    $script:result.install.productName = $info.ProductName
    $script:result.install.version = $info.ProductVersion
    Assert-InstalledProductVersion $ExpectedProductVersion $info.ProductVersion $selected.FullName
    $script:result.install.executable = $selected.FullName
    Write-Event 'install' ("Found exact installed executable {0} (product {1}, version {2})." -f $selected.FullName, $info.ProductName, $info.ProductVersion)
    return $selected
}

function Add-UiAutomationTypes {
    try {
        Add-Type -AssemblyName UIAutomationClient
        Add-Type -AssemblyName UIAutomationTypes
        Add-Type -AssemblyName System.Drawing
        Add-Type -AssemblyName System.Windows.Forms
        $typeDefinition = @"
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
public static class QualificationWindowCapture {
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdcBlt, uint nFlags);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
}
"@
        Add-Type -TypeDefinition $typeDefinition -ReferencedAssemblies 'System.Drawing.dll'
        return $true
    } catch {
        Write-Event 'uia' ("UIAutomation assemblies are unavailable: {0}" -f $_.Exception.Message) 'warning'
        return $false
    }
}

function Find-AppWindow {
    param([Parameter(Mandatory = $true)][int]$ProcessId)
    $root = [System.Windows.Automation.AutomationElement]::RootElement
    $pidCondition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $ProcessId)
    $windowCondition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty, [System.Windows.Automation.ControlType]::Window)
    $condition = [System.Windows.Automation.AndCondition]::new($pidCondition, $windowCondition)
    $windows = $root.FindAll([System.Windows.Automation.TreeScope]::Children, $condition)
    if ($windows.Count -gt 0) { return $windows[0] }
    # Some WebView2/Tauri builds do not expose the top-level window as a direct child.
    $all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
    if ($all.Count -gt 0) { return $all[0] }
    return $null
}

function Find-UiaByName {
    param(
        [Parameter(Mandatory = $true)]$Root,
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $false)]$ControlType
    )
    $nameCondition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, $Name)
    $condition = if ($null -eq $ControlType) {
        $nameCondition
    } else {
        $typeCondition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty, $ControlType)
        [System.Windows.Automation.AndCondition]::new($nameCondition, $typeCondition)
    }
    return $Root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
}

function Find-UiaByNameContains {
    param(
        [Parameter(Mandatory = $true)]$Root,
        [Parameter(Mandatory = $true)][string]$Needle,
        [Parameter(Mandatory = $false)]$ControlType
    )
    $condition = if ($null -eq $ControlType) {
        [System.Windows.Automation.Condition]::TrueCondition
    } else {
        [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ControlTypeProperty, $ControlType)
    }
    $elements = $Root.FindAll([System.Windows.Automation.TreeScope]::Descendants, $condition)
    foreach ($element in $elements) {
        try {
            if ($element.Current.Name -like "*$Needle*") { return $element }
        } catch { }
    }
    return $null
}

function Find-ProjectOpener {
    param(
        [Parameter(Mandatory = $true)]$Root,
        [Parameter(Mandatory = $true)][string]$Title
    )
    $elements = $Root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    # Keep this separate from PowerShell's automatic $Matches variable, which
    # the -match check below populates with the matched text.
    $projectOpeners = [System.Collections.Generic.List[object]]::new()
    foreach ($element in $elements) {
        try {
            $name = [string]$element.Current.Name
            if ($name.StartsWith($Title, [StringComparison]::OrdinalIgnoreCase) -and $name -match '\bLast opened\b' -and $element.Current.IsEnabled) {
                $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern) | Out-Null
                [void]$projectOpeners.Add($element)
            }
        } catch { }
    }
    if ($projectOpeners.Count -eq 1) { return $projectOpeners[0] }
    return $null
}

function Get-ProjectOpenerSnapshot {
    param(
        [Parameter(Mandatory = $true)]$Root,
        [Parameter(Mandatory = $true)][string]$Title
    )
    $elements = $Root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
    $records = [System.Collections.Generic.List[string]]::new()
    foreach ($element in $elements) {
        try {
            $name = [string]$element.Current.Name
            if ($name.IndexOf($Title, [StringComparison]::OrdinalIgnoreCase) -lt 0) { continue }
            $displayName = if ($name.Length -gt 160) { $name.Substring(0, 160) } else { $name }
            $controlType = [string]$element.Current.ControlType.ProgrammaticName
            $enabled = [bool]$element.Current.IsEnabled
            $invokable = $false
            try { $element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern) | Out-Null; $invokable = $true } catch { }
            [void]$records.Add(("name='{0}', type='{1}', enabled={2}, invoke={3}" -f $displayName, $controlType, $enabled, $invokable))
            if ($records.Count -ge 12) { break }
        } catch { }
    }
    if ($records.Count -eq 0) { return 'no UIA elements contained the expected project title' }
    return ($records -join ' | ')
}

function Assert-NoEditorTrial {
    param([Parameter(Mandatory = $true)]$Window)
    $trial = Find-UiaByName $Window 'Open editor trial'
    if ($null -ne $trial) {
        throw [System.InvalidOperationException]::new('The release UI exposed the debug-only Open editor trial action.')
    }
    Write-Event 'release-gate' 'Confirmed the release UI does not expose the debug-only Open editor trial action.'
}

function Invoke-Uia {
    param([Parameter(Mandatory = $true)]$Element)
    $pattern = $Element.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern)
    $pattern.Invoke()
}

function Set-UiaValue {
    param(
        [Parameter(Mandatory = $true)]$Element,
        [Parameter(Mandatory = $true)][string]$Value
    )
    try {
        $pattern = $Element.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
        $pattern.SetValue($Value)
        return 'valuePattern'
    } catch {
        return $null
    }
}

function Set-UiaText {
    param(
        [Parameter(Mandatory = $true)]$Element,
        [Parameter(Mandatory = $true)][string]$Value
    )
    $method = Set-UiaValue $Element $Value
    if ($null -ne $method) { return $method }
    try {
        $Element.SetFocus()
        $focused = [System.Windows.Automation.AutomationElement]::FocusedElement
        if ($null -eq $focused) { return $null }
        $expectedRuntimeId = @($Element.GetRuntimeId()) -join ','
        $focusedRuntimeId = @($focused.GetRuntimeId()) -join ','
        if ([string]::IsNullOrWhiteSpace($expectedRuntimeId) -or $expectedRuntimeId -ne $focusedRuntimeId -or $focused.Current.ProcessId -ne $Element.Current.ProcessId) {
            return $null
        }
        $foreground = [QualificationWindowCapture]::GetForegroundWindow()
        [uint32]$foregroundPid = 0
        if ($foreground -eq [IntPtr]::Zero -or [QualificationWindowCapture]::GetWindowThreadProcessId($foreground, [ref]$foregroundPid) -eq 0 -or $foregroundPid -ne [uint32]$Element.Current.ProcessId) {
            return $null
        }
        $ownedRecord = @($script:ownedAppProcesses | Where-Object { [int]$_.pid -eq [int]$foregroundPid } | Select-Object -First 1)
        if ($ownedRecord.Count -ne 1) { return $null }
        $foregroundProcess = Get-Process -Id ([int]$foregroundPid) -ErrorAction SilentlyContinue
        $samePath = $false
        $sameStart = $false
        try { $samePath = [String]::Equals($foregroundProcess.Path, [string]$ownedRecord[0].path, [StringComparison]::OrdinalIgnoreCase) } catch { }
        try { $sameStart = $null -ne $foregroundProcess -and $foregroundProcess.StartTime -eq $ownedRecord[0].startTime } catch { }
        if (-not ($samePath -and $sameStart)) { return $null }
        [System.Windows.Forms.SendKeys]::SendWait($Value)
        return 'uiaFocus+sendKeys'
    } catch {
        return $null
    }
}

function Get-UiaText {
    param([Parameter(Mandatory = $true)]$Element)
    try {
        $pattern = $Element.GetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern)
        return $pattern.DocumentRange.GetText(-1)
    } catch { }
    try {
        $pattern = $Element.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
        return $pattern.Current.Value
    } catch { }
    return $null
}

function Get-UiaSelectedValue {
    param([Parameter(Mandatory = $true)]$Element)
    try {
        $pattern = $Element.GetCurrentPattern([System.Windows.Automation.SelectionPattern]::Pattern)
        foreach ($selected in @($pattern.Current.GetSelection())) {
            try {
                $name = [string]$selected.Current.Name
                if (-not [string]::IsNullOrWhiteSpace($name)) { return $name }
            } catch { }
        }
    } catch { }
    try {
        $pattern = $Element.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern)
        $value = [string]$pattern.Current.Value
        if (-not [string]::IsNullOrWhiteSpace($value)) { return $value }
    } catch { }
    return $null
}

function Select-Chapter {
    param([Parameter(Mandatory = $true)]$Window)
    $startWith = Wait-Until { Find-UiaByName $Window 'Start with' ([System.Windows.Automation.ControlType]::ComboBox) } 15 'the Start with document-kind selector'
    $selected = Get-UiaSelectedValue $startWith
    if ([string]::Equals($selected, 'Chapter', [StringComparison]::OrdinalIgnoreCase)) {
        Write-Event 'document' 'The Start with selector already has Chapter selected; no option-list interaction was required.'
        return
    }
    try {
        $expand = $startWith.GetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern)
        $expand.Expand()
    } catch {
        throw [System.InvalidOperationException]::new('The Start with selector did not expose ExpandCollapsePattern.')
    }
    try {
        $chapter = Wait-Until { Find-UiaByName $Window 'Chapter' ([System.Windows.Automation.ControlType]::ListItem) } 10 'the Chapter option'
        $selection = $chapter.GetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern)
        $selection.Select()
        $selected = Get-UiaSelectedValue $startWith
        if (-not [string]::Equals($selected, 'Chapter', [StringComparison]::OrdinalIgnoreCase)) {
            throw [System.InvalidOperationException]::new(("The Start with selector did not confirm Chapter after selection; current value was '{0}'." -f $selected))
        }
        Write-Event 'document' 'Selected and confirmed Chapter in the Start with selector.'
    } finally {
        try { $expand.Collapse() } catch { }
    }
}

function Start-QualifiedApp {
    param([Parameter(Mandatory = $true)][string]$Path)
    $process = Start-Process -FilePath $Path -WindowStyle Hidden -PassThru
    $started = $null
    try { $started = $process.StartTime } catch { }
    $script:ownedAppProcesses.Add([ordered]@{ pid = $process.Id; path = $Path; startTime = $started })
    return $process
}

function Convert-ProcessCreationTime {
    param([Parameter(Mandatory = $false)]$Value)
    if ($null -eq $Value) { return $null }
    if ($Value -is [DateTime]) { return [DateTime]$Value }
    try { return [System.Management.ManagementDateTimeConverter]::ToDateTime([string]$Value) } catch { return $null }
}

function Test-ProcessStartTime {
    param(
        [Parameter(Mandatory = $false)]$Actual,
        [Parameter(Mandatory = $false)]$Expected
    )
    if ($null -eq $Actual -or $null -eq $Expected) { return $false }
    try { return [Math]::Abs(([DateTime]$Actual - [DateTime]$Expected).TotalSeconds) -le 2 } catch { return $false }
}

function Get-OwnedProcessTree {
    param([Parameter(Mandatory = $true)][int]$RootPid)
    $processes = @(Get-CimInstance Win32_Process -ErrorAction Stop)
    $ownedIds = [System.Collections.Generic.HashSet[int]]::new()
    [void]$ownedIds.Add($RootPid)
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($process in $processes) {
            $candidatePid = [int]$process.ProcessId
            if ($ownedIds.Contains([int]$process.ParentProcessId) -and $ownedIds.Add($candidatePid)) {
                $changed = $true
            }
        }
    }
    return @($processes | Where-Object { $ownedIds.Contains([int]$_.ProcessId) })
}

function Stop-VerifiedOwnedProcessTree {
    param([Parameter(Mandatory = $true)]$Record)
    $ownedPid = [int]$Record.pid
    $live = Get-Process -Id $ownedPid -ErrorAction SilentlyContinue
    if ($null -eq $live) { return }
    $samePath = $false
    $sameStart = $false
    try { $samePath = [String]::Equals($live.Path, [string]$Record.path, [StringComparison]::OrdinalIgnoreCase) } catch { }
    try { $sameStart = $null -ne $Record.startTime -and $live.StartTime -eq $Record.startTime } catch { }
    if (-not ($samePath -and $sameStart)) {
        throw [System.InvalidOperationException]::new("Refusing to stop PID $ownedPid because it no longer matches the recorded app path/start time.")
    }

    $tree = @(Get-OwnedProcessTree $ownedPid)
    $verifiedDescendants = [System.Collections.Generic.List[object]]::new()
    $unverifiedDescendants = [System.Collections.Generic.List[int]]::new()
    foreach ($process in ($tree | Where-Object { [int]$_.ProcessId -ne $ownedPid } | Sort-Object ProcessId -Descending)) {
        $descendantPath = [string]$process.ExecutablePath
        $descendantStart = Convert-ProcessCreationTime $process.CreationDate
        if ([string]::IsNullOrWhiteSpace($descendantPath) -or $null -eq $descendantStart) {
            [void]$unverifiedDescendants.Add([int]$process.ProcessId)
            continue
        }
        $descendantLive = Get-Process -Id ([int]$process.ProcessId) -ErrorAction SilentlyContinue
        if ($null -eq $descendantLive) { continue }
        $samePath = $false
        $sameStart = $false
        try { $samePath = [String]::Equals($descendantLive.Path, $descendantPath, [StringComparison]::OrdinalIgnoreCase) } catch { }
        try { $sameStart = $null -ne $descendantLive -and (Test-ProcessStartTime $descendantLive.StartTime $descendantStart) } catch { }
        if (-not ($samePath -and $sameStart)) {
            [void]$unverifiedDescendants.Add([int]$process.ProcessId)
            continue
        }
        [void]$verifiedDescendants.Add([ordered]@{ pid = [int]$process.ProcessId; path = $descendantPath; startTime = $descendantStart })
    }
    foreach ($recordedDescendant in $verifiedDescendants) {
        $descendantLive = Get-Process -Id ([int]$recordedDescendant.pid) -ErrorAction SilentlyContinue
        $samePath = $false
        $sameStart = $false
        try { $samePath = [String]::Equals($descendantLive.Path, [string]$recordedDescendant.path, [StringComparison]::OrdinalIgnoreCase) } catch { }
        try { $sameStart = $null -ne $descendantLive -and (Test-ProcessStartTime $descendantLive.StartTime $recordedDescendant.startTime) } catch { }
        if ($samePath -and $sameStart) {
            Stop-Process -Id ([int]$recordedDescendant.pid) -Force -ErrorAction SilentlyContinue
        }
    }
    Stop-Process -Id $ownedPid -Force -ErrorAction SilentlyContinue
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $rootLive = Get-Process -Id $ownedPid -ErrorAction SilentlyContinue
        if ($null -eq $rootLive) { break }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($null -ne (Get-Process -Id $ownedPid -ErrorAction SilentlyContinue)) {
        throw [System.TimeoutException]::new("Owned process PID $ownedPid did not exit within 20 seconds.")
    }
    $descendantDeadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $remainingDescendants = @($verifiedDescendants | Where-Object { $null -ne (Get-Process -Id ([int]$_.pid) -ErrorAction SilentlyContinue) })
        if ($remainingDescendants.Count -eq 0) { break }
        Start-Sleep -Milliseconds 250
    } while ([DateTime]::UtcNow -lt $descendantDeadline)
    if ($remainingDescendants.Count -gt 0) {
        throw [System.TimeoutException]::new(("Owned descendant process(es) did not exit within 20 seconds: {0}" -f (($remainingDescendants | ForEach-Object pid) -join ', ')))
    }
    if ($unverifiedDescendants.Count -gt 0) {
        throw [System.InvalidOperationException]::new(("Could not verify descendant process identities before cleanup: {0}" -f (($unverifiedDescendants | Sort-Object -Unique) -join ', ')))
    }
}

function Stop-OwnedApp {
    param([Parameter(Mandatory = $true)]$Process)
    $ownedPid = $Process.Id
    $script:result.sameVersionReinstall.forcedProcessStop = $true
    Write-Event 'process' ("Force-stopping owned app PID {0} for restart; this is crash/reopen evidence, not a normal close." -f $ownedPid) 'warning'
    $record = @($script:ownedAppProcesses | Where-Object { [int]$_.pid -eq $ownedPid } | Select-Object -First 1)
    if ($record.Count -ne 1) {
        throw [System.InvalidOperationException]::new("No recorded ownership identity exists for app PID $ownedPid.")
    }
    Stop-VerifiedOwnedProcessTree $record[0]
}

function Close-QualifiedApp {
    param([Parameter(Mandatory = $true)]$Process)
    $ownedPid = $Process.Id
    $script:result.sameVersionReinstall.normalCloseAttempted = $true
    $normalCloseRequested = $false
    try {
        $window = Find-AppWindow $ownedPid
        if ($null -ne $window) {
            try {
                $windowPattern = $window.GetCurrentPattern([System.Windows.Automation.WindowPattern]::Pattern)
                $windowPattern.Close()
                $normalCloseRequested = $true
                Write-Event 'process' ("Requested normal close through UIAutomation WindowPattern for owned app PID {0}." -f $ownedPid)
            } catch {
                Write-Event 'process' ("UIAutomation WindowPattern.Close was unavailable for owned app PID {0}: {1}" -f $ownedPid, $_.Exception.Message) 'warning'
            }
        }
    } catch { }
    if (-not $normalCloseRequested) {
        try {
            if ($Process.CloseMainWindow()) {
                $normalCloseRequested = $true
                Write-Event 'process' ("Requested normal close through CloseMainWindow for owned app PID {0}." -f $ownedPid)
            }
        } catch {
            Write-Event 'process' ("CloseMainWindow was unavailable for owned app PID {0}: {1}" -f $ownedPid, $_.Exception.Message) 'warning'
        }
    }
    if ($normalCloseRequested) {
        $deadline = [DateTime]::UtcNow.AddSeconds(15)
        do {
            $live = Get-Process -Id $ownedPid -ErrorAction SilentlyContinue
            if ($null -eq $live) {
                $script:result.sameVersionReinstall.normalCloseSucceeded = $true
                Write-Event 'process' ("Owned app PID {0} exited after a normal close request." -f $ownedPid)
                return
            }
            Start-Sleep -Milliseconds 250
        } while ([DateTime]::UtcNow -lt $deadline)
        Write-Event 'process' ("Owned app PID {0} did not exit within 15 seconds after a normal close request; using forced cleanup." -f $ownedPid) 'warning'
    }
    Stop-OwnedApp $Process
}

function Stop-OwnedAppsOnFailure {
    foreach ($record in @($script:ownedAppProcesses)) {
        try {
            $live = Get-Process -Id ([int]$record.pid) -ErrorAction SilentlyContinue
            if ($null -eq $live) { continue }
            $samePath = $false
            $sameStart = $false
            try { $samePath = [String]::Equals($live.Path, [string]$record.path, [StringComparison]::OrdinalIgnoreCase) } catch { }
            try { $sameStart = $null -ne $record.startTime -and $live.StartTime -eq $record.startTime } catch { }
            if ($samePath -and $sameStart) {
                $script:result.sameVersionReinstall.forcedProcessStop = $true
                Stop-VerifiedOwnedProcessTree $record
                Write-Event 'cleanup' ("Stopped owned app PID {0} after qualification failure." -f $record.pid) 'warning'
            } else {
                Write-Event 'cleanup' ("Skipped PID {0}; it no longer matched the recorded app identity." -f $record.pid) 'warning'
            }
        } catch {
            Write-Event 'cleanup' ("Could not inspect or stop recorded app PID {0}: {1}" -f $record.pid, $_.Exception.Message) 'warning'
        }
    }
}

function Capture-OwnedWindow {
    param(
        [Parameter(Mandatory = $true)]$Window,
        [Parameter(Mandatory = $true)][string]$Path
    )
    try {
        $handle = [IntPtr]$Window.Current.NativeWindowHandle
        if ($handle -eq [IntPtr]::Zero) { throw 'The UIAutomation window has no native handle.' }
        $rect = [QualificationWindowCapture+RECT]::new()
        if (-not [QualificationWindowCapture]::GetWindowRect($handle, [ref]$rect)) { throw 'GetWindowRect failed.' }
        $width = $rect.Right - $rect.Left
        $height = $rect.Bottom - $rect.Top
        if ($width -lt 2 -or $height -lt 2) { throw "Window bounds are invalid: ${width}x${height}." }
        $bitmap = New-Object System.Drawing.Bitmap($width, $height)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        $hdc = $graphics.GetHdc()
        try {
            $printed = [QualificationWindowCapture]::PrintWindow($handle, $hdc, 2)
            if (-not $printed) { $printed = [QualificationWindowCapture]::PrintWindow($handle, $hdc, 0) }
        } finally {
            $graphics.ReleaseHdc($hdc)
            $graphics.Dispose()
        }
        if (-not $printed) {
            $bitmap.Dispose()
            throw 'PrintWindow failed; no screen-region fallback is used because it could capture another application.'
        }
        $bitmap.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png)
        $bitmap.Dispose()
        return $true
    } catch {
        Write-Event 'screenshot' ("Owned-window screenshot failed: {0}" -f $_.Exception.Message) 'warning'
        return $false
    }
}

function Write-FailureDiagnostics {
    param([Parameter(Mandatory = $true)][int]$ProcessId)
    $script:result.diagnostics.ownedProcessId = $ProcessId
    $window = Find-AppWindow $ProcessId
    if ($null -eq $window) {
        Write-Event 'diagnostics' ("No owned UIAutomation window was available for PID {0} before cleanup." -f $ProcessId) 'warning'
        return
    }

    $failureScreenshot = Join-Path $runRoot 'failure.png'
    if (Capture-OwnedWindow $window $failureScreenshot) {
        $script:result.diagnostics.failureScreenshot = 'failure.png'
    }

    $entries = [System.Collections.Generic.List[object]]::new()
    $alertTexts = [System.Collections.Generic.List[string]]::new()
    try {
        $elements = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
        foreach ($element in $elements) {
            try {
                $name = [string]$element.Current.Name
                $controlType = [string]$element.Current.ControlType.ProgrammaticName
                $enabled = [bool]$element.Current.IsEnabled
                if ($entries.Count -lt 30) {
                    [void]$entries.Add([ordered]@{
                        name = if ($name.Length -gt 240) { $name.Substring(0, 240) } else { $name }
                        controlType = $controlType
                        enabled = $enabled
                    })
                }

                $text = $null
                if ($controlType -match 'Window|Text|Edit|Document') {
                    $text = Get-UiaText $element
                }
                $combined = "{0} {1}" -f $name, [string]$text
                if ($combined -match '(?i)error|fail|warning|alert|dialog') {
                    $diagnosticText = if ([string]::IsNullOrWhiteSpace([string]$text)) { $name } else { $text }
                    if (-not [string]::IsNullOrWhiteSpace([string]$diagnosticText) -and $alertTexts.Count -lt 16) {
                        $displayText = [string]$diagnosticText
                        if ($displayText.Length -gt 400) { $displayText = $displayText.Substring(0, 400) }
                        [void]$alertTexts.Add(("name='{0}', type='{1}', text='{2}'" -f $name, $controlType, $displayText))
                    }
                }
            } catch {
                # UIA nodes can disappear during a renderer transition. Keep
                # the diagnostic bounded and retain all nodes read successfully.
            }
        }
    } catch {
        Write-Event 'diagnostics' ("Owned UIAutomation enumeration failed: {0}" -f $_.Exception.Message) 'warning'
    }
    $script:result.diagnostics.uiAutomationEntries = @($entries)
    $script:result.diagnostics.alertTexts = @($alertTexts | Select-Object -Unique)
    Write-Event 'diagnostics' ("Captured failure diagnostics for owned PID {0}: {1} UIA entries, {2} alert/error text entries, screenshot={3}." -f $ProcessId, $entries.Count, $alertTexts.Count, ($null -ne $script:result.diagnostics.failureScreenshot)) 'warning'
}

try {
    Write-Event 'start' ("Starting hosted Windows package qualification in fresh runner root {0}. This run does not claim offline runtime or upgrade support." -f $QualificationRoot)
    $installer = Resolve-Installer
    $script:result.installer.name = $installer.Name
    $script:result.installer.sourcePath = $InstallerPath
    $script:result.installer.sha256 = Get-FileSha256 $installer.FullName
    Write-Event 'install' ("Using installer {0}; SHA-256 {1}." -f $installer.Name, $script:result.installer.sha256)
    Write-BuildMetadata

    Install-Silently -Path $installer.FullName -Stage 'install' -Arguments ("/S /D={0}" -f $InstallRoot) | Out-Null
    $installed = Find-InstalledExecutable
    Write-BuildMetadata
    $uiaAvailable = Add-UiAutomationTypes
    $script:result.firstLaunch.uiAutomationAvailable = $uiaAvailable
    if (-not $uiaAvailable) {
        $script:result.status = 'partial'
        Write-Event 'uia' 'First-launch install verification passed, but UIAutomation could not be loaded; no Library or persistence claim is made.' 'warning'
    } else {
        $app = Start-QualifiedApp -Path $installed.FullName
        $script:result.firstLaunch.pid = $app.Id
        Write-Event 'first-launch' ("Started installed app PID {0} with a hidden process window." -f $app.Id)
        $window = Wait-Until { Find-AppWindow $app.Id } 60 'the installed app UIAutomation window'
        $library = Wait-Until { Find-UiaByName $window 'Your library' } 60 'the Your library label'
        $script:result.firstLaunch.libraryVisible = $null -ne $library
        Assert-NoEditorTrial $window
        Write-Event 'first-launch' 'Confirmed the Your library label through UIAutomation.'
        $shotPath = Join-Path $runRoot 'first-launch.png'
        if (Capture-OwnedWindow $window $shotPath) { $script:result.firstLaunch.screenshot = 'first-launch.png' }

        $projectTitle = 'CI package qualification project'
        $documentTitle = 'CI package qualification chapter'
        $englishText = 'The harbour bell rings once. A red lantern waits beside the quiet pier.'
        $newProject = Wait-Until { Find-UiaByName $window 'New project' } 20 'New project button'
        Invoke-Uia $newProject
        $projectTitleBox = Wait-Until { Find-UiaByName $window 'Project title' ([System.Windows.Automation.ControlType]::Edit) } 15 'Project title field'
        if ($null -eq (Set-UiaValue $projectTitleBox $projectTitle)) { throw 'Project title did not expose ValuePattern.' }
        Invoke-Uia (Wait-Until { Find-UiaByName $window 'Create project' } 15 'Create project submit button')
        $script:result.firstLaunch.projectCreated = $true
        Invoke-Uia (Wait-Until { Find-UiaByName $window 'Start writing' } 30 'blank project Start writing choice')
        Invoke-Uia (Wait-Until { Find-UiaByName $window 'Create a chapter' } 30 'Create a chapter button')
        Select-Chapter $window
        $titleBox = Wait-Until { Find-UiaByName $window 'Title' ([System.Windows.Automation.ControlType]::Edit) } 15 'document Title field'
        if ($null -eq (Set-UiaValue $titleBox $documentTitle)) { throw 'Document title did not expose ValuePattern.' }
        Invoke-Uia (Wait-Until { Find-UiaByName $window 'Create' } 15 'document Create button')
        $script:result.firstLaunch.documentCreated = $true
        $editor = Wait-Until { Find-UiaByName $window 'Manuscript' } 30 'Manuscript editor'
        $entryMethod = Set-UiaText $editor $englishText
        $script:result.firstLaunch.textEntryMethod = $entryMethod
        $script:result.firstLaunch.textEntered = $null -ne $entryMethod
        if ($null -eq $entryMethod) {
            Write-Event 'first-launch' 'UIAutomation could not type into the Manuscript editor; qualification is limited to first-launch Library evidence.' 'warning'
        } else {
            Start-Sleep -Seconds 2
            $readback = Get-UiaText $editor
            $script:result.firstLaunch.textReadback = $null -ne $readback -and $readback.Contains($englishText)
            if (-not $script:result.firstLaunch.textReadback) {
                Write-Event 'first-launch' 'Text entry was accepted by a UIA pattern, but the editor did not provide a matching text readback.' 'warning'
            }
        }
        try { Wait-Until { Find-UiaByName $window 'Saved' } 30 'Saved status' | Out-Null } catch { Write-Event 'first-launch' 'Saved status was not exposed through UIAutomation before timeout.' 'warning' }

        Invoke-Uia (Wait-Until { Find-UiaByName $window 'All projects' } 20 'All projects button')
        # A WebView2 navigation may replace the UIA subtree. Reacquire the
        # owned native window while waiting; querying the pre-navigation
        # element can remain empty after the renderer has returned to Library.
        $window = Wait-Until { Find-AppWindow $app.Id } 30 'the installed app UIAutomation window after closing the editor'
        Wait-Until {
            $currentWindow = Find-AppWindow $app.Id
            if ($null -ne $currentWindow) { Find-UiaByName $currentWindow 'Your library' }
        } 30 'Your library after closing the editor' | Out-Null
        $window = Find-AppWindow $app.Id
        try {
            $projectButton = Wait-Until {
                $currentWindow = Find-AppWindow $app.Id
                if ($null -ne $currentWindow) { Find-ProjectOpener $currentWindow $projectTitle }
            } 30 'created project opener in the Library'
        } catch {
            Write-Event 'uia' ("Project opener UIA snapshot at timeout: {0}" -f (Get-ProjectOpenerSnapshot $window $projectTitle)) 'warning'
            throw
        }
        Invoke-Uia $projectButton
        $reopenedEditor = Wait-Until {
            $currentWindow = Find-AppWindow $app.Id
            if ($null -ne $currentWindow) { Find-UiaByName $currentWindow 'Manuscript' }
        } 30 'reopened Manuscript editor'
        $reopenedText = Get-UiaText $reopenedEditor
        $script:result.firstLaunch.reopened = $true
        if ($script:result.firstLaunch.textReadback) {
            $script:result.firstLaunch.textReadback = $null -ne $reopenedText -and $reopenedText.Contains($englishText)
        }
        Write-Event 'first-launch' ("Closed the editor to Library and reopened the synthetic project; text readback after reopen: {0}." -f $script:result.firstLaunch.textReadback)
        Close-QualifiedApp $app

        $uninstallerPath = Join-Path $InstallRoot 'uninstall.exe'
        if (-not (Test-Path -LiteralPath $uninstallerPath -PathType Leaf)) {
            throw [System.IO.FileNotFoundException]::new("Expected uninstaller was not installed: $uninstallerPath")
        }
        $script:result.sameVersionReinstall.defaultUninstallAttempted = $true
        # NSIS normally copies the uninstaller to TEMP and returns from its
        # bootstrap process. Its documented final _?= argument keeps removal
        # in the process we wait for. Preserve the default data-retention choice.
        $uninstallRoot = Assert-ContainedPath $InstallRoot $QualificationRoot 'Uninstall root'
        Write-Event 'default-uninstall' 'Running the uninstaller in place with /S /P and final _?= so process exit covers removal; no delete-data option is supplied.'
        Install-Silently -Path $uninstallerPath -Stage 'default-uninstall' -Arguments ("/S /P _?={0}" -f $uninstallRoot) | Out-Null
        if (Test-Path -LiteralPath (Join-Path $InstallRoot 'webnovel-desktop.exe') -PathType Leaf) {
            throw 'Default uninstall returned success but the application executable remains.'
        }
        Write-Event 'default-uninstall' 'Default uninstall removed application files; no delete-data option was supplied.'
        $script:result.sameVersionReinstall.attempted = $true
        Install-Silently -Path $installer.FullName -Stage 'same-version-reinstall' -Arguments ("/S /D={0}" -f $InstallRoot) | Out-Null
        $app = Start-QualifiedApp -Path $installed.FullName
        $window = Wait-Until { Find-AppWindow $app.Id } 60 'the reinstalled app UIAutomation window'
        $library = Wait-Until { Find-UiaByName $window 'Your library' } 60 'the Your library label after same-version reinstall'
        $script:result.sameVersionReinstall.libraryVisible = $null -ne $library
        Assert-NoEditorTrial $window
        try {
            $retained = Wait-Until { Find-ProjectOpener $window $projectTitle } 30 'retained project opener after same-version reinstall'
        } catch {
            Write-Event 'uia' ("Reinstall project opener UIA snapshot at timeout: {0}" -f (Get-ProjectOpenerSnapshot $window $projectTitle)) 'warning'
            $retained = $null
        }
        $script:result.sameVersionReinstall.projectRetained = $null -ne $retained
        if ($null -ne $retained) {
            try {
                Invoke-Uia $retained
                $reinstalledEditor = Wait-Until {
                    $currentWindow = Find-AppWindow $app.Id
                    if ($null -ne $currentWindow) { Find-UiaByName $currentWindow 'Manuscript' }
                } 30 'reinstalled Manuscript editor'
                $window = Find-AppWindow $app.Id
                $reinstalledDocument = Find-UiaByNameContains $window $documentTitle ([System.Windows.Automation.ControlType]::Button)
                $script:result.sameVersionReinstall.documentRetained = $null -ne $reinstalledDocument
                $reinstalledText = Get-UiaText $reinstalledEditor
                $script:result.sameVersionReinstall.textRetained = $script:result.firstLaunch.textReadback -and $null -ne $reinstalledText -and $reinstalledText.Contains($englishText)
                Write-Event 'same-version-reinstall' ("After reinstall, document retained: {0}; text retained by UIA readback: {1}." -f $script:result.sameVersionReinstall.documentRetained, $script:result.sameVersionReinstall.textRetained)
            } catch {
                Write-Event 'same-version-reinstall' ("Optional document/text readback after reinstall was unavailable: {0}" -f $_.Exception.Message) 'warning'
            }
        }
        Write-Event 'same-version-reinstall' ("Reinstalled the identical installer after default uninstall and found the synthetic project in Library: {0}. This is retention evidence, not an upgrade qualification." -f $script:result.sameVersionReinstall.projectRetained)
        $shotPath = Join-Path $runRoot 'same-version-reinstall.png'
        if (Capture-OwnedWindow $window $shotPath) { $script:result.sameVersionReinstall.screenshot = 'same-version-reinstall.png' }
        Close-QualifiedApp $app

        if ($script:result.firstLaunch.libraryVisible -and $script:result.firstLaunch.projectCreated -and $script:result.firstLaunch.documentCreated -and $script:result.firstLaunch.reopened -and $script:result.sameVersionReinstall.projectRetained -and $script:result.sameVersionReinstall.documentRetained -and $script:result.sameVersionReinstall.textRetained -and $script:result.sameVersionReinstall.normalCloseSucceeded -and -not $script:result.sameVersionReinstall.forcedProcessStop) {
            if ($script:result.firstLaunch.textReadback) {
                $script:result.status = 'passed'
            } else {
                $script:result.status = 'partial'
                Write-Event 'result' 'Install, Library, project, reopen, and same-version retention passed; editor text readback was unavailable or mismatched.' 'warning'
            }
        } else {
            $script:result.status = 'partial'
            Write-Event 'result' 'Install and/or UI workflow was incomplete; result is partial and does not claim data retention.' 'warning'
        }
    }
} catch {
    $message = $_.Exception.Message
    $script:result.errors.Add($message)
    $script:result.status = if ($script:blocked) { 'blocked' } else { 'failed' }
    if ($null -ne $app) {
        try { Write-FailureDiagnostics -ProcessId $app.Id } catch { Write-Event 'diagnostics' ("Failure diagnostics could not be collected: {0}" -f $_.Exception.Message) 'warning' }
    }
    Write-Event 'error' $message 'error'
} finally {
    try {
        Stop-OwnedAppsOnFailure
    } catch {
        $cleanupMessage = "Owned process cleanup failed: $($_.Exception.Message)"
        $script:result.errors.Add($cleanupMessage)
        if ($script:result.status -eq 'passed') { $script:result.status = 'failed' }
        Write-Event 'cleanup' $cleanupMessage 'error'
    }
    if ($script:result.status -eq 'passed' -and $script:result.sameVersionReinstall.forcedProcessStop) {
        $script:result.status = 'partial'
        Write-Event 'cleanup' 'The lifecycle required forced process cleanup; downgrading the result so normal-close qualification cannot pass.' 'error'
    }
    Save-Result
}

Write-Output ("Qualification {0}. Result: {1}" -f $script:result.status, $resultPath)
if ($script:result.status -eq 'failed') { exit 1 }
if ($script:result.status -eq 'blocked') { exit 2 }
if ($script:result.status -eq 'partial') { exit 3 }
exit 0
