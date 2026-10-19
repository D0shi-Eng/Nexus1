use crate::{
    domain::{
        error::AppError,
        provider::{
            DiscoveredModel, ModelId, ModelProvider, ProviderFailure, ProviderGeneration,
            ProviderHealth, ProviderId, ProviderStructuredGeneration, ProviderToolDefinition,
            ProviderToolResponse, RuntimeExecutionInput,
        },
    },
    infrastructure::{
        database::AppState,
        model_registry_repository::ModelRegistryRepository,
        providers::ollama::{OllamaProviderAdapter, OllamaProviderConfig},
    },
};

pub struct ProviderRuntimeService {
    provider: OllamaProviderAdapter,
}

impl ProviderRuntimeService {
    pub fn local_ollama() -> Self {
        Self {
            provider: OllamaProviderAdapter::new(OllamaProviderConfig::local_default()),
        }
    }

    pub fn provider_id(&self) -> ProviderId {
        self.provider.provider_id()
    }

    pub fn health(&self) -> ProviderHealth {
        self.provider.health()
    }

    pub fn discover_models(&self) -> Result<Vec<DiscoveredModel>, AppError> {
        self.provider.discover_models().map_err(AppError::from)
    }

    // توليد نصي عادي — المسار المغلق الذي يرفض أي استدعاءات أدوات.
    pub fn generate_text(
        &self,
        model_id: &ModelId,
        text: &str,
    ) -> Result<ProviderGeneration, ProviderFailure> {
        let input = RuntimeExecutionInput::new(text)?;
        self.provider.generate_non_streaming(model_id, &input)
    }

    // المسار الواعي بالأدوات (MP03) — استدعاءات مهيكلة فقط عبر المزود.
    pub fn generate_text_with_tools(
        &self,
        model_id: &ModelId,
        text: &str,
        tools: &[ProviderToolDefinition],
    ) -> Result<ProviderToolResponse, ProviderFailure> {
        let input = RuntimeExecutionInput::new(text)?;
        self.provider.generate_with_tools(model_id, &input, tools)
    }
}

impl ModelProvider for ProviderRuntimeService {
    fn provider_id(&self) -> ProviderId {
        self.provider.provider_id()
    }

    fn health(&self) -> ProviderHealth {
        self.provider.health()
    }

    fn discover_models(&self) -> Result<Vec<DiscoveredModel>, ProviderFailure> {
        self.provider.discover_models()
    }

    fn generate_non_streaming(
        &self,
        model_id: &ModelId,
        input: &RuntimeExecutionInput,
    ) -> Result<ProviderGeneration, ProviderFailure> {
        self.provider.generate_non_streaming(model_id, input)
    }

    fn generate_with_tools(
        &self,
        model_id: &ModelId,
        input: &RuntimeExecutionInput,
        tools: &[ProviderToolDefinition],
    ) -> Result<ProviderToolResponse, ProviderFailure> {
        self.provider.generate_with_tools(model_id, input, tools)
    }

    // المسار المهيكل (MP04): تمرير مباشر — لا اكتظاف بالافتراضي الفاشل المغلق
    fn generate_structured(
        &self,
        model_id: &ModelId,
        input: &RuntimeExecutionInput,
    ) -> Result<ProviderStructuredGeneration, ProviderFailure> {
        self.provider.generate_structured(model_id, input)
    }
}

pub struct ProviderResolver;

impl ProviderResolver {
    pub fn resolve_enabled_local_model(
        state: &AppState,
        provider: &dyn ModelProvider,
    ) -> Result<ModelId, AppError> {
        Self::resolve_enabled_local_model_with_capability(state, provider)
            .map(|(model_id, _supports_tools)| model_id)
    }

    // العقد نفسه مع قدرة الأدوات المكتشفة حتمياً: تقاطع المفعّل مع المكتشف
    // يجب أن يكون بالضبط نموذجاً واحداً وإلا فشل مغلق بلا أي بدل صامت (§26).
    pub fn resolve_enabled_local_model_with_capability(
        state: &AppState,
        provider: &dyn ModelProvider,
    ) -> Result<(ModelId, bool), AppError> {
        let discovered_models = provider.discover_models().map_err(AppError::from)?;
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        let enabled_model_ids =
            ModelRegistryRepository::list_enabled_local_models(&connection, &state.tenant_id())?;
        let matching_models = discovered_models
            .iter()
            .filter(|discovered_model| {
                enabled_model_ids
                    .iter()
                    .any(|enabled_model_id| enabled_model_id == &discovered_model.id)
            })
            .collect::<Vec<_>>();

        match matching_models.as_slice() {
            [model] => Ok((model.id.clone(), model.supports_tools)),
            _ => Err(AppError::ModelUnavailable),
        }
    }
}

#[cfg(test)]
#[path = "provider_runtime_service_tests.rs"]
mod tests;
