use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        tool::{
            validate_tool_identity, validate_tool_version, ToolContract, ToolEffectProfile,
            ToolRecord, ToolStatus, ToolVersionRecord,
        },
    },
    infrastructure::{
        database::{append_audit_event, latest_audit_event_hash, AppState},
        tool_repository::ToolRepository,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateToolInput {
    pub name: String,
    pub description: String,
    pub input_contract: ToolContract,
    pub output_contract: ToolContract,
    pub effect: ToolEffectProfile,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateToolVersionInput {
    pub tool_id: String,
    pub input_contract: ToolContract,
    pub output_contract: ToolContract,
    pub effect: ToolEffectProfile,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolIdInput {
    pub tool_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolMutationResult {
    pub tool: ToolRecord,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolVersionMutationResult {
    pub version: ToolVersionRecord,
}

// سطح القراءة الأدنى تستهلكه اختبارات العزل/الصلاحيات؛ سطح الواجهة هو الكتالوج.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsData {
    pub has_data: bool,
    pub items: Vec<ToolRecord>,
}

#[cfg_attr(not(test), allow(dead_code))]
// قائمة بسيطة فوق الكتالوج نفسه — مصدر حقيقة واحد لقراءة الأدوات.
pub fn list_tools(state: &AppState) -> Result<ToolsData, AppError> {
    let catalog = get_tool_catalog(state)?;
    Ok(ToolsData {
        has_data: catalog.has_data,
        items: catalog.items.into_iter().map(|item| item.tool).collect(),
    })
}

pub fn create_tool(
    state: &AppState,
    input: CreateToolInput,
) -> Result<ToolMutationResult, AppError> {
    authorization_service::require_permission(state, "TOOL", "CREATE")?;
    validate_tool_identity(&input.name, &input.description)?;
    validate_tool_version(
        1,
        &input.input_contract,
        &input.output_contract,
        &input.effect,
    )?;
    let now = Utc::now().to_rfc3339();
    let tool = ToolRecord {
        id: format!("tool_{}", Uuid::new_v4()),
        tenant_id: state.tenant_id(),
        name: input.name.trim().to_owned(),
        description: input.description.trim().to_owned(),
        status: ToolStatus::Active,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let version = ToolVersionRecord {
        id: format!("tool_version_{}", Uuid::new_v4()),
        tenant_id: state.tenant_id(),
        tool_id: tool.id.clone(),
        tool_status: ToolStatus::Active,
        version_number: 1,
        input_contract: input.input_contract,
        output_contract: input.output_contract,
        effect: input.effect,
        created_at: now.clone(),
    };
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    ToolRepository::insert(&transaction, &tool)?;
    ToolRepository::insert_version(&transaction, &version)?;
    append_tool_audit(&transaction, state, "TOOL_CREATED", &tool.id, &now)?;
    append_tool_audit(
        &transaction,
        state,
        "TOOL_VERSION_CREATED",
        &version.id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    Ok(ToolMutationResult { tool })
}

pub fn create_tool_version(
    state: &AppState,
    input: CreateToolVersionInput,
) -> Result<ToolVersionMutationResult, AppError> {
    authorization_service::require_permission(state, "TOOL", "UPDATE")?;
    validate_identifier(&input.tool_id)?;
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let tool = ToolRepository::find(&transaction, &state.tenant_id(), &input.tool_id)?
        .ok_or(AppError::ResourceNotFound)?;
    if tool.status == ToolStatus::Disabled {
        return Err(AppError::InvalidStateTransition);
    }
    let version_number =
        ToolRepository::latest_version_number(&transaction, &state.tenant_id(), &input.tool_id)?
            + 1;
    validate_tool_version(
        version_number,
        &input.input_contract,
        &input.output_contract,
        &input.effect,
    )?;
    let now = Utc::now().to_rfc3339();
    let version = ToolVersionRecord {
        id: format!("tool_version_{}", Uuid::new_v4()),
        tenant_id: state.tenant_id(),
        tool_id: input.tool_id,
        tool_status: tool.status,
        version_number,
        input_contract: input.input_contract,
        output_contract: input.output_contract,
        effect: input.effect,
        created_at: now.clone(),
    };
    ToolRepository::insert_version(&transaction, &version)?;
    append_tool_audit(
        &transaction,
        state,
        "TOOL_VERSION_CREATED",
        &version.id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    Ok(ToolVersionMutationResult { version })
}

pub fn disable_tool(state: &AppState, input: ToolIdInput) -> Result<ToolMutationResult, AppError> {
    authorization_service::require_permission(state, "TOOL", "UPDATE")?;
    validate_identifier(&input.tool_id)?;
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let existing = ToolRepository::find(&transaction, &state.tenant_id(), &input.tool_id)?
        .ok_or(AppError::ResourceNotFound)?;
    if existing.status == ToolStatus::Disabled {
        return Err(AppError::InvalidStateTransition);
    }
    if !ToolRepository::set_status(
        &transaction,
        &state.tenant_id(),
        &input.tool_id,
        ToolStatus::Disabled,
        &now,
    )? {
        return Err(AppError::Conflict);
    }
    let tool = ToolRepository::find(&transaction, &state.tenant_id(), &input.tool_id)?
        .ok_or(AppError::ResourceNotFound)?;
    append_tool_audit(&transaction, state, "TOOL_DISABLED", &tool.id, &now)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    Ok(ToolMutationResult { tool })
}

fn validate_identifier(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

// ==== أهلية وكيل ↔ نسخة أداة دقيقة (MP03) — TOOL/ASSIGN منفصلة عن القراءة ====

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignToolInput {
    pub agent_id: String,
    pub tool_version_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAssignmentIdInput {
    pub assignment_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolAssignmentView {
    pub id: String,
    pub agent_id: String,
    pub tool_id: String,
    pub tool_name: String,
    pub tool_version_id: String,
    pub version_number: i64,
    pub assigned_at: String,
    pub unassigned_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolAssignmentsData {
    pub has_data: bool,
    pub items: Vec<AgentToolAssignmentView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolVersionView {
    pub id: String,
    pub version_number: i64,
    pub effect: ToolEffectProfile,
    // الصدق المنتجي (§29/§70): تسجيل أداة لا يعني قابلية التنفيذ —
    // القابلية تثبت بربط محوّل موثوق معروف بالكود الخلفي فقط.
    pub executable: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCatalogItem {
    pub tool: ToolRecord,
    pub versions: Vec<ToolVersionView>,
    pub assigned_agents: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCatalogData {
    pub has_data: bool,
    pub items: Vec<ToolCatalogItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAssignmentMutationResult {
    pub assignment: AgentToolAssignmentView,
}

pub fn assign_tool(
    state: &AppState,
    input: AssignToolInput,
) -> Result<ToolAssignmentMutationResult, AppError> {
    authorization_service::require_permission(state, "TOOL", "ASSIGN")?;
    validate_identifier(&input.agent_id)?;
    validate_identifier(&input.tool_version_id)?;
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    // الوكيل يجب أن يكون موجوداً ونشطاً داخل المستأجر (نمط تعيين المهارات).
    let agent = crate::infrastructure::agent_repository::AgentRepository::find(
        &transaction,
        &state.tenant_id(),
        &input.agent_id,
    )?
    .ok_or(AppError::ResourceNotFound)?;
    if agent.status != crate::domain::models::AgentStatus::Active {
        return Err(AppError::AgentDisabled);
    }
    let version =
        ToolRepository::find_version(&transaction, &state.tenant_id(), &input.tool_version_id)?
            .ok_or(AppError::ResourceNotFound)?;
    if version.tool_status == ToolStatus::Disabled {
        return Err(AppError::InvalidStateTransition);
    }
    if crate::infrastructure::agent_tool_assignment_repository::AgentToolAssignmentRepository::find_active(
        &transaction,
        &state.tenant_id(),
        &input.agent_id,
        &input.tool_version_id,
    )?
    .is_some()
    {
        return Err(AppError::Conflict);
    }
    let record =
        crate::infrastructure::agent_tool_assignment_repository::AgentToolAssignmentRecord {
            id: format!("agent_tool_{}", Uuid::new_v4()),
            tenant_id: state.tenant_id(),
            agent_id: input.agent_id.clone(),
            tool_version_id: input.tool_version_id.clone(),
            assigned_at: now.clone(),
            unassigned_at: None,
        };
    crate::infrastructure::agent_tool_assignment_repository::AgentToolAssignmentRepository::insert(
        &transaction,
        &record,
    )?;
    append_tool_audit(&transaction, state, "AGENT_TOOL_ASSIGNED", &record.id, &now)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    Ok(ToolAssignmentMutationResult {
        assignment: AgentToolAssignmentView {
            id: record.id,
            agent_id: record.agent_id,
            tool_id: version.tool_id.clone(),
            tool_name: String::new(),
            tool_version_id: record.tool_version_id,
            version_number: version.version_number,
            assigned_at: record.assigned_at,
            unassigned_at: None,
        },
    })
}

pub fn unassign_tool(state: &AppState, input: ToolAssignmentIdInput) -> Result<(), AppError> {
    authorization_service::require_permission(state, "TOOL", "ASSIGN")?;
    validate_identifier(&input.assignment_id)?;
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let changed =
        crate::infrastructure::agent_tool_assignment_repository::AgentToolAssignmentRepository::unassign(
            &transaction,
            &state.tenant_id(),
            &input.assignment_id,
            &now,
        )?;
    if !changed {
        return Err(AppError::ResourceNotFound);
    }
    append_tool_audit(
        &transaction,
        state,
        "AGENT_TOOL_UNASSIGNED",
        &input.assignment_id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    Ok(())
}

pub fn list_agent_tool_assignments(
    state: &AppState,
    agent_id: String,
) -> Result<AgentToolAssignmentsData, AppError> {
    authorization_service::require_permission(state, "TOOL", "READ")?;
    validate_identifier(&agent_id)?;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let assignments =
        crate::infrastructure::agent_tool_assignment_repository::AgentToolAssignmentRepository::list_active_for_agent(
            &transaction, &state.tenant_id(), &agent_id,
        )?;
    let mut items = Vec::new();
    for assignment in assignments {
        let version = ToolRepository::find_version(
            &transaction,
            &state.tenant_id(),
            &assignment.tool_version_id,
        )?;
        let tool = match version.as_ref() {
            Some(version) => {
                ToolRepository::find(&transaction, &state.tenant_id(), &version.tool_id)?
            }
            None => None,
        };
        items.push(AgentToolAssignmentView {
            id: assignment.id.clone(),
            agent_id: assignment.agent_id.clone(),
            tool_id: version
                .as_ref()
                .map(|version| version.tool_id.clone())
                .unwrap_or_default(),
            tool_name: tool.map(|tool| tool.name).unwrap_or_default(),
            tool_version_id: assignment.tool_version_id.clone(),
            version_number: version
                .as_ref()
                .map(|version| version.version_number)
                .unwrap_or(0),
            assigned_at: assignment.assigned_at.clone(),
            unassigned_at: assignment.unassigned_at.clone(),
        });
    }
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    Ok(AgentToolAssignmentsData {
        has_data: !items.is_empty(),
        items,
    })
}

// كتالوج الأدوات بصدق: لكل نسخة هل توجد ربطية محوّل موثوق معروفة —
// REGISTERED_ONLY مقابل EXECUTABLE_TRUSTED_ADAPTER.
pub fn get_tool_catalog(state: &AppState) -> Result<ToolCatalogData, AppError> {
    authorization_service::require_permission(state, "TOOL", "READ")?;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let tools = ToolRepository::list(&transaction, &state.tenant_id())?;
    let mut items = Vec::new();
    for tool in tools {
        let mut versions = Vec::new();
        for version in
            ToolRepository::list_versions_for_tool(&transaction, &state.tenant_id(), &tool.id)?
        {
            let executable = crate::infrastructure::tool_adapter_binding_repository::ToolAdapterBindingRepository::find_for_version(
                &transaction,
                &state.tenant_id(),
                &version.id,
            )?
            .map(|binding| crate::application::tool_adapter_registry::lookup_adapter(&binding.adapter_key).is_some())
            .unwrap_or(false);
            versions.push(ToolVersionView {
                id: version.id,
                version_number: version.version_number,
                effect: version.effect,
                executable,
            });
        }
        versions.sort_by(|left, right| right.version_number.cmp(&left.version_number));
        items.push(ToolCatalogItem {
            assigned_agents: Vec::new(),
            tool,
            versions,
        });
    }
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    Ok(ToolCatalogData {
        has_data: !items.is_empty(),
        items,
    })
}

fn append_tool_audit(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    event_type: &str,
    resource_id: &str,
    occurred_at: &str,
) -> Result<(), AppError> {
    let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
    append_audit_event(
        transaction,
        &state.tenant_id(),
        &format!("audit_{}", Uuid::new_v4()),
        event_type,
        &state.current_user_id()?,
        "Tool",
        resource_id,
        occurred_at,
        &previous_hash,
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "tool_service_tests.rs"]
mod tests;
