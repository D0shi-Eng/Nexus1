use chrono::{DateTime, Duration, Utc};
use rusqlite::OptionalExtension;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        models::{
            compliance_status_code, obligation_priority_code, obligation_status_code,
            review_status_code, ArchivedObligation, ComplianceStatus, CreatedObligation,
            ObligationDetails, ObligationPriority, ObligationStatus, ObligationSummaryMetrics,
            ObligationsData, ReviewStatus, UpdatedObligation, VerifiedObligation,
        },
    },
    infrastructure::{
        database::{append_audit_event, latest_audit_event_hash, AppState},
        obligation_repository::{ObligationRepository, ObligationUpdateValues},
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateObligationInput {
    pub obligation_id: String,
    pub title: String,
    pub description: String,
    pub owner: String,
    pub due_date: Option<String>,
    pub priority: ObligationPriority,
    pub status: ObligationStatus,
    pub compliance_status: ComplianceStatus,
    pub review_status: ReviewStatus,
    pub expected_version: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveObligationInput {
    pub obligation_id: String,
    pub expected_version: i64,
}

pub fn list_obligations(state: &AppState) -> Result<ObligationsData, AppError> {
    authorization_service::require_permission(state, "OBLIGATION", "READ")?;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    // وضع البيانات يُقرأ من المستأجر نفسه لا من أول صف: مؤسسة إنتاجية بلا التزامات
    // كانت تُعرض موسومة «ديمو/تركيبي» لأن الصف الأول غير موجود — ادعاء منشأ خاطئ
    // وكان يحجب مسار تأليف الالتزام نفسه في مساحة الإنتاج الفارغة.
    let data_mode = tenant_data_mode(&connection, &state.tenant_id())?;
    let items = ObligationRepository::list_active(&connection, &state.tenant_id())?;

    Ok(ObligationsData {
        summary: calculate_summary(&items),
        items,
        data_mode,
        generated_at: Utc::now().to_rfc3339(),
    })
}

// وضع بيانات المستأجر — المصدر الوحيد للحقيقة هو صف المستأجر، لا محتوى القائمة.
fn tenant_data_mode(
    connection: &rusqlite::Connection,
    tenant_id: &str,
) -> Result<crate::domain::models::DataMode, AppError> {
    let raw: String = connection
        .query_row(
            "SELECT data_mode FROM tenants WHERE id = ?1",
            rusqlite::params![tenant_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)?;
    crate::domain::models::parse_data_mode(raw)
}

// ===== تأليف الالتزام المؤسسي (0026) =====
//
// هذا هو مدخل العمل الإنتاجي: مؤسسة جديدة تُنشئ أول التزاماتها بنفسها.
// حدود الصدق: المنشأ يُسجَّل `USER_PROVIDED` دائماً ولا يُرقّى إلى `OFFICIAL`،
// وحالة التحقق تبدأ `UNVERIFIED` — التحقق البشري مسار منفصل بصلاحية منفصلة.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateObligationInput {
    pub source_id: String,
    pub title: String,
    pub description: String,
    pub owner: String,
    pub due_date: Option<String>,
    pub priority: ObligationPriority,
    // الانطباق يعلنه المستخدم صراحةً؛ لا يستنتجه النظام من نص المصدر.
    pub applicability: String,
}

pub fn create_obligation(
    state: &AppState,
    input: CreateObligationInput,
) -> Result<CreatedObligation, AppError> {
    authorization_service::require_permission(state, "OBLIGATION", "CREATE")?;
    validate_create_input(&input)?;
    let tenant_id = state.tenant_id();
    let now = Utc::now().to_rfc3339();
    let obligation_id = format!("obl_{}", Uuid::new_v4());

    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    // المصدر المرجعي يجب أن يكون في نفس المستأجر — يمنع الربط العابر مبكراً
    // بنفس رمز الخطأ الذي يُنتجه مفتاح الربط في قاعدة البيانات.
    let source_exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM regulatory_sources WHERE id = ?1 AND tenant_id = ?2)",
            rusqlite::params![input.source_id.trim(), &tenant_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)?;
    if !source_exists {
        return Err(AppError::ObligationSourceInvalid);
    }

    transaction
        .execute(
            "INSERT INTO obligations (
                id, tenant_id, source_id, title, description, priority, owner, deadline,
                severity, status, applicability, evidence_status, verification_state,
                compliance_status, review_status, source_type, version, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?6, 'OPEN', ?9, 'MISSING', 'UNVERIFIED',
                       'NOT_ASSESSED', 'NOT_REVIEWED', 'USER_PROVIDED', 1, ?10, ?10)",
            rusqlite::params![
                &obligation_id,
                &tenant_id,
                input.source_id.trim(),
                input.title.trim(),
                input.description.trim(),
                obligation_priority_code(&input.priority),
                input.owner.trim(),
                input.due_date.as_deref(),
                input.applicability.trim(),
                &now,
            ],
        )
        .map_err(|_| AppError::StorageOperation)?;

    append_obligation_audit_event(
        &transaction,
        state,
        "OBLIGATION_CREATED",
        &obligation_id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(CreatedObligation {
        id: obligation_id,
        status: "OPEN".to_owned(),
        source_type: "USER_PROVIDED".to_owned(),
        verification_state: "UNVERIFIED".to_owned(),
        created_at: now,
    })
}

fn validate_create_input(input: &CreateObligationInput) -> Result<(), AppError> {
    validate_identifier(&input.source_id)?;
    let due_date_is_valid = input
        .due_date
        .as_deref()
        .map(|value| DateTime::parse_from_rfc3339(value).is_ok())
        .unwrap_or(true);
    let applicability_is_valid = matches!(
        input.applicability.trim(),
        "APPLIES" | "DOES_NOT_APPLY" | "UNCERTAIN" | "REQUIRES_REVIEW"
    );
    if input.title.trim().is_empty()
        || input.title.chars().count() > 240
        || input.description.chars().count() > 4_000
        || input.owner.trim().is_empty()
        || input.owner.chars().count() > 160
        || !due_date_is_valid
        || !applicability_is_valid
    {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

pub fn get_obligation(
    state: &AppState,
    obligation_id: &str,
) -> Result<ObligationDetails, AppError> {
    authorization_service::require_permission(state, "OBLIGATION", "READ")?;
    validate_identifier(obligation_id)?;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    ObligationRepository::get_active_details(&connection, &state.tenant_id(), obligation_id)?
        .ok_or(AppError::ResourceNotFound)
}

pub fn update_obligation(
    state: &AppState,
    input: UpdateObligationInput,
) -> Result<UpdatedObligation, AppError> {
    authorization_service::require_permission(state, "OBLIGATION", "UPDATE")?;
    validate_update_input(&input)?;

    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let existing = ObligationRepository::get_active_details(
        &connection,
        &state.tenant_id(),
        &input.obligation_id,
    )?
    .ok_or(AppError::ResourceNotFound)?;
    if existing.version != input.expected_version {
        return Err(AppError::Conflict);
    }
    if !is_valid_state_transition(&existing.obligation.status, &input.status) {
        return Err(AppError::InvalidStateTransition);
    }

    let now = Utc::now().to_rfc3339();
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let values = ObligationUpdateValues {
        title: input.title.trim(),
        description: input.description.trim(),
        owner: input.owner.trim(),
        due_date: input.due_date.as_deref(),
        priority: obligation_priority_code(&input.priority),
        status: obligation_status_code(&input.status),
        compliance_status: compliance_status_code(&input.compliance_status),
        review_status: review_status_code(&input.review_status),
        expected_version: input.expected_version,
        updated_at: &now,
    };
    let version = ObligationRepository::update_active(
        &transaction,
        &state.tenant_id(),
        &input.obligation_id,
        &values,
    )?
    .ok_or(AppError::Conflict)?;
    append_obligation_audit_event(
        &transaction,
        state,
        "OBLIGATION_UPDATED",
        &input.obligation_id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(UpdatedObligation {
        id: input.obligation_id,
        updated_at: now,
        version,
    })
}

// =====================================================================
// خطوة التحقق البشري — §14 «Review / Approval → Verification»
// ---------------------------------------------------------------------
// العيب الذي أُغلق: لا مسار في المنتج كان يرفع `obligations.verification_state`
// إلى HUMAN_VERIFIED — لا `update_obligation` (لا تكتب العمود) ولا أي أمر آخر.
// فكان مؤشر «تغطية تم التحقق منها» في اللوحة **صفراً بنيوياً**: لا يمكن أن
// يتحرك مهما أنجز المستخدم، وهو أسوأ من غياب المؤشر لأنه يوهم بالفشل.
//
// فصل السلطة: الصلاحية `obligation:verify` منفصلة عن `obligation:update`،
// وتُمنح لجهة المراجعة وحدها — فلا يتحقق منفّذ العمل من عمله بنفسه.
//
// نظام الحقيقة محفوظ: هذا المسار إنسان صريح (نداء واحد بفاعل مُعرَّف)، ولا
// يوجد أي مسار آلي يرفع التصنيف — و`domain/truth.rs` يمنع الترقية الآلية أصلاً.
// =====================================================================
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyObligationInput {
    pub obligation_id: String,
    pub expected_version: u32,
}

pub fn verify_obligation(
    state: &AppState,
    input: VerifyObligationInput,
) -> Result<VerifiedObligation, AppError> {
    authorization_service::require_permission(state, "OBLIGATION", "VERIFY")?;
    validate_identifier(&input.obligation_id)?;
    if input.expected_version < 1 {
        return Err(AppError::ValidationFailed);
    }

    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let tenant_id = state.tenant_id();

    let row: Option<(String, i64)> = connection
        .query_row(
            "SELECT verification_state, version FROM obligations
             WHERE id = ?1 AND tenant_id = ?2 AND archived_at IS NULL",
            rusqlite::params![&input.obligation_id, &tenant_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| AppError::DataAccess)?;
    // التزام مستأجر آخر يُعامل كغير موجود — لا تسريب وجود عبر رمز خطأ مختلف.
    let (verification_state, version) = row.ok_or(AppError::ResourceNotFound)?;
    if version != i64::from(input.expected_version) {
        return Err(AppError::Conflict);
    }
    // التحقق فعل واحد لا يتكرر: تكراره ادعاء بحدث لم يقع.
    if verification_state == "HUMAN_VERIFIED" {
        return Err(AppError::InvalidStateTransition);
    }
    // شرط مسبق: لا تحقق بشري بلا دليل مسجَّل فعلاً على الالتزام.
    if !obligation_has_evidence(&connection, &tenant_id, &input.obligation_id)? {
        return Err(AppError::ObligationVerificationRequiresEvidence);
    }

    let now = Utc::now().to_rfc3339();
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let changed = transaction
        .execute(
            "UPDATE obligations SET verification_state = 'HUMAN_VERIFIED', updated_at = ?1, version = version + 1
             WHERE id = ?2 AND tenant_id = ?3 AND version = ?4",
            rusqlite::params![&now, &input.obligation_id, &tenant_id, input.expected_version],
        )
        .map_err(|_| AppError::StorageOperation)?;
    if changed != 1 {
        return Err(AppError::Conflict);
    }
    append_obligation_audit_event(
        &transaction,
        state,
        "OBLIGATION_HUMAN_VERIFIED",
        &input.obligation_id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(VerifiedObligation {
        id: input.obligation_id,
        verification_state: "HUMAN_VERIFIED".to_owned(),
        verified_at: now,
        version: input.expected_version + 1,
    })
}

// وجود دليل مسجَّل فعلاً — نفس مجموعة الحالات المقبولة المستخدمة في شرط طلب
// اعتماد المهمة، فلا يتباعد تعريف «الدليل» بين المسارين.
fn obligation_has_evidence(
    connection: &rusqlite::Connection,
    tenant_id: &str,
    obligation_id: &str,
) -> Result<bool, AppError> {
    let mut statement = connection
        .prepare("SELECT status FROM evidence_items WHERE tenant_id = ?1 AND obligation_id = ?2")
        .map_err(|_| AppError::DataAccess)?;
    let rows = statement
        .query_map(rusqlite::params![tenant_id, obligation_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|_| AppError::DataAccess)?;
    for row in rows {
        let status = row.map_err(|_| AppError::DataAccess)?;
        if crate::domain::task_lifecycle::is_acceptable_evidence_status(&status) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn archive_obligation(
    state: &AppState,
    input: ArchiveObligationInput,
) -> Result<ArchivedObligation, AppError> {
    authorization_service::require_permission(state, "OBLIGATION", "ARCHIVE")?;
    validate_identifier(&input.obligation_id)?;
    if input.expected_version < 1 {
        return Err(AppError::ValidationFailed);
    }

    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let existing_version = ObligationRepository::active_version(
        &connection,
        &state.tenant_id(),
        &input.obligation_id,
    )?
    .ok_or(AppError::ResourceNotFound)?;
    if existing_version != input.expected_version {
        return Err(AppError::Conflict);
    }

    let archived_at = Utc::now().to_rfc3339();
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let archived = ObligationRepository::archive_active(
        &transaction,
        &state.tenant_id(),
        &input.obligation_id,
        &state.current_user_id()?,
        input.expected_version,
        &archived_at,
    )?;
    if !archived {
        return Err(AppError::Conflict);
    }
    append_obligation_audit_event(
        &transaction,
        state,
        "OBLIGATION_ARCHIVED",
        &input.obligation_id,
        &archived_at,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(ArchivedObligation {
        id: input.obligation_id,
        archived_at,
    })
}

fn append_obligation_audit_event(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    event_type: &str,
    obligation_id: &str,
    occurred_at: &str,
) -> Result<(), AppError> {
    let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
    append_audit_event(
        transaction,
        &state.tenant_id(),
        &format!("audit_{}", Uuid::new_v4()),
        event_type,
        &state.current_user_id()?,
        "Obligation",
        obligation_id,
        occurred_at,
        &previous_hash,
    )?;
    Ok(())
}

fn validate_identifier(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

fn validate_update_input(input: &UpdateObligationInput) -> Result<(), AppError> {
    validate_identifier(&input.obligation_id)?;
    let due_date_is_valid = input
        .due_date
        .as_deref()
        .map(|value| DateTime::parse_from_rfc3339(value).is_ok())
        .unwrap_or(true);
    if input.title.trim().is_empty()
        || input.title.chars().count() > 240
        || input.description.chars().count() > 4_000
        || input.owner.trim().is_empty()
        || input.owner.chars().count() > 160
        || input.expected_version < 1
        || !due_date_is_valid
    {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

fn is_valid_state_transition(current: &ObligationStatus, requested: &ObligationStatus) -> bool {
    matches!(
        (current, requested),
        (ObligationStatus::Open, ObligationStatus::Open)
            | (ObligationStatus::Open, ObligationStatus::InProgress)
            | (ObligationStatus::Open, ObligationStatus::AtRisk)
            | (ObligationStatus::Open, ObligationStatus::RequiresReview)
            | (ObligationStatus::InProgress, ObligationStatus::InProgress)
            | (ObligationStatus::InProgress, ObligationStatus::AtRisk)
            | (ObligationStatus::InProgress, ObligationStatus::Completed)
            | (
                ObligationStatus::InProgress,
                ObligationStatus::RequiresReview
            )
            | (ObligationStatus::AtRisk, ObligationStatus::AtRisk)
            | (ObligationStatus::AtRisk, ObligationStatus::InProgress)
            | (ObligationStatus::AtRisk, ObligationStatus::RequiresReview)
            | (ObligationStatus::Completed, ObligationStatus::Completed)
            | (
                ObligationStatus::Completed,
                ObligationStatus::RequiresReview
            )
            | (
                ObligationStatus::RequiresReview,
                ObligationStatus::RequiresReview
            )
            | (ObligationStatus::RequiresReview, ObligationStatus::Open)
            | (
                ObligationStatus::RequiresReview,
                ObligationStatus::InProgress
            )
            | (ObligationStatus::RequiresReview, ObligationStatus::AtRisk)
            | (
                ObligationStatus::RequiresReview,
                ObligationStatus::Completed
            )
    )
}

fn calculate_summary(
    items: &[crate::domain::models::ObligationListItem],
) -> ObligationSummaryMetrics {
    let now = Utc::now();
    let due_soon_limit = now + Duration::days(7);
    let mut summary = ObligationSummaryMetrics {
        total: items.len() as u32,
        critical: 0,
        due_soon: 0,
        overdue: 0,
        non_compliant: 0,
        under_review: 0,
    };

    for item in items {
        if item.priority == ObligationPriority::Critical {
            summary.critical += 1;
        }
        if item.compliance_status == ComplianceStatus::NonCompliant {
            summary.non_compliant += 1;
        }
        if item.review_status == ReviewStatus::InReview
            || item.status == ObligationStatus::RequiresReview
        {
            summary.under_review += 1;
        }
        if let Some(due_date) = item.due_date.as_deref().and_then(parse_due_date) {
            if due_date < now {
                summary.overdue += 1;
            } else if due_date <= due_soon_limit {
                summary.due_soon += 1;
            }
        }
    }
    summary
}

fn parse_due_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {

    use chrono::Utc;
    use rusqlite::Connection;

    use super::{
        archive_obligation, create_obligation, get_obligation, list_obligations, update_obligation,
        verify_obligation, ArchiveObligationInput, CreateObligationInput, UpdateObligationInput,
        VerifyObligationInput,
    };
    use crate::{
        domain::{
            error::AppError,
            models::{ComplianceStatus, ObligationPriority, ObligationStatus, ReviewStatus},
        },
        infrastructure::database::AppState,
    };

    fn state_with_obligation_permissions() -> AppState {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .execute_batch(include_str!("../../migrations/0001_initial.sql"))
            .expect("هجرة الأساس يجب أن تنجح");
        connection
            .execute_batch(include_str!(
                "../../migrations/0002_identity_and_access.sql"
            ))
            .expect("هجرة الهوية يجب أن تنجح");
        connection
            .execute_batch(include_str!("../../migrations/0003_workflow_and_risk.sql"))
            .expect("هجرة العمل يجب أن تنجح");
        connection
            .execute_batch(include_str!("../../migrations/0004_evidence_metadata.sql"))
            .expect("هجرة الأدلة يجب أن تنجح");
        connection
            .execute_batch(include_str!(
                "../../migrations/0007_obligation_management.sql"
            ))
            .expect("هجرة الالتزامات يجب أن تنجح");
        // 0008 تحمل مشغّلات عزل تحديث العلاقات بين المستأجرين — لا يجوز إسقاطها
        // من التركيبة: غيابها يُبطل اختبار الضابط لا الضابط نفسه.
        connection
            .execute_batch(include_str!(
                "../../migrations/0008_obligation_relationship_update_isolation.sql"
            ))
            .expect("هجرة عزل تحديث العلاقات يجب أن تنجح");
        connection
            .execute_batch(concat!(
                include_str!("../../migrations/0025_task_lifecycle.sql"),
                include_str!("../../migrations/0026_production_user_and_authoring.sql")
            ))
            .expect("هجرة دورة حياة المهمة وعيوب UAT يجب أن تنجح");
        let now = Utc::now().to_rfc3339();

        connection.execute("INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant-a', 'Tenant A', 'DEMO_SYNTHETIC', ?1, ?1)", [&now]).expect("المستأجر يجب أن ينشأ");
        connection.execute("INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES ('user-a', 'tenant-a', 'User A', 'a@nexus.local', NULL, 'ACTIVE', ?1, ?1)", [&now]).expect("المستخدم يجب أن ينشأ");
        connection.execute("INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-a', 'tenant-a', 'OBLIGATION_MANAGER', 'Obligation Manager', ?1, ?1)", [&now]).expect("الدور يجب أن ينشأ");
        for (id, code, action) in [
            ("perm-read", "obligation:read", "READ"),
            ("perm-create", "obligation:create", "CREATE"),
            ("perm-update", "obligation:update", "UPDATE"),
            ("perm-archive", "obligation:archive", "ARCHIVE"),
            ("perm-verify", "obligation:verify", "VERIFY"),
        ] {
            connection.execute("INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES (?1, ?2, 'OBLIGATION', ?3, 'HIGHLY_RESTRICTED', ?4)", (id, code, action, &now)).expect("الصلاحية يجب أن تنشأ");
            connection
                .execute(
                    "INSERT INTO role_permissions (role_id, permission_id) VALUES ('role-a', ?1)",
                    [id],
                )
                .expect("ربط الصلاحية يجب أن ينجح");
        }
        connection.execute("INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user-a', 'role-a', ?1, 'system')", [&now]).expect("تعيين الدور يجب أن ينجح");
        connection.execute("INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session-a', 'tenant-a', 'user-a', ?1, '2030-01-01T00:00:00Z', NULL, 'test')", [&now]).expect("الجلسة يجب أن تنشأ");
        connection.execute("INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES ('source-a', 'tenant-a', 'DEMO / SYNTHETIC Source A', 'Synthetic source A', 'https://example.invalid/demo/a', '1.0', ?1, 0, 'REQUIRES_REVIEW', ?1, ?1)", [&now]).expect("المصدر يجب أن ينشأ");
        connection.execute("INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, source_type, created_at, updated_at) VALUES ('obligation-a', 'tenant-a', 'source-a', 'Synthetic obligation A', 'Owner A', '2030-01-10T00:00:00Z', 'CRITICAL', 'OPEN', 'APPLIES', 'MISSING', 'UNVERIFIED', 'SYNTHETIC', ?1, ?1)", [&now]).expect("الالتزام يجب أن ينشأ");

        AppState::test_instance(connection, "tenant-a".to_owned(), "user-a".to_owned())
    }

    // 0025: خطوة التحقق البشري — الشروط المسبقة والأهلية وعدم التكرار.
    #[test]
    fn human_verification_is_governed_evidenced_and_single_shot() {
        let state = state_with_obligation_permissions();
        let input = |version: u32| VerifyObligationInput {
            obligation_id: "obligation-a".to_owned(),
            expected_version: version,
        };

        // 1) بلا دليل مسجَّل: رفض صريح بخطئه المعنون.
        assert!(matches!(
            verify_obligation(&state, input(1)),
            Err(AppError::ObligationVerificationRequiresEvidence)
        ));

        // 2) بعد تسجيل دليل مقبول: التحقق ينجح ويُدوَّن.
        {
            let now = Utc::now().to_rfc3339();
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute(
                    "INSERT INTO evidence_items (id, tenant_id, obligation_id, classification, status, content_hash, version, created_at, updated_at) VALUES ('evidence-verify', 'tenant-a', 'obligation-a', 'INTERNAL', 'VALID', 'hash-verify', 1, ?1, ?1)",
                    [&now],
                )
                .expect("الدليل يجب أن ينشأ");
        }
        let verified = verify_obligation(&state, input(1)).expect("التحقق بوجود دليل يجب أن ينجح");
        assert_eq!(verified.verification_state, "HUMAN_VERIFIED");
        assert_eq!(verified.version, 2);

        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            let (state_code, version): (String, i64) = connection
                .query_row(
                    "SELECT verification_state, version FROM obligations WHERE id = 'obligation-a'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("قراءة الالتزام يجب أن تنجح");
            assert_eq!(state_code, "HUMAN_VERIFIED");
            assert_eq!(version, 2, "التحقق فعل كتابة يرفع الإصدار");
            let audits: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM audit_events WHERE resource_id = 'obligation-a' AND event_type = 'OBLIGATION_HUMAN_VERIFIED'",
                    [],
                    |row| row.get(0),
                )
                .expect("عدّ التدقيق يجب أن ينجح");
            assert_eq!(audits, 1);
        }

        // 3) التحقق فعل واحد: تكراره ادعاء بحدث لم يقع.
        assert!(matches!(
            verify_obligation(&state, input(2)),
            Err(AppError::InvalidStateTransition)
        ));
        // 4) إصدار قديم يُرفض قبل أي كتابة.
        assert!(matches!(
            verify_obligation(&state, input(1)),
            Err(AppError::Conflict)
        ));
    }

    #[test]
    fn human_verification_requires_its_own_permission_and_stays_in_tenant() {
        let state = state_with_obligation_permissions();
        let now = Utc::now().to_rfc3339();

        // التزام في مستأجر آخر: غير موجود من منظور هذا المستأجر.
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute(
                    "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant-b', 'Tenant B', 'DEMO_SYNTHETIC', ?1, ?1)",
                    [&now],
                )
                .expect("المستأجر الآخر يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES ('source-b', 'tenant-b', 'Synthetic', 'Synthetic B', NULL, '1.0', ?1, 0, 'UNVERIFIED', ?1, ?1)",
                    [&now],
                )
                .expect("مصدر المستأجر الآخر يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, source_type, created_at, updated_at) VALUES ('obligation-b', 'tenant-b', 'source-b', 'Synthetic B', 'Owner B', NULL, 'HIGH', 'OPEN', 'APPLIES', 'MISSING', 'UNVERIFIED', 'SYNTHETIC', ?1, ?1)",
                    [&now],
                )
                .expect("التزام المستأجر الآخر يجب أن ينشأ");
        }

        // عزل المستأجرين يُفحص أولاً والصلاحية لا تزال ممنوحة: التزام مستأجر آخر
        // يُعامل كغير موجود، فلا يُكشف وجوده برمز خطأ مختلف.
        assert!(matches!(
            verify_obligation(
                &state,
                VerifyObligationInput {
                    obligation_id: "obligation-b".to_owned(),
                    expected_version: 1,
                },
            ),
            Err(AppError::ResourceNotFound)
        ));

        // ثم تُسحب الصلاحية: الأهلية شرط مستقل عن `obligation:update`.
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute(
                    "DELETE FROM role_permissions WHERE permission_id = 'perm-verify'",
                    [],
                )
                .expect("سحب الصلاحية يجب أن ينجح");
        }
        assert!(matches!(
            verify_obligation(
                &state,
                VerifyObligationInput {
                    obligation_id: "obligation-a".to_owned(),
                    expected_version: 1,
                },
            ),
            Err(AppError::AuthorizationDenied)
        ));
    }

    fn update_input(version: i64) -> UpdateObligationInput {
        UpdateObligationInput {
            obligation_id: "obligation-a".to_owned(),
            title: "Updated synthetic obligation".to_owned(),
            description: "Updated record used only for local verification.".to_owned(),
            owner: "Updated Owner".to_owned(),
            due_date: Some("2030-01-12T00:00:00Z".to_owned()),
            priority: ObligationPriority::High,
            status: ObligationStatus::InProgress,
            compliance_status: ComplianceStatus::PartiallyCompliant,
            review_status: ReviewStatus::InReview,
            expected_version: version,
        }
    }

    fn add_other_tenant_obligation(state: &AppState) {
        let now = Utc::now().to_rfc3339();
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection.execute("INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant-b', 'Tenant B', 'DEMO_SYNTHETIC', ?1, ?1)", [&now]).expect("المستأجر الآخر يجب أن ينشأ");
        connection.execute("INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES ('source-b', 'tenant-b', 'DEMO / SYNTHETIC Source B', 'Synthetic source B', 'https://example.invalid/demo/b', '1.0', ?1, 0, 'UNVERIFIED', ?1, ?1)", [&now]).expect("مصدر المستأجر الآخر يجب أن ينشأ");
        connection.execute("INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, source_type, created_at, updated_at) VALUES ('obligation-b', 'tenant-b', 'source-b', 'Synthetic obligation B', 'Owner B', NULL, 'HIGH', 'OPEN', 'APPLIES', 'MISSING', 'UNVERIFIED', 'SYNTHETIC', ?1, ?1)", [&now]).expect("التزام المستأجر الآخر يجب أن ينشأ");
    }

    #[test]
    fn lists_and_gets_current_tenant_obligations_with_related_resources() {
        let state = state_with_obligation_permissions();
        let now = Utc::now().to_rfc3339();
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection.execute("INSERT INTO evidence_items (id, tenant_id, obligation_id, classification, status, content_hash, version, created_at, updated_at, label, expires_at) VALUES ('evidence-a', 'tenant-a', 'obligation-a', 'INTERNAL', 'REQUIRES_REVIEW', 'hash-a', 1, ?1, ?1, 'Synthetic evidence', NULL)", [&now]).expect("مرجع الدليل يجب أن ينشأ");
            connection.execute("INSERT INTO tasks (id, tenant_id, obligation_id, title, description, assignee_user_id, due_at, priority, status, created_at, updated_at) VALUES ('task-a', 'tenant-a', 'obligation-a', 'Synthetic task', 'Synthetic task description', 'user-a', NULL, 'HIGH', 'OPEN', ?1, ?1)", [&now]).expect("المهمة يجب أن تنشأ");
            connection.execute("INSERT INTO risks (id, tenant_id, obligation_id, title, severity, likelihood, impact, status, owner, created_at, updated_at) VALUES ('risk-a', 'tenant-a', 'obligation-a', 'Synthetic risk', 'HIGH', 3, 4, 'OPEN', 'Owner A', ?1, ?1)", [&now]).expect("الخطر يجب أن ينشأ");
            connection.execute("INSERT INTO approvals (id, tenant_id, resource_type, resource_id, resource_version, requested_by, required_role_code, decision, decision_by, decision_reason, decided_at, created_at, updated_at) VALUES ('approval-a', 'tenant-a', 'TASK', 'task-a', 1, 'user-a', 'OBLIGATION_MANAGER', 'PENDING', NULL, NULL, NULL, ?1, ?1)", [&now]).expect("الموافقة يجب أن تنشأ");
        }

        let listed = list_obligations(&state).expect("القائمة يجب أن تقرأ");
        assert_eq!(listed.items.len(), 1);
        assert_eq!(listed.summary.total, 1);
        assert_eq!(
            listed.items[0].source_type,
            crate::domain::models::SourceType::Synthetic
        );
        let details = get_obligation(&state, "obligation-a").expect("التفاصيل يجب أن تقرأ");
        assert_eq!(details.related_resources.evidence.len(), 1);
        assert_eq!(details.related_resources.tasks.len(), 1);
        assert_eq!(details.related_resources.risks.len(), 1);
        assert_eq!(details.related_resources.approvals.len(), 1);
    }

    #[test]
    fn cross_tenant_read_update_and_archive_are_not_found() {
        let state = state_with_obligation_permissions();
        add_other_tenant_obligation(&state);
        assert!(matches!(
            get_obligation(&state, "obligation-b"),
            Err(AppError::ResourceNotFound)
        ));
        let mut cross_update = update_input(1);
        cross_update.obligation_id = "obligation-b".to_owned();
        assert!(matches!(
            update_obligation(&state, cross_update),
            Err(AppError::ResourceNotFound)
        ));
        assert!(matches!(
            archive_obligation(
                &state,
                ArchiveObligationInput {
                    obligation_id: "obligation-b".to_owned(),
                    expected_version: 1
                }
            ),
            Err(AppError::ResourceNotFound)
        ));
    }

    #[test]
    fn rejects_user_without_permission_and_expired_session() {
        let state = state_with_obligation_permissions();
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute("DELETE FROM role_permissions WHERE role_id = 'role-a'", [])
                .expect("إزالة الصلاحية يجب أن تنجح");
        }
        assert!(matches!(
            list_obligations(&state),
            Err(AppError::AuthorizationDenied)
        ));

        let expired_state = state_with_obligation_permissions();
        {
            let connection = expired_state.connection.lock().expect("القفل يجب أن يتاح");
            connection.execute("UPDATE local_sessions SET expires_at = '2020-01-01T00:00:00Z' WHERE id = 'session-a'", []).expect("إنهاء الجلسة يجب أن ينجح");
        }
        assert!(matches!(
            list_obligations(&expired_state),
            Err(AppError::AuthorizationDenied)
        ));
    }

    #[test]
    fn validates_input_and_prevents_invalid_status_transition() {
        let state = state_with_obligation_permissions();
        let mut invalid = update_input(1);
        invalid.title = " ".to_owned();
        assert!(matches!(
            update_obligation(&state, invalid),
            Err(AppError::ValidationFailed)
        ));
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute(
                    "UPDATE obligations SET status = 'COMPLETED' WHERE id = 'obligation-a'",
                    [],
                )
                .expect("إعداد الحالة يجب أن ينجح");
        }
        let mut transition = update_input(1);
        transition.status = ObligationStatus::Open;
        assert!(matches!(
            update_obligation(&state, transition),
            Err(AppError::InvalidStateTransition)
        ));
    }

    #[test]
    fn updates_archives_and_writes_audit_events_with_optimistic_concurrency() {
        let state = state_with_obligation_permissions();
        let updated =
            update_obligation(&state, update_input(1)).expect("التحديث المصرح به يجب أن ينجح");
        assert_eq!(updated.version, 2);
        assert!(matches!(
            update_obligation(&state, update_input(1)),
            Err(AppError::Conflict)
        ));
        let archived = archive_obligation(
            &state,
            ArchiveObligationInput {
                obligation_id: "obligation-a".to_owned(),
                expected_version: 2,
            },
        )
        .expect("الأرشفة يجب أن تنجح");
        assert_eq!(archived.id, "obligation-a");
        let listed = list_obligations(&state).expect("القائمة يجب أن تقرأ بعد الأرشفة");
        assert!(listed.items.is_empty());
        assert!(matches!(
            get_obligation(&state, "obligation-a"),
            Err(AppError::ResourceNotFound)
        ));
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let audit_count: i64 = connection.query_row("SELECT COUNT(*) FROM audit_events WHERE tenant_id = 'tenant-a' AND resource_id = 'obligation-a' AND event_type IN ('OBLIGATION_UPDATED', 'OBLIGATION_ARCHIVED')", [], |row| row.get(0)).expect("استعلام التدقيق يجب أن ينجح");
        assert_eq!(audit_count, 2);
    }

    #[test]
    fn database_trigger_prevents_cross_tenant_source_and_related_resource_links() {
        let state = state_with_obligation_permissions();
        add_other_tenant_obligation(&state);
        let now = Utc::now().to_rfc3339();
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        assert!(connection.execute("INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, created_at, updated_at) VALUES ('invalid-source-link', 'tenant-a', 'source-b', 'Invalid relation', 'Owner A', NULL, 'LOW', 'OPEN', 'APPLIES', 'MISSING', 'UNVERIFIED', ?1, ?1)", [&now]).is_err());
        assert!(connection.execute("INSERT INTO evidence_items (id, tenant_id, obligation_id, classification, status, content_hash, version, created_at, updated_at, label, expires_at) VALUES ('invalid-evidence-link', 'tenant-a', 'obligation-b', 'INTERNAL', 'REQUIRES_REVIEW', 'hash-invalid', 1, ?1, ?1, 'Invalid evidence', NULL)", [&now]).is_err());
        assert!(connection.execute("INSERT INTO tasks (id, tenant_id, obligation_id, title, description, assignee_user_id, due_at, priority, status, created_at, updated_at) VALUES ('invalid-task-link', 'tenant-a', 'obligation-b', 'Invalid task', 'Invalid relation', 'user-a', NULL, 'LOW', 'OPEN', ?1, ?1)", [&now]).is_err());
        assert!(connection.execute("INSERT INTO risks (id, tenant_id, obligation_id, title, severity, likelihood, impact, status, owner, created_at, updated_at) VALUES ('invalid-risk-link', 'tenant-a', 'obligation-b', 'Invalid risk', 'LOW', 1, 1, 'OPEN', 'Owner A', ?1, ?1)", [&now]).is_err());
    }

    #[test]
    fn database_trigger_prevents_cross_tenant_related_resource_updates() {
        let state = state_with_obligation_permissions();
        add_other_tenant_obligation(&state);
        let now = Utc::now().to_rfc3339();
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection.execute("INSERT INTO evidence_items (id, tenant_id, obligation_id, classification, status, content_hash, version, created_at, updated_at, label, expires_at) VALUES ('evidence-update-link', 'tenant-a', 'obligation-a', 'INTERNAL', 'REQUIRES_REVIEW', 'hash-update', 1, ?1, ?1, 'Valid evidence', NULL)", [&now]).expect("الدليل الصحيح يجب أن ينشأ");
        connection.execute("INSERT INTO tasks (id, tenant_id, obligation_id, title, description, assignee_user_id, due_at, priority, status, created_at, updated_at) VALUES ('task-update-link', 'tenant-a', 'obligation-a', 'Valid task', 'Valid relation', 'user-a', NULL, 'LOW', 'OPEN', ?1, ?1)", [&now]).expect("المهمة الصحيحة يجب أن تنشأ");
        connection.execute("INSERT INTO risks (id, tenant_id, obligation_id, title, severity, likelihood, impact, status, owner, created_at, updated_at) VALUES ('risk-update-link', 'tenant-a', 'obligation-a', 'Valid risk', 'LOW', 1, 1, 'OPEN', 'Owner A', ?1, ?1)", [&now]).expect("الخطر الصحيح يجب أن ينشأ");

        assert!(connection.execute("UPDATE evidence_items SET obligation_id = 'obligation-b' WHERE id = 'evidence-update-link'", []).is_err());
        assert!(connection
            .execute(
                "UPDATE tasks SET obligation_id = 'obligation-b' WHERE id = 'task-update-link'",
                []
            )
            .is_err());
        assert!(connection
            .execute(
                "UPDATE risks SET obligation_id = 'obligation-b' WHERE id = 'risk-update-link'",
                []
            )
            .is_err());
    }

    #[test]
    fn rejects_each_missing_obligation_permission_and_revoked_session() {
        let read_denied = state_with_obligation_permissions();
        {
            let connection = read_denied.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute(
                    "DELETE FROM role_permissions WHERE permission_id = 'perm-read'",
                    [],
                )
                .expect("إزالة صلاحية القراءة يجب أن تنجح");
        }
        assert!(matches!(
            list_obligations(&read_denied),
            Err(AppError::AuthorizationDenied)
        ));

        let update_denied = state_with_obligation_permissions();
        {
            let connection = update_denied.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute(
                    "DELETE FROM role_permissions WHERE permission_id = 'perm-update'",
                    [],
                )
                .expect("إزالة صلاحية التحديث يجب أن تنجح");
        }
        assert!(matches!(
            update_obligation(&update_denied, update_input(1)),
            Err(AppError::AuthorizationDenied)
        ));

        let archive_denied = state_with_obligation_permissions();
        {
            let connection = archive_denied.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute(
                    "DELETE FROM role_permissions WHERE permission_id = 'perm-archive'",
                    [],
                )
                .expect("إزالة صلاحية الأرشفة يجب أن تنجح");
        }
        assert!(matches!(
            archive_obligation(
                &archive_denied,
                ArchiveObligationInput {
                    obligation_id: "obligation-a".to_owned(),
                    expected_version: 1
                }
            ),
            Err(AppError::AuthorizationDenied)
        ));

        let revoked = state_with_obligation_permissions();
        {
            let connection = revoked.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute(
                    "UPDATE local_sessions SET revoked_at = ?1 WHERE id = 'session-a'",
                    [Utc::now().to_rfc3339()],
                )
                .expect("إلغاء الجلسة يجب أن ينجح");
        }
        assert!(matches!(
            list_obligations(&revoked),
            Err(AppError::AuthorizationDenied)
        ));
    }

    #[test]
    fn rolls_back_obligation_update_when_audit_event_cannot_be_written() {
        let state = state_with_obligation_permissions();
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection.execute_batch("CREATE TRIGGER fail_obligation_update_audit BEFORE INSERT ON audit_events WHEN NEW.event_type = 'OBLIGATION_UPDATED' BEGIN SELECT RAISE(ABORT, 'forced audit failure'); END;").expect("محفز اختبار الفشل يجب أن ينشأ");
        }

        assert!(matches!(
            update_obligation(&state, update_input(1)),
            Err(AppError::StorageOperation)
        ));

        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let persisted: (String, i64, i64) = connection.query_row("SELECT title, version, COUNT(*) OVER () FROM obligations WHERE id = 'obligation-a'", [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).expect("استعلام الالتزام يجب أن ينجح");
        assert_eq!(persisted.0, "Synthetic obligation A");
        assert_eq!(persisted.1, 1);
        let audit_count: i64 = connection.query_row("SELECT COUNT(*) FROM audit_events WHERE resource_id = 'obligation-a' AND event_type = 'OBLIGATION_UPDATED'", [], |row| row.get(0)).expect("استعلام التدقيق يجب أن ينجح");
        assert_eq!(audit_count, 0);
    }

    #[test]
    fn not_found_is_returned_for_missing_obligation() {
        let state = state_with_obligation_permissions();
        assert!(matches!(
            get_obligation(&state, "missing-obligation"),
            Err(AppError::ResourceNotFound)
        ));
    }

    // ===== 0026: تأليف الالتزام في وضع الإنتاج (UAT-03) =====
    #[test]
    fn creates_a_user_provided_obligation_bound_to_its_tenant_source() {
        let state = state_with_obligation_permissions();
        let created = create_obligation(
            &state,
            CreateObligationInput {
                source_id: "source-a".to_owned(),
                title: "تقرير اختبار إنتاجي".to_owned(),
                description: "وصف التزام أنشأه المستخدم".to_owned(),
                owner: "Privacy Officer".to_owned(),
                due_date: Some("2030-03-01T00:00:00Z".to_owned()),
                priority: ObligationPriority::High,
                applicability: "APPLIES".to_owned(),
            },
        )
        .expect("تأليف الالتزام يجب أن ينجح");

        // الصدق في المنشأ: مدخل المستخدم لا يصبح رسمياً ولا متحققاً.
        assert_eq!(created.source_type, "USER_PROVIDED");
        assert_eq!(created.verification_state, "UNVERIFIED");
        assert_eq!(created.status, "OPEN");

        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let row: (String, String, String, String, i64) = connection
            .query_row(
                "SELECT source_type, verification_state, applicability, priority, version FROM obligations WHERE id = ?1 AND tenant_id = 'tenant-a'",
                [&created.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .expect("الالتزام المنشأ يجب أن يُقرأ");
        assert_eq!(row.0, "USER_PROVIDED");
        assert_eq!(row.1, "UNVERIFIED");
        assert_eq!(row.2, "APPLIES");
        assert_eq!(row.3, "HIGH");
        assert_eq!(row.4, 1);
        let audits: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE resource_id = ?1 AND event_type = 'OBLIGATION_CREATED'",
                [&created.id],
                |row| row.get(0),
            )
            .expect("التدقيق يجب أن يُقرأ");
        assert_eq!(audits, 1);
        // الالتزام المنشأ يظهر في القائمة المأذونة (مسار الواجهة نفسه).
        drop(connection);
        let listed = list_obligations(&state).expect("القائمة يجب أن تُقرأ");
        assert!(listed.items.iter().any(|item| item.id == created.id));
    }

    #[test]
    fn obligation_creation_rejects_foreign_source_and_invalid_input_without_rows() {
        let state = state_with_obligation_permissions();
        let before: i64 = state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .query_row("SELECT COUNT(*) FROM obligations", [], |row| row.get(0))
            .expect("العدّ يجب أن ينجح");

        // مصدر غير موجود في هذا المستأجر.
        assert!(matches!(
            create_obligation(
                &state,
                CreateObligationInput {
                    source_id: "source-other-tenant".to_owned(),
                    title: "عابر للمستأجرين".to_owned(),
                    description: String::new(),
                    owner: "Owner".to_owned(),
                    due_date: None,
                    priority: ObligationPriority::Low,
                    applicability: "APPLIES".to_owned(),
                },
            ),
            Err(AppError::ObligationSourceInvalid)
        ));
        // انطباق خارج التعداد المُعلن.
        assert!(matches!(
            create_obligation(
                &state,
                CreateObligationInput {
                    source_id: "source-a".to_owned(),
                    title: "انطباق غير صالح".to_owned(),
                    description: String::new(),
                    owner: "Owner".to_owned(),
                    due_date: None,
                    priority: ObligationPriority::Low,
                    applicability: "MADE_UP".to_owned(),
                },
            ),
            Err(AppError::ValidationFailed)
        ));
        // عنوان فارغ وتاريخ غير صالح.
        assert!(matches!(
            create_obligation(
                &state,
                CreateObligationInput {
                    source_id: "source-a".to_owned(),
                    title: "   ".to_owned(),
                    description: String::new(),
                    owner: "Owner".to_owned(),
                    due_date: None,
                    priority: ObligationPriority::Low,
                    applicability: "UNCERTAIN".to_owned(),
                },
            ),
            Err(AppError::ValidationFailed)
        ));
        assert!(matches!(
            create_obligation(
                &state,
                CreateObligationInput {
                    source_id: "source-a".to_owned(),
                    title: "تاريخ غير صالح".to_owned(),
                    description: String::new(),
                    owner: "Owner".to_owned(),
                    due_date: Some("not-a-date".to_owned()),
                    priority: ObligationPriority::Low,
                    applicability: "UNCERTAIN".to_owned(),
                },
            ),
            Err(AppError::ValidationFailed)
        ));

        let after: i64 = state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .query_row("SELECT COUNT(*) FROM obligations", [], |row| row.get(0))
            .expect("العدّ يجب أن ينجح");
        assert_eq!(before, after, "كل رفض يجب ألا يترك صفاً جزئياً");
    }

    #[test]
    fn obligation_creation_needs_its_own_create_permission() {
        let state = state_with_obligation_permissions();
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute(
                    "DELETE FROM role_permissions WHERE role_id = 'role-a' AND permission_id = 'perm-create'",
                    [],
                )
                .expect("سحب صلاحية الإنشاء يجب أن ينجح");
        }
        assert!(matches!(
            create_obligation(
                &state,
                CreateObligationInput {
                    source_id: "source-a".to_owned(),
                    title: "بلا صلاحية".to_owned(),
                    description: String::new(),
                    owner: "Owner".to_owned(),
                    due_date: None,
                    priority: ObligationPriority::Low,
                    applicability: "APPLIES".to_owned(),
                },
            ),
            Err(AppError::AuthorizationDenied)
        ));
    }
}
