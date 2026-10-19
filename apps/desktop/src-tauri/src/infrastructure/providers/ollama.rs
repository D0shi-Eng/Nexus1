use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use ureq::Agent;

use crate::domain::provider::{
    DiscoveredModel, ModelId, ModelProvider, ProviderFailure, ProviderGeneration, ProviderHealth,
    ProviderId, ProviderStructuredGeneration, ProviderToolCall, ProviderToolDefinition,
    ProviderToolResponse, RuntimeExecutionInput,
};

const LOOPBACK_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 11434;
const HEALTH_TIMEOUT: Duration = Duration::from_secs(2);
// حد التوليد 120ث (رُفع من 30ث في MP04): qwen3:4b محلياً على عتاد مكتبي
// يتجاوز 30ث تحت حمل متسلسل — الحد يبقى محدوداً والفشل يبقى مغلقاً (§98).
const GENERATION_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_DISCOVERY_BYTES: usize = 256 * 1024;
const MAX_GENERATION_BYTES: usize = 32 * 1024;
const MAX_TOOLS_REQUEST_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OllamaProviderConfig {
    port: u16,
}

impl OllamaProviderConfig {
    pub fn local_default() -> Self {
        Self { port: DEFAULT_PORT }
    }

    #[cfg(test)]
    pub(crate) fn for_loopback_port(port: u16) -> Result<Self, ProviderFailure> {
        if port == 0 {
            return Err(ProviderFailure::Unavailable);
        }

        Ok(Self { port })
    }

    fn endpoint(&self, path: &str) -> String {
        format!("http://{LOOPBACK_HOST}:{}{path}", self.port)
    }
}

pub struct OllamaProviderAdapter {
    config: OllamaProviderConfig,
    health_timeout: Duration,
    generation_timeout: Duration,
}

impl OllamaProviderAdapter {
    pub fn new(config: OllamaProviderConfig) -> Self {
        Self {
            config,
            health_timeout: HEALTH_TIMEOUT,
            generation_timeout: GENERATION_TIMEOUT,
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        config: OllamaProviderConfig,
        health_timeout: Duration,
        generation_timeout: Duration,
    ) -> Self {
        Self {
            config,
            health_timeout,
            generation_timeout,
        }
    }

    fn create_agent(timeout: Duration) -> Agent {
        Agent::config_builder()
            .timeout_global(Some(timeout))
            .max_redirects(0)
            .proxy(None)
            .build()
            .new_agent()
    }

    fn read_json_response(
        &self,
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

    fn request_tags(&self) -> Result<Vec<DiscoveredModel>, ProviderFailure> {
        let response = Self::create_agent(self.health_timeout)
            .get(self.config.endpoint("/api/tags"))
            .call()
            .map_err(map_transport_error)?;
        let body = self.read_json_response(response, MAX_DISCOVERY_BYTES)?;
        let payload: OllamaTagsResponse =
            serde_json::from_str(&body).map_err(|_| ProviderFailure::InvalidResponse)?;

        payload
            .models
            .into_iter()
            .map(DiscoveredModel::try_from)
            .collect()
    }

    fn request_generation(
        &self,
        model_id: &ModelId,
        input: &RuntimeExecutionInput,
    ) -> Result<ProviderGeneration, ProviderFailure> {
        let payload = OllamaGenerateRequest {
            model: model_id.as_str(),
            prompt: input.as_str(),
            stream: false,
        };
        let request_json =
            serde_json::to_string(&payload).map_err(|_| ProviderFailure::InvalidResponse)?;
        let response = Self::create_agent(self.generation_timeout)
            .post(self.config.endpoint("/api/generate"))
            .header("content-type", "application/json")
            .send(request_json)
            .map_err(map_transport_error)?;
        let body = self.read_json_response(response, MAX_GENERATION_BYTES)?;
        let payload: OllamaGenerateResponse =
            serde_json::from_str(&body).map_err(|_| ProviderFailure::InvalidResponse)?;

        if payload.tool_calls.is_some() || payload.function_call.is_some() {
            return Err(ProviderFailure::ToolCallUnsupported);
        }
        if !payload.done
            || payload.response.is_empty()
            || payload.response.len() > MAX_GENERATION_BYTES
        {
            return Err(ProviderFailure::InvalidResponse);
        }

        let response_model_id = ModelId::new(payload.model)?;
        if response_model_id != *model_id {
            return Err(ProviderFailure::InvalidResponse);
        }

        Ok(ProviderGeneration {
            model_id: response_model_id,
            output: payload.response,
            generated_at: payload
                .created_at
                .unwrap_or_else(|| Utc::now().to_rfc3339()),
        })
    }

    // المسار الواعي بالأدوات (MP03): استدعاءات مهيكلة من واجهة /api/chat فقط —
    // لا يُشتق أي استدعاء أداة من نثر النص. أكثر من استدعاء واحد في استجابة
    // واحدة يُرفض فشلاً مغلقاً (حد نداء الأدوات صارم).
    fn request_chat_with_tools(
        &self,
        model_id: &ModelId,
        input: &RuntimeExecutionInput,
        tools: &[ProviderToolDefinition],
    ) -> Result<ProviderToolResponse, ProviderFailure> {
        for tool in tools {
            tool.validate()?;
        }
        let payload = OllamaChatRequest {
            model: model_id.as_str(),
            messages: vec![OllamaChatMessage {
                role: "user",
                content: input.as_str(),
            }],
            stream: false,
            think: false,
            tools: tools
                .iter()
                .map(|tool| {
                    let parameters = serde_json::from_str(&tool.parameters_json)
                        .map_err(|_| ProviderFailure::InvalidResponse)?;
                    Ok(OllamaToolDefinition {
                        function: OllamaToolFunction {
                            name: &tool.name,
                            description: &tool.description,
                            parameters,
                        },
                    })
                })
                .collect::<Result<Vec<_>, ProviderFailure>>()?,
        };
        let request_json =
            serde_json::to_string(&payload).map_err(|_| ProviderFailure::InvalidResponse)?;
        if request_json.len() > MAX_TOOLS_REQUEST_BYTES + MAX_GENERATION_BYTES {
            return Err(ProviderFailure::InputTooLarge);
        }
        let response = Self::create_agent(self.generation_timeout)
            .post(self.config.endpoint("/api/chat"))
            .header("content-type", "application/json")
            .send(request_json)
            .map_err(map_transport_error)?;
        let body = self.read_json_response(response, MAX_GENERATION_BYTES)?;
        let payload: OllamaChatResponse =
            serde_json::from_str(&body).map_err(|_| ProviderFailure::InvalidResponse)?;

        let response_model_id = ModelId::new(payload.model)?;
        if !model_id.accepts_reported(&response_model_id) || !payload.done {
            return Err(ProviderFailure::InvalidResponse);
        }

        match payload.message.tool_calls.len() {
            0 => {
                if payload.message.content.is_empty()
                    || payload.message.content.len() > MAX_GENERATION_BYTES
                {
                    return Err(ProviderFailure::InvalidResponse);
                }
                Ok(ProviderToolResponse::Text(ProviderGeneration {
                    model_id: response_model_id,
                    output: payload.message.content,
                    generated_at: payload
                        .created_at
                        .unwrap_or_else(|| Utc::now().to_rfc3339()),
                }))
            }
            1 => {
                let call = &payload.message.tool_calls[0].function;
                let arguments = match &call.arguments {
                    Some(value) if value.is_object() => value.clone(),
                    Some(serde_json::Value::String(text)) => {
                        serde_json::from_str(text).map_err(|_| ProviderFailure::InvalidResponse)?
                    }
                    _ => return Err(ProviderFailure::InvalidResponse),
                };
                if call.name.is_empty() || call.name.len() > 64 {
                    return Err(ProviderFailure::InvalidResponse);
                }
                Ok(ProviderToolResponse::ToolCall(ProviderToolCall {
                    tool_name: call.name.clone(),
                    arguments,
                }))
            }
            _ => Err(ProviderFailure::ToolCallUnsupported),
        }
    }
    // المسار المهيكل (MP04): صيغة JSON مقيدة من /api/generate — للمخطط/المدقق.
    // الاستجابة يجب أن تكون JSON كائناً صالحاً وإلا فشل مغلق (لا اشتقاق من نثر).
    fn request_structured_generation(
        &self,
        model_id: &ModelId,
        input: &RuntimeExecutionInput,
    ) -> Result<ProviderStructuredGeneration, ProviderFailure> {
        let payload = OllamaStructuredRequest {
            model: model_id.as_str(),
            prompt: input.as_str(),
            stream: false,
            format: "json",
        };
        let request_json =
            serde_json::to_string(&payload).map_err(|_| ProviderFailure::InvalidResponse)?;
        let response = Self::create_agent(self.generation_timeout)
            .post(self.config.endpoint("/api/generate"))
            .header("content-type", "application/json")
            .send(request_json)
            .map_err(map_transport_error)?;
        let body = self.read_json_response(response, MAX_GENERATION_BYTES)?;
        let payload: OllamaGenerateResponse =
            serde_json::from_str(&body).map_err(|_| ProviderFailure::InvalidResponse)?;
        if payload.tool_calls.is_some() || payload.function_call.is_some() {
            return Err(ProviderFailure::ToolCallUnsupported);
        }
        if !payload.done || payload.response.is_empty() {
            return Err(ProviderFailure::InvalidResponse);
        }
        let response_model_id = ModelId::new(payload.model)?;
        if response_model_id != *model_id {
            return Err(ProviderFailure::InvalidResponse);
        }
        let value: serde_json::Value = serde_json::from_str(payload.response.trim())
            .map_err(|_| ProviderFailure::InvalidResponse)?;
        if !value.is_object() {
            return Err(ProviderFailure::InvalidResponse);
        }

        Ok(ProviderStructuredGeneration {
            model_id: response_model_id,
            value,
            generated_at: payload
                .created_at
                .unwrap_or_else(|| Utc::now().to_rfc3339()),
        })
    }
}

impl ModelProvider for OllamaProviderAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::ollama_local()
    }

    fn health(&self) -> ProviderHealth {
        match self.request_tags() {
            Ok(_) => ProviderHealth::Available,
            Err(_) => ProviderHealth::Unavailable,
        }
    }

    fn discover_models(&self) -> Result<Vec<DiscoveredModel>, ProviderFailure> {
        self.request_tags()
    }

    fn generate_non_streaming(
        &self,
        model_id: &ModelId,
        input: &RuntimeExecutionInput,
    ) -> Result<ProviderGeneration, ProviderFailure> {
        self.request_generation(model_id, input)
    }

    fn generate_with_tools(
        &self,
        model_id: &ModelId,
        input: &RuntimeExecutionInput,
        tools: &[ProviderToolDefinition],
    ) -> Result<ProviderToolResponse, ProviderFailure> {
        self.request_chat_with_tools(model_id, input, tools)
    }

    fn generate_structured(
        &self,
        model_id: &ModelId,
        input: &RuntimeExecutionInput,
    ) -> Result<ProviderStructuredGeneration, ProviderFailure> {
        self.request_structured_generation(model_id, input)
    }
}

#[derive(Debug, Deserialize)]
struct OllamaTagsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
    model: Option<String>,
    modified_at: Option<String>,
    size: Option<u64>,
    #[serde(default)]
    capabilities: Vec<String>,
}

impl TryFrom<OllamaModel> for DiscoveredModel {
    type Error = ProviderFailure;

    fn try_from(value: OllamaModel) -> Result<Self, Self::Error> {
        let model_id = ModelId::new(value.model.unwrap_or_else(|| value.name.clone()))?;
        if value.name.is_empty() || value.name.len() > 256 {
            return Err(ProviderFailure::InvalidResponse);
        }

        Ok(Self {
            id: model_id,
            display_name: value.name,
            modified_at: value.modified_at,
            size_bytes: value.size,
            // القدرة تُكشف حتمياً من المزود — لا افتراض ولا استنتاج من الاسم.
            supports_tools: value
                .capabilities
                .iter()
                .any(|capability| capability == "tools"),
        })
    }
}

#[derive(Serialize)]
struct OllamaGenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
}

#[derive(Serialize)]
struct OllamaStructuredRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    stream: bool,
    format: &'a str,
}

#[derive(Debug, Deserialize)]
struct OllamaGenerateResponse {
    model: String,
    created_at: Option<String>,
    response: String,
    done: bool,
    tool_calls: Option<serde_json::Value>,
    function_call: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct OllamaChatRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaChatMessage<'a>>,
    stream: bool,
    tools: Vec<OllamaToolDefinition<'a>>,
    // مسار الأدوات يرفض وضع التفكير المسبق للنماذج الهجينة (qwen3): التفكير
    // يُنتج محتوى فارغاً بلا استدعاء أداة، وقرار الأداة يبقى من قناة tool_calls
    // المهيكلة حصراً — لا اشتقاق من نثر التفكير (§52/§94).
    #[serde(rename = "think")]
    think: bool,
}

#[derive(Serialize)]
struct OllamaChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct OllamaToolDefinition<'a> {
    function: OllamaToolFunction<'a>,
}

#[derive(Serialize)]
struct OllamaToolFunction<'a> {
    name: &'a str,
    description: &'a str,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    model: String,
    created_at: Option<String>,
    done: bool,
    message: OllamaChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponseMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    tool_calls: Vec<OllamaChatToolCall>,
}

#[derive(Debug, Deserialize)]
struct OllamaChatToolCall {
    function: OllamaChatToolCallFunction,
}

#[derive(Debug, Deserialize)]
struct OllamaChatToolCallFunction {
    name: String,
    #[serde(default)]
    arguments: Option<serde_json::Value>,
}

fn map_transport_error(error: ureq::Error) -> ProviderFailure {
    match error {
        ureq::Error::Timeout(_) => ProviderFailure::Timeout,
        ureq::Error::BodyExceedsLimit(_) => ProviderFailure::OutputTooLarge,
        _ => ProviderFailure::Unavailable,
    }
}
