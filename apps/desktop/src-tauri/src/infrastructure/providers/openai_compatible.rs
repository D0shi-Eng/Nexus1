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
// محوّل OpenAI-compatible (LM Studio · OpenAI · DeepSeek · Qwen · Z.ai · xAI ·
// OpenRouter · أي خادم بعقد OpenAI).
// ---------------------------------------------------------------------
// مبادئ ملزمة:
//   * العنوان الأساسي يمر إلزاماً عبر سياسة الوجهة الشبكية قبل أي طلب:
//     https لغير الحلقي، http للحلقي وحده، بلا بيانات اعتماد في العنوان،
//     وبلا نطاقات محظورة. الفشل يحمل رمزاً آمناً واحداً.
//   * المفتاح سر اختياري يُقرأ من المخزن الآمن لحظة الطلب فقط. لا يُسجَّل ولا
//     يُطبع ولا يظهر في أي خطأ: كل الأخطاء رموز ثابتة، وأي نص من الاستجابة
//     يُهمَل عند الفشل.
//   * كل استجابة محدودة الحجم ومفحوصة النوع، ومهلة قصيرة، وبلا تحويلات،
//     وبلا وسيط.
//   * القدرات لا تُخترع: `GET /models` لا يُرجع نافذة سياق ولا أدوات في العقد
//     الشائع، فلا ندّعيها — تبقى غير معروفة (None) إلا ما نستطيع إثباته.
//   * الأدوات والمخرجات المهيكلة: لا يُدَّعى أي منهما. `generate_with_tools`
//     و`generate_structured` يبقيان على الفشل المغلق الافتراضي للعقد لأن
//     دعمهما يختلف اختلافاً جوهرياً بين الموفرين، والادعاء الكاذب فيهما
//     أخطر من عدم الدعم. هذا قرار صريح لا نقص في التنفيذ.
// =====================================================================

/// حد الاستكشاف: 1MB — يكفي مئات النماذج ولا يفتح باب استجابة ضخمة.
const MAX_DISCOVERY_BYTES: usize = 1024 * 1024;
/// حد التوليد: 64KB نص — أكبر من أي إجابة مقصودة في هذا المنتج.
const MAX_GENERATION_BYTES: usize = 64 * 1024;
/// مهلة الاستكشاف/الفحص — قصيرة عمداً (فشل سريع).
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
/// مهلة التوليد — أوسع لأن النماذج السحابية قد تتأخر.
const GENERATION_TIMEOUT: Duration = Duration::from_secs(120);

/// مفاتيح الطلب المؤقتة — لا تُخزَّن ولا تُنسخ خارج عمر الطلب.
pub struct OpenAiCompatibleConfig {
    /// الوجهة المتحقق منها عبر سياسة الشبكة (تحمل المخطط والمضيف والمنفذ).
    pub destination: AllowedDestination,
    /// مسار تعداد النماذج، مثال "/v1/models".
    pub models_path: String,
    /// مسار المحادثة، مثال "/v1/chat/completions".
    pub chat_path: String,
    /// المفتاح السري لحظة الطلب — None يعني بلا مصادقة (مزود محلي).
    credential: Option<String>,
}

impl OpenAiCompatibleConfig {
    pub fn new(
        destination: AllowedDestination,
        models_path: impl Into<String>,
        chat_path: impl Into<String>,
        credential: Option<String>,
    ) -> Self {
        Self {
            destination,
            models_path: models_path.into(),
            chat_path: chat_path.into(),
            credential,
        }
    }

    /// عنوان استكشاف النماذج المبني من الوجهة المثبَّتة.
    pub fn models_url(&self) -> String {
        format!(
            "{}{}",
            self.destination.origin(),
            normalise_path(&self.models_path)
        )
    }

    /// عنوان المحادثة.
    pub fn chat_url(&self) -> String {
        format!(
            "{}{}",
            self.destination.origin(),
            normalise_path(&self.chat_path)
        )
    }
}

fn normalise_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    }
}

pub struct OpenAiCompatibleAdapter {
    config: OpenAiCompatibleConfig,
    discovery_timeout: Duration,
    generation_timeout: Duration,
    /// معرّف الموفر المنطقي الذي ينتجه هذا المحوّل (مشتق من صف الموفر).
    provider_id: ProviderId,
}

impl OpenAiCompatibleAdapter {
    pub fn new(provider_id: ProviderId, config: OpenAiCompatibleConfig) -> Self {
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
        config: OpenAiCompatibleConfig,
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

    /// وكيل HTTP مبني حصراً من قيود النقل التي فرضتها سياسة الوجهة، **وعلى
    /// العناوين المُتحقَّق منها وحدها** (تثبيت DNS — لا حلّ ثانٍ لحظة الاتصال).
    fn create_agent(destination: &AllowedDestination, timeout: Duration) -> Agent {
        super::pinned_resolver::create_pinned_agent(destination, timeout)
    }

    /// قراءة جسم الاستجابة ضمن الحد وبعد فحص النوع — كل فشل رمز آمن واحد.
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
        let mut request = Self::create_agent(&self.config.destination, self.discovery_timeout)
            .get(self.config.models_url());
        if let Some(credential) = self.config.credential.as_deref() {
            request = request.header("authorization", &format!("Bearer {credential}"));
        }
        let response = request.call().map_err(map_transport_error)?;
        let body = Self::read_json_response(response, MAX_DISCOVERY_BYTES)?;
        let payload: OpenAiModelsResponse =
            serde_json::from_str(&body).map_err(|_| ProviderFailure::InvalidResponse)?;

        payload
            .data
            .into_iter()
            .map(|entry| {
                let model_id = ModelId::new(entry.id)?;
                Ok(DiscoveredModel {
                    display_name: model_id.as_str().to_owned(),
                    id: model_id,
                    modified_at: entry.created.map(|seconds| {
                        chrono::DateTime::from_timestamp(seconds, 0)
                            .map(|instant| instant.to_rfc3339())
                            .unwrap_or_default()
                    }),
                    size_bytes: None,
                    // العقد الشائع لـ /models لا يعلن قدرة الأدوات — لا ادعاء.
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
        let payload = OpenAiChatRequest {
            model: model_id.as_str(),
            messages: vec![OpenAiChatMessage {
                role: "user",
                content: input.as_str(),
            }],
            stream: false,
        };
        let request_json =
            serde_json::to_string(&payload).map_err(|_| ProviderFailure::InvalidResponse)?;
        let mut request = Self::create_agent(&self.config.destination, self.generation_timeout)
            .post(self.config.chat_url())
            .header("content-type", "application/json");
        if let Some(credential) = self.config.credential.as_deref() {
            request = request.header("authorization", &format!("Bearer {credential}"));
        }
        let response = request.send(request_json).map_err(map_transport_error)?;
        let body = Self::read_json_response(response, MAX_GENERATION_BYTES)?;
        let payload: OpenAiChatResponse =
            serde_json::from_str(&body).map_err(|_| ProviderFailure::InvalidResponse)?;

        // استدعاء أداة في مسار النص العادي ⇒ رفض مغلق (لا اشتقاق من النثر).
        if payload
            .choices
            .iter()
            .any(|choice| choice.message.tool_calls.is_some())
        {
            return Err(ProviderFailure::ToolCallUnsupported);
        }
        // أكثر من خيار واحد: العقد يطلب اختياراً واحداً صريحاً.
        if payload.choices.len() != 1 {
            return Err(ProviderFailure::InvalidResponse);
        }
        let choice = payload
            .choices
            .into_iter()
            .next()
            .ok_or(ProviderFailure::InvalidResponse)?;
        let content = choice
            .message
            .content
            .ok_or(ProviderFailure::InvalidResponse)?;
        if content.is_empty() || content.len() > MAX_GENERATION_BYTES {
            return Err(ProviderFailure::InvalidResponse);
        }
        // صدق الهوية: النموذج المُعاد يجب أن يكون المطلوب أو لقطته الموثّقة
        // (مثل `gpt-4o` ⇒ `gpt-4o-2024-08-06`). المساواة الحرفية وحدها كانت
        // ترفض استجابة سليمة من OpenAI وAnthropic وxAI.
        let response_model_id = ModelId::new(payload.model)?;
        if !model_id.accepts_reported(&response_model_id) {
            return Err(ProviderFailure::InvalidResponse);
        }

        Ok(ProviderGeneration {
            model_id: response_model_id,
            output: content,
            generated_at: Utc::now().to_rfc3339(),
        })
    }
}

impl ModelProvider for OpenAiCompatibleAdapter {
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

    // لا تجاوز للافتراضي: الأدوات تبقى غير مدعومة ما لم يعلن الموفر دعمها.
    // تنفيذ `generate_with_tools` يتطلب دعماً مثبتاً لكل موفر على حدة،
    // والادعاء بغير المثبت يخالف قاعدة عدم اختراع القدرات.
}

/// تحويل فشل النقل إلى رمز ثابت — لا نص من الخادم ولا عنوان ولا اعتماد.
fn map_transport_error(error: ureq::Error) -> ProviderFailure {
    match error {
        ureq::Error::Timeout(_) => ProviderFailure::Timeout,
        ureq::Error::BodyExceedsLimit(_) => ProviderFailure::OutputTooLarge,
        // 401/403: غياب صلاحية أو مفتاح خاطئ. لا يُكشف نص الخادم.
        ureq::Error::StatusCode(code) if code == 401 || code == 403 => {
            ProviderFailure::CredentialMissing
        }
        // رفض الاتصال (لا خدمة تستمع) وكل عطل نقل آخر ⇒ غير متاح.
        // لا تفصيل نظام ولا منفذ ولا مضيف يخرج من هنا.
        _ => ProviderFailure::Unavailable,
    }
}

// ===== أشكال الاستجابة =====

#[derive(Debug, serde::Deserialize)]
struct OpenAiModelsResponse {
    data: Vec<OpenAiModelEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct OpenAiModelEntry {
    id: String,
    created: Option<i64>,
}

#[derive(Serialize)]
struct OpenAiChatRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAiChatMessage<'a>>,
    stream: bool,
}

#[derive(Serialize)]
struct OpenAiChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, serde::Deserialize)]
struct OpenAiChatResponse {
    model: String,
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, serde::Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
}

#[derive(Debug, serde::Deserialize)]
struct OpenAiResponseMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<serde_json::Value>,
}

// لا غلاف لسياسة الوجهة هنا: build_adapter يستدعي NetworkPolicy مباشرةً،
// والاختبارات تستدعيها بنفسها عبر `network_policy_for_tests` أدناه.
#[cfg(test)]
fn evaluate_destination(base_url: &str) -> Result<AllowedDestination, ProviderFailure> {
    use crate::domain::network_policy::{NetworkPolicy, SystemResolver};

    let resolver = SystemResolver;
    NetworkPolicy::new(&resolver)
        .evaluate(base_url)
        .map_err(|rejection| ProviderFailure::EndpointRejected(rejection.code()))
}

#[cfg(test)]
#[path = "openai_compatible_tests.rs"]
mod tests;
