# جرد تراخيص الأطراف الثالثة — NEXUS V1

**الحالة:** `THIRD_PARTY_LICENSE_INVENTORY = MEASURED_2026-10-18` · `PRODUCT_LICENSE = Apache-2.0` · `COMMERCIAL_REVIEW = PENDING_OWNER`

**تاريخ القياس:** 2026-10-18 · **نطاق القياس:** `<REPO_ROOT>` (المستودع الخاص، فرع العمل الحالي) · **الإصدار:** `0.1.0`

> هذا جرد تقني مقاس من الملفات الفعلية، وليس رأياً قانونياً. لا يعتمد حكماً على توافق
> ولا يستبدل مراجعة محامٍ. كل رقم أدناه مأخوذ من قياس جرى في هذه الجولة، والقياسات
> التي تعذّرت مذكورة صراحةً كغير مقيسة.

---

## 1. المنهج والأوامر المستخدمة (exact commands)

كل الأوامر نُفِّذت من جذر المستودع `<REPO_ROOT>` في PowerShell 7 (`pwsh`)، وهي **قراءة فقط**
بلا أي كتابة في `package-lock.json` أو `Cargo.lock` أو أي مصدر.

```powershell
# (1) مساحة npm — كل مدخلات package-lock.json بلا استثناء
Get-Content package-lock.json -Raw | ConvertFrom-Json -AsHashtable   # .packages

# (2) ترخيص كل حزمة npm من بيان الحزمة الفعلي
Get-Content "node_modules/<pkg>/package.json" -Raw | ConvertFrom-Json -AsHashtable
#     الحقول المقروءة: license  ثم licenses[].type  (بلا أي استنتاج من الاسم)

# (3) Rust — بيان الكريتات المحلولة من cargo metadata (قراءة فقط)
cargo metadata --format-version 1 --offline `
  --manifest-path apps/desktop/src-tauri/Cargo.toml
#     الناتج 2,426,249 بايت · 458 حزمة · 457 تحمل حقل license

# (4) تحقق مستقل: بيان السجل المحلي لكل كريت
#     $env:USERPROFILE\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\<name>-<version>\Cargo.toml
#     القسم [package] فقط · الحقول: license ثم license-file

# (5) الخطوط والأصول
Get-ChildItem apps/desktop/public/fonts -File
Get-ChildItem assets -Recurse -File | Measure-Object Length -Sum
git ls-files    # 549 ملفاً متتبعاً

# (6) أوزان النماذج
Get-ChildItem -Recurse -File -Include *.gguf,*.bin,*.safetensors,*.onnx,*.pt,*.pth,*.h5,*.mlmodel
#     مع استثناء node_modules/ و target/ و .git/
```

**الملفات المقيسة (بايت):**

| الملف | البايت |
|---|---|
| `package-lock.json` | 154,498 |
| `apps/desktop/src-tauri/Cargo.lock` | 117,252 |
| `THIRD_PARTY_NOTICES.md` | 12,968 |
| `LICENSE` (جديد في هذه الجولة) | 11,357 |
| `SECURITY.md` | 7,928 |

---

## 2. الفصل (a) — كود NEXUS الأصلي

| البند | القيمة المقيسة |
|---|---|
| حزم مساحة العمل (npm) | 2: `@nexus/desktop@0.1.0` (`apps/desktop`) · `@nexus/contracts@0.1.0` (`packages/contracts`) |
| كريت الجذر (Rust) | 1: `nexus_desktop@0.1.0` — العضو الوحيد في مساحة cargo |
| ملفات متتبعة في Git | **549** |
| حقل `license` في `package.json` الجذر / `apps/desktop` / `packages/contracts` | **غائب في الثلاثة** |
| حقل `license` في `apps/desktop/src-tauri/Cargo.toml` | **غائب** |
| ترخيص المنتج المعتمد | **Apache-2.0** — نص كامل في `LICENSE` (11,357 بايت) |

هذا الكود مملوك للمشروع ومُرخَّص بـ Apache-2.0. **لا** يحمل أي من البيانات الأربعة حقل
`license`/`license.workspace`، لأن هذه الجولة ممنوعة من تعديل `package.json` أو
`Cargo.toml`؛ الفجوة موثّقة في §10.

**تحقق من نص `LICENSE`:** مطابق حرفياً لنص Apache License 2.0 الرسمي — 201 سطراً،
11,357 بايت، LF فقط (0 CRLF)، SHA-256 = `C71D239DF91726FC519C6EB72D318EC65820627232B2F796219E87DCF35D0AB4`،
ويحوي `END OF TERMS AND CONDITIONS` و`APPENDIX: How to apply the Apache License to your work.`
مع `[yyyy] [name of copyright owner]` **دون استبدال** (كما يُنشر رسمياً).
المصدر: `https://www.apache.org/licenses/LICENSE-2.0.txt` (HTTP 200)، وقوبل بايت-ببايت مع
نسخة Apache-2.0 قياسية موجودة محلياً (`node_modules/@eslint/core/LICENSE`, 11,357 بايت,
بصمة SHA-256 نفسها) — تطابق تام.

---

## 3. الفصل (b) — تبعيات المصدر المفتوح

### 3.1 npm — 303 حزمة خارجية محلولة

`package-lock.json` (lockfileVersion 3) يحوي **308** مدخلاً: الجذر + مساحتا عمل +
**305** تحت `node_modules/`، منها **2** مدخلا ربط (link) لمساحتي العمل ⇒
**303 حزمة خارجية حقيقية**، و**303** مفتاح `name@version` فريد (لا تكرار فعلي؛
المفاتيح المتداخلة التسعة هي نسخ مُظلِّلة بإصدارات مختلفة).

| التصنيف | العدد |
|---|---|
| إنتاجية وقت التشغيل (غير `dev`) | **16** |
| أداة بناء معلَّمة `devOptional` | **1** (`typescript@5.9.3`) |
| إجمالي المدخلات غير المعلَّمة `dev` | **17** |
| تطويرية (`dev`) | **286** |
| اختيارية (optional) | 64 — منها **60** غير مثبتة على win32 |
| مثبَّتة وبيانها مقروء | **243** — وكلها تحمل حقل ترخيص صريح |
| غير مثبتة (ثنائيات منصّات أخرى) | **60** — ترخيصها من ترويسة `package-lock.json` |

**توزيع تراخيص npm (303 حزمة):**

| الترخيص | العدد | منها إنتاجي |
|---|---|---|
| `MIT` | **243** (193 مثبتة + 50 من القفل) | 13 |
| `Apache-2.0` | **17** | 1 (`typescript@5.9.3`) |
| `ISC` | **14** | 1 (`lucide-react@0.511.0`) |
| `Apache-2.0 OR MIT` | **13** (3 مثبتة + 10 من القفل) | 1 (`@tauri-apps/api@2.11.1`) |
| `BSD-2-Clause` | **8** | 0 |
| `BSD-3-Clause` | **3** | 0 |
| `MIT OR Apache-2.0` | **1** | 1 (`@tauri-apps/plugin-dialog@2.7.2`) |
| `BlueOak-1.0.0` | **1** | 0 |
| `CC-BY-4.0` | **1** | 0 |
| `MIT-0` | **1** | 0 |
| `Python-2.0` | **1** | 0 |
| **الإجمالي** | **303** | **17** |

**حزم الإنتاج الـ17** (16 وقت تشغيل + `typescript@5.9.3` المعلَّم `devOptional` كأداة
بناء — يُحزَّم ناتجه لا كوده) — كلها بترخيص صريح، لا استثناء:

| الحزمة | الإصدار | الترخيص |
|---|---|---|
| `@babel/runtime` | 7.29.7 | MIT |
| `@tanstack/query-core` | 5.101.4 | MIT |
| `@tanstack/react-query` | 5.101.4 | MIT |
| `@tauri-apps/api` | 2.11.1 | Apache-2.0 OR MIT |
| `@tauri-apps/plugin-dialog` | 2.7.2 | MIT OR Apache-2.0 |
| `clsx` | 2.1.1 | MIT |
| `html-parse-stringify` | 3.1.0 | MIT |
| `i18next` | 24.2.3 | MIT |
| `i18next-browser-languagedetector` | 8.2.1 | MIT |
| `lucide-react` | 0.511.0 | **ISC** |
| `react` | 19.2.8 | MIT |
| `react-dom` | 19.2.8 | MIT |
| `react-i18next` | 15.7.4 | MIT |
| `scheduler` | 0.27.0 | MIT |
| `typescript` (معلَّم `devOptional`) | 5.9.3 | **Apache-2.0** |
| `void-elements` | 3.1.0 | MIT |
| `zod` | 3.25.76 | MIT |

**حزم بأسماء تراخيص غير شائعة (كلها تطويرية):** `minimatch@10.2.6` = `BlueOak-1.0.0` ·
`caniuse-lite@1.0.30001809` = `CC-BY-4.0` · `argparse@2.0.1` = `Python-2.0` ·
`@csstools/color-helpers@5.1.0` = `MIT-0`. لا شيء منها إنتاجي، وكلها متسامحة/إسنادية.

**ملفات ترخيص داخل حزم الإنتاج:** 17/17 تحمل ملف ترخيص فعلياً
(`LICENSE` أو `license` أو `LICENSE.txt` أو `LICENSE.spdx` أو
`LICENSE_APACHE-2.0`+`LICENSE_MIT` في `@tauri-apps/api`).

**حزم npm بترخيص مفقود/مجهول: صفر.** لا حزمة بلا ترخيص معلن (المدخلان
`@nexus/desktop` و`@nexus/contracts` مدخلا ربط لمساحة العمل = كود المشروع نفسه).

### 3.2 Rust — 458 مدخل قفل، 457 كريتاً خارجياً

`apps/desktop/src-tauri/Cargo.lock`: **458** مدخل `[[package]]` = **457** كريتاً من
crates.io + كريت الجذر. **457/457** تحمل حقل `license` في بيانها؛ **صفر** حقل غائب،
**صفر** `license-file` وحده. تحقق مزدوج: `cargo metadata` مقابل بيان السجل المحلي ⇒
**0 اختلافات** في 457 كريتاً.

**توزيع تعبيرات الترخيص (30 تعبيراً فريداً):**

| تعبير الترخيص | العدد |
|---|---|
| `MIT OR Apache-2.0` | **221** |
| `MIT` | **101** |
| `Apache-2.0 OR MIT` | **34** |
| `Unicode-3.0` | **18** |
| `Zlib OR Apache-2.0 OR MIT` | **17** |
| `MIT/Apache-2.0` | **16** |
| `Unlicense OR MIT` | **9** |
| **`MPL-2.0`** | **5** |
| `BSD-3-Clause` | **3** |
| `Apache-2.0/MIT` | **3** |
| `ISC` | **3** |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | **3** |
| `Apache-2.0` | **2** |
| `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | **2** |
| `BSD-3-Clause OR MIT OR Apache-2.0` | **2** |
| `Zlib` | **2** |
| `Unlicense/MIT` | **2** |
| `MIT OR Apache-2.0 OR Zlib` | **2** |
| `MIT OR Zlib OR Apache-2.0` | **1** |
| `BSD-3-Clause/MIT` | **1** |
| `CC0-1.0 OR MIT-0 OR Apache-2.0` | **1** |
| `BSD-3-Clause AND MIT` | **1** |
| `Apache-2.0 WITH LLVM-exception` | **1** |
| `Apache-2.0 OR ISC OR MIT` | **1** |
| `Apache-2.0 AND MIT` | **1** |
| `Apache-2.0 AND ISC` | **1** |
| `Apache-2.0 / MIT` | **1** |
| `0BSD OR MIT OR Apache-2.0` | **1** |
| `CDLA-Permissive-2.0` | **1** |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | **1** |
| **الإجمالي** | **457** |

**مجاميع مشتقة:** 311 كريتاً لديها خيار `Apache-2.0` · 146 بلا خيار Apache إطلاقاً ·
421 لديها خيار MIT · 101 بترخيص `MIT` وحده.

**الكريتات الـ146 بلا خيار Apache-2.0** (تُلزم بإشعارها الخاص، لا بترخيص المنتج):
101 `MIT` · 18 `Unicode-3.0` · 9 `Unlicense OR MIT` · 5 `MPL-2.0` · 3 `BSD-3-Clause` ·
3 `ISC` · 2 `Unlicense/MIT` · 2 `Zlib` · 1 `BSD-3-Clause AND MIT` · 1 `BSD-3-Clause/MIT` ·
1 `CDLA-Permissive-2.0`.

**تعبيرات `AND` (كلا الالتزامين يسري فعلاً):** `brotli@9.0.0` (`BSD-3-Clause AND MIT`) ·
`dpi@0.1.2` (`Apache-2.0 AND MIT`) · `ring@0.17.14` (`Apache-2.0 AND ISC`) ·
`unicode-ident@1.0.24` (`(MIT OR Apache-2.0) AND Unicode-3.0`).

### 3.3 الحواجز والتراخيص الكوبي-ليفت

| البند | العدد | الحكم |
|---|---|---|
| حزم بترخيص **مفقود/مجهول** (خارجي) | **0** | لا حاجز |
| `GPL-*` / `AGPL-*` / `SSPL` / `CC-BY-NC*` / `Commons-Clause` / `EUPL` / `OSL` في npm | **0** | لا حاجز |
| `GPL-*` / `AGPL-*` / `SSPL` / `CC-BY-NC*` / `Commons-Clause` / `EUPL` / `OSL` في Rust | **0** | لا حاجز |
| **`MPL-2.0`** (كوبي-ليفت على مستوى الملف) | **5** | ليس حاجزاً؛ يُلزم بالإشعار وإتاحة مصدر الحزمة نفسها |
| `LGPL-2.1-or-later` كبديل ثالث | **2** | ليس حاجزاً؛ يُنتخب MIT/Apache-2.0 |

**حزم `MPL-2.0` الخمس (مقيسة بالاسم والإصدار):**

| الكريت | الإصدار | ملف الترخيص في الحزمة |
|---|---|---|
| `cssparser` | 0.37.0 | `LICENSE` |
| `cssparser-macros` | 0.7.1 | `LICENSE` |
| `dtoa-short` | 0.3.5 | `LICENSE` |
| `option-ext` | 0.2.0 | `LICENSE.txt` |
| `selectors` | 0.38.0 | **لا ملف ترخيص في الحزمة المنشورة** (الترخيص في `Cargo.toml` فقط) |

أثرها: MPL-2.0 لا تفرض ترخيص الكود المحيط؛ تُلزم بإتاحة **مصدر الملفات المغطاة نفسها**
وبإبقاء إشعاراتها. لم تُعدَّل أي من هذه الحزم في المستودع (كلها من crates.io بلا
`[patch]` أو مصدر git). `selectors@0.38.0` بلا ملف ترخيص مرفق ⇒ يلزم نص MPL-2.0 من
مصدره الرسمي عند التوزيع، لا نسخ ملف غير موجود.

**`LGPL-2.1-or-later` كبديل:** `r-efi@5.3.0` و`r-efi@6.0.0` بتعبير
`MIT OR Apache-2.0 OR LGPL-2.1-or-later` — بدائل متعددة، يُنتخب MIT/Apache-2.0 فلا
ينشأ أي التزام LGPL. الحزمة نفسها لا تحمل ملف ترخيص في السجل المحلي.

---

## 4. الفصل (c) — الخطوط والأيقونات والأصول في `assets/`

### 4.1 الخطوط — خط ثالث مضمَّن فعلاً

| البند | القياس |
|---|---|
| العائلة | **IBM Plex Sans Arabic** |
| الملفات | **8** ملفات `.woff2` (عربي × 400/500/600/700 · لاتيني × 400/500/600/700) |
| الحجم الكلي | **257,368 بايت** (≈ 251.3 KiB) |
| الموضع | `apps/desktop/public/fonts/` — متتبعة في Git (8 + `OFL.txt`) |
| ملف الترخيص | `apps/desktop/public/fonts/OFL.txt` — **4,456 بايت**، نص SIL OFL 1.1 كامل |
| الترخيص | **SIL Open Font License 1.1 (OFL-1.1)** |
| سطر حقوق النشر | `Copyright © 2017 IBM Corp. with Reserved Font Name "Plex"` |
| **Reserved Font Name** | **نعم — `"Plex"`** |
| نسبة الإسناد مطلوبة | **نعم** (ترويسة `apps/desktop/src/styles/fonts.css` + `THIRD_PARTY_NOTICES.md` §7) |
| تعديل على ملفات الخط | **لا** — منسوخة كما هي من مستودع IBM/plex عبر `tools/fonts/fetch-plex-arabic.mjs` |

**ما يفرضه OFL-1.1 هنا:** إبقاء سطر حقوق النشر ونص الترخيص مع الملفات؛ منع بيع الخط
منفرداً؛ ومنع استخدام **Reserved Font Name `"Plex"`** لأي نسخة معدَّلة. بما أن الملفات
غير معدَّلة وتُوزَّع باسمها الأصلي، فالنسبة والإسناد هما الالتزام الوحيد القائم، وهو
منفَّذ في `NOTICE` و`THIRD_PARTY_NOTICES.md`.

الخطوط **تُحزَّم فعلاً في التطبيق**: نسخ مبنية في `apps/desktop/dist/fonts/`
(.gitignore) ونسخ مضمَّنة في ناتج tauri codegen تحت `src-tauri/target/release/build/.../tauri-codegen-assets/`.
أي أن OFL-1.1 ترافق الحزمة الموزَّعة، لا المستودع فقط.

### 4.2 `assets/` — أصول أصلية (منشأة للمشروع)

| البند | القياس |
|---|---|
| عدد الملفات | **30** |
| الحجم الكلي | **489,647 بايت** |
| `assets/branding/nexus-icon.svg` | 2,357 بايت |
| `assets/branding/nexus-icon.ico` | 118,770 بايت |
| `assets/branding/png/` | 8 ملفات PNG (710 → 34,753 بايت) |
| `assets/branding/preview/` | 16 ملف PNG معاينة |
| `assets/branding/build/` + تقارير | `nexus_mark.py` (36,899) · `css_token_audit.py` (4,598) · `build-report.txt` (8,732) · `PREVIEW.md` (9,748) |
| ملفات ترخيص بجوار الأصول | **لا شيء** — ولا `OFL`/`LICENSE` في `assets/` |
| أصل خارجي | **لا شيء** — الأيقونة والعلامة أُنشئتا للمشروع (`nexus_mark.py` هو المولِّد) |

الأصول الأصلية مملوكة للمشروع وتخضع لـ Apache-2.0 مثل بقية الكود؛ تسجيل العلامة
التجارية قرار مالك لم يُتخذ (بوابة `L11` في `LEGAL_OWNER_GATE_AR.md`).

### 4.3 أيقونات التطبيق و`lucide-react`

| البند | القياس |
|---|---|
| `apps/desktop/src-tauri/icons/` | **15** ملفاً: `icon.ico` (118,770) + 13 PNG + `StoreLogo.png` |
| أيقونة الحزمة الفعلية | `icon.ico` و4 PNG تعلنها `tauri.conf.json` (`bundle.icon`) |
| مكتبة أيقونات وقت التشغيل | `lucide-react@0.511.0` — **ISC** (تبعية إنتاجية) |
| ملف ترخيص lucide | `node_modules/lucide-react/LICENSE` موجود |

### 4.4 تناقض موثَّق مع `THIRD_PARTY_NOTICES.md`

| الادعاء القائم في `THIRD_PARTY_NOTICES.md` | القياس في هذه الجولة |
|---|---|
| «`icons/icon.ico` — 15,174 بايت» (§4) | `icon.ico` = **118,770 بايت** |
| «خطوط مضمّنة: **لا شيء**» (§4) | **8 ملفات woff2 / 257,368 بايت تُحزَّم فعلاً** (§7 من الملف نفسه يذكرها — تناقض داخلي) |
| «لا صور مضمّنة» (§4) | صحيح للمصدر؛ الصور في `assets/` و`icons/` أدوات بناء/أصول واجهة فقط |

---

## 5. الفصل (d) — مكوّنات طرف ثالث تُحزَّم في المنتج

| المكوّن | المصدر | الترخيص | يُوزَّع؟ |
|---|---|---|---|
| IBM Plex Sans Arabic (8 × woff2) | `apps/desktop/public/fonts/` | **OFL-1.1** (Reserved Font Name `"Plex"`) | **نعم** — مضمَّن في الحزمة |
| SQLite (مضمَّن داخل `rusqlite` بميزة `bundled`) | `libsqlite3-sys` / SQLite | **Public Domain** (SQLite) | نعم، كجزء من المكتبة المترابطة |
| WebView2 Runtime | مكوّن نظام Windows | شروط Microsoft (غير مُعاد توزيعه في المستودع) | لا — يُفترض وجوده على النظام |
| NSIS installer stub | أداة بناء Tauri | `zlib/libpng` (أداة بناء) | فقط إن كان المثبّت موزَّعاً؛ **المثبّت غير موجود في المستودع** |
| أيقونات `lucide-react` (SVG مولَّدة في الحزمة) | npm | ISC | نعم، ككود مضمَّن |

بقية تبعيات npm تُحزَّم كـ JavaScript مُجمَّع، وتبعيات Rust تُحزَّم ككود آلة داخل
التنفيذي — وكلاهما يوجب إرفاق إشعاراتها (وهو ما يفعله `THIRD_PARTY_NOTICES.md`).

---

## 6. الفصل (e) — أوزان نماذج ذكاء اصطناعي

| النمط | العدد خارج `node_modules/` و`target/` و`.git/` |
|---|---|
| `*.gguf` | **0** |
| `*.bin` | **0** |
| `*.safetensors` | **0** |
| `*.onnx` · `*.pt` · `*.pth` · `*.h5` · `*.mlmodel` | **0** |

⇒ **لا أوزان نماذج في المستودع ولا في الحزمة.** لا تنشأ أي حقوق توزيع على أوزان.
الاستدلال يجري عبر خدمة محلية يشغّلها المستخدم (Ollama على `127.0.0.1:11434` أو
LM Studio أو خادم متوافق) — انظر §7.

---

## 7. الفصل (f) — خدمات خارجية بشروط منفصلة (ليست كوداً مُعاد توزيعه)

| الخدمة/النقطة | Destination | تُحزَّم؟ | الأثر الترخيصي في هذا المستودع |
|---|---|---|---|
| OpenRouter | `https://openrouter.ai/api/v1/...` | **لا** | شروط خدمة منفصلة؛ لا كود ولا بيانات تُعاد توزيعها |
| خادم Bonsai المحلي | خادم المستخدم (OpenAI-compatible) | **لا** | خارج نطاق الترخيص |
| Ollama | `http://127.0.0.1:11434` | لا | حلقي فقط |
| LM Studio | `http://127.0.0.1:1234` | لا | حلقي فقط |
| OpenAI · Anthropic · xAI · DeepSeek · Qwen/DashScope · Z.ai | نقاط HTTPS بعيدة | **لا** | شروط مزوّد منفصلة؛ المستخدم يجلب مفتاحه |

هذه الخدمات **ليست** مكوّنات مُعادة التوزيع، ولا تُدخل أي التزام ترخيص في `NOTICE`.
العلاقة بها تعاقدية بين المستخدم والمزوّد، ولا يوزّع NEXUS أي مفتاح أو بيانات اعتماد.

---

## 8. قائمة الإشعارات الواجبة (notice-retention)

| المصدر | الترخيص | ما يجب إبقاؤه |
|---|---|---|
| 5 كريتات MPL-2.0 | `MPL-2.0` | نص MPL-2.0 + إتاحة مصدر الحزمة نفسها؛ لا تعديل جرى |
| `webpki-roots@1.0.9` | `CDLA-Permissive-2.0` | إشعار CDLA (بيانات جذور الشهادات) |
| 19 كريتاً `Unicode-3.0` (18 + `unicode-ident`) | `Unicode-3.0` | إشعار Unicode |
| IBM Plex Sans Arabic | `OFL-1.1` | سطر حقوق النشر + نص OFL + عدم استخدام RFN `"Plex"` لنسخة معدَّلة |
| 17 حزمة npm إنتاجية | MIT/ISC/Apache-2.0 | نصوص تراخيصها (متاحة في حزمها) |
| `caniuse-lite@1.0.30001809` | `CC-BY-4.0` | إسناد (تطويرية فقط) |
| بقية الكريتات الـ146 بلا خيار Apache | MIT/BSD/ISC/Zlib/Unlicense | إشعاراتها عبر `THIRD_PARTY_NOTICES.md` |

---

## 9. التوافق مع Apache-2.0 — ما يقوله هذا القياس

1. **لا حزمة كوبي-ليفت إلزامية** (GPL/AGPL/LGPL/SSPL/CC-BY-NC/Commons-Clause) في أي من
   النظامين. العدد **صفر** في npm و**صفر** في Rust.
2. **MPL-2.0 (5 كريتات)** كوبي-ليفت على مستوى الملف داخل الحزمة نفسها فقط؛ لا تفرض
   ترخيص الشجرة. متوافقة مع توزيع منتج Apache-2.0 بشرط الإشعار وإتاحة مصدر الحزمة.
3. **`LGPL-2.1-or-later`** يظهر كبديل ثالث في `r-efi` فقط، ويُنتخب بدلاً منه MIT/Apache-2.0.
4. كل التراخيص الأخرى متسامحة (MIT/ISC/BSD/Apache/Zlib/Unicode/Unlicense/0BSD/CC0/MIT-0/
   BlueOak-1.0.0/Python-2.0) ⇒ **متوافقة مع Apache-2.0** ولا تمنع ترخيص المنتج بها.
5. `CC-BY-4.0` (حزمة تطويرية واحدة) تفرض إسناداً لا كوبي-ليفت، ولا تمنع الترخيص.
6. **صفر حزمة بترخيص مفقود أو مجهول** في التبعيات الخارجية للنظامين.

⇒ بحسب هذا القياس، **Apache-2.0 متوافق مع مجموعة التبعيات المقيسة** بشرط إبقاء
إشعارات MPL-2.0/OFL/Unicode/CDLA/CC-BY والمكوّنات المتسامحة.

---

## 10. الفجوات المفتوحة والتناقضات (لا تُقنَّع)

| # | البند | التفصيل | المسار المقترح |
|---|---|---|---|
| G1 | حقول SPDX غائبة في البيانات | `package.json` (الجذر) · `apps/desktop/package.json` · `packages/contracts/package.json` · `apps/desktop/src-tauri/Cargo.toml` بلا حقل `license`. الجولة ممنوعة من تعديلها | تعديل مستقبلي: `"license": "Apache-2.0"` |
| G2 | `SECURITY.md` سطر 73 | «لا يوجد ملف `LICENSE` عن قصد… قرار الترخيص مؤجَّل» — لم يُعدَّل (ممنوع في هذه المهمة) | تحديثه في جولة لاحقة |
| G3 | `docs/legal/README.md` سطر 24 | «لا يوجد ترخيص منتج معتمد… لم يكن هناك `LICENSE`» — صار غير صحيح | تحديثه |
| G4 | `docs/legal/LEGAL_OWNER_GATE_AR.md` | `L3`/`L5` ما زالتا `BLOCKED_BY_OWNER`؛ ترخيص المنتج لم يُدرج كبوابة مغلقة | تحديث الحالة |
| G5 | `THIRD_PARTY_NOTICES.md` §4 | أرقام قديمة/متناقضة: `icon.ico` 15,174 مقابل 118,770 مقيسة؛ «خطوط مضمّنة: لا شيء» مقابل 8 ملفات تُحزَّم | تحديثه |
| G6 | `selectors@0.38.0` | لا ملف ترخيص داخل الحزمة المنشورة (MPL-2.0 في `Cargo.toml` فقط) | إرفاق نص MPL-2.0 عند التوزيع |
| G7 | `package.json` حقول `private: true` | الحزمة الجذر ومساحتا العمل ما زالت `private` — لا يمنع فتح المصدر لكن يمنع النشر على npm | قرار مالك |
| G8 | لا `CONTRIBUTING.md` | لا يوجد ملف مساهمة أصلاً ⇒ لا تناقض ترخيصي فيه | اختياري مستقبلاً |

---

## 11. خلاصة القياس

| المؤشر | القيمة |
|---|---|
| حزم npm الخارجية | 303 (16 وقت تشغيل + 1 `devOptional` · 286 تطويرية · 64 اختيارية) |
| كريتات Rust الخارجية | 457 (من 458 مدخل قفل) |
| إجمالي المكوّنات الخارجية المقيسة | **760** |
| بلا ترخيص معلن (خارجي) | **0** |
| كوبي-ليفت إلزامي (GPL/AGPL/LGPL/SSPL/CC-BY-NC/Commons) | **0** |
| MPL-2.0 (كوبي-ليفت ملفي) | **5** كريتات |
| خطوط مضمَّنة | 8 × woff2 = 257,368 بايت · OFL-1.1 · RFN `"Plex"` |
| أوزان نماذج في المستودع | **0** |
| خدمات خارجية مُعادة التوزيع | **0** |
| ترخيص المنتج | **Apache-2.0** (`LICENSE`, 11,357 بايت، مطابق للنص الرسمي) |
