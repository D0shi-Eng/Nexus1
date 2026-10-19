use serde::Serialize;
use uuid::Uuid;

use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        models::{is_valid_agent_run_transition, AgentRunRecord, AgentRunStatus},
        tool_invocation::{ToolInvocationRecord, ToolInvocationStatus},
    },
    infrastructure::{
        agent_run_repository::AgentRunRepository,
        approval_repository::{ApprovalRecord, ApprovalRepository},
        database::{append_audit_event, latest_audit_event_hash, AppState},
        tool_invocation_repository::ToolInvocationRepository,
    },
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInvocationsData {
    pub has_data: bool,
    pub items: Vec<ToolInvocationRecord>,
}

// قراءة حالة استدعاءات أدوات التشغيل — عرض فقط، التنفيذ خلفي حصراً.
pub fn list_tool_invocations_for_run(
    state: &AppState,
    run_id: String,
) -> Result<ToolInvocationsData, AppError> {
    authorization_service::require_permission(state, "AGENT", "READ")?;
    if run_id.trim().is_empty() || run_id.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let items = ToolInvocationRepository::list_for_run(&connection, &state.tenant_id(), &run_id)?;
    Ok(ToolInvocationsData {
        has_data: !items.is_empty(),
        items,
    })
}

// تفريع قرار الموافقة لنوع TOOL_INVOCATION (يُستدعى من approval_service):
// القبول يعيد التشغيل إلى QUEUED ليعيد الطالب البدء (إعادة البروتوكول المحدود
// مع احترام الهوية المعتمدة)، والرفض يلغي الاستدعاء والتشغيل معاً.
pub fn transition_after_tool_approval_decision(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    approval: &ApprovalRecord,
    decided_at: &str,
) -> Result<(), AppError> {
    let link = ApprovalRepository::find_tool_invocation_link(
        transaction,
        &state.tenant_id(),
        &approval.id,
    )?
    .ok_or(AppError::StaleApproval)?;
    let invocation = ToolInvocationRepository::find_in_transaction(
        transaction,
        &state.tenant_id(),
        &link.tool_invocation_id,
    )?
    .ok_or(AppError::StaleApproval)?;
    if approval.resource_version != invocation.revision
        || invocation.status != ToolInvocationStatus::WaitingApproval
    {
        return Err(AppError::StaleApproval);
    }
    let run = AgentRunRepository::find_in_transaction(
        transaction,
        &state.tenant_id(),
        &invocation.agent_run_id,
    )?
    .ok_or(AppError::StaleApproval)?;
    if run.status != AgentRunStatus::WaitingApproval {
        return Err(AppError::StaleApproval);
    }

    match approval.decision.as_str() {
        "APPROVED" => {
            if !is_valid_agent_run_transition(&run.status, &AgentRunStatus::Queued) {
                return Err(AppError::InvalidStateTransition);
            }
            let changed = AgentRunRepository::transition(
                transaction,
                &state.tenant_id(),
                &run.id,
                &run.status,
                run.revision,
                &AgentRunStatus::Queued,
                run.revision + 1,
                None,
                decided_at,
            )?;
            if !changed {
                return Err(AppError::Conflict);
            }
            append_run_audit(
                transaction,
                state,
                "AGENT_RUN_REQUEUED_AFTER_APPROVAL",
                &run.id,
                decided_at,
            )?;
        }
        "REJECTED" => {
            let changed = ToolInvocationRepository::transition(
                transaction,
                &state.tenant_id(),
                &invocation.id,
                ToolInvocationStatus::WaitingApproval,
                invocation.revision,
                ToolInvocationStatus::Cancelled,
                invocation.revision + 1,
                None,
                Some(decided_at),
                None,
                decided_at,
            )?;
            if !changed {
                return Err(AppError::Conflict);
            }
            append_invocation_audit(
                transaction,
                state,
                "TOOL_INVOCATION_CANCELLED",
                &invocation.id,
                decided_at,
            )?;
            if !is_valid_agent_run_transition(&run.status, &AgentRunStatus::Cancelled) {
                return Err(AppError::InvalidStateTransition);
            }
            let changed = AgentRunRepository::transition(
                transaction,
                &state.tenant_id(),
                &run.id,
                &run.status,
                run.revision,
                &AgentRunStatus::Cancelled,
                run.revision + 1,
                Some(decided_at),
                decided_at,
            )?;
            if !changed {
                return Err(AppError::Conflict);
            }
            append_run_audit(
                transaction,
                state,
                "AGENT_RUN_CANCELLED",
                &run.id,
                decided_at,
            )?;
        }
        _ => return Err(AppError::ValidationFailed),
    }
    Ok(())
}

// إلغاء موافقة استدعاء أدوات منتظرة عند إلغاء صاحب الطلب للتشغيل —
// يُستدعى من cancel_waiting_agent_run عندما لا توجد موافقة تشغيل وكلاء.
pub fn cancel_pending_tool_invocation_for_run(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    run: &AgentRunRecord,
    cancelled_at: &str,
) -> Result<bool, AppError> {
    let Some(approval) = ApprovalRepository::find_pending_tool_invocation_for_run(
        transaction,
        &state.tenant_id(),
        &run.id,
    )?
    else {
        return Ok(false);
    };
    let link = ApprovalRepository::find_tool_invocation_link(
        transaction,
        &state.tenant_id(),
        &approval.id,
    )?
    .ok_or(AppError::StaleApproval)?;
    let invocation = ToolInvocationRepository::find_in_transaction(
        transaction,
        &state.tenant_id(),
        &link.tool_invocation_id,
    )?
    .ok_or(AppError::StaleApproval)?;
    if approval.resource_version != invocation.revision
        || invocation.status != ToolInvocationStatus::WaitingApproval
    {
        return Err(AppError::StaleApproval);
    }

    let cancelled = ApprovalRepository::cancel_pending(
        transaction,
        &state.tenant_id(),
        &approval.id,
        &state.current_user_id()?,
        cancelled_at,
    )?;
    if !cancelled {
        return Err(AppError::Conflict);
    }
    let changed = ToolInvocationRepository::transition(
        transaction,
        &state.tenant_id(),
        &invocation.id,
        ToolInvocationStatus::WaitingApproval,
        invocation.revision,
        ToolInvocationStatus::Cancelled,
        invocation.revision + 1,
        None,
        Some(cancelled_at),
        None,
        cancelled_at,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    append_invocation_audit(
        transaction,
        state,
        "APPROVAL_CANCELLED",
        &approval.id,
        cancelled_at,
    )?;
    append_invocation_audit(
        transaction,
        state,
        "TOOL_INVOCATION_CANCELLED",
        &invocation.id,
        cancelled_at,
    )?;
    Ok(true)
}

fn append_run_audit(
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
    )
    .map(|_| ())
}

fn append_invocation_audit(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    event_type: &str,
    invocation_id: &str,
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
        "ToolInvocation",
        invocation_id,
        occurred_at,
        &previous_hash,
    )
    .map(|_| ())
}
