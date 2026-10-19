use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        models::{
            parse_control_effectiveness, ControlDetail, ControlEffectiveness, ControlStatus,
            ControlsData,
        },
    },
    infrastructure::{
        control_repository::{self, ControlLinkKind},
        database::{append_audit_event, latest_audit_event_hash, AppState},
    },
};

// حدود التحقق المعلنة: عنوان 1..=200، تصنيف/مالك <=120، معرّفات 1..=128.
const MAX_TITLE_CHARS: usize = 200;
const MAX_TEXT_CHARS: usize = 120;
const MAX_IDENTIFIER_CHARS: usize = 128;
const MAX_ASSESSMENT_NOTE_CHARS: usize = 2_000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateControlInput {
    pub title: String,
    pub category: Option<String>,
    pub owner: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateControlInput {
    pub control_id: String,
    pub title: String,
    pub category: Option<String>,
    pub owner: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlIdInput {
    pub control_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetControlStatusInput {
    pub control_id: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlObligationLinkInput {
    pub control_id: String,
    pub obligation_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlRiskLinkInput {
    pub control_id: String,
    pub risk_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlEvidenceLinkInput {
    pub control_id: String,
    pub evidence_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordControlAssessmentInput {
    pub control_id: String,
    pub effectiveness: String,
    pub assessment_note: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlMutationResult {
    pub control_id: String,
    pub status: ControlStatus,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlAssessmentResult {
    pub control_id: String,
    pub effectiveness: ControlEffectiveness,
    pub last_assessed_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlLinkMutationResult {
    pub control_id: String,
    pub link_kind: &'static str,
    pub target_id: String,
    pub link_count: u32,
    pub changed: bool,
}

pub fn load_controls(state: &AppState) -> Result<ControlsData, AppError> {
    authorization_service::require_permission(state, "CONTROL", "READ")?;

    let generated_at = Utc::now().to_rfc3339();
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    control_repository::load_controls(&connection, &state.tenant_id(), &generated_at)
}

pub fn get_control_detail(
    state: &AppState,
    input: ControlIdInput,
) -> Result<ControlDetail, AppError> {
    authorization_service::require_permission(state, "CONTROL", "READ")?;
    validate_identifier(&input.control_id)?;

    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    control_repository::load_control_detail(
        &connection,
        &state.tenant_id(),
        input.control_id.trim(),
    )
}

// إنشاء ضابط: تفويض ← تحقق ← معاملة واحدة ← تدقيق ← تثبيت.
// يُولد دائماً بحالة ACTIVE وفعالية NOT_ASSESSED — لا يقبل أي منهما من الواجهة.
pub fn create_control(
    state: &AppState,
    input: CreateControlInput,
) -> Result<ControlMutationResult, AppError> {
    authorization_service::require_permission(state, "CONTROL", "CREATE")?;
    let title = validate_title(&input.title)?;
    let category = validate_optional_text(input.category.as_deref())?;
    let owner = validate_optional_text(input.owner.as_deref())?;

    let now = Utc::now().to_rfc3339();
    let control_id = format!("control_{}", Uuid::new_v4());
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    control_repository::insert_control(
        &transaction,
        &control_id,
        &state.tenant_id(),
        title,
        category,
        owner,
        &now,
    )?;
    append_control_audit(&transaction, state, "CONTROL_CREATED", &control_id, &now)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(ControlMutationResult {
        control_id,
        status: ControlStatus::Active,
        updated_at: now,
    })
}

pub fn update_control(
    state: &AppState,
    input: UpdateControlInput,
) -> Result<ControlMutationResult, AppError> {
    authorization_service::require_permission(state, "CONTROL", "UPDATE")?;
    validate_identifier(&input.control_id)?;
    let title = validate_title(&input.title)?;
    let category = validate_optional_text(input.category.as_deref())?;
    let owner = validate_optional_text(input.owner.as_deref())?;

    let now = Utc::now().to_rfc3339();
    let control_id = input.control_id.trim().to_owned();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    let status = control_repository::current_status(&transaction, &state.tenant_id(), &control_id)?;
    let changed = control_repository::update_control(
        &transaction,
        &state.tenant_id(),
        &control_id,
        title,
        category,
        owner,
        &now,
    )?;
    if changed != 1 {
        return Err(AppError::ResourceNotFound);
    }
    append_control_audit(&transaction, state, "CONTROL_UPDATED", &control_id, &now)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(ControlMutationResult {
        control_id,
        status,
        updated_at: now,
    })
}

// تعطيل/تغيير حالة الضابط: DTO مخصص (control:disable) لا يمنح قدرة تأليف عامة.
pub fn disable_control(
    state: &AppState,
    input: SetControlStatusInput,
) -> Result<ControlMutationResult, AppError> {
    authorization_service::require_permission(state, "CONTROL", "DISABLE")?;
    validate_identifier(&input.control_id)?;
    // الحالة تُحلّ بالقائمة المغلقة نفسها المخزنة في المخطط — لا قيمة حرة.
    let status = parse_control_status_code(&input.status)?;

    let now = Utc::now().to_rfc3339();
    let control_id = input.control_id.trim().to_owned();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    let changed = control_repository::set_control_status(
        &transaction,
        &state.tenant_id(),
        &control_id,
        &status,
        &now,
    )?;
    if changed != 1 {
        return Err(AppError::ResourceNotFound);
    }
    append_control_audit(
        &transaction,
        state,
        "CONTROL_STATUS_CHANGED",
        &control_id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(ControlMutationResult {
        control_id,
        status,
        updated_at: now,
    })
}

// تقييم الفعالية: فعل مراجعة مستقل بصلاحية control:assess.
pub fn record_control_assessment(
    state: &AppState,
    input: RecordControlAssessmentInput,
) -> Result<ControlAssessmentResult, AppError> {
    authorization_service::require_permission(state, "CONTROL", "ASSESS")?;
    validate_identifier(&input.control_id)?;
    let effectiveness = parse_control_effectiveness_code(&input.effectiveness)?;
    let note = validate_optional_note(input.assessment_note.as_deref())?;

    let now = Utc::now().to_rfc3339();
    let control_id = input.control_id.trim().to_owned();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    let changed = control_repository::record_control_assessment(
        &transaction,
        &state.tenant_id(),
        &control_id,
        crate::domain::models::control_effectiveness_code(&effectiveness),
        note,
        &now,
    )?;
    if changed != 1 {
        return Err(AppError::ResourceNotFound);
    }
    append_control_audit(&transaction, state, "CONTROL_ASSESSED", &control_id, &now)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(ControlAssessmentResult {
        control_id,
        effectiveness,
        last_assessed_at: now,
    })
}

pub fn link_control_obligation(
    state: &AppState,
    input: ControlObligationLinkInput,
) -> Result<ControlLinkMutationResult, AppError> {
    change_control_link(
        state,
        &input.control_id,
        ControlLinkKind::Obligation,
        &input.obligation_id,
        true,
    )
}

pub fn unlink_control_obligation(
    state: &AppState,
    input: ControlObligationLinkInput,
) -> Result<ControlLinkMutationResult, AppError> {
    change_control_link(
        state,
        &input.control_id,
        ControlLinkKind::Obligation,
        &input.obligation_id,
        false,
    )
}

pub fn link_control_risk(
    state: &AppState,
    input: ControlRiskLinkInput,
) -> Result<ControlLinkMutationResult, AppError> {
    change_control_link(
        state,
        &input.control_id,
        ControlLinkKind::Risk,
        &input.risk_id,
        true,
    )
}

pub fn unlink_control_risk(
    state: &AppState,
    input: ControlRiskLinkInput,
) -> Result<ControlLinkMutationResult, AppError> {
    change_control_link(
        state,
        &input.control_id,
        ControlLinkKind::Risk,
        &input.risk_id,
        false,
    )
}

pub fn link_control_evidence(
    state: &AppState,
    input: ControlEvidenceLinkInput,
) -> Result<ControlLinkMutationResult, AppError> {
    change_control_link(
        state,
        &input.control_id,
        ControlLinkKind::Evidence,
        &input.evidence_id,
        true,
    )
}

pub fn unlink_control_evidence(
    state: &AppState,
    input: ControlEvidenceLinkInput,
) -> Result<ControlLinkMutationResult, AppError> {
    change_control_link(
        state,
        &input.control_id,
        ControlLinkKind::Evidence,
        &input.evidence_id,
        false,
    )
}

// مسار واحد لكل تغييرات الربط: صلاحية control:link، معاملة واحدة، تحقق ملكية الضابط والهدف،
// إدراج/حذف مُعاملي، تحديث طابع الضابط، حدث تدقيق واحد، ثم التثبيت.
// الربط المكرر محايد (idempotent): يُعلن changed=false بلا حدث تدقيق مكرر.
// فك الربط غير الموجود يُرفض صراحةً (ResourceNotFound) فلا يمرّ نجاح وهمي.
fn change_control_link(
    state: &AppState,
    control_id: &str,
    kind: ControlLinkKind,
    target_id: &str,
    link: bool,
) -> Result<ControlLinkMutationResult, AppError> {
    authorization_service::require_permission(state, "CONTROL", "LINK")?;
    validate_identifier(control_id)?;
    validate_identifier(target_id)?;

    let now = Utc::now().to_rfc3339();
    let control_id = control_id.trim().to_owned();
    let target_id = target_id.trim().to_owned();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    // ملكية المستأجر للضابط والهدف معاً قبل أي كتابة — كلاهما شرط.
    if !control_repository::control_exists(&transaction, &state.tenant_id(), &control_id)? {
        return Err(AppError::ResourceNotFound);
    }
    if !control_repository::target_exists(&transaction, &state.tenant_id(), kind, &target_id)? {
        return Err(AppError::ResourceNotFound);
    }

    let changed_rows = if link {
        control_repository::insert_control_link(
            &transaction,
            &state.tenant_id(),
            &control_id,
            kind,
            &target_id,
            &now,
        )?
    } else {
        control_repository::delete_control_link(
            &transaction,
            &state.tenant_id(),
            &control_id,
            kind,
            &target_id,
        )?
    };
    let changed = changed_rows == 1;

    if changed {
        control_repository::touch_control(&transaction, &state.tenant_id(), &control_id, &now)?;
        append_control_audit(
            &transaction,
            state,
            "CONTROL_LINK_CHANGED",
            &control_id,
            &now,
        )?;
    } else if !link {
        return Err(AppError::ResourceNotFound);
    }

    let link_count =
        control_repository::link_count(&transaction, &state.tenant_id(), &control_id, kind)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(ControlLinkMutationResult {
        control_id,
        link_kind: kind.target_column(),
        target_id,
        link_count,
        changed,
    })
}

fn append_control_audit(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    event_type: &str,
    control_id: &str,
    occurred_at: &str,
) -> Result<(), AppError> {
    let audit_id = format!("audit_{}", Uuid::new_v4());
    let previous_hash = latest_audit_event_hash(transaction, &state.tenant_id())?;
    append_audit_event(
        transaction,
        &state.tenant_id(),
        &audit_id,
        event_type,
        &state.current_user_id()?,
        "Control",
        control_id,
        occurred_at,
        &previous_hash,
    )
    .map(|_| ())
}

fn validate_title(value: &str) -> Result<&str, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_TITLE_CHARS {
        return Err(AppError::ValidationFailed);
    }
    Ok(trimmed)
}

fn validate_identifier(value: &str) -> Result<(), AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_IDENTIFIER_CHARS {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

// نص اختياري: الفراغ يُطبّع إلى NULL بدل تخزين سلسلة فارغة مضللة.
fn validate_optional_text(value: Option<&str>) -> Result<Option<&str>, AppError> {
    match value {
        None => Ok(None),
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.chars().count() > MAX_TEXT_CHARS {
                return Err(AppError::ValidationFailed);
            }
            Ok(Some(trimmed))
        }
    }
}

fn validate_optional_note(value: Option<&str>) -> Result<Option<&str>, AppError> {
    match value {
        None => Ok(None),
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.chars().count() > MAX_ASSESSMENT_NOTE_CHARS {
                return Err(AppError::ValidationFailed);
            }
            Ok(Some(trimmed))
        }
    }
}

// القائمة المغلقة للحالة كما في قيد المخطط — قيمة خارجها تُرفض قبل أي كتابة.
fn parse_control_status_code(value: &str) -> Result<ControlStatus, AppError> {
    match value.trim() {
        "ACTIVE" => Ok(ControlStatus::Active),
        "INACTIVE" => Ok(ControlStatus::Inactive),
        "DISABLED" => Ok(ControlStatus::Disabled),
        _ => Err(AppError::ValidationFailed),
    }
}

fn parse_control_effectiveness_code(value: &str) -> Result<ControlEffectiveness, AppError> {
    parse_control_effectiveness(value.trim().to_owned()).map_err(|_| AppError::ValidationFailed)
}

#[cfg(test)]
#[path = "control_service_tests.rs"]
mod tests;
