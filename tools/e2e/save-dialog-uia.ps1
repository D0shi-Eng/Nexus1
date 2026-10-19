# NEXUS V1 — UAT-06: التعامل مع حوار الحفظ الأصلي في Windows عبر UI Automation.
#
# الغاية: إثبات أن حوار الحفظ *الفعلي* الذي يفتحه Tauri يعمل: كتابة مسار وجهة،
# حفظ الملف، أو إلغاء الحوار. لا محاكاة ولا استدعاء مباشر لأمر الخلف.
#
# لماذا Win32 EnumWindows + UIA FromHandle: حوار الحفظ الحديث (IFileSaveDialog)
# قد لا يظهر كابن مباشر لسطح المكتب في شجرة UIA قبل تحميل مزوّده، فالتعداد
# النوافذ عبر Win32 ثم ربط المقبض بشجرة UIA هو المسار الموثوق.
#
# المخرجات: سطر واحد ثابت يُقرأ آلياً (UIA_*) + exit code.
param(
  [Parameter(Mandatory = $true)]
  [ValidateSet('save', 'cancel', 'list')]
  [string]$Action,
  [string]$TargetPath,
  [int]$TimeoutSeconds = 30,
  [int]$ProcessId = 0,
  [string]$DiagnosticsPath
)

$ErrorActionPreference = 'Stop'

try {
  Add-Type -AssemblyName UIAutomationClient
  Add-Type -AssemblyName UIAutomationTypes
} catch {
  Write-Output 'UIA_ASSEMBLY_UNAVAILABLE'
  exit 2
}

if (-not ('NexusWin32' -as [type])) {
  Add-Type -Namespace '' -Name 'NexusWin32' -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);
public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr hWnd, System.Text.StringBuilder lpClassName, int nMaxCount);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, System.Text.StringBuilder lpString, int nMaxCount);
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hWnd);
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
'@
}

$AutomationElement = [System.Windows.Automation.AutomationElement]
$TreeScope = [System.Windows.Automation.TreeScope]
$ControlType = [System.Windows.Automation.ControlType]
$ValuePattern = [System.Windows.Automation.ValuePattern]
$InvokePattern = [System.Windows.Automation.InvokePattern]

function Get-VisibleWindows {
  $script:collected = New-Object System.Collections.ArrayList
  $callback = [NexusWin32+EnumWindowsProc] {
    param([IntPtr]$handle, [IntPtr]$lparam)
    try {
      if ([NexusWin32]::IsWindowVisible($handle)) {
        $processId = 0
        [void][NexusWin32]::GetWindowThreadProcessId($handle, [ref]$processId)
        $className = New-Object System.Text.StringBuilder 256
        [void][NexusWin32]::GetClassName($handle, $className, 256)
        $title = New-Object System.Text.StringBuilder 512
        [void][NexusWin32]::GetWindowText($handle, $title, 512)
        [void]$script:collected.Add([pscustomobject]@{
            Handle = $handle
            ProcessId = $processId
            ClassName = $className.ToString()
            Title = $title.ToString()
          })
      }
    } catch { }
    return $true
  }
  [void][NexusWin32]::EnumWindows($callback, [IntPtr]::Zero)
  return $script:collected
}

# اختيار نافذة الحوار: نافذة مربّع حوار قياسية (#32770) تابعة لعملية التطبيق أو
# عنوانها يدل على الحفظ. لا نختار النافذة الرئيسية (#Chrome_WidgetWin_0...).
function Select-DialogWindow {
  param($Windows)
  $candidates = @()
  foreach ($window in $Windows) {
    if ($ProcessId -ne 0 -and $window.ProcessId -ne $ProcessId) { continue }
    if ($window.ClassName -eq '#32770') { $candidates += $window; continue }
    if ($window.Title -match '^(Save As|حفظ باسم|Save|حفظ)$') { $candidates += $window }
  }
  return $candidates | Select-Object -First 1
}

$diagnostics = New-Object System.Collections.ArrayList
$dialogWindow = $null
$deadline = (Get-Date).AddSeconds($TimeoutSeconds)
while (-not $dialogWindow -and (Get-Date) -lt $deadline) {
  $windows = Get-VisibleWindows
  $dialogWindow = Select-DialogWindow -Windows $windows
  if (-not $dialogWindow) {
    if ($DiagnosticsPath -and (Get-Date) -lt $deadline) {
      [void]$diagnostics.Add(($windows | Where-Object { $ProcessId -eq 0 -or $_.ProcessId -eq $ProcessId } | ForEach-Object { "$($_.ProcessId)|$($_.ClassName)|$($_.Title)" }))
    }
    Start-Sleep -Milliseconds 300
  }
}

if ($DiagnosticsPath) {
  try { ($diagnostics | Select-Object -Last 5) | Set-Content -Path $DiagnosticsPath -Encoding utf8 } catch { }
}

if (-not $dialogWindow) {
  Write-Output 'UIA_DIALOG_NOT_FOUND'
  exit 3
}

try {
  $dialog = $AutomationElement::FromHandle($dialogWindow.Handle)
} catch {
  Write-Output 'UIA_ELEMENT_FROM_HANDLE_FAILED'
  exit 7
}

Write-Output ("UIA_DIALOG_FOUND title={0} class={1}" -f $dialogWindow.Title, $dialogWindow.ClassName)

function Invoke-ButtonByName {
  param($Dialog, [string[]]$Names)
  try {
    $buttonCondition = New-Object System.Windows.Automation.PropertyCondition($AutomationElement::ControlTypeProperty, $ControlType::Button)
    $buttons = $Dialog.FindAll($TreeScope::Descendants, $buttonCondition)
  } catch { return $false }
  foreach ($button in $buttons) {
    try {
      $label = $button.Current.Name
      if ($Names -contains $label) {
        $pattern = $button.GetCurrentPattern($InvokePattern::Pattern)
        if ($pattern) { $pattern.Invoke(); return $true }
      }
    } catch { continue }
  }
  return $false
}

if ($Action -eq 'list') {
  Write-Output 'UIA_LIST_OK'
  exit 0
}

if ($Action -eq 'cancel') {
  $cancelled = Invoke-ButtonByName -Dialog $dialog -Names @('Cancel', 'إلغاء', '&Cancel')
  if (-not $cancelled) {
    try {
      [void][NexusWin32]::SetForegroundWindow($dialogWindow.Handle)
      Start-Sleep -Milliseconds 150
      Add-Type -AssemblyName System.Windows.Forms
      [System.Windows.Forms.SendKeys]::SendWait('{ESC}')
      $cancelled = $true
    } catch { }
  }
  if ($cancelled) { Write-Output 'UIA_CANCELLED'; exit 0 }
  Write-Output 'UIA_CANCEL_FAILED'
  exit 4
}

# إيجاد حقل اسم الملف: الحقل الافتراضي يحمل امتداد التقرير (csv/md/markdown).
$filenameSet = $false
try {
  $editCondition = New-Object System.Windows.Automation.PropertyCondition($AutomationElement::ControlTypeProperty, $ControlType::Edit)
  $edits = $dialog.FindAll($TreeScope::Descendants, $editCondition)
} catch { $edits = @() }
foreach ($edit in $edits) {
  try {
    $valuePattern = $edit.GetCurrentPattern($ValuePattern::Pattern)
    if (-not $valuePattern) { continue }
    $current = $valuePattern.Current.Value
    if ($current -match '\.(csv|md|markdown)$') {
      $valuePattern.SetValue($TargetPath)
      $filenameSet = $true
      break
    }
  } catch { continue }
}

if (-not $filenameSet) {
  # بديل واقعي: الحقل قد لا يوفّر ValuePattern في الحوار الحديث، فيُكتب المسار
  # بلوحة المفاتيح كما يفعل المستخدم (Ctrl+A ثم المسار ثم Enter). المسار المختار
  # لا يحوي محارف خاصة بـSendKeys.
  try {
    [void][NexusWin32]::SetForegroundWindow($dialogWindow.Handle)
    Start-Sleep -Milliseconds 500
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.SendKeys]::SendWait('^a')
    Start-Sleep -Milliseconds 200
    [System.Windows.Forms.SendKeys]::SendWait($TargetPath)
    Start-Sleep -Milliseconds 400
    [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
    Write-Output 'UIA_SAVE_KEYSTROKES_SENT'
    exit 0
  } catch { }
  Write-Output 'UIA_FILENAME_FIELD_NOT_FOUND'
  exit 5
}
Write-Output ("UIA_FILENAME_SET path={0}" -f $TargetPath)

Start-Sleep -Milliseconds 250
$saved = Invoke-ButtonByName -Dialog $dialog -Names @('Save', 'حفظ', '&Save', 'Save As', 'حفظ باسم')
if (-not $saved) {
  try {
    [void][NexusWin32]::SetForegroundWindow($dialogWindow.Handle)
    Start-Sleep -Milliseconds 150
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
    $saved = $true
  } catch { }
}
if ($saved) { Write-Output 'UIA_SAVE_INVOKED'; exit 0 }
Write-Output 'UIA_SAVE_FAILED'
exit 6
