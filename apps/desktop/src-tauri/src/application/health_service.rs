use chrono::Utc;

use crate::{
    application::{ai_governance_service, audit_service, provider_runtime_service},
    domain::{
        error::AppError,
        models::{AiProviderCheck, HealthState, HealthStatus, RecentFailureCounts},
    },
    infrastructure::database::AppState,
};

// صحة بيئة العمل: كلها من قاعدة البيانات المحلية — بلا اتصال شبكة في هذا المسار.
pub fn get_health(state: &AppState) -> Result<HealthStatus, AppError> {
    let (schema_version, configured_model_id, failures) = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        connection
            .query_row("SELECT 1", [], |row| row.get::<_, i32>(0))
            .map_err(|_| AppError::DataAccess)?;

        let schema_version: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .map_err(|_| AppError::DataAccess)?;
        let tenant_id = state.tenant_id();
        let configured_model_id: Option<String> = connection
            .query_row(
                "SELECT model_id FROM ai_model_registry WHERE tenant_id = ?1 AND enabled = 1 ORDER BY updated_at DESC LIMIT 1",
                [&tenant_id],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|error| match error {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
            .map_err(|_| AppError::DataAccess)?;

        let cutoff = (Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
        let agent_runs_failed_24h: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM agent_runs WHERE tenant_id = ?1 AND status = 'FAILED' AND updated_at > ?2",
                [&tenant_id, &cutoff],
                |row| row.get(0),
            )
            .map_err(|_| AppError::DataAccess)?;
        let provider_errors_24h: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM agent_execution_results WHERE tenant_id = ?1 AND failure_code IS NOT NULL AND created_at > ?2",
                [&tenant_id, &cutoff],
                |row| row.get(0),
            )
            .map_err(|_| AppError::DataAccess)?;

        (
            schema_version,
            configured_model_id,
            RecentFailureCounts {
                agent_runs_failed_24h,
                provider_errors_24h,
            },
        )
    };

    let audit_chain_is_valid = audit_service::verify_audit_chain(state).unwrap_or(false);
    let has_enabled_ai_model = ai_governance_service::has_enabled_model(state).unwrap_or(false);

    Ok(HealthStatus {
        application: if audit_chain_is_valid {
            HealthState::Healthy
        } else {
            HealthState::Degraded
        },
        database: HealthState::Healthy,
        // نموذج مُمكن = مُعد وصحيح؛ "غير مُعد" هو الحالة الوحيدة الأخرى هنا —
        // فحص المزود الحي فعل صريح منفصل (check_ai_provider) لا تشويه للوسم.
        ai: if has_enabled_ai_model {
            HealthState::Healthy
        } else {
            HealthState::NotConfigured
        },
        checked_at: Utc::now().to_rfc3339(),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        schema_version,
        runtime_mode: state.runtime_mode()?.as_str().to_owned(),
        configured_model_id,
        recent_failures: failures,
    })
}

// فحص Ollama الحي الصريح — مهلة قصيرة مضمونة في طبقة المزود (2 ثانية).
pub fn check_ai_provider(state: &AppState) -> Result<AiProviderCheck, AppError> {
    crate::application::authorization_service::require_permission(state, "INTEGRATION", "READ")?;
    let runtime = provider_runtime_service::ProviderRuntimeService::local_ollama();
    Ok(AiProviderCheck {
        reachable: matches!(
            runtime.health(),
            crate::domain::provider::ProviderHealth::Available
        ),
        checked_at: Utc::now().to_rfc3339(),
    })
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{check_ai_provider, get_health};
    use crate::domain::models::RuntimeMode;
    use crate::infrastructure::database::{
        initialize_demo_database, AppState, DEMO_OPERATOR_USER_ID,
    };

    fn demo_state() -> AppState {
        let connection = Connection::open_in_memory().expect("قاعدة الاختبار يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("مفاتيح الأجنبة يجب أن تعمل");
        initialize_demo_database(&connection).expect("التهيئة يجب أن تنجح");
        AppState::test_instance(
            connection,
            crate::infrastructure::database::DEMO_TENANT_ID,
            DEMO_OPERATOR_USER_ID,
        )
    }

    #[test]
    fn health_reports_environment_identity_and_local_metrics_only() {
        let state = demo_state();
        let health = get_health(&state).expect("الصحة يجب أن تُقرأ");

        assert_eq!(health.app_version, env!("CARGO_PKG_VERSION"));
        assert_eq!(health.schema_version, 26);
        assert_eq!(health.runtime_mode, RuntimeMode::Demo.as_str());
        assert_eq!(health.configured_model_id, None);
        assert_eq!(health.recent_failures.agent_runs_failed_24h, 0);
        assert_eq!(health.recent_failures.provider_errors_24h, 0);
        // لا نموذج مُمكن في القاعدة الطازجة — الوسم صادق لا "منخفض".
        assert_eq!(health.ai, crate::domain::models::HealthState::NotConfigured);
    }

    #[test]
    fn provider_check_requires_authorization_and_fails_closed_without_ollama() {
        let state = demo_state();
        // المشغّل يمتلك قراءة التكاملات — الفحص يمر ثم يُبلّغ عن حالة حقيقية (لا انهيار).
        let outcome = check_ai_provider(&state).expect("الفحص يجب أن يمر لمن يملك قراءة التكاملات");
        // الجواب مطابق لحالة المزود الفعلية في هذه اللحظة: بلا خادم محلي ⇒ غير متاح.
        let runtime =
            crate::application::provider_runtime_service::ProviderRuntimeService::local_ollama();
        assert_eq!(
            outcome.reachable,
            matches!(
                runtime.health(),
                crate::domain::provider::ProviderHealth::Available
            ),
            "reachable يجب أن يعكس حالة المزود الفعلية لا قيمة ثابتة"
        );
        assert!(!outcome.checked_at.is_empty());

        // بلا إذن: رفض تفويض صريح.
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute("DELETE FROM role_permissions", [])
            .expect("نزع المنح يجب أن ينجح");
        drop(connection);
        assert!(check_ai_provider(&state).is_err());
    }
}
