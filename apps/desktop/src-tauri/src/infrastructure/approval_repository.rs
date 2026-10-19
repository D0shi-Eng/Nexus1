use rusqlite::{params, OptionalExtension, Transaction};

use crate::domain::error::AppError;

#[derive(Debug, Clone)]
pub struct ApprovalRecord {
    pub id: String,
    pub tenant_id: String,
    pub resource_type: String,
    pub resource_id: String,
    pub resource_version: i64,
    pub requested_by: String,
    pub required_role_code: String,
    pub approval_requirement_code: Option<String>,
    pub decision: String,
}

#[derive(Debug, Clone)]
pub struct AgentRunApprovalLink {
    pub approval_id: String,
    pub tenant_id: String,
    pub agent_run_id: String,
    pub resource_version: i64,
}

#[derive(Debug, Clone)]
pub struct ToolInvocationApprovalLink {
    pub approval_id: String,
    pub tenant_id: String,
    pub tool_invocation_id: String,
    pub resource_version: i64,
}

pub struct ApprovalRepository;

impl ApprovalRepository {
    pub fn insert_agent_run_request(
        transaction: &Transaction<'_>,
        approval: &ApprovalRecord,
        created_at: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO approvals (
                    id, tenant_id, resource_type, resource_id, resource_version, requested_by,
                    required_role_code, approval_requirement_code, decision, decision_by,
                    decision_reason, decided_at, created_at, updated_at
                 ) VALUES (?1, ?2, 'AGENT_RUN', ?3, ?4, ?5, ?6, ?7, 'PENDING', NULL, NULL, NULL, ?8, ?8)",
                params![
                    &approval.id,
                    &approval.tenant_id,
                    &approval.resource_id,
                    approval.resource_version,
                    &approval.requested_by,
                    &approval.required_role_code,
                    &approval.approval_requirement_code,
                    created_at,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn insert_agent_run_link(
        transaction: &Transaction<'_>,
        link: &AgentRunApprovalLink,
        created_at: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO agent_run_approval_links (
                    approval_id, tenant_id, agent_run_id, resource_version, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    &link.approval_id,
                    &link.tenant_id,
                    &link.agent_run_id,
                    link.resource_version,
                    created_at,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn find_by_id(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        approval_id: &str,
    ) -> Result<Option<ApprovalRecord>, AppError> {
        transaction
            .query_row(
                "SELECT id, tenant_id, resource_type, resource_id, resource_version, requested_by,
                        required_role_code, approval_requirement_code, decision
                 FROM approvals
                 WHERE id = ?1 AND tenant_id = ?2",
                params![approval_id, tenant_id],
                map_approval,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find_pending_agent_run_request(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        run_id: &str,
        revision: i64,
        requirement_code: &str,
    ) -> Result<Option<ApprovalRecord>, AppError> {
        transaction
            .query_row(
                "SELECT id, tenant_id, resource_type, resource_id, resource_version, requested_by,
                        required_role_code, approval_requirement_code, decision
                 FROM approvals
                 WHERE tenant_id = ?1
                   AND resource_type = 'AGENT_RUN'
                   AND resource_id = ?2
                   AND resource_version = ?3
                   AND approval_requirement_code = ?4
                   AND decision = 'PENDING'",
                params![tenant_id, run_id, revision, requirement_code],
                map_approval,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find_pending_agent_run_for_run(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        run_id: &str,
    ) -> Result<Option<ApprovalRecord>, AppError> {
        transaction
            .query_row(
                "SELECT id, tenant_id, resource_type, resource_id, resource_version, requested_by,
                        required_role_code, approval_requirement_code, decision
                 FROM approvals
                 WHERE tenant_id = ?1
                   AND resource_type = 'AGENT_RUN'
                   AND resource_id = ?2
                   AND decision = 'PENDING'",
                params![tenant_id, run_id],
                map_approval,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find_agent_run_link(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        approval_id: &str,
    ) -> Result<Option<AgentRunApprovalLink>, AppError> {
        transaction
            .query_row(
                "SELECT approval_id, tenant_id, agent_run_id, resource_version
                 FROM agent_run_approval_links
                 WHERE approval_id = ?1 AND tenant_id = ?2",
                params![approval_id, tenant_id],
                |row| {
                    Ok(AgentRunApprovalLink {
                        approval_id: row.get(0)?,
                        tenant_id: row.get(1)?,
                        agent_run_id: row.get(2)?,
                        resource_version: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    // ==== موافقات استدعاء الأدوات (MP03) — نفس جدول الموافقات بنوع مورد مخصص ====

    pub fn insert_tool_invocation_request(
        transaction: &Transaction<'_>,
        approval: &ApprovalRecord,
        created_at: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO approvals (
                    id, tenant_id, resource_type, resource_id, resource_version, requested_by,
                    required_role_code, approval_requirement_code, decision, decision_by,
                    decision_reason, decided_at, created_at, updated_at
                 ) VALUES (?1, ?2, 'TOOL_INVOCATION', ?3, ?4, ?5, ?6, ?7, 'PENDING', NULL, NULL, NULL, ?8, ?8)",
                params![
                    &approval.id,
                    &approval.tenant_id,
                    &approval.resource_id,
                    approval.resource_version,
                    &approval.requested_by,
                    &approval.required_role_code,
                    &approval.approval_requirement_code,
                    created_at,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    // الموافقة المرتبطة باستدعاء محدد — أساس رفع REQUIRE_APPROVAL إلى ALLOW
    // عند تطابق الهوية الدقيقة (نسخة + هاش مدخل).
    pub fn find_tool_invocation_approval(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        invocation_id: &str,
    ) -> Result<Option<ApprovalRecord>, AppError> {
        transaction
            .query_row(
                "SELECT id, tenant_id, resource_type, resource_id, resource_version, requested_by,
                        required_role_code, approval_requirement_code, decision
                 FROM approvals
                 WHERE tenant_id = ?1
                   AND resource_type = 'TOOL_INVOCATION'
                   AND resource_id = ?2
                   AND decision IN ('PENDING', 'APPROVED')
                 ORDER BY created_at DESC, id DESC
                 LIMIT 1",
                params![tenant_id, invocation_id],
                map_approval,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn insert_tool_invocation_link(
        transaction: &Transaction<'_>,
        link: &ToolInvocationApprovalLink,
        created_at: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO tool_invocation_approval_links (
                    approval_id, tenant_id, tool_invocation_id, resource_version, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    &link.approval_id,
                    &link.tenant_id,
                    &link.tool_invocation_id,
                    link.resource_version,
                    created_at,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn find_tool_invocation_link(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        approval_id: &str,
    ) -> Result<Option<ToolInvocationApprovalLink>, AppError> {
        transaction
            .query_row(
                "SELECT approval_id, tenant_id, tool_invocation_id, resource_version
                 FROM tool_invocation_approval_links
                 WHERE approval_id = ?1 AND tenant_id = ?2",
                params![approval_id, tenant_id],
                |row| {
                    Ok(ToolInvocationApprovalLink {
                        approval_id: row.get(0)?,
                        tenant_id: row.get(1)?,
                        tool_invocation_id: row.get(2)?,
                        resource_version: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    // موافقة استدعاء أدوات قائمة لتشغيل معين — يستخدمها إلغاء التشغيل المنتظر.
    pub fn find_pending_tool_invocation_for_run(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        run_id: &str,
    ) -> Result<Option<ApprovalRecord>, AppError> {
        transaction
            .query_row(
                "SELECT a.id, a.tenant_id, a.resource_type, a.resource_id, a.resource_version, a.requested_by,
                        a.required_role_code, a.approval_requirement_code, a.decision
                 FROM approvals a
                 JOIN tool_invocation_approval_links l ON l.approval_id = a.id AND l.tenant_id = a.tenant_id
                 JOIN tool_invocations i ON i.id = l.tool_invocation_id AND i.tenant_id = l.tenant_id
                 WHERE a.tenant_id = ?1
                   AND i.agent_run_id = ?2
                   AND a.decision = 'PENDING'
                 ORDER BY a.created_at DESC, a.id DESC
                 LIMIT 1",
                params![tenant_id, run_id],
                map_approval,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn decide_pending(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        approval_id: &str,
        decision: &str,
        reviewer_id: &str,
        reason: &str,
        decided_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE approvals
                 SET decision = ?1,
                     decision_by = ?2,
                     decision_reason = ?3,
                     decided_at = ?4,
                     updated_at = ?4
                 WHERE id = ?5
                   AND tenant_id = ?6
                   AND decision = 'PENDING'",
                params![
                    decision,
                    reviewer_id,
                    reason,
                    decided_at,
                    approval_id,
                    tenant_id,
                ],
            )
            .map_err(|_| AppError::StorageOperation)
            .map(|changed| changed == 1)
    }

    pub fn cancel_pending(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        approval_id: &str,
        requester_id: &str,
        cancelled_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE approvals
                 SET decision = 'CANCELLED',
                     decision_by = ?1,
                     decision_reason = 'REQUESTER_CANCELLED_AGENT_RUN',
                     decided_at = ?2,
                     updated_at = ?2
                 WHERE id = ?3
                   AND tenant_id = ?4
                   AND decision = 'PENDING'",
                params![requester_id, cancelled_at, approval_id, tenant_id],
            )
            .map_err(|_| AppError::StorageOperation)
            .map(|changed| changed == 1)
    }

    // ===== موافقات الخطط (MP04) — نفس نمط ربط تشغيلات/أدوات MP03 =====

    pub fn insert_plan_request(
        transaction: &Transaction<'_>,
        approval: &ApprovalRecord,
        now: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO approvals (
                    id, tenant_id, resource_type, resource_id, resource_version,
                    requested_by, required_role_code, approval_requirement_code,
                    decision, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
                params![
                    approval.id,
                    approval.tenant_id,
                    approval.resource_type,
                    approval.resource_id,
                    approval.resource_version,
                    approval.requested_by,
                    approval.required_role_code,
                    approval.approval_requirement_code,
                    approval.decision,
                    now,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        transaction
            .execute(
                "INSERT INTO plan_approval_links (
                    approval_id, tenant_id, plan_id, resource_version, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    approval.id,
                    approval.tenant_id,
                    approval.resource_id,
                    approval.resource_version,
                    now,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn find_pending_plan_for_plan(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        plan_id: &str,
    ) -> Result<Option<ApprovalRecord>, AppError> {
        transaction
            .query_row(
                "SELECT id, tenant_id, resource_type, resource_id, resource_version,
                        requested_by, required_role_code, approval_requirement_code, decision
                 FROM approvals
                 WHERE tenant_id = ?1
                   AND resource_type = 'PLAN'
                   AND resource_id = ?2
                   AND decision = 'PENDING'
                 ORDER BY created_at ASC
                 LIMIT 1",
                params![tenant_id, plan_id],
                map_approval,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    // إلغاء كل الموافقات المنتظرة لمورد محدد — يستخدمه إلغاء الخطة قبل التنفيذ
    pub fn cancel_pending_for_resource(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        resource_type: &str,
        resource_id: &str,
        requester_id: &str,
        cancelled_at: &str,
    ) -> Result<usize, AppError> {
        transaction
            .execute(
                "UPDATE approvals
                 SET decision = 'CANCELLED',
                     decision_by = ?1,
                     decision_reason = 'REQUESTER_CANCELLED',
                     decided_at = ?2,
                     updated_at = ?2
                 WHERE tenant_id = ?3
                   AND resource_type = ?4
                   AND resource_id = ?5
                   AND decision = 'PENDING'",
                params![
                    requester_id,
                    cancelled_at,
                    tenant_id,
                    resource_type,
                    resource_id
                ],
            )
            .map(|changed| changed)
            .map_err(|_| AppError::StorageOperation)
    }
}

fn map_approval(row: &rusqlite::Row<'_>) -> rusqlite::Result<ApprovalRecord> {
    Ok(ApprovalRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        resource_type: row.get(2)?,
        resource_id: row.get(3)?,
        resource_version: row.get(4)?,
        requested_by: row.get(5)?,
        required_role_code: row.get(6)?,
        approval_requirement_code: row.get(7)?,
        decision: row.get(8)?,
    })
}
