param(
    [Parameter(Mandatory)][int]$OwnerPid,
    [ValidateSet('Read', 'Dismiss')][string]$Action = 'Read',
    [switch]$WaitForSignal
)

$ErrorActionPreference = 'Stop'
$ownedProcess = Get-Process -Id $OwnerPid -ErrorAction Stop
if ($ownedProcess.ProcessName -ne 'webnovel-desktop') {
    throw "The PID is not the owned native test application: $($ownedProcess.ProcessName)."
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class OwnedContextMenu {
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect { public int Left; public int Top; public int Right; public int Bottom; }
    [StructLayout(LayoutKind.Sequential)]
    public struct Point { public int X; public int Y; }
    public delegate bool EnumWindowsProc(IntPtr window, IntPtr data);
    [DllImport("user32.dll", EntryPoint="GetWindowThreadProcessId")]
    public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr window, ref Point point);
    [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumWindowsProc callback, IntPtr data);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    public static extern int GetClassName(IntPtr window, System.Text.StringBuilder name, int capacity);
    [DllImport("user32.dll", EntryPoint="PostMessageW")]
    public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
    public static IntPtr[] DescendantWindows(IntPtr parent) {
        var windows = new System.Collections.Generic.List<IntPtr>();
        EnumChildWindows(parent, (window, _) => { windows.Add(window); return true; }, IntPtr.Zero);
        return windows.ToArray();
    }
    public static IntPtr MouseLParam(int x, int y) {
        return new IntPtr((y << 16) | (x & 0xffff));
    }
    public static string ClassName(IntPtr window) {
        var name = new System.Text.StringBuilder(256);
        return GetClassName(window, name, name.Capacity) > 0 ? name.ToString() : "";
    }
}
"@

function Get-OwnedProcessIds {
    $processes = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)
    $owned = [System.Collections.Generic.HashSet[uint32]]::new()
    [void]$owned.Add([uint32]$OwnerPid)
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($processInfo in $processes) {
            $candidatePid = [uint32]$processInfo.ProcessId
            $parentPid = [uint32]$processInfo.ParentProcessId
            if ($owned.Contains($parentPid) -and $owned.Add($candidatePid)) {
                $changed = $true
            }
        }
    }
    return @($owned | Sort-Object)
}

function Get-WebViewTarget {
    $ownedProcessIds = @(Get-OwnedProcessIds)
    $mainHandle = [IntPtr]$ownedProcess.MainWindowHandle
    if ($mainHandle -eq [IntPtr]::Zero) {
        $root = [Windows.Automation.AutomationElement]::RootElement
        $windowQuery = [Windows.Automation.AndCondition]::new(
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ProcessIdProperty, $OwnerPid),
            [Windows.Automation.PropertyCondition]::new(
                [Windows.Automation.AutomationElement]::ControlTypeProperty,
                [Windows.Automation.ControlType]::Window))
        $windows = $root.FindAll([Windows.Automation.TreeScope]::Descendants, $windowQuery)
        foreach ($window in $windows) {
            $candidate = [IntPtr]$window.Current.NativeWindowHandle
            if ($candidate -ne [IntPtr]::Zero -and [OwnedContextMenu]::IsWindow($candidate)) {
                $mainHandle = $candidate
                break
            }
        }
    }
    if ($mainHandle -eq [IntPtr]::Zero -or -not [OwnedContextMenu]::IsWindow($mainHandle)) {
        throw "The owned native application did not expose a main window for PID $OwnerPid."
    }

    $handles = @([OwnedContextMenu]::DescendantWindows($mainHandle))
    $candidates = [System.Collections.Generic.List[object]]::new()
    foreach ($handle in $handles) {
        if ($handle -eq [IntPtr]::Zero -or -not [OwnedContextMenu]::IsWindow($handle) -or -not [OwnedContextMenu]::IsWindowVisible($handle)) { continue }
        $windowPid = [uint32]0
        [OwnedContextMenu]::GetWindowThreadProcessId($handle, [ref]$windowPid) | Out-Null
        if (-not ($ownedProcessIds -contains $windowPid)) { continue }
        $client = [OwnedContextMenu+Rect]::new()
        if (-not [OwnedContextMenu]::GetClientRect($handle, [ref]$client)) { continue }
        $width = $client.Right - $client.Left
        $height = $client.Bottom - $client.Top
        if ($width -lt 200 -or $height -lt 120) { continue }
        $screenOrigin = [OwnedContextMenu+Point]::new()
        if (-not [OwnedContextMenu]::ClientToScreen($handle, [ref]$screenOrigin)) { continue }
        $candidates.Add([ordered]@{
            handle = $handle.ToInt64()
            processId = $windowPid
            className = [OwnedContextMenu]::ClassName($handle)
            screenLeft = $screenOrigin.X
            screenTop = $screenOrigin.Y
            clientWidth = $width
            clientHeight = $height
            area = $width * $height
        })
    }
    if ($candidates.Count -eq 0) {
        throw 'The owned WebView child window was not discoverable for PID-targeted context-menu input.'
    }
    $preferredClasses = @('Chrome_RenderWidgetHostHWND', 'Chrome_WidgetWin_0', 'Chrome_WidgetWin_1')
    $preferred = @($candidates | Where-Object { $preferredClasses -contains $_.className })
    if ($preferred.Count -gt 0) {
        return $preferred | Sort-Object area -Descending | Select-Object -First 1
    }
    return $candidates | Sort-Object area -Descending | Select-Object -First 1
}

function Send-WebViewRightClick {
    param([Parameter(Mandatory)]$Target, [Parameter(Mandatory)][int]$X, [Parameter(Mandatory)][int]$Y)
    if ($X -lt 0 -or $Y -lt 0 -or $X -ge $Target.clientWidth -or $Y -ge $Target.clientHeight) {
        throw "The requested WebView context-menu point ($X,$Y) is outside the owned client area."
    }
    $handle = [IntPtr]$Target.handle
    $livePid = [uint32]0
    [OwnedContextMenu]::GetWindowThreadProcessId($handle, [ref]$livePid) | Out-Null
    $ownedProcessIds = @(Get-OwnedProcessIds)
    if (-not [OwnedContextMenu]::IsWindow($handle) -or -not [OwnedContextMenu]::IsWindowVisible($handle) -or
        -not ($ownedProcessIds -contains $livePid) -or $livePid -ne [uint32]$Target.processId) {
        throw "The selected WebView HWND $($Target.handle) was no longer owned immediately before WM_RBUTTON input."
    }
    $lParam = [OwnedContextMenu]::MouseLParam($X, $Y)
    # Send only to the owned WebView child. This does not move the user's
    # pointer, steal foreground focus, or inject input into another process.
    if (-not [OwnedContextMenu]::PostMessage($handle, 0x0204, [IntPtr]2, $lParam)) {
        throw 'The owned WebView rejected WM_RBUTTONDOWN.'
    }
    if (-not [OwnedContextMenu]::PostMessage($handle, 0x0205, [IntPtr]0, $lParam)) {
        throw 'The owned WebView rejected WM_RBUTTONUP.'
    }
}

function Get-ContextMenuSnapshot {
    $root = [Windows.Automation.AutomationElement]::RootElement
    $menuItemCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::MenuItem)
    $menuCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::Menu)
    $ownedProcessIds = @(Get-OwnedProcessIds)
    $windows = @{}
    $items = [System.Collections.Generic.List[object]]::new()

    # Popup menus can be top-level windows or descendants of the WebView2
    # process. Query only process IDs in the owned tree so another application
    # is never inspected or dismissed.
    foreach ($ownedPid in $ownedProcessIds) {
        $processCondition = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ProcessIdProperty, [int]$ownedPid)
        $menuQuery = [Windows.Automation.AndCondition]::new($processCondition, $menuCondition)
        $ownedMenus = $root.FindAll([Windows.Automation.TreeScope]::Descendants, $menuQuery)
        foreach ($element in $ownedMenus) {
            try {
                $handle = [IntPtr]$element.Current.NativeWindowHandle
                if ($handle -ne [IntPtr]::Zero -and [OwnedContextMenu]::IsWindow($handle)) {
                    $windows[$handle.ToInt64()] = $handle
                }
            } catch { }
        }
    }

    # Menu items are often descendants of a popup whose ProcessId is the
    # WebView2 child rather than the Tauri parent. Constrain every UIA query
    # by the previously computed owned process ID before reading labels.
    foreach ($ownedPid in $ownedProcessIds) {
        $processCondition = [Windows.Automation.PropertyCondition]::new(
            [Windows.Automation.AutomationElement]::ProcessIdProperty, [int]$ownedPid)
        $itemQuery = [Windows.Automation.AndCondition]::new($processCondition, $menuItemCondition)
        $ownedItems = $root.FindAll([Windows.Automation.TreeScope]::Descendants, $itemQuery)
        foreach ($element in $ownedItems) {
            try {
                if ($element.Current.IsOffscreen) { continue }
                $elementPid = [uint32]$element.Current.ProcessId
                $name = [string]$element.Current.Name
                $automationId = [string]$element.Current.AutomationId
                $className = [string]$element.Current.ClassName
                $handle = [IntPtr]$element.Current.NativeWindowHandle
                $items.Add([ordered]@{
                    name = $name
                    automationId = $automationId
                    className = $className
                    enabled = [bool]$element.Current.IsEnabled
                    processId = $elementPid
                    windowHandle = $handle.ToInt64()
                })
                if ($handle -ne [IntPtr]::Zero -and [OwnedContextMenu]::IsWindow($handle)) {
                    $windows[$handle.ToInt64()] = $handle
                }
            } catch { }
        }
    }

    $contextItems = @($items.ToArray() | Where-Object { $_.name -ne 'System' })
    $contextHandles = if ($contextItems.Count -gt 0) {
        @($windows.Keys | ForEach-Object { [int64]$_ })
    } else {
        @()
    }
    [ordered]@{
        ownerPid = $OwnerPid
        ownedProcessIds = @($ownedProcessIds)
        menuVisible = ($contextItems.Count -gt 0)
        items = $contextItems
        menuWindowHandles = $contextHandles
        inspectedProcessIds = @($contextItems | ForEach-Object { $_.processId } | Sort-Object -Unique)
    }
}

if ($WaitForSignal) {
    $webView = Get-WebViewTarget
    @{ ready = $true; ownerPid = $OwnerPid; webView = $webView } | ConvertTo-Json -Compress -Depth 6
    $signalText = [Console]::In.ReadToEnd()
    try { $signal = $signalText | ConvertFrom-Json } catch { throw 'The native context-menu input signal was not valid JSON.' }
    if ($null -eq $signal.x -or $null -eq $signal.y) { throw 'The native context-menu input signal did not contain x and y.' }
    Send-WebViewRightClick -Target $webView -X ([int]$signal.x) -Y ([int]$signal.y)
}

$deadline = [DateTime]::UtcNow.AddSeconds(8)
$snapshot = $null
while ([DateTime]::UtcNow -lt $deadline) {
    $snapshot = Get-ContextMenuSnapshot
    if ($snapshot.menuVisible) { break }
    Start-Sleep -Milliseconds 100
}
if ($null -eq $snapshot) { throw 'The owned native context-menu snapshot could not be created.' }

if ($Action -eq 'Dismiss') {
    foreach ($handleValue in @($snapshot.menuWindowHandles)) {
        $handle = [IntPtr]$handleValue
        if (-not [OwnedContextMenu]::IsWindow($handle)) { continue }
        # WM_CANCELMODE closes only the owned popup menu. Do not send a global
        # Escape or mouse event that could affect another application.
        [OwnedContextMenu]::PostMessage($handle, 0x001F, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
    }
    $snapshot.dismissed = $true
}

$snapshot | ConvertTo-Json -Compress -Depth 8
