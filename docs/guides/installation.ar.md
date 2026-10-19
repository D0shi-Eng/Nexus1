# دليل التثبيت والبناء

**ينطبق على:** NEXUS V1 `0.1.0` · Windows 11 x64
**الحالة:** توزيع المصدر فقط. **لا يوجد مثبّت منشور ولا إصدار ثنائي رسمي بعد.**

يفصل هذا الدليل بين أربعة أمور يخلط بينها كثيرون:

| المصطلح | الحالة في V1 |
|---|---|
| **تنزيل المصدر** | متاح: استنساخ مستودع Git. |
| **البناء من المصدر** | متاح: موثّق ومُتحقق منه في هذا المستودع. |
| **التثبيت المحلي** | متاح فقط عبر حزمة NSIS غير الموقّعة التي تبنيها بنفسك. |
| **إصدار ثنائي رسمي** | **غير متاح.** لا مثبّت موقّع، ولا أثر GitHub Release، ولا صفحة تنزيل. |

> المشروع لا يوزّع ملفات تنفيذية غير موقّعة. وإذا بنيت مثبّتاً بنفسك فهو أثر محلي لديك و**غير موقّع**.

---

## 1. المتطلبات (متحقق منها من المستودع)

| المتطلب | الإصدار | الدليل |
|---|---|---|
| Windows | 11 x64 | حزمة Tauri تستهدف WebView2 على Windows، وCI يعمل على `windows-latest`. |
| Node.js | **>= 22** (مُختبر بـ24.14.1) | `package.json` ← `engines.node`. |
| npm | **>= 10**، والقفل مولّد بـ`npm@11.11.0` | `package.json` ← `engines.npm` و`packageManager`. |
| Rust | **>= 1.77.2** (مُختبر بـ1.97.1) | `apps/desktop/src-tauri/Cargo.toml` ← `rust-version`. |
| سلسلة أدوات Rust | `x86_64-pc-windows-msvc` مع `rustfmt` | CI يستعمل `dtolnay/rust-toolchain@stable` + `rustfmt`. |
| أدوات بناء C++ | MSVC + Windows SDK | لازمة لربط ثنائيات Tauri/WebView2. |
| WebView2 | Evergreen Runtime | مضمن في Windows 11، وإلا ثبّت Microsoft WebView2 Runtime. |
| NSIS | يجلبه Tauri CLI لهدف `nsis` | `bundle.targets: ["nsis"]`. |

أدوات اختيارية تستخدمها حزمة ضمان الجودة ولا تلزم لبناء المنتج: `playwright-core` لاختبارات CDP، وNVDA لتحقق الوصولية.

## 2. الحصول على المصدر

```powershell
git clone https://github.com/D0shi-Eng/Nexus1.git
cd Nexus1
```

## 3. تثبيت الاعتمادات

```powershell
npm ci
```

يثبّت `npm ci` رسم الاعتمادات المقفل حرفياً من `package-lock.json` للجذر و`apps/desktop` و`packages/contracts` ومساحات الأدوات.

## 4. تشغيل الواجهة وحدها (تكرار سريع على الواجهة)

```powershell
npm run dev
```

يشغّل Vite على `http://localhost:1420`. يعرض الواجهة **بلا** خلف Tauri، لذا أوامر سطح المكتب غير متاحة؛ استخدمه لتخطيط الواجهة فقط.

## 5. تشغيل تطبيق سطح المكتب الحقيقي في التطوير

```powershell
npm run desktop -- dev
```

يشغّل Vite ونافذة Tauri التطويرية (WebView2) مع خلف Rust موصول.

> سكربت مساحة سطح المكتب يضبط `CARGO_TARGET_DIR=%LOCALAPPDATA%\NEXUS\cargo-target` ليبقى مخرج بناء Rust خارج المستودع، ويمكنك تجاوزه بـ`CARGO_TARGET_DIR` خاص بك.

## 6. بوابات الجودة (نفس أوامر CI)

JavaScript / TypeScript:

```powershell
npm run typecheck
npm run lint
npm run test
npm run build
```

Rust:

```powershell
cd apps/desktop/src-tauri
cargo fmt --check
cargo check --all-targets
cargo test --lib -- --test-threads=1
```

الاختبارات متسلسلة عمداً لأن جزءاً منها يمر بأسطح DPAPI وSQLite. ويحتاج `cargo test` إلى متطلبات بناء Tauri/WebView2.

## 7. بناء ثنائي الإصدار ومثبّت NSIS

```powershell
# من جذر المستودع
npm run tauri --workspace=@nexus/desktop -- build
```

وللبناء بسكربت الإصدار الذي يعقّم مسارات الباني والمستخدم في المرشح الداخلي:

```powershell
pwsh -File apps/desktop/src-tauri/build-release.ps1
```

المخرجات المتوقعة (دليل الهدف الافتراضي):

- التطبيق: `%LOCALAPPDATA%\NEXUS\cargo-target\release\nexus_desktop.exe`
  (أو `apps/desktop/src-tauri/target/release/nexus_desktop.exe` إن لم تضبط `CARGO_TARGET_DIR`).
- المثبّت: `...\release\bundle\nsis\NEXUS_0.1.0_x64-setup.exe`.

المثبّت **غير موقّع** (`NotSigned`)، وقد يعرض SmartScreen تحذيراً. لا يوفّر المشروع شهادة توقيع كود.

## 8. التثبيت المحلي (اختياري، لبناءك أنت)

1. ابنِ مثبّت NSIS كما في الخطوة 7.
2. شغّل `NEXUS_0.1.0_x64-setup.exe`.
3. يثبّت التطبيق وأداة إزالة ونصوصاً قانونية (`legal/LICENSE` و`legal/NOTICE` و`legal/THIRD_PARTY_NOTICES.md`).
4. بيانات التطبيق في `%LOCALAPPDATA%\com.nexus.obligationos`. أداة الإزالة **لا تحذف** مجلد البيانات؛ خُذ نسخة احتياطية من داخل التطبيق إن كنت تحتاجها.

## 9. حاويات الاختبار المعزولة

تستعمل حزمة ضمان الجودة معرّف تطبيق مختلفاً (`com.nexus.obligationos.uatharness`) ومجلد بيانات منفصلاً، فلا تفتح اختبارات القبول قاعدة مستخدم حيّة أبداً:

```powershell
# بناء ثنائي اختبار معزول
cd apps/desktop
$env:CARGO_TARGET_DIR="$env:LOCALAPPDATA\NEXUS\cargo-target"
npx tauri build --debug --no-bundle --config ..\..\..\tools\e2e\uat-harness.conf.json

# بذر وتشغيل دورة الإنتاج على الحاوية المعزولة
pwsh -File tools\e2e\run-uat.ps1 -Phase production -Reset
```

هذه الحزم أدوات تطوير وليست جزءاً من المنتج.

## 10. استكشاف الأخطاء

| العَرَض | السبب / الحل |
|---|---|
| `link.exe not found` | ثبّت Visual Studio Build Tools مع حمل عمل C++ وWindows SDK. |
| نافذة WebView2 فارغة | ثبّت/أصلح Microsoft WebView2 Evergreen Runtime. |
| `cargo test` يعجز عن كتابة ملفات مؤقتة | قيد بيئي (مثل مجلد منخفض التكامل). ابنِ/اختبر من مجلد مستخدم عادي أو اضبط `CARGO_TARGET_DIR` و`TEMP`. |
| تثبيت npm غير حتمي | استخدم `npm ci` لا `npm install`، ولا تعدّل القفل بلا سبب. |
| SmartScreen يحذّر من المثبّت | متوقع لبناء غير موقّع. راجع [الأمان](security.ar.md) — التوقيع الموثوق قرار مالك. |
