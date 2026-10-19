use chrono::Utc;
use uuid::Uuid;

use crate::{
    application::authorization_service,
    domain::{
        agent_executor::AgentExecutor,
        error::AppError,
        execution_input::ExecutionInput,
        execution_result::{
            ExecutionFailureCode, ExecutionResult, ExecutionResultOutcome, ExecutionResultRecord,
        },
        models::{is_valid_agent_run_transition, AgentRunRecord, AgentRunStatus, AgentStatus},
        trusted_skill_selection::TrustedSkillSelection,
    },
    infrastructure::{
        agent_repository::AgentRepository,
        agent_run_repository::AgentRunRepository,
        agent_run_skill_binding_repository::AgentRunSkillBindingRepository,
        database::{append_audit_event, latest_audit_event_hash, AppState},
        execution_result_repository::ExecutionResultRepository,
    },
};

pub struct GovernedAgentExecutorService;

impl GovernedAgentExecutorService {
    pub(crate) fn execute_trusted(
        state: &AppState,
        run_id: &str,
        input: ExecutionInput,
        executor: &dyn AgentExecutor,
    ) -> Result<ExecutionResultRecord, AppError> {
        authorization_service::require_permission(state, "AGENT", "EXECUTE")?;
        validate_identifier(run_id)?;

        let running_run = start_execution(state, run_id)?;
        let result = match executor.execute(input) {
            Ok(result) => result,
            Err(_) => ExecutionResult::failed(
                executor.provider_id(),
                ExecutionFailureCode::ExecutionInfrastructureError,
            ),
        };

        finalize_execution(state, &running_run, result)
    }

    pub(crate) fn execute_trusted_with_skill_selection(
        state: &AppState,
        run_id: &str,
        input: ExecutionInput,
        selection: Option<TrustedSkillSelection>,
        executor: &dyn AgentExecutor,
    ) -> Result<ExecutionResultRecord, AppError> {
        authorization_service::require_permission(state, "AGENT", "EXECUTE")?;
        validate_identifier(run_id)?;
        let running_run = start_execution(state, run_id)?;
        if let Some(selection) = selection {
            bind_skill_selection(state, &running_run, &selection)?;
        }
        let result = match executor.execute(input) {
            Ok(result) => result,
            Err(_) => ExecutionResult::failed(
                executor.provider_id(),
                ExecutionFailureCode::ExecutionInfrastructureError,
            ),
        };
        finalize_execution(state, &running_run, result)
    }
}

fn bind_skill_selection(
    state: &AppState,
    run: &AgentRunRecord,
    selection: &TrustedSkillSelection,
) -> Result<(), AppError> {
    if selection.tenant_id() != state.tenant_id().as_str() || selection.agent_id() != run.agent_id {
        return Err(AppError::AuthorizationDenied);
    }
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let bound = AgentRunSkillBindingRepository::find(&transaction, &state.tenant_id(), &run.id)?
        .ok_or(AppError::StaleApproval)?;
    let bound_revision =
        AgentRunSkillBindingRepository::find_revision(&transaction, &state.tenant_id(), &run.id)?
            .ok_or(AppError::StaleApproval)?;
    if bound_revision != run.revision || bound != *selection {
        return Err(AppError::StaleApproval);
    }
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

pub(crate) fn start_execution(state: &AppState, run_id: &str) -> Result<AgentRunRecord, AppError> {
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
    if run.status != AgentRunStatus::Queued
        || !is_valid_agent_run_transition(&run.status, &AgentRunStatus::Running)
    {
        return Err(AppError::InvalidStateTransition);
    }

    let changed = AgentRunRepository::transition_to_running(
        &transaction,
        &state.tenant_id(),
        &run.id,
        run.revision,
        &now,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    append_execution_audit(
        &transaction,
        state,
        "AGENT_RUN_EXECUTION_STARTED",
        &run.id,
        &now,
    )?;
    let running_run =
        AgentRunRepository::find_in_transaction(&transaction, &state.tenant_id(), &run.id)?
            .ok_or(AppError::ResourceNotFound)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(running_run)
}

pub(crate) fn finalize_execution(
    state: &AppState,
    running_run: &AgentRunRecord,
    result: ExecutionResult,
) -> Result<ExecutionResultRecord, AppError> {
    finalize_execution_with_intelligence(state, running_run, result, None, "AI_INTERPRETATION")
}

// إنهاء MP04: نفس الضمانات (CAS + محصونية + تدقيق) مع تخزين الاقتباسات
// المتحققة وtruth_class المطبوع — الافتراضي AI_INTERPRETATION لا يرقى ذاتياً (C-8).
pub(crate) fn finalize_execution_with_intelligence(
    state: &AppState,
    running_run: &AgentRunRecord,
    result: ExecutionResult,
    citations_json: Option<&str>,
    truth_class: &str,
) -> Result<ExecutionResultRecord, AppError> {
    if !matches!(truth_class, "AI_INTERPRETATION" | "UNKNOWN" | "CONFLICTING") {
        return Err(AppError::ValidationFailed);
    }
    let now = Utc::now().to_rfc3339();
    let terminal_status = match result.outcome() {
        ExecutionResultOutcome::Completed => AgentRunStatus::Completed,
        ExecutionResultOutcome::Failed => AgentRunStatus::Failed,
    };
    let audit_event = match result.outcome() {
        ExecutionResultOutcome::Completed => "AGENT_RUN_EXECUTION_COMPLETED",
        ExecutionResultOutcome::Failed => "AGENT_RUN_EXECUTION_FAILED",
    };
    let record = ExecutionResultRecord {
        id: format!("execution_result_{}", Uuid::new_v4()),
        tenant_id: state.tenant_id(),
        agent_run_id: running_run.id.clone(),
        agent_run_revision: running_run.revision,
        result,
        created_at: now.clone(),
    };

    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let current =
        AgentRunRepository::find_in_transaction(&transaction, &state.tenant_id(), &running_run.id)?
            .ok_or(AppError::ResourceNotFound)?;
    if current.status != AgentRunStatus::Running || current.revision != running_run.revision {
        return Err(AppError::Conflict);
    }
    if ExecutionResultRepository::find_in_transaction(
        &transaction,
        &state.tenant_id(),
        &current.id,
    )?
    .is_some()
    {
        return Err(AppError::Conflict);
    }
    cancel_orphan_tool_invocations(&transaction, state, &current.id, &now)?;
    if !is_valid_agent_run_transition(&current.status, &terminal_status) {
        return Err(AppError::InvalidStateTransition);
    }

    ExecutionResultRepository::insert_with_intelligence(
        &transaction,
        &record,
        citations_json,
        truth_class,
    )?;
    let changed = AgentRunRepository::transition(
        &transaction,
        &state.tenant_id(),
        &current.id,
        &current.status,
        current.revision,
        &terminal_status,
        current.revision + 1,
        Some(&now),
        &now,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    append_execution_audit(&transaction, state, audit_event, &current.id, &now)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    // تحرير قفل الاتصال قبل أي نداء لاحق يقفل بدوره (نمط MP03 §54)
    drop(connection);

    // ذاكرة حلقيّة محدودة: حدث إنهاء واحد لكل تشغيل منجز — معرفات لا نصوص (C-7)
    if record.result.outcome() == ExecutionResultOutcome::Completed {
        let _ = crate::application::memory_service::record_episodic_run_event(
            state,
            &record.agent_run_id,
            &format!("Agent run completed: {}", record.agent_run_id),
            "AGENT_RUN",
        );
    }

    Ok(record)
}

// إنهاء أي استدعاءات أدوات غير منتهية مرتبطة بتشغيلٍ يُنهى — لا يبقى
// استدعاء فعالاً بعد حالة تشغيل نهائية (§77).
fn cancel_orphan_tool_invocations(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    run_id: &str,
    occurred_at: &str,
) -> Result<(), AppError> {
    use crate::domain::tool_invocation::ToolInvocationStatus;
    use crate::infrastructure::tool_invocation_repository::ToolInvocationRepository;

    let orphans = ToolInvocationRepository::list_non_terminal_for_run(
        transaction,
        &state.tenant_id(),
        run_id,
    )?;
    for invocation in orphans {
        let changed = ToolInvocationRepository::transition(
            transaction,
            &state.tenant_id(),
            &invocation.id,
            invocation.status,
            invocation.revision,
            ToolInvocationStatus::Cancelled,
            invocation.revision + 1,
            None,
            Some(occurred_at),
            None,
            occurred_at,
        )?;
        if !changed {
            return Err(AppError::Conflict);
        }
        let audit_id = format!("audit_{}", Uuid::new_v4());
        let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
        append_audit_event(
            transaction,
            &state.tenant_id(),
            &audit_id,
            "TOOL_INVOCATION_CANCELLED",
            &state.current_user_id()?,
            "ToolInvocation",
            &invocation.id,
            occurred_at,
            &previous_hash,
        )?;
    }
    Ok(())
}

fn append_execution_audit(
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

fn validate_identifier(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

#[cfg(test)]
#[path = "governed_agent_executor_service_tests.rs"]
mod tests;
