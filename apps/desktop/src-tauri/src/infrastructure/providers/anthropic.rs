use std::time::Duration;

use chrono::Utc;
use serde::Serialize;
use ureq::Agent;

use crate::domain::{
    network_policy::AllowedDestination,
    provider::{
        DiscoveredModel, ModelId, ModelProvider, ProviderFailure, ProviderGeneration,
        ProviderHealth, ProviderId, RuntimeExecutionInput,
    },
};

// =====================================================================
// محوّل Anthropic (واجهة الرسائل الخاصة بها لا واجهة OpenAI).
// ---------------------------------------------------------------------
// == مصادر التحقق (لا تخمين) ==
// راجعت تعريفات حزمة Anthropic الرسمية TypeScript على GitHub للتحقق من كل حقل
// مستعمل هنا، لأن واجهة Anthropic تختلف جوهرياً عن OpenAI:
//   * POST https://api.anthropic.com/v1/messages والترويسات `x-api-key`
//     و`anthropic-version: 2023-06-01`، والجسم يتطلب `max_tokens` و`messages`.
//   * استجابة Message: { id, type, role: "assistant", model, content: [ContentBlock],
//     stop_reason, stop_sequence, usage }.
//   * ContentBlock اتحاد يضم TextBlock { type: "text", text, citations }
//     وThinkingBlock وRedactedThinkingBlock وToolUseBlock وغيرها.
//   * GET https://api.anthropic.com/v1/models يُرجع صفحة { data, has_more,
//     first_id, last_id } وعناصرها ModelInfo { id, display_name, created_at,
//     max_input_tokens, capabilities, lifecycle, type }.
//   * capabilities.image_input.supported وcapabilities.structured_outputs.supported
//     حقول منطقية حقيقية — لذا نستطيع إعلان الرؤية والمخرجات المهيكلة كمكتشفة.
//   * لا يوجد حقل يعلن دعم أدوات العميل (function calling) في ModelInfo؛
//     capabilities.server_tools تخص أدوات الخادم لا أدواتنا. لذلك
//     `supports_tools` يبقى **غير معروف (None)** ولا ندّعي false ولا true.
//
// == الحدود الملزمة ==
//   * العنوان الأساسي يمر إلزاماً عبر سياسة الوجهة الشبكية.
//   * المفتاح يُقرأ من المخزن الآمن لحظة الطلب فقط ولا يظهر في أي خطأ أو سجل.
//   * كل استجابة محدودة الحجم ومفحوصة النوع، وبلا تحويلات وبلا وسيط.
//   * مسار الأدوات (`generate_with_tools`) يبقى على الفشل المغلق الافتراضي:
//     دعم استدعاء أدوات العميل لم يُتحقق منه في هذا المحوّل، والادعاء بغير
//     المثبت أخطر من عدم الدعم.
// =====================================================================

const MAX_DISCOVERY_BYTES: usize = 1024 * 1024;
const MAX_GENERATION_BYTES: usize = 64 * 1024;
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
const GENERATION_TIMEOUT: Duration = Duration::from_secs(120);
/// نسخة واجهة Anthropic — القيمة الموثّقة في مرجع الواجهة.
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// حدّ `max_tokens` المطلوب في كل طلب. قيمة محافظة صريحة لا مرنة:
/// إجابة أطول من ذلك تُقطع من الخادم بدل أن تفتح حدّاً كبيراً بلا داع.
const MAX_OUTPUT_TOKENS: u32 = 4096;

pub struct AnthropicConfig {
    pub destination: AllowedDestination,
    credential: Option<String>,
}

impl AnthropicConfig {
    pub fn new(destination: AllowedDestination, credential: Option<String>) -> Self {
        Self {
            destination,
            credential,
        }
    }

    fn messages_url(&self) -> String {
        format!("{}/v1/messages", self.destination.origin())
    }

    fn models_url(&self) -> String {
        format!("{}/v1/models", self.destination.origin())
    }
}

pub struct AnthropicAdapter {
    config: AnthropicConfig,
    discovery_timeout: Duration,
    generation_timeout: Duration,
    provider_id: ProviderId,
}

impl AnthropicAdapter {
    pub fn new(provider_id: ProviderId, config: AnthropicConfig) -> Self {
        Self {
            config,
            discovery_timeout: DISCOVERY_TIMEOUT,
            generation_timeout: GENERATION_TIMEOUT,
            provider_id,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        provider_id: ProviderId,
        config: AnthropicConfig,
        discovery_timeout: Duration,
        generation_timeout: Duration,
    ) -> Self {
        Self {
            config,
            discovery_timeout,
            generation_timeout,
            provider_id,
        }
    }

    /// وكيل HTTP مبني على العناوين المُتحقَّق منها وحدها (تثبيت DNS — لا حلّ
    /// ثانٍ لحظة الاتصال)، وبنفس قيود النقل التي فرضتها سياسة الوجهة.
    fn create_agent(destination: &AllowedDestination, timeout: Duration) -> Agent {
        super::pinned_resolver::create_pinned_agent(destination, timeout)
    }

    fn read_json_response(
        response: ureq::http::Response<ureq::Body>,
        limit: usize,
    ) -> Result<String, ProviderFailure> {
        let is_json = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json"));
        if !is_json {
            return Err(ProviderFailure::InvalidResponse);
        }

        response
            .into_body()
            .with_config()
            .limit(limit as u64)
            .lossy_utf8(false)
            .read_to_string()
            .map_err(map_transport_error)
    }

    fn request_models(&self) -> Result<Vec<DiscoveredModel>, ProviderFailure> {
        let agent = Self::create_agent(&self.config.destination, self.discovery_timeout);
        let mut request = agent
            .get(self.config.models_url())
            .header("anthropic-version", ANTHROPIC_VERSION);
        if let Some(credential) = self.config.credential.as_deref() {
            request = request.header("x-api-key", credential);
        }
        let response = request.call().map_err(map_transport_error)?;
        let body = Self::read_json_response(response, MAX_DISCOVERY_BYTES)?;
        let payload: AnthropicModelsPage =
            serde_json::from_str(&body).map_err(|_| ProviderFailure::InvalidResponse)?;

        payload
            .data
            .into_iter()
            .map(|entry| {
                let model_id = ModelId::new(entry.id)?;
                Ok(DiscoveredModel {
                    display_name: entry
                        .display_name
                        .unwrap_or_else(|| model_id.as_str().to_owned()),
                    id: model_id,
                    modified_at: entry.created_at,
                    size_bytes: None,
                    // لم تُتحقق قدرة أدوات العميل — لا ادعاء.
                    supports_tools: false,
                })
            })
            .collect()
    }

    fn request_generation(
        &self,
        model_id: &ModelId,
        input: &RuntimeExecutionInput,
    ) -> Result<ProviderGeneration, ProviderFailure> {
        let payload = AnthropicMessagesRequest {
            model: model_id.as_str(),
            max_tokens: MAX_OUTPUT_TOKENS,
            messages: vec![AnthropicInputMessage {
                role: "user",
                content: input.as_str(),
            }],
            stream: false,
        };
        let request_json =
            serde_json::to_string(&payload).map_err(|_| ProviderFailure::InvalidResponse)?;

        let agent = Self::create_agent(&self.config.destination, self.generation_timeout);
        let mut request = agent
            .post(self.config.messages_url())
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json");
        if let Some(credential) = self.config.credential.as_deref() {
            request = request.header("x-api-key", credential);
        }
        let response = request.send(request_json).map_err(map_transport_error)?;
        let body = Self::read_json_response(response, MAX_GENERATION_BYTES)?;
        let payload: AnthropicMessage =
            serde_json::from_str(&body).map_err(|_| ProviderFailure::InvalidResponse)?;

        // الهوية: النموذج المُعاد يجب أن يكون المطلوب أو لقطته الموثّقة
        // (واجهة Anthropic تُعيد لقطة مؤرّخة عند طلب اسم مستعار).
        let response_model_id = ModelId::new(payload.model)?;
        if !model_id.accepts_reported(&response_model_id) {
            return Err(ProviderFailure::InvalidResponse);
        }
        // الدور يجب أن يكون assistant — أي شيء آخر استجابة غير متوقعة.
        if payload.role != "assistant" {
            return Err(ProviderFailure::InvalidResponse);
        }

        // نجمع كل كتل النص فقط. وجود أي كتلة غير نصية (tool_use / thinking)
        // يعني أن الإجابة ليست نصاً بسيطاً ⇒ رفض مغلق بدل تفسير ناقص صامت.
        let mut text = String::new();
        for block in payload.content {
            let fragment = block.as_text().ok_or(ProviderFailure::InvalidResponse)?;
            text_push(&mut text, fragment)?;
        }
        if text.is_empty() {
            return Err(ProviderFailure::InvalidResponse);
        }

        Ok(ProviderGeneration {
            model_id: response_model_id,
            output: text,
            generated_at: Utc::now().to_rfc3339(),
        })
    }
}

/// تجميع كتل النص مع فرض الحد الأقصى — لا نص بلا سقف.
fn text_push(accumulated: &mut String, fragment: &str) -> Result<(), ProviderFailure> {
    if accumulated.len() + fragment.len() > MAX_GENERATION_BYTES {
        return Err(ProviderFailure::OutputTooLarge);
    }
    accumulated.push_str(fragment);
    Ok(())
}

impl ModelProvider for AnthropicAdapter {
    fn provider_id(&self) -> ProviderId {
        self.provider_id.clone()
    }

    fn health(&self) -> ProviderHealth {
        match self.request_models() {
            Ok(_) => ProviderHealth::Available,
            Err(_) => ProviderHealth::Unavailable,
        }
    }

    fn discover_models(&self) -> Result<Vec<DiscoveredModel>, ProviderFailure> {
        self.request_models()
    }

    fn generate_non_streaming(
        &self,
        model_id: &ModelId,
        input: &RuntimeExecutionInput,
    ) -> Result<ProviderGeneration, ProviderFailure> {
        self.request_generation(model_id, input)
    }

    // مسار الأدوات يبقى على الفشل المغلق الافتراضي: لم أتحقق من دعم استدعاء
    // أدوات العميل في واجهة الرسائل، ولا يجوز اختراع قدرة.
}

/// تحويل فشل النقل إلى رمز ثابت — لا نص من الخادم ولا عنوان ولا مفتاح.
fn map_transport_error(error: ureq::Error) -> ProviderFailure {
    match error {
        ureq::Error::Timeout(_) => ProviderFailure::Timeout,
        ureq::Error::BodyExceedsLimit(_) => ProviderFailure::OutputTooLarge,
        ureq::Error::StatusCode(code) if code == 401 || code == 403 => {
            ProviderFailure::CredentialMissing
        }
        _ => ProviderFailure::Unavailable,
    }
}

// ===== أشكال الطلب والاستجابة (مطابقة لحزمة Anthropic الرسمية) =====

#[derive(Serialize)]
struct AnthropicMessagesRequest<'a> {
    model: &'a str,
    /// إلزامي في واجهة الرسائل — لا طلب بلا سقف مخرجات.
    max_tokens: u32,
    messages: Vec<AnthropicInputMessage<'a>>,
    stream: bool,
}

#[derive(Serialize)]
struct AnthropicInputMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, serde::Deserialize)]
struct AnthropicMessage {
    model: String,
    role: String,
    content: Vec<AnthropicContentBlock>,
}

/// كتل المحتوى: نتعرف على النص، وكل ما عداه يُرفض فشلاً مغلقاً.
/// يُقرأ الوسم `type` أولاً ثم يُطلب حقل `text` صراحةً، فتبقى الكتلة غير
/// المعروفة (tool_use · thinking · redacted_thinking · بحث وبث…) مرفوضة
/// بلا فشل تحليل عام يُخفي السبب، وبلا أي تفسير جزئي للإجابة.
#[derive(Debug, serde::Deserialize)]
struct AnthropicContentBlock {
    #[serde(rename = "type")]
    block_type: String,
    #[serde(default)]
    text: Option<String>,
}

impl AnthropicContentBlock {
    /// نص الكتلة إن كانت كتلة نصية سليمة، وإلا `None` (يُفسَّر فشلاً مغلقاً).
    fn as_text(&self) -> Option<&str> {
        if self.block_type == "text" {
            self.text.as_deref()
        } else {
            None
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct AnthropicModelsPage {
    data: Vec<AnthropicModelInfo>,
}

/// حزمة Anthropic تعلن `display_name` و`created_at` كحقلين قابلين للعدم
/// (`| null`) في بعض الإصدارات، لذا نتعامل مع الغياب بلا فشل.
#[derive(Debug, serde::Deserialize)]
struct AnthropicModelInfo {
    id: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
}

// لا غلاف لسياسة الوجهة هنا: build_adapter يستدعي NetworkPolicy مباشرةً،
// والاختبارات تستدعيها بنفسها عبر السياسة العامة.
#[cfg(test)]
fn evaluate_destination(base_url: &str) -> Result<AllowedDestination, ProviderFailure> {
    use crate::domain::network_policy::{NetworkPolicy, SystemResolver};

    let resolver = SystemResolver;
    NetworkPolicy::new(&resolver)
        .evaluate(base_url)
        .map_err(|rejection| ProviderFailure::EndpointRejected(rejection.code()))
}

#[cfg(test)]
#[path = "anthropic_tests.rs"]
mod tests;
