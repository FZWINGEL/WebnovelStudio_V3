param(
    [Parameter(Mandatory)][int]$OwnerPid,
    [switch]$WaitForSignal
)

$ErrorActionPreference = 'Stop'
$process = Get-Process -Id $OwnerPid -ErrorAction Stop
if ($process.ProcessName -ne 'webnovel-desktop') {
    throw "The PID is not the owned native test application: $($process.ProcessName)."
}

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class OwnedCloseWindow {
    private delegate bool EnumWindowCallback(IntPtr window, IntPtr arg);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowCallback callback, IntPtr arg);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr window, System.Text.StringBuilder text, int count);
    [DllImport("user32.dll", EntryPoint="GetWindowThreadProcessId")]
    public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")]
    public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll", EntryPoint="PostMessageW")]
    public static extern bool PostMessage(IntPtr window, uint message, IntPtr wParam, IntPtr lParam);
    public static IntPtr FindMainWindow(uint owner) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((window, unused) => {
            uint pid; GetWindowThreadProcessId(window, out pid);
            if (pid != owner) return true;
            var text = new System.Text.StringBuilder(256);
            GetWindowText(window, text, 256);
            if (!text.ToString().StartsWith("WebnovelStudio V3", StringComparison.Ordinal)) return true;
            found = window; return false;
        }, IntPtr.Zero);
        return found;
    }
}
"@

# Process.MainWindowHandle can return Tauri's untitled helper window.
# WM_CLOSE must target the owned writing window to exercise its save guard.
$handle = [OwnedCloseWindow]::FindMainWindow([uint32]$OwnerPid)
if ($handle -eq [IntPtr]::Zero) {
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    $pidCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ProcessIdProperty, $OwnerPid)
    $windowCondition = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ControlTypeProperty,
        [Windows.Automation.ControlType]::Window)
    $condition = [Windows.Automation.AndCondition]::new($pidCondition, $windowCondition)
    $windows = [Windows.Automation.AutomationElement]::RootElement.FindAll(
        [Windows.Automation.TreeScope]::Children, $condition)
    if ($windows.Count -eq 0) {
        $windows = [Windows.Automation.AutomationElement]::RootElement.FindAll(
            [Windows.Automation.TreeScope]::Descendants, $condition)
    }
    if ($windows.Count -gt 0) { $handle = [IntPtr]$windows[0].Current.NativeWindowHandle }
}
if ($handle -eq [IntPtr]::Zero -or -not [OwnedCloseWindow]::IsWindow($handle)) {
    throw "The owned native application did not expose a main window for PID $OwnerPid."
}
$windowPid = [uint32]0
[OwnedCloseWindow]::GetWindowThreadProcessId($handle, [ref]$windowPid) | Out-Null
if ($windowPid -ne [uint32]$OwnerPid) {
    throw "The native window belongs to PID $windowPid, not the owned PID $OwnerPid."
}
$ready = @{ ready = $true; ownerPid = $OwnerPid; windowHandle = $handle.ToInt64(); verified = $true } | ConvertTo-Json -Compress
Write-Output $ready
if ($WaitForSignal) {
    [Console]::In.ReadToEnd() | Out-Null
}
if (-not [OwnedCloseWindow]::PostMessage($handle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) {
    throw "The owned native window rejected WM_CLOSE."
}
@{ action = 'WM_CLOSE'; ownerPid = $OwnerPid; windowHandle = $handle.ToInt64(); verified = $true } | ConvertTo-Json -Compress
