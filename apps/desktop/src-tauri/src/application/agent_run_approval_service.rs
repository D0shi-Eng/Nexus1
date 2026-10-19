use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        models::{is_valid_agent_run_transition, AgentRunRecord, AgentRunStatus, AgentStatus},
        trusted_skill_selection::TrustedSkillSelection,
    },
    infrastructure::{
        agent_repository::AgentRepository,
        agent_run_repository::AgentRunRepository,
        agent_run_skill_binding_repository::AgentRunSkillBindingRepository,
        approval_repository::{AgentRunApprovalLink, ApprovalRecord, ApprovalRepository},
        database::{append_audit_event, latest_audit_event_hash, AppState},
    },
};

#[derive(Debug, Clone)]
pub struct AgentRunApprovalRequirement {
    pub approval_requirement_code: String,
    pub required_role_code: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunApprovalRequestResult {
    pub approval_id: String,
    pub run: AgentRunRecord,
}

pub fn request_agent_run_approval(
    state: &AppState,
    run_id: &str,
    requirement: AgentRunApprovalRequirement,
) -> Result<AgentRunApprovalRequestResult, AppError> {
    request_agent_run_approval_with_skill(state, run_id, requirement, None)
}

pub fn request_agent_run_approval_with_skill(
    state: &AppState,
    run_id: &str,
    requirement: AgentRunApprovalRequirement,
    selection: Option<TrustedSkillSelection>,
) -> Result<AgentRunApprovalRequestResult, AppError> {
    authorization_service::require_permission(state, "AGENT", "EXECUTE")?;
    validate_identifier(run_id)?;
    validate_requirement(&requirement)?;

    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let run = AgentRunRepository::find_in_transaction(&transaction, &state.tenant_id(), run_id)?
        .ok_or(AppError::ResourceNotFound)?;
    let agent = AgentRepository::find(&transaction, &state.tenant_id(), &run.agent_id)?
        .ok_or(AppError::ResourceNotFound)?;
    if agent.status != AgentStatus::Active {
        return Err(AppError::AgentDisabled);
    }
    if let Some(selection) = selection.as_ref() {
        validate_skill_selection(state, &run, selection)?;
    }

    if let Some(existing) = ApprovalRepository::find_pending_agent_run_request(
        &transaction,
        &state.tenant_id(),
        &run.id,
        run.revision,
        &requirement.approval_requirement_code,
    )? {
        if run.status != AgentRunStatus::WaitingApproval {
            return Err(AppError::Conflict);
        }
        if let Some(selection) = selection.as_ref() {
            ensure_skill_binding_matches(&transaction, state, &run, selection)?;
        }
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
        return Ok(AgentRunApprovalRequestResult {
            approval_id: existing.id,
            run,
        });
    }

    if !is_valid_agent_run_transition(&run.status, &AgentRunStatus::WaitingApproval)
        || run.status != AgentRunStatus::Queued
    {
        return Err(AppError::InvalidStateTransition);
    }

    let approval = ApprovalRecord {
        id: format!("approval_{}", Uuid::new_v4()),
        tenant_id: state.tenant_id(),
        resource_type: "AGENT_RUN".to_owned(),
        resource_id: run.id.clone(),
        resource_version: run.revision,
        requested_by: run.requested_by_user_id.clone(),
        required_role_code: requirement.required_role_code,
        approval_requirement_code: Some(requirement.approval_requirement_code),
        decision: "PENDING".to_owned(),
    };
    let link = AgentRunApprovalLink {
        approval_id: approval.id.clone(),
        tenant_id: state.tenant_id(),
        agent_run_id: run.id.clone(),
        resource_version: run.revision,
    };

    if let Some(selection) = selection.as_ref() {
        AgentRunSkillBindingRepository::insert(
            &transaction,
            &run.id,
            run.revision,
            selection,
            &now,
        )?;
    }
    ApprovalRepository::insert_agent_run_request(&transaction, &approval, &now)?;
    ApprovalRepository::insert_agent_run_link(&transaction, &link, &now)?;
    let changed = AgentRunRepository::transition(
        &transaction,
        &state.tenant_id(),
        &run.id,
        &run.status,
        run.revision,
        &AgentRunStatus::WaitingApproval,
        run.revision,
        None,
        &now,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    append_audit(
        &transaction,
        state,
        "AGENT_RUN_APPROVAL_REQUESTED",
        "AgentRun",
        &run.id,
        &now,
    )?;
    let updated_run =
        AgentRunRepository::find_in_transaction(&transaction, &state.tenant_id(), &run.id)?
            .ok_or(AppError::ResourceNotFound)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(AgentRunApprovalRequestResult {
        approval_id: approval.id,
        run: updated_run,
    })
}

fn validate_skill_selection(
    state: &AppState,
    run: &AgentRunRecord,
    selection: &TrustedSkillSelection,
) -> Result<(), AppError> {
    if selection.tenant_id() != state.tenant_id().as_str() || selection.agent_id() != run.agent_id {
        return Err(AppError::AuthorizationDenied);
    }
    Ok(())
}

fn ensure_skill_binding_matches(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    run: &AgentRunRecord,
    selection: &TrustedSkillSelection,
) -> Result<(), AppError> {
    let bound = AgentRunSkillBindingRepository::find(transaction, &state.tenant_id(), &run.id)?
        .ok_or(AppError::StaleApproval)?;
    let bound_revision =
        AgentRunSkillBindingRepository::find_revision(transaction, &state.tenant_id(), &run.id)?
            .ok_or(AppError::StaleApproval)?;
    if bound_revision != run.revision || bound != *selection {
        return Err(AppError::StaleApproval);
    }
    Ok(())
}

pub fn cancel_waiting_agent_run(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    run: &AgentRunRecord,
    cancelled_at: &str,
) -> Result<AgentRunRecord, AppError> {
    if run.status != AgentRunStatus::WaitingApproval
        || !is_valid_agent_run_transition(&run.status, &AgentRunStatus::Cancelled)
    {
        return Err(AppError::InvalidStateTransition);
    }
    if run.requested_by_user_id != state.current_user_id()? {
        return Err(AppError::AuthorizationDenied);
    }

    // التشغيل المنتظر قد يكون بانتظار موافقة تشغيل (وكلاء) أو موافقة استدعاء
    // أداة (MP03) — كلاهما يُلغى من قِبل صاحب الطلب وحده.
    let pending_run_approval = ApprovalRepository::find_pending_agent_run_for_run(
        transaction,
        &state.tenant_id(),
        &run.id,
    )?;
    if pending_run_approval.is_none() {
        let tool_cancelled =
            crate::application::tool_invocation_service::cancel_pending_tool_invocation_for_run(
                transaction,
                state,
                run,
                cancelled_at,
            )?;
        if tool_cancelled {
            let changed = AgentRunRepository::transition(
                transaction,
                &state.tenant_id(),
                &run.id,
                &run.status,
                run.revision,
                &AgentRunStatus::Cancelled,
                run.revision + 1,
                Some(cancelled_at),
                cancelled_at,
            )?;
            if !changed {
                return Err(AppError::Conflict);
            }
            append_audit(
                transaction,
                state,
                "AGENT_RUN_CANCELLED",
                "AgentRun",
                &run.id,
                cancelled_at,
            )?;
            return AgentRunRepository::find_in_transaction(
                transaction,
                &state.tenant_id(),
                &run.id,
            )?
            .ok_or(AppError::ResourceNotFound);
        }
    }

    let approval = ApprovalRepository::find_pending_agent_run_for_run(
        transaction,
        &state.tenant_id(),
        &run.id,
    )?
    .ok_or(AppError::StaleApproval)?;
    let link =
        ApprovalRepository::find_agent_run_link(transaction, &state.tenant_id(), &approval.id)?
            .ok_or(AppError::StaleApproval)?;
    if approval.resource_version != run.revision
        || link.resource_version != run.revision
        || link.agent_run_id != run.id
    {
        return Err(AppError::StaleApproval);
    }

    let approval_cancelled = ApprovalRepository::cancel_pending(
        transaction,
        &state.tenant_id(),
        &approval.id,
        &state.current_user_id()?,
        cancelled_at,
    )?;
    if !approval_cancelled {
        return Err(AppError::Conflict);
    }
    let changed = AgentRunRepository::transition(
        transaction,
        &state.tenant_id(),
        &run.id,
        &run.status,
        run.revision,
        &AgentRunStatus::Cancelled,
        run.revision + 1,
        Some(cancelled_at),
        cancelled_at,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    append_audit(
        transaction,
        state,
        "APPROVAL_CANCELLED",
        "Approval",
        &approval.id,
        cancelled_at,
    )?;
    append_audit(
        transaction,
        state,
        "AGENT_RUN_CANCELLED",
        "AgentRun",
        &run.id,
        cancelled_at,
    )?;
    AgentRunRepository::find_in_transaction(transaction, &state.tenant_id(), &run.id)?
        .ok_or(AppError::ResourceNotFound)
}

pub fn transition_after_approval_decision(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    approval: &ApprovalRecord,
    decided_at: &str,
) -> Result<AgentRunRecord, AppError> {
    let link =
        ApprovalRepository::find_agent_run_link(transaction, &state.tenant_id(), &approval.id)?
            .ok_or(AppError::StaleApproval)?;
    let run = AgentRunRepository::find_in_transaction(
        transaction,
        &state.tenant_id(),
        &link.agent_run_id,
    )?
    .ok_or(AppError::StaleApproval)?;
    if approval.resource_type != "AGENT_RUN"
        || approval.resource_id != run.id
        || approval.resource_version != run.revision
        || link.resource_version != run.revision
        || run.status != AgentRunStatus::WaitingApproval
    {
        return Err(AppError::StaleApproval);
    }

    let next_status = match approval.decision.as_str() {
        "APPROVED" => AgentRunStatus::Queued,
        "REJECTED" => AgentRunStatus::Cancelled,
        _ => return Err(AppError::ValidationFailed),
    };
    if !is_valid_agent_run_transition(&run.status, &next_status) {
        return Err(AppError::InvalidStateTransition);
    }
    let completed_at = if next_status == AgentRunStatus::Cancelled {
        Some(decided_at)
    } else {
        None
    };
    let changed = AgentRunRepository::transition(
        transaction,
        &state.tenant_id(),
        &run.id,
        &run.status,
        run.revision,
        &next_status,
        run.revision + 1,
        completed_at,
        decided_at,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    append_audit(
        transaction,
        state,
        if next_status == AgentRunStatus::Queued {
            "AGENT_RUN_REQUEUED_AFTER_APPROVAL"
        } else {
            "AGENT_RUN_CANCELLED"
        },
        "AgentRun",
        &run.id,
        decided_at,
    )?;
    AgentRunRepository::find_in_transaction(transaction, &state.tenant_id(), &run.id)?
        .ok_or(AppError::ResourceNotFound)
}

fn validate_identifier(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

fn validate_requirement(requirement: &AgentRunApprovalRequirement) -> Result<(), AppError> {
    if !is_code(&requirement.approval_requirement_code) || !is_code(&requirement.required_role_code)
    {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

fn is_code(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 128
        && value.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
}

fn append_audit(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    event_type: &str,
    resource_type: &str,
    resource_id: &str,
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
        resource_type,
        resource_id,
        occurred_at,
        &previous_hash,
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {

    use chrono::Utc;
    use rusqlite::{params, Connection};

    use super::{request_agent_run_approval, AgentRunApprovalRequirement};
    use crate::{
        application::{
            agent_run_service::{
                cancel_agent_run, create_agent_run, CancelAgentRunInput, CreateAgentRunInput,
            },
            approval_service::{decide_approval, DecideApprovalInput},
        },
        domain::{error::AppError, models::AgentRunStatus},
        infrastructure::database::AppState,
    };

    fn state_with_approval_lifecycle() -> AppState {
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
        for user_id in ["requester", "reviewer", "wrong-role"] {
            connection
                .execute(
                    "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, 'tenant-a', ?1, ?2, NULL, 'ACTIVE', ?3, ?3)",
                    params![user_id, format!("{user_id}@nexus.test"), &now],
                )
                .expect("المستخدم يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, 'tenant-a', ?2, ?3, '2030-01-01T00:00:00Z', NULL, 'test')",
                    params![format!("session-{user_id}"), user_id, &now],
                )
                .expect("الجلسة يجب أن تنشأ");
        }
        for (role_id, role_code) in [
            ("role-executor", "EXECUTOR"),
            ("role-reviewer", "TENANT_ADMIN"),
            ("role-reader", "READER"),
        ] {
            connection
                .execute(
                    "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES (?1, 'tenant-a', ?2, ?2, ?3, ?3)",
                    params![role_id, role_code, &now],
                )
                .expect("الدور يجب أن ينشأ");
        }
        for (permission_id, resource_type, action) in [
            ("permission-execute", "AGENT", "EXECUTE"),
            ("permission-decide", "APPROVAL", "DECIDE"),
        ] {
            connection
                .execute(
                    "INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES (?1, ?2, ?3, ?4, 'HIGHLY_RESTRICTED', ?5)",
                    params![permission_id, format!("{resource_type}:{action}"), resource_type, action, &now],
                )
                .expect("الصلاحية يجب أن تنشأ");
        }
        for (user_id, role_id) in [
            ("requester", "role-executor"),
            ("reviewer", "role-reviewer"),
            ("wrong-role", "role-reader"),
        ] {
            connection
                .execute(
                    "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, ?2, ?3, 'test')",
                    params![user_id, role_id, &now],
                )
                .expect("تعيين الدور يجب أن ينجح");
        }
        for (role_id, permission_id) in [
            ("role-executor", "permission-execute"),
            ("role-reviewer", "permission-decide"),
            ("role-reader", "permission-decide"),
        ] {
            connection
                .execute(
                    "INSERT INTO role_permissions (role_id, permission_id) VALUES (?1, ?2)",
                    params![role_id, permission_id],
                )
                .expect("منح الصلاحية يجب أن ينجح");
        }
        for (agent_id, tenant_id, status) in [
            ("agent-a", "tenant-a", "ACTIVE"),
            ("agent-disabled", "tenant-a", "DISABLED"),
            ("agent-b", "tenant-b", "ACTIVE"),
        ] {
            connection
                .execute(
                    "INSERT INTO agents (id, tenant_id, name, description, status, created_at, updated_at) VALUES (?1, ?2, ?1, 'Test agent', ?3, ?4, ?4)",
                    params![agent_id, tenant_id, status, &now],
                )
                .expect("الوكيل يجب أن ينشأ");
        }

        AppState::test_instance(connection, "tenant-a".to_owned(), "requester".to_owned())
    }

    fn requirement() -> AgentRunApprovalRequirement {
        AgentRunApprovalRequirement {
            approval_requirement_code: "PRIVILEGED_EFFECT".to_owned(),
            required_role_code: "TENANT_ADMIN".to_owned(),
        }
    }

    #[test]
    fn request_is_atomic_idempotent_and_pre_execution() {
        let state = state_with_approval_lifecycle();
        let created = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-a".to_owned(),
            },
        )
        .expect("إنشاء AgentRun يجب أن ينجح");
        assert_eq!(created.run.revision, 1);

        let first = request_agent_run_approval(&state, &created.run.id, requirement())
            .expect("طلب الموافقة يجب أن ينجح");
        let second = request_agent_run_approval(&state, &created.run.id, requirement())
            .expect("إعادة المحاولة يجب أن تعيد الطلب القائم");
        assert_eq!(first.approval_id, second.approval_id);
        assert_eq!(first.run.status, AgentRunStatus::WaitingApproval);
        assert_eq!(first.run.revision, 1);
        assert!(first.run.started_at.is_none());
        assert!(first.run.completed_at.is_none());

        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let pending_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM approvals WHERE resource_type = 'AGENT_RUN' AND decision = 'PENDING'",
                [],
                |row| row.get(0),
            )
            .expect("عدد الطلبات يجب أن يقرأ");
        let link_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM agent_run_approval_links", [], |row| {
                row.get(0)
            })
            .expect("عدد الروابط يجب أن يقرأ");
        let request_audit_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE event_type = 'AGENT_RUN_APPROVAL_REQUESTED' AND resource_id = ?1",
                [&created.run.id],
                |row| row.get(0),
            )
            .expect("تدقيق الطلب يجب أن يقرأ");
        assert_eq!(pending_count, 1);
        assert_eq!(link_count, 1);
        assert_eq!(request_audit_count, 1);
    }

    #[test]
    fn approval_grant_requeues_and_increments_revision() {
        let state = state_with_approval_lifecycle();
        let created = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-a".to_owned(),
            },
        )
        .expect("إنشاء AgentRun يجب أن ينجح");
        let request = request_agent_run_approval(&state, &created.run.id, requirement())
            .expect("طلب الموافقة يجب أن ينجح");
        state.set_active_user_for_tests("reviewer");

        decide_approval(
            &state,
            DecideApprovalInput {
                approval_id: request.approval_id.clone(),
                decision: "APPROVED".to_owned(),
                reason: "Independent reviewer approved the governed request.".to_owned(),
            },
        )
        .expect("قرار القبول يجب أن ينجح");

        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let (status, revision, started_at, completed_at): (
            String,
            i64,
            Option<String>,
            Option<String>,
        ) = connection
            .query_row(
                "SELECT status, revision, started_at, completed_at FROM agent_runs WHERE id = ?1",
                [&created.run.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("التشغيل يجب أن يقرأ");
        let decision: String = connection
            .query_row(
                "SELECT decision FROM approvals WHERE id = ?1",
                [&request.approval_id],
                |row| row.get(0),
            )
            .expect("القرار يجب أن يقرأ");
        assert_eq!(status, "QUEUED");
        assert_eq!(revision, 2);
        assert!(started_at.is_none());
        assert!(completed_at.is_none());
        assert_eq!(decision, "APPROVED");
    }

    #[test]
    fn rejection_cancels_run_and_cancellation_invalidates_pending_approval() {
        let state = state_with_approval_lifecycle();
        let created = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-a".to_owned(),
            },
        )
        .expect("إنشاء AgentRun يجب أن ينجح");
        let rejected_request = request_agent_run_approval(&state, &created.run.id, requirement())
            .expect("طلب الموافقة يجب أن ينجح");
        state.set_active_user_for_tests("reviewer");
        decide_approval(
            &state,
            DecideApprovalInput {
                approval_id: rejected_request.approval_id,
                decision: "REJECTED".to_owned(),
                reason: "Independent reviewer rejected the governed request.".to_owned(),
            },
        )
        .expect("قرار الرفض يجب أن ينجح");
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let (status, revision): (String, i64) = connection
            .query_row(
                "SELECT status, revision FROM agent_runs WHERE id = ?1",
                [&created.run.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("التشغيل المرفوض يجب أن يقرأ");
        assert_eq!(status, "CANCELLED");
        assert_eq!(revision, 2);
        drop(connection);

        let created_again = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-a".to_owned(),
            },
        );
        assert!(matches!(created_again, Err(AppError::AuthorizationDenied)));
        state.set_active_user_for_tests("requester");
        let waiting_run = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-a".to_owned(),
            },
        )
        .expect("إنشاء تشغيل ثانٍ يجب أن ينجح");
        let pending_request =
            request_agent_run_approval(&state, &waiting_run.run.id, requirement())
                .expect("طلب الموافقة الثاني يجب أن ينجح");
        cancel_agent_run(
            &state,
            CancelAgentRunInput {
                run_id: waiting_run.run.id.clone(),
            },
        )
        .expect("إلغاء صاحب الطلب يجب أن ينجح");
        state.set_active_user_for_tests("reviewer");
        assert!(matches!(
            decide_approval(
                &state,
                DecideApprovalInput {
                    approval_id: pending_request.approval_id,
                    decision: "APPROVED".to_owned(),
                    reason: "Cancelled approval must not be usable.".to_owned(),
                },
            ),
            Err(AppError::StaleApproval)
        ));
    }

    #[test]
    fn request_rejects_disabled_agent_and_rolls_back_when_audit_fails() {
        let state = state_with_approval_lifecycle();
        let active_run = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-a".to_owned(),
            },
        )
        .expect("إنشاء AgentRun يجب أن ينجح");
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "UPDATE agents SET status = 'DISABLED' WHERE id = 'agent-a'",
                [],
            )
            .expect("تعطيل الوكيل يجب أن ينجح");
        drop(connection);
        assert!(matches!(
            request_agent_run_approval(&state, &active_run.run.id, requirement()),
            Err(AppError::AgentDisabled)
        ));

        let state = state_with_approval_lifecycle();
        let run = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-a".to_owned(),
            },
        )
        .expect("إنشاء AgentRun يجب أن ينجح");
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_agent_run_approval_audit BEFORE INSERT ON audit_events
                 WHEN NEW.event_type = 'AGENT_RUN_APPROVAL_REQUESTED'
                 BEGIN
                   SELECT RAISE(ABORT, 'رفض اختبار تدقيق الموافقة');
                 END;",
            )
            .expect("مشغل الاختبار يجب أن ينشأ");
        drop(connection);
        assert!(matches!(
            request_agent_run_approval(&state, &run.run.id, requirement()),
            Err(AppError::StorageOperation)
        ));
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let (status, approval_count): (String, i64) = connection
            .query_row(
                "SELECT (SELECT status FROM agent_runs WHERE id = ?1), (SELECT COUNT(*) FROM approvals WHERE resource_type = 'AGENT_RUN')",
                [&run.run.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("نتيجة التراجع يجب أن تقرأ");
        assert_eq!(status, "QUEUED");
        assert_eq!(approval_count, 0);
    }

    #[test]
    fn winning_decision_blocks_competing_decision_and_disabled_agent_stays_denied() {
        let state = state_with_approval_lifecycle();
        let created = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-a".to_owned(),
            },
        )
        .expect("إنشاء AgentRun يجب أن ينجح");
        let request = request_agent_run_approval(&state, &created.run.id, requirement())
            .expect("طلب الموافقة يجب أن ينجح");
        state.set_active_user_for_tests("reviewer");
        decide_approval(
            &state,
            DecideApprovalInput {
                approval_id: request.approval_id.clone(),
                decision: "APPROVED".to_owned(),
                reason: "First reviewer decision wins the pending row.".to_owned(),
            },
        )
        .expect("قرار القبول الأول يجب أن ينجح");
        assert!(matches!(
            decide_approval(
                &state,
                DecideApprovalInput {
                    approval_id: request.approval_id,
                    decision: "REJECTED".to_owned(),
                    reason: "Competing decision must not overwrite the winner.".to_owned(),
                },
            ),
            Err(AppError::StaleApproval)
        ));
        state.set_active_user_for_tests("requester");
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "UPDATE agents SET status = 'DISABLED' WHERE id = 'agent-a'",
                [],
            )
            .expect("تعطيل الوكيل يجب أن ينجح");
        drop(connection);
        let decision =
            crate::application::execution_dispatcher::ExecutionDispatcher::evaluate_readiness(
                &state,
                &created.run.id,
            )
            .expect("تقييم الجاهزية يجب أن ينجح");
        assert_eq!(
            decision.outcome,
            crate::domain::execution_policy::ExecutionDecisionOutcome::Deny
        );
        assert_eq!(
            decision.reason,
            crate::domain::execution_policy::ExecutionDecisionReason::AgentDisabled
        );
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let (status, revision, started_at, completed_at): (
            String,
            i64,
            Option<String>,
            Option<String>,
        ) = connection
            .query_row(
                "SELECT status, revision, started_at, completed_at FROM agent_runs WHERE id = ?1",
                [&created.run.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("التشغيل يجب أن يقرأ");
        assert_eq!(status, "QUEUED");
        assert_eq!(revision, 2);
        assert!(started_at.is_none());
        assert!(completed_at.is_none());
    }

    #[test]
    fn approval_audit_failure_rolls_back_decision_and_run_transition() {
        let state = state_with_approval_lifecycle();
        let created = create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: "agent-a".to_owned(),
            },
        )
        .expect("إنشاء AgentRun يجب أن ينجح");
        let request = request_agent_run_approval(&state, &created.run.id, requirement())
            .expect("طلب الموافقة يجب أن ينجح");
        state.set_active_user_for_tests("reviewer");
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_approval_grant_audit BEFORE INSERT ON audit_events
                 WHEN NEW.event_type = 'APPROVAL_GRANTED'
                 BEGIN
                   SELECT RAISE(ABORT, 'رفض اختبار تدقيق قرار الموافقة');
                 END;",
            )
            .expect("مشغل الاختبار يجب أن ينشأ");
        drop(connection);
        assert!(matches!(
            decide_approval(
                &state,
                DecideApprovalInput {
                    approval_id: request.approval_id.clone(),
                    decision: "APPROVED".to_owned(),
                    reason: "Audit failure must roll back every decision mutation.".to_owned(),
                },
            ),
            Err(AppError::StorageOperation)
        ));
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let (approval_decision, status, revision): (String, String, i64) = connection
            .query_row(
                "SELECT (SELECT decision FROM approvals WHERE id = ?1), (SELECT status FROM agent_runs WHERE id = ?2), (SELECT revision FROM agent_runs WHERE id = ?2)",
                params![&request.approval_id, &created.run.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("نتيجة التراجع يجب أن تقرأ");
        assert_eq!(approval_decision, "PENDING");
        assert_eq!(status, "WAITING_APPROVAL");
        assert_eq!(revision, 1);
    }
}
