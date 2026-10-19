# Application screenshots

These screenshots are real captures of **NEXUS V1 `0.1.0`** running in an isolated demo container. They are source documentation only — they are not bundled inside the application.

- العربية: [نظرة عامة بالعربية](#نظرة-عامة-بالعربية)

## How they were produced (verifiable facts)

| Property | Value |
|---|---|
| Application binary | Debug build of the **current source tree** (Tauri/WebView2), built with `tools/e2e/uat-harness.conf.json` |
| Application identifier | `com.nexus.obligationos.uatharness` — a **different** identity from the live product (`com.nexus.obligationos`) |
| Data directory | `%LOCALAPPDATA%\com.nexus.obligationos.uatharness` — never the user's live database |
| Data mode | `DEMO_SYNTHETIC`; every window carries the **DEMO / SYNTHETIC DATA** label |
| Synthetic users | `demo.admin@nexus.local`, `demo.operator@nexus.local`, `demo.reviewer@nexus.local` |
| Synthetic records | Demo tenant/sources/obligations/tasks/evidence/approvals, plus one synthetic evidence record, one synthetic task, and the resulting pending approval created through the governed IPC path |
| Window | 1360 × 880 client area, devicePixelRatio 1 |
| Capture method | Playwright over the real WebView2 CDP endpoint; content-only screenshots (no fabricated UI, no image composition) |
| Verification | 26 captures, 0 console errors, 0 page errors, 0 horizontal-overflow failures; binary scan found no personal path strings in any PNG |

The demo data is intentionally artificial and explicitly labelled. No real account, company document, API key, personal Windows path, or session credential appears in any image.

## Coverage

| File | Screen | Language | Alt text |
|---|---|---|---|
| `01-overview-en.png` | Executive Overview | English LTR | Executive overview showing readiness estimate, verified coverage, evidence completeness and the priority queue for synthetic demo data |
| `01-overview-ar.png` | نظرة تنفيذية | العربية RTL | اللوحة التنفيذية بالعربية: تقدير الجاهزية وتغطية التحقق واكتمال الأدلة وقائمة الأولويات ببيانات ديمو تركيبية |
| `02-obligations-ar.png` | الالتزامات | العربية RTL | سجل الالتزامات بالعربية مع المصادر والحالات |
| `02-obligations-en.png` | Obligations | English LTR | Obligations register with source references and statuses |
| `03-regulatory-intelligence-ar.png` | الذكاء التنظيمي | العربية RTL | المصادر التنظيمية المسجلة ووسوم كونها مقدَّمة من المستخدم |
| `03-regulatory-intelligence-en.png` | Regulatory Intelligence | English LTR | Registered regulatory sources and their user-provided provenance |
| `04-risks-ar.png` | المخاطر | العربية RTL | سجل المخاطر للقراءة فقط |
| `05-evidence-ar.png` | الأدلة | العربية RTL | سجل الأدلة المرجعية وحالات التحقق |
| `05-evidence-en.png` | Evidence | English LTR | Evidence register with verification states |
| `06-tasks-ar.png` | المهام | العربية RTL | سجل المهام ودورة حياتها |
| `06-tasks-en.png` | Tasks | English LTR | Task register with lifecycle statuses and approval state |
| `07-controls-ar.png` | الضوابط | العربية RTL | سجل الضوابط وتقييم الفعالية |
| `08-agents-ar.png` | الوكلاء | العربية RTL | الوكلاء المحكومون وحدود التنفيذ |
| `09-knowledge-ar.png` | المعرفة | العربية RTL | مصادر المعرفة والاقتباسات |
| `10-reports-ar.png` | التقارير | العربية RTL | تعريفات التقارير المحلية الستة |
| `11-audit-ar.png` | التدقيق | العربية RTL | سلسلة أحداث التدقيق وحالة التحقق |
| `11-audit-en.png` | Audit | English LTR | Hash-chained audit events and chain verification state |
| `12-organization-ar.png` | المؤسسة | العربية RTL | الوضع التشغيلي المشتق للمؤسسة |
| `13-integrations-ar.png` | التكاملات | العربية RTL | حالة التكاملات والمزوّدين |
| `13-integrations-en.png` | Integrations | English LTR | Provider and integration status |
| `14-settings-general-ar.png` | الإعدادات — عام | العربية RTL | قسم الإعدادات العام ووضع التشغيل |
| `14-settings-general-en.png` | Settings — General | English LTR | General settings and active runtime mode |
| `15-ai-providers-ar.png` | مزوّدو الذكاء الاصطناعي | العربية RTL | سجل المزوّدين مع حالة "غير متحقق بعد" |
| `15-ai-providers-en.png` | AI Providers | English LTR | Provider registry showing the built-in local provider as not yet verified |
| `16-approvals-attention-ar.png` | الموافقات في قائمة الاهتمام | العربية RTL | موافقة معلّقة ضمن قائمة ما يحتاج إلى اهتمام |
| `17-approval-dialog-ar.png` | حوار قرار الموافقة | العربية RTL | حوار قرار الموافقة بسبب القرار، مع تأكيد أن القرار يُسجَّل في التدقيق |

---

<div dir="rtl">

## نظرة عامة بالعربية

هذه اللقطات التقطت من **NEXUS V1 `0.1.0`** أثناء تشغيله في حاوية ديمو معزولة، وهي توثيق للمصدر فقط ولا تُضمَّن داخل التطبيق.

- **البناء:** بناء تصحيحي من **شجرة المصدر الحالية** بمعرّف تطبيق مختلف (`com.nexus.obligationos.uatharness`).
- **البيانات:** ديمو تركيبية بحتة، وكل نافذة تحمل وسوم **DEMO / SYNTHETIC DATA**؛ والمستخدمون `demo.*@nexus.local` فقط.
- **العزل:** مجلد بيانات منفصل تماماً، ولم تُفتح قاعدة المستخدم الحيّة في أي مرحلة.
- **الطريقة:** التقاط عبر CDP من WebView2 الحقيقي، بلا تركيب صور ولا تعديل واجهة.
- **التحقق:** 26 لقطة، وصفر أخطاء console أو page، وصفر تجاوز أفقي، وفحص بايتات الصور لم يجد أي مسار شخصي.

لا تظهر في أي صورة بيانات مستخدم حقيقية أو حساب شخصي أو مفتاح API أو مستند شركة أو مسار Windows شخصي أو بيانات جلسة.

</div>
