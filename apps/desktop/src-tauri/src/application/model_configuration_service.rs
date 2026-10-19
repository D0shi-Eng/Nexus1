use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    application::{authorization_service, provider_runtime_service::ProviderRuntimeService},
    domain::{error::AppError, provider::ModelId, provider::ModelProvider},
    infrastructure::{
        database::{append_audit_event, latest_audit_event_hash, AppState},
        model_registry_repository::ModelRegistryRepository,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureLocalModelInput {
    pub model_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalModelConfigurationResult {
    pub configured_model_id: Option<String>,
}

// تكوين النموذج المحلي محكوم خلفياً (§23): القبول فقط لنموذج مكتشف حياً
// الآن من مثيل Ollama المحلي الموثوق — لا عنوان ولا بيانات اعتماد من العميل.
pub fn configure_local_model(
    state: &AppState,
    input: ConfigureLocalModelInput,
) -> Result<LocalModelConfigurationResult, AppError> {
    let provider = ProviderRuntimeService::local_ollama();
    configure_local_model_with_provider(state, input, &provider)
}

// الحقن مخصص للاختبارات — الإنتاج يكتشف من المثيل المحلي الموثوق حصراً.
pub(crate) fn configure_local_model_with_provider(
    state: &AppState,
    input: ConfigureLocalModelInput,
    provider: &dyn ModelProvider,
) -> Result<LocalModelConfigurationResult, AppError> {
    authorization_service::require_permission(state, "INTEGRATION", "UPDATE")?;
    let model_id = ModelId::new(&input.model_id).map_err(|_| AppError::ValidationFailed)?;

    let discovered = provider.discover_models().map_err(AppError::from)?;
    if !discovered.iter().any(|model| model.id == model_id) {
        // نموذج غير مكتشف من المصدر الموثوق ⇒ رفض (لا افتراض ولا تنزيل).
        return Err(AppError::ModelUnavailable);
    }

    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    ModelRegistryRepository::ensure_local_model_row(
        &transaction,
        &state.tenant_id(),
        model_id.as_str(),
        &now,
    )?;
    let changed = ModelRegistryRepository::enable_single_local_model(
        &transaction,
        &state.tenant_id(),
        model_id.as_str(),
        &now,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    append_model_config_audit(
        &transaction,
        state,
        "MODEL_CONFIGURATION_CHANGED",
        model_id.as_str(),
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(LocalModelConfigurationResult {
        configured_model_id: Some(model_id.as_str().to_owned()),
    })
}

// إخلاء التكوين: إلغاء تفعيل كل النماذج المحلية — التشغيل يفشل مغلقاً بعدها.
pub fn clear_local_model_configuration(
    state: &AppState,
) -> Result<LocalModelConfigurationResult, AppError> {
    authorization_service::require_permission(state, "INTEGRATION", "UPDATE")?;
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    ModelRegistryRepository::disable_all_local_models(&transaction, &state.tenant_id(), &now)?;
    append_model_config_audit(
        &transaction,
        state,
        "MODEL_CONFIGURATION_CLEARED",
        "LOCAL_OLLAMA",
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(LocalModelConfigurationResult {
        configured_model_id: None,
    })
}

fn append_model_config_audit(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    event_type: &str,
    model_id: &str,
    occurred_at: &str,
) -> Result<(), AppError> {
    let audit_id = format!("audit_{}", Uuid::new_v4());
    let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
    append_audit_event(
        transaction,
        &state.tenant_id(),
        &audit_id,
        event_type,
        &state.current_user_id()?,
        "ModelConfiguration",
        model_id,
        occurred_at,
        &previous_hash,
    )
    .map(|_| ())
}
