<div dir="rtl">

# NEXUS V1 — ملاحظات الإصدار `0.1.0`

**الإصدار:** `0.1.0` · **التاريخ:** 2026-10-19 (مراجعة عقود المزوّدين والإصدار النهائي) · **الحالة:** مرشّح إصدار متحقَّق داخلياً، **غير موقّع**، **غير منشور** · **الترخيص:** Apache License 2.0

هذه الملاحظات تخصّ **V1 فقط**. لا يوجد V2، ولا ادعاء بأي قدرة غير مبنية. المستودع **خاص**، ولا يوجد إصدار تجاري منشور. المثبّت يتضمّن الآن نصوص `LICENSE` و`NOTICE` و`THIRD_PARTY_NOTICES.md` داخل مجلد `legal/` في مسار التثبيت.

---

## English summary

- **What it is:** NEXUS is a local-first Windows 11 x64 desktop application (Tauri 2 · Rust · React 19 · SQLite) for governed obligations, evidence, risk, controls, tasks, approvals and reports, with an AI agent runtime that holds no executive authority — every run and tool invocation passes authorization, approval and audit gates in the Rust backend.
- **Version:** `0.1.0`, internally verified release candidate. **Unsigned.** No public release artifact, no code signing certificate, no bundled model weights.
- **Data:** one local SQLite file in `%LOCALAPPDATA%\com.nexus.obligationos`. No telemetry, no automatic network connection, no cloud sync.
- **Providers:** the only `LIVE_INFERENCE_VERIFIED` path is local (Bonsai 2 over `127.0.0.1`). All ten provider presets are `DOCS_VERIFIED` against each vendor's current official documentation (19 Oct 2026) and `CONTRACT_TESTED` against recorded API shapes on local mock servers — **no cloud provider has `AUTHENTICATION_VERIFIED` or `LIVE_INFERENCE_VERIFIED`**. No paid API was used and no real key is stored. See [`docs/ai/PROVIDER_SUPPORT_MATRIX.md`](ai/PROVIDER_SUPPORT_MATRIX.md).
- **License:** Apache License 2.0, Copyright 2026 The NEXUS Authors.
- **Read this first:** known limitations below (unsigned build — trusted signing is an owner gate while the unsigned installer is technically tested; non-100% system DPI not verified; the declared minimum window 980×640 and 1100×700/1280×820 are verified across RTL/LTR and light/dark/high-contrast; audit-chain trust boundary; OpenRouter pending owner key).

---

## 1. ما هو NEXUS

NEXUS تطبيق سطح مكتب لنظام Windows 11 (x64) لإدارة **الالتزامات التنظيمية والأدلة والمخاطر والضوابط والمهام والموافقات والتقارير** لفرق الامتثال والحوكمة، مع طبقة تشغيل وكلاء ذكاء اصطناعي **محكومة بالكامل**: التفويض والموافقة والتدقيق تُقيَّم في الخلف (Rust)، لا في الواجهة.

- **محلي أولاً:** لا تليمتري، ولا اتصال شبكي تلقائي، ولا مزامنة سحابية.
- **ثنائي اللغة:** العربية والإنجليزية بدعم اتجاه RTL/LTR.
- **سلطة واحدة:** الخلف هو مصدر الحقيقة في الهوية والجلسة والصلاحية؛ الواجهة لا ترسل هوية ولا مستأجراً ولا صلاحيات، والخلف لا يقبلها.
- **أثر تدقيق متسلسل:** SHA-256 لكل حدث، مع تحقق فعلي يعيد تشغيل التجزئة.
- **معمارية:** Clean Architecture داخل نواة Rust (`domain` / `application` / `infrastructure` / `presentation`)، و**117 أمر IPC**، ومهاجرات مرقمة `0001 → 0026`.

## 2. الوحدات الـ15 وحالتها الحقيقية

الحالات أدناه منقولة كما هي من `README.ar.md` §17 — **لم تُرقَّ أي حالة** في هذه الملاحظات.

| الوحدة | الحالة المنفَّذة فعلاً |
|---|---|
| `overview` نظرة تنفيذية | **Functional** — جاهزية مشتقة، و`غير مقيس` عند نقص البيانات، وصحة السلسلة |
| `attention` ما يحتاج اهتماماً | **Functional** — أولويات مشتقة بمرشّحات وإجراءات |
| `obligations` الالتزامات | **Functional** — إنشاء/تعديل/أرشفة/تحقق بشري |
| `regulatory` الذكاء التنظيمي | **Functional (user-provided)** — تسجيل مصدر غير رسمي؛ لا تغذية حية |
| `risks` المخاطر | **Read-only** — سجل مخاطر |
| `evidence` الأدلة | **Functional** — تسجيل ودورة حالة وقرار المراجع |
| `tasks` المهام | **Functional** — آلة حالة كاملة وسجل انتقالات |
| `controls` الضوابط | **Functional** — تأليف وتقييم وربط |
| `agents` الوكلاء | **Functional** — تشغيل محكوم فعلي بلا استنتاج افتراضي |
| `knowledge` المعرفة | **Functional** — TXT/Markdown + FTS5 + اقتباسات مُتحقَّقة |
| `reports` التقارير | **Functional** — 6 تعريفات + تصدير CSV/Markdown |
| `audit` التدقيق | **Read-only + verify** — سلسلة SHA-256 وتحقق فعلي |
| `organization` المؤسسة | **Read-only** — وضع تشغيلي مشتق |
| `integrations` التكاملات | **Read-only** — حالة المزوّدين والنماذج |
| `settings` الإعدادات | **Functional** — عام/مظهر/لغة/مزوّدون/بيانات/مستخدمون/أمان/حول |

## 3. التعامل مع البيانات

- **التخزين محلي بالكامل:** ملف SQLite واحد داخل مجلد بيانات التطبيق، والمعرّف المعلن هو `com.nexus.obligationos` (`tauri.conf.json`)، فيكون المسار:
  `%LOCALAPPDATA%\com.nexus.obligationos\nexus-local.db`
- **لا تليمتري، ولا استعلام دوري، ولا اتصال شبكي تلقائي** في القشرة.
- **لا مزامنة سحابية ولا نشر مركزي متعدد المؤسسات** — كلاهما خارج نطاق V1 بنيوياً.
- **خزنة الاعتماد:** مفاتيح مزوّدي النماذج تُشفَّر عند السكون بحماية نظام التشغيل (DPAPI بمستوى المستخدم، مربوطة بالمزوّد)؛ لا تمر في SQLite ولا في سجل التدقيق ولا في السجلات ولا تُعاد عبر IPC.
- **النسخ الاحتياطي:** `VACUUM INTO` مع بيان يحمل إصدار المخطط وSHA-256، ورفض التلاعب والاستعادة غير المؤكدة. الاستعادة **تُبطل مرساة سلسلة التدقيق صراحةً** ثم تُثبَّت من جديد عند أول تحقق ناجح.
- **التصدير:** CSV/Markdown عبر حوار الحفظ الأصلي، بكتابة ذرّية، مع حدث تدقيق واحد لكل تصدير.

## 4. المزوّدون وحدود واجهات API

- **التوليد في V1 متحقَّق حيّاً على مزوّد محلي عبر `127.0.0.1` فقط** (Bonsai 2 عبر `OPENAI_COMPATIBLE` · ومسار Ollama الأصلي متاح)، والاستدعاء **صريح من المستخدم** لا تلقائي.
- **المزوّدون السحابيون: عقود موثّقة ومختبَرة، بلا تحقق حيّ.** الأنواع العشرة كلها `DOCS_VERIFIED` من الوثائق الرسمية الحالية (19 أكتوبر 2026) و`CONFIGURATION_VERIFIED` و`CONTRACT_TESTED` عبر خوادم HTTP وهمية محلية؛ DeepSeek/Qwen/Z.ai لديها `REACHABILITY_VERIFIED` من فحص 15 أكتوبر 2026. **لا يوجد أي مزوّد سحابي بمستوى `AUTHENTICATION_VERIFIED` أو `LIVE_INFERENCE_VERIFIED` أو `TOOL_CALLING_LIVE_VERIFIED`.** المصفوفة الكاملة: [`docs/ai/PROVIDER_SUPPORT_MATRIX.md`](ai/PROVIDER_SUPPORT_MATRIX.md).
- **البث (Streaming) غير منفَّذ:** كل طلب يرسل `"stream": false`. استدعاء الأدوات والمخرجات المهيكلة منفَّذان في مسار Ollama الأصلي فقط، وفي المحوّل المتوافق مع OpenAI ومحوّل Anthropic يفشلان مغلقين (`CAPABILITY_NOT_IMPLEMENTED`) ولا تُعرض قدرة غير مدعومة.
- **لم يُستخدم أي مفتاح مدفوع ولا أي استدعاء API مدفوع** في بناء V1 أو التحقق منه؛ ولا مفاتيح حقيقية مودعة.
- **سياسة الوجهة الشبكية** ترفض `http` لغير الحلقي، وبيانات الاعتماد داخل العنوان، والنطاقات الخاصة/link-local/الوصفية، وتثبّت العنوان المتحقق منه، وتفرض حدود حجم ومهلة.
- **لا تُضمَّن أي أوزان نماذج ولا نموذج** في المستودع أو الحزمة.

## 5. القيود المعروفة (معلنة — لا تُقنَّع)

- **الحزمة غير موقّعة:** الثنائي والمثبّت `NotSigned` (مقاس فعلياً بـ`Get-AuthenticodeSignature`). **لا يوجد توقيع تجريبي ذاتي (self-signed) ولم يُستخدم**، ولا توجد شهادة توقيع في المستودع. التوقيع والشهادة بوابة مالك. **`UNSIGNED_INSTALLER_TECHNICALLY_TESTED = VERIFIED` منفصل عن `TRUSTED_SIGNED_DISTRIBUTION_READY = NOT_VERIFIED`.**
- **التثبيت والإزالة: `VERIFIED` (19 أكتوبر 2026)** — `DEF-INSTALL-01` أُغلق: السبب الجذري كان بيئياً (تسمية سلامة منخفضة على مجلد المشروع من صندوق عزل وكيل سابق تمنع كتابة `HKCU`/Start Menu/`%LOCALAPPDATA%` لأي مثبّت يُشغَّل من داخله). أُعيد القياس من سياق مستخدم صحيح: تثبيت تفاعلي وصامت، تسجيل Installed Apps بـ`UninstallString` صالح، اختصارا Start Menu وسطح المكتب، تشغيل من الاختصار، إزالة كاملة، بقاء بيانات المستخدم ببصمة متطابقة، وإعادة تثبيت على البيانات نفسها. **لم يُعدَّل مصدر المثبّت.** التفصيل الكامل في سجل تحقق محلي غير منشور محفوظ عند المشرف.
- **الترقية: `SYNTHETIC_LOCAL_VERIFIED`** — حزمتان محليتان `0.0.9 → 0.1.0` مع اكتشاف «Already Installed / older version» وترحيل مخطط `25 → 26` بلا فقدان بيانات. `UPGRADE_FROM_RELEASED_VERSION = NOT_APPLICABLE` لأن V1 أول إصدار منشور (ولا يوجد إصدار سابق منشور).
- **الإتاحة: `NVDA_ACCESSIBILITY = VERIFIED` (19 أكتوبر 2026)** — جلسة NVDA 2026.2 المحمول الرسمي الموقَّع: نطق شاشة الدخول والحقول المطلوبة والوحدات الخمس عشرة وإدارة المستخدمين والحوارات ورسائل الخطأ، وتبديل العربية RTL/الإنجليزية LTR، مع حصر التركيز (12 Tab داخل الحوار) واستعادة التركيز بعد Escape. **مصفوفة النافذة الصغيرة = VERIFIED (19 أكتوبر 2026):** نافذة WebView2 الحقيقية عند 980×640 (الحد الأدنى المعلن) و1100×700 و1280×820 × العربية RTL/الإنجليزية LTR × فاتح/داكن/تباين عالٍ (18 خلية، قياس DOM فعلي): صفر تجاوز أفقي للمستند، صفر قص لعناصر حيوية، الحوارات محصورة، وحصر التركيز وEscape سليمان. أُغلقت ثلاثة عيوب جذرية أثناء الإغلاق: `SW-01` تمدد `.main-content` تحت `min-content`، `SW-02` حجب جدول الالتزامات للتمرير الأفقي، `SW-03` فلات `.visually-hidden` المطلق من قصّ حاويات التمرير وتوسيعه عرض المستند. **ما زال غير مقيس:** DPI نظام ≠ 100%، نافذة أصغر من الحد الأدنى المعلن 980×640 (ليست مسار مستخدم)، قارئ برايل. الثيمات السبعة وRTL/LTR ولوحة المفاتيح وأسماء الوصول والتباين مقيسة 7/7.
- **حدود نموذج ثقة سلسلة التدقيق:** السلسلة تكشف تعديل حدث، والحذف من المنتصف، وإعادة الترتيب، وتغيير الفاعل/المستأجر، واقتطاع الذيل مقارنةً بالمرساة (`audit-anchor.json`). **لا تكشف** إعادة كتابة **قاعدة البيانات والمرساة معاً** بشكل متسق، ولا تحمي من مالك الجهاز. المرساة **داخل مجلد بيانات التطبيق** لا خارج الجهاز؛ التحصين الكامل (HMAC بمفتاح خارجي أو مرساة خارجية) قرار مالك مؤجل إلى V2.
- **غير منفَّذ في V1:** الاسترجاع الدلالي المتجهي · إدخال PDF · الأدوات المُعدِّلة · Council · Scheduler · SSO/MFA · التغذية التنظيمية الحية · النشر المركزي متعدد المؤسسات.
- **`cargo audit`:** نُفِّذ لاحقاً في جولة التحصين (**0 ثغرات** · 458 حزمة · تحذيران غير مشمولين في هدف Windows)، بينما لا يزال جدول `README` يذكر `NOT_VERIFIED` من جولة أقدم — راجع §9.
- **تاريخ Git:** يحتوي مسار ملف تعريف مؤلف قديماً؛ المستودع خاص، وإزالة الأثر تتطلب إعادة كتابة تاريخ (ممنوعة) ⇒ قرار مالك قبل أي فتح عام.
- NEXUS يعرض **تقديرات جاهزية** مشتقة من البيانات والأدلة المتاحة محلياً، ولا يقرر الامتثال القانوني ولا يمثل استشارة قانونية.

## 6. التثبيت والإزالة والاحتفاظ بالبيانات

- **نوع المثبّت:** NSIS فقط (`bundle.targets: ["nsis"]` في `tauri.conf.json`).
- **اسم الحزمة المتوقع عند البناء:** `NEXUS_0.1.0_x64-setup.exe` في `bundle/nsis/`.
- **البناء:** `pwsh -File apps/desktop/src-tauri/build-release.ps1` (يفرض `--remap-path-prefix` لتعقيم مسارات الباني). التحزيم يتطلب Windows. القياس النهائي: **0 مطابقة** لمسار الملف الشخصي أو مسار المشروع المحلي أو بريد المالك داخل الثنائي والمثبّت (فحص بايتات فعلي).
- **نصوص الترخيص داخل الحزمة:** `bundle.licenseFile` يعرض `LICENSE` في صفحة الترخيص، و`bundle.resources` ينسخ `LICENSE` و`NOTICE` و`THIRD_PARTY_NOTICES.md` إلى `legal/` في مسار التثبيت — تحقق فعلي بعد التثبيت (11,357 · 2,830 · 13,179 بايت).
- **الإزالة والاحتفاظ بالبيانات: `VERIFIED` بالقياس** — قياس فعلي في 19 أكتوبر 2026 (وأُعيد على الحزمة النهائية): `uninstall.exe /S` أزال مجلد التثبيت وسجل Installed Apps واختصاري Start Menu وسطح المكتب (الأربعة `False` بعد الإزالة)، وأبقى `%LOCALAPPDATA%\com.nexus.obligationos` ببصمة SHA-256 متطابقة قبل الإزالة وبعدها، وبقيت سلامة قاعدة المستخدم `integrity_check=ok` و`FK=0` وسلسلة تدقيق `chain_ok=true` (16 حدثاً، مخطط 26).
- **قبل حذف المجلد يدوياً:** خُذ نسخة احتياطية من داخل التطبيق (`VACUUM INTO` + بيان) إن كانت البيانات مطلوبة؛ فحذف المجلد يُفقد سجل التدقيق ومرساته أيضاً.
- **لا يُرفع المثبّت إلى المستودع**، ولا يُنشر أي أثر إصدار عام.

## 7. الترخيص

- **الترخيص:** **Apache License 2.0** — النص الكامل في [`LICENSE`](../LICENSE).
- **حقوق النشر:** **Copyright 2026 The NEXUS Authors**.
- **تراخيص الأطراف الثالثة:** [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md) — جرد مبني على القوافل الفعلية (Rust: 30 تعبير ترخيص، 5 حزم MPL-2.0 بإشعارات فقط، لا GPL/AGPL مُلزِم؛ JavaScript: 17 تبعية إنتاجية مرخّصة). خط IBM Plex Sans Arabic بترخيص SIL OFL 1.1 ونص الترخيص مضمّن.
- **المساهمات:** Apache-2.0 وارداً = صادراً، بلا CLA — راجع [`CONTRIBUTING.md`](../CONTRIBUTING.md) §9.
- **حدود:** الترخيص يمنح حقوق استخدام/تعديل/توزيع للشيفرة؛ ولا يمنح شهادة توقيع ولا ضماناً قانونياً للامتثال.

## 8. ما لا يتضمنه هذا الإصدار

- **لا يوجد أثر إصدار عام منشور** (ولا صفحة إصدارات، ولا حزمة تحميل) — المستودع خاص ولم يُنشر أي مثبّت.
- **لا توجد شهادة توقيع كود** ولا توقيع ناشر موثوق؛ الثنائي والمثبّت `NotSigned`، ولا توقيع تجريبي ذاتي.
- **لا تُضمَّن أوزان نماذج** ولا أي نموذج لغوي؛ المزوّد المحلي يبقى مسؤولية المستخدم.
- **لا يوجد تحديث تلقائي** ولا قناة تحديث.
- **لا يوجد نشر سحابي مركزي** ولا SSO/MFA ولا مزامنة بين الأجهزة.
- **لا نسخة macOS أو Linux.**
- **لا بيانات ديمو حقيقية:** أي بيانات عرض هي `DEMO / SYNTHETIC` ومعلنة كذلك.

## 9. ما تم قياسه فعلياً (مرجع، بلا تلفيق)

المصدر: قياسات تشغيل حقيقية على شجرة الإصدار الحالية (سجل التدقيق الداخلي محفوظ عند المشرف وخارج الشجرة المنشورة).

| البوابة | النتيجة |
|---|---|
| `npm run typecheck` | PASS — Exit 0 |
| `npm run lint` (`--max-warnings=0`) | PASS — Exit 0 · 0 تحذير |
| `npm run test` | PASS — **34 ملفاً / 202 اختباراً / 0 فاشل** |
| `npm run build` | PASS — Exit 0 |
| `cargo fmt --check` | PASS — Exit 0 |
| `cargo check --all-targets` | PASS — 0 خطأ / 0 تحذير |
| `cargo test --lib -- --test-threads=1` | PASS — **622 ناجح / 0 فاشل / 10 مُهملة** |
| تحقق قاعدة الإنتاج (قراءة فقط) | **15/15 PASS** (سلامة، مفاتيح أجنبية، سلسلة تدقيق، مرساة، RBAC، عزل) |
| UAT سطح المكتب الحقيقي (WebView2) | **90/90 PASS** (42 إنتاجي + 23 ديمو + 7 واجهة/ثيمات + 18 حوار حفظ أصلي) |
| `npm audit` / `cargo audit` | 0 ثغرة / 0 ثغرة |

> **ملاحظة قياس (جولة 19 أكتوبر 2026):** أُعيد تشغيل بوابات JS/Rust على المصدر النهائي بعد إصلاحات عقود المزوّدين: JS **202/202**، Rust **622 ناجح / 0 فاشل / 10 مُهملة**. هذا يعكس إضافة 8 اختبارات عقود جديدة (لقطات النماذج الموثّقة، عقد OpenRouter، تصنيف حالات الأخطاء، مطابقة العناوين الموثّقة)، ولا يغيّر أي ادعاء سابق.

للتفاصيل والدلائل: [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md) · [`docs/ai/PROVIDER_SUPPORT_MATRIX.md`](ai/PROVIDER_SUPPORT_MATRIX.md) · [`docs/legal/THIRD_PARTY_LICENSE_INVENTORY.md`](legal/THIRD_PARTY_LICENSE_INVENTORY.md) · [`SECURITY.md`](../SECURITY.md). تقارير القبول الخام وتقارير التدقيق الهندسي الداخلية محفوظة خارج الشجرة المنشورة بقصد؛ الاستنتاجات ذات الأثر على المستخدم معادة أعلاه.

</div>
