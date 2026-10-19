use chrono::Utc;

use crate::{
    application::authorization_service,
    domain::{error::AppError, models::RisksData},
    infrastructure::{database::AppState, risk_repository},
};

pub fn load_risks(state: &AppState) -> Result<RisksData, AppError> {
    authorization_service::require_permission(state, "RISK", "READ")?;

    let generated_at = Utc::now().to_rfc3339();
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    risk_repository::load_risks(&connection, &state.tenant_id(), &generated_at)
}

#[cfg(test)]
mod tests {

    use chrono::Utc;
    use rusqlite::Connection;

    use super::load_risks;
    use crate::{domain::error::AppError, infrastructure::database::AppState};

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
        for tenant_id in ["tenant-risk-a", "tenant-risk-b", "tenant-risk-empty"] {
            connection
                .execute(
                    "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?1, 'DEMO_SYNTHETIC', ?2, ?2)",
                    (tenant_id, &now),
                )
                .expect("tenant should be inserted");
        }

        for tenant_id in ["tenant-risk-a", "tenant-risk-b"] {
            let source_id = format!("source-{tenant_id}");
            let obligation_id = format!("obligation-{tenant_id}");
            connection
                .execute(
                    "INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES (?1, ?2, 'Synthetic Authority', 'Synthetic Source', NULL, '1.0', ?3, 0, 'REQUIRES_REVIEW', ?3, ?3)",
                    (&source_id, tenant_id, &now),
                )
                .expect("source should be inserted");
            connection
                .execute(
                    "INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, source_type, created_at, updated_at) VALUES (?1, ?2, ?3, 'Synthetic obligation', 'Owner', NULL, 'HIGH', 'OPEN', 'APPLIES', 'MISSING', 'REQUIRES_REVIEW', 'SYNTHETIC', ?4, ?4)",
                    (&obligation_id, tenant_id, &source_id, &now),
                )
                .expect("obligation should be inserted");
        }

        insert_risk(
            &connection,
            "risk-a-critical",
            "tenant-risk-a",
            "obligation-tenant-risk-a",
            "Critical A",
            "CRITICAL",
            "OPEN",
            "2026-08-20T00:00:00Z",
        );
        insert_risk(
            &connection,
            "risk-a-high",
            "tenant-risk-a",
            "obligation-tenant-risk-a",
            "High A",
            "HIGH",
            "MITIGATING",
            "2026-08-19T00:00:00Z",
        );
        insert_risk(
            &connection,
            "risk-a-closed",
            "tenant-risk-a",
            "obligation-tenant-risk-a",
            "Closed A",
            "LOW",
            "CLOSED",
            "2026-08-18T00:00:00Z",
        );
        insert_risk(
            &connection,
            "risk-b-only",
            "tenant-risk-b",
            "obligation-tenant-risk-b",
            "Risk B",
            "HIGH",
            "ACCEPTED",
            "2026-08-20T00:00:00Z",
        );

        create_authorized_user(
            &connection,
            "tenant-risk-a",
            "user-risk-a",
            "role-risk-a",
            "session-risk-a",
            &now,
        );

        AppState::test_instance(
            connection,
            "tenant-risk-a".to_owned(),
            "user-risk-a".to_owned(),
        )
    }

    fn insert_risk(
        connection: &Connection,
        id: &str,
        tenant_id: &str,
        obligation_id: &str,
        title: &str,
        severity: &str,
        status: &str,
        updated_at: &str,
    ) {
        connection
            .execute(
                "INSERT INTO risks (id, tenant_id, obligation_id, title, severity, likelihood, impact, status, owner, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, 3, 4, ?6, 'Risk owner', ?7, ?7)",
                (id, tenant_id, obligation_id, title, severity, status, updated_at),
            )
            .expect("risk should be inserted");
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
                "INSERT OR IGNORE INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES ('permission-risk-read', 'risk:read', 'RISK', 'READ', 'HIGHLY_RESTRICTED', ?1)",
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
                "INSERT INTO role_permissions (role_id, permission_id) VALUES (?1, 'permission-risk-read')",
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
    fn returns_only_authorized_tenant_risks_and_derived_summary() {
        let state = test_state();

        let data = load_risks(&state).expect("authorized risk read should succeed");

        assert!(data.has_data);
        assert_eq!(data.summary.total, 3);
        assert_eq!(data.summary.open, 1);
        assert_eq!(data.summary.mitigating, 1);
        assert_eq!(data.summary.accepted, 0);
        assert_eq!(data.summary.closed, 1);
        assert_eq!(
            data.items
                .iter()
                .map(|risk| risk.id.as_str())
                .collect::<Vec<_>>(),
            vec!["risk-a-critical", "risk-a-high", "risk-a-closed"]
        );
        assert!(data.items.iter().all(|risk| !risk.id.contains("risk-b")));
    }

    #[test]
    fn denies_read_without_risk_read_permission() {
        let state = test_state();
        state
            .connection
            .lock()
            .expect("connection should be available")
            .execute("DELETE FROM role_permissions", [])
            .expect("permission should be removed");

        assert!(matches!(
            load_risks(&state),
            Err(AppError::AuthorizationDenied)
        ));
    }

    #[test]
    fn denies_read_for_expired_revoked_and_locked_identity() {
        let state = test_state();
        {
            let connection = state
                .connection
                .lock()
                .expect("connection should be available");
            connection
                .execute("UPDATE local_sessions SET expires_at = '2020-01-01T00:00:00Z' WHERE id = 'session-risk-a'", [])
                .expect("session should expire");
        }
        assert!(matches!(
            load_risks(&state),
            Err(AppError::AuthorizationDenied)
        ));

        {
            let connection = state
                .connection
                .lock()
                .expect("connection should be available");
            connection
                .execute("UPDATE local_sessions SET expires_at = '2030-01-01T00:00:00Z', revoked_at = '2026-01-01T00:00:00Z' WHERE id = 'session-risk-a'", [])
                .expect("session should be revoked");
        }
        assert!(matches!(
            load_risks(&state),
            Err(AppError::AuthorizationDenied)
        ));

        {
            let connection = state
                .connection
                .lock()
                .expect("connection should be available");
            connection
                .execute(
                    "UPDATE local_sessions SET revoked_at = NULL WHERE id = 'session-risk-a'",
                    [],
                )
                .expect("session should be restored for the locked-user assertion");
            connection
                .execute(
                    "UPDATE local_users SET status = 'LOCKED' WHERE id = 'user-risk-a'",
                    [],
                )
                .expect("user should become locked");
        }
        assert!(matches!(
            load_risks(&state),
            Err(AppError::AuthorizationDenied)
        ));
    }

    #[test]
    fn isolates_risk_list_and_summary_between_authorized_tenants() {
        let state = test_state();
        let now = Utc::now().to_rfc3339();
        {
            let connection = state
                .connection
                .lock()
                .expect("connection should be available");
            create_authorized_user(
                &connection,
                "tenant-risk-b",
                "user-risk-b",
                "role-risk-b",
                "session-risk-b",
                &now,
            );
        }

        state.set_active_tenant_for_tests("tenant-risk-b");
        state.set_active_user_for_tests("user-risk-b");
        let data = load_risks(&state).expect("tenant B should be authorized");

        assert_eq!(data.summary.total, 1);
        assert_eq!(data.summary.accepted, 1);
        assert_eq!(data.items.len(), 1);
        assert_eq!(data.items[0].id, "risk-b-only");
    }

    #[test]
    fn denies_user_when_authenticated_state_is_bound_to_another_tenant() {
        let state = test_state();
        state.set_active_tenant_for_tests("tenant-risk-b");

        assert!(matches!(
            load_risks(&state),
            Err(AppError::AuthorizationDenied)
        ));
    }

    #[test]
    fn returns_empty_state_without_creating_audit_events() {
        let state = test_state();
        let now = Utc::now().to_rfc3339();
        {
            let connection = state
                .connection
                .lock()
                .expect("connection should be available");
            create_authorized_user(
                &connection,
                "tenant-risk-empty",
                "user-risk-empty",
                "role-risk-empty",
                "session-risk-empty",
                &now,
            );
        }
        state.set_active_tenant_for_tests("tenant-risk-empty");
        state.set_active_user_for_tests("user-risk-empty");
        let before_count: i64 = state
            .connection
            .lock()
            .expect("connection should be available")
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
            .expect("audit count should load");

        let data = load_risks(&state).expect("empty tenant should load");
        let after_count: i64 = state
            .connection
            .lock()
            .expect("connection should be available")
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
            .expect("audit count should load");

        assert!(!data.has_data);
        assert_eq!(data.summary.total, 0);
        assert!(data.items.is_empty());
        assert_eq!(before_count, after_count);
    }

    #[test]
    fn rejects_invalid_persisted_risk_status() {
        let state = test_state();
        {
            let connection = state
                .connection
                .lock()
                .expect("connection should be available");
            connection
                .execute_batch("PRAGMA ignore_check_constraints = ON;")
                .expect("constraints should be temporarily disabled");
            connection
                .execute(
                    "UPDATE risks SET status = 'UNKNOWN' WHERE id = 'risk-a-critical'",
                    [],
                )
                .expect("invalid status should be stored for the test");
            connection
                .execute_batch("PRAGMA ignore_check_constraints = OFF;")
                .expect("constraints should be restored");
        }

        assert!(matches!(load_risks(&state), Err(AppError::DataAccess)));
    }

    #[test]
    fn returns_risks_in_a_deterministic_order_and_is_read_only() {
        let state = test_state();
        let before_count: i64 = state
            .connection
            .lock()
            .expect("connection should be available")
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
            .expect("audit count should load");

        let first = load_risks(&state).expect("first risk read should succeed");
        let second = load_risks(&state).expect("second risk read should succeed");
        let after_count: i64 = state
            .connection
            .lock()
            .expect("connection should be available")
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
            .expect("audit count should load");

        assert_eq!(
            first
                .items
                .iter()
                .map(|risk| risk.id.as_str())
                .collect::<Vec<_>>(),
            second
                .items
                .iter()
                .map(|risk| risk.id.as_str())
                .collect::<Vec<_>>(),
        );
        assert_eq!(before_count, after_count);
    }
}
