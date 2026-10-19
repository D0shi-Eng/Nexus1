use chrono::Utc;

use crate::{
    application::{authorization_service, provider_runtime_service::ProviderRuntimeService},
    domain::{
        error::AppError,
        models::{IntegrationProviderState, IntegrationProviderStatus, IntegrationStatusData},
        provider::ProviderHealth,
    },
    infrastructure::{database::AppState, model_registry_repository::ModelRegistryRepository},
};

// الوحدة 14 — حالة تكاملات قراءة فقط للأسس المدعومة فعلياً:
// مزود Ollama المحلي (loopback وحده) وسجل نماذج الذكاء المحلي.
// لا موصلات سحابية ولا إضافة مفاتيح ولا تعديل إعدادات في هذه المرحلة.
pub fn load_integration_status(state: &AppState) -> Result<IntegrationStatusData, AppError> {
    authorization_service::require_permission(state, "INTEGRATION", "READ")?;

    let now = Utc::now().to_rfc3339();
    let provider_service = ProviderRuntimeService::local_ollama();
    let provider_id = provider_service.provider_id().as_str().to_owned();

    // «مدعوم» لا تعني «متصلاً»: الفحص فعلي، والحالة تنطق بما وجدناه فقط.
    let (provider_state, discovered_model_ids) = match provider_service.health() {
        ProviderHealth::Available => match provider_service.discover_models() {
            Ok(models) => (
                IntegrationProviderState::Connected,
                models
                    .into_iter()
                    .map(|model| model.id.as_str().to_owned())
                    .collect::<Vec<_>>(),
            ),
            // المزود حي لكن الاستكشاف فشل — لا ندّعي اتصالاً كاملاً ولا نجزم بعدمه.
            Err(_) => (IntegrationProviderState::Unknown, Vec::new()),
        },
        ProviderHealth::Unavailable => (IntegrationProviderState::Unavailable, Vec::new()),
        ProviderHealth::Unknown => (IntegrationProviderState::Unknown, Vec::new()),
    };

    let enabled_local_models = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        ModelRegistryRepository::list_enabled_local_models(&connection, &state.tenant_id())?
            .into_iter()
            .map(|model_id| model_id.as_str().to_owned())
            .collect::<Vec<_>>()
    };

    Ok(IntegrationStatusData {
        providers: vec![IntegrationProviderStatus {
            provider_id,
            provider_kind: "LOCAL".to_owned(),
            state: provider_state,
            discovered_model_ids,
            checked_at: now.clone(),
        }],
        enabled_local_models,
        // MP03: مسار التكوين المحكوم موجود الآن (اختيار من المكتشف حياً فقط
        // بصلاحية integration:update) — العلم يعبّر عن توافر المسار بصدق.
        model_registry_configuration_available: true,
        checked_at: now,
    })
}

#[cfg(test)]
mod tests {

    use chrono::Utc;
    use rusqlite::{params, Connection};

    use super::load_integration_status;
    use crate::{
        domain::{error::AppError, models::IntegrationProviderState},
        infrastructure::database::AppState,
    };

    fn test_state() -> AppState {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        for migration in [
            include_str!("../../migrations/0001_initial.sql"),
            include_str!("../../migrations/0002_identity_and_access.sql"),
            include_str!("../../migrations/0005_ai_governance.sql"),
        ] {
            connection
                .execute_batch(migration)
                .expect("هجرة الاختبار يجب أن تنجح");
        }

        let now = Utc::now().to_rfc3339();
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant-int', 'تكاملات', 'DEMO_SYNTHETIC', ?1, ?1)",
                params![&now],
            )
            .expect("المستأجر يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES ('user-int', 'tenant-int', 'قارئ تكاملات', 'int@test.local', NULL, 'ACTIVE', ?1, ?1)",
                params![&now],
            )
            .expect("المستخدم يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES ('perm-int-read', 'integration:read', 'INTEGRATION', 'READ', 'HIGHLY_RESTRICTED', ?1)",
                params![&now],
            )
            .expect("الصلاحية يجب أن تنشأ");
        connection
            .execute(
                "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-int', 'tenant-int', 'READER', 'قارئ', ?1, ?1)",
                params![&now],
            )
            .expect("الدور يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO role_permissions (role_id, permission_id) VALUES ('role-int', 'perm-int-read')",
                [],
            )
            .expect("المنح يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user-int', 'role-int', ?1, 'system')",
                params![&now],
            )
            .expect("الإسناد يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session-int', 'tenant-int', 'user-int', ?1, '2030-01-01T00:00:00Z', NULL, 'test')",
                params![&now],
            )
            .expect("الجلسة يجب أن تنشأ");

        AppState::test_instance(connection, "tenant-int".to_owned(), "user-int".to_owned())
    }

    #[test]
    fn reports_only_supported_local_foundations_truthfully() {
        let state = test_state();
        let status = load_integration_status(&state).expect("قراءة التكاملات المأذونة يجب أن تنجح");

        assert_eq!(status.providers.len(), 1);
        assert_eq!(status.providers[0].provider_id, "OLLAMA_LOCAL");
        assert_eq!(status.providers[0].provider_kind, "LOCAL");
        // الحالة تنطق بالفحص الفعلي لا بافتراض بيئة: اتصال إن كان المزود حياً ونجح الاستكشاف،
        // أو عدم توافر/جهل — وفي حالتي الفشل لا نماذج مزعومة.
        match status.providers[0].state {
            IntegrationProviderState::Connected => {}
            IntegrationProviderState::Unavailable | IntegrationProviderState::Unknown => {
                assert!(status.providers[0].discovered_model_ids.is_empty());
            }
        }
        assert!(status.enabled_local_models.is_empty());
        // مسار التكوين المحكوم متاح في MP03 — العلم صادق دائماً.
        assert!(status.model_registry_configuration_available);
    }

    #[test]
    fn denies_integration_read_without_permission() {
        let state = test_state();
        {
            let connection = state.connection.lock().expect("القفل يجب أن ينجح");
            connection
                .execute("DELETE FROM role_permissions", [])
                .expect("حذف المنح يجب أن ينجح");
        }
        assert!(matches!(
            load_integration_status(&state),
            Err(AppError::AuthorizationDenied)
        ));
    }

    #[test]
    fn exposes_enabled_registry_models_scoped_to_the_tenant() {
        let state = test_state();
        {
            let connection = state.connection.lock().expect("القفل يجب أن ينجح");
            let now = Utc::now().to_rfc3339();
            connection
                .execute(
                    "INSERT INTO ai_model_registry (id, tenant_id, provider_kind, model_id, display_name, enabled, data_residency, allowed_classification_ceiling, requires_human_approval, created_at, updated_at) VALUES ('model-1', 'tenant-int', 'LOCAL', 'qwen3:4b', 'Qwen3 4B', 1, 'LOCAL', 'INTERNAL', 1, ?1, ?1)",
                    params![&now],
                )
                .expect("النموذج الممكّن يجب أن ينشأ");
        }
        let status =
            load_integration_status(&state).expect("قراءة التكاملات بعد التمكين يجب أن تنجح");
        assert_eq!(status.enabled_local_models, vec!["qwen3:4b".to_owned()]);
    }
}
