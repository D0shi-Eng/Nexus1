// =====================================================================
// سجل الموفرين — أنواع العقد المجمّد (frozen contract types)
// ---------------------------------------------------------------------
// كل بنية هنا تُسلسَل بـ camelCase مطابقاً حرفياً لعقد TypeScript المجمّد في
// packages/contracts/src/index.ts. أي تغيير في اسم حقل هنا يكسر الواجهة.
//
// قواعد الصدق الملزمة التي يعبّر عنها هذا الملف:
//   - لا يُعاد أي سر إلى الواجهة إطلاقاً؛ الحقل الوحيد هو credential_configured.
//   - القيم غير المتحقق منها تبقى `null` ولا تُخترع: نافذة السياق والقدرات.
//   - capability_source يعلن مصدر المعرفة (اكتشاف حي · إعلان الموفر · مجهول).
//   - UNVERIFIED تعني «لم يُختبر الاتصال» وليست نجاحاً.
// =====================================================================

use serde::{Deserialize, Serialize};

use crate::domain::provider::ProviderFailure;

/// أنواع الموفرين المدعومة — مطابقة لـ aiProviderKinds في العقد.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AiProviderKind {
    Ollama,
    LmStudio,
    OpenAiCompatible,
    OpenAi,
    DeepSeek,
    Anthropic,
    Qwen,
    Zai,
    Xai,
    OpenRouter,
}

impl AiProviderKind {
    /// الرمز الذي تراه الواجهة — مطابق لعقد AiProviderKind.
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Ollama => "OLLAMA",
            Self::LmStudio => "LM_STUDIO",
            Self::OpenAiCompatible => "OPENAI_COMPATIBLE",
            Self::OpenAi => "OPENAI",
            Self::DeepSeek => "DEEPSEEK",
            Self::Anthropic => "ANTHROPIC",
            Self::Qwen => "QWEN",
            Self::Zai => "ZAI",
            Self::Xai => "XAI",
            Self::OpenRouter => "OPENROUTER",
        }
    }

    /// الرمز في قاعدة البيانات — نفس المجموعة، مع قيمة بديلة للصفوف التاريخية.
    pub fn as_database_code(self) -> &'static str {
        self.as_code()
    }

    pub fn parse_code(value: &str) -> Result<Self, ProviderFailure> {
        match value {
            "OLLAMA" => Ok(Self::Ollama),
            "LM_STUDIO" => Ok(Self::LmStudio),
            "OPENAI_COMPATIBLE" => Ok(Self::OpenAiCompatible),
            "OPENAI" => Ok(Self::OpenAi),
            "DEEPSEEK" => Ok(Self::DeepSeek),
            "ANTHROPIC" => Ok(Self::Anthropic),
            "QWEN" => Ok(Self::Qwen),
            "ZAI" => Ok(Self::Zai),
            "XAI" => Ok(Self::Xai),
            "OPENROUTER" => Ok(Self::OpenRouter),
            _ => Err(ProviderFailure::InvalidResponse),
        }
    }

    /// الصفوف التاريخية في ai_model_registry تحمل 'LOCAL' للنماذج المحلية.
    /// 'LOCAL' تعني «مزود محلي بعقد Ollama» في هذا المنتج — الترجمة صريحة
    /// ولا تخمين فيها.
    pub fn from_database_code(value: &str) -> Result<Self, ProviderFailure> {
        if value == "LOCAL" {
            return Ok(Self::Ollama);
        }

        Self::parse_code(value)
    }

    /// هل هذا الموفر محلي (لا يغادر الجهاز)؟ يحدد اشتراط السر وسياسة الشبكة.
    pub fn is_local(self) -> bool {
        matches!(self, Self::Ollama | Self::LmStudio)
    }

    /// هل يتطلب الموفر مادة سرية؟ الحقيقة من الموفر نفسه لا من تخمين المستخدم.
    pub fn requires_credential(self) -> bool {
        !self.is_local()
    }

    /// هل يدعم الموفر استكشاف النماذج عبر واجهة برمجية؟ كل الأنواع المدعومة
    /// تُقدّم مسار تعداد نماذج (Ollama /api/tags، وOpenAI-compatible /models)،
    /// لكن الاكتشاف نفسه قد يفشل — والفشل يُعلن ولا يُخفى.
    pub fn supports_discovery(self) -> bool {
        true
    }
}

impl Serialize for AiProviderKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_code())
    }
}

impl<'de> Deserialize<'de> for AiProviderKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse_code(&value).map_err(serde::de::Error::custom)
    }
}

/// حالات الاتصال — لا CONNECTED إلا بعد اختبار ناجح فعلي.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiProviderConnectionState {
    Connected,
    Disconnected,
    Unavailable,
    Unverified,
}

impl AiProviderConnectionState {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Connected => "CONNECTED",
            Self::Disconnected => "DISCONNECTED",
            Self::Unavailable => "UNAVAILABLE",
            Self::Unverified => "UNVERIFIED",
        }
    }

    pub fn parse_code(value: &str) -> Result<Self, ProviderFailure> {
        match value {
            "CONNECTED" => Ok(Self::Connected),
            "DISCONNECTED" => Ok(Self::Disconnected),
            "UNAVAILABLE" => Ok(Self::Unavailable),
            "UNVERIFIED" => Ok(Self::Unverified),
            _ => Err(ProviderFailure::InvalidResponse),
        }
    }
}

impl Serialize for AiProviderConnectionState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_code())
    }
}

/// مصدر معرفة القدرات — يمنع تقديم التخمين كحقيقة.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiCapabilitySource {
    /// اكتُشفت من استجابة المزود الحية.
    Discovered,
    /// أعلنها المستخدم/الموفر دون إثبات آلي.
    Declared,
    /// لا معرفة — كل القيم تبقى null.
    Unknown,
}

impl AiCapabilitySource {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::Discovered => "DISCOVERED",
            Self::Declared => "DECLARED",
            Self::Unknown => "UNKNOWN",
        }
    }

    pub fn parse_code(value: &str) -> Result<Self, ProviderFailure> {
        match value {
            "DISCOVERED" => Ok(Self::Discovered),
            "DECLARED" => Ok(Self::Declared),
            "UNKNOWN" => Ok(Self::Unknown),
            _ => Err(ProviderFailure::InvalidResponse),
        }
    }
}

impl Serialize for AiCapabilitySource {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_code())
    }
}

/// حالة نقل موفر — مشتقة من سياسة الشبكة عند الحفظ لا من ادعاء العميل.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiTransportKind {
    /// http إلى عنوان حلقي فقط.
    HttpLoopback,
    /// https إلى أي وجهة مسموحة.
    Https,
}

impl AiTransportKind {
    pub fn as_database_code(self) -> &'static str {
        match self {
            Self::HttpLoopback => "HTTP_LOOPBACK",
            Self::Https => "HTTPS",
        }
    }

    pub fn parse_database_code(value: &str) -> Result<Self, ProviderFailure> {
        match value {
            "HTTP_LOOPBACK" => Ok(Self::HttpLoopback),
            "HTTPS" => Ok(Self::Https),
            _ => Err(ProviderFailure::InvalidResponse),
        }
    }
}

// ===== بنى العقد المُسلسَلة إلى الواجهة =====

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderRecord {
    pub provider_id: String,
    pub kind: AiProviderKind,
    pub display_name: String,
    /// عنوان أساسي بلا أي مادة سرية؛ يُعرض للمستخدم للتحقق البصري.
    pub base_url: String,
    pub enabled: bool,
    pub connection_state: AiProviderConnectionState,
    pub connection_checked_at: Option<String>,
    /// هل توجد مادة سرية محفوظة في المخزن الآمن؟ لا يُعاد السر نفسه أبداً.
    pub credential_configured: bool,
    pub requires_credential: bool,
    pub supports_discovery: bool,
    pub model_count: i64,
    pub built_in: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModelRecord {
    pub model_id: String,
    pub provider_id: String,
    pub provider_kind: AiProviderKind,
    pub provider_display_name: String,
    pub display_name: String,
    pub available: bool,
    pub selected: bool,
    /// null يعني غير معروف بثقة — لا قيمة مخترعة.
    pub context_limit: Option<i64>,
    pub supports_tools: Option<bool>,
    pub supports_structured_output: Option<bool>,
    pub supports_vision: Option<bool>,
    pub capability_source: AiCapabilitySource,
    pub last_discovered_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProvidersData {
    pub providers: Vec<AiProviderRecord>,
    pub selected_provider_id: Option<String>,
    pub selected_model_id: Option<String>,
    pub data_mode: String,
    pub generated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModelsData {
    pub models: Vec<AiModelRecord>,
    pub selected_provider_id: Option<String>,
    pub selected_model_id: Option<String>,
    pub data_mode: String,
    pub generated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderMutationResult {
    pub provider_id: String,
    pub connection_state: AiProviderConnectionState,
    pub credential_configured: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderCredentialResult {
    pub provider_id: String,
    pub credential_configured: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderTestResult {
    pub provider_id: String,
    pub connection_state: AiProviderConnectionState,
    pub checked_at: String,
    pub latency_ms: i64,
    pub discovered_model_count: i64,
    /// رمز خطأ آمن فقط — لا نص استجابة ولا تفصيل شبكي ولا أي مادة سرية.
    pub safe_error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModelDiscoveryResult {
    pub provider_id: String,
    pub models: Vec<AiModelRecord>,
    pub discovered_at: String,
    pub safe_error_code: Option<String>,
}

// ===== مدخلات الأوامر =====

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureAiProviderInput {
    pub provider_id: String,
    pub kind: AiProviderKind,
    pub display_name: String,
    pub base_url: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderIdInput {
    pub provider_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAiProviderCredentialInput {
    pub provider_id: String,
    /// يُستهلك مرة واحدة ويُخزَّن في مخزن آمن مدعوم بنظام التشغيل؛ لا يُسجَّل ولا يُعاد.
    pub credential: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectAiModelInput {
    pub provider_id: String,
    pub model_id: String,
}

// ===== سجلات المخزن (طبقة البنية التحتية) =====

/// صف مثيل موفر كما هو في ai_provider_registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderInstanceRecord {
    pub id: String,
    pub tenant_id: String,
    pub provider_id: String,
    pub kind: AiProviderKind,
    pub display_name: String,
    pub base_url: String,
    pub transport: AiTransportKind,
    pub enabled: bool,
    pub connection_state: AiProviderConnectionState,
    pub connection_checked_at: Option<String>,
    pub credential_configured: bool,
    pub built_in: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// قدرات نموذج مكتشفة — كل حقل اختياري بقصد: الغياب يعني «غير معروف».
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiscoveredCapabilities {
    pub context_limit: Option<i64>,
    pub supports_tools: Option<bool>,
    pub supports_structured_output: Option<bool>,
    pub supports_vision: Option<bool>,
}

/// صف نموذج كما هو في ai_model_registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRegistryRecord {
    pub id: String,
    pub tenant_id: String,
    pub provider_kind: AiProviderKind,
    pub model_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub provider_id: Option<String>,
    pub capabilities: DiscoveredCapabilities,
    pub capability_source: AiCapabilitySource,
    pub available: bool,
    pub last_discovered_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::{
        AiCapabilitySource, AiModelRecord, AiProviderConnectionState, AiProviderKind,
        AiProviderRecord, AiTransportKind,
    };

    #[test]
    fn provider_kind_codes_match_the_frozen_typescript_contract() {
        let expected = [
            (AiProviderKind::Ollama, "OLLAMA"),
            (AiProviderKind::LmStudio, "LM_STUDIO"),
            (AiProviderKind::OpenAiCompatible, "OPENAI_COMPATIBLE"),
            (AiProviderKind::OpenAi, "OPENAI"),
            (AiProviderKind::DeepSeek, "DEEPSEEK"),
            (AiProviderKind::Anthropic, "ANTHROPIC"),
            (AiProviderKind::Qwen, "QWEN"),
            (AiProviderKind::Zai, "ZAI"),
            (AiProviderKind::Xai, "XAI"),
            (AiProviderKind::OpenRouter, "OPENROUTER"),
        ];
        assert_eq!(expected.len(), 10, "العقد يعرّف عشرة أنواع موفر");
        for (kind, code) in expected {
            assert_eq!(kind.as_code(), code);
            assert_eq!(
                AiProviderKind::parse_code(code).expect("التحليل يجب أن ينجح"),
                kind
            );
        }
        // الصفوف التاريخية: 'LOCAL' تُترجم إلى مزود Ollama صراحةً بلا تخمين.
        assert_eq!(
            AiProviderKind::from_database_code("LOCAL").expect("LOCAL مقبول"),
            AiProviderKind::Ollama
        );
        assert!(AiProviderKind::parse_code("LOCAL").is_err());
        assert!(AiProviderKind::parse_code("GOOGLE").is_err());
        assert!(AiProviderKind::parse_code("MISTRAL").is_err());
    }

    #[test]
    fn connection_state_codes_match_the_frozen_contract() {
        for (state, code) in [
            (AiProviderConnectionState::Connected, "CONNECTED"),
            (AiProviderConnectionState::Disconnected, "DISCONNECTED"),
            (AiProviderConnectionState::Unavailable, "UNAVAILABLE"),
            (AiProviderConnectionState::Unverified, "UNVERIFIED"),
        ] {
            assert_eq!(state.as_code(), code);
            assert_eq!(
                AiProviderConnectionState::parse_code(code).expect("التحليل"),
                state
            );
        }
    }

    #[test]
    fn capability_source_codes_match_the_frozen_contract() {
        for (source, code) in [
            (AiCapabilitySource::Discovered, "DISCOVERED"),
            (AiCapabilitySource::Declared, "DECLARED"),
            (AiCapabilitySource::Unknown, "UNKNOWN"),
        ] {
            assert_eq!(source.as_code(), code);
            assert_eq!(
                AiCapabilitySource::parse_code(code).expect("التحليل"),
                source
            );
        }
    }

    #[test]
    fn transport_kind_codes_match_the_migration_constraint() {
        for (kind, code) in [
            (AiTransportKind::HttpLoopback, "HTTP_LOOPBACK"),
            (AiTransportKind::Https, "HTTPS"),
        ] {
            assert_eq!(kind.as_database_code(), code);
            assert_eq!(
                AiTransportKind::parse_database_code(code).expect("التحليل"),
                kind
            );
        }
    }

    #[test]
    fn local_providers_do_not_require_credentials_and_remote_ones_do() {
        assert!(!AiProviderKind::Ollama.requires_credential());
        assert!(!AiProviderKind::LmStudio.requires_credential());
        for remote in [
            AiProviderKind::OpenAi,
            AiProviderKind::DeepSeek,
            AiProviderKind::Anthropic,
            AiProviderKind::Qwen,
            AiProviderKind::Zai,
            AiProviderKind::Xai,
            AiProviderKind::OpenRouter,
            AiProviderKind::OpenAiCompatible,
        ] {
            assert!(remote.requires_credential(), "{remote:?} يتطلب سراً");
            assert!(!remote.is_local());
        }
    }

    // صدق القدرات: الغياب يُسلسَل null لا صفراً ولا false.
    #[test]
    fn unknown_capabilities_serialize_as_null_and_never_as_zero_or_false() {
        let record = AiModelRecord {
            model_id: "mystery-model".to_owned(),
            provider_id: "PROVIDER_ONE".to_owned(),
            provider_kind: AiProviderKind::OpenAiCompatible,
            provider_display_name: "Custom".to_owned(),
            display_name: "Mystery".to_owned(),
            available: true,
            selected: false,
            context_limit: None,
            supports_tools: None,
            supports_structured_output: None,
            supports_vision: None,
            capability_source: AiCapabilitySource::Unknown,
            last_discovered_at: None,
        };
        let json = serde_json::to_string(&record).expect("التسلسل يجب أن ينجح");
        for field in [
            "\"contextLimit\":null",
            "\"supportsTools\":null",
            "\"supportsStructuredOutput\":null",
            "\"supportsVision\":null",
            "\"lastDiscoveredAt\":null",
        ] {
            assert!(json.contains(field), "الحقل {field} يجب أن يكون null");
        }
        assert!(!json.contains("\"contextLimit\":0"), "لا صفر مخترع");
        assert!(!json.contains("\"supportsTools\":false"), "لا false مخترع");
        assert!(json.contains("\"capabilitySource\":\"UNKNOWN\""));

        // والقيم المتحقق منها تُسلسَل كما هي.
        let known = AiModelRecord {
            context_limit: Some(131_072),
            supports_tools: Some(true),
            supports_structured_output: Some(false),
            supports_vision: Some(true),
            ..record
        };
        let json = serde_json::to_string(&known).expect("التسلسل");
        assert!(json.contains("\"contextLimit\":131072"));
        assert!(json.contains("\"supportsTools\":true"));
        assert!(json.contains("\"supportsStructuredOutput\":false"));
        assert!(json.contains("\"supportsVision\":true"));
    }

    #[test]
    fn provider_record_serializes_with_camel_case_and_never_with_a_secret_field() {
        let record = AiProviderRecord {
            provider_id: "OPENAI_PRIMARY".to_owned(),
            kind: AiProviderKind::OpenAi,
            display_name: "OpenAI Primary".to_owned(),
            base_url: "https://api.openai.com".to_owned(),
            enabled: true,
            connection_state: AiProviderConnectionState::Unverified,
            connection_checked_at: None,
            credential_configured: true,
            requires_credential: true,
            supports_discovery: true,
            model_count: 3,
            built_in: false,
        };
        let json = serde_json::to_string(&record).expect("التسلسل");
        for field in [
            "\"providerId\"",
            "\"displayName\"",
            "\"baseUrl\"",
            "\"connectionState\"",
            "\"connectionCheckedAt\":null",
            "\"credentialConfigured\":true",
            "\"requiresCredential\":true",
            "\"supportsDiscovery\":true",
            "\"modelCount\":3",
            "\"builtIn\":false",
        ] {
            assert!(json.contains(field), "الحقل {field} يجب أن يوجد");
        }
        // لا حقل سر في العقد المُسلسَل إطلاقاً.
        for forbidden in ["credential\":", "apiKey", "token", "secret", "password"] {
            assert!(
                !json.contains(forbidden),
                "لا يجوز أن يظهر {forbidden} في عقد الموفر"
            );
        }
    }
}
