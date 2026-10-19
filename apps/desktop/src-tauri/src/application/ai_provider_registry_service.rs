use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use uuid::Uuid;

use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        network_policy::{NetworkPolicy, SystemResolver},
        provider::{
            ModelId, ModelProvider, ProviderExecutionIdentity, ProviderFailure, ProviderId,
        },
        provider_registry::{
            AiProviderKind, AiTransportKind, DiscoveredCapabilities, ProviderInstanceRecord,
        },
    },
    infrastructure::{
        ai_provider_repository::{AiModelRepository, AiProviderRepository},
        credential_vault::{CredentialVault, VaultError},
        database::{append_audit_event, latest_audit_event_hash, AppState},
        providers::{
            anthropic::{AnthropicAdapter, AnthropicConfig},
            ollama::{OllamaProviderAdapter, OllamaProviderConfig},
            openai_compatible::{OpenAiCompatibleAdapter, OpenAiCompatibleConfig},
            presets,
        },
    },
};

// =====================================================================
// إعادة تصدير أنواع العقد المجمّد — سطح واحد للأوامر
// ---------------------------------------------------------------------
// طبقة العرض تستورد من الخدمة وحدها، فتظل أنواع العقد في مكان واحد
// (domain/provider_registry.rs) ولا تتكرر ولا تنحرف عن
// packages/contracts/src/index.ts.
// =====================================================================
pub use crate::domain::provider_registry::{
    AiModelDiscoveryResult, AiModelRecord, AiModelsData, AiProviderConnectionState,
    AiProviderCredentialResult, AiProviderIdInput, AiProviderMutationResult, AiProviderRecord,
    AiProviderTestResult, AiProvidersData, ConfigureAiProviderInput, SelectAiModelInput,
    SetAiProviderCredentialInput,
};

// =====================================================================
// خدمة سجل الموفرين وسجل النماذج (0023)
// ---------------------------------------------------------------------
// الحدود الملزمة في هذا الملف:
//   1) كل أمر: تفويض ← تحقق ← معاملة واحدة ← التحقق من المستأجر لكل معرّف ←
//      حدث تدقيق بسلسلة هاش ← التزام.
//   2) لا يُحمل قفل قاعدة البيانات أبداً خلال نداء شبكة: نقرأ الإعداد، نُسقط
//      القفل، ننفّذ النداء، ثم نفتح معاملة قصيرة لكتابة النتيجة.
//   3) لا يُعاد سر إلى الواجهة ولا يُدرج في التدقيق ولا في أي سجل. الأسرار
//      تمر إلى مخزن نظام التشغيل وتُقرأ لحظة بناء الترويسة فقط.
//   4) UNVERIFIED ليست نجاحاً: لا يصبح أي موفر CONNECTED بغير اختبار ناجح.
// =====================================================================

/// نتيجة اختبار اتصال أو استكشاف: حالة + عدد + رمز خطأ آمن (بلا تفاصيل شبكية).
struct ProbeOutcome {
    state: AiProviderConnectionState,
    models: Vec<DiscoveredModelView>,
    safe_error_code: Option<String>,
}

/// عرض نموذج مكتشف قبل تحويله إلى عقد الواجهة.
struct DiscoveredModelView {
    model_id: String,
    display_name: String,
    capabilities: DiscoveredCapabilities,
}

/// تحويل فشل الموفر إلى حالة اتصال + رمز آمن. لا يخرج أي تفصيل شبكي أو سر.
fn classify_failure(failure: ProviderFailure) -> (AiProviderConnectionState, String) {
    match failure {
        ProviderFailure::Timeout => (
            AiProviderConnectionState::Unavailable,
            "PROVIDER_TIMEOUT".to_owned(),
        ),
        ProviderFailure::EndpointRejected(code) => {
            (AiProviderConnectionState::Unavailable, code.to_owned())
        }
        ProviderFailure::CredentialMissing => (
            AiProviderConnectionState::Unavailable,
            "PROVIDER_UNAVAILABLE".to_owned(),
        ),
        ProviderFailure::ModelUnavailable => (
            AiProviderConnectionState::Disconnected,
            "MODEL_UNAVAILABLE".to_owned(),
        ),
        ProviderFailure::Unavailable => (
            AiProviderConnectionState::Unavailable,
            "PROVIDER_UNAVAILABLE".to_owned(),
        ),
        ProviderFailure::InvalidResponse
        | ProviderFailure::OutputTooLarge
        | ProviderFailure::ToolCallUnsupported => (
            AiProviderConnectionState::Disconnected,
            "PROVIDER_RESPONSE_INVALID".to_owned(),
        ),
        ProviderFailure::InputTooLarge => (
            AiProviderConnectionState::Disconnected,
            "VALIDATION_FAILED".to_owned(),
        ),
    }
}

fn vault_error(error: VaultError) -> AppError {
    match error {
        VaultError::InvalidInput => AppError::ValidationFailed,
        VaultError::Corrupted => AppError::ProviderCredentialMissing,
        VaultError::StorageUnavailable => AppError::ProviderCredentialStorageUnavailable,
        VaultError::OsProtectionFailed => AppError::ProviderCredentialStorageUnavailable,
    }
}

/// التحقق من الوجهة عبر السياسة ثم اشتقاق نوع النقل المُخزَّن.
fn evaluate_transport(base_url: &str) -> Result<(AiTransportKind, String), AppError> {
    let resolver = SystemResolver;
    let destination = NetworkPolicy::new(&resolver)
        .evaluate(base_url)
        .map_err(|rejection| AppError::ProviderEndpointRejected(rejection.code()))?;
    let transport = if destination.is_secure() {
        AiTransportKind::Https
    } else {
        AiTransportKind::HttpLoopback
    };

    // العنوان المُخزَّن هو الأصل المطبَّع المتحقق منه — لا النص الخام من العميل.
    Ok((transport, destination.origin()))
}

fn validate_provider_id(value: &str) -> Result<String, AppError> {
    if !crate::infrastructure::credential_vault::is_valid_provider_secret_id(value) {
        return Err(AppError::ValidationFailed);
    }

    Ok(value.to_owned())
}

fn validate_display_name(value: &str) -> Result<String, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > 120 {
        return Err(AppError::ValidationFailed);
    }

    Ok(trimmed.to_owned())
}

// وضع بيانات المستأجر يُقرأ من صف المستأجر نفسه — لا من وضع التشغيل.
// كان هذا الموضع يشتقّ الوسم من `RuntimeMode` ويسقط كل ما ليس إنتاجاً إلى
// «DEMO_SYNTHETIC»، فيُوسم مستأجر غير مهيأ بأنه ديمو، ويخالف `attention_service`
// الذي يقرأ العمود نفسه. مصدر واحد للحقيقة: عمود tenants.data_mode.
//
// يُقرأ على الاتصال القائم **لا** بقفل جديد: قفل `AppState.connection` من نوع
// std وغير معاد الدخول، و`list_models` يحتفظ بالحارس حتى نهاية الدالة فنداء
// يقفل ثانيةً جمود مؤكد.
fn read_data_mode(connection: &rusqlite::Connection, tenant_id: &str) -> Result<String, AppError> {
    connection
        .query_row(
            "SELECT data_mode FROM tenants WHERE id = ?1",
            [tenant_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| AppError::ResourceNotFound)
}

// =====================================================================
// المصنع: صف موفر ← محوّل ModelProvider
// ---------------------------------------------------------------------
// لا يقرأ السر إلا لحظة بناء المحوّل، ويحتفظ به داخل الإعداد المؤقت للمحوّل
// في الذاكرة. لا يُسجَّل ولا يُعاد ولا يُقارن.
// =====================================================================

/// بناء محوّل موفر من صفه. يُرجع فشلاً مغلقاً إن غاب سر مطلوب أو رُفض العنوان.
///
/// `tenant_id` إلزامي: مفتاح المخزن الآمن صار (مستأجر، موفر) — راجع
/// `CredentialVault`. بلا تمريره يقرأ مستأجر سرّ مستأجر آخر عند تطابق المعرّف.
pub fn build_adapter(
    tenant_id: &str,
    record: &ProviderInstanceRecord,
    vault: &Arc<dyn CredentialVault>,
) -> Result<Box<dyn ModelProvider>, AppError> {
    let provider_id =
        ProviderId::new(record.provider_id.clone()).map_err(|_| AppError::ProviderNotConfigured)?;

    // الموفر المحلي المدمج: نستعمل واجهة Ollama الأصلية على الحلقي حصراً،
    // فلا يتحول عنوان المستخدم إلى وسيلة لبلوغ شبكة داخلية.
    if record.kind == AiProviderKind::Ollama && record.built_in {
        return Ok(Box::new(OllamaProviderAdapter::new(
            OllamaProviderConfig::local_default(),
        )));
    }

    let credential = if record.kind.requires_credential() {
        match vault
            .load(tenant_id, &record.provider_id)
            .map_err(vault_error)?
        {
            Some(credential) => Some(credential),
            None => return Err(AppError::ProviderCredentialMissing),
        }
    } else {
        None
    };

    // إعادة التحقق من الوجهة عند كل بناء محوّل: الإعداد المخزَّن قد يكون قديماً
    // أو كُتب قبل تشديد السياسة — لا نثق بالصف المخزَّن وحده.
    let destination = {
        let resolver = SystemResolver;
        NetworkPolicy::new(&resolver)
            .evaluate(&record.base_url)
            .map_err(|rejection| AppError::ProviderEndpointRejected(rejection.code()))?
    };

    match record.kind {
        AiProviderKind::Anthropic => {
            let preset = presets::preset_for(record.kind).ok_or(AppError::ProviderNotConfigured)?;
            let config = AnthropicConfig::new(destination, credential);
            let _ = preset;
            Ok(Box::new(AnthropicAdapter::new(provider_id, config)))
        }
        _ => {
            let preset = presets::preset_for(record.kind).ok_or(AppError::ProviderNotConfigured)?;
            let config = OpenAiCompatibleConfig::new(
                destination,
                preset.models_path,
                preset.chat_path,
                credential,
            );
            Ok(Box::new(OpenAiCompatibleAdapter::new(provider_id, config)))
        }
    }
}

/// هوية **وجهة** التنفيذ للاختيار الحالي — بلا بناء محوّل، وبلا شبكة، وبلا لمس
/// أي مادة سرية.
///
/// تُستعمل في **طرفَي** ربط الموافقة: لحظة الطلب (تُخزَّن) ولحظة التنفيذ
/// (تُقارن). ولأن الطرفين يُشتقّان من نفس المصدر الوحيد — الاختيار في
/// `ai_model_registry` + صف الموفر في `ai_provider_registry` — فأي تبديل للموفر
/// بينهما يُنتج بصمة مختلفة ويُرفض التنفيذ.
///
/// عند غياب أي اختيار صريح يبقى المسار المحلي التاريخي كما هو: Ollama على
/// الحلقي، وهو نفس ما يعيده `resolve_active_provider` في هذا الفرع.
pub fn resolve_active_provider_identity(
    state: &AppState,
) -> Result<ProviderExecutionIdentity, AppError> {
    let selected_record = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        let selected = AiModelRepository::selected_pair(&connection, &state.tenant_id())?;
        match selected {
            Some((provider_row_id, _model_id)) => {
                AiProviderRepository::list_for_tenant(&connection, &state.tenant_id())?
                    .into_iter()
                    .find(|record| record.id == provider_row_id)
            }
            None => None,
        }
    };

    let identity = match selected_record {
        Some(record) => ProviderExecutionIdentity::new(
            record.provider_id,
            record.kind.as_database_code(),
            record.base_url,
            record.transport.as_database_code(),
        ),
        None => {
            // نفس عنوان الإعداد المدمج، مأخوذاً من المصدر الواحد لا مُكرَّراً هنا.
            let preset = presets::preset_for(AiProviderKind::Ollama)
                .ok_or(AppError::ProviderNotConfigured)?;
            ProviderExecutionIdentity::new(
                ProviderId::ollama_local().as_str(),
                AiProviderKind::Ollama.as_database_code(),
                preset.base_url,
                AiTransportKind::HttpLoopback.as_database_code(),
            )
        }
    };

    identity.map_err(|_| AppError::ProviderNotConfigured)
}

/// الموفر المختار فعلياً للتنفيذ: (المحوّل، معرّف الموفر).
/// يحافظ على السلوك المحلي القائم حين لا يوجد أي انتخابات صريحة.
pub fn resolve_active_provider(
    state: &AppState,
) -> Result<(Box<dyn ModelProvider>, ProviderId), AppError> {
    let resolved = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        let selected = AiModelRepository::selected_pair(&connection, &state.tenant_id())?;
        match selected {
            Some((provider_row_id, _model_id)) => {
                let record =
                    AiProviderRepository::list_for_tenant(&connection, &state.tenant_id())?
                        .into_iter()
                        .find(|record| record.id == provider_row_id);
                record
            }
            None => None,
        }
    };

    let vault = state.credential_vault()?;
    match resolved {
        Some(record) => {
            if !record.enabled {
                return Err(AppError::ProviderDisabled);
            }
            let adapter = build_adapter(&state.tenant_id(), &record, &vault)?;
            let provider_id = adapter.provider_id();
            Ok((adapter, provider_id))
        }
        None => {
            // المسار التاريخي المحلي يبقى كما هو: مزود Ollama على الحلقي.
            let adapter = OllamaProviderAdapter::new(OllamaProviderConfig::local_default());
            let provider_id = adapter.provider_id();
            Ok((Box::new(adapter), provider_id))
        }
    }
}

// =====================================================================
// القراءة: get_ai_providers
// =====================================================================

pub fn list_providers(state: &AppState) -> Result<AiProvidersData, AppError> {
    authorization_service::require_permission(state, "INTEGRATION", "READ")?;
    let tenant_id = state.tenant_id();
    let vault = state.credential_vault()?;
    let generated_at = Utc::now().to_rfc3339();

    let (records, selected, data_mode) = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        let records = AiProviderRepository::list_for_tenant(&connection, &tenant_id)?;
        let selected = AiModelRepository::selected_pair(&connection, &tenant_id)?;
        let data_mode = read_data_mode(&connection, &tenant_id)?;
        (records, selected, data_mode)
    };

    let mut providers = Vec::with_capacity(records.len());
    let mut selected_provider_id = None;
    let mut selected_model_id = None;
    for record in &records {
        let model_count = {
            let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
            AiModelRepository::count_for_provider(&connection, &tenant_id, &record.id)?
        };
        // علامة السر تُقرأ من المخزن لا من الصف: الصف قد يكون قديماً.
        let credential_configured = vault.exists(&tenant_id, &record.provider_id);
        if let Some((row_id, model_id)) = &selected {
            if row_id == &record.id {
                selected_provider_id = Some(record.provider_id.clone());
                selected_model_id = Some(model_id.clone());
            }
        }
        providers.push(AiProviderRecord {
            provider_id: record.provider_id.clone(),
            kind: record.kind,
            display_name: record.display_name.clone(),
            base_url: record.base_url.clone(),
            enabled: record.enabled,
            connection_state: record.connection_state,
            connection_checked_at: record.connection_checked_at.clone(),
            credential_configured,
            requires_credential: record.kind.requires_credential(),
            supports_discovery: record.kind.supports_discovery(),
            model_count,
            built_in: record.built_in,
        });
    }

    Ok(AiProvidersData {
        providers,
        selected_provider_id,
        selected_model_id,
        data_mode,
        generated_at,
    })
}

// =====================================================================
// الطفرات
// =====================================================================

pub fn configure_provider(
    state: &AppState,
    input: ConfigureAiProviderInput,
) -> Result<AiProviderMutationResult, AppError> {
    authorization_service::require_permission(state, "INTEGRATION", "UPDATE")?;
    let provider_id = validate_provider_id(&input.provider_id)?;
    let display_name = validate_display_name(&input.display_name)?;
    // العنوان إلزامي وغير فارغ؛ الأنواع ذات الإعداد غير المتحقق تطلب من
    // المستخدم إدخاله بنفسه (لا اختراع افتراضي).
    let (transport, normalised_base_url) = evaluate_transport(&input.base_url)?;
    let now = Utc::now().to_rfc3339();
    let tenant_id = state.tenant_id();
    let actor = state.current_user_id()?;
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    let record = AiProviderRepository::upsert(
        &transaction,
        &tenant_id,
        &provider_id,
        input.kind,
        &display_name,
        &normalised_base_url,
        transport,
        input.enabled,
        &now,
    )?;
    let credential_configured = record.credential_configured;
    append_provider_audit(
        &transaction,
        &tenant_id,
        &actor,
        "AI_PROVIDER_CONFIGURED",
        &provider_id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(AiProviderMutationResult {
        provider_id: record.provider_id,
        connection_state: record.connection_state,
        credential_configured,
    })
}

pub fn remove_provider(
    state: &AppState,
    provider_id: &str,
) -> Result<AiProviderMutationResult, AppError> {
    authorization_service::require_permission(state, "INTEGRATION", "UPDATE")?;
    let provider_id = validate_provider_id(provider_id)?;
    let now = Utc::now().to_rfc3339();
    let tenant_id = state.tenant_id();
    let actor = state.current_user_id()?;
    let vault = state.credential_vault()?;

    let deleted = {
        let connection = state
            .connection
            .lock()
            .map_err(|_| AppError::StorageOperation)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        // التحقق من المستأجر: الصف يُقرأ داخل المعاملة بمعرّف المستأجر النشط.
        let existing =
            AiProviderRepository::find_in_transaction(&transaction, &tenant_id, &provider_id)?;
        let Some(existing) = existing else {
            return Err(AppError::ProviderNotConfigured);
        };
        if existing.built_in {
            // الموفر المدمج جزء من الأساس القابل للتشغيل — لا يُحذف.
            return Err(AppError::Conflict);
        }
        let deleted = AiProviderRepository::delete(&transaction, &tenant_id, &provider_id)?;
        if deleted == 0 {
            return Err(AppError::ResourceNotFound);
        }
        append_provider_audit(
            &transaction,
            &tenant_id,
            &actor,
            "AI_PROVIDER_REMOVED",
            &provider_id,
            &now,
        )?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
        deleted
    };
    let _ = deleted;

    // حذف السر بعد نجاح حذف الصف: لو فشل حذف الصف لا نفقد السر بلا سبب.
    // الفشل هنا لا يُسقط العملية لأن الصف اختفى فعلاً والسر صار يتيماً.
    let _ = vault.delete(&tenant_id, &provider_id);

    Ok(AiProviderMutationResult {
        provider_id,
        connection_state: AiProviderConnectionState::Unverified,
        credential_configured: false,
    })
}

pub fn set_credential(
    state: &AppState,
    input: SetAiProviderCredentialInput,
) -> Result<AiProviderCredentialResult, AppError> {
    // صلاحية منفصلة صريحة: قراءة الموفرين لا تمنح كتابة الأسرار.
    authorization_service::require_permission(state, "INTEGRATION", "CREDENTIAL")?;
    let provider_id = validate_provider_id(&input.provider_id)?;
    if input.credential.is_empty() || input.credential.len() > 8 * 1024 {
        return Err(AppError::ValidationFailed);
    }
    let now = Utc::now().to_rfc3339();
    let tenant_id = state.tenant_id();
    let actor = state.current_user_id()?;
    let vault = state.credential_vault()?;

    // 1) التحقق من وجود الموفر داخل المستأجر قبل لمس المخزن الآمن.
    {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        if AiProviderRepository::find_by_provider_id(&connection, &tenant_id, &provider_id)?
            .is_none()
        {
            return Err(AppError::ProviderNotConfigured);
        }
    }

    // 2) الكتابة في مخزن نظام التشغيل — خارج أي معاملة قاعدة بيانات.
    //    السر يمر هنا مرة واحدة ولا يُنسخ إلى أي سجل أو حدث تدقيق.
    vault
        .store(&tenant_id, &provider_id, &input.credential)
        .map_err(vault_error)?;

    // 3) العلامة في القاعدة + حدث تدقيق **بلا أي مادة سرية**.
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let changed = AiProviderRepository::set_credential_marker(
        &transaction,
        &tenant_id,
        &provider_id,
        true,
        // حفظ مفتاح لا يعني اتصالاً: الحالة تبقى/تعود UNVERIFIED.
        AiProviderConnectionState::Unverified,
        None,
        &now,
    )?;
    if changed == 0 {
        return Err(AppError::ProviderNotConfigured);
    }
    append_provider_audit(
        &transaction,
        &tenant_id,
        &actor,
        "AI_PROVIDER_CREDENTIAL_SET",
        &provider_id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(AiProviderCredentialResult {
        provider_id,
        credential_configured: true,
    })
}

pub fn clear_credential(
    state: &AppState,
    provider_id: &str,
) -> Result<AiProviderCredentialResult, AppError> {
    authorization_service::require_permission(state, "INTEGRATION", "CREDENTIAL")?;
    let provider_id = validate_provider_id(provider_id)?;
    let now = Utc::now().to_rfc3339();
    let tenant_id = state.tenant_id();
    let actor = state.current_user_id()?;
    let vault = state.credential_vault()?;

    {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        if AiProviderRepository::find_by_provider_id(&connection, &tenant_id, &provider_id)?
            .is_none()
        {
            return Err(AppError::ProviderNotConfigured);
        }
    }

    vault
        .delete(&tenant_id, &provider_id)
        .map_err(vault_error)?;

    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    // حذف السر يُخرج الموفر من حالة الاتصال: لا ادعاء اتصال بلا مفتاح.
    // الحالة تعود UNVERIFIED في كل الأحوال (لم يُختبر بعد إزالة المفتاح).
    let changed = AiProviderRepository::set_credential_marker(
        &transaction,
        &tenant_id,
        &provider_id,
        false,
        AiProviderConnectionState::Unverified,
        None,
        &now,
    )?;
    if changed == 0 {
        return Err(AppError::ProviderNotConfigured);
    }
    append_provider_audit(
        &transaction,
        &tenant_id,
        &actor,
        "AI_PROVIDER_CREDENTIAL_CLEARED",
        &provider_id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(AiProviderCredentialResult {
        provider_id,
        credential_configured: false,
    })
}

// =====================================================================
// الاختبار والاكتشاف
// =====================================================================

/// اختبار الاتصال: نداء حقيقي واحد (اكتشاف النماذج) خارج أي قفل قاعدة بيانات.
/// لا يمس الإعداد ولا السر عند الفشل — الفشل يُسجَّل كحالة ولا يُتلف شيئاً.
pub fn test_provider(
    state: &AppState,
    provider_id: &str,
) -> Result<AiProviderTestResult, AppError> {
    authorization_service::require_permission(state, "INTEGRATION", "UPDATE")?;
    let provider_id = validate_provider_id(provider_id)?;
    let tenant_id = state.tenant_id();
    let actor = state.current_user_id()?;
    let vault = state.credential_vault()?;

    let record = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        AiProviderRepository::find_by_provider_id(&connection, &tenant_id, &provider_id)?
            .ok_or(AppError::ProviderNotConfigured)?
    };

    let started = Instant::now();
    let probe = probe_provider(&tenant_id, &record, &vault);
    let latency_ms = started.elapsed().as_millis().min(i64::MAX as u128) as i64;
    let checked_at = Utc::now().to_rfc3339();

    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    AiProviderRepository::update_connection_state(
        &transaction,
        &tenant_id,
        &provider_id,
        probe.state,
        Some(&checked_at),
        &checked_at,
    )?;
    // نماذج الاختبار الناجح تُخزَّن بنفس مسار الاكتشاف — نتيجة واحدة بلا ازدواج.
    if probe.safe_error_code.is_none() {
        persist_discovered_models(
            &transaction,
            &tenant_id,
            &record,
            &probe.models,
            &checked_at,
        )?;
    }
    append_provider_audit(
        &transaction,
        &tenant_id,
        &actor,
        "AI_PROVIDER_TESTED",
        &provider_id,
        &checked_at,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(AiProviderTestResult {
        provider_id,
        connection_state: probe.state,
        checked_at,
        latency_ms,
        discovered_model_count: probe.models.len() as i64,
        safe_error_code: probe.safe_error_code,
    })
}

pub fn discover_models(
    state: &AppState,
    provider_id: &str,
) -> Result<AiModelDiscoveryResult, AppError> {
    authorization_service::require_permission(state, "INTEGRATION", "UPDATE")?;
    let provider_id = validate_provider_id(provider_id)?;
    let tenant_id = state.tenant_id();
    let actor = state.current_user_id()?;
    let vault = state.credential_vault()?;

    let record = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        AiProviderRepository::find_by_provider_id(&connection, &tenant_id, &provider_id)?
            .ok_or(AppError::ProviderNotConfigured)?
    };

    let probe = probe_provider(&tenant_id, &record, &vault);
    let discovered_at = Utc::now().to_rfc3339();

    // العيبة المُثبتة (كانت CRITICAL — جمود كامل للخلف):
    // كان الحارس `connection` يُربط في نطاق الدالة نفسها (لا في كتلة)، فيبقى
    // مملوكاً حتى آخر سطر، ثم يُستدعى `list_models_for_provider` ⇒ `list_models`
    // الذي يطلب `state.connection.lock()` مرة ثانية على نفس الخيط. و`Mutex` في
    // المكتبة القياسية **غير قابل لإعادة الدخول**، فيجمُد الخيط للأبد حاملاً
    // القفل: كل أمر IPC لاحق يلمس القاعدة يتجمّد معه. أي أن زر «استكشاف» في
    // لوحة الموفرين كان يُجمّد التطبيق نهائياً. الإصلاح: حصر الحارس في كتلة كما
    // في `select_model`، فلا يبقى قفل مفتوح عبر أي نداء لاحق.
    {
        let connection = state
            .connection
            .lock()
            .map_err(|_| AppError::StorageOperation)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        if probe.safe_error_code.is_none() {
            persist_discovered_models(
                &transaction,
                &tenant_id,
                &record,
                &probe.models,
                &discovered_at,
            )?;
        }
        append_provider_audit(
            &transaction,
            &tenant_id,
            &actor,
            "AI_PROVIDER_MODELS_DISCOVERED",
            &provider_id,
            &discovered_at,
        )?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
    }

    let models = list_models_for_provider(state, &record)?;

    Ok(AiModelDiscoveryResult {
        provider_id,
        models,
        discovered_at,
        safe_error_code: probe.safe_error_code,
    })
}

/// نداء شبكة حقيقي بلا أي قفل: بناء المحوّل ثم تعداد النماذج.
fn probe_provider(
    tenant_id: &str,
    record: &ProviderInstanceRecord,
    vault: &Arc<dyn CredentialVault>,
) -> ProbeOutcome {
    let adapter = match build_adapter(tenant_id, record, vault) {
        Ok(adapter) => adapter,
        Err(AppError::ProviderEndpointRejected(code)) => {
            return ProbeOutcome {
                state: AiProviderConnectionState::Unavailable,
                models: Vec::new(),
                safe_error_code: Some(code.to_owned()),
            }
        }
        Err(AppError::ProviderCredentialMissing) => {
            return ProbeOutcome {
                state: AiProviderConnectionState::Unavailable,
                models: Vec::new(),
                safe_error_code: Some("PROVIDER_UNAVAILABLE".to_owned()),
            }
        }
        Err(_) => {
            return ProbeOutcome {
                state: AiProviderConnectionState::Unavailable,
                models: Vec::new(),
                safe_error_code: Some("EXECUTION_INFRASTRUCTURE_ERROR".to_owned()),
            }
        }
    };

    match adapter.discover_models() {
        Ok(models) => ProbeOutcome {
            // نجاح تعداد النماذج هو اختبار اتصال ناجح فعلي.
            state: AiProviderConnectionState::Connected,
            models: models
                .into_iter()
                .map(|model| DiscoveredModelView {
                    model_id: model.id.as_str().to_owned(),
                    display_name: model.display_name,
                    capabilities: DiscoveredCapabilities {
                        // واجهتا Ollama وAnthropic تعطيان حجم النافذة أحياناً،
                        // لكن العقد الحالي لا يمرّره من المحوّل؛ لا ندّعي رقماً.
                        context_limit: None,
                        // القدرة تُعلن فقط إن أثبتها الموفر؛ محوّل Ollama
                        // يكشفها من capabilities، ومحوّلا OpenAI/Anthropic
                        // لا يعلنانها في مسار التعداد ⇒ None لا false.
                        supports_tools: if record.kind == AiProviderKind::Ollama {
                            Some(model.supports_tools)
                        } else {
                            None
                        },
                        supports_structured_output: None,
                        supports_vision: None,
                    },
                })
                .collect(),
            safe_error_code: None,
        },
        Err(failure) => {
            let (state, code) = classify_failure(failure);
            ProbeOutcome {
                state,
                models: Vec::new(),
                safe_error_code: Some(code),
            }
        }
    }
}

/// كتابة النماذج المكتشفة: إسقاط التوفر أولاً ثم الإدراج/التحديث.
fn persist_discovered_models(
    transaction: &rusqlite::Transaction<'_>,
    tenant_id: &str,
    record: &ProviderInstanceRecord,
    models: &[DiscoveredModelView],
    now: &str,
) -> Result<(), AppError> {
    AiModelRepository::mark_provider_models_unavailable(transaction, tenant_id, &record.id, now)?;
    for model in models {
        AiModelRepository::upsert_discovered_model(
            transaction,
            tenant_id,
            &record.id,
            record.kind,
            record.transport,
            &model.model_id,
            &model.display_name,
            &model.capabilities,
            now,
        )?;
    }

    Ok(())
}

// =====================================================================
// قراءة النماذج والانتخاب
// =====================================================================

pub fn list_models(state: &AppState) -> Result<AiModelsData, AppError> {
    authorization_service::require_permission(state, "INTEGRATION", "READ")?;
    let tenant_id = state.tenant_id();
    let generated_at = Utc::now().to_rfc3339();

    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let providers = AiProviderRepository::list_for_tenant(&connection, &tenant_id)?;
    let selected = AiModelRepository::selected_pair(&connection, &tenant_id)?;
    let models = AiModelRepository::list_for_tenant(&connection, &tenant_id)?;
    let data_mode = read_data_mode(&connection, &tenant_id)?;

    let mut selected_provider_id = None;
    let mut selected_model_id = None;
    if let Some((row_id, model_id)) = &selected {
        selected_model_id = Some(model_id.clone());
        selected_provider_id = providers
            .iter()
            .find(|provider| &provider.id == row_id)
            .map(|provider| provider.provider_id.clone());
    }

    let records = models
        .into_iter()
        .filter_map(|model| {
            let row_id = model.provider_id.clone()?;
            let provider = providers.iter().find(|provider| provider.id == row_id)?;
            Some(AiModelRecord {
                model_id: model.model_id.clone(),
                provider_id: provider.provider_id.clone(),
                provider_kind: provider.kind,
                provider_display_name: provider.display_name.clone(),
                display_name: model.display_name.clone(),
                available: model.available,
                selected: model.enabled,
                context_limit: model.capabilities.context_limit,
                supports_tools: model.capabilities.supports_tools,
                supports_structured_output: model.capabilities.supports_structured_output,
                supports_vision: model.capabilities.supports_vision,
                capability_source: model.capability_source,
                last_discovered_at: model.last_discovered_at.clone(),
            })
        })
        .collect();

    Ok(AiModelsData {
        models: records,
        selected_provider_id,
        selected_model_id,
        data_mode,
        generated_at,
    })
}

fn list_models_for_provider(
    state: &AppState,
    record: &ProviderInstanceRecord,
) -> Result<Vec<AiModelRecord>, AppError> {
    let data = list_models(state)?;

    Ok(data
        .models
        .into_iter()
        .filter(|model| model.provider_id == record.provider_id)
        .collect())
}

pub fn select_model(state: &AppState, input: SelectAiModelInput) -> Result<AiModelsData, AppError> {
    authorization_service::require_permission(state, "INTEGRATION", "UPDATE")?;
    let provider_id = validate_provider_id(&input.provider_id)?;
    let model_id = ModelId::new(&input.model_id)
        .map_err(|_| AppError::ValidationFailed)?
        .as_str()
        .to_owned();
    // شرط إضافي: الموفر يتطلب سراً ⇒ لا انتخاب بلا سر محفوظ (فشل مغلق مبكر).
    let tenant_id = state.tenant_id();
    let actor = state.current_user_id()?;
    let now = Utc::now().to_rfc3339();
    let vault = state.credential_vault()?;

    {
        let connection = state
            .connection
            .lock()
            .map_err(|_| AppError::StorageOperation)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        // التحقق من المستأجر لكل معرّف مُشار إليه.
        let provider =
            AiProviderRepository::find_in_transaction(&transaction, &tenant_id, &provider_id)?
                .ok_or(AppError::ProviderNotConfigured)?;
        if !provider.enabled {
            return Err(AppError::ProviderDisabled);
        }
        if provider.kind.requires_credential() && !vault.exists(&tenant_id, &provider_id) {
            return Err(AppError::ProviderCredentialMissing);
        }
        let model = AiModelRepository::find_in_transaction(
            &transaction,
            &tenant_id,
            &provider.id,
            &model_id,
        )?
        .ok_or(AppError::ProviderModelSelectionInvalid)?;
        if !model.available {
            return Err(AppError::ProviderModelSelectionInvalid);
        }
        let selected = AiModelRepository::select_single(
            &transaction,
            &tenant_id,
            &provider.id,
            &model_id,
            &now,
        )?;
        if !selected {
            return Err(AppError::ProviderModelSelectionInvalid);
        }
        append_provider_audit(
            &transaction,
            &tenant_id,
            &actor,
            "AI_MODEL_SELECTED",
            &format!("{provider_id}:{model_id}"),
            &now,
        )?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
    }

    list_models(state)
}

pub fn clear_model_selection(state: &AppState) -> Result<AiModelsData, AppError> {
    authorization_service::require_permission(state, "INTEGRATION", "UPDATE")?;
    let tenant_id = state.tenant_id();
    let actor = state.current_user_id()?;
    let now = Utc::now().to_rfc3339();
    {
        let connection = state
            .connection
            .lock()
            .map_err(|_| AppError::StorageOperation)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        AiModelRepository::clear_selection(&transaction, &tenant_id, &now)?;
        append_provider_audit(
            &transaction,
            &tenant_id,
            &actor,
            "AI_MODEL_SELECTION_CLEARED",
            "AI_MODEL_SELECTION",
            &now,
        )?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
    }

    list_models(state)
}

// =====================================================================
// التدقيق — لا يحمل أبداً أي مادة سرية
// =====================================================================

fn append_provider_audit(
    transaction: &rusqlite::Transaction<'_>,
    tenant_id: &str,
    actor: &str,
    event_type: &str,
    resource_id: &str,
    occurred_at: &str,
) -> Result<(), AppError> {
    let audit_id = format!("audit_{}", Uuid::new_v4());
    let previous_hash = latest_audit_event_hash(transaction, tenant_id)?;
    append_audit_event(
        transaction,
        tenant_id,
        &audit_id,
        event_type,
        actor,
        "AiProvider",
        resource_id,
        occurred_at,
        &previous_hash,
    )
    .map(|_| ())
}

#[cfg(test)]
#[path = "ai_provider_registry_service_tests.rs"]
mod tests;
