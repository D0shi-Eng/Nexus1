use chrono::Utc;
use rusqlite::{params, Connection};

use super::{finalize_execution, start_execution, GovernedAgentExecutorService};
use crate::{
    application::{
        agent_run_service::{cancel_agent_run, CancelAgentRunInput},
        provider_agent_executor::ProviderAgentExecutor,
    },
    domain::{
        error::AppError,
        execution_input::{ExecutionInput, ExecutionInstruction},
        execution_result::{ExecutionFailureCode, ExecutionResult, ExecutionResultOutcome},
        models::AgentRunStatus,
        provider::{
            DiscoveredModel, ModelId, ModelProvider, ProviderFailure, ProviderGeneration,
            ProviderHealth, ProviderId, RuntimeExecutionInput,
        },
    },
    infrastructure::database::AppState,
};

#[derive(Clone, Copy)]
enum TestGenerationBehavior {
    Success,
    Failure(ProviderFailure),
}

struct TestProvider {
    models: Vec<DiscoveredModel>,
    behavior: TestGenerationBehavior,
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
        model_id: &ModelId,
        input: &RuntimeExecutionInput,
    ) -> Result<ProviderGeneration, ProviderFailure> {
        match self.behavior {
            TestGenerationBehavior::Success => Ok(ProviderGeneration {
                model_id: model_id.clone(),
                output: format!("result:{}", input.as_str()),
                generated_at: "2026-08-24T00:00:00Z".to_owned(),
            }),
            TestGenerationBehavior::Failure(failure) => Err(failure),
        }
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

fn state_with_run() -> AppState {
    let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
    crate::infrastructure::database::apply_migrations(&connection)
        .expect("هجرة اختبار المنفذ يجب أن تنجح");

    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "INSERT INTO tenants (id, name, data_mode, created_at, updated_at)
             VALUES ('tenant-a', 'Tenant A', 'DEMO_SYNTHETIC', ?1, ?1)",
            [&now],
        )
        .expect("المستأجر يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at)
             VALUES ('user-a', 'tenant-a', 'User A', 'user-a@nexus.test', NULL, 'ACTIVE', ?1, ?1)",
            [&now],
        )
        .expect("المستخدم يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at)
             VALUES ('role-a', 'tenant-a', 'EXECUTOR', 'Executor', ?1, ?1)",
            [&now],
        )
        .expect("الدور يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at)
             VALUES ('permission-execute', 'agent:execute', 'AGENT', 'EXECUTE', 'HIGHLY_RESTRICTED', ?1)",
            [&now],
        )
        .expect("الصلاحية يجب أن تنشأ");
    connection
        .execute(
            "INSERT INTO role_permissions (role_id, permission_id) VALUES ('role-a', 'permission-execute')",
            [],
        )
        .expect("منح صلاحية الاختبار يجب أن ينجح");
    connection
        .execute(
            "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by)
             VALUES ('user-a', 'role-a', ?1, 'test')",
            [&now],
        )
        .expect("تعيين الدور يجب أن ينجح");
    connection
        .execute(
            "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label)
             VALUES ('session-a', 'tenant-a', 'user-a', ?1, '2030-01-01T00:00:00Z', NULL, 'test')",
            [&now],
        )
        .expect("الجلسة يجب أن تنشأ");
    connection
        .execute(
            "INSERT INTO agents (id, tenant_id, name, description, status, created_at, updated_at)
             VALUES ('agent-a', 'tenant-a', 'Agent A', 'Agent test', 'ACTIVE', ?1, ?1)",
            [&now],
        )
        .expect("الوكيل يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO agent_runs (
                id, tenant_id, agent_id, requested_by_user_id, status, request_summary, revision,
                started_at, completed_at, created_at, updated_at
             ) VALUES (
                'run-a', 'tenant-a', 'agent-a', 'user-a', 'QUEUED', 'GOVERNED_EXECUTION_REQUEST', 1,
                NULL, NULL, ?1, ?1
             )",
            [&now],
        )
        .expect("طلب التشغيل يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO ai_model_registry (
                id, tenant_id, provider_kind, model_id, display_name, enabled,
                data_residency, allowed_classification_ceiling, requires_human_approval,
                created_at, updated_at
             ) VALUES (
                'model-a', 'tenant-a', 'LOCAL', 'qwen3:4b', 'qwen3:4b', 1,
                'LOCAL', 'PUBLIC', 1, ?1, ?1
             )",
            [&now],
        )
        .expect("صف النموذج يجب أن ينشأ");

    AppState::test_instance(connection, "tenant-a".to_owned(), "user-a".to_owned())
}

fn input() -> ExecutionInput {
    ExecutionInput::new(
        ExecutionInstruction::new("trusted execution instruction")
            .expect("تعليمة الاختبار يجب أن تكون صالحة"),
    )
}

#[test]
fn completes_with_result_provenance_and_terminal_run_state() {
    let state = state_with_run();
    let provider = TestProvider {
        models: vec![discovered_model("qwen3:4b")],
        behavior: TestGenerationBehavior::Success,
    };
    let executor = ProviderAgentExecutor::new(&state, &provider);

    let result = GovernedAgentExecutorService::execute_trusted(&state, "run-a", input(), &executor)
        .expect("التنفيذ الحتمي يجب أن ينجح");

    assert_eq!(result.result.outcome(), ExecutionResultOutcome::Completed);
    assert_eq!(
        result.result.output(),
        Some("result:trusted execution instruction")
    );
    assert_eq!(result.result.provider_id().as_str(), "OLLAMA_LOCAL");
    let connection = state.connection.lock().expect("القفل يجب أن ينجح");
    let (status, revision, started, completed): (String, i64, Option<String>, Option<String>) = connection
        .query_row(
            "SELECT status, revision, started_at, completed_at FROM agent_runs WHERE id = 'run-a'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("الحالة النهائية يجب أن تقرأ");
    let audit_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events
             WHERE resource_id = 'run-a'
               AND event_type IN ('AGENT_RUN_EXECUTION_STARTED', 'AGENT_RUN_EXECUTION_COMPLETED')",
            [],
            |row| row.get(0),
        )
        .expect("أحداث التدقيق يجب أن تقرأ");

    assert_eq!(status, "COMPLETED");
    assert_eq!(revision, 3);
    assert!(started.is_some());
    assert!(completed.is_some());
    assert_eq!(audit_count, 2);
}

#[test]
fn maps_provider_timeout_to_a_failed_result_and_terminal_run() {
    let state = state_with_run();
    let provider = TestProvider {
        models: vec![discovered_model("qwen3:4b")],
        behavior: TestGenerationBehavior::Failure(ProviderFailure::Timeout),
    };
    let executor = ProviderAgentExecutor::new(&state, &provider);

    let result = GovernedAgentExecutorService::execute_trusted(&state, "run-a", input(), &executor)
        .expect("فشل المزود يجب أن يسجل كنتيجة");

    assert_eq!(result.result.outcome(), ExecutionResultOutcome::Failed);
    assert_eq!(
        result.result.failure_code(),
        Some(ExecutionFailureCode::ProviderTimeout)
    );
    let connection = state.connection.lock().expect("القفل يجب أن ينجح");
    let status: String = connection
        .query_row(
            "SELECT status FROM agent_runs WHERE id = 'run-a'",
            [],
            |row| row.get(0),
        )
        .expect("الحالة يجب أن تقرأ");
    assert_eq!(status, "FAILED");
}

#[test]
fn fails_closed_when_no_single_authorized_model_matches_discovery() {
    let state = state_with_run();
    let provider = TestProvider {
        models: vec![discovered_model("other:1b")],
        behavior: TestGenerationBehavior::Success,
    };
    let executor = ProviderAgentExecutor::new(&state, &provider);

    let result = GovernedAgentExecutorService::execute_trusted(&state, "run-a", input(), &executor)
        .expect("غياب النموذج يجب أن يسجل كنتيجة فشل");

    assert_eq!(result.result.outcome(), ExecutionResultOutcome::Failed);
    assert_eq!(
        result.result.failure_code(),
        Some(ExecutionFailureCode::ModelUnavailable)
    );
}

#[test]
fn cancellation_wins_against_late_result_finalization() {
    let state = state_with_run();
    let running = start_execution(&state, "run-a").expect("بدء التنفيذ يجب أن ينجح");
    let cancelled = cancel_agent_run(
        &state,
        CancelAgentRunInput {
            run_id: "run-a".to_owned(),
        },
    )
    .expect("إلغاء التشغيل الجاري يجب أن ينجح");
    let result = ExecutionResult::completed(
        ProviderId::ollama_local(),
        ProviderGeneration {
            model_id: ModelId::new("qwen3:4b").expect("معرف النموذج يجب أن يكون صالحاً"),
            output: "late output".to_owned(),
            generated_at: "2026-08-24T00:00:00Z".to_owned(),
        },
    )
    .expect("نتيجة الاختبار يجب أن تكون صالحة");

    let finalized = finalize_execution(&state, &running, result);

    assert_eq!(cancelled.run.status, AgentRunStatus::Cancelled);
    assert!(matches!(finalized, Err(AppError::Conflict)));
    let connection = state.connection.lock().expect("القفل يجب أن ينجح");
    let result_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM agent_execution_results", [], |row| {
            row.get(0)
        })
        .expect("عدد النتائج يجب أن يقرأ");
    assert_eq!(result_count, 0);
}

#[test]
fn terminal_result_write_rolls_back_when_its_audit_event_fails() {
    let state = state_with_run();
    let running = start_execution(&state, "run-a").expect("بدء التنفيذ يجب أن ينجح");
    let connection = state.connection.lock().expect("القفل يجب أن ينجح");
    connection
        .execute_batch(
            "CREATE TRIGGER fail_execution_completion_audit
             BEFORE INSERT ON audit_events
             WHEN NEW.event_type = 'AGENT_RUN_EXECUTION_COMPLETED'
             BEGIN
               SELECT RAISE(ABORT, 'رفض اختبار التدقيق');
             END;",
        )
        .expect("مشغل فشل التدقيق يجب أن ينشأ");
    drop(connection);
    let result = ExecutionResult::completed(
        ProviderId::ollama_local(),
        ProviderGeneration {
            model_id: ModelId::new("qwen3:4b").expect("معرف النموذج يجب أن يكون صالحاً"),
            output: "completed output".to_owned(),
            generated_at: "2026-08-24T00:00:00Z".to_owned(),
        },
    )
    .expect("نتيجة الاختبار يجب أن تكون صالحة");

    let finalized = finalize_execution(&state, &running, result);

    assert!(matches!(finalized, Err(AppError::StorageOperation)));
    let connection = state.connection.lock().expect("القفل يجب أن ينجح");
    let status: String = connection
        .query_row(
            "SELECT status FROM agent_runs WHERE id = 'run-a'",
            [],
            |row| row.get(0),
        )
        .expect("الحالة يجب أن تقرأ");
    let result_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM agent_execution_results", [], |row| {
            row.get(0)
        })
        .expect("عدد النتائج يجب أن يقرأ");
    assert_eq!(status, "RUNNING");
    assert_eq!(result_count, 0);
}

#[test]
fn execution_results_reject_update_and_delete_after_persistence() {
    let state = state_with_run();
    let provider = TestProvider {
        models: vec![discovered_model("qwen3:4b")],
        behavior: TestGenerationBehavior::Success,
    };
    let executor = ProviderAgentExecutor::new(&state, &provider);
    let result = GovernedAgentExecutorService::execute_trusted(&state, "run-a", input(), &executor)
        .expect("التنفيذ الحتمي يجب أن ينجح");
    let connection = state.connection.lock().expect("القفل يجب أن ينجح");

    let update = connection.execute(
        "UPDATE agent_execution_results SET created_at = '2026-01-01T00:00:00Z' WHERE id = ?1",
        params![&result.id],
    );
    let delete = connection.execute(
        "DELETE FROM agent_execution_results WHERE id = ?1",
        params![&result.id],
    );

    assert!(update.is_err());
    assert!(delete.is_err());
}
