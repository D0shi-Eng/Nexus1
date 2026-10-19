# NEXUS V1 — بناء حزمة الإصدار (Release Candidate) بأمر قابل لإعادة الإنتاج.
#
# الغاية الإضافية الإلزامية: منع تسريب مسارات جهاز الباني (اسم مستخدم Windows،
# مجلد الملف الشخصي) داخل الثنائي والمثبّت. Rust يُضمّن مسارات المصدر المطلقة في
# رسائل panic، فتظهر `C:\Users\<name>\.cargo\...` داخل الملف التنفيذي القابل
# للتوزيع. الإصلاح: --remap-path-prefix يربط بادئة الملف الشخصي بقيمة عامة.
#
# لا يُكتب أي مسار شخصي ثابت في المستودع: البادئة تُشتق وقت البناء من USERPROFILE.
#
# الاستخدام:
#   pwsh -File apps/desktop/src-tauri/build-release.ps1
param(
  [string]$Placeholder = '<USER_PROFILE>'
)

$ErrorActionPreference = 'Stop'
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot '..\..\..')
Set-Location $repoRoot

$profileRoot = $env:USERPROFILE
if (-not $profileRoot) { throw 'USERPROFILE غير متاح — لا يمكن تعقيم مسارات البناء.' }
# مسارات المصدر تظهر غالباً بالشكل الأمامي أو العكسي؛ نربط الاثنين احتياطاً.
$remaps = @(
  "--remap-path-prefix=$profileRoot=$Placeholder",
  "--remap-path-prefix=$($profileRoot -replace '\\','/')=$Placeholder"
)
$env:RUSTFLAGS = ($remaps -join ' ')
Write-Host "RUSTFLAGS=$env:RUSTFLAGS"

Set-Location (Join-Path $repoRoot 'apps\desktop')
npx tauri build
exit $LASTEXITCODE
