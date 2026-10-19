<div dir="rtl">

# NEXUS V1 — مصفوفة دعم مزوّدي الذكاء الاصطناعي ومستويات التحقق

**آخر مراجعة للوثائق الرسمية:** 19 أكتوبر 2026 · **الإصدار:** `0.1.0` · **الترخيص:** Apache-2.0

> **قاعدة الصدق الحاكمة:** لا يُمنح أي مستوى تحقق دون دليله. `DOCS_VERIFIED` ليست `LIVE_VERIFIED`.
> لا يوجد في هذا المستودع مفتاح API حقيقي، ولم يُنفَّذ أي نداء مدفوع في بناء V1 أو اختباره.

---

## 1. مستويات التحقق المستخدمة

| المستوى | المعنى | كيف يُثبت |
|---|---|---|
| `DOCS_VERIFIED` | العنوان الأساسي والمسارات والمصادقة مقروءة في **الوثائق الرسمية الحالية** للجهة المالكة | مراجعة مضيف الوثيقة الرسمي (مذكور في الجدول أدناه) |
| `CONFIGURATION_VERIFIED` | تكوين NEXUS ينتج العنوان الكامل الصحيح (origin + path) حرفياً | اختبارات `presets.rs` |
| `CONTRACT_TESTED` | عقد الطلب/الاستجابة (الحقول، المصادقة، الأخطاء، الحدود) مُختبَر على **خادم HTTP وهمي محلي** | `*_tests.rs` بمقابس محلية حقيقية، بلا مفاتيح |
| `REACHABILITY_VERIFIED` | اتصال فعلي من جهاز التطوير أثبت وجود المسار (401/400 لا 404) بتاريخ 15 أكتوبر 2026 | قياس سابق موثّق؛ **ليس** تحققاً للعقد |
| `AUTHENTICATION_VERIFIED` | مفتاح حقيقي قُبل لدى المزوّد | **غير متحقق لأي مزوّد سحابي** |
| `LIVE_INFERENCE_VERIFIED` | توليد نص حقيقي عاد من المزوّد عبر مسار NEXUS المحكوم | **Bonsai 2 المحلي فقط** (19 أكتوبر 2026) |
| `TOOL_CALLING_LIVE_VERIFIED` | استدعاء أداة حقيقي من نموذج حي عبر مسار الأدوات | **غير متحقق لأي مزوّد** |

> `CONTRACT_TESTED` لا تعني أن المزوّد يعمل اليوم: الخادم الوهمي يثبت أن NEXUS **يفهم العقد الموثّق**، لا أن المزوّد متاح أو أن المفتاح صالح.

---

## 2. جرد المزوّدين الفعلي في المصدر (10 أنواع)

المصدر الوحيد للحقيقة: `apps/desktop/src-tauri/src/domain/provider_registry.rs` (الأنواع) و`infrastructure/providers/presets.rs` (العناوين) و`application/ai_provider_registry_service.rs` (بناء المحوّل والربط بالموافقة).

| # | النوع (`kind`) | المحوّل | الواجهة الأساسية | المصادقة | تعداد النماذج | المحادثة |
|---|---|---|---|---|---|---|
| 1 | `OLLAMA` | `ollama.rs` (واجهة Ollama الأصلية) | `http://127.0.0.1:11434` | بلا مفتاح (حلقي) | `GET /api/tags` ✅ موثّق | `POST /api/generate` · `POST /api/chat` ✅ |
| 2 | `LM_STUDIO` | `openai_compatible.rs` | `http://127.0.0.1:1234` | بلا مفتاح (حلقي) | `GET /v1/models` ✅ | `POST /v1/chat/completions` ✅ |
| 3 | `OPENAI_COMPATIBLE` | `openai_compatible.rs` | **يدخله المستخدم** (لا عنوان مخترع) | مفتاح (أو قيمة موضعية لخادم محلي) | `GET /v1/models` | `POST /v1/chat/completions` |
| 4 | `OPENAI` | `openai_compatible.rs` | `https://api.openai.com` | `Authorization: Bearer` | `GET /v1/models` ✅ | `POST /v1/chat/completions` ✅ |
| 5 | `DEEPSEEK` | `openai_compatible.rs` | `https://api.deepseek.com` | `Authorization: Bearer` | `GET /v1/models` (عبر بديل `/v1` المتوافق) ✅ | `POST /v1/chat/completions` ✅ |
| 6 | `ANTHROPIC` | `anthropic.rs` (واجهة الرسائل الخاصة) | `https://api.anthropic.com` | `x-api-key` + `anthropic-version: 2023-06-01` ✅ | `GET /v1/models` ✅ | `POST /v1/messages` ✅ |
| 7 | `QWEN` | `openai_compatible.rs` | `https://dashscope.aliyuncs.com` (نطاق قديم موثّق؛ الموصى به خاص بمساحة العمل) | `Authorization: Bearer` | `GET /compatible-mode/v1/models` ⚠️ **غير موثّق** (تحقق وصول فقط) | `POST /compatible-mode/v1/chat/completions` ✅ |
| 8 | `ZAI` | `openai_compatible.rs` | `https://api.z.ai` (إصدار `v4` لا `v1`) | `Authorization: Bearer` | `GET /api/paas/v4/models` ⚠️ **غير موثّق** (تحقق وصول فقط) | `POST /api/paas/v4/chat/completions` ✅ |
| 9 | `XAI` | `openai_compatible.rs` | `https://api.x.ai` | `Authorization: Bearer` | `GET /v1/models` ✅ | `POST /v1/chat/completions` ✅ |
| 10 | `OPENROUTER` | `openai_compatible.rs` | `https://openrouter.ai/api` | `Authorization: Bearer` | `GET /v1/models` ✅ | `POST /v1/chat/completions` ✅ |

**ملاحظات الجرد:**
- `llama.cpp` و`Bonsai 2` و`vLLM` وأي خادم متوافق: تُستخدم عبر `OPENAI_COMPATIBLE` بعنوان يدخله المستخدم (حلقي فقط عبر `http`).
- `Zhipu / open.bigmodel.cn` **ليست** ضمن المزوّدين؛ `ZAI` يخص `api.z.ai` وحده.
- لا توجد أنواع أخرى في المصدر. أي اسم غير مذكور هنا غير مدعوم ولا يُدّعى.

---

## 3. القدرات — ما هو منفَّذ فعلاً وما ليس كذلك

| القدرة | Ollama (محلي) | OpenAI-compatible (كل السحابيين + LM Studio + llama.cpp) | Anthropic |
|---|---|---|---|
| توليد نص غير متدفق | ✅ منفَّذ ومختبَر | ✅ منفَّذ ومختبَر (لقطات النماذج مقبولة) | ✅ منفَّذ ومختبَر |
| **البث (Streaming/SSE)** | ❌ **غير منفَّذ** — الطلب يرسل `"stream": false` دائماً | ❌ **غير منفَّذ** | ❌ **غير منفَّذ** |
| استدعاء الأدوات (Tool calling) | ✅ `/api/chat` + `tools`؛ أكثر من استدعاء واحد يُرفض فشلاً مغلقاً | ❌ **فشل مغلق** `CAPABILITY_NOT_IMPLEMENTED` | ❌ **فشل مغلق** |
| مخرجات مهيكلة (JSON) | ✅ `format: "json"` على `/api/generate` | ❌ **فشل مغلق** (لا `response_format`؛ دعمه يختلف بين المزوّدين) | ❌ **فشل مغلق** |
| تعداد النماذج | ✅ `/api/tags` + قدرات | ✅ `/v1/models` (حيث يُوثّق) | ✅ `/v1/models` |
| كشف القدرات في الواجهة | من `/api/tags` (`capabilities`) | `null` = غير معروف (لا ادعاء `false`) | `null` = غير معروف |

**قاعدة معلنة:** عدم تنفيذ البث والأدوات في المحوّل المتوافق مع OpenAI **قرار صريح** لا نقص مخفي: دعم هذه القدرات يختلف جوهرياً بين المزوّدين، والادعاء الكاذب أخطر من عدم الدعم. الواجهة تُظهر الفشل المغلق ولا تدّعي نجاحاً.

---

## 4. حالة التحقق لكل مزوّد (لا يُرفع مستوى بلا دليل)

| النوع | DOCS_VERIFIED | CONFIGURATION_VERIFIED | CONTRACT_TESTED | REACHABILITY | LIVE |
|---|---|---|---|---|---|
| `OLLAMA` | ✅ | ✅ | ✅ (خادم وهمي) | — (محلي) | ✅ `LIVE_INFERENCE_VERIFIED` عبر Bonsai 2/llama.cpp لا عبر Ollama نفسه |
| `LM_STUDIO` | ✅ | ✅ | ✅ (العقد المشترك) | — (محلي) | ❌ |
| `OPENAI_COMPATIBLE` | ✅ (عقد OpenAI) | ✅ | ✅ (خادم وهمي) | ✅ محلي حيّ (Bonsai 2) | ✅ `LIVE_INFERENCE_VERIFIED` (Bonsai 2) |
| `OPENAI` | ✅ | ✅ | ✅ | ❌ | ❌ `PENDING_OWNER_KEY` |
| `DEEPSEEK` | ✅ | ✅ | ✅ | ✅ (15 أكتوبر، 401 لا 404) | ❌ `PENDING_OWNER_KEY` |
| `ANTHROPIC` | ✅ | ✅ | ✅ | ❌ | ❌ `PENDING_OWNER_KEY` |
| `QWEN` | ✅ (قاعدة + محادثة) · ⚠️ تعداد النماذج غير موثّق | ✅ | ✅ | ✅ (15 أكتوبر) | ❌ `PENDING_OWNER_KEY` |
| `ZAI` | ✅ (قاعدة + محادثة) · ⚠️ تعداد النماذج غير موثّق | ✅ | ✅ | ✅ (15 أكتوبر) | ❌ `PENDING_OWNER_KEY` |
| `XAI` | ✅ | ✅ | ✅ | ❌ | ❌ `PENDING_OWNER_KEY` |
| `OPENROUTER` | ✅ | ✅ | ✅ (عقد OpenRouter) | ❌ | ❌ `PENDING_OWNER_KEY` |

**خلاصة صريحة:** لا مزوّد سحابي واحد يملك `AUTHENTICATION_VERIFIED` أو `LIVE_INFERENCE_VERIFIED` أو `TOOL_CALLING_LIVE_VERIFIED` في هذا الإصدار.

---

## 5. مصادر الوثائق الرسمية المراجعة

| المزوّد | الوثيقة الرسمية المقروءة |
|---|---|
| OpenAI | <https://developers.openai.com/api/reference/resources/chat> · <https://developers.openai.com/api/reference/resources/models/methods/list> |
| Anthropic | <https://platform.claude.com/docs/en/api/messages> · <https://platform.claude.com/docs/en/api/models/list> · <https://platform.claude.com/docs/en/api/overview> |
| DeepSeek | <https://api-docs.deepseek.com/> · <https://api-docs.deepseek.com/api/create-chat-completion> · <https://api-docs.deepseek.com/api/list-models> |
| Qwen / Alibaba | <https://www.alibabacloud.com/help/en/model-studio/compatibility-of-openai-with-dashscope> |
| Z.ai | <https://docs.z.ai/guides/overview/quick-start> · <https://docs.z.ai/api-reference/llm/chat-completion> · <https://docs.z.ai/openapi.json> |
| xAI | <https://docs.x.ai/developers/rest-api-reference/inference/chat-completions> · <https://docs.x.ai/developers/rest-api-reference/inference/models> |
| OpenRouter | <https://openrouter.ai/docs/api-reference/overview> · <https://openrouter.ai/docs/api-reference/chat-completion> · <https://openrouter.ai/docs/api-reference/list-available-models> |
| Ollama | <https://raw.githubusercontent.com/ollama/ollama/main/docs/api.md> (النسخة المصدرية الرسمية قبل الانتقال إلى docs.ollama.com) |
| LM Studio | <https://lmstudio.ai/docs/developer/openai-compat> |
| llama.cpp | عبر `OPENAI_COMPATIBLE` — لا عنوان افتراضي في NEXUS |

---

## 6. فروق موثّقة بين الوثائق والتنفيذ (معلنة، لا مخفية)

1. **Anthropic — المصادقة:** الوثائق الحالية تُقدّم `Authorization: Bearer` كمسار أساسي و`x-api-key` كبديل «legacy» مدعوم. NEXUS يستعمل `x-api-key` (مدعوم رسمياً) ويضيف `anthropic-version` الإلزامية. لم يُتحقق من مسار OAuth.
2. **Anthropic — البث:** ينتهي بـ`message_stop` لا `[DONE]`. لا يهم حالياً لأن NEXUS لا يبثّ.
3. **OpenAI/xAI — الأخطاء:** الشكل الموثّق يختلف بينهما (`{"error":{...}}` مقابل `{"code","error"}`). NEXUS **يتجاهل جسم الخطأ بالكامل** ويصنّف بحالة HTTP فقط، فلا يعتمد على شكل غير مضمون ولا يسرّب نصاً.
4. **DeepSeek — لقطة النموذج:** `model` في الاستجابة قد يُعيد لقطة مؤرّخة؛ `ModelId::accepts_reported` تقبل لقطة موثّقة رقمية فقط وترفض أي نموذج مختلف فعلاً.
5. **Qwen — النطاقات:** الموصى به الآن نطاق خاص بمساحة العمل (`https://{WorkspaceId}.{region}.maas.aliyuncs.com`)، والنطاق القديم `dashscope.aliyuncs.com` ما زال موثّقاً كمتاح. المفاتيح مرتبطة بالإقليم.
6. **Qwen — الأدوات والبث:** الوثائق تمنع استخدام الأدوات مع `stream=true`؛ NEXUS لا يبثّ أصلاً.
7. **Z.ai — تعداد النماذج:** لا يوجد مسار تعداد نماذج في الوثائق ولا في مواصفة OpenAPI (14 مساراً). المسار المستعمل في NEXUS أُثبت بالوصول فقط (401 بخطأ z.ai لا 404) ⇒ قد يفشل الاكتشاف ويُعلن الفشل بدل اختراع قائمة.
8. **xAI/OpenAI — التوصية:** الوثائق تصف `chat/completions` كسلف لواجهة `Responses`. NEXUS يبني على `chat/completions` الموصوفة بالكامل، وهذا اختيار مسجّل لا ادعاء حداثة.
9. **LM Studio — المنفذ:** الافتراضي 1234 موثّق، لكنه قابل للتغيير من إعدادات LM Studio؛ الواجهة تنبّه لذلك.
10. **الخوادم المحلية غير المصادَقة:** `OPENAI_COMPATIBLE` يتطلب مادة سرية في العقد الحالي حتى للخادم الحلقي الذي لا يطلبها (llama.cpp/Bonsai)؛ الحل الحالي قيمة موضعية محلية. **قيود معلنة** مسجّلة في `PROJECT_STATE` ولا تُخفى.

---

## 7. الأمان (ثابت في كل المزوّدين)

- المفتاح يُقرأ من خزنة نظام التشغيل (DPAPI) لحظة الطلب فقط؛ لا يُسجَّل ولا يعود عبر IPC.
- كل عنوان يمر عبر سياسة الوجهة: `https` إلزامي لغير الحلقي، `http` للحلقي وحده، رفض بيانات الاعتماد داخل العنوان، رفض النطاقات الخاصة/link-local/الوصفية، تثبيت DNS بعد التحقق (منع DNS rebinding)، بلا تحويلات، وحدود حجم ومهلة.
- لا تفعيل تلقائي لأي مزوّد؛ كل نداء شبكي صريح من المستخدم.
- ربط الموافقة يحمل هوية الوجهة (provider + kind + endpoint + model) ويُرفض التنفيذ إذا تغيّرت بعد الاعتماد.

</div>

---

# English summary

- **Provider inventory (source of truth):** ten fixed kinds — `OLLAMA`, `LM_STUDIO`, `OPENAI_COMPATIBLE`, `OPENAI`, `DEEPSEEK`, `ANTHROPIC`, `QWEN`, `ZAI`, `XAI`, `OPENROUTER`. `llama.cpp` / `Bonsai 2` / vLLM are supported through `OPENAI_COMPATIBLE`.
- **Verification levels:** `DOCS_VERIFIED` and `CONFIGURATION_VERIFIED` and `CONTRACT_TESTED` (mock HTTP) for all ten; `REACHABILITY_VERIFIED` only for DeepSeek/Qwen/Z.ai (15 Oct 2026, 401-not-404 probe); `LIVE_INFERENCE_VERIFIED` only for local Bonsai 2 (19 Oct 2026); `AUTHENTICATION_VERIFIED`, `TOOL_CALLING_LIVE_VERIFIED` and any cloud `LIVE_INFERENCE_VERIFIED` are **NOT verified**.
- **Not implemented, not claimed:** streaming (all providers request `stream: false`), tool calling and structured output outside the native Ollama path (fail closed).
- **OpenRouter:** documentation-verified and contract-tested; live verification `PENDING_OWNER_KEY`. No external request has been made.
- **Honest gaps:** Z.ai and Qwen model-listing endpoints are not documented by their vendors; DeepSeek publishes no machine-readable error schema; Anthropic `x-api-key` is documented as a legacy-but-supported fallback.
- **Security:** OS-backed credential vault, strict SSRF/network policy with pinned DNS, no redirects, no implicit network activity, approval bound to the exact destination and model.
