param(
    [Parameter(Mandatory)][int]$OwnerPid,
    [Parameter(Mandatory)][ValidateSet('Save', 'Cancel')][string]$Action,
    [Parameter(Mandatory)][string]$TestRoot,
    [string]$Destination = ''
)
$ErrorActionPreference = 'Stop'
$ownedProcess = Get-Process -Id $OwnerPid
if ($ownedProcess.ProcessName -ne 'webnovel-desktop') { throw 'The PID is not the owned native test application.' }
$resolvedRoot = [IO.Path]::GetFullPath($TestRoot).TrimEnd('\') + '\'
if ($Action -eq 'Save') {
    $resolvedDestination = [IO.Path]::GetFullPath($Destination)
    if (-not $resolvedDestination.StartsWith($resolvedRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'The synthetic destination must remain inside the owned test directory.'
    }
    if (Test-Path -LiteralPath $resolvedDestination) { throw 'The synthetic destination already exists.' }
}
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class OwnedSaveDialog {
    [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageTimeoutW")]
    public static extern IntPtr WriteText(IntPtr window, uint message, IntPtr w, string text, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageTimeoutW")]
    public static extern IntPtr ReadText(IntPtr window, uint message, IntPtr w, StringBuilder text, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, StringBuilder text, int capacity);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsChild(IntPtr parent, IntPtr child);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll", EntryPoint="PostMessageW")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
}
"@
function Assert-OwnedControl([IntPtr]$handle, [int]$controlId, [string]$className) {
    $nativePid = [uint32]0
    [OwnedSaveDialog]::GetWindowThreadProcessId($handle, [ref]$nativePid) | Out-Null
    $class = [Text.StringBuilder]::new(128)
    [OwnedSaveDialog]::GetClassName($handle, $class, $class.Capacity) | Out-Null
    if ($handle -eq [IntPtr]::Zero -or $nativePid -ne $OwnerPid -or
        -not [OwnedSaveDialog]::IsChild($dialogHandle, $handle) -or
        [OwnedSaveDialog]::GetDlgCtrlID($handle) -ne $controlId -or $class.ToString() -ne $className) {
        throw "The owned dialog's $className control could not be verified."
    }
}
$processCondition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty, $OwnerPid)
$nameCondition = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, 'Save draft as a new file')
$dialogElement = $null
$deadline = [DateTime]::UtcNow.AddSeconds(15)
while ([DateTime]::UtcNow -lt $deadline -and $null -eq $dialogElement) {
    $windows = [Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Children, $processCondition)
    foreach ($window in $windows) {
        if ($window.Current.Name -eq 'Save draft as a new file') { $dialogElement = $window; break }
        $dialogElement = $window.FindFirst([Windows.Automation.TreeScope]::Descendants, $nameCondition)
        if ($null -ne $dialogElement) { break }
    }
    if ($null -eq $dialogElement) { Start-Sleep -Milliseconds 100 }
}
if ($null -eq $dialogElement -or $dialogElement.Current.ProcessId -ne $OwnerPid) { throw 'The owned Save dialog did not appear.' }
$dialogHandle = [IntPtr]$dialogElement.Current.NativeWindowHandle
if ($dialogHandle -eq [IntPtr]::Zero) { throw 'The owned Save dialog has no native handle.' }
function Find-Control([int]$id, [string]$className) {
    $condition = [Windows.Automation.AndCondition]::new(
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, [string]$id),
        [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ClassNameProperty, $className)
    )
    $control = $dialogElement.FindFirst([Windows.Automation.TreeScope]::Descendants, $condition)
    if ($null -eq $control) { throw "The owned dialog did not expose control $id." }
    Assert-OwnedControl ([IntPtr]$control.Current.NativeWindowHandle) $id $className
    return $control
}
if ($Action -eq 'Save') {
    $field = Find-Control 1001 'Edit'
    $fieldHandle = [IntPtr]$field.Current.NativeWindowHandle
    # Some Windows configurations expose native controls without UIA patterns.
    # WM_SETTEXT/GETTEXT address this verified filename field only; no keystrokes.
    $pattern = $null
    if ($field.TryGetCurrentPattern([Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
        $pattern.SetValue($resolvedDestination)
    } else {
        $result = [IntPtr]::Zero
        $sent = [OwnedSaveDialog]::WriteText($fieldHandle, 0x000C, [IntPtr]::Zero, $resolvedDestination, 2, 1000, [ref]$result)
        if ($sent -eq [IntPtr]::Zero -or $result -eq [IntPtr]::Zero) { throw 'The filename field rejected text input.' }
    }
    $text = [Text.StringBuilder]::new(4096)
    $result = [IntPtr]::Zero
    $sent = [OwnedSaveDialog]::ReadText($fieldHandle, 0x000D, [IntPtr]$text.Capacity, $text, 2, 1000, [ref]$result)
    if ($sent -eq [IntPtr]::Zero -or $text.ToString() -ne $resolvedDestination) { throw 'The filename field does not contain the exact synthetic destination.' }
}
$buttonId = if ($Action -eq 'Save') { 1 } else { 2 }
$button = Find-Control $buttonId 'Button'
if (-not $button.Current.IsEnabled -or $button.Current.IsOffscreen) { throw 'The owned dialog action is unavailable.' }
$pattern = $null
$method = 'UIA InvokePattern'
if ($button.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    $pattern.Invoke()
} else {
    $method = 'owned native BM_CLICK'
    [OwnedSaveDialog]::SetForegroundWindow($dialogHandle) | Out-Null
    # Queue the click: Save can run a modal shell operation, so never block the
    # helper inside synchronous SendMessage. Verify completion below.
    if (-not [OwnedSaveDialog]::PostMessage([IntPtr]$button.Current.NativeWindowHandle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)) {
        throw 'The owned dialog rejected its button action.'
    }
}
$deadline = [DateTime]::UtcNow.AddSeconds(10)
while ([OwnedSaveDialog]::IsWindow($dialogHandle) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
if ([OwnedSaveDialog]::IsWindow($dialogHandle)) { throw "The owned Save dialog did not close after $Action ($method)." }
if ($Action -eq 'Save') {
    while (-not (Test-Path -LiteralPath $resolvedDestination) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
    if (-not (Test-Path -LiteralPath $resolvedDestination)) { throw 'The requested export file was not created.' }
}
@{ action=$Action; ownedDialog=$true; filenameSet=($Action -eq 'Save'); method=$method } | ConvertTo-Json -Compress
