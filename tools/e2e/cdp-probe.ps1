<#
اختبار قابلية التحكم بمنفذ تصحيح WebView2 في نسخة الاختبار.
الغرض: إثبات أن WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS يفتح المنفذ فعلاً، قبل بناء أي قياس E2E عليه.
#>
param(
  [string]$Exe = "",
  [int]$Port = 9222,
  [int]$WaitSeconds = 45
)

$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
if (-not $Exe) {
  $Exe = Join-Path $repoRoot '.tmp-build\e2e-app\apps\desktop\src-tauri\target\debug\nexus_desktop.exe'
}

function Get-TestWebViewProcs {
  Get-CimInstance Win32_Process -Filter "Name='msedgewebview2.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -and $_.CommandLine -match 'e2e-app|e2e-webview-profile' }
}
function Stop-TestApp {
  Get-Process -Name nexus_desktop -ErrorAction SilentlyContinue | ForEach-Object {
    if ($_.Path -eq $Exe) { Stop-Process -Id $_.Id -Force -ErrorAction SilentlyContinue }
  }
  Get-TestWebViewProcs | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  Start-Sleep -Seconds 3
}

Stop-TestApp
Write-Output "cleanup done; remaining test webviews: $((Get-TestWebViewProcs | Measure-Object).Count)"

$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
$logDir = Join-Path $repoRoot '.tmp-build\e2e'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null

$proc = Start-Process -FilePath $Exe -PassThru `
  -RedirectStandardError "$logDir\cdp-probe-err.log" `
  -RedirectStandardOutput "$logDir\cdp-probe-out.log"

$deadline = (Get-Date).AddSeconds($WaitSeconds)
$opened = $false
while ((Get-Date) -lt $deadline) {
  Start-Sleep -Milliseconds 700
  if ($proc.HasExited) { break }
  try {
    $r = Invoke-WebRequest -Uri "http://127.0.0.1:$Port/json/version" -TimeoutSec 3 -UseBasicParsing -ErrorAction Stop
    if ($r.StatusCode -eq 200) {
      Write-Output "CDP_OPENED: $($r.Content -replace '\s+',' ')"
      $opened = $true
      break
    }
  } catch { }
}

Write-Output "HasExited=$($proc.HasExited)  CDP_OPENED=$opened"
if (-not $proc.HasExited) {
  $wv = Get-TestWebViewProcs
  Write-Output "webview children: $($wv.Count)"
  $wv | Select-Object -First 2 | ForEach-Object {
    $c = $_.CommandLine
    $hasPort = $c -match 'remote-debugging-port'
    Write-Output "  pid=$($_.ProcessId) hasRemoteDebuggingArg=$hasPort"
  }
}
Write-Output "--- stderr (first 6) ---"
Get-Content "$logDir\cdp-probe-err.log" -ErrorAction SilentlyContinue | Select-Object -First 6

Stop-TestApp
Write-Output "final cleanup done"
