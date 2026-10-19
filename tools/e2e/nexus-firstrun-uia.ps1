# NEXUS — تحقق حقيقي من التهيئة الأولى ودخول أول مسؤول داخل نافذة WebView2 الفعلية.
#
# لماذا UIA لا CDP: بوابة CDP في WebView2 مثبتة الفشل في هذه البيئة
# (`tauri.e2e.conf.json` + `nexus-e2e.mjs`). هذا المسار يستخدم شجرة إمكانية
# الوصول (UI Automation) التي يبنيها Chromium عند تشغيل
# `--force-renderer-accessibility`، فيتعامل مع العناصر الحقيقية في الـDOM
# ويستدعي أوامر Rust الحقيقية — لا جسر مزيّف ولا بيانات وهمية.
#
# البناء (مرة واحدة، معرّف معزول فلا يمسّ قاعدة المستخدم الحيّة):
#   cd apps\desktop
#   $env:CARGO_TARGET_DIR="$env:LOCALAPPDATA\NEXUS\cargo-target"
#   npx tauri build --debug --no-bundle --config <REPO_ROOT>\tools\e2e\firstrun-harness.conf.json
#
# التشغيل:
#   powershell -File tools/e2e/nexus-firstrun-uia.ps1            # إقلاع + تحقق + دخول
#   powershell -File tools/e2e/nexus-firstrun-uia.ps1 -Phase verify   # فحص قاعدة الحاوية فقط
#
# قاعدة الحاوية: %LOCALAPPDATA%\com.nexus.obligationos.firstrunharness
# (معرّف مستقل — قاعدة المستخدم الحقيقية لا تُلمس إطلاقاً)

param(
  [ValidateSet('all', 'bootstrap', 'restart-login', 'verify')]
  [string]$Phase = 'all',
  [string]$OrganizationName = 'First Run Harness Organization',
  [string]$AdminName = 'First Run Administrator',
  [string]$AdminEmail = 'firstrun.admin@example.test',
  [string]$AdminPassword = 'Str0ng!Passw0rd#2026',
  [string]$CountryCode = 'QA',
  [string]$Industry = 'Financial Services',
  [string]$WindowTitle = 'NEXUS — FIRST RUN HARNESS (ISOLATED)',
  [string]$Exe = "$env:LOCALAPPDATA\NEXUS\cargo-target\debug\nexus_desktop.exe",
  [switch]$KeepRunning
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes

$AppDataDirectory = Join-Path $env:LOCALAPPDATA 'com.nexus.obligationos.firstrunharness'
$DatabasePath = Join-Path $AppDataDirectory 'nexus-local.db'

function Stop-HarnessApp {
  Get-Process -Name nexus_desktop -ErrorAction SilentlyContinue | Stop-Process -Force
  Start-Sleep -Seconds 2
}

function Start-HarnessApp {
  if (-not (Test-Path $Exe)) {
    throw "الثنائي غير موجود: $Exe — ابنِه بأمر البناء الموثّق في رأس الملف."
  }
  $process = Start-Process -FilePath $Exe -PassThru
  Start-Sleep -Seconds 9
  if ($process.HasExited) { throw "انتهى التطبيق فوراً برمز $($process.ExitCode)" }
  Write-Output "APP_STARTED pid=$($process.Id)"
}

function Get-HarnessWindow {
  $deadline = (Get-Date).AddSeconds(20)
  while ((Get-Date) -lt $deadline) {
    $window = [System.Windows.Automation.AutomationElement]::RootElement.FindFirst(
      [System.Windows.Automation.TreeScope]::Descendants,
      (New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::NameProperty, $WindowTitle)))
    if ($window) { return $window }
    Start-Sleep -Milliseconds 500
  }
  throw "لم تُعثر على النافذة: $WindowTitle"
}

function Wait-ForElement($window, [string]$id) {
  $deadline = (Get-Date).AddSeconds(20)
  while ((Get-Date) -lt $deadline) {
    $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
      (New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::AutomationIdProperty, $id)))
    if ($element) { return $element }
    Start-Sleep -Milliseconds 400
  }
  throw "لم يظهر العنصر: $id"
}

function Find-ById($window, [string]$id) {
  $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::AutomationIdProperty, $id)))
  if (-not $element) { throw "لم يُعثر على العنصر: $id" }
  return $element
}

function Set-Value($element, [string]$value) {
  $pattern = $null
  if (-not $element.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
    throw 'العنصر لا يدعم ValuePattern'
  }
  $pattern.SetValue($value)
}

function Get-Value($element) {
  $pattern = $null
  if (-not $element.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
    return ''
  }
  return $pattern.Current.Value
}

function Find-PrimaryButton($window) {
  $button = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.AndCondition(
      (New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Button)),
      (New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ClassNameProperty,
        'button button--primary')))))
  if (-not $button) { throw 'لم يُعثر على الزر الأساسي' }
  return $button
}

function Invoke-Element($element) {
  $pattern = $null
  if (-not $element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$pattern)) {
    throw 'العنصر لا يدعم InvokePattern'
  }
  $pattern.Invoke()
}

function Select-Country($window, [string]$code) {
  $combobox = Find-ById $window 'setup-country-code'
  $expand = $null
  if (-not $combobox.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref]$expand)) {
    throw 'قائمة الدول لا تدعم ExpandCollapsePattern'
  }
  $expand.Expand()
  Start-Sleep -Milliseconds 1200
  # القائمة المنبثقة نافذة مستقلة: تُبحث عناصرها على مستوى سطح المكتب.
  $items = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
    [System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
      [System.Windows.Automation.ControlType]::ListItem)))
  $suffix = "($code)"
  foreach ($item in $items) {
    $name = $item.Current.Name -replace '\s+', ' '
    if (-not $name.EndsWith($suffix)) { continue }
    $selection = $null
    if ($item.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$selection)) {
      $selection.Select()
    } elseif ($item.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$selection)) {
      $selection.Invoke()
    }
    Start-Sleep -Milliseconds 500
    Write-Output "COUNTRY_SELECTED=$name"
    return
  }
  throw "لم يُعثر على الدولة في القائمة: $code"
}

function Write-Texts($window, [int]$limit = 15) {
  $texts = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
      [System.Windows.Automation.ControlType]::Text)))
  $values = $texts | ForEach-Object { ($_.Current.Name -replace '\s+', ' ') } |
    Where-Object { $_.Length -gt 0 } | Select-Object -First $limit
  $values | ForEach-Object { Write-Output "TEXT=$_" }
}

function Invoke-BootstrapPhase {
  Start-HarnessApp
  $window = Get-HarnessWindow
  $null = Wait-ForElement $window 'setup-country-code'
  Write-Output "INITIAL_COUNTRY='$((Get-Value (Find-ById $window 'setup-country-code')))'"
  Write-Output "INITIAL_INDUSTRY='$((Get-Value (Find-ById $window 'setup-industry')))'"
  Select-Country $window $CountryCode
  Set-Value (Find-ById $window 'setup-organization-name') $OrganizationName
  Set-Value (Find-ById $window 'setup-admin-display-name') $AdminName
  Set-Value (Find-ById $window 'setup-industry') $Industry
  Set-Value (Find-ById $window 'setup-admin-email') $AdminEmail
  Set-Value (Find-ById $window 'setup-password') $AdminPassword
  Write-Output "COUNTRY_FINAL='$((Get-Value (Find-ById $window 'setup-country-code')))'"
  Write-Output "INDUSTRY_FINAL='$((Get-Value (Find-ById $window 'setup-industry')))'"
  Invoke-Element (Find-PrimaryButton $window)
  Write-Output 'BOOTSTRAP_SUBMITTED=true'
  Start-Sleep -Seconds 6
  Write-Texts $window
  $formErrors = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::ClassNameProperty, 'form-error')))
  if ($formErrors.Count -gt 0) { throw "ظهر خطأ نموذج بعد الإقلاع: $($formErrors[0].Current.Name)" }
  Write-Output 'PRODUCTION_SHELL=true'
}

function Invoke-LoginPhase {
  Start-HarnessApp
  $window = Get-HarnessWindow
  $null = Wait-ForElement $window 'login-email'
  Set-Value (Find-ById $window 'login-email') $AdminEmail
  Set-Value (Find-ById $window 'login-password') $AdminPassword
  Invoke-Element (Find-PrimaryButton $window)
  Write-Output 'LOGIN_SUBMITTED=true'
  Start-Sleep -Seconds 5
  Write-Texts $window
  $loginForm = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,
    (New-Object System.Windows.Automation.PropertyCondition(
      [System.Windows.Automation.AutomationElement]::AutomationIdProperty, 'login-email')))
  if ($loginForm) { throw 'شاشة الدخول ما زالت ظاهرة بعد إرسال بيانات صحيحة' }
  Write-Output 'FIRST_ADMIN_LOGGED_IN=true'
}

function Invoke-VerifyPhase {
  $sqlite = Get-Command sqlite3 -ErrorAction SilentlyContinue
  if (-not $sqlite) {
    Write-Output "DB_PATH=$DatabasePath"
    Write-Output 'SQLITE3_NOT_FOUND=verify manually with any read-only sqlite client'
    return
  }
  Write-Output "DB_PATH=$DatabasePath"
  $checks = [ordered]@{
    integrity = "PRAGMA integrity_check;"
    foreign_keys = "SELECT COUNT(*) FROM pragma_foreign_key_check;"
    schema = "SELECT COALESCE(MAX(version), 0) FROM schema_migrations;"
    mode = "SELECT mode FROM app_runtime_state WHERE id = 1;"
    organizations = "SELECT country_code || '|' || industry FROM organizations;"
    tenants = "SELECT COUNT(*) || '|' || MIN(data_mode) FROM tenants;"
    admins = "SELECT COUNT(*) FROM local_users WHERE status = 'ACTIVE';"
    audit_events = "SELECT COUNT(*) FROM audit_events;"
    audit_chain_broken = "SELECT COUNT(*) FROM audit_events e WHERE e.previous_hash <> COALESCE((SELECT p.event_hash FROM audit_events p WHERE p.tenant_id = e.tenant_id AND p.rowid < e.rowid ORDER BY p.rowid DESC LIMIT 1), '');"
  }
  foreach ($key in $checks.Keys) {
    $value = & $sqlite.Source -readonly $DatabasePath $checks[$key] 2>&1
    Write-Output "DB_$key=$value"
  }
  $chainBroken = & $sqlite.Source -readonly $DatabasePath $checks['audit_chain_broken']
  if ([int]$chainBroken -ne 0) { throw 'سلسلة التدقيق مكسورة في قاعدة الحاوية' }
  Write-Output 'AUDIT_CHAIN_LINKED=true'
}

if ($Phase -in @('all', 'bootstrap', 'restart-login')) {
  Stop-HarnessApp
  # بيئة نظيفة: الحاوية التجريبية فقط، ومعرّف مستقل لا يمسّ قاعدة المستخدم.
  if (Test-Path $AppDataDirectory) {
    $resolved = (Resolve-Path $AppDataDirectory).Path
    if ($resolved -ne $AppDataDirectory) { throw "مسار الحاوية غير مطابق: $resolved" }
    Remove-Item -LiteralPath $resolved -Recurse -Force
    Write-Output 'HARNESS_APPDATA_RESET=true'
  }
}

switch ($Phase) {
  'bootstrap' {
    Invoke-BootstrapPhase
  }
  'restart-login' {
    Stop-HarnessApp
    Invoke-LoginPhase
  }
  'verify' {
    Invoke-VerifyPhase
  }
  'all' {
    Invoke-BootstrapPhase
    Stop-HarnessApp
    Invoke-LoginPhase
    Invoke-VerifyPhase
  }
}

if (-not $KeepRunning) { Stop-HarnessApp }
Write-Output 'HARNESS_DONE=true'

