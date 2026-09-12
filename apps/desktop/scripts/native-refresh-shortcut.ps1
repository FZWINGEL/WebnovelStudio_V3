param(
    [Parameter(Mandatory)][int]$OwnerPid,
    [Parameter(Mandatory)][ValidateSet('ControlR', 'F5', 'ShiftF5', 'ControlF5', 'ControlShiftF5', 'ControlShiftR', 'Probe')][string]$Key,
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
public static class OwnedRefreshShortcut {
    [StructLayout(LayoutKind.Sequential)]
    public struct KeyboardInput {
        public ushort VirtualKey;
        public ushort ScanCode;
        public uint Flags;
        public uint Time;
        public IntPtr ExtraInfo;
    }
    [StructLayout(LayoutKind.Explicit, Size=40)]
    public struct Input {
        [FieldOffset(0)] public uint Type;
        [FieldOffset(8)] public KeyboardInput Keyboard;
    }
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect { public int Left; public int Top; public int Right; public int Bottom; }
    [StructLayout(LayoutKind.Sequential)]
    public struct GuiThreadInfo {
        public uint Size;
        public uint Flags;
        public IntPtr Active;
        public IntPtr Focus;
        public IntPtr Capture;
        public IntPtr MenuOwner;
        public IntPtr MoveSize;
        public IntPtr Caret;
        public Rect CaretRect;
    }
    [DllImport("user32.dll", EntryPoint="GetWindowThreadProcessId")]
    public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")]
    public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")]
    public static extern bool IsChild(IntPtr parent, IntPtr child);
    [DllImport("user32.dll")]
    public static extern IntPtr GetAncestor(IntPtr window, uint flags);
    [DllImport("user32.dll")]
    public static extern IntPtr GetParent(IntPtr window);
    [DllImport("user32.dll")]
    public static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")]
    public static extern bool BringWindowToTop(IntPtr window);
    [DllImport("user32.dll")]
    public static extern bool SetActiveWindow(IntPtr window);
    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();
    [DllImport("kernel32.dll")]
    public static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")]
    public static extern bool AttachThreadInput(uint sourceThread, uint targetThread, bool attach);
    [DllImport("user32.dll")]
    public static extern bool GetGUIThreadInfo(uint thread, ref GuiThreadInfo info);
    [DllImport("user32.dll")]
    public static extern uint SendInput(uint count, Input[] inputs, int size);
    public static IntPtr FocusedWindow(IntPtr window) {
        uint ignoredPid;
        uint thread = GetWindowThreadProcessId(window, out ignoredPid);
        var info = new GuiThreadInfo { Size = (uint)Marshal.SizeOf<GuiThreadInfo>() };
        return GetGUIThreadInfo(thread, ref info) ? info.Focus : IntPtr.Zero;
    }
    public static bool FocusWindow(IntPtr window) {
        uint ignoredPid;
        uint targetThread = GetWindowThreadProcessId(window, out ignoredPid);
        uint foregroundThread = GetWindowThreadProcessId(GetForegroundWindow(), out ignoredPid);
        uint currentThread = GetCurrentThreadId();
        bool attachedForeground = foregroundThread != currentThread && foregroundThread != 0 && AttachThreadInput(currentThread, foregroundThread, true);
        bool attachedTarget = targetThread != currentThread && targetThread != 0 && AttachThreadInput(currentThread, targetThread, true);
        try {
            ShowWindow(window, 5);
            BringWindowToTop(window);
            SetActiveWindow(window);
            return SetForegroundWindow(window);
        } finally {
            if (attachedTarget) AttachThreadInput(currentThread, targetThread, false);
            if (attachedForeground) AttachThreadInput(currentThread, foregroundThread, false);
        }
    }
    public static void SendKey(ushort key, bool up) {
        var input = new Input { Type = 1, Keyboard = new KeyboardInput { VirtualKey = key, Flags = up ? 2u : 0u } };
        if (SendInput(1, new[] { input }, Marshal.SizeOf<Input>()) != 1) throw new InvalidOperationException("SendInput rejected the keyboard event.");
    }
}
"@

$handle = [IntPtr]$process.MainWindowHandle
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
if ($handle -eq [IntPtr]::Zero -or -not [OwnedRefreshShortcut]::IsWindow($handle)) {
    throw "The owned native application did not expose a main window for PID $OwnerPid."
}
$windowPid = [uint32]0
[OwnedRefreshShortcut]::GetWindowThreadProcessId($handle, [ref]$windowPid) | Out-Null
if ($windowPid -ne [uint32]$OwnerPid) {
    throw "The native window belongs to PID $windowPid, not the owned PID $OwnerPid."
}

$ready = @{ ready = $true; ownerPid = $OwnerPid; windowHandle = $handle.ToInt64(); key = $Key; verified = $true } | ConvertTo-Json -Compress
Write-Output $ready
if ($WaitForSignal) {
    [Console]::In.ReadToEnd() | Out-Null
}

[OwnedRefreshShortcut]::ShowWindow($handle, 5) | Out-Null
[OwnedRefreshShortcut]::FocusWindow($handle) | Out-Null
function Test-OwnedProcessTree {
    param([uint32]$CandidatePid, [uint32]$RootPid)
    $currentPid = $CandidatePid
    for ($depth = 0; $depth -lt 12; $depth += 1) {
        if ($currentPid -eq $RootPid) { return $true }
        $processInfo = Get-CimInstance Win32_Process -Filter "ProcessId = $currentPid" -ErrorAction SilentlyContinue
        if (-not $processInfo -or [uint32]$processInfo.ParentProcessId -eq $currentPid) { return $false }
        $currentPid = [uint32]$processInfo.ParentProcessId
    }
    return $false
}
function Assert-OwnedFocus {
    param([string]$ShortcutKey)
    $foregroundPid = [uint32]0
    $foregroundReady = $false
    for ($attempt = 0; $attempt -lt 20; $attempt += 1) {
        $foreground = [OwnedRefreshShortcut]::GetForegroundWindow()
        if ($foreground -ne [IntPtr]::Zero) {
            [OwnedRefreshShortcut]::GetWindowThreadProcessId($foreground, [ref]$foregroundPid) | Out-Null
            if ($foregroundPid -eq [uint32]$OwnerPid) {
                $foregroundReady = $true
                break
            }
        }
        Start-Sleep -Milliseconds 100
    }
    if (-not $foregroundReady) {
        throw "The owned native application was not foreground before sending $ShortcutKey (foreground PID $foregroundPid)."
    }
    $focused = [OwnedRefreshShortcut]::FocusedWindow($handle)
    if ($focused -eq [IntPtr]::Zero) {
        throw "The owned WebView did not expose a focused child before sending $ShortcutKey."
    }
    $focusedPid = [uint32]0
    [OwnedRefreshShortcut]::GetWindowThreadProcessId($focused, [ref]$focusedPid) | Out-Null
    $focusedRoot = [OwnedRefreshShortcut]::GetAncestor($focused, 2)
    $isOwnedChild = [OwnedRefreshShortcut]::IsChild($handle, $focused)
    $parent = [OwnedRefreshShortcut]::GetParent($focused)
    if (-not $isOwnedChild -and $focusedRoot -ne $handle -and $parent -ne $handle -and -not (Test-OwnedProcessTree -CandidatePid $focusedPid -RootPid ([uint32]$OwnerPid))) {
        throw "The focused window belongs to PID $focusedPid outside the owned application process tree rooted at $OwnerPid (focused=$focused, root=$focusedRoot, parent=$parent, main=$handle)."
    }
}
Assert-OwnedFocus -ShortcutKey $Key
$virtual = @{ R = [byte]0x52; F5 = [byte]0x74; CTRL = [byte]0x11; SHIFT = [byte]0x10 }
$keysStarted = $false
try {
    # Recheck ownership and focus immediately before SendInput; the user can
    # switch windows while the helper is waiting on process cleanup.
    Assert-OwnedFocus -ShortcutKey $Key
    $keysStarted = $true
    if ($Key -eq 'Probe') {
        [OwnedRefreshShortcut]::SendKey($virtual.CTRL, $false)
        [OwnedRefreshShortcut]::SendKey($virtual.CTRL, $true)
    } elseif ($Key -eq 'F5') {
        [OwnedRefreshShortcut]::SendKey($virtual.F5, $false)
        [OwnedRefreshShortcut]::SendKey($virtual.F5, $true)
    } elseif ($Key -eq 'ShiftF5' -or $Key -eq 'ControlF5' -or $Key -eq 'ControlShiftF5') {
        $controlF5 = $Key -eq 'ControlF5' -or $Key -eq 'ControlShiftF5'
        $shiftF5 = $Key -eq 'ShiftF5' -or $Key -eq 'ControlShiftF5'
        if ($controlF5) { [OwnedRefreshShortcut]::SendKey($virtual.CTRL, $false) }
        if ($shiftF5) { [OwnedRefreshShortcut]::SendKey($virtual.SHIFT, $false) }
        [OwnedRefreshShortcut]::SendKey($virtual.F5, $false)
        [OwnedRefreshShortcut]::SendKey($virtual.F5, $true)
        if ($shiftF5) { [OwnedRefreshShortcut]::SendKey($virtual.SHIFT, $true) }
        if ($controlF5) { [OwnedRefreshShortcut]::SendKey($virtual.CTRL, $true) }
    } else {
        [OwnedRefreshShortcut]::SendKey($virtual.CTRL, $false)
        if ($Key -eq 'ControlShiftR') {
            [OwnedRefreshShortcut]::SendKey($virtual.SHIFT, $false)
        }
        [OwnedRefreshShortcut]::SendKey($virtual.R, $false)
        [OwnedRefreshShortcut]::SendKey($virtual.R, $true)
        if ($Key -eq 'ControlShiftR') {
            [OwnedRefreshShortcut]::SendKey($virtual.SHIFT, $true)
        }
        [OwnedRefreshShortcut]::SendKey($virtual.CTRL, $true)
    }
} finally {
    # Release every modifier if an individual SendInput call failed.
    if (-not $keysStarted) {
        # Failed ownership checks must never inject even a key release.
    } elseif ($Key -eq 'Probe') {
        [OwnedRefreshShortcut]::SendKey($virtual.CTRL, $true)
    } elseif ($Key -eq 'ShiftF5' -or $Key -eq 'ControlF5' -or $Key -eq 'ControlShiftF5') {
        if ($Key -eq 'ShiftF5' -or $Key -eq 'ControlShiftF5') { [OwnedRefreshShortcut]::SendKey($virtual.SHIFT, $true) }
        if ($Key -eq 'ControlF5' -or $Key -eq 'ControlShiftF5') { [OwnedRefreshShortcut]::SendKey($virtual.CTRL, $true) }
    } elseif ($Key -ne 'F5') {
        [OwnedRefreshShortcut]::SendKey($virtual.R, $true)
        if ($Key -eq 'ControlShiftR') { [OwnedRefreshShortcut]::SendKey($virtual.SHIFT, $true) }
        [OwnedRefreshShortcut]::SendKey($virtual.CTRL, $true)
    }
}
@{ action = 'shortcut-sent'; ownerPid = $OwnerPid; windowHandle = $handle.ToInt64(); key = $Key; verified = $true } | ConvertTo-Json -Compress
