use chrono::Utc;
use rusqlite::{params, Connection};

use crate::{
    application::provider_runtime_service::ProviderResolver,
    domain::{
        error::AppError,
        provider::{
            DiscoveredModel, ModelId, ModelProvider, ProviderFailure, ProviderGeneration,
            ProviderHealth, ProviderId, RuntimeExecutionInput,
        },
    },
    infrastructure::database::AppState,
};

struct TestProvider {
    models: Vec<DiscoveredModel>,
}

impl ModelProvider for TestProvider {
    fn provider_id(&self) -> ProviderId {
        ProviderId::ollama_local()
    }

    fn health(&self) -> ProviderHealth {
        ProviderHealth::Available
    }

    fn discover_models(&self) -> Result<Vec<DiscoveredModel>, ProviderFailure> {
        Ok(self.models.clone())
    }

    fn generate_non_streaming(
        &self,
        _model_id: &ModelId,
        _input: &RuntimeExecutionInput,
    ) -> Result<ProviderGeneration, ProviderFailure> {
        unreachable!("جيل النموذج ليس جزءاً من اختبار Resolver")
    }
}

fn discovered_model(model_id: &str) -> DiscoveredModel {
    DiscoveredModel {
        id: ModelId::new(model_id).expect("معرف نموذج الاختبار يجب أن يكون صالحاً"),
        display_name: model_id.to_owned(),
        modified_at: None,
        size_bytes: None,
        supports_tools: false,
    }
}

fn state_with_registry(entries: &[(&str, &str, bool)]) -> AppState {
    let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
    connection
        .execute_batch(include_str!("../../migrations/0001_initial.sql"))
        .expect("الهجرة الأساسية يجب أن تنجح");
    connection
        .execute_batch(include_str!("../../migrations/0005_ai_governance.sql"))
        .expect("هجرة سجل النماذج يجب أن تنجح");

    let now = Utc::now().to_rfc3339();
    for tenant_id in ["tenant-a", "tenant-b"] {
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at)
                 VALUES (?1, ?2, 'DEMO_SYNTHETIC', ?3, ?3)",
                params![tenant_id, tenant_id, &now],
            )
            .expect("المستأجر يجب أن ينشأ");
    }
    for (tenant_id, model_id, enabled) in entries {
        connection
            .execute(
                "INSERT INTO ai_model_registry (
                    id, tenant_id, provider_kind, model_id, display_name, enabled,
                    data_residency, allowed_classification_ceiling, requires_human_approval,
                    created_at, updated_at
                 ) VALUES (?1, ?2, 'LOCAL', ?3, ?3, ?4, 'LOCAL', 'PUBLIC', 1, ?5, ?5)",
                params![
                    format!("{tenant_id}-{model_id}"),
                    tenant_id,
                    model_id,
                    i64::from(*enabled),
                    &now,
                ],
            )
            .expect("صف سجل النموذج يجب أن ينشأ");
    }

    AppState::test_instance(connection, "tenant-a".to_owned(), "user-a".to_owned())
}

#[test]
fn resolves_the_single_enabled_local_model_present_in_discovery() {
    let state = state_with_registry(&[("tenant-a", "llama3.2:3b", true)]);
    let provider = TestProvider {
        models: vec![discovered_model("llama3.2:3b")],
    };

    let model = ProviderResolver::resolve_enabled_local_model(&state, &provider)
        .expect("التطابق الواحد يجب أن يحل");

    assert_eq!(model.as_str(), "llama3.2:3b");
}

#[test]
fn rejects_when_no_enabled_registry_model_matches_discovery() {
    let state = state_with_registry(&[("tenant-a", "llama3.2:3b", true)]);
    let provider = TestProvider {
        models: vec![discovered_model("qwen3:4b")],
    };

    assert!(matches!(
        ProviderResolver::resolve_enabled_local_model(&state, &provider),
        Err(AppError::ModelUnavailable)
    ));
}

#[test]
fn rejects_multiple_enabled_discovered_models_without_a_backend_selection() {
    let state = state_with_registry(&[
        ("tenant-a", "llama3.2:3b", true),
        ("tenant-a", "qwen3:4b", true),
    ]);
    let provider = TestProvider {
        models: vec![
            discovered_model("llama3.2:3b"),
            discovered_model("qwen3:4b"),
        ],
    };

    assert!(matches!(
        ProviderResolver::resolve_enabled_local_model(&state, &provider),
        Err(AppError::ModelUnavailable)
    ));
}

#[test]
fn ignores_registry_entries_from_another_tenant() {
    let state = state_with_registry(&[("tenant-b", "llama3.2:3b", true)]);
    let provider = TestProvider {
        models: vec![discovered_model("llama3.2:3b")],
    };

    assert!(matches!(
        ProviderResolver::resolve_enabled_local_model(&state, &provider),
        Err(AppError::ModelUnavailable)
    ));
}

#[test]
#[ignore = "يتطلب خدمة Ollama محلية مثبتة وقيد التشغيل"]
fn verifies_live_local_ollama_health_and_model_discovery_without_generation() {
    let service =
        crate::application::provider_runtime_service::ProviderRuntimeService::local_ollama();

    assert_eq!(service.health(), ProviderHealth::Available);
    service
        .discover_models()
        .expect("اكتشاف النماذج الحي يجب أن ينجح عند توافر المزود المحلي");
}
