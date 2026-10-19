use chrono::Utc;

use crate::{
    application::authorization_service,
    domain::{error::AppError, models::OrganizationalIntelligenceData},
    infrastructure::{database::AppState, organizational_intelligence_repository},
};

pub fn load_organizational_intelligence(
    state: &AppState,
) -> Result<OrganizationalIntelligenceData, AppError> {
    // وحدة الذكاء التنظيمي (13) تُمسّ بصلاحية قراءة خاصة بها — لا بصلاحية الذكاء التنظيمي (4).
    authorization_service::require_permission(state, "ORGANIZATION", "READ")?;

    let now = Utc::now().to_rfc3339();
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    organizational_intelligence_repository::load_organizational_intelligence(
        &connection,
        &state.tenant_id(),
        &now,
    )
}

#[cfg(test)]
mod tests {

    use chrono::Utc;
    use rusqlite::Connection;

    use super::load_organizational_intelligence;
    use crate::{
        domain::{
            error::{AppError, SafeError},
            models::{
                OrganizationalIntelligenceDataMode, OrganizationalIntelligencePriorityKind,
                OrganizationalIntelligencePriorityReason,
            },
        },
        infrastructure::database::AppState,
    };

    fn test_state() -> AppState {
        let connection = Connection::open_in_memory().expect("in-memory database should open");
        for migration in [
            include_str!("../../migrations/0001_initial.sql"),
            include_str!("../../migrations/0002_identity_and_access.sql"),
            include_str!("../../migrations/0003_workflow_and_risk.sql"),
            include_str!("../../migrations/0004_evidence_metadata.sql"),
            include_str!("../../migrations/0005_ai_governance.sql"),
            include_str!("../../migrations/0006_correct_demo_truth_labels.sql"),
            include_str!("../../migrations/0007_obligation_management.sql"),
            include_str!("../../migrations/0008_obligation_relationship_update_isolation.sql"),
            include_str!("../../migrations/0025_task_lifecycle.sql"),
            include_str!("../../migrations/0026_production_user_and_authoring.sql"),
        ] {
            connection
                .execute_batch(migration)
                .expect("test migration should apply");
        }

        let now = Utc::now().to_rfc3339();
        for (tenant_id, data_mode, source_verification, organization_name) in [
            (
                "tenant-intelligence-a",
                "DEMO_SYNTHETIC",
                "REQUIRES_REVIEW",
                "Organization A",
            ),
            (
                "tenant-intelligence-b",
                "PRODUCTION",
                "HUMAN_VERIFIED",
                "Organization B",
            ),
        ] {
            connection
                .execute(
                    "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?1, ?2, ?3, ?3)",
                    (tenant_id, data_mode, &now),
                )
                .expect("tenant should be inserted");
            connection
                .execute(
                    "INSERT INTO organizations (id, tenant_id, legal_name, country_code, industry, created_at, updated_at) VALUES (?1, ?2, ?3, 'SA', 'Technology', ?4, ?4)",
                    (format!("organization-{tenant_id}"), tenant_id, organization_name, &now),
                )
                .expect("organization should be inserted");
            connection
                .execute(
                    "INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES (?1, ?2, 'Synthetic Authority', 'Synthetic Source', NULL, '1.0', ?3, 0, ?4, ?3, ?3)",
                    (format!("source-{tenant_id}"), tenant_id, &now, source_verification),
                )
                .expect("source should be inserted");
            connection
                .execute(
                    "INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, source_type, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'Owner', '2020-01-01T00:00:00Z', 'CRITICAL', 'AT_RISK', 'APPLIES', 'MISSING', 'REQUIRES_REVIEW', 'SYNTHETIC', ?5, ?5)",
                    (
                        format!("obligation-{tenant_id}"),
                        tenant_id,
                        format!("source-{tenant_id}"),
                        format!("Obligation {tenant_id}"),
                        &now,
                    ),
                )
                .expect("obligation should be inserted");
            connection
                .execute(
                    "INSERT INTO risks (id, tenant_id, obligation_id, title, severity, likelihood, impact, status, owner, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'HIGH', 3, 4, 'OPEN', 'Risk owner', ?5, ?5)",
                    (
                        format!("risk-{tenant_id}"),
                        tenant_id,
                        format!("obligation-{tenant_id}"),
                        format!("Risk {tenant_id}"),
                        &now,
                    ),
                )
                .expect("risk should be inserted");
            connection
                .execute(
                    "INSERT INTO approvals (id, tenant_id, resource_type, resource_id, resource_version, requested_by, required_role_code, decision, decision_by, decision_reason, decided_at, created_at, updated_at) VALUES (?1, ?2, 'TASK', ?3, 1, 'system', 'READER', 'PENDING', NULL, NULL, NULL, ?4, ?4)",
                    (format!("approval-{tenant_id}"), tenant_id, format!("task-{tenant_id}"), &now),
                )
                .expect("approval should be inserted");
        }

        create_authorized_user(
            &connection,
            "tenant-intelligence-a",
            "user-intelligence-a",
            "role-intelligence-a",
            "session-intelligence-a",
            &now,
        );
        connection
            .execute(
                "INSERT INTO tasks (id, tenant_id, obligation_id, title, description, assignee_user_id, due_at, priority, status, created_at, updated_at) VALUES ('task-tenant-intelligence-a', 'tenant-intelligence-a', 'obligation-tenant-intelligence-a', 'Blocked task A', 'Test task', 'user-intelligence-a', '2020-01-01T00:00:00Z', 'HIGH', 'BLOCKED', ?1, ?1)",
                [&now],
            )
            .expect("tenant A task should be inserted");
        connection
            .execute(
                "INSERT INTO tasks (id, tenant_id, obligation_id, title, description, assignee_user_id, due_at, priority, status, created_at, updated_at) VALUES ('task-tenant-intelligence-b', 'tenant-intelligence-b', 'obligation-tenant-intelligence-b', 'Blocked task B', 'Test task', NULL, '2020-01-01T00:00:00Z', 'HIGH', 'BLOCKED', ?1, ?1)",
                [&now],
            )
            .expect("tenant B task should be inserted");

        AppState::test_instance(
            connection,
            "tenant-intelligence-a".to_owned(),
            "user-intelligence-a".to_owned(),
        )
    }

    fn create_authorized_user(
        connection: &Connection,
        tenant_id: &str,
        user_id: &str,
        role_id: &str,
        session_id: &str,
        now: &str,
    ) {
        connection
            .execute(
                "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, NULL, 'ACTIVE', ?5, ?5)",
                (user_id, tenant_id, user_id, format!("{user_id}@test.local"), now),
            )
            .expect("user should be inserted");
        connection
            .execute(
                "INSERT OR IGNORE INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES ('permission-organization-read', 'organization:read', 'ORGANIZATION', 'READ', 'HIGHLY_RESTRICTED', ?1)",
                [now],
            )
            .expect("permission should be inserted");
        connection
            .execute(
                "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES (?1, ?2, 'READER', 'Reader', ?3, ?3)",
                (role_id, tenant_id, now),
            )
            .expect("role should be inserted");
        connection
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, ?2, ?3, 'system')",
                (user_id, role_id, now),
            )
            .expect("role assignment should be inserted");
        connection
            .execute(
                "INSERT INTO role_permissions (role_id, permission_id) VALUES (?1, 'permission-organization-read')",
                [role_id],
            )
            .expect("role permission should be inserted");
        connection
            .execute(
                "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, ?2, ?3, ?4, '2030-01-01T00:00:00Z', NULL, 'test')",
                (session_id, tenant_id, user_id, now),
            )
            .expect("session should be inserted");
    }

    #[test]
    fn returns_authorized_tenant_data_with_derived_metrics() {
        let state = test_state();

        let data = load_organizational_intelligence(&state)
            .expect("authorized regulatory intelligence read should succeed");

        assert_eq!(
            data.organization
                .expect("organization should exist")
                .legal_name,
            "Organization A"
        );
        assert_eq!(
            data.data_mode,
            OrganizationalIntelligenceDataMode::DemoSynthetic
        );
        assert_eq!(data.sources.total, 1);
        assert_eq!(data.sources.requires_review, 1);
        assert_eq!(data.operations.active_obligations, 1);
        assert_eq!(data.operations.at_risk_obligations, 1);
        assert_eq!(data.operations.overdue_obligations, 1);
        assert_eq!(data.operations.evidence_gaps, 1);
        assert_eq!(data.operations.active_tasks, 1);
        assert_eq!(data.operations.blocked_tasks, 1);
        assert_eq!(data.operations.active_risks, 1);
        assert_eq!(data.operations.pending_approvals, 1);
        assert!(data.has_data);
        assert!(data
            .priorities
            .iter()
            .all(|priority| { priority.id.ends_with("tenant-intelligence-a") }));
        assert!(data.priorities.iter().any(|priority| {
            priority.kind == OrganizationalIntelligencePriorityKind::Obligation
                && priority.reason == OrganizationalIntelligencePriorityReason::ObligationAtRisk
        }));
    }

    #[test]
    fn denies_read_without_the_required_permission_and_maps_to_safe_error() {
        let state = test_state();
        state
            .connection
            .lock()
            .expect("connection should be available")
            .execute("DELETE FROM role_permissions", [])
            .expect("permission should be removed");

        let error = load_organizational_intelligence(&state)
            .expect_err("missing permission should deny the read");

        assert!(matches!(error, AppError::AuthorizationDenied));
        assert_eq!(SafeError::from(error).code, "AUTHORIZATION_DENIED");
    }

    #[test]
    fn denies_read_for_expired_and_revoked_sessions() {
        let state = test_state();
        state
            .connection
            .lock()
            .expect("connection should be available")
            .execute(
                "UPDATE local_sessions SET expires_at = '2020-01-01T00:00:00Z' WHERE id = 'session-intelligence-a'",
                [],
            )
            .expect("session should be expired");
        assert!(matches!(
            load_organizational_intelligence(&state),
            Err(AppError::AuthorizationDenied)
        ));

        state
            .connection
            .lock()
            .expect("connection should be available")
            .execute(
                "UPDATE local_sessions SET expires_at = '2030-01-01T00:00:00Z', revoked_at = '2026-01-01T00:00:00Z' WHERE id = 'session-intelligence-a'",
                [],
            )
            .expect("session should be revoked");
        assert!(matches!(
            load_organizational_intelligence(&state),
            Err(AppError::AuthorizationDenied)
        ));
    }

    #[test]
    fn isolates_derived_metrics_and_priorities_between_authorized_tenants() {
        let state = test_state();
        let data_a =
            load_organizational_intelligence(&state).expect("tenant A read should succeed");
        let now = Utc::now().to_rfc3339();
        {
            let connection = state
                .connection
                .lock()
                .expect("connection should be available");
            create_authorized_user(
                &connection,
                "tenant-intelligence-b",
                "user-intelligence-b",
                "role-intelligence-b",
                "session-intelligence-b",
                &now,
            );
        }
        state.set_active_tenant_for_tests("tenant-intelligence-b");
        state.set_active_user_for_tests("user-intelligence-b");

        let data_b =
            load_organizational_intelligence(&state).expect("tenant B read should succeed");

        assert_eq!(
            data_b
                .organization
                .expect("organization should exist")
                .legal_name,
            "Organization B"
        );
        assert_eq!(data_b.sources.total, 1);
        assert_eq!(data_b.sources.human_verified, 1);
        assert!(data_b
            .priorities
            .iter()
            .all(|priority| { priority.id.ends_with("tenant-intelligence-b") }));
        assert_ne!(data_a.priorities[0].id, data_b.priorities[0].id);
        assert_eq!(data_a.operations.active_tasks, 1);
        assert_eq!(data_b.operations.active_tasks, 1);
    }

    #[test]
    fn denies_a_user_when_the_state_is_switched_to_another_tenant() {
        let state = test_state();
        state.set_active_tenant_for_tests("tenant-intelligence-b");

        assert!(matches!(
            load_organizational_intelligence(&state),
            Err(AppError::AuthorizationDenied)
        ));
    }

    #[test]
    fn reports_an_empty_tenant_without_inventing_metrics_or_audit_events() {
        let state = test_state();
        let before_count: i64 = state
            .connection
            .lock()
            .expect("connection should be available")
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
            .expect("audit count should load");
        {
            let connection = state
                .connection
                .lock()
                .expect("connection should be available");
            connection
                .execute(
                    "DELETE FROM approvals WHERE tenant_id = 'tenant-intelligence-a'",
                    [],
                )
                .expect("approvals should delete");
            connection
                .execute(
                    "DELETE FROM risks WHERE tenant_id = 'tenant-intelligence-a'",
                    [],
                )
                .expect("risks should delete");
            connection
                .execute(
                    "DELETE FROM tasks WHERE tenant_id = 'tenant-intelligence-a'",
                    [],
                )
                .expect("tasks should delete");
            connection
                .execute(
                    "DELETE FROM obligations WHERE tenant_id = 'tenant-intelligence-a'",
                    [],
                )
                .expect("obligations should delete");
            connection
                .execute(
                    "DELETE FROM regulatory_sources WHERE tenant_id = 'tenant-intelligence-a'",
                    [],
                )
                .expect("sources should delete");
            connection
                .execute(
                    "DELETE FROM organizations WHERE tenant_id = 'tenant-intelligence-a'",
                    [],
                )
                .expect("organization should delete");
        }

        let data = load_organizational_intelligence(&state)
            .expect("authorized empty tenant read should succeed");
        let after_count: i64 = state
            .connection
            .lock()
            .expect("connection should be available")
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
            .expect("audit count should load");

        assert!(!data.has_data);
        assert!(data.organization.is_none());
        assert_eq!(data.sources.total, 0);
        assert_eq!(data.operations.active_obligations, 0);
        assert!(data.priorities.is_empty());
        assert_eq!(before_count, after_count);
    }

    #[test]
    fn rejects_an_invalid_persisted_data_mode() {
        let state = test_state();
        {
            let connection = state
                .connection
                .lock()
                .expect("connection should be available");
            connection
                .execute_batch("PRAGMA ignore_check_constraints = ON;")
                .expect("test database should allow invalid-state simulation");
            connection
                .execute(
                    "UPDATE tenants SET data_mode = 'UNKNOWN' WHERE id = 'tenant-intelligence-a'",
                    [],
                )
                .expect("invalid data mode should be stored for the test");
            connection
                .execute_batch("PRAGMA ignore_check_constraints = OFF;")
                .expect("test database should restore constraint enforcement");
        }

        assert!(matches!(
            load_organizational_intelligence(&state),
            Err(AppError::DataAccess)
        ));
    }

    #[test]
    fn returns_priorities_in_a_deterministic_order() {
        let state = test_state();

        let first = load_organizational_intelligence(&state).expect("first read should succeed");
        let second = load_organizational_intelligence(&state).expect("second read should succeed");

        let first_ids: Vec<&str> = first
            .priorities
            .iter()
            .map(|priority| priority.id.as_str())
            .collect();
        let second_ids: Vec<&str> = second
            .priorities
            .iter()
            .map(|priority| priority.id.as_str())
            .collect();
        assert_eq!(first_ids, second_ids);
    }
}
