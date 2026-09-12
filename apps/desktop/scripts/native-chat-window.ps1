param(
    [Parameter(Mandatory)][int]$OwnerPid,
    [ValidateRange(800, 1600)][int]$LogicalWidth = 800,
    [ValidateRange(600, 1200)][int]$LogicalHeight = 600,
    [switch]$Capture
)

# Resize only the synthetic process supplied by the owning native harness.
# Never activate a window or switch the interactive desktop.
$ErrorActionPreference = 'Stop'
$ownedProcess = Get-Process -Id $OwnerPid -ErrorAction Stop
if ($ownedProcess.ProcessName -ne 'webnovel-desktop') { throw 'Unexpected native test process.' }
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class OwnedChatWindow {
    private delegate bool EnumWindowCallback(IntPtr window, IntPtr arg);
    [DllImport("user32.dll")] private static extern bool EnumWindows(EnumWindowCallback callback, IntPtr arg);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr window, out Rect rect);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr window, System.Text.StringBuilder text, int count);
    [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
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
[OwnedChatWindow]::SetProcessDpiAwarenessContext([IntPtr](-4)) | Out-Null
$handle = [OwnedChatWindow]::FindMainWindow([uint32]$OwnerPid)
if ($handle -eq [IntPtr]::Zero) { throw 'The owned test process has no main window.' }
$windowPid = [uint32]0
[OwnedChatWindow]::GetWindowThreadProcessId($handle, [ref]$windowPid) | Out-Null
if ($windowPid -ne [uint32]$OwnerPid) { throw 'Window ownership changed.' }
$outer = New-Object OwnedChatWindow+Rect
$client = New-Object OwnedChatWindow+Rect
if (-not [OwnedChatWindow]::GetWindowRect($handle, [ref]$outer) -or -not [OwnedChatWindow]::GetClientRect($handle, [ref]$client)) {
    throw 'Unable to inspect the owned window bounds.'
}
$scale = [OwnedChatWindow]::GetDpiForWindow($handle) / 96.0
if ($scale -le 0) { throw 'The owned window did not report its DPI.' }
$width = [int]($LogicalWidth * $scale + $outer.Right - $outer.Left - $client.Right)
$height = [int]($LogicalHeight * $scale + $outer.Bottom - $outer.Top - $client.Bottom)
# SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE
if (-not [OwnedChatWindow]::SetWindowPos($handle, [IntPtr]::Zero, 0, 0, $width, $height, 22)) {
    throw 'The owned test window refused resizing.'
}
$caption = New-Object Text.StringBuilder 256
[OwnedChatWindow]::GetWindowText($handle, $caption, 256) | Out-Null
$after = New-Object OwnedChatWindow+Rect
[OwnedChatWindow]::GetClientRect($handle, [ref]$after) | Out-Null
if ($Capture) {
    # CDP screenshots are cropped by WebView2 when controller zoom is active.
    # Capture only this owned synthetic client window, never the desktop.
    Add-Type -AssemblyName System.Drawing
    $bitmap = [System.Drawing.Bitmap]::new($after.Right, $after.Bottom)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    try {
        if (-not [OwnedChatWindow]::PrintWindow($handle, $dc, 3)) { throw 'The owned window could not be captured.' }
    } finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
    try {
        $capturePath = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../../.local/native-results/chat/native-zoom-window.png'))
        $bitmap.Save($capturePath, [System.Drawing.Imaging.ImageFormat]::Png)
    } finally { $bitmap.Dispose() }
}
@{ ownerPid = $OwnerPid; title = $caption.ToString(); logicalWidth = $LogicalWidth; logicalHeight = $LogicalHeight; scale = $scale; activated = $false; clientWidth = $after.Right; clientHeight = $after.Bottom; requestedOuterWidth = $width; requestedOuterHeight = $height } | ConvertTo-Json -Compress
