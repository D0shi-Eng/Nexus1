use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        execution_policy::{
            execution_intent_reference_from_summary, ApprovalRequirement, ExecutionDecision,
            ExecutionEnvironment, ExecutionPolicyInput, ExecutionRequest, PolicyGate,
        },
        execution_policy_governor::ExecutionPolicyGovernor,
    },
    infrastructure::{
        agent_repository::AgentRepository, agent_run_repository::AgentRunRepository,
        database::AppState,
    },
};

pub struct ExecutionDispatcher;

impl ExecutionDispatcher {
    pub fn evaluate_readiness(
        state: &AppState,
        run_id: &str,
    ) -> Result<ExecutionDecision, AppError> {
        authorization_service::require_permission(state, "AGENT", "EXECUTE")?;
        validate_identifier(run_id)?;

        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        let run =
            AgentRunRepository::find_in_transaction(&transaction, &state.tenant_id(), run_id)?
                .ok_or(AppError::ResourceNotFound)?;
        let agent = AgentRepository::find(&transaction, &state.tenant_id(), &run.agent_id)?
            .ok_or(AppError::ResourceNotFound)?;
        let request = ExecutionRequest {
            run_id: run.id,
            agent_id: agent.id.clone(),
            authenticated_tenant_id: state.tenant_id(),
            authenticated_user_id: state.current_user_id()?,
            intent_reference: execution_intent_reference_from_summary(&run.request_summary)?,
        };
        let policy_input = ExecutionPolicyInput {
            request,
            platform_policy: PolicyGate::Allow,
            tenant_policy: PolicyGate::Allow,
            run_tenant_id: run.tenant_id,
            agent_tenant_id: agent.tenant_id,
            run_status: run.status,
            agent_status: agent.status,
            authorization_granted: true,
            approval_requirement: ApprovalRequirement::NotRequired,
            execution_environment: ExecutionEnvironment::ExecutorUnavailable,
        };

        Ok(ExecutionPolicyGovernor::evaluate(&policy_input))
    }
}

fn validate_identifier(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    use chrono::Utc;
    use rusqlite::{params, Connection};

    use super::ExecutionDispatcher;
    use crate::{
        domain::{
            error::AppError,
            execution_policy::{ExecutionDecisionOutcome, ExecutionDecisionReason},
        },
        infrastructure::database::AppState,
    };

    fn state_with_run(
        grant_execute: bool,
        session_state: &str,
        user_status: &str,
        agent_status: &str,
        run_status: &str,
    ) -> AppState {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        crate::infrastructure::database::apply_migrations(&connection)
            .expect("هجرة اختبار التنفيذ يجب أن تنجح");

        let now = Utc::now().to_rfc3339();
        let expires_at = if session_state == "EXPIRED" {
            "2020-01-01T00:00:00Z"
        } else {
            "2030-01-01T00:00:00Z"
        };
        let revoked_at = if session_state == "REVOKED" {
            Some(now.as_str())
        } else {
            None
        };
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant-a', 'Tenant A', 'DEMO_SYNTHETIC', ?1, ?1)",
                [&now],
            )
            .expect("المستأجر يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES ('user-a', 'tenant-a', 'User A', 'user-a@test.local', NULL, ?1, ?2, ?2)",
                params![user_status, &now],
            )
            .expect("المستخدم يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-a', 'tenant-a', 'EXECUTOR', 'Executor', ?1, ?1)",
                [&now],
            )
            .expect("الدور يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES ('permission-execute', 'agent:execute', 'AGENT', 'EXECUTE', 'HIGHLY_RESTRICTED', ?1)",
                [&now],
            )
            .expect("الصلاحية يجب أن تنشأ");
        connection
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user-a', 'role-a', ?1, 'test')",
                [&now],
            )
            .expect("تعيين الدور يجب أن ينجح");
        if grant_execute {
            connection
                .execute(
                    "INSERT INTO role_permissions (role_id, permission_id) VALUES ('role-a', 'permission-execute')",
                    [],
                )
                .expect("منح صلاحية الاختبار يجب أن ينجح");
        }
        connection
            .execute(
                "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session-a', 'tenant-a', 'user-a', ?1, ?2, ?3, 'test')",
                params![&now, expires_at, revoked_at],
            )
            .expect("الجسلة يجب أن تنشأ");
        connection
            .execute(
                "INSERT INTO agents (id, tenant_id, name, description, status, created_at, updated_at) VALUES ('agent-a', 'tenant-a', 'Agent A', 'Test agent', ?1, ?2, ?2)",
                params![agent_status, &now],
            )
            .expect("الوكيل يجب أن ينشأ");
        let (started_at, completed_at) = match run_status {
            "QUEUED" => (None, None),
            "WAITING_APPROVAL" => (None, None),
            _ => (Some(now.as_str()), Some(now.as_str())),
        };
        connection
            .execute(
                "INSERT INTO agent_runs (id, tenant_id, agent_id, requested_by_user_id, status, request_summary, revision, started_at, completed_at, created_at, updated_at) VALUES ('run-a', 'tenant-a', 'agent-a', 'user-a', ?1, 'GOVERNED_EXECUTION_REQUEST', 1, ?2, ?3, ?4, ?4)",
                params![run_status, started_at, completed_at, &now],
            )
            .expect("سجل التشغيل يجب أن ينشأ");

        AppState::test_instance(connection, "tenant-a".to_owned(), "user-a".to_owned())
    }

    #[test]
    fn evaluates_current_runtime_as_unavailable_without_mutating_the_run() {
        let state = state_with_run(true, "ACTIVE", "ACTIVE", "ACTIVE", "QUEUED");
        let (before, audit_count_before) = {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            let run_state = connection
                .query_row(
                    "SELECT status || ':' || updated_at FROM agent_runs WHERE id = 'run-a'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("السجل يجب أن يقرأ");
            let audit_count = connection
                .query_row("SELECT COUNT(*) FROM audit_events", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("سجل التدقيق يجب أن يقرأ");
            (run_state, audit_count)
        };

        let decision = ExecutionDispatcher::evaluate_readiness(&state, "run-a")
            .expect("التقييم المصرح يجب أن ينجح");

        let (after, audit_count_after) = {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            let run_state = connection
                .query_row(
                    "SELECT status || ':' || updated_at FROM agent_runs WHERE id = 'run-a'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("السجل يجب أن يقرأ");
            let audit_count = connection
                .query_row("SELECT COUNT(*) FROM audit_events", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("سجل التدقيق يجب أن يقرأ");
            (run_state, audit_count)
        };
        assert_eq!(decision.outcome, ExecutionDecisionOutcome::Deny);
        assert_eq!(
            decision.reason,
            ExecutionDecisionReason::ExecutionNotAvailable
        );
        assert_eq!(before, after);
        assert_eq!(audit_count_before, audit_count_after);
    }

    #[test]
    fn denies_without_agent_execute_permission() {
        let state = state_with_run(false, "ACTIVE", "ACTIVE", "ACTIVE", "QUEUED");

        let result = ExecutionDispatcher::evaluate_readiness(&state, "run-a");

        assert!(matches!(result, Err(AppError::AuthorizationDenied)));
    }

    #[test]
    fn denies_expired_session() {
        let state = state_with_run(true, "EXPIRED", "ACTIVE", "ACTIVE", "QUEUED");

        let result = ExecutionDispatcher::evaluate_readiness(&state, "run-a");

        assert!(matches!(result, Err(AppError::AuthorizationDenied)));
    }

    #[test]
    fn denies_revoked_session() {
        let state = state_with_run(true, "REVOKED", "ACTIVE", "ACTIVE", "QUEUED");

        let result = ExecutionDispatcher::evaluate_readiness(&state, "run-a");

        assert!(matches!(result, Err(AppError::AuthorizationDenied)));
    }

    #[test]
    fn denies_locked_user() {
        let state = state_with_run(true, "ACTIVE", "LOCKED", "ACTIVE", "QUEUED");

        let result = ExecutionDispatcher::evaluate_readiness(&state, "run-a");

        assert!(matches!(result, Err(AppError::AuthorizationDenied)));
    }

    #[test]
    fn rejects_disabled_agent() {
        let state = state_with_run(true, "ACTIVE", "ACTIVE", "DISABLED", "QUEUED");

        let decision =
            ExecutionDispatcher::evaluate_readiness(&state, "run-a").expect("القرار يجب أن يعاد");

        assert_eq!(decision.outcome, ExecutionDecisionOutcome::Deny);
        assert_eq!(decision.reason, ExecutionDecisionReason::AgentDisabled);
    }

    #[test]
    fn rejects_invalid_run_state() {
        let state = state_with_run(true, "ACTIVE", "ACTIVE", "ACTIVE", "WAITING_APPROVAL");

        let decision =
            ExecutionDispatcher::evaluate_readiness(&state, "run-a").expect("القرار يجب أن يعاد");

        assert_eq!(decision.outcome, ExecutionDecisionOutcome::Deny);
        assert_eq!(decision.reason, ExecutionDecisionReason::InvalidRunState);
    }
}
