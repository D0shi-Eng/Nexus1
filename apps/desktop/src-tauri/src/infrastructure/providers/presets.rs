// =====================================================================
// إعدادات الموفرين الجاهزة (Presets)
// ---------------------------------------------------------------------
// كل إعداد يقدّم عنواناً أساسياً ومسارات وصفية ونوع السر المطلوب فقط.
// قاعدة ملزمة: **لا يُخترع عنوان**. كل عنوان هنا إمّا موثّق رسمياً من الجهة
// المالكة (مذكور اسم الموثِّق)، وإمّا يُترك **فارغاً** ويُطلب من المستخدم
// إدخاله بنفسه مع تصريح صريح بعدم التحقق. المسارات (`/v1/models` و
// `/v1/chat/completions`) هي عقد OpenAI المعلن، ولا تُضاف إلا مع عنوان موثّق.
//
// تصنيف التحقق في هذا الملف:
//   VERIFIED   — العنوان مقروء في **الوثائق الرسمية الحالية** للجهة المالكة
//                (مضيف الوثيقة مذكور في `documented_by`). روجعت وثائق الموفرين
//                كلها في 19 أكتوبر 2026. هذا المستوى = DOCS_VERIFIED ولا يعني
//                تحققاً حيّاً: لا نداء بمفتاح حقيقي ولا استدعاء نموذج سحابي.
//   UNVERIFIED — لا وثيقة ولا اتصال؛ العنوان فارغ والنص العربي يقول ذلك.
//
// ملاحظة صدق: بعض **المسارات الفرعية** (تعداد النماذج لدى Z.ai وQwen) غير
// موثّقة رسمياً؛ يبقى دليل وجودها اتصالاً فعلياً سابقاً (401 لا 404) ويُعلن ذلك
// صراحةً في `note_ar` بدل رفعها إلى مستوى التوثيق.
// =====================================================================

use crate::domain::provider_registry::AiProviderKind;

/// حالة التحقق من العنوان الأساسي — تُعرض للمستخدم بصدق.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaseUrlProvenance {
    /// موثّق رسمياً من الجهة المالكة (الاسم في `documented_by`).
    Verified,
    /// غير متحقق: العنوان فارغ ويجب على المستخدم إدخاله.
    Unverified,
}

/// إعداد جاهز لنوع موفر.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPreset {
    pub kind: AiProviderKind,
    pub display_name: String,
    /// عنوان أساسي موثّق، أو نص فارغ عندما لا يكون موثّقاً.
    pub base_url: String,
    pub models_path: String,
    pub chat_path: String,
    /// الموثِّق الرسمي — فارغ عندما يكون العنوان غير متحقق.
    pub documented_by: String,
    pub provenance: BaseUrlProvenance,
    /// شرح عربي موجز يُعرض في الواجهة (يوضح المصدر أو سبب الفراغ).
    pub note_ar: String,
}

// لا مشتقات على ProviderPreset عن قصد: العنوان الفارغ نفسه هو الإشارة إلى
// إعداد غير متحقق، وقراءته مباشرةً (`base_url.is_empty()`) أوضح من غلاف حوله.

const OPENAI_MODELS_PATH: &str = "/v1/models";
const OPENAI_CHAT_PATH: &str = "/v1/chat/completions";

/// إعدادات الموفرين العشرة — الترتيب مطابق لترتيب العقد المجمّد.
pub fn provider_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            kind: AiProviderKind::Ollama,
            display_name: "Ollama (Local)".to_owned(),
            base_url: "http://127.0.0.1:11434".to_owned(),
            // واجهة Ollama الأصلية (/api/tags و/api/generate) هي المسار
            // المستعمل في محوّله المخصص؛ هذه المسارات تخص سطح /v1 المتوافق مع
            // OpenAI الذي يوثقه مشروع Ollama للمهاجرين من OpenAI.
            models_path: OPENAI_MODELS_PATH.to_owned(),
            chat_path: OPENAI_CHAT_PATH.to_owned(),
            documented_by: "Ollama (docs.ollama.com/api: /api/tags و/api/generate و/api/chat، وسطح /v1 المتوافق)".to_owned(),
            provenance: BaseUrlProvenance::Verified,
            note_ar: "محلي على العنوان الحلقي 127.0.0.1:11434 — لا يغادر الجهاز ولا يتطلب مفتاحاً. المنفذ الافتراضي موثّق رسمياً.".to_owned(),
        },
        ProviderPreset {
            kind: AiProviderKind::LmStudio,
            display_name: "LM Studio (Local)".to_owned(),
            base_url: "http://127.0.0.1:1234".to_owned(),
            models_path: OPENAI_MODELS_PATH.to_owned(),
            chat_path: OPENAI_CHAT_PATH.to_owned(),
            documented_by: "LM Studio (lmstudio.ai/docs/developer/openai-compat: /v1/models و/v1/chat/completions على المنفذ 1234)".to_owned(),
            provenance: BaseUrlProvenance::Verified,
            note_ar: "خادم LM Studio المحلي بواجهة متوافقة مع OpenAI على 127.0.0.1:1234 — المنفذ الافتراضي موثّق، لكنه قابل للتغيير من إعدادات LM Studio فيجب التحقق من العنوان الفعلي."
                .to_owned(),
        },
        ProviderPreset {
            kind: AiProviderKind::OpenAiCompatible,
            display_name: "OpenAI-Compatible Server".to_owned(),
            // لا عنوان: أي خادم متوافق — المستخدم يعرف عنوانه لا نحن.
            base_url: String::new(),
            models_path: OPENAI_MODELS_PATH.to_owned(),
            chat_path: OPENAI_CHAT_PATH.to_owned(),
            documented_by: String::new(),
            provenance: BaseUrlProvenance::Unverified,
            note_ar: "غير متحقق: لا يوجد عنوان افتراضي لهذا النوع بحكم تعريفه. أدخل العنوان الأساسي لخادمك (https إلزامي لغير الحلقي)."
                .to_owned(),
        },
        ProviderPreset {
            kind: AiProviderKind::OpenAi,
            display_name: "OpenAI".to_owned(),
            base_url: "https://api.openai.com".to_owned(),
            models_path: OPENAI_MODELS_PATH.to_owned(),
            chat_path: OPENAI_CHAT_PATH.to_owned(),
            documented_by: "OpenAI (developers.openai.com/api/reference: /v1/models و/v1/chat/completions)".to_owned(),
            provenance: BaseUrlProvenance::Verified,
            note_ar: "موثّق رسمياً من OpenAI. المسارات /v1/models و/v1/chat/completions جزء من عقد OpenAI المعلن. ترويسة المصادقة: Authorization: Bearer."
                .to_owned(),
        },
        ProviderPreset {
            kind: AiProviderKind::DeepSeek,
            display_name: "DeepSeek".to_owned(),
            // موثّق رسمياً من api-docs.deepseek.com (19 أكتوبر 2026):
            //   * قاعدة OpenAI-المتوافقة: https://api.deepseek.com (و«/v1» بديل
            //     متوافق مع OpenAI SDK بلا علاقة بإصدار النموذج).
            //   * تعداد النماذج: GET /models (وعبر البديل /v1: GET /v1/models).
            //   * المحادثة: POST /chat/completions (وعبر البديل: /v1/chat/completions).
            // ولذلك العنوان هنا + /v1/models تنتج نفس العنوان الموثّق عبر البديل.
            base_url: "https://api.deepseek.com".to_owned(),
            models_path: OPENAI_MODELS_PATH.to_owned(),
            chat_path: OPENAI_CHAT_PATH.to_owned(),
            documented_by: "DeepSeek (api-docs.deepseek.com: GET /models وPOST /chat/completions، و/v1 بديل متوافق مع OpenAI)".to_owned(),
            provenance: BaseUrlProvenance::Verified,
            note_ar: "موثّق رسمياً من DeepSeek. العنوان https://api.deepseek.com هو القاعدة المعلنة، و/v1 بديل متوافق مع OpenAI SDK. المصادقة: Authorization: Bearer. رسائل الأخطاء تُصنَّف بحالة HTTP فقط لأن الوثائق لا تنشر مخطط جسم خطأ قابلاً للقراءة الآلية."
                .to_owned(),
        },
        ProviderPreset {
            kind: AiProviderKind::Anthropic,
            display_name: "Anthropic".to_owned(),
            base_url: "https://api.anthropic.com".to_owned(),
            // Anthropic لا تستعمل مسارات OpenAI: محوّلها المخصص يستعمل
            // /v1/messages و/v1/models بترويسة x-api-key.
            models_path: "/v1/models".to_owned(),
            chat_path: "/v1/messages".to_owned(),
            documented_by: "Anthropic (platform.claude.com/docs/en/api: /v1/messages و/v1/models، وanthropic-version: 2023-06-01)".to_owned(),
            provenance: BaseUrlProvenance::Verified,
            note_ar: "موثّق رسمياً من Anthropic. يستعمل واجهة الرسائل الخاصة بها (/v1/messages مع x-api-key المدعوم وanthropic-version) لا واجهة OpenAI. لا يوجد إرسال نموذج [DONE] في بثّها — لا بثّ في NEXUS أصلاً."
                .to_owned(),
        },
        ProviderPreset {
            kind: AiProviderKind::Qwen,
            display_name: "Qwen (Alibaba Cloud Model Studio)".to_owned(),
            // موثّق رسمياً من وثائق Alibaba Cloud Model Studio (19 أكتوبر 2026):
            //   * POST /compatible-mode/v1/chat/completions على مضيفات إقليمية
            //     بصيغة https://{WorkspaceId}.{region}.maas.aliyuncs.com، وهذه
            //     تتطلب معرّف مساحة عمل لا يمكن حزمه في إعداد جاهز.
            //   * النطاق القديم https://dashscope.aliyuncs.com موثّق بأنه **ما زال
            //     متاحاً** مع التوصية بالانتقال إلى النطاق الخاص بمساحة العمل.
            //   * تعداد النماذج عبر /compatible-mode/v1/models **غير موثّق** في
            //     الصفحات الرسمية المقروءة؛ وجوده أُثبت باتصال فعلي (401 لا 404)
            //     فقط ⇒ يبقى غير موثّق ويُعلن كذلك.
            //   * المفتاح مرتبط بالإقليم: مفتاح إقليم آخر يُرجع 401 invalid_api_key.
            base_url: "https://dashscope.aliyuncs.com".to_owned(),
            models_path: "/compatible-mode/v1/models".to_owned(),
            chat_path: "/compatible-mode/v1/chat/completions".to_owned(),
            documented_by: "Alibaba Cloud Model Studio (alibabacloud.com/help/model-studio: وضع التوافق مع OpenAI). المسار /compatible-mode/v1/models غير موثّق ويبقى بتحقق الوصول وحده".to_owned(),
            provenance: BaseUrlProvenance::Verified,
            note_ar: "موثّق رسمياً: النطاق القديم dashscope.aliyuncs.com ما زال متاحاً، والموصى به نطاق خاص بمساحة العمل https://{WorkspaceId}.{region}.maas.aliyuncs.com/compatible-mode/v1. المفتاح مرتبط بالإقليم. مسار تعداد النماذج غير موثّق (تحقق بالوصول فقط)، والأدوات لا تُستخدم مع البث في واجهة Qwen."
                .to_owned(),
        },
        ProviderPreset {
            kind: AiProviderKind::Zai,
            display_name: "Z.ai".to_owned(),
            // موثّق رسمياً من docs.z.ai (19 أكتوبر 2026):
            //   * القاعدة: https://api.z.ai/api/paas/v4 (إصدار v4 لا v1).
            //   * المحادثة: POST /api/paas/v4/chat/completions.
            //   * **لا يوجد** مسار تعداد نماذج في الوثائق ولا في مواصفة OpenAPI
            //     المنشورة (14 مساراً بلا /models). المسار أدناه أُثبت بالوصول فقط
            //     (401 بخطأ z.ai لا 404) ⇒ يبقى غير موثّق ويُعلن كذلك.
            base_url: "https://api.z.ai".to_owned(),
            models_path: "/api/paas/v4/models".to_owned(),
            chat_path: "/api/paas/v4/chat/completions".to_owned(),
            documented_by: "Z.ai (docs.z.ai: القاعدة https://api.z.ai/api/paas/v4 وPOST /paas/v4/chat/completions). المسار /models غير موثّق ويبقى بتحقق الوصول وحده".to_owned(),
            provenance: BaseUrlProvenance::Verified,
            note_ar: "موثّق رسمياً: العنوان الأساسي هو https://api.z.ai وإصدار الواجهة v4 لا v1. تعداد النماذج غير موثّق رسمياً (تحقق بالوصول فقط)، لذا قد يفشل الاكتشاف ويُعلن الفشل بدل اختراع قائمة. المصادقة: Authorization: Bearer."
                .to_owned(),
        },
        ProviderPreset {
            kind: AiProviderKind::Xai,
            display_name: "xAI (Grok)".to_owned(),
            base_url: "https://api.x.ai".to_owned(),
            models_path: OPENAI_MODELS_PATH.to_owned(),
            chat_path: OPENAI_CHAT_PATH.to_owned(),
            documented_by: "xAI (docs.x.ai: «The API is served at https://api.x.ai»، و/v1/chat/completions و/v1/models)".to_owned(),
            provenance: BaseUrlProvenance::Verified,
            note_ar: "موثّق رسمياً من xAI: القاعدة https://api.x.ai، والمسارات /v1/chat/completions و/v1/models. المصادقة: Authorization: Bearer. واجهة Chat Completions مصنَّفة رسمياً سليفةً لواجهة Responses لكنها ما زالت موثّقة بالكامل."
                .to_owned(),
        },
        ProviderPreset {
            kind: AiProviderKind::OpenRouter,
            display_name: "OpenRouter".to_owned(),
            base_url: "https://openrouter.ai/api".to_owned(),
            models_path: OPENAI_MODELS_PATH.to_owned(),
            chat_path: OPENAI_CHAT_PATH.to_owned(),
            documented_by: "OpenRouter (openrouter.ai/docs/api-reference: POST /api/v1/chat/completions وGET /api/v1/models، مصادقة Bearer)".to_owned(),
            provenance: BaseUrlProvenance::Verified,
            note_ar: "موثّق رسمياً من OpenRouter: القاعدة https://openrouter.ai/api، والمسارات /v1/chat/completions و/v1/models. المصادقة: Authorization: Bearer. لم يُنفَّذ أي نداء حي — يلزم مفتاح المالك وسقف تكلفة."
                .to_owned(),
        },
    ]
}

/// إعداد نوع واحد — `None` يعني نوعاً بلا إعداد (لا يحدث للأنواع العشرة).
pub fn preset_for(kind: AiProviderKind) -> Option<ProviderPreset> {
    provider_presets()
        .into_iter()
        .find(|preset| preset.kind == kind)
}

#[cfg(test)]
mod tests {
    use super::{preset_for, provider_presets, BaseUrlProvenance};
    use crate::domain::provider_registry::AiProviderKind;

    #[test]
    fn covers_exactly_the_ten_contract_provider_kinds() {
        let presets = provider_presets();
        assert_eq!(presets.len(), 10);
        for kind in [
            AiProviderKind::Ollama,
            AiProviderKind::LmStudio,
            AiProviderKind::OpenAiCompatible,
            AiProviderKind::OpenAi,
            AiProviderKind::DeepSeek,
            AiProviderKind::Anthropic,
            AiProviderKind::Qwen,
            AiProviderKind::Zai,
            AiProviderKind::Xai,
            AiProviderKind::OpenRouter,
        ] {
            let preset = preset_for(kind).unwrap_or_else(|| panic!("{kind:?} بلا إعداد"));
            assert_eq!(preset.kind, kind);
            assert!(!preset.display_name.is_empty());
            assert!(!preset.note_ar.is_empty(), "لكل إعداد شرح عربي");
            assert!(preset.chat_path.starts_with('/'));
            assert!(preset.models_path.starts_with('/'));
        }
    }

    // القاعدة الجوهرية: كل عنوان غير موثّق يبقى فارغاً — لا اختراع.
    #[test]
    fn unverified_presets_carry_an_empty_base_url_and_say_so_in_arabic() {
        for preset in provider_presets() {
            match preset.provenance {
                BaseUrlProvenance::Verified => {
                    assert!(
                        !preset.base_url.is_empty(),
                        "{:?} موثّق فيجب أن يحمل عنواناً",
                        preset.kind
                    );
                    assert!(
                        !preset.documented_by.is_empty(),
                        "{:?} موثّق فيجب أن يُسمّي الموثِّق",
                        preset.kind
                    );
                }
                BaseUrlProvenance::Unverified => {
                    assert!(
                        preset.base_url.is_empty(),
                        "{:?} غير موثّق فيجب أن يكون العنوان فارغاً",
                        preset.kind
                    );
                    assert!(preset.documented_by.is_empty());
                    assert!(
                        preset.note_ar.contains("غير متحقق"),
                        "{:?} يجب أن يصرّح بعدم التحقق",
                        preset.kind
                    );
                }
            }
        }
    }

    #[test]
    fn every_verified_preset_uses_https_unless_it_is_a_local_loopback_provider() {
        for preset in provider_presets() {
            if preset.base_url.is_empty() {
                continue;
            }
            if preset.kind.is_local() {
                assert!(
                    preset.base_url.starts_with("http://127.0.0.1:"),
                    "{:?} محلي فيجب أن يكون على الحلقي",
                    preset.kind
                );
            } else {
                assert!(
                    preset.base_url.starts_with("https://"),
                    "{:?} بعيد فيجب أن يكون https",
                    preset.kind
                );
            }
            // لا بيانات اعتماد مضمّنة في أي عنوان إعداد.
            assert!(!preset.base_url.contains('@'));
        }
    }

    #[test]
    fn anthropic_preset_uses_its_own_paths_not_the_openai_ones() {
        let preset = preset_for(AiProviderKind::Anthropic).expect("إعداد Anthropic");
        assert_eq!(preset.chat_path, "/v1/messages");
        assert_eq!(preset.models_path, "/v1/models");
    }

    #[test]
    fn the_openai_surface_presets_use_the_documented_openai_paths() {
        for kind in [
            AiProviderKind::OpenAi,
            AiProviderKind::Xai,
            AiProviderKind::OpenRouter,
            AiProviderKind::OpenAiCompatible,
            AiProviderKind::DeepSeek,
        ] {
            let preset = preset_for(kind).expect("إعداد");
            assert_eq!(preset.chat_path, "/v1/chat/completions", "{kind:?}");
            assert_eq!(preset.models_path, "/v1/models", "{kind:?}");
        }
    }

    // العنوانان التاليان ليسا مساري OpenAI الافتراضيين، وأُثبتا بالوصول الفعلي
    // (15 أكتوبر 2026) ثم دُعِّما بوثائق الجهة الرسمية (19 أكتوبر 2026) — وهذا
    // الاختبار يمنع «تصحيحهما» عودةً إلى /v1.
    #[test]
    fn qwen_and_zai_presets_keep_their_actually_verified_non_default_paths() {
        let qwen = preset_for(AiProviderKind::Qwen).expect("إعداد Qwen");
        assert_eq!(qwen.base_url, "https://dashscope.aliyuncs.com");
        assert_eq!(qwen.models_path, "/compatible-mode/v1/models");
        assert_eq!(qwen.chat_path, "/compatible-mode/v1/chat/completions");

        let zai = preset_for(AiProviderKind::Zai).expect("إعداد Z.ai");
        assert_eq!(zai.base_url, "https://api.z.ai");
        assert_eq!(zai.models_path, "/api/paas/v4/models");
        assert_eq!(zai.chat_path, "/api/paas/v4/chat/completions");
        // إصدار الواجهة v4 لا v1 — لا يجوز أن يُستبدل بمسار OpenAI الافتراضي.
        assert!(!zai.models_path.starts_with("/v1/"));
    }

    // العناوين التي كانت «تحققاً بالوصول» فقط صارت موثّقة من الوثائق الرسمية
    // (19 أكتوبر 2026)، مع بقاء نفس العناوين التي استجابت فعلاً: لا تغيير صامت.
    #[test]
    fn formerly_unverified_presets_now_cite_official_documentation() {
        let expected = [
            (
                AiProviderKind::DeepSeek,
                "https://api.deepseek.com",
                "/v1/models",
                "api-docs.deepseek.com",
            ),
            (
                AiProviderKind::Qwen,
                "https://dashscope.aliyuncs.com",
                "/compatible-mode/v1/models",
                "alibabacloud.com",
            ),
            (
                AiProviderKind::Zai,
                "https://api.z.ai",
                "/api/paas/v4/models",
                "docs.z.ai",
            ),
        ];
        for (kind, base_url, models_path, documentation_host) in expected {
            let preset = preset_for(kind).expect("إعداد");
            assert_eq!(preset.provenance, BaseUrlProvenance::Verified, "{kind:?}");
            assert_eq!(preset.base_url, base_url, "{kind:?}");
            assert_eq!(preset.models_path, models_path, "{kind:?}");
            assert!(
                preset.documented_by.contains(documentation_host),
                "{kind:?} يجب أن يسمّي الوثيقة الرسمية التي تحقّق منها"
            );
        }
    }

    // بوابة صدق: كل عنوان «موثّق» يجب أن يسمّي مضيف وثائق رسمياً معروفاً.
    // تمنع إدخال «Verified» بلا مرجع، وتمنع تحويل تحقق الوصول إلى توثيق زوراً.
    #[test]
    fn every_verified_preset_names_a_recognised_official_documentation_host() {
        const OFFICIAL_DOCUMENTATION_HOSTS: [&str; 9] = [
            "docs.ollama.com",
            "lmstudio.ai/docs",
            "developers.openai.com",
            "api-docs.deepseek.com",
            "platform.claude.com",
            "alibabacloud.com",
            "docs.z.ai",
            "docs.x.ai",
            "openrouter.ai/docs",
        ];
        for preset in provider_presets() {
            if preset.provenance != BaseUrlProvenance::Verified {
                continue;
            }
            assert!(
                OFFICIAL_DOCUMENTATION_HOSTS
                    .iter()
                    .any(|host| preset.documented_by.contains(host)),
                "{:?} موثّق فيجب أن يذكر مضيف وثائق رسمياً معروفاً: {}",
                preset.kind,
                preset.documented_by
            );
        }
    }

    // صدق معلن: مسار تعداد النماذج غير الموثّق في Z.ai وQwen يجب أن يُعلن
    // صراحةً في نص الإعداد، فلا يقرؤه المستخدم كعقد موثّق.
    #[test]
    fn presets_with_undocumented_model_listing_say_so_out_loud() {
        for kind in [AiProviderKind::Qwen, AiProviderKind::Zai] {
            let preset = preset_for(kind).expect("إعداد");
            assert!(
                preset.note_ar.contains("غير موثّق"),
                "{kind:?} يجب أن يعلن أن مسار تعداد النماذج غير موثّق"
            );
        }
    }

    // عقد العنوان الكامل (origin + path) كما تُبنى فعلياً في المحوّلات — مطابقة
    // حرفية للعناوين المقروءة في الوثائق الرسمية (19 أكتوبر 2026).
    #[test]
    fn composed_endpoints_match_the_documented_urls_exactly() {
        let expected = [
            (
                AiProviderKind::OpenAi,
                "https://api.openai.com/v1/models",
                "https://api.openai.com/v1/chat/completions",
            ),
            (
                AiProviderKind::DeepSeek,
                // البديل المتوافق مع OpenAI: base /v1 + /models.
                "https://api.deepseek.com/v1/models",
                "https://api.deepseek.com/v1/chat/completions",
            ),
            (
                AiProviderKind::Anthropic,
                "https://api.anthropic.com/v1/models",
                "https://api.anthropic.com/v1/messages",
            ),
            (
                AiProviderKind::Qwen,
                "https://dashscope.aliyuncs.com/compatible-mode/v1/models",
                "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions",
            ),
            (
                AiProviderKind::Zai,
                "https://api.z.ai/api/paas/v4/models",
                "https://api.z.ai/api/paas/v4/chat/completions",
            ),
            (
                AiProviderKind::Xai,
                "https://api.x.ai/v1/models",
                "https://api.x.ai/v1/chat/completions",
            ),
            (
                AiProviderKind::OpenRouter,
                "https://openrouter.ai/api/v1/models",
                "https://openrouter.ai/api/v1/chat/completions",
            ),
            (
                AiProviderKind::Ollama,
                "http://127.0.0.1:11434/v1/models",
                "http://127.0.0.1:11434/v1/chat/completions",
            ),
            (
                AiProviderKind::LmStudio,
                "http://127.0.0.1:1234/v1/models",
                "http://127.0.0.1:1234/v1/chat/completions",
            ),
        ];
        for (kind, models_url, chat_url) in expected {
            let preset = preset_for(kind).expect("إعداد");
            assert_eq!(
                format!("{}{}", preset.base_url, preset.models_path),
                models_url,
                "{kind:?} — عنوان تعداد النماذج"
            );
            assert_eq!(
                format!("{}{}", preset.base_url, preset.chat_path),
                chat_url,
                "{kind:?} — عنوان المحادثة"
            );
        }
    }
}
