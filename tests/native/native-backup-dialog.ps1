param(
    [Parameter(Mandatory)][int]$OwnerPid,
    [Parameter(Mandatory)][ValidateSet('Save', 'Open')][string]$Action,
    [Parameter(Mandatory)][string]$TestRoot,
    [Parameter(Mandatory)][string]$Destination,
    [ValidateSet('Save project backup', 'Recover a backup as a new project')]
    [string]$DialogTitle = 'Save project backup'
)

$ErrorActionPreference = 'Stop'
$ownedProcess = Get-Process -Id $OwnerPid -ErrorAction Stop
if ($ownedProcess.ProcessName -ne 'webnovel-desktop') { throw 'The PID is not the owned native test application.' }
$resolvedRoot = [IO.Path]::GetFullPath($TestRoot).TrimEnd('\') + '\'
$resolvedDestination = [IO.Path]::GetFullPath($Destination)
if (-not $resolvedDestination.StartsWith($resolvedRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'The synthetic backup path must remain inside the owned test directory.'
}
if ($Action -eq 'Save' -and (Test-Path -LiteralPath $resolvedDestination)) {
    throw 'The synthetic backup destination already exists.'
}
if ($Action -eq 'Open' -and -not (Test-Path -LiteralPath $resolvedDestination -PathType Leaf)) {
    throw 'The synthetic backup source does not exist.'
}

Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class OwnedBackupDialog {
    [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageTimeoutW")]
    public static extern IntPtr WriteText(IntPtr window, uint message, IntPtr w, string text, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageTimeoutW")]
    public static extern IntPtr SendScalar(IntPtr window, uint message, IntPtr w, IntPtr l, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll", CharSet=CharSet.Unicode, EntryPoint="SendMessageTimeoutW")]
    public static extern IntPtr ReadText(IntPtr window, uint message, IntPtr w, StringBuilder text, uint flags, uint timeout, out IntPtr result);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr window, StringBuilder text, int capacity);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsChild(IntPtr parent, IntPtr child);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindowEnabled(IntPtr window);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll", EntryPoint="PostMessageW")] public static extern bool PostMessage(IntPtr window, uint message, IntPtr w, IntPtr l);
}
"@

function Assert-OwnedControl([IntPtr]$handle, [int]$controlId, [string]$className) {
    $nativePid = [uint32]0
    [OwnedBackupDialog]::GetWindowThreadProcessId($handle, [ref]$nativePid) | Out-Null
    $class = [Text.StringBuilder]::new(128)
    [OwnedBackupDialog]::GetClassName($handle, $class, $class.Capacity) | Out-Null
    if ($handle -eq [IntPtr]::Zero -or $nativePid -ne $OwnerPid -or
        -not [OwnedBackupDialog]::IsChild($dialogHandle, $handle) -or
        [OwnedBackupDialog]::GetDlgCtrlID($handle) -ne $controlId -or $class.ToString() -ne $className) {
        throw "The owned dialog's $className control could not be verified."
    }
}

$processCondition = [Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::ProcessIdProperty, $OwnerPid)
$nameCondition = [Windows.Automation.PropertyCondition]::new(
    [Windows.Automation.AutomationElement]::NameProperty, $DialogTitle)
$dialogElement = $null
$deadline = [DateTime]::UtcNow.AddSeconds(20)
while ([DateTime]::UtcNow -lt $deadline -and $null -eq $dialogElement) {
    $windows = [Windows.Automation.AutomationElement]::RootElement.FindAll(
        [Windows.Automation.TreeScope]::Children, $processCondition)
    foreach ($window in $windows) {
        if ($window.Current.Name -eq $DialogTitle) { $dialogElement = $window; break }
        $dialogElement = $window.FindFirst([Windows.Automation.TreeScope]::Descendants, $nameCondition)
        if ($null -ne $dialogElement) { break }
    }
    if ($null -eq $dialogElement) { Start-Sleep -Milliseconds 100 }
}
if ($null -eq $dialogElement -or $dialogElement.Current.ProcessId -ne $OwnerPid) {
    throw 'The owned native backup dialog did not appear.'
}
$dialogHandle = [IntPtr]$dialogElement.Current.NativeWindowHandle
if ($dialogHandle -eq [IntPtr]::Zero) { throw 'The owned backup dialog has no native handle.' }

function Find-FileNameControl {
    $editClass = [Windows.Automation.PropertyCondition]::new(
        [Windows.Automation.AutomationElement]::ClassNameProperty, 'Edit')
    $candidates = @($dialogElement.FindAll([Windows.Automation.TreeScope]::Descendants, $editClass) | Where-Object {
        $handle = [IntPtr]$_.Current.NativeWindowHandle
        $id = [int]$_.Current.AutomationId
        -not $_.Current.IsOffscreen -and [OwnedBackupDialog]::IsWindowVisible($handle) -and
        [OwnedBackupDialog]::IsWindowEnabled($handle) -and ($id -eq 1001 -or $id -eq 1148)
    })
    if ($candidates.Count -ne 1) {
        throw "The owned dialog exposed $($candidates.Count) visible filename controls; expected one."
    }
    $control = $candidates[0]
    Assert-OwnedControl ([IntPtr]$control.Current.NativeWindowHandle) ([int]$control.Current.AutomationId) 'Edit'
    return $control
}

$field = Find-FileNameControl
$fieldHandle = [IntPtr]$field.Current.NativeWindowHandle
if (-not [OwnedBackupDialog]::PostMessage($dialogHandle, 0x0028, $fieldHandle, [IntPtr]1)) {
    throw 'The owned dialog rejected its filename focus request.'
}
Start-Sleep -Milliseconds 100
Assert-OwnedControl $fieldHandle ([int]$field.Current.AutomationId) 'Edit'
$result = [IntPtr]::Zero
$sent = [OwnedBackupDialog]::SendScalar($fieldHandle, 0x00B1, [IntPtr]::Zero, [IntPtr](-1), 2, 1000, [ref]$result)
if ($sent -eq [IntPtr]::Zero) { throw 'The owned filename selection was not accepted.' }
$sent = [OwnedBackupDialog]::WriteText($fieldHandle, 0x00C2, [IntPtr]1, $resolvedDestination, 2, 1000, [ref]$result)
if ($sent -eq [IntPtr]::Zero) { throw 'The owned filename replacement was not accepted.' }
$text = [Text.StringBuilder]::new(4096)
$sent = [OwnedBackupDialog]::ReadText($fieldHandle, 0x000D, [IntPtr]$text.Capacity, $text, 2, 1000, [ref]$result)
if ($sent -eq [IntPtr]::Zero -or $text.ToString() -ne $resolvedDestination) {
    throw 'The filename field does not contain the exact synthetic backup path.'
}

$buttonCondition = [Windows.Automation.AndCondition]::new(
    [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty, '1'),
    [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ClassNameProperty, 'Button'))
$buttons = @($dialogElement.FindAll([Windows.Automation.TreeScope]::Descendants, $buttonCondition) | Where-Object {
    $handle = [IntPtr]$_.Current.NativeWindowHandle
    -not $_.Current.IsOffscreen -and [OwnedBackupDialog]::IsWindowVisible($handle) -and [OwnedBackupDialog]::IsWindowEnabled($handle)
})
if ($buttons.Count -ne 1) { throw "The owned dialog exposed $($buttons.Count) visible action buttons; expected one." }
$button = $buttons[0]
Assert-OwnedControl ([IntPtr]$button.Current.NativeWindowHandle) 1 'Button'
$pattern = $null
$method = 'UIA InvokePattern'
if ($button.TryGetCurrentPattern([Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    $pattern.Invoke()
} else {
    $method = 'owned native BM_CLICK'
    [OwnedBackupDialog]::SetForegroundWindow($dialogHandle) | Out-Null
    if (-not [OwnedBackupDialog]::PostMessage([IntPtr]$button.Current.NativeWindowHandle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero)) {
        throw 'The owned dialog rejected its action.'
    }
}
$deadline = [DateTime]::UtcNow.AddSeconds(15)
while ([OwnedBackupDialog]::IsWindow($dialogHandle) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
if ([OwnedBackupDialog]::IsWindow($dialogHandle)) { throw "The owned backup dialog did not close after $Action ($method)." }
if ($Action -eq 'Save') {
    while (-not (Test-Path -LiteralPath $resolvedDestination) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 100 }
    if (-not (Test-Path -LiteralPath $resolvedDestination -PathType Leaf)) { throw 'The requested backup archive was not created.' }
}
@{ action=$Action; ownedDialog=$true; filenameSet=$true; method=$method; destination=$resolvedDestination } | ConvertTo-Json -Compress
