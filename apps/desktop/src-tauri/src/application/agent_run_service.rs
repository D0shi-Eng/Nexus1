use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    application::{agent_run_approval_service, authorization_service},
    domain::{
        error::AppError,
        models::{
            is_valid_agent_run_transition, AgentRunRecord, AgentRunResultData, AgentRunStatus,
            AgentRunsData, AgentStatus,
        },
    },
    infrastructure::{
        agent_repository::AgentRepository,
        agent_run_repository::AgentRunRepository,
        database::{append_audit_event, latest_audit_event_hash, AppState},
        execution_result_repository::ExecutionResultRepository,
    },
};

const GOVERNED_EXECUTION_REQUEST: &str = "GOVERNED_EXECUTION_REQUEST";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgentRunInput {
    pub agent_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelAgentRunInput {
    pub run_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunMutationResult {
    pub run: AgentRunRecord,
}

pub fn list_agent_runs(state: &AppState) -> Result<AgentRunsData, AppError> {
    authorization_service::require_permission(state, "AGENT", "READ")?;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let items = AgentRunRepository::list(&connection, &state.tenant_id())?;
    Ok(AgentRunsData {
        has_data: !items.is_empty(),
        items,
    })
}

pub fn get_agent_run(state: &AppState, run_id: String) -> Result<AgentRunRecord, AppError> {
    authorization_service::require_permission(state, "AGENT", "READ")?;
    validate_identifier(&run_id)?;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    AgentRunRepository::find(&connection, &state.tenant_id(), &run_id)?
        .ok_or(AppError::ResourceNotFound)
}

// مسار قراءة نتيجة التنفيذ (§24 من عقد MP02): قراءة آمنة فقط —
// لا تحديث ولا حذف (الجدول محصن بمشغلات المنع)، والمخرَج يعرض منشأه كاملاً
// (مزود/نموذج/طابع زمني/رمز فشل مغلق) ولا يُرقّى إلى حقيقة بأي حال.
pub fn get_agent_run_result(
    state: &AppState,
    input: CancelAgentRunInput,
) -> Result<AgentRunResultData, AppError> {
    authorization_service::require_permission(state, "AGENT", "READ")?;
    validate_identifier(&input.run_id)?;

    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let run = AgentRunRepository::find(&connection, &state.tenant_id(), &input.run_id)?
        .ok_or(AppError::ResourceNotFound)?;
    let record = ExecutionResultRepository::find(&connection, &state.tenant_id(), &input.run_id)?
        .ok_or(AppError::ResourceNotFound)?;
    let (citations_json, truth_class) = ExecutionResultRepository::find_intelligence_fields(
        &connection,
        &state.tenant_id(),
        &input.run_id,
    )?
    .unwrap_or((None, "AI_INTERPRETATION".to_owned()));

    Ok(AgentRunResultData {
        run_id: run.id,
        run_status: run.status,
        outcome: record.result.outcome().as_code().to_owned(),
        provider_id: record.result.provider_id().as_str().to_owned(),
        model_id: record
            .result
            .model_id()
            .map(|model_id| model_id.as_str().to_owned()),
        failure_code: record
            .result
            .failure_code()
            .map(|code| code.as_code().to_owned()),
        output: record.result.output().map(str::to_owned),
        provider_generated_at: record.result.provider_generated_at().map(str::to_owned),
        created_at: record.created_at,
        citations_json,
        truth_class: Some(truth_class),
    })
}

pub fn create_agent_run(
    state: &AppState,
    input: CreateAgentRunInput,
) -> Result<AgentRunMutationResult, AppError> {
    authorization_service::require_permission(state, "AGENT", "EXECUTE")?;
    validate_identifier(&input.agent_id)?;

    let now = Utc::now().to_rfc3339();
    let record = AgentRunRecord {
        id: format!("agent_run_{}", Uuid::new_v4()),
        tenant_id: state.tenant_id(),
        agent_id: input.agent_id,
        requested_by_user_id: state.current_user_id()?,
        status: AgentRunStatus::Queued,
        request_summary: GOVERNED_EXECUTION_REQUEST.to_owned(),
        revision: 1,
        started_at: None,
        completed_at: None,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let agent = AgentRepository::find(&transaction, &state.tenant_id(), &record.agent_id)?
        .ok_or(AppError::ResourceNotFound)?;
    if agent.status != AgentStatus::Active {
        return Err(AppError::AgentDisabled);
    }

    AgentRunRepository::insert(&transaction, &record)?;
    append_agent_run_audit(&transaction, state, "AGENT_RUN_CREATED", &record.id, &now)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(AgentRunMutationResult { run: record })
}

pub fn cancel_agent_run(
    state: &AppState,
    input: CancelAgentRunInput,
) -> Result<AgentRunMutationResult, AppError> {
    authorization_service::require_permission(state, "AGENT", "EXECUTE")?;
    validate_identifier(&input.run_id)?;

    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let existing =
        AgentRunRepository::find_in_transaction(&transaction, &state.tenant_id(), &input.run_id)?
            .ok_or(AppError::ResourceNotFound)?;
    if existing.status == AgentRunStatus::WaitingApproval {
        let record = agent_run_approval_service::cancel_waiting_agent_run(
            &transaction,
            state,
            &existing,
            &now,
        )?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
        return Ok(AgentRunMutationResult { run: record });
    }

    let next_status = AgentRunStatus::Cancelled;
    if !is_valid_agent_run_transition(&existing.status, &next_status)
        || (existing.status != AgentRunStatus::Queued && existing.status != AgentRunStatus::Running)
    {
        return Err(AppError::InvalidStateTransition);
    }

    let changed = AgentRunRepository::transition(
        &transaction,
        &state.tenant_id(),
        &input.run_id,
        &existing.status,
        existing.revision,
        &next_status,
        existing.revision + 1,
        Some(&now),
        &now,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    let record =
        AgentRunRepository::find_in_transaction(&transaction, &state.tenant_id(), &input.run_id)?
            .ok_or(AppError::ResourceNotFound)?;
    append_agent_run_audit(&transaction, state, "AGENT_RUN_CANCELLED", &record.id, &now)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(AgentRunMutationResult { run: record })
}

fn validate_identifier(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

fn append_agent_run_audit(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    event_type: &str,
    run_id: &str,
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
        "AgentRun",
        run_id,
        occurred_at,
        &previous_hash,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {

    use chrono::Utc;
    use rusqlite::{params, Connection};

    use super::{
        cancel_agent_run, create_agent_run, get_agent_run, get_agent_run_result, list_agent_runs,
        CancelAgentRunInput, CreateAgentRunInput,
    };
    use crate::{
        domain::{
            error::AppError,
            models::{is_valid_agent_run_transition, AgentRunStatus},
        },
        infrastructure::database::AppState,
    };

    fn state_with_agent_permissions() -> AppState {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        crate::infrastructure::database::apply_migrations(&connection)
            .expect("هجرة الاختبار يجب أن تنجح");
        let now = Utc::now().to_rfc3339();
        for tenant_id in ["tenant-a", "tenant-b"] {
            connection
                .execute(
                    "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?2, 'DEMO_SYNTHETIC', ?3, ?3)",
                    params![tenant_id, tenant_id, &now],
                )
                .expect("المستأجر يجب أن ينشأ");
        }
        for (tenant_id, user_id) in [("tenant-a", "user-a"), ("tenant-b", "user-b")] {
            connection
                .execute(
                    "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, NULL, 'ACTIVE', ?5, ?5)",
                    params![user_id, tenant_id, user_id, format!("{user_id}@nexus.test"), &now],
                )
                .expect("المستخدم يجب أن ينشأ");
        }
        connection
            .execute(
                "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-a', 'tenant-a', 'ADMIN', 'Admin', ?1, ?1)",
                [&now],
            )
            .expect("الدور يجب أن ينشأ");
        for (permission_id, action) in [
            ("permission-agent-read", "READ"),
            ("permission-agent-execute", "EXECUTE"),
        ] {
            connection
                .execute(
                    "INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES (?1, ?2, 'AGENT', ?3, 'HIGHLY_RESTRICTED', ?4)",
                    params![permission_id, format!("agent:{action}"), action, &now],
                )
                .expect("الصلاحية يجب أن تنشأ");
            connection
                .execute(
                    "INSERT INTO role_permissions (role_id, permission_id) VALUES ('role-a', ?1)",
                    [permission_id],
                )
                .expect("منح الصلاحية يجب أن ينجح");
        }
        connection
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user-a', 'role-a', ?1, 'test')",
                [&now],
            )
            .expect("تعيين الدور يجب أن ينجح");
        connection
            .execute(
                "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session-a', 'tenant-a', 'user-a', ?1, '2030-01-01T00:00:00Z', NULL, 'test')",
                [&now],
            )
            .expect("الجلسة يجب أن تنشأ");
        for (agent_id, tenant_id, status) in [
            ("agent-active-a", "tenant-a", "ACTIVE"),
            ("agent-disabled-a", "tenant-a", "DISABLED"),
            ("agent-active-b", "tenant-b", "ACTIVE"),
        ] {
            connection
                .execute(
                    "INSERT INTO agents (id, tenant_id, name, description, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'Agent test', ?4, ?5, ?5)",
                    params![agent_id, tenant_id, agent_id, status, &now],
                )
                .expect("الوكيل يجب أن ينشأ");
        }

        AppState::test_instance(connection, "tenant-a".to_owned(), "user-a".to_owned())
    }

    #[test]
    fn migration_preserves_foreign_keys_and_indexes() {
        let state = state_with_agent_permissions();
        let connection = state.connection.lock().expect("القفل يجب أن ينجح");
        let foreign_key_errors: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("فحص المفاتيح يجب أن ينجح");
        let index_exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'idx_agent_runs_tenant_updated')",
                [],
                |row| row.get(0),
            )
            .expect("فحص الفهرس يجب أن ينجح");
        let run_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))
            .expect("عدد السجلات يجب أن يقرأ");
        assert_eq!(foreign_key_errors, 0);
        assert!(index_exists);
        assert_eq!(run_count, 0);
        assert!(connection
            .execute(
                "INSERT INTO agent_runs (id, tenant_id, agent_id, requested_by_user_id, status, request_summary, started_at, completed_at, created_at, updated_at) VALUES ('invalid-run', 'tenant-a', 'agent-active-a', 'user-a', 'UNKNOWN', 'GOVERNED_EXECUTION_REQUEST', NULL, NULL, '2026-08-21T00:00:00Z', '2026-08-21T00:00:00Z')",
                [],
            )
            .is_err());
    }

    #[test]
    fn creates_active_agent_run_and_audits_it() {
        let state = state_with_agent_permissions();
        let result = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-active-a".to_owned(),
            },
        )
        .expect("إنشاء الطلب يجب أن ينجح");
        assert_eq!(result.run.status, AgentRunStatus::Queued);
        assert_eq!(result.run.request_summary, "GOVERNED_EXECUTION_REQUEST");
        assert_eq!(result.run.requested_by_user_id, "user-a");
        assert_eq!(result.run.revision, 1);

        let connection = state.connection.lock().expect("القفل يجب أن ينجح");
        let audit_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE event_type = 'AGENT_RUN_CREATED' AND resource_id = ?1",
                [&result.run.id],
                |row| row.get(0),
            )
            .expect("فحص التدقيق يجب أن ينجح");
        assert_eq!(audit_count, 1);
    }

    #[test]
    fn rolls_back_creation_when_run_audit_fails() {
        let state = state_with_agent_permissions();
        let connection = state.connection.lock().expect("القفل يجب أن ينجح");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_agent_run_audit BEFORE INSERT ON audit_events
                 WHEN NEW.event_type = 'AGENT_RUN_CREATED'
                 BEGIN
                   SELECT RAISE(ABORT, 'رفض اختبار التدقيق');
                 END;",
            )
            .expect("مشغل الاختبار يجب أن ينشأ");
        drop(connection);

        assert!(matches!(
            create_agent_run(
                &state,
                CreateAgentRunInput {
                    agent_id: "agent-active-a".to_owned(),
                },
            ),
            Err(AppError::StorageOperation)
        ));
        let connection = state.connection.lock().expect("القفل يجب أن ينجح");
        let run_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM agent_runs", [], |row| row.get(0))
            .expect("عدد السجلات يجب أن يقرأ");
        assert_eq!(run_count, 0);
    }

    #[test]
    fn rejects_disabled_and_cross_tenant_agents() {
        let state = state_with_agent_permissions();
        let disabled = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-disabled-a".to_owned(),
            },
        )
        .expect_err("الوكيل المعطل يجب أن يرفض");
        assert!(matches!(disabled, AppError::AgentDisabled));

        let cross_tenant = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-active-b".to_owned(),
            },
        )
        .expect_err("وكيل مستأجر آخر يجب أن يرفض");
        assert!(matches!(cross_tenant, AppError::ResourceNotFound));
    }

    #[test]
    fn allows_queued_cancellation_and_rejects_repeat() {
        let state = state_with_agent_permissions();
        let created = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-active-a".to_owned(),
            },
        )
        .expect("إنشاء الطلب يجب أن ينجح");
        let cancelled = cancel_agent_run(
            &state,
            CancelAgentRunInput {
                run_id: created.run.id.clone(),
            },
        )
        .expect("إلغاء الطلب المنتظر يجب أن ينجح");
        assert_eq!(cancelled.run.status, AgentRunStatus::Cancelled);
        assert!(cancelled.run.completed_at.is_some());

        let repeated = cancel_agent_run(
            &state,
            CancelAgentRunInput {
                run_id: created.run.id,
            },
        )
        .expect_err("إلغاء الحالة النهائية يجب أن يرفض");
        assert!(matches!(repeated, AppError::InvalidStateTransition));
    }

    #[test]
    fn isolates_reads_and_checks_transition_matrix() {
        let state = state_with_agent_permissions();
        let created = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-active-a".to_owned(),
            },
        )
        .expect("إنشاء الطلب يجب أن ينجح");
        let found = get_agent_run(&state, created.run.id.clone()).expect("القراءة يجب أن تنجح");
        assert_eq!(found.id, created.run.id);
        assert!(
            list_agent_runs(&state)
                .expect("القائمة يجب أن تنجح")
                .has_data
        );

        {
            let connection = state.connection.lock().expect("القفل يجب أن ينجح");
            connection
                .execute(
                    "UPDATE agent_runs SET tenant_id = 'tenant-b' WHERE id = ?1",
                    [&created.run.id],
                )
                .expect_err("قيد الملكية المركب يجب أن يمنع النقل العابر للمستأجر");
            connection
                .execute(
                    "INSERT INTO agent_runs (id, tenant_id, agent_id, requested_by_user_id, status, request_summary, started_at, completed_at, created_at, updated_at) VALUES ('agent-run-b', 'tenant-b', 'agent-active-b', 'user-b', 'QUEUED', 'GOVERNED_EXECUTION_REQUEST', NULL, NULL, '2026-08-21T00:00:00Z', '2026-08-21T00:00:00Z')",
                    [],
                )
                .expect("تشغيل المستأجر الآخر يجب أن ينشأ");
        }
        assert!(matches!(
            get_agent_run(&state, "agent-run-b".to_owned()),
            Err(AppError::ResourceNotFound)
        ));
        assert!(matches!(
            cancel_agent_run(
                &state,
                CancelAgentRunInput {
                    run_id: "agent-run-b".to_owned(),
                },
            ),
            Err(AppError::ResourceNotFound)
        ));

        assert!(is_valid_agent_run_transition(
            &AgentRunStatus::Queued,
            &AgentRunStatus::Running
        ));
        assert!(is_valid_agent_run_transition(
            &AgentRunStatus::Queued,
            &AgentRunStatus::WaitingApproval
        ));
        assert!(is_valid_agent_run_transition(
            &AgentRunStatus::Running,
            &AgentRunStatus::WaitingApproval
        ));
        assert!(is_valid_agent_run_transition(
            &AgentRunStatus::WaitingApproval,
            &AgentRunStatus::Running
        ));
        assert!(is_valid_agent_run_transition(
            &AgentRunStatus::WaitingApproval,
            &AgentRunStatus::Queued
        ));
        assert!(is_valid_agent_run_transition(
            &AgentRunStatus::Running,
            &AgentRunStatus::Completed
        ));
        assert!(is_valid_agent_run_transition(
            &AgentRunStatus::Running,
            &AgentRunStatus::Failed
        ));
        assert!(is_valid_agent_run_transition(
            &AgentRunStatus::Queued,
            &AgentRunStatus::Cancelled
        ));
        assert!(is_valid_agent_run_transition(
            &AgentRunStatus::Running,
            &AgentRunStatus::Cancelled
        ));
        assert!(is_valid_agent_run_transition(
            &AgentRunStatus::WaitingApproval,
            &AgentRunStatus::Cancelled
        ));
        assert!(!is_valid_agent_run_transition(
            &AgentRunStatus::Queued,
            &AgentRunStatus::Completed
        ));
    }

    #[test]
    fn rejects_expired_revoked_and_locked_requesters() {
        let state = state_with_agent_permissions();
        let connection = state.connection.lock().expect("القفل يجب أن ينجح");
        connection
            .execute(
                "UPDATE local_sessions SET expires_at = '2000-01-01T00:00:00Z'",
                [],
            )
            .expect("تعديل الجلسة يجب أن ينجح");
        drop(connection);
        assert!(matches!(
            create_agent_run(
                &state,
                CreateAgentRunInput {
                    agent_id: "agent-active-a".to_owned(),
                },
            ),
            Err(AppError::AuthorizationDenied)
        ));

        let connection = state.connection.lock().expect("القفل يجب أن ينجح");
        connection
            .execute(
                "UPDATE local_sessions SET expires_at = '2030-01-01T00:00:00Z', revoked_at = ?1",
                [Utc::now().to_rfc3339()],
            )
            .expect("إلغاء الجلسة يجب أن ينجح");
        drop(connection);
        assert!(matches!(
            create_agent_run(
                &state,
                CreateAgentRunInput {
                    agent_id: "agent-active-a".to_owned(),
                },
            ),
            Err(AppError::AuthorizationDenied)
        ));

        let connection = state.connection.lock().expect("القفل يجب أن ينجح");
        connection
            .execute("UPDATE local_sessions SET revoked_at = NULL", [])
            .expect("إرجاع الجلسة يجب أن ينجح");
        connection
            .execute(
                "UPDATE local_users SET status = 'LOCKED' WHERE id = 'user-a'",
                [],
            )
            .expect("قفل المستخدم يجب أن ينجح");
        drop(connection);
        assert!(matches!(
            create_agent_run(
                &state,
                CreateAgentRunInput {
                    agent_id: "agent-active-a".to_owned(),
                },
            ),
            Err(AppError::AuthorizationDenied)
        ));
    }

    fn seed_execution_result(
        state: &AppState,
        run_id: &str,
        outcome: &str,
        output: Option<&str>,
        failure_code: Option<&str>,
    ) {
        let connection = state.connection.lock().expect("القفل يجب أن ينجح");
        connection
            .execute(
                "INSERT INTO agent_execution_results (id, tenant_id, agent_run_id, agent_run_revision, outcome, provider_id, model_id, output, failure_code, provider_generated_at, created_at)
                 VALUES (?1, 'tenant-a', ?2, 1, ?3, 'OLLAMA_LOCAL', ?4, ?5, ?6, ?7, '2026-08-24T00:00:00Z')",
                params![
                    format!("result-{run_id}"),
                    run_id,
                    outcome,
                    if outcome == "COMPLETED" {
                        Some("qwen3:4b".to_owned())
                    } else {
                        None
                    },
                    output.map(str::to_owned),
                    failure_code.map(str::to_owned),
                    if outcome == "COMPLETED" {
                        Some("2026-08-24T00:00:00Z".to_owned())
                    } else {
                        None
                    },
                ],
            )
            .expect("نتيجة التنفيذ يجب أن تُزرع");
    }

    #[test]
    fn reads_execution_result_with_provenance_and_fails_closed_for_missing_results() {
        let state = state_with_agent_permissions();
        let created = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-active-a".to_owned(),
            },
        )
        .expect("إنشاء الطلب يجب أن ينجح");

        // لا نتيجة بعد → رفض مغلق لا بيانات جزئية.
        assert!(matches!(
            get_agent_run_result(
                &state,
                CancelAgentRunInput {
                    run_id: created.run.id.clone(),
                },
            ),
            Err(AppError::ResourceNotFound)
        ));

        seed_execution_result(
            &state,
            &created.run.id,
            "COMPLETED",
            Some("trusted bounded output"),
            None,
        );
        let result = get_agent_run_result(
            &state,
            CancelAgentRunInput {
                run_id: created.run.id.clone(),
            },
        )
        .expect("قراءة النتيجة يجب أن تنجح");
        assert_eq!(result.outcome, "COMPLETED");
        assert_eq!(result.provider_id, "OLLAMA_LOCAL");
        assert_eq!(result.model_id.as_deref(), Some("qwen3:4b"));
        assert_eq!(result.output.as_deref(), Some("trusted bounded output"));
        assert_eq!(result.run_status, AgentRunStatus::Queued);

        // فشل مغلق للطلب العابر للمستأجرين ولو وُجدت نتيجة له.
        {
            let connection = state.connection.lock().expect("القفل يجب أن ينجح");
            connection
                .execute(
                    "INSERT INTO agent_runs (id, tenant_id, agent_id, requested_by_user_id, status, request_summary, revision, started_at, completed_at, created_at, updated_at) VALUES ('agent-run-result-b', 'tenant-b', 'agent-active-b', 'user-b', 'FAILED', 'GOVERNED_EXECUTION_REQUEST', 1, '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z')",
                    [],
                )
                .expect("تشغيل المستأجر الآخر يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO agent_execution_results (id, tenant_id, agent_run_id, agent_run_revision, outcome, provider_id, model_id, output, failure_code, provider_generated_at, created_at)
                     VALUES ('result-b', 'tenant-b', 'agent-run-result-b', 1, 'FAILED', 'OLLAMA_LOCAL', NULL, NULL, 'PROVIDER_TIMEOUT', NULL, '2026-08-24T00:00:00Z')",
                    [],
                )
                .expect("نتيجة المستأجر الآخر يجب أن تنشأ");
        }
        assert!(matches!(
            get_agent_run_result(
                &state,
                CancelAgentRunInput {
                    run_id: "agent-run-result-b".to_owned(),
                },
            ),
            Err(AppError::ResourceNotFound)
        ));

        // بلا صلاحية قراءة وكلاء: رفض تفويض.
        {
            let connection = state.connection.lock().expect("القفل يجب أن ينجح");
            connection
                .execute(
                    "DELETE FROM role_permissions WHERE permission_id = 'permission-agent-read'",
                    [],
                )
                .expect("سحب صلاحية القراءة يجب أن ينجح");
        }
        assert!(matches!(
            get_agent_run_result(
                &state,
                CancelAgentRunInput {
                    run_id: created.run.id,
                },
            ),
            Err(AppError::AuthorizationDenied)
        ));
    }
}
