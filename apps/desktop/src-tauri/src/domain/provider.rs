use std::fmt;

use sha2::{Digest, Sha256};

// حد الإدخال المُجمَّع — رُفع إلى 24KB في المرحلة 04 بقرار مضبوط (C-4):
// مسار RAG المحكوم يتطلب طبقة معرفة مسترجعة فوق الطبقات القائمة، والحد
// يبقى صغيراً متوقعاً مقابل سياق النموذج الفعلي ولا يُرفع لمجرد السعة.
pub const MAX_RUNTIME_INPUT_BYTES: usize = 24 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProviderId(String);

impl ProviderId {
    pub fn new(value: impl Into<String>) -> Result<Self, ProviderFailure> {
        let value = value.into();
        let is_valid = !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
        if !is_valid {
            return Err(ProviderFailure::InvalidResponse);
        }

        Ok(Self(value))
    }

    pub fn ollama_local() -> Self {
        Self("OLLAMA_LOCAL".to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// معرّف النموذج كما يعلنه الموفر — **نص حرفي غير قابل للتفاوض**.
///
/// لماذا اتّسع هذا النطاق (عيبة مُثبتة حيّاً بمقابلة الخوادم الفعلية):
///   * llama.cpp (Bonsai 2) يشير إلى النموذج بمسار ملف كامل على ويندوز، مثل
///     `D:\Bonsi\...\Ternary-Bonsai-2-27B-PQ2_0.gguf` ⇒ يحتاج `\` و`:`.
///   * LM Studio يعلن `qwen/qwen3.8-27b` ⇒ يحتاج `/`.
///   * OpenRouter يعلن **465 من 465** معرّفاً بصيغة `vendor/model`، وبعضها يبدأ
///     بـ`~` مثل `~openai/gpt-astra-latest` ⇒ يحتاج `/` و`~`.
///
/// كان النطاق المغلق السابق `[A-Za-z0-9._-:]` يرفض كل هذه المعرّفات. والأثر
/// أخطر من رفض صف واحد: تعداد النماذج يُجمَع في `Result`، فمعرّف واحد مرفوض
/// **يُسقط التعداد كاملاً** ⇒ يصير الموفر غير قابل للاستخدام إطلاقاً (وهذا ما
/// يمنع ربط Bonsai 2 وLM Studio وOpenRouter من الأساس).
///
/// الحصانة محفوظة **بشرط صريح لا بالتضييق الأبجدي**: كل مقطع مفصول بـ`/` أو `\`
/// يجب ألا يكون `.` ولا `..`، فلا يبقى مسار تصاعدي صالح، ويظل
/// `../../outside` مرفوضاً كما كان. ويُرفض أيضاً أي محرف خارج القائمة المسموحة
/// (لا محارف تحكّم ولا `%` ولا NUL).
///
/// **قاعدة ملزمة:** هذا المعرّف يُنقل إلى المزود وإلى القاعدة ولا يُستخدم
/// إطلاقاً لبناء مسار ملفات. أي استخدام لاحق كمسار يستوجب إعادة تقييم أمني.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelId(String);

impl ModelId {
    pub fn new(value: impl Into<String>) -> Result<Self, ProviderFailure> {
        let value = value.into();
        let is_valid = !value.is_empty()
            && value.len() <= 256
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/' | b'\\' | b'~')
            })
            // لا مقطع تصاعدي: يمنع `../../x` و`..\..\x` رغم السماح بالفاصلين.
            && value
                .split(['/', '\\'])
                .all(|segment| segment != "." && segment != "..");

        if !is_valid {
            return Err(ProviderFailure::InvalidResponse);
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// هل يطابق المعرّف **الذي أعلنه الموفر في الاستجابة** المعرّف المطلوب؟
    ///
    /// القاعدة الموثّقة: عدة مزوّدات تضع في حقل `model` **لقطة النموذج المُنفَّذة**
    /// (model snapshot) لا الاسم المستعار الذي طُلب:
    ///   * OpenAI: طلب `gpt-4o` ⇒ الاستجابة `"model": "gpt-4o-2024-08-06"`.
    ///   * Anthropic: أسماء مستعارة تُحلّ إلى لقطة بتاريخ مثل `-20250819`.
    ///   * xAI: طلب `grok-2` ⇒ الاستجابة قد تكون `grok-2-1212`.
    /// لذلك كانت المساواة الحرفية وحدها تُفشل توليداً سليماً لدى هذه المزوّدات
    /// (`InvalidResponse` رغم نجاح الطلب) — وهي عيبة تصحيحها لا يفتح أي باب.
    ///
    /// الضابط:
    ///   * مساواة حرفية، أو
    ///   * المُعاد = المطلوب + `-` + **لقطة رقمية** (أرقام وشرطات) بأربعة أرقام
    ///     على الأقل.
    /// فيبقى `gpt-4o-mini` مرفوضاً عند طلب `gpt-4o` (لاحقة حرفية)، ويبقى
    /// `grok-2-vision-1212` مرفوضاً، ولا يوجد أي تحويل صامت إلى نموذج آخر.
    pub fn accepts_reported(&self, reported: &Self) -> bool {
        if self.0 == reported.0 {
            return true;
        }

        let Some(suffix) = reported.0.strip_prefix(&self.0) else {
            return false;
        };
        let Some(snapshot) = suffix.strip_prefix('-') else {
            return false;
        };
        !snapshot.is_empty()
            && snapshot
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'-')
            && snapshot.bytes().filter(u8::is_ascii_digit).count() >= 4
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderHealth {
    Unknown,
    Available,
    Unavailable,
}

/// هوية **وجهة** التنفيذ — تُثبَّت لحظة طلب التشغيل وتُقارَن إلزامياً لحظة
/// التنفيذ.
///
/// العيبة التي أُغلقت بها: كان ربط الموافقة يحمل معرّف النموذج وحده، فلم يكن
/// يميّز بين وجهتين مختلفتين تعلنان النموذج نفسه. فيكفي أن يُغيَّر الموفر
/// المختار بعد الاعتماد ليجري التنفيذ على وجهة **لم تُعتمد** (تسريب بيانات إلى
/// مزوّد خارجي غير مصرّح، أو تنفيذ على نموذج مختلف تحت المعرّف نفسه).
///
/// كل حقول هذه البنية مُشتقّة من مصدر واحد: صف الموفر في `ai_provider_registry`
/// (أو المسار المحلي التاريخي عند غياب أي اختيار). لا تُخزَّن هنا أي مادة سرية.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderExecutionIdentity {
    /// معرّف الموفر المنطقي، مثال `OLLAMA_LOCAL` أو `PROVIDER_OPENAI`.
    pub provider_id: String,
    /// عائلة الموفر، وهي التي تحدد مسارات الواجهة المستعملة.
    pub provider_kind: String,
    /// العنوان الأساسي كما خُزِّن بعد تطبيع سياسة الشبكة.
    pub base_url: String,
    /// نوع النقل (`HTTP_LOOPBACK` أو `HTTPS`).
    pub transport: String,
}

impl ProviderExecutionIdentity {
    pub fn new(
        provider_id: impl Into<String>,
        provider_kind: impl Into<String>,
        base_url: impl Into<String>,
        transport: impl Into<String>,
    ) -> Result<Self, ProviderFailure> {
        let identity = Self {
            provider_id: provider_id.into(),
            provider_kind: provider_kind.into(),
            base_url: base_url.into(),
            transport: transport.into(),
        };
        let is_valid = !identity.provider_id.is_empty()
            && identity.provider_id.len() <= 128
            && !identity.provider_kind.is_empty()
            && identity.provider_kind.len() <= 64
            && !identity.base_url.is_empty()
            && identity.base_url.len() <= 512
            && !identity.transport.is_empty()
            && identity.transport.len() <= 32;

        if !is_valid {
            return Err(ProviderFailure::InvalidResponse);
        }

        Ok(identity)
    }

    /// بصمة الوجهة: SHA-256 على ترميز **بادئة الطول**، فلا يمكن لقيمتين مختلفتين
    /// أن تتصادما مهما احتوتا من فواصل أو محارف متشابهة.
    pub fn endpoint_identity(&self) -> String {
        canonical_hash(&[
            &self.provider_id,
            &self.provider_kind,
            &self.base_url,
            &self.transport,
        ])
    }

    /// بصمة الربط الكاملة. تجمع: هوية الوجهة + هوية النموذج + نسخة بروفايل
    /// التنفيذ + هاش المهمة + مراجعة التشغيل. أي تغيّر في أحدها يُبطل الاعتماد.
    pub fn approval_binding_hash(
        &self,
        model_id: &str,
        execution_profile_version_id: &str,
        task_content_hash: &str,
        agent_run_revision: i64,
    ) -> String {
        canonical_hash(&[
            &self.provider_id,
            &self.provider_kind,
            &self.endpoint_identity(),
            model_id,
            execution_profile_version_id,
            task_content_hash,
            &agent_run_revision.to_string(),
        ])
    }
}

/// ترميز قانوني غير قابل للالتباس: طول كل جزء (u64 صغير النهاية) يُبثّ قبله.
fn canonical_hash(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredModel {
    pub id: ModelId,
    pub display_name: String,
    pub modified_at: Option<String>,
    pub size_bytes: Option<u64>,
    /// قدرة الأدوات المهيكلة كما يكشفها المزود حتمياً (لا افتراض).
    pub supports_tools: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeExecutionInput {
    value: String,
}

impl RuntimeExecutionInput {
    pub fn new(value: impl Into<String>) -> Result<Self, ProviderFailure> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_RUNTIME_INPUT_BYTES
            || !value.is_char_boundary(value.len())
        {
            return Err(ProviderFailure::InputTooLarge);
        }

        Ok(Self { value })
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderGeneration {
    pub model_id: ModelId,
    pub output: String,
    pub generated_at: String,
}

/// تعريف أداة معلنة للمزود — وصف غير تنفيذي؛ السلطة تبقى خلفياً.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderToolDefinition {
    pub name: String,
    pub description: String,
    /// مخطط معاملات JSON (كائن) يبقى ضمن حد الطلب.
    pub parameters_json: String,
}

impl ProviderToolDefinition {
    pub fn validate(&self) -> Result<(), ProviderFailure> {
        let name_valid = !self.name.is_empty()
            && self.name.len() <= 64
            && self
                .name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
        if !name_valid || self.description.len() > 1_024 || self.parameters_json.len() > 1_024 {
            return Err(ProviderFailure::InvalidResponse);
        }
        match serde_json::from_str::<serde_json::Value>(&self.parameters_json) {
            Ok(value) if value.is_object() => Ok(()),
            _ => Err(ProviderFailure::InvalidResponse),
        }
    }
}

/// استدعاء أداة مهيكل من المزود — لا يُشتق أبداً من نثر النص.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderToolCall {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// استجابة مسار الأدوات الواعي: نص نهائي أو استدعاء أداة واحد مهيكل.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderToolResponse {
    Text(ProviderGeneration),
    ToolCall(ProviderToolCall),
}

/// توليد مهيكل مقيد بصيغة JSON — للمخطط والمدقق حصراً؛ يُتحقق منه كمخطط
/// مغلق ولا يُثق به كنثر (C-10/§94).
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderStructuredGeneration {
    pub model_id: ModelId,
    pub value: serde_json::Value,
    pub generated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderFailure {
    Unavailable,
    Timeout,
    InvalidResponse,
    ModelUnavailable,
    InputTooLarge,
    OutputTooLarge,
    ToolCallUnsupported,
    /// رفض سياسة الوجهة الشبكية (0023) — يحمل الرمز الآمن الثابت للسياسة.
    /// لا يحمل أي جزء من العنوان ولا عنوان IP محلول، فلا يتسرب شيء إلى الواجهة.
    EndpointRejected(&'static str),
    /// غياب سر مطلوب لهذا الموفر — فشل مغلق قبل أي محاولة شبكة.
    CredentialMissing,
}

impl fmt::Display for ProviderFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Unavailable => "provider unavailable",
            Self::Timeout => "provider timeout",
            Self::InvalidResponse => "provider response invalid",
            Self::ModelUnavailable => "model unavailable",
            Self::InputTooLarge => "provider input too large",
            Self::OutputTooLarge => "provider output too large",
            Self::ToolCallUnsupported => "provider tool call unsupported",
            Self::EndpointRejected(code) => code,
            Self::CredentialMissing => "provider credential missing",
        };

        formatter.write_str(value)
    }
}

impl std::error::Error for ProviderFailure {}

pub trait ModelProvider {
    fn provider_id(&self) -> ProviderId;
    fn health(&self) -> ProviderHealth;
    fn discover_models(&self) -> Result<Vec<DiscoveredModel>, ProviderFailure>;
    fn generate_non_streaming(
        &self,
        model_id: &ModelId,
        input: &RuntimeExecutionInput,
    ) -> Result<ProviderGeneration, ProviderFailure>;

    /// المسار الواعي بالأدوات — الافتراضي فشل مغلق: أي مزود لا يدعمه يرفضه.
    fn generate_with_tools(
        &self,
        _model_id: &ModelId,
        _input: &RuntimeExecutionInput,
        _tools: &[ProviderToolDefinition],
    ) -> Result<ProviderToolResponse, ProviderFailure> {
        Err(ProviderFailure::ToolCallUnsupported)
    }

    /// التوليد المهيكل بصيغة JSON — الافتراضي فشل مغلق: لا اشتقاق JSON من نثر.
    fn generate_structured(
        &self,
        _model_id: &ModelId,
        _input: &RuntimeExecutionInput,
    ) -> Result<ProviderStructuredGeneration, ProviderFailure> {
        Err(ProviderFailure::InvalidResponse)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ModelId, ProviderExecutionIdentity, ProviderFailure, RuntimeExecutionInput,
        MAX_RUNTIME_INPUT_BYTES,
    };

    #[test]
    fn rejects_model_identifiers_outside_the_closed_format() {
        assert!(matches!(
            ModelId::new("../../outside"),
            Err(ProviderFailure::InvalidResponse)
        ));
        assert!(ModelId::new("llama3.2:3b").is_ok());
    }

    /// العيبة المُثبتة بالوثائق (19 أكتوبر 2026): عدة مزوّدات تُعيد في حقل
    /// `model` **لقطة النموذج** لا الاسم المستعار المطلوب. المساواة الحرفية
    /// وحدها كانت تُفشل توليداً سليماً، والاختبار يثبّت الضابط في الاتجاهين.
    #[test]
    fn model_identity_accepts_documented_snapshot_aliases_and_nothing_else() {
        let accepts = |requested: &str, reported: &str| {
            ModelId::new(requested)
                .expect("معرّف الطلب")
                .accepts_reported(&ModelId::new(reported).expect("معرّف الاستجابة"))
        };

        // مساواة حرفية (الغالبية).
        assert!(accepts("gpt-4o-mini", "gpt-4o-mini"));
        assert!(accepts("llama3.2:3b", "llama3.2:3b"));

        // لقطات موثّقة: OpenAI (تاريخ)، Anthropic (تاريخ مضغوط)، xAI (رقم إصدار).
        assert!(accepts("gpt-4o", "gpt-4o-2024-08-06"));
        assert!(accepts("gpt-4.1", "gpt-4.1-2025-04-14"));
        assert!(accepts("claude-sonnet-4-5", "claude-sonnet-4-5-20250929"));
        assert!(accepts("grok-2", "grok-2-1212"));

        // اتجاه واحد فقط: طلب لقطة وإعادة اسم عام ليس مطابقة.
        assert!(!accepts("gpt-4o-2024-08-06", "gpt-4o"));

        // نموذج مختلف فعلاً يُرفض: لاحقة حرفية، أو لقطة بلا أرقام كافية، أو
        // بادئة مختلفة كلّياً، أو نموذج من عائلة أخرى يبدأ بالاسم نفسه.
        assert!(!accepts("gpt-4o", "gpt-4o-mini"));
        assert!(!accepts("gpt-4", "gpt-4o-mini"));
        assert!(!accepts("grok-2", "grok-2-vision-1212"));
        assert!(!accepts("claude-3-5", "claude-3-5-sonnet-20241022"));
        assert!(!accepts("qwen3:4b", "qwen3:8b"));
    }

    // انحدار العيبة المُثبتة حيّاً: هذه المعرّفات مأخوذة **حرفياً** من استجابات
    // الخوادم الفعلية على هذا الجهاز (16 أكتوبر 2026)، وكانت كلها مرفوضة قبل
    // الإصلاح ⇒ تعداد النماذج يسقط كاملاً والموفر يصير غير قابل للاستخدام.
    #[test]
    fn accepts_the_model_identifiers_real_servers_actually_report() {
        let observed = [
            // llama.cpp (Bonsai 2) — حقل `id` في GET /v1/models هو مسار الملف.
            r"D:\Bonsi\Bonsai-demo\models\bonsai2-gguf\27B\Ternary-Bonsai-2-27B-PQ2_0.gguf",
            // LM Studio — معرّف بفاصل مائل.
            "qwen/qwen3.8-27b",
            "huihui-qwen3.8-27b-abliterated",
            "text-embedding-nomic-embed-text-v1.5",
            // Ollama.
            "llama3.2:3b",
            "qwen3:4b",
            // OpenRouter — 465/465 معرّفاً بهذه الصيغة، وبعضها يبدأ بـ`~`.
            "moondream/moondream3-preview",
            "nvidia/nemotron-3.5-super-vl:free",
            "~openai/gpt-astra-latest",
            // OpenAI وAnthropic.
            "gpt-4o-mini",
            "claude-sonnet-4-5",
        ];
        for candidate in observed {
            assert!(
                ModelId::new(candidate).is_ok(),
                "معرّف حقيقي مرفوض: {candidate}"
            );
        }
    }

    // الحصانة لم تُشترَ بالتوسيع: المسارات التصاعدية تبقى مرفوضة على الفاصلين.
    #[test]
    fn still_rejects_traversal_on_both_separators_after_widening_the_charset() {
        for candidate in [
            "../../outside",
            r"..\..\outside",
            "a/../b",
            r"D:\models\..\..\secret.gguf",
            "..",
            "./model",
        ] {
            assert!(
                matches!(
                    ModelId::new(candidate),
                    Err(ProviderFailure::InvalidResponse)
                ),
                "مسار تصاعدي يجب أن يُرفض: {candidate}"
            );
        }
    }

    // المحارف خارج القائمة المسموحة تبقى مرفوضة (لا محارف تحكّم ولا مسافات ولا NUL).
    #[test]
    fn still_rejects_control_characters_spaces_and_overlong_values() {
        for candidate in [
            "model\nname",
            "model name",
            "model\0name",
            "model%2e%2e",
            "model;rm -rf /",
            "model\"quote",
        ] {
            assert!(
                matches!(
                    ModelId::new(candidate),
                    Err(ProviderFailure::InvalidResponse)
                ),
                "قيمة غير مسموحة يجب أن تُرفض: {candidate:?}"
            );
        }
        // الحد الأعلى للطول يبقى قائماً (يطابق قيد مخطط القاعدة 1..256).
        assert!(ModelId::new("a".repeat(256)).is_ok());
        assert!(ModelId::new("a".repeat(257)).is_err());
        assert!(ModelId::new(String::new()).is_err());
    }

    #[test]
    fn rejects_runtime_input_above_the_contract_limit() {
        let oversized = "a".repeat(MAX_RUNTIME_INPUT_BYTES + 1);
        assert!(matches!(
            RuntimeExecutionInput::new(oversized),
            Err(ProviderFailure::InputTooLarge)
        ));
    }

    // ===== 0024: هوية وجهة التنفيذ =====

    fn identity(provider_id: &str, kind: &str, base_url: &str) -> ProviderExecutionIdentity {
        ProviderExecutionIdentity::new(provider_id, kind, base_url, "HTTPS")
            .expect("هوية الاختبار يجب أن تُبنى")
    }

    // أي تغيّر في أي مكوّن من مكوّنات الوجهة يُنتج بصمة مختلفة — وهذا ما يجعل
    // تبديل الموفر بعد الاعتماد قابلاً للكشف لا مجرد نية.
    #[test]
    fn endpoint_identity_changes_when_any_destination_component_changes() {
        let base = identity("PROVIDER_A", "OPENAI", "https://api.openai.com");
        assert_eq!(base.endpoint_identity().len(), 64);

        let other_id = identity("PROVIDER_B", "OPENAI", "https://api.openai.com");
        let other_kind = identity("PROVIDER_A", "DEEPSEEK", "https://api.openai.com");
        let other_url = identity("PROVIDER_A", "OPENAI", "https://api.deepseek.com");

        for candidate in [other_id, other_kind, other_url] {
            assert_ne!(
                base.endpoint_identity(),
                candidate.endpoint_identity(),
                "مكوّن وجهة مختلف يجب أن يُنتج بصمة مختلفة"
            );
        }
        // نفس المدخلات ⇒ نفس البصمة (حتمية).
        assert_eq!(
            base.endpoint_identity(),
            identity("PROVIDER_A", "OPENAI", "https://api.openai.com").endpoint_identity()
        );
    }

    // بصمة الربط تجمع الوجهة والنموذج ونسخة البروفايل وهاش المهمة والمراجعة.
    #[test]
    fn approval_binding_hash_covers_every_bound_component() {
        let destination = identity("OLLAMA_LOCAL", "OLLAMA", "http://127.0.0.1:11434");
        let baseline = destination.approval_binding_hash("qwen3:4b", "profile_v1", "taskhash", 1);
        assert_eq!(baseline.len(), 64);

        let variants = [
            destination.approval_binding_hash("llama3.2:3b", "profile_v1", "taskhash", 1),
            destination.approval_binding_hash("qwen3:4b", "profile_v2", "taskhash", 1),
            destination.approval_binding_hash("qwen3:4b", "profile_v1", "otherhash", 1),
            destination.approval_binding_hash("qwen3:4b", "profile_v1", "taskhash", 2),
            identity("OTHER", "OLLAMA", "http://127.0.0.1:11434").approval_binding_hash(
                "qwen3:4b",
                "profile_v1",
                "taskhash",
                1,
            ),
        ];
        for variant in variants {
            assert_ne!(baseline, variant, "كل مكوّن مربوط يجب أن يؤثر في البصمة");
        }
    }

    // الترميز بادئة الطول يمنع التصادم بين توزيعات مختلفة على نفس الفواصل.
    #[test]
    fn canonical_encoding_is_free_of_separator_ambiguity() {
        let joined = ProviderExecutionIdentity::new("A:B", "C", "D", "E")
            .expect("هوية")
            .endpoint_identity();
        let split = ProviderExecutionIdentity::new("A", "B:C", "D", "E")
            .expect("هوية")
            .endpoint_identity();
        assert_ne!(joined, split);
    }

    #[test]
    fn rejects_incomplete_destination_identities() {
        assert!(ProviderExecutionIdentity::new("", "OLLAMA", "http://x", "HTTPS").is_err());
        assert!(ProviderExecutionIdentity::new("A", "", "http://x", "HTTPS").is_err());
        assert!(ProviderExecutionIdentity::new("A", "OLLAMA", "", "HTTPS").is_err());
        assert!(ProviderExecutionIdentity::new("A", "OLLAMA", "http://x", "").is_err());
    }
}
