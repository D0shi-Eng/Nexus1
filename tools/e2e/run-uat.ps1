# NEXUS V1 — تشغيل جولة قبول سطح المكتب الحقيقية على حاوية بيانات معزولة.
#
# كل مرحلة تعمل على حاوية AppData مستقلة تماماً:
#   %LOCALAPPDATA%\com.nexus.obligationos.uatharness
# وهي ليست قاعدة المستخدم الحيّة (%LOCALAPPDATA%\com.nexus.obligationos).
#
# بناء الثنائي المُقاس (مرة واحدة، لا يمسّ بيان الإنتاج):
#   cd apps\desktop
#   $env:CARGO_TARGET_DIR="$env:LOCALAPPDATA\NEXUS\cargo-target"
#   npx tauri build --debug --no-bundle --config <REPO_ROOT>\tools\e2e\uat-harness.conf.json
#
# الاستخدام:
#   powershell -File tools/e2e/run-uat.ps1 -Phase production -Reset
#   powershell -File tools/e2e/run-uat.ps1 -Phase demo -Reset
#   node tools/e2e/nexus-uat-ui.mjs <exe> --evidence=<dir> --port=9444   # بعد مرحلة demo

param(
  [ValidateSet('production', 'demo')]
  [string]$Phase = 'demo',
  [switch]$Reset,
  [int]$Port = 9444,
  [string]$Exe = "$env:LOCALAPPDATA\NEXUS\cargo-target\debug\nexus_desktop.exe"
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$evidence = Join-Path $repoRoot ".tmp-uat\evidence-$Phase"
$container = Join-Path $env:LOCALAPPDATA 'com.nexus.obligationos.uatharness'
$liveContainer = Join-Path $env:LOCALAPPDATA 'com.nexus.obligationos'

# حماية صريحة: لا نلمس قاعدة المستخدم الحيّة تحت أي ظرف.
if ($container -eq $liveContainer) { throw 'رفض: مسار الحاوية يطابق قاعدة المستخدم الحيّة.' }
if (-not $container.StartsWith($env:LOCALAPPDATA)) { throw "مسار الحاوية خارج LOCALAPPDATA: $container" }

Get-Process -Name nexus_desktop -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Milliseconds 800

if ($Reset -and (Test-Path $container)) {
  $resolved = (Resolve-Path -LiteralPath $container).Path
  if ($resolved -ne $container) { throw "مسار الحاوية غير مطابق بعد الحل: $resolved" }
  Remove-Item -LiteralPath $resolved -Recurse -Force
  Write-Output "CONTAINER_RESET=true path=$resolved"
}

if (-not (Test-Path $Exe)) { throw "الثنائي غير موجود: $Exe" }
New-Item -ItemType Directory -Force -Path $evidence | Out-Null

Write-Output "PHASE=$Phase"
Write-Output "EXE=$Exe"
Write-Output "APP_DATA=$container"
Write-Output "EVIDENCE=$evidence"

& node (Join-Path $repoRoot 'tools\e2e\nexus-uat.mjs') `
  $Exe `
  "--phase=$Phase" `
  "--evidence=$evidence" `
  "--port=$Port" `
  "--appdata=$container"
$code = $LASTEXITCODE

Get-Process -Name nexus_desktop -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Output "UAT_EXIT_CODE=$code"
exit $code
