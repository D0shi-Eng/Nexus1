# تكاملات مزوّدي الذكاء الاصطناعي

**ينطبق على:** NEXUS V1 `0.1.0` · **تاريخ مراجعة الوثائق:** 19 أكتوبر 2026.

يضم NEXUS **عشرة أنواع مزوّدين ثابتة**. توضح هذه الصفحة ما ينفّذه كل محوّل بالضبط، وكيف يُعدّ، وما الذي تم التحقق منه فعلاً. ولا تُنسب أي قدرة إلى مزوّد تتجاوز الدليل المذكور هنا.

> **قاعدة التحقق:** لا يُعلن ادعاء إلا بالمستوى الذي قيس فعلاً. و`DOCS_VERIFIED` ليست `LIVE_VERIFIED`. ولا يوجد في هذا المستودع **مفتاح API حقيقي**، ولم يُنفَّذ أي نداء مدفوع أثناء بناء V1 أو اختباره.

---

## 1. مستويات التحقق

| المستوى | المعنى | كيف أُثبت |
|---|---|---|
| `DOCS_VERIFIED` | العنوان الأساسي والمسارات والمصادقة مقروءة في الوثائق الرسمية الحالية للجهة | مراجعة الوثائق الرسمية (المضيف مذكور لكل مزوّد) |
| `CONFIGURATION_VERIFIED` | NEXUS يبني العنوان الكامل الموثّق حرفياً (origin + path) | اختبارات `presets.rs` |
| `CONTRACT_TESTED` | عقد الطلب/الاستجابة مُختبَر على **خادم HTTP وهمي محلي** | `*_tests.rs` بمقابس محلية حقيقية بلا مفاتيح |
| `REACHABILITY_VERIFIED` | اتصال فعلي أثبت وجود المسار (401/400 لا 404) | فحص بتاريخ 15 أكتوبر 2026 |
| `AUTHENTICATION_VERIFIED` | مفتاح حقيقي قُبل لدى المزوّد | **لا مزوّد سحابي** |
| `LIVE_INFERENCE_VERIFIED` | خرج نموذج حقيقي عاد عبر NEXUS | **Bonsai 2 المحلي فقط** (19 أكتوبر 2026) |
| `TOOL_CALLING_LIVE_VERIFIED` | استدعاء أداة حقيقي نفّذه نموذج حي | **لا مزوّد** |

`CONTRACT_TESTED` تثبت أن NEXUS يفهم العقد الموثّق، ولا تثبت أن المزوّد متاح أو أن المفتاح صالح.

## 2. مصفوفة المزوّدين

| # | المزوّد (النوع) | المحوّل | نوع API | العنوان الأساسي الافتراضي | يحتاج مفتاحاً | تعداد النماذج | المحادثة | البث | استدعاء الأدوات | المخرجات المهيكلة | حالة التحقق | القيود المعروفة |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | **Ollama** (`OLLAMA`) | `providers/ollama.rs` | Ollama الأصلي + سطح `/v1` الموثّق | `http://127.0.0.1:11434` | لا (حلقي) | `GET /api/tags` ✅ موثّق | `POST /api/generate` و`POST /api/chat` ✅ | ❌ غير منفّذ (`stream:false`) | ✅ `/api/chat` + `tools`؛ و**أكثر من استدعاء واحد يُرفض فشلاً مغلقاً** | ✅ `format:"json"` على `/api/generate` | `DOCS` + `CONFIG` + `CONTRACT`؛ **بلا تحقق حيّ عبر هذا المحوّل** | محلي فقط؛ المحوّل يستعمل المسارات الأصلية بينما يوثّق الإعداد سطح `/v1`. |
| 2 | **LM Studio** (`LM_STUDIO`) | `providers/openai_compatible.rs` | متوافق مع OpenAI | `http://127.0.0.1:1234` | لا (حلقي) | `GET /v1/models` ✅ | `POST /v1/chat/completions` ✅ | ❌ | ❌ فشل مغلق | ❌ فشل مغلق | `DOCS` + `CONFIG` + `CONTRACT` | المنفذ الافتراضي موثّق لكنه قابل للتغيير؛ تحقق من العنوان الفعلي. |
| 3 | **خادم متوافق مع OpenAI** (`OPENAI_COMPATIBLE`) | `providers/openai_compatible.rs` | متوافق مع OpenAI | **فارغ** — تدخله أنت | نعم (حتى للحلقي؛ قيد V1 معلن) | `GET /v1/models` | `POST /v1/chat/completions` | ❌ | ❌ | ❌ | `CONFIG` + `CONTRACT`؛ و**`LIVE_INFERENCE_VERIFIED` مع Bonsai 2 المحلي عبر `127.0.0.1`** | يغطي llama.cpp وBonsai 2 وvLLM؛ ويتطلب قيمة اعتماد موضعية للخوادم المحلية غير المصادَقة. |
| 4 | **OpenAI** (`OPENAI`) | `providers/openai_compatible.rs` | Chat Completions | `https://api.openai.com` | نعم — `Authorization: Bearer` | `GET /v1/models` ✅ | `POST /v1/chat/completions` ✅ | ❌ | ❌ | ❌ | `DOCS` + `CONFIG` + `CONTRACT`؛ والحي **PENDING_OWNER_KEY** | يستعمل Chat Completions الموثّقة لا واجهة Responses. |
| 5 | **DeepSeek** (`DEEPSEEK`) | `providers/openai_compatible.rs` | متوافق مع OpenAI | `https://api.deepseek.com` | نعم — Bearer | `GET /v1/models` (بديل `/v1` موثّق) ✅ | `POST /v1/chat/completions` ✅ | ❌ | ❌ | ❌ | `DOCS` + `CONFIG` + `CONTRACT` + **`REACHABILITY_VERIFIED`**؛ والحي **PENDING_OWNER_KEY** | لقطات النماذج تُفحص بصرامة؛ وأجسام الأخطاء تُصنّف بحالة HTTP فقط لأن DeepSeek لا تنشر مخطط خطأ قابلاً للقراءة الآلية. |
| 6 | **Anthropic** (`ANTHROPIC`) | `providers/anthropic.rs` | Anthropic Messages API | `https://api.anthropic.com` | نعم — `x-api-key` + `anthropic-version: 2023-06-01` | `GET /v1/models` ✅ | `POST /v1/messages` ✅ | ❌ (بثّ المزوّد ينتهي بـ`message_stop`؛ غير مستعمل) | ❌ | ❌ | `DOCS` + `CONFIG` + `CONTRACT`؛ والحي **PENDING_OWNER_KEY** | يستعمل `x-api-key` الموثّق كبديل مدعوم؛ ومسار OAuth غير متحقق. |
| 7 | **Qwen / Alibaba Cloud Model Studio** (`QWEN`) | `providers/openai_compatible.rs` | متوافق مع OpenAI (وضع التوافق) | `https://dashscope.aliyuncs.com` (قديم لكن موثّق) | نعم — Bearer (مرتبط بالإقليم) | `GET /compatible-mode/v1/models` ⚠️ **غير موثّق من الجهة** (تحقق وصول فقط) | `POST /compatible-mode/v1/chat/completions` ✅ | ❌ | ❌ | ❌ | `DOCS` (القاعدة + المحادثة) + `CONFIG` + `CONTRACT` + **`REACHABILITY_VERIFIED`**؛ والحي **PENDING_OWNER_KEY** | النطاق الموصى به خاص بمساحة عمل `https://{WorkspaceId}.{region}.maas.aliyuncs.com` ويتطلب معرّف مساحة عمل وغير مضمّن؛ وقد يفشل الاكتشاف ويُعلن فشلاً. |
| 8 | **Z.ai** (`ZAI`) | `providers/openai_compatible.rs` | متوافق مع OpenAI (سطح `v4`) | `https://api.z.ai` | نعم — Bearer | `GET /api/paas/v4/models` ⚠️ **غير موثّق من الجهة** (تحقق وصول فقط) | `POST /api/paas/v4/chat/completions` ✅ | ❌ | ❌ | ❌ | `DOCS` (القاعدة + المحادثة) + `CONFIG` + `CONTRACT` + **`REACHABILITY_VERIFIED`**؛ والحي **PENDING_OWNER_KEY** | الإصدار `v4` لا `v1`؛ ولا يوجد مسار تعداد نماذج في OpenAPI المنشور (14 مساراً)، فقد يفشل الاكتشاف بحكم المزوّد. |
| 9 | **xAI (Grok)** (`XAI`) | `providers/openai_compatible.rs` | Chat Completions | `https://api.x.ai` | نعم — Bearer | `GET /v1/models` ✅ | `POST /v1/chat/completions` ✅ | ❌ | ❌ | ❌ | `DOCS` + `CONFIG` + `CONTRACT`؛ بلا فحص وصول؛ والحي **PENDING_OWNER_KEY** | تصف الوثائق Chat Completions كسلف لواجهة Responses؛ وNEXUS يستهدف السطح الموثّق بالكامل عمداً. |
| 10 | **OpenRouter** (`OPENROUTER`) | `providers/openai_compatible.rs` | Chat Completions | `https://openrouter.ai/api` | نعم — Bearer | `GET /v1/models` ✅ | `POST /v1/chat/completions` ✅ | ❌ | ❌ | ❌ | `DOCS` + `CONFIG` + `CONTRACT`؛ والحي **PENDING_OWNER_KEY** | لم يُنفَّذ أي نداء. التحقق الحي يحتاج مفتاح المالك وسقف تكلفة. |

الدلالة: ✅ منفَّذ وموثّق؛ ❌ غير منفَّذ عمداً، والمحوّل يفشل مغلقاً بـ`CAPABILITY_NOT_IMPLEMENTED` بدل ادعاء القدرة.

## 3. غير منفَّذ في كل المزوّدين

- **البث/SSE** — كل المحوّلات ترسل `stream: false`، والواجهة لا تعرض خرجاً متدفقاً.
- **استدعاء الأدوات خارج مسار Ollama الأصلي** — محوّلا OpenAI-compatible وAnthropic يرفضانه صراحةً.
- **المخرجات المهيكلة خارج مسار Ollama الأصلي** — لا يُرسل `response_format` لمزوّدي OpenAI-compatible لأن الدعم يختلف، ويفشل المحوّل مغلقاً بدل التخمين.
- **النداءات التلقائية أو الوكلاء في الخلفية أو إعادة المحاولة على خدمات مدفوعة** — لا وجود لأي منها؛ كل نداء شبكي إجراء مستخدم صريح.
- **أوزان النماذج** — غير مضمّنة، والمزوّد المحلي مسؤولية المستخدم.

## 4. أمثلة إعداد (قيم موضعية فقط)

```text
نوع المزوّد:    OpenAI-compatible
العنوان:        https://your-provider.example/v1
مفتاح API:      <YOUR_API_KEY>        # يُخزَّن بـWindows DPAPI ولا يُحفظ نصاً صريحاً
```

ولخادم محلي متوافق مع OpenAI:

```text
نوع المزوّد:    OpenAI-compatible
العنوان:        http://127.0.0.1:8080
مفتاح API:      <LOCAL_PLACEHOLDER>   # V1 يتطلب قيمة اعتماد حتى للخوادم المحلية غير المصادَقة
```

لا تُودِع مفاتيح حقيقية في المستودع ولا في التوثيق ولا في الصور.

## 5. الوثائق الرسمية المراجعة

| المزوّد | الوثائق الرسمية المراجعة |
|---|---|
| OpenAI | `developers.openai.com/api/reference` (chat completions، models list) |
| Anthropic | `platform.claude.com/docs/en/api` (messages، models، overview) |
| DeepSeek | `api-docs.deepseek.com` (create chat completion، list models) |
| Qwen / Alibaba | `alibabacloud.com/help/model-studio` (وضع التوافق مع OpenAI) |
| Z.ai | `docs.z.ai` (quick start، chat completion، OpenAPI المنشور) |
| xAI | `docs.x.ai` (chat completions، models) |
| OpenRouter | `openrouter.ai/docs/api-reference` (chat completion، list models) |
| Ollama | `docs.ollama.com/api` (tags، generate، chat، توافق `/v1`) |
| LM Studio | `lmstudio.ai/docs/developer/openai-compat` |

## 6. خصائص أمنية مشتركة لكل المزوّدين

- المفاتيح مشفّرة بـWindows DPAPI (نطاق المستخدم) وتُقرأ لحظة الطلب فقط.
- لا تظهر المفاتيح في SQLite ولا السجلات ولا أحداث التدقيق ولا استجابات IPC.
- كل وجهة تمر بسياسة الشبكة قبل الاتصال: HTTPS لغير الحلقي، وHTTP للحلقي فقط، وبلا بيانات اعتماد في العنوان، ورفض المضيفين الخاصين/link-local/الوصفيين، وتثبيت DNS، وبلا تحويلات، وحدود حجم ومهلة.
- سجلات الموافقة مربوطة بهوية الوجهة بدقة (المزوّد، النوع، المسار، النموذج)؛ وإذا تغيّرت بعد الاعتماد يُرفض التنفيذ.
- صفوف المزوّدين داخل نطاق المستأجر، والاعتماد يُعنون بمعرّف المستأجر + المزوّد.

## 7. خلاصة صادقة

- **عشرة مزوّدين مدمجون على مستوى الوثائق والعقد.**
- **مسار محلي واحد أنتج استنتاجاً حقيقياً** (Bonsai 2 عبر `127.0.0.1` من خلال محوّل OpenAI-compatible).
- **صفر مزوّد سحابي بمصادقة أو استنتاج حي متحقق.**
- **البث واستدعاء الأدوات السحابي والمخرجات المهيكلة السحابية غير منفَّذة** ولا يُعلن عنها.
