# Security Policy — NEXUS

**Applies to:** NEXUS V1 `0.1.0` · **Supported version:** `0.1.0` only (pre-release; no release branches)
**Repository:** `D0shi-Eng/Nexus1` · **License:** Apache-2.0

This document is the authoritative security policy. It is written honestly: NEXUS is not certified, the audit chain is not tamper-proof, and the local database is not encrypted by the application.

---

## 1. Supported versions

| Version | Status |
|---|---|
| `0.1.0` (V1) | Supported. Security fixes are prepared against this line. |
| Any older build or fork | Not supported. |
| V2 | Not started; does not exist. |

There are no release branches and no parallel support for older versions. NEXUS is a pre-release candidate and has no published binary release.

## 2. Security scope

**In scope**

- The Rust core: authentication, sessions, authorization, tenant isolation, audit chain and anchor, backup/restore, report export, and local logging.
- The IPC boundary: all 117 Tauri commands, `capabilities/main-capability.json`, and the CSP in `tauri.conf.json`.
- The SQLite schema and migrations `0001 → 0026`, including tenant-scoping triggers.
- The governed AI path: provider registry, destination policy, approval binding, tool authorization, and the DPAPI credential vault.
- The export/backup path: native save dialog, destination validation, atomic writes.
- Supply chain: `package-lock.json`, `Cargo.lock`, and the dependency inventories.

**Out of scope**

- The security of Ollama, LM Studio, or any external model server, and any model weights.
- The host operating system, local administrator actions, or manual modification of the database or anchor by the device owner.
- Demo/synthetic test workspaces as production data.
- Vulnerabilities in third-party dependencies without a concrete exploitation path inside NEXUS (reported upstream, tracked here as a dependency update).

## 3. Reporting a vulnerability

Use **private coordinated disclosure**:

1. **Preferred channel:** GitHub Security Advisories — "Report a vulnerability" on `D0shi-Eng/Nexus1`. This keeps the report private and creates a tracked fix.
2. While the repository is private, use the repository's private collaboration/security channel with the maintainer. **Do not** open a public issue or pull request describing the vulnerability.
3. Include: affected version (`0.1.0`), impact, reproduction steps on a synthetic/test workspace, and evidence without real personal data.
4. **Coordination window:** 90 days from acknowledgement before any public discussion, extendable by agreement while a fix is in progress.
5. Do not test against real user data, do not use social engineering or denial of service, and do not access systems you do not own.

**Commitment:** acknowledgement of receipt, a tracked fix, and notification when a corrective release is prepared. There is **no bug-bounty program** and no monetary commitment.

**No personal e-mail address is published.** Do not invent one; use the GitHub channel above.

## 4. Implemented security properties

| Area | Property |
|---|---|
| Passwords | Argon2id hashing; 12–128 character policy; progressive lockout (5/10/15 attempts → 60/300/900 s) |
| Sessions | Production sessions last 12 hours; every permission check is bound to the exact active session and its `expires_at`/`revoked_at` |
| RBAC | Session-bound role/permission checks in Rust; production roles `TENANT_ADMIN`, `CONTRIBUTOR`, `REVIEWER`; no self role modification; no privilege escalation when granting roles |
| Tenant isolation | `tenant_id` on business tables, tenant-filtered repositories, and SQLite triggers rejecting cross-tenant re-parenting |
| Independent approvals | The requester cannot decide their own approval; the evidence registrant cannot verify their own evidence (backend-enforced) |
| Audit | SHA-256 hash chain with previous-hash linking and a local anchor; real verification command; detects modification, deletion, reordering, actor/tenant substitution, and tail truncation |
| Secrets | AI provider credentials encrypted with Windows DPAPI (user scope); never in SQLite, logs, audit events, or IPC responses |
| Network | HTTPS off-loopback; HTTP only on `127.0.0.1`; no credentials in URLs; private/link-local/descriptive hosts rejected; pinned DNS; no redirects; bounded size/timeouts |
| IPC | 117 typed commands; serde validation; minimal capabilities (`core:default`, `dialog:allow-open`, `dialog:allow-save`); local-only CSP |
| Export | Native save dialog; destination policy; atomic bounded write; one audit event per export |

## 5. Known security limitations (declared)

- **Audit trust boundary:** a party who controls the whole device can rewrite the SQLite database and the local anchor consistently. The anchor is local; it is not an external timestamp or hardware root of trust.
- **Encryption at rest:** the SQLite database is not encrypted by NEXUS. Full-disk encryption and OS account security are the user's responsibility.
- **Unsigned binaries:** the V1 build is `NotSigned`; no code-signing certificate is distributed.
- **No MFA/SSO:** authentication is local to the device.
- **No independent certification:** no penetration test, security certification, or compliance attestation has been performed. All V1 verification is internal.
- **AI prompt injection:** governed tool/approval boundaries reduce impact, but no claim is made that a language model cannot be manipulated.
- **Advisories:** `cargo audit` reports two informational advisories that are not present in the Windows target; they are documented rather than hidden.

## 6. Rules for contributors

- Never commit API keys, tokens, passwords, private keys, signing certificates, `.env` files, or local databases.
- Do not add remote runtime resources (CDN, fonts, scripts); CSP forbids them and local supply is a design requirement.
- Any change to authorization, authentication, sessions, or capabilities requires a regression test.
- Absolute developer paths (`C:\Users\...`, host names, account names) must not appear in source, tests, docs, screenshots, or commit messages.
- Report and fix security issues through the private channel, not in public review threads.

---

<div dir="rtl">

# سياسة الأمن — NEXUS

**ينطبق على:** NEXUS V1 `0.1.0` · **الإصدار المدعوم:** `0.1.0` فقط (ما قبل الإصدار؛ لا فروع إصدار)
**المستودع:** `D0shi-Eng/Nexus1` · **الترخيص:** Apache-2.0

هذا المستند هو سياسة الأمن الملزمة. وهو مكتوب بصدق: NEXUS غير معتمد، وسلسلة التدقيق ليست مانعة للعبث، وقاعدة البيانات المحلية غير مشفّرة بواسطة التطبيق.

## 1. الإصدارات المدعومة

| الإصدار | الحالة |
|---|---|
| `0.1.0` (V1) | مدعوم. تُجهَّز إصلاحات الأمن لهذا الخط. |
| أي بناء أقدم أو نسخة متفرعة | غير مدعوم. |
| V2 | لم يبدأ؛ غير موجود. |

لا توجد فروع إصدار ولا دعم متوازٍ لنسخ قديمة. NEXUS مرشّح ما قبل إصدار ولا يوجد إصدار ثنائي منشور.

## 2. نطاق الأمن

**داخل النطاق**

- نواة Rust: المصادقة والجلسات والتفويض وعزل المستأجرين وسلسلة التدقيق والمرساة والنسخ الاحتياطي والاستعادة وتصدير التقارير والسجل المحلي.
- حدود IPC: الأوامر الـ117 في Tauri، و`capabilities/main-capability.json`، وسياسة CSP في `tauri.conf.json`.
- مخطط SQLite والهجرات `0001 → 0026` بما فيها مشغّلات نطاق المستأجر.
- مسار الذكاء الاصطناعي المحكوم: سجل المزوّدين وسياسة الوجهة وربط الموافقة وتفويض الأدوات وخزنة DPAPI.
- مسار التصدير/النسخ الاحتياطي: حوار الحفظ الأصلي والتحقق من الوجهة والكتابة الذرّية.
- سلسلة التوريد: `package-lock.json` و`Cargo.lock` وجرود التبعيات.

**خارج النطاق**

- أمن Ollama أو LM Studio أو أي خادم نموذج خارجي وأي أوزان نماذج.
- نظام التشغيل المضيف أو إجراءات مدير الجهاز أو التعديل اليدوي لقاعدة البيانات أو المرساة من مالك الجهاز.
- مساحات الديمو/الاختبار التركيبية كبيانات إنتاج.
- ثغرات التبعيات الخارجية دون مسار استغلال ملموس داخل NEXUS (تُبلَّغ للجهة الأصلية وتُتابع هنا كتحديث تبعية).

## 3. الإبلاغ عن ثغرة

يُستخدم **الإفصاح الخاص المنسَّق**:

1. **المفضّل:** GitHub Security Advisories — «Report a vulnerability» على `D0shi-Eng/Nexus1`. يبقي التقرير خاصاً وينشئ إصلاحاً متتبَّعاً.
2. إلى أن يصبح المستودع عاماً، استخدم قناة التعاون/الأمن الخاصة في المستودع مع المشرف. **لا** تفتح Issue أو PR عاماً يصف الثغرة.
3. ضمّن: الإصدار المتأثر (`0.1.0`)، والأثر، وخطوات إعادة الإنتاج على مساحة تركيبية/اختبارية، ودليلاً بلا بيانات شخصية حقيقية.
4. **مهلة التنسيق:** 90 يوماً من الإقرار بالاستلام قبل أي حديث عام، وتُمدّ بالاتفاق إذا كان الإصلاح جارياً.
5. لا تختبر بيانات مستخدم حقيقية، ولا تُجرِ هندسة اجتماعية أو حجب خدمة، ولا تصل إلى أنظمة لا تملكها.

**التزامنا:** الإقرار بالاستلام، وتتبّع الإصلاح، وإشعارك عند تجهيز إصدار مصحّح. ولا يوجد **برنامج مكافآت** ولا التزام مالي.

**لا يوجد بريد شخصي منشور.** لا تخترع عنواناً؛ استخدم قناة GitHub أعلاه.

## 4. الخصائص الأمنية المنفَّذة

| المجال | الخاصية |
|---|---|
| كلمات المرور | تجزئة Argon2id؛ سياسة 12–128 حرفاً؛ قفل تدريجي (5/10/15 محاولة ← 60/300/900 ثانية) |
| الجلسات | جلسة إنتاج 12 ساعة؛ وكل فحص صلاحية مربوط بالجلسة النشطة بعينها و`expires_at`/`revoked_at` |
| RBAC | فحص أدوار وصلاحيات مربوط بالجلسة في Rust؛ الأدوار الإنتاجية `TENANT_ADMIN` و`CONTRIBUTOR` و`REVIEWER`؛ لا تعديل للذات ولا تصعيد عند منح الأدوار |
| عزل المستأجرين | `tenant_id` في جداول الأعمال، ومستودعات مفلترة بالمستأجر، ومشغّلات SQLite ترفض نقل الملكية بين المستأجرين |
| موافقات مستقلة | الطالب لا يقرر موافقته؛ ومقدّم الدليل لا يتحقق من دليله (مفروض في الخلف) |
| التدقيق | سلسلة SHA-256 بربط التجزئة السابقة ومرساة محلية؛ وأمر تحقق فعلي؛ يكشف التعديل والحذف وإعادة الترتيب وتبديل الفاعل/المستأجر واقتطاع الذيل |
| الأسرار | اعتمادات AI مشفّرة بـWindows DPAPI (نطاق المستخدم)؛ لا تُخزَّن في SQLite ولا السجلات ولا التدقيق ولا استجابات IPC |
| الشبكة | HTTPS لغير الحلقي؛ وHTTP على `127.0.0.1` فقط؛ وبلا بيانات اعتماد في العنوان؛ ورفض المضيفين الخاصين/link-local/الوصفيين؛ وتثبيت DNS؛ وبلا تحويلات؛ وحدود حجم/مهلة |
| IPC | 117 أمراً مطبوعاً؛ تحقق serde؛ قدرات دنيا (`core:default`، `dialog:allow-open`، `dialog:allow-save`)؛ وCSP محلية فقط |
| التصدير | حوار حفظ أصلي؛ وسياسة وجهة؛ وكتابة ذرّية محدودة؛ وحدث تدقيق واحد لكل تصدير |

## 5. قيود أمنية معروفة (معلنة)

- **حدود ثقة التدقيق:** من يتحكم بالجهاز كاملاً يستطيع إعادة كتابة قاعدة SQLite والمرساة المحلية بشكل متسق. المرساة محلية وليست ختماً زمنياً خارجياً ولا جذر ثقة عتادياً.
- **التشفير عند السكون:** قاعدة SQLite غير مشفّرة بواسطة NEXUS. وتشفير القرص الكامل وأمن حساب النظام مسؤولية المستخدم.
- **ثنائيات غير موقّعة:** بناء V1 `NotSigned`، ولا شهادة توقيع موزّعة.
- **لا MFA/SSO:** المصادقة محلية على الجهاز.
- **لا شهادة مستقلة:** لم يُجرَ اختبار اختراق أو شهادة أمنية أو إقرار امتثال. وكل تحقق V1 داخلي.
- **حقن التوجيه في AI:** حدود الأدوات والموافقات تخفف الأثر، لكن لا يُدَّعى أن نموذج اللغة غير قابل للتلاعب.
- **التحذيرات:** يُبلغ `cargo audit` عن تحذيرين معلوماتيين غائبين عن هدف Windows؛ وهما موثّقان لا مخفيان.

## 6. قواعد المساهمين

- لا تُودِع مفاتيح API ولا رموزاً ولا كلمات مرور ولا مفاتيح خاصة ولا شهادات توقيع ولا ملفات `.env` ولا قواعد بيانات محلية.
- لا تُضف موارد وقت تشغيل بعيدة (CDN أو خطوط أو سكربتات)؛ فـCSP تمنعها والتوريد المحلي شرط تصميمي.
- أي تغيير في التفويض أو المصادقة أو الجلسات أو القدرات يتطلب اختبار انحدار.
- المسارات المطلقة للمطور (`C:\Users\...` وأسماء المضيفين والحسابات) يجب ألا تظهر في الشيفرة أو الاختبارات أو التوثيق أو الصور أو رسائل الالتزام.
- أبلغ عن المشكلات الأمنية وأصلحها عبر القناة الخاصة لا في خيوط المراجعة العامة.

</div>
