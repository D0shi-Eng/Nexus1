use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    application::authorization_service,
    domain::{
        agent_execution_profile::{
            validate_profile_identity, AgentExecutionProfileVersion, MAX_PROFILE_INSTRUCTION_BYTES,
        },
        error::AppError,
    },
    infrastructure::{
        agent_execution_profile_repository::AgentExecutionProfileRepository,
        agent_repository::AgentRepository,
        database::{append_audit_event, latest_audit_event_hash, AppState},
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgentExecutionProfileInput {
    pub agent_id: String,
    pub instruction: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecutionProfileMutationResult {
    pub profile: AgentExecutionProfileVersion,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecutionProfilesData {
    pub has_data: bool,
    pub items: Vec<AgentExecutionProfileVersion>,
    pub latest_version_id: Option<String>,
}

pub fn list_agent_execution_profiles(
    state: &AppState,
    agent_id: String,
) -> Result<AgentExecutionProfilesData, AppError> {
    authorization_service::require_permission(state, "AGENT", "READ")?;
    validate_agent_id(&agent_id)?;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let items = AgentExecutionProfileRepository::list_for_agent(
        &connection,
        &state.tenant_id(),
        &agent_id,
    )?;
    let latest_version_id = items.first().map(|profile| profile.id.clone());
    Ok(AgentExecutionProfilesData {
        has_data: !items.is_empty(),
        items,
        latest_version_id,
    })
}

// إنشاء نسخة بروفايل جديدة: التعديل ينشئ نسخاً إضافية ولا يلمس التاريخ إطلاقاً —
// مشغلات SQL تمنع UPDATE/DELETE على agent_execution_profiles.
pub fn create_agent_execution_profile(
    state: &AppState,
    input: CreateAgentExecutionProfileInput,
) -> Result<AgentExecutionProfileMutationResult, AppError> {
    authorization_service::require_permission(state, "AGENT", "UPDATE")?;
    validate_agent_id(&input.agent_id)?;
    validate_profile_identity(&input.agent_id, 1, &input.instruction)?;

    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    // وجود الوكيل داخل المستأجر النشط شرط لإنشاء نسخة بروفايل — لا بروفايل يتيم.
    AgentRepository::find(&transaction, &state.tenant_id(), &input.agent_id)?
        .ok_or(AppError::ResourceNotFound)?;

    let next_version_number = AgentExecutionProfileRepository::latest_version_number(
        &transaction,
        &state.tenant_id(),
        &input.agent_id,
    )? + 1;
    let record = AgentExecutionProfileVersion {
        id: format!("agent_execution_profile_{}", Uuid::new_v4()),
        tenant_id: state.tenant_id(),
        agent_id: input.agent_id,
        version_number: next_version_number,
        instruction: input.instruction,
        created_at: now.clone(),
    };
    AgentExecutionProfileRepository::insert(&transaction, &record)?;
    append_profile_audit(
        &transaction,
        state,
        "AGENT_EXECUTION_PROFILE_CREATED",
        &record.id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(AgentExecutionProfileMutationResult { profile: record })
}

fn validate_agent_id(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

pub(crate) fn validate_task_content(task_content: &str) -> Result<(), AppError> {
    let trimmed = task_content.trim();
    if trimmed.is_empty() || trimmed.as_bytes().len() > MAX_PROFILE_INSTRUCTION_BYTES {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

fn append_profile_audit(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    event_type: &str,
    profile_id: &str,
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
        "AgentExecutionProfile",
        profile_id,
        occurred_at,
        &previous_hash,
    )
    .map(|_| ())
}
