use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    application::{agent_run_approval_service, authorization_service},
    domain::error::AppError,
    infrastructure::{
        approval_repository::{ApprovalRecord, ApprovalRepository},
        database::{append_audit_event, latest_audit_event_hash, AppState},
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecideApprovalInput {
    pub approval_id: String,
    pub decision: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalDecisionResult {
    pub approval_id: String,
    pub decision: String,
    pub resource_id: String,
}

pub fn decide_approval(
    state: &AppState,
    input: DecideApprovalInput,
) -> Result<ApprovalDecisionResult, AppError> {
    authorization_service::require_permission(state, "APPROVAL", "DECIDE")?;
    validate_decision_input(&input)?;

    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let approval =
        ApprovalRepository::find_by_id(&transaction, &state.tenant_id(), &input.approval_id)?
            .ok_or(AppError::ResourceNotFound)?;

    if approval.decision != "PENDING" {
        return if approval.resource_type == "AGENT_RUN" {
            Err(AppError::StaleApproval)
        } else {
            Err(AppError::ResourceNotFound)
        };
    }
    if approval.requested_by == state.current_user_id()? {
        return Err(AppError::SelfApprovalForbidden);
    }
    authorization_service::require_role_in_transaction(
        &transaction,
        state,
        &approval.required_role_code,
    )?;

    let decided = ApprovalRepository::decide_pending(
        &transaction,
        &state.tenant_id(),
        &approval.id,
        &input.decision,
        &state.current_user_id()?,
        input.reason.trim(),
        &now,
    )?;
    if !decided {
        return Err(AppError::Conflict);
    }

    append_approval_audit(
        &transaction,
        state,
        if input.decision == "APPROVED" {
            "APPROVAL_GRANTED"
        } else {
            "APPROVAL_REJECTED"
        },
        &approval.id,
        &now,
    )?;

    if approval.resource_type == "AGENT_RUN" {
        let decided_approval = ApprovalRecord {
            decision: input.decision.clone(),
            ..approval.clone()
        };
        agent_run_approval_service::transition_after_approval_decision(
            &transaction,
            state,
            &decided_approval,
            &now,
        )?;
    } else if approval.resource_type == "PLAN" {
        // تفريع موافقات الخطط (MP04) — نفس نمط انتقال ما بعد القرار
        let decided_approval = ApprovalRecord {
            decision: input.decision.clone(),
            ..approval.clone()
        };
        crate::application::planner_service::transition_after_plan_decision(
            &transaction,
            state,
            &decided_approval,
            &now,
        )?;
    } else if approval.resource_type == "TOOL_INVOCATION" {
        // تفريع موافقات استدعاء الأدوات (MP03) — خدمة مركزة خارج ملف الاعتمادات
        // الخاص بتشغيلات الوكلاء (§53).
        let decided_approval = ApprovalRecord {
            decision: input.decision.clone(),
            ..approval.clone()
        };
        crate::application::tool_invocation_service::transition_after_tool_approval_decision(
            &transaction,
            state,
            &decided_approval,
            &now,
        )?;
    } else if approval.resource_type == "TASK" {
        // تفريع موافقات المهام (0025) — العيب الذي أُغلق هنا: كان القرار يُكتب
        // في جدول الموافقات فقط ولا تتحرك المهمة إطلاقاً، فتبقى OPEN بعد
        // الاعتماد وبعد الرفض معاً. الآن القرار ينقل الحالة داخل نفس المعاملة،
        // ويرفض العمل الذي تغيّر بعد طلب المراجعة (StaleApproval).
        // القرار الجديد يُمرَّر صراحةً: السجل المقروء ما زال PENDING.
        let decided_approval = ApprovalRecord {
            decision: input.decision.clone(),
            ..approval.clone()
        };
        crate::application::task_service::transition_after_task_approval_decision(
            &transaction,
            state,
            &decided_approval,
            input.reason.trim(),
            &now,
        )?;
    }

    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(ApprovalDecisionResult {
        approval_id: input.approval_id,
        decision: input.decision,
        resource_id: approval.resource_id,
    })
}

fn validate_decision_input(input: &DecideApprovalInput) -> Result<(), AppError> {
    if !matches!(input.decision.as_str(), "APPROVED" | "REJECTED")
        || input.reason.trim().is_empty()
        || input.reason.chars().count() > 1_000
    {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

fn append_approval_audit(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    event_type: &str,
    approval_id: &str,
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
        "Approval",
        approval_id,
        occurred_at,
        &previous_hash,
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {

    use chrono::Utc;
    use rusqlite::{params, Connection};

    use super::{decide_approval, DecideApprovalInput};
    use crate::{domain::error::AppError, infrastructure::database::AppState};

    fn state_with_pending_approval(actor_id: &str) -> AppState {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        for migration in [
            include_str!("../../migrations/0001_initial.sql"),
            include_str!("../../migrations/0002_identity_and_access.sql"),
            include_str!("../../migrations/0003_workflow_and_risk.sql"),
            include_str!("../../migrations/0010_agent_foundation.sql"),
            include_str!("../../migrations/0011_agent_runs.sql"),
            include_str!("../../migrations/0012_agent_run_approval_lifecycle.sql"),
            include_str!("../../migrations/0025_task_lifecycle.sql"),
            include_str!("../../migrations/0026_production_user_and_authoring.sql"),
        ] {
            connection
                .execute_batch(migration)
                .expect("هجرة الاختبار يجب أن تنجح");
        }
        let now = Utc::now().to_rfc3339();
        for tenant_id in ["tenant-approval-a", "tenant-approval-b"] {
            connection.execute("INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?2, 'DEMO_SYNTHETIC', ?3, ?3)", (tenant_id, tenant_id, &now)).expect("المستأجر يجب أن ينشأ");
        }
        for user_id in ["user-requester", "user-reviewer", "user-wrong-role"] {
            connection.execute("INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, 'tenant-approval-a', ?1, ?2, NULL, 'ACTIVE', ?3, ?3)", params![user_id, format!("{user_id}@test.local"), &now]).expect("المستخدم يجب أن ينشأ");
            connection.execute("INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, 'tenant-approval-a', ?2, ?3, '2030-01-01T00:00:00Z', NULL, 'test')", params![format!("session-{user_id}"), user_id, &now]).expect("الجلسة يجب أن تنشأ");
        }
        connection.execute("INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-admin', 'tenant-approval-a', 'TENANT_ADMIN', 'Tenant Admin', ?1, ?1)", [&now]).expect("الدور المطلوب يجب أن ينشأ");
        connection.execute("INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-reader', 'tenant-approval-a', 'READER', 'Reader', ?1, ?1)", [&now]).expect("الدور البديل يجب أن ينشأ");
        connection.execute("INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES ('permission-approval', 'approval:decide', 'APPROVAL', 'DECIDE', 'HIGHLY_RESTRICTED', ?1)", [&now]).expect("الصلاحية يجب أن تنشأ");
        for user_id in ["user-requester", "user-reviewer"] {
            connection.execute("INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, 'role-admin', ?2, 'system')", params![user_id, &now]).expect("تعيين الدور يجب أن ينجح");
        }
        connection.execute("INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user-wrong-role', 'role-reader', ?1, 'system')", [&now]).expect("تعيين الدور البديل يجب أن ينجح");
        connection.execute("INSERT INTO role_permissions (role_id, permission_id) VALUES ('role-admin', 'permission-approval')", []).expect("منح الصلاحية يجب أن ينجح");
        connection.execute("INSERT INTO role_permissions (role_id, permission_id) VALUES ('role-reader', 'permission-approval')", []).expect("منح الصلاحية البديلة يجب أن ينجح");
        // 0025: قرار موافقة المهمة صار يحرّك المهمة فعلاً، فلا بد من وجود مهمة
        // حقيقية في حالة READY_FOR_REVIEW وبإصدار مطابق لإصدار الموافقة — وإلا
        // فالقرار على عمل غير موجود يجب أن يُرفض (وهذا ما يختبره ResourceNotFound
        // في اختبار العزل). الالتزام الأب مطلوب لضابط العزل بين المستأجرين.
        for (source_id, obligation_id, task_id, tenant_id, user_id) in [
            (
                "source-approval-a",
                "obligation-approval-a",
                "task-a",
                "tenant-approval-a",
                "user-requester",
            ),
            (
                "source-approval-b",
                "obligation-approval-b",
                "task-b",
                "tenant-approval-b",
                "user-requester",
            ),
        ] {
            connection.execute("INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES (?1, ?2, 'Synthetic', 'Synthetic source', NULL, '1', ?3, 0, 'UNVERIFIED', ?3, ?3)", (source_id, tenant_id, &now)).expect("المصدر يجب أن ينشأ");
            connection.execute("INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, created_at, updated_at) VALUES (?1, ?2, ?3, 'Synthetic obligation', 'Owner', NULL, 'HIGH', 'OPEN', 'APPLIES', 'VALID', 'UNVERIFIED', ?4, ?4)", (obligation_id, tenant_id, source_id, &now)).expect("الالتزام يجب أن ينشأ");
            connection.execute("INSERT INTO tasks (id, tenant_id, obligation_id, title, description, assignee_user_id, due_at, priority, status, version, completed_at, created_at, updated_at) VALUES (?1, ?2, ?3, 'Synthetic task', 'Submitted for review', ?4, NULL, 'HIGH', 'READY_FOR_REVIEW', 1, NULL, ?5, ?5)", (task_id, tenant_id, obligation_id, user_id, &now)).expect("المهمة يجب أن تنشأ");
        }
        connection.execute("INSERT INTO approvals (id, tenant_id, resource_type, resource_id, resource_version, requested_by, required_role_code, decision, decision_by, decision_reason, decided_at, created_at, updated_at) VALUES ('approval-a', 'tenant-approval-a', 'TASK', 'task-a', 1, 'user-requester', 'TENANT_ADMIN', 'PENDING', NULL, NULL, NULL, ?1, ?1)", [&now]).expect("الموافقة يجب أن تنشأ");
        connection.execute("INSERT INTO approvals (id, tenant_id, resource_type, resource_id, resource_version, requested_by, required_role_code, decision, decision_by, decision_reason, decided_at, created_at, updated_at) VALUES ('approval-b', 'tenant-approval-b', 'TASK', 'task-b', 1, 'user-requester', 'TENANT_ADMIN', 'PENDING', NULL, NULL, NULL, ?1, ?1)", [&now]).expect("موافقة المستأجر الآخر يجب أن تنشأ");

        AppState::test_instance(
            connection,
            "tenant-approval-a".to_owned(),
            actor_id.to_owned(),
        )
    }

    #[test]
    fn decides_pending_approval_and_writes_audit_event() {
        let state = state_with_pending_approval("user-reviewer");
        let decision = decide_approval(
            &state,
            DecideApprovalInput {
                approval_id: "approval-a".to_owned(),
                decision: "APPROVED".to_owned(),
                reason: "Human reviewer accepted the remediation plan.".to_owned(),
            },
        )
        .expect("القرار المصرح به يجب أن ينجح");

        assert_eq!(decision.decision, "APPROVED");
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let stored_decision: String = connection
            .query_row(
                "SELECT decision FROM approvals WHERE id = 'approval-a'",
                [],
                |row| row.get(0),
            )
            .expect("استعلام القرار يجب أن ينجح");
        let audit_count: i32 = connection.query_row("SELECT COUNT(*) FROM audit_events WHERE resource_id = 'approval-a' AND event_type = 'APPROVAL_GRANTED'", [], |row| row.get(0)).expect("استعلام التدقيق يجب أن ينجح");
        assert_eq!(stored_decision, "APPROVED");
        assert_eq!(audit_count, 1);
    }

    #[test]
    fn rejects_self_approval_and_role_mismatch() {
        let self_state = state_with_pending_approval("user-requester");
        assert!(matches!(
            decide_approval(
                &self_state,
                DecideApprovalInput {
                    approval_id: "approval-a".to_owned(),
                    decision: "APPROVED".to_owned(),
                    reason: "Requester must not decide this request.".to_owned(),
                },
            ),
            Err(AppError::SelfApprovalForbidden)
        ));

        let wrong_role_state = state_with_pending_approval("user-wrong-role");
        assert!(matches!(
            decide_approval(
                &wrong_role_state,
                DecideApprovalInput {
                    approval_id: "approval-a".to_owned(),
                    decision: "APPROVED".to_owned(),
                    reason: "Role mismatch must fail safely.".to_owned(),
                },
            ),
            Err(AppError::AuthorizationDenied)
        ));
    }

    #[test]
    fn cannot_decide_another_tenants_approval() {
        let state = state_with_pending_approval("user-reviewer");
        let result = decide_approval(
            &state,
            DecideApprovalInput {
                approval_id: "approval-b".to_owned(),
                decision: "APPROVED".to_owned(),
                reason: "Cross tenant decision must fail.".to_owned(),
            },
        );
        assert!(matches!(result, Err(AppError::ResourceNotFound)));
    }
}
