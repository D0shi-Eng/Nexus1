use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        skill::{
            validate_skill_identity, validate_skill_version, SkillRecord, SkillStatus,
            SkillVersionRecord,
        },
    },
    infrastructure::{
        database::{append_audit_event, latest_audit_event_hash, AppState},
        skill_repository::SkillRepository,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSkillInput {
    pub name: String,
    pub description: String,
    pub instruction: String,
    pub input_contract: String,
    pub output_contract: String,
    pub constraints: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSkillVersionInput {
    pub skill_id: String,
    pub instruction: String,
    pub input_contract: String,
    pub output_contract: String,
    pub constraints: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillIdInput {
    pub skill_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignSkillInput {
    pub agent_id: String,
    pub skill_version_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnassignSkillInput {
    pub assignment_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMutationResult {
    pub skill: SkillRecord,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillVersionMutationResult {
    pub version: SkillVersionRecord,
}

// سطح القراءة الأدنى تستهلكه اختبارات العزل/الصلاحيات؛ سطح الواجهة هو الكتالوج.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillsData {
    pub has_data: bool,
    pub items: Vec<SkillRecord>,
}

#[cfg_attr(not(test), allow(dead_code))]
// قائمة بسيطة فوق الكتالوج نفسه — مصدر حقيقة واحد لقراءة المهارات.
pub fn list_skills(state: &AppState) -> Result<SkillsData, AppError> {
    let catalog = get_skill_catalog(state)?;
    Ok(SkillsData {
        has_data: catalog.has_data,
        items: catalog.items.into_iter().map(|item| item.skill).collect(),
    })
}

pub fn create_skill(
    state: &AppState,
    input: CreateSkillInput,
) -> Result<SkillMutationResult, AppError> {
    authorization_service::require_permission(state, "SKILL", "CREATE")?;
    validate_skill_identity(&input.name, &input.description)?;
    validate_skill_version(
        1,
        &input.instruction,
        &input.input_contract,
        &input.output_contract,
        &input.constraints,
    )?;

    let now = Utc::now().to_rfc3339();
    let skill = SkillRecord {
        id: format!("skill_{}", Uuid::new_v4()),
        tenant_id: state.tenant_id(),
        name: input.name.trim().to_owned(),
        description: input.description.trim().to_owned(),
        status: SkillStatus::Active,
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let version = SkillVersionRecord {
        id: format!("skill_version_{}", Uuid::new_v4()),
        tenant_id: state.tenant_id(),
        skill_id: skill.id.clone(),
        skill_status: SkillStatus::Active,
        version_number: 1,
        instruction: input.instruction.trim().to_owned(),
        input_contract: input.input_contract.trim().to_owned(),
        output_contract: input.output_contract.trim().to_owned(),
        constraints: input.constraints.trim().to_owned(),
        created_at: now.clone(),
    };
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    SkillRepository::insert(&transaction, &skill)?;
    SkillRepository::insert_version(&transaction, &version)?;
    append_skill_audit(&transaction, state, "SKILL_CREATED", &skill.id, &now)?;
    append_skill_audit(
        &transaction,
        state,
        "SKILL_VERSION_CREATED",
        &version.id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    Ok(SkillMutationResult { skill })
}

pub fn create_skill_version(
    state: &AppState,
    input: CreateSkillVersionInput,
) -> Result<SkillVersionMutationResult, AppError> {
    authorization_service::require_permission(state, "SKILL", "UPDATE")?;
    validate_identifier(&input.skill_id)?;
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let skill = SkillRepository::find(&transaction, &state.tenant_id(), &input.skill_id)?
        .ok_or(AppError::ResourceNotFound)?;
    if skill.status == SkillStatus::Disabled {
        return Err(AppError::InvalidStateTransition);
    }
    let version_number =
        SkillRepository::latest_version_number(&transaction, &state.tenant_id(), &input.skill_id)?
            + 1;
    validate_skill_version(
        version_number,
        &input.instruction,
        &input.input_contract,
        &input.output_contract,
        &input.constraints,
    )?;
    let now = Utc::now().to_rfc3339();
    let version = SkillVersionRecord {
        id: format!("skill_version_{}", Uuid::new_v4()),
        tenant_id: state.tenant_id(),
        skill_id: skill.id,
        skill_status: SkillStatus::Active,
        version_number,
        instruction: input.instruction.trim().to_owned(),
        input_contract: input.input_contract.trim().to_owned(),
        output_contract: input.output_contract.trim().to_owned(),
        constraints: input.constraints.trim().to_owned(),
        created_at: now.clone(),
    };
    SkillRepository::insert_version(&transaction, &version)?;
    append_skill_audit(
        &transaction,
        state,
        "SKILL_VERSION_CREATED",
        &version.id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    Ok(SkillVersionMutationResult { version })
}

pub fn disable_skill(
    state: &AppState,
    input: SkillIdInput,
) -> Result<SkillMutationResult, AppError> {
    authorization_service::require_permission(state, "SKILL", "UPDATE")?;
    validate_identifier(&input.skill_id)?;
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let existing = SkillRepository::find(&transaction, &state.tenant_id(), &input.skill_id)?
        .ok_or(AppError::ResourceNotFound)?;
    if existing.status == SkillStatus::Disabled {
        return Err(AppError::InvalidStateTransition);
    }
    if !SkillRepository::set_status(
        &transaction,
        &state.tenant_id(),
        &input.skill_id,
        SkillStatus::Disabled,
        &now,
    )? {
        return Err(AppError::Conflict);
    }
    let skill = SkillRepository::find(&transaction, &state.tenant_id(), &input.skill_id)?
        .ok_or(AppError::ResourceNotFound)?;
    append_skill_audit(&transaction, state, "SKILL_DISABLED", &skill.id, &now)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    Ok(SkillMutationResult { skill })
}

pub fn assign_skill(state: &AppState, input: AssignSkillInput) -> Result<(), AppError> {
    authorization_service::require_permission(state, "SKILL", "ASSIGN")?;
    validate_identifier(&input.agent_id)?;
    validate_identifier(&input.skill_version_id)?;
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let agent_active: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM agents WHERE tenant_id = ?1 AND id = ?2 AND status = 'ACTIVE')",
            (&state.tenant_id(), &input.agent_id),
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)?;
    if !agent_active {
        return Err(AppError::ResourceNotFound);
    }
    let version =
        SkillRepository::find_version(&transaction, &state.tenant_id(), &input.skill_version_id)?
            .ok_or(AppError::ResourceNotFound)?;
    let skill = SkillRepository::find(&transaction, &state.tenant_id(), &version.skill_id)?
        .ok_or(AppError::ResourceNotFound)?;
    if skill.status == SkillStatus::Disabled {
        return Err(AppError::InvalidStateTransition);
    }
    let assignment_id = format!("agent_skill_{}", Uuid::new_v4());
    SkillRepository::assign(
        &transaction,
        &assignment_id,
        &state.tenant_id(),
        &input.agent_id,
        &input.skill_version_id,
        &now,
    )?;
    append_skill_audit(
        &transaction,
        state,
        "AGENT_SKILL_ASSIGNED",
        &assignment_id,
        &now,
    )?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

pub fn unassign_skill(state: &AppState, input: UnassignSkillInput) -> Result<(), AppError> {
    authorization_service::require_permission(state, "SKILL", "ASSIGN")?;
    validate_identifier(&input.assignment_id)?;
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    if !SkillRepository::unassign(&transaction, &state.tenant_id(), &input.assignment_id, &now)? {
        return Err(AppError::ResourceNotFound);
    }
    append_skill_audit(
        &transaction,
        state,
        "AGENT_SKILL_UNASSIGNED",
        &input.assignment_id,
        &now,
    )?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

fn validate_identifier(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

// ==== كتالوج المهارات (MP03): سطح قراءة واحد يغذي إدارة المهارات في الواجهة ====

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCatalogItem {
    pub skill: SkillRecord,
    pub versions: Vec<SkillVersionRecord>,
    pub assignments: Vec<crate::infrastructure::skill_repository::SkillAssignmentRow>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCatalogData {
    pub has_data: bool,
    pub items: Vec<SkillCatalogItem>,
}

pub fn get_skill_catalog(state: &AppState) -> Result<SkillCatalogData, AppError> {
    authorization_service::require_permission(state, "SKILL", "READ")?;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let skills = SkillRepository::list(&transaction, &state.tenant_id())?;
    let mut items = Vec::new();
    for skill in skills {
        let versions =
            SkillRepository::list_versions_for_skill(&transaction, &state.tenant_id(), &skill.id)?;
        let assignments = SkillRepository::list_assignments_for_skill(
            &transaction,
            &state.tenant_id(),
            &skill.id,
        )?;
        items.push(SkillCatalogItem {
            skill,
            versions,
            assignments,
        });
    }
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    Ok(SkillCatalogData {
        has_data: !items.is_empty(),
        items,
    })
}

fn append_skill_audit(
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
        "Skill",
        resource_id,
        occurred_at,
        &previous_hash,
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "skill_service_tests.rs"]
mod tests;
