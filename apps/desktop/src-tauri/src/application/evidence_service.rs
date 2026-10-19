use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        models::{
            evidence_status_code, parse_evidence_status, EvidenceData, EvidenceRecord,
            EvidenceStatus,
        },
    },
    infrastructure::{
        database::{append_audit_event, latest_audit_event_hash, AppState},
        evidence_repository,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterEvidenceReferenceInput {
    pub obligation_id: String,
    pub label: String,
    pub classification: String,
    pub expires_at: Option<String>,
    pub content_fingerprint: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredEvidenceReference {
    pub evidence_id: String,
    pub status: String,
    pub verification_state: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateEvidenceStatusInput {
    pub evidence_id: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatedEvidenceStatus {
    pub evidence_id: String,
    pub obligation_id: String,
    pub status: EvidenceStatus,
    pub version: i64,
    pub obligation_evidence_status: String,
    pub updated_at: String,
}

// نقل الدليل عبر دورة حياته الفعلية: تفويض ← تحقق ← معاملة واحدة ← تدقيق ← تثبيت.
// لا يقبل أي حقل ملكية من الواجهة (لا تصنيف ولا التزام ولا مستأجر) — الحالة وحدها.
// لا آلة حالات معرّفة في النموذج القائم: القائمة البيضاء هي تعداد EvidenceStatus حصراً،
// والاسم يُحلّ بالمحلّل القائم نفسه (parse_evidence_status) فلا تُخترع حالات جديدة.
pub fn update_evidence_status(
    state: &AppState,
    input: UpdateEvidenceStatusInput,
) -> Result<UpdatedEvidenceStatus, AppError> {
    authorization_service::require_permission(state, "EVIDENCE", "UPDATE")?;

    let evidence_id = input.evidence_id.trim();
    if evidence_id.is_empty() || evidence_id.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    // محلّل الحالة القائم: أي قيمة خارج التعداد تُرفض بنفس خطأ المحلّل (فشل مغلق).
    let status = parse_evidence_status(input.status.trim().to_owned())?;
    let status_code = evidence_status_code(&status);

    let now = Utc::now().to_rfc3339();
    let audit_id = format!("audit_{}", Uuid::new_v4());
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    // ملكية المستأجر للدليل شرط صريح قبل أي كتابة؛ غيابها قرار معلن لا خطأ تخزين.
    let (obligation_id, registered_by, expires_at) = transaction
        .query_row(
            "SELECT obligation_id, registered_by, expires_at FROM evidence_items WHERE id = ?1 AND tenant_id = ?2",
            params![evidence_id, &state.tenant_id()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => AppError::ResourceNotFound,
            _ => AppError::StorageOperation,
        })?;

    // ===== منع اعتماد دليل منتهي الصلاحية =====
    // الانتهاء ليس حالة تُكتب فوق السجل بل قيد على الانتقال: يُرفض أي نقل إلى
    // حالة "مقبولة" بعد انقضاء الأجل برمز صريح، ويبقى الصف شاهداً على حالته.
    let is_expired = expires_at
        .as_deref()
        .is_some_and(|expiry| expiry <= now.as_str());
    if is_expired
        && matches!(
            status_code,
            "UPLOADED" | "PROCESSING" | "ANALYZED" | "VALID"
        )
    {
        return Err(AppError::EvidenceExpired);
    }

    // فصل المهام (UAT-04): من قدّم الدليل لا يقرر حالة التحقق منه. الضابط هنا
    // في الخلف فلا يكفي إخفاء القائمة في الواجهة، ويُطبَّق على كل نقل حالة —
    // بما فيه الرفض — لأن كلا الاتجاهين قرار تحقق.
    let actor_id = state.current_user_id()?;
    if registered_by.as_deref() == Some(actor_id.as_str()) {
        return Err(AppError::EvidenceSelfVerificationForbidden);
    }

    let changed = transaction
        .execute(
            "UPDATE evidence_items SET status = ?1, updated_at = ?2, version = version + 1 WHERE id = ?3 AND tenant_id = ?4",
            params![status_code, &now, evidence_id, &state.tenant_id()],
        )
        .map_err(|_| AppError::StorageOperation)?;
    if changed != 1 {
        return Err(AppError::ResourceNotFound);
    }

    // إعادة حساب حالة دليل الالتزام المالك بنفس قاعدة مسار التسجيل القائم (دالة مشتركة).
    let obligation_evidence_status = evidence_repository::obligation_evidence_status(
        &transaction,
        &state.tenant_id(),
        &obligation_id,
    )?;
    transaction
        .execute(
            "UPDATE obligations SET evidence_status = ?1, updated_at = ?2 WHERE id = ?3 AND tenant_id = ?4",
            params![&obligation_evidence_status, &now, &obligation_id, &state.tenant_id()],
        )
        .map_err(|_| AppError::StorageOperation)?;

    let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
    append_audit_event(
        &transaction,
        &state.tenant_id(),
        &audit_id,
        "EVIDENCE_STATUS_UPDATED",
        &state.current_user_id()?,
        "Evidence",
        evidence_id,
        &now,
        &previous_hash,
    )?;
    let version: i64 = transaction
        .query_row(
            "SELECT version FROM evidence_items WHERE id = ?1 AND tenant_id = ?2",
            params![evidence_id, &state.tenant_id()],
            |row| row.get(0),
        )
        .map_err(|_| AppError::StorageOperation)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(UpdatedEvidenceStatus {
        evidence_id: evidence_id.to_owned(),
        obligation_id,
        status,
        version,
        obligation_evidence_status,
        updated_at: now,
    })
}

// تفاصيل دليل واحد بنفس نموذج القراءة القائم — قراءة محضة بلا أي أثر تدقيقي.
pub fn get_evidence_detail(
    state: &AppState,
    evidence_id: String,
) -> Result<EvidenceRecord, AppError> {
    authorization_service::require_permission(state, "EVIDENCE", "READ")?;

    let evidence_id = evidence_id.trim();
    if evidence_id.is_empty() || evidence_id.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    evidence_repository::load_evidence_detail(&connection, &state.tenant_id(), evidence_id)
}

pub fn load_evidence(state: &AppState) -> Result<EvidenceData, AppError> {
    authorization_service::require_permission(state, "EVIDENCE", "READ")?;

    let generated_at = Utc::now().to_rfc3339();
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    evidence_repository::load_evidence(&connection, &state.tenant_id(), &generated_at)
}

pub fn register_evidence_reference(
    state: &AppState,
    input: RegisterEvidenceReferenceInput,
) -> Result<RegisteredEvidenceReference, AppError> {
    authorization_service::require_permission(state, "EVIDENCE", "CREATE")?;
    validate_input(&input)?;

    let now = Utc::now().to_rfc3339();
    let evidence_id = format!("evidence_{}", Uuid::new_v4());
    let audit_id = format!("audit_{}", Uuid::new_v4());
    let fingerprint_hash = format!(
        "{:x}",
        Sha256::digest(input.content_fingerprint.trim().as_bytes())
    );
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let obligation_exists = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM obligations WHERE id = ?1 AND tenant_id = ?2)",
            params![&input.obligation_id, &state.tenant_id()],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| AppError::StorageOperation)?;
    if !obligation_exists {
        return Err(AppError::ResourceNotFound);
    }

    transaction
        .execute(
            "INSERT INTO evidence_items (id, tenant_id, obligation_id, classification, status, content_hash, version, created_at, updated_at, label, expires_at, registered_by) VALUES (?1, ?2, ?3, ?4, 'REQUIRES_REVIEW', ?5, 1, ?6, ?6, ?7, ?8, ?9)",
            params![&evidence_id, &state.tenant_id(), &input.obligation_id, &input.classification, &fingerprint_hash, &now, input.label.trim(), &input.expires_at, &state.current_user_id()?],
        )
        .map_err(|_| AppError::StorageOperation)?;
    transaction
        .execute(
            "UPDATE obligations SET evidence_status = 'REQUIRES_REVIEW', updated_at = ?1 WHERE id = ?2 AND tenant_id = ?3",
            params![&now, &input.obligation_id, &state.tenant_id()],
        )
        .map_err(|_| AppError::StorageOperation)?;

    let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
    append_audit_event(
        &transaction,
        &state.tenant_id(),
        &audit_id,
        "EVIDENCE_REFERENCE_REGISTERED",
        &state.current_user_id()?,
        "Evidence",
        &evidence_id,
        &now,
        &previous_hash,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(RegisteredEvidenceReference {
        evidence_id,
        status: "REQUIRES_REVIEW".to_owned(),
        verification_state: "REQUIRES_REVIEW".to_owned(),
    })
}

fn validate_input(input: &RegisterEvidenceReferenceInput) -> Result<(), AppError> {
    let classification_is_valid = matches!(
        input.classification.as_str(),
        "PUBLIC" | "INTERNAL" | "CONFIDENTIAL" | "RESTRICTED" | "HIGHLY_RESTRICTED"
    );
    if input.obligation_id.trim().is_empty()
        || input.label.trim().is_empty()
        || input.label.chars().count() > 180
        || input.content_fingerprint.trim().is_empty()
        || input.content_fingerprint.chars().count() > 1_000
        || !classification_is_valid
    {
        return Err(AppError::ValidationFailed);
    }
    // صلاحية الدليل: التاريخ المعلن يجب أن يكون صيغة RFC 3339 صحيحة **ومستقبلاً**.
    // تاريخ منقضٍ عند التسجيل يعني دليلاً ميتاً من لحظته، وقبوله يلوّث الجاهزية.
    if let Some(expires_at) = input.expires_at.as_deref() {
        let trimmed = expires_at.trim();
        if trimmed.is_empty() || trimmed.chars().count() > 64 {
            return Err(AppError::ValidationFailed);
        }
        let declared = chrono::DateTime::parse_from_rfc3339(trimmed)
            .map_err(|_| AppError::ValidationFailed)?;
        if declared <= Utc::now() {
            return Err(AppError::EvidenceExpired);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    use chrono::Utc;
    use rusqlite::Connection;

    use super::{register_evidence_reference, RegisterEvidenceReferenceInput};
    use crate::{domain::error::AppError, infrastructure::database::AppState};
    fn state_with_evidence_permission() -> AppState {
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
            .execute_batch(include_str!("../../migrations/0004_evidence_metadata.sql"))
            .expect("هجرة الأدلة يجب أن تنجح");
        // 0026: عمود `registered_by` مطلوب لمسار التسجيل الحقيقي.
        connection
            .execute_batch(include_str!(
                "../../migrations/0026_production_user_and_authoring.sql"
            ))
            .expect("هجرة فصل الأدلة يجب أن تنجح");
        let now = Utc::now().to_rfc3339();
        connection.execute("INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant-evidence', 'Evidence tenant', 'DEMO_SYNTHETIC', ?1, ?1)", [&now]).expect("المستأجر يجب أن ينشأ");
        connection.execute("INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES ('user-evidence', 'tenant-evidence', 'Evidence User', 'evidence@test.local', NULL, 'ACTIVE', ?1, ?1)", [&now]).expect("المستخدم يجب أن ينشأ");
        connection.execute("INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-evidence', 'tenant-evidence', 'TENANT_ADMIN', 'Tenant Admin', ?1, ?1)", [&now]).expect("الدور يجب أن ينشأ");
        connection.execute("INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES ('permission-evidence', 'evidence:create', 'EVIDENCE', 'CREATE', 'HIGHLY_RESTRICTED', ?1)", [&now]).expect("الصلاحية يجب أن تنشأ");
        connection.execute("INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user-evidence', 'role-evidence', ?1, 'system')", [&now]).expect("تعيين الدور يجب أن ينجح");
        connection.execute("INSERT INTO role_permissions (role_id, permission_id) VALUES ('role-evidence', 'permission-evidence')", []).expect("تعيين الصلاحية يجب أن ينجح");
        connection.execute("INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session-evidence', 'tenant-evidence', 'user-evidence', ?1, '2030-01-01T00:00:00Z', NULL, 'test')", [&now]).expect("الجلسة يجب أن تنشأ");
        connection.execute("INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES ('source-evidence', 'tenant-evidence', 'Synthetic', 'Synthetic source', NULL, '1', ?1, 0, 'UNVERIFIED', ?1, ?1)", [&now]).expect("المصدر يجب أن ينشأ");
        connection.execute("INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, created_at, updated_at) VALUES ('obligation-evidence', 'tenant-evidence', 'source-evidence', 'Synthetic obligation', 'Owner', NULL, 'HIGH', 'OPEN', 'APPLIES', 'MISSING', 'UNVERIFIED', ?1, ?1)", [&now]).expect("الالتزام يجب أن ينشأ");

        AppState::test_instance(
            connection,
            "tenant-evidence".to_owned(),
            "user-evidence".to_owned(),
        )
    }

    #[test]
    fn cannot_register_evidence_for_another_tenants_obligation() {
        let state = state_with_evidence_permission();
        let now = Utc::now().to_rfc3339();
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection.execute("INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant-evidence-other', 'Other tenant', 'DEMO_SYNTHETIC', ?1, ?1)", [&now]).expect("المستأجر الآخر يجب أن ينشأ");
            connection.execute("INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES ('source-evidence-other', 'tenant-evidence-other', 'Synthetic', 'Other source', NULL, '1', ?1, 0, 'UNVERIFIED', ?1, ?1)", [&now]).expect("مصدر المستأجر الآخر يجب أن ينشأ");
            connection.execute("INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, created_at, updated_at) VALUES ('obligation-evidence-other', 'tenant-evidence-other', 'source-evidence-other', 'Other obligation', 'Owner', NULL, 'HIGH', 'OPEN', 'APPLIES', 'MISSING', 'UNVERIFIED', ?1, ?1)", [&now]).expect("التزام المستأجر الآخر يجب أن ينشأ");
        }

        let result = register_evidence_reference(
            &state,
            RegisterEvidenceReferenceInput {
                obligation_id: "obligation-evidence-other".to_owned(),
                label: "Cross tenant evidence".to_owned(),
                classification: "INTERNAL".to_owned(),
                expires_at: None,
                content_fingerprint: "cross-tenant-reference".to_owned(),
            },
        );

        assert!(matches!(result, Err(AppError::ResourceNotFound)));
    }

    #[test]
    fn registers_reference_marks_obligation_for_review_and_audits_the_action() {
        let state = state_with_evidence_permission();
        let registered = register_evidence_reference(
            &state,
            RegisterEvidenceReferenceInput {
                obligation_id: "obligation-evidence".to_owned(),
                label: "Synthetic evidence register entry".to_owned(),
                classification: "INTERNAL".to_owned(),
                expires_at: None,
                content_fingerprint: "reference-2026-08".to_owned(),
            },
        )
        .expect("تسجيل المرجع المصرح به يجب أن ينجح");

        assert_eq!(registered.status, "REQUIRES_REVIEW");
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let evidence_count: i32 = connection
            .query_row(
                "SELECT COUNT(*) FROM evidence_items WHERE id = ?1 AND status = 'REQUIRES_REVIEW'",
                [&registered.evidence_id],
                |row| row.get(0),
            )
            .expect("استعلام الدليل يجب أن ينجح");
        let obligation_status: String = connection
            .query_row(
                "SELECT evidence_status FROM obligations WHERE id = 'obligation-evidence'",
                [],
                |row| row.get(0),
            )
            .expect("استعلام الالتزام يجب أن ينجح");
        let audit_count: i32 = connection.query_row("SELECT COUNT(*) FROM audit_events WHERE resource_id = ?1 AND event_type = 'EVIDENCE_REFERENCE_REGISTERED'", [&registered.evidence_id], |row| row.get(0)).expect("استعلام التدقيق يجب أن ينجح");
        assert_eq!(evidence_count, 1);
        assert_eq!(obligation_status, "REQUIRES_REVIEW");
        assert_eq!(audit_count, 1);
    }
}

#[cfg(test)]
mod read_tests {

    use chrono::Utc;
    use rusqlite::Connection;

    use super::load_evidence;
    use crate::{domain::error::AppError, infrastructure::database::AppState};

    fn test_state() -> AppState {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        for migration in [
            include_str!("../../migrations/0001_initial.sql"),
            include_str!("../../migrations/0002_identity_and_access.sql"),
            include_str!("../../migrations/0003_workflow_and_risk.sql"),
            include_str!("../../migrations/0004_evidence_metadata.sql"),
            include_str!("../../migrations/0005_ai_governance.sql"),
            include_str!("../../migrations/0006_correct_demo_truth_labels.sql"),
            include_str!("../../migrations/0007_obligation_management.sql"),
            include_str!("../../migrations/0008_obligation_relationship_update_isolation.sql"),
            include_str!("../../migrations/0025_task_lifecycle.sql"),
            include_str!("../../migrations/0026_production_user_and_authoring.sql"),
        ] {
            connection
                .execute_batch(migration)
                .expect("هجرة الاختبار يجب أن تنجح");
        }

        let now = Utc::now().to_rfc3339();
        for tenant_id in [
            "tenant-evidence-a",
            "tenant-evidence-b",
            "tenant-evidence-empty",
        ] {
            connection
                .execute(
                    "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?1, 'DEMO_SYNTHETIC', ?2, ?2)",
                    (tenant_id, &now),
                )
                .expect("المستأجر يجب أن ينشأ");
        }

        for tenant_id in ["tenant-evidence-a", "tenant-evidence-b"] {
            let source_id = format!("source-{tenant_id}");
            let obligation_id = format!("obligation-{tenant_id}");
            connection
                .execute(
                    "INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES (?1, ?2, 'Synthetic Authority', 'Synthetic Source', NULL, '1.0', ?3, 0, 'REQUIRES_REVIEW', ?3, ?3)",
                    (&source_id, tenant_id, &now),
                )
                .expect("المصدر يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, source_type, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'Owner', NULL, 'HIGH', 'OPEN', 'APPLIES', 'MISSING', 'REQUIRES_REVIEW', 'SYNTHETIC', ?5, ?5)",
                    (&obligation_id, tenant_id, &source_id, format!("Obligation {tenant_id}"), &now),
                )
                .expect("الالتزام يجب أن ينشأ");
        }

        insert_evidence(
            &connection,
            "evidence-a-review",
            "tenant-evidence-a",
            "obligation-tenant-evidence-a",
            "Review register entry",
            "INTERNAL",
            "REQUIRES_REVIEW",
            None,
            "2026-08-21T00:00:00Z",
        );
        insert_evidence(
            &connection,
            "evidence-a-expired",
            "tenant-evidence-a",
            "obligation-tenant-evidence-a",
            "Expired register entry",
            "CONFIDENTIAL",
            "EXPIRED",
            Some("2026-08-01T00:00:00Z"),
            "2026-08-20T00:00:00Z",
        );
        insert_evidence(
            &connection,
            "evidence-a-valid",
            "tenant-evidence-a",
            "obligation-tenant-evidence-a",
            "Valid register entry",
            "PUBLIC",
            "VALID",
            None,
            "2026-08-19T00:00:00Z",
        );
        insert_evidence(
            &connection,
            "evidence-b-only",
            "tenant-evidence-b",
            "obligation-tenant-evidence-b",
            "Tenant B register entry",
            "INTERNAL",
            "VALID",
            None,
            "2026-08-22T00:00:00Z",
        );
        create_authorized_user(
            &connection,
            "tenant-evidence-a",
            "user-evidence-a",
            "role-evidence-a",
            "session-evidence-a",
            &now,
        );

        AppState::test_instance(
            connection,
            "tenant-evidence-a".to_owned(),
            "user-evidence-a".to_owned(),
        )
    }

    fn insert_evidence(
        connection: &Connection,
        id: &str,
        tenant_id: &str,
        obligation_id: &str,
        label: &str,
        classification: &str,
        status: &str,
        expires_at: Option<&str>,
        updated_at: &str,
    ) {
        connection
            .execute(
                "INSERT INTO evidence_items (id, tenant_id, obligation_id, classification, status, content_hash, version, created_at, updated_at, label, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, ?7, ?7, ?8, ?9)",
                (id, tenant_id, obligation_id, classification, status, format!("hash-{id}"), updated_at, label, expires_at),
            )
            .expect("الدليل يجب أن ينشأ");
    }

    fn create_authorized_user(
        connection: &Connection,
        tenant_id: &str,
        user_id: &str,
        role_id: &str,
        session_id: &str,
        now: &str,
    ) {
        connection
            .execute(
                "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, NULL, 'ACTIVE', ?5, ?5)",
                (user_id, tenant_id, user_id, format!("{user_id}@test.local"), now),
            )
            .expect("المستخدم يجب أن ينشأ");
        connection
            .execute(
                "INSERT OR IGNORE INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES ('permission-evidence-read', 'evidence:read', 'EVIDENCE', 'READ', 'HIGHLY_RESTRICTED', ?1)",
                [now],
            )
            .expect("صلاحية القراءة يجب أن تنشأ");
        connection
            .execute(
                "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES (?1, ?2, 'READER', 'Reader', ?3, ?3)",
                (role_id, tenant_id, now),
            )
            .expect("الدور يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, ?2, ?3, 'system')",
                (user_id, role_id, now),
            )
            .expect("تعيين الدور يجب أن ينجح");
        connection
            .execute(
                "INSERT INTO role_permissions (role_id, permission_id) VALUES (?1, 'permission-evidence-read')",
                [role_id],
            )
            .expect("تعيين صلاحية القراءة يجب أن ينجح");
        connection
            .execute(
                "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, ?2, ?3, ?4, '2030-01-01T00:00:00Z', NULL, 'test')",
                (session_id, tenant_id, user_id, now),
            )
            .expect("الجلسة يجب أن تنشأ");
    }

    #[test]
    fn returns_only_authorized_tenant_evidence_and_derived_summary() {
        let state = test_state();

        let data = load_evidence(&state).expect("قراءة الأدلة المصرح بها يجب أن تنجح");

        assert!(data.has_data);
        assert_eq!(data.summary.total, 3);
        assert_eq!(data.summary.requires_review, 1);
        assert_eq!(data.summary.expired, 1);
        assert_eq!(
            data.items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "evidence-a-review",
                "evidence-a-expired",
                "evidence-a-valid"
            ]
        );
        assert!(data
            .items
            .iter()
            .all(|item| !item.id.contains("evidence-b")));
    }

    #[test]
    fn denies_read_without_evidence_read_permission() {
        let state = test_state();
        state
            .connection
            .lock()
            .expect("الاتصال يجب أن يتاح")
            .execute(
                "DELETE FROM role_permissions WHERE role_id = 'role-evidence-a'",
                [],
            )
            .expect("إزالة الصلاحية يجب أن تنجح");

        assert!(matches!(
            load_evidence(&state),
            Err(AppError::AuthorizationDenied)
        ));
    }

    #[test]
    fn denies_read_for_expired_revoked_and_locked_identity() {
        let state = test_state();
        {
            let connection = state.connection.lock().expect("الاتصال يجب أن يتاح");
            connection
                .execute("UPDATE local_sessions SET expires_at = '2020-01-01T00:00:00Z' WHERE id = 'session-evidence-a'", [])
                .expect("انتهاء الجلسة يجب أن ينجح");
        }
        assert!(matches!(
            load_evidence(&state),
            Err(AppError::AuthorizationDenied)
        ));

        {
            let connection = state.connection.lock().expect("الاتصال يجب أن يتاح");
            connection
                .execute("UPDATE local_sessions SET expires_at = '2030-01-01T00:00:00Z', revoked_at = '2026-01-01T00:00:00Z' WHERE id = 'session-evidence-a'", [])
                .expect("إلغاء الجلسة يجب أن ينجح");
        }
        assert!(matches!(
            load_evidence(&state),
            Err(AppError::AuthorizationDenied)
        ));

        {
            let connection = state.connection.lock().expect("الاتصال يجب أن يتاح");
            connection
                .execute(
                    "UPDATE local_sessions SET revoked_at = NULL WHERE id = 'session-evidence-a'",
                    [],
                )
                .expect("استعادة الجلسة يجب أن تنجح");
            connection
                .execute(
                    "UPDATE local_users SET status = 'LOCKED' WHERE id = 'user-evidence-a'",
                    [],
                )
                .expect("قفل المستخدم يجب أن ينجح");
        }
        assert!(matches!(
            load_evidence(&state),
            Err(AppError::AuthorizationDenied)
        ));
    }

    #[test]
    fn isolates_evidence_between_authorized_tenants() {
        let state = test_state();
        let now = Utc::now().to_rfc3339();
        {
            let connection = state.connection.lock().expect("الاتصال يجب أن يتاح");
            create_authorized_user(
                &connection,
                "tenant-evidence-b",
                "user-evidence-b",
                "role-evidence-b",
                "session-evidence-b",
                &now,
            );
        }

        state.set_active_tenant_for_tests("tenant-evidence-b");
        state.set_active_user_for_tests("user-evidence-b");
        let data = load_evidence(&state).expect("قراءة المستأجر ب يجب أن تنجح");

        assert_eq!(data.summary.total, 1);
        assert_eq!(data.items.len(), 1);
        assert_eq!(data.items[0].id, "evidence-b-only");
    }

    #[test]
    fn denies_user_when_authenticated_state_uses_another_tenant() {
        let state = test_state();
        state.set_active_tenant_for_tests("tenant-evidence-b");

        assert!(matches!(
            load_evidence(&state),
            Err(AppError::AuthorizationDenied)
        ));
    }

    #[test]
    fn returns_empty_state_without_creating_audit_events() {
        let state = test_state();
        let now = Utc::now().to_rfc3339();
        {
            let connection = state.connection.lock().expect("الاتصال يجب أن يتاح");
            create_authorized_user(
                &connection,
                "tenant-evidence-empty",
                "user-evidence-empty",
                "role-evidence-empty",
                "session-evidence-empty",
                &now,
            );
        }
        state.set_active_tenant_for_tests("tenant-evidence-empty");
        state.set_active_user_for_tests("user-evidence-empty");
        let before_count: i64 = state
            .connection
            .lock()
            .expect("الاتصال يجب أن يتاح")
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
            .expect("عدد التدقيق يجب أن يقرأ");

        let data = load_evidence(&state).expect("المستأجر الفارغ يجب أن يقرأ");
        let after_count: i64 = state
            .connection
            .lock()
            .expect("الاتصال يجب أن يتاح")
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
            .expect("عدد التدقيق يجب أن يقرأ");

        assert!(!data.has_data);
        assert_eq!(data.summary.total, 0);
        assert!(data.items.is_empty());
        assert_eq!(before_count, after_count);
    }

    #[test]
    fn rejects_invalid_persisted_evidence_classification() {
        let state = test_state();
        {
            let connection = state.connection.lock().expect("الاتصال يجب أن يتاح");
            connection
                .execute_batch("PRAGMA ignore_check_constraints = ON;")
                .expect("تعطيل القيود المؤقت يجب أن ينجح");
            connection
                .execute("UPDATE evidence_items SET classification = 'UNKNOWN' WHERE id = 'evidence-a-review'", [])
                .expect("تخزين القيمة غير الصحيحة للاختبار يجب أن ينجح");
            connection
                .execute_batch("PRAGMA ignore_check_constraints = OFF;")
                .expect("استعادة القيود يجب أن تنجح");
        }

        assert!(matches!(load_evidence(&state), Err(AppError::DataAccess)));
    }

    #[test]
    fn returns_evidence_in_a_deterministic_order_and_is_read_only() {
        let state = test_state();
        let before_count: i64 = state
            .connection
            .lock()
            .expect("الاتصال يجب أن يتاح")
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
            .expect("عدد التدقيق يجب أن يقرأ");

        let first = load_evidence(&state).expect("القراءة الأولى يجب أن تنجح");
        let second = load_evidence(&state).expect("القراءة الثانية يجب أن تنجح");
        let after_count: i64 = state
            .connection
            .lock()
            .expect("الاتصال يجب أن يتاح")
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
            .expect("عدد التدقيق يجب أن يقرأ");

        assert_eq!(
            first
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            second
                .items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
        );
        assert_eq!(before_count, after_count);
    }
}

// ===== دورة حياة حالة الدليل (0022 / evidence:update) =====
#[cfg(test)]
mod lifecycle_tests {

    use super::{
        get_evidence_detail, register_evidence_reference, update_evidence_status,
        RegisterEvidenceReferenceInput, UpdateEvidenceStatusInput,
    };
    use crate::{
        application::tests_test_fixture::{
            Fixture, EVIDENCE_A, EVIDENCE_B, OBLIGATION_A, ROLE_A, TENANT_A,
        },
        domain::error::AppError,
    };

    const UPDATE_PERMISSIONS: [&str; 2] =
        ["permission_evidence_read", "permission_evidence_update"];

    fn update(fixture: &Fixture, evidence_id: &str, status: &str) -> Result<(), AppError> {
        update_evidence_status(
            &fixture.state,
            UpdateEvidenceStatusInput {
                evidence_id: evidence_id.to_owned(),
                status: status.to_owned(),
            },
        )
        .map(|_| ())
    }

    // مسار سعيد: الحالة تتغير، النسخة تزيد بمقدار واحد بالضبط، حدث تدقيق واحد،
    // وتجميع حالة دليل الالتزام يُعاد حسابه بنفس قاعدة مسار التسجيل القائم.
    #[test]
    fn updates_status_bumps_version_once_and_audits_exactly_one_event() {
        let fixture = Fixture::new();
        fixture.grant(ROLE_A, &UPDATE_PERMISSIONS);
        assert_eq!(fixture.evidence_version(EVIDENCE_A), 1);
        assert_eq!(fixture.obligation_evidence_status(OBLIGATION_A), "MISSING");

        let updated = update_evidence_status(
            &fixture.state,
            UpdateEvidenceStatusInput {
                evidence_id: EVIDENCE_A.to_owned(),
                status: "VALID".to_owned(),
            },
        )
        .expect("تحديث الحالة المصرح به يجب أن ينجح");

        assert_eq!(updated.evidence_id, EVIDENCE_A);
        assert_eq!(updated.obligation_id, OBLIGATION_A);
        assert_eq!(updated.version, 2);
        assert_eq!(updated.obligation_evidence_status, "REQUIRES_REVIEW");
        assert_eq!(fixture.evidence_status(EVIDENCE_A), "VALID");
        assert_eq!(fixture.evidence_version(EVIDENCE_A), 2);
        assert_eq!(
            fixture.obligation_evidence_status(OBLIGATION_A),
            "REQUIRES_REVIEW"
        );
        assert_eq!(
            fixture.audit_count("EVIDENCE_STATUS_UPDATED", EVIDENCE_A),
            1
        );
        assert_eq!(fixture.count("SELECT COUNT(*) FROM audit_events"), 1);
    }

    #[test]
    fn denies_status_update_without_the_update_permission_and_writes_nothing() {
        let fixture = Fixture::new();
        fixture.revoke(ROLE_A, "permission_evidence_update");

        assert!(matches!(
            update(&fixture, EVIDENCE_A, "VALID"),
            Err(AppError::AuthorizationDenied)
        ));
        assert_eq!(fixture.evidence_status(EVIDENCE_A), "REQUIRES_REVIEW");
        assert_eq!(fixture.evidence_version(EVIDENCE_A), 1);
        assert_eq!(fixture.count("SELECT COUNT(*) FROM audit_events"), 0);
    }

    // معرّف دليل مستأجر آخر موجود فعلاً لكنه غير مرئي هنا: قرار "غير موجود" بلا أي كتابة.
    #[test]
    fn cross_tenant_evidence_id_is_not_found_and_writes_nothing() {
        let fixture = Fixture::new();
        fixture.grant(ROLE_A, &UPDATE_PERMISSIONS);

        assert!(matches!(
            update(&fixture, EVIDENCE_B, "VALID"),
            Err(AppError::ResourceNotFound)
        ));
        assert_eq!(fixture.evidence_status(EVIDENCE_B), "REQUIRES_REVIEW");
        assert_eq!(fixture.evidence_version(EVIDENCE_B), 1);
        assert_eq!(fixture.count("SELECT COUNT(*) FROM audit_events"), 0);
    }

    #[test]
    fn unknown_evidence_id_is_not_found_and_writes_nothing() {
        let fixture = Fixture::new();
        fixture.grant(ROLE_A, &UPDATE_PERMISSIONS);

        assert!(matches!(
            update(&fixture, "evidence-does-not-exist", "VALID"),
            Err(AppError::ResourceNotFound)
        ));
        assert_eq!(fixture.count("SELECT COUNT(*) FROM audit_events"), 0);
        assert_eq!(fixture.obligation_evidence_status(OBLIGATION_A), "MISSING");
    }

    // قيمة خارج تعداد EvidenceStatus تُرفض بنفس خطأ المحلّل القائم (DataAccess)،
    // والقيمة الفارغة تُرفض كتحقق مدخلات — كلاهما قبل أي كتابة.
    #[test]
    fn rejects_status_values_outside_the_existing_enum() {
        let fixture = Fixture::new();
        fixture.grant(ROLE_A, &UPDATE_PERMISSIONS);

        assert!(matches!(
            update(&fixture, EVIDENCE_A, "ARCHIVED"),
            Err(AppError::DataAccess)
        ));
        assert!(matches!(
            update(&fixture, EVIDENCE_A, ""),
            Err(AppError::DataAccess)
        ));
        assert!(matches!(
            update(&fixture, EVIDENCE_A, "valid"),
            Err(AppError::DataAccess)
        ));
        assert_eq!(fixture.evidence_status(EVIDENCE_A), "REQUIRES_REVIEW");
        assert_eq!(fixture.evidence_version(EVIDENCE_A), 1);
        assert_eq!(fixture.count("SELECT COUNT(*) FROM audit_events"), 0);
    }

    #[test]
    fn rejects_empty_and_oversized_evidence_identifiers() {
        let fixture = Fixture::new();
        fixture.grant(ROLE_A, &UPDATE_PERMISSIONS);

        assert!(matches!(
            update(&fixture, "   ", "VALID"),
            Err(AppError::ValidationFailed)
        ));
        let oversized = "e".repeat(129);
        assert!(matches!(
            update(&fixture, &oversized, "VALID"),
            Err(AppError::ValidationFailed)
        ));
        assert_eq!(fixture.count("SELECT COUNT(*) FROM audit_events"), 0);
    }

    // التفاصيل قراءة محضة: نفس صف القائمة، بلا أي أثر تدقيقي، ومقيّدة بالمستأجر النشط.
    #[test]
    fn returns_evidence_detail_without_auditing_and_isolates_tenants() {
        let fixture = Fixture::new();
        fixture.grant(ROLE_A, &UPDATE_PERMISSIONS);

        let detail = get_evidence_detail(&fixture.state, EVIDENCE_A.to_owned())
            .expect("قراءة تفاصيل الدليل يجب أن تنجح");
        assert_eq!(detail.id, EVIDENCE_A);
        assert_eq!(detail.obligation_id, OBLIGATION_A);
        assert_eq!(detail.obligation_title, "Obligation a");
        assert_eq!(detail.label, "Evidence a");
        assert_eq!(
            serde_json::to_value(&detail.status).expect("تسلسل الحالة يجب أن ينجح"),
            "REQUIRES_REVIEW"
        );
        assert_eq!(fixture.count("SELECT COUNT(*) FROM audit_events"), 0);

        assert!(matches!(
            get_evidence_detail(&fixture.state, EVIDENCE_B.to_owned()),
            Err(AppError::ResourceNotFound)
        ));

        fixture.switch_to_tenant_b();
        assert!(matches!(
            get_evidence_detail(&fixture.state, EVIDENCE_A.to_owned()),
            Err(AppError::ResourceNotFound)
        ));
    }

    // UNIQUE(tenant_id, content_hash, version) لا يمنع التكرار فحسب، بل يمنع وجود صفّين
    // بنفس البصمة في نفس الإصدار داخل المستأجر — والمُحدِّث القائم يقبل INSERT عند الإصدار 1
    // فقط ولا يقبل UPDATE/DELETE لعمودي الهوية. لذا لا يمكن بنيوياً أن يوجد صفّ آخر
    // يحتجز الإصدار التالي لنفس البصمة، وارتفاع version لا يصطدم بأي قيد.
    // يثبت هذا الاختبار المنع فعلياً على مستوى المخطط، ويبقى فشل التحديث مغلقاً بلا أثر جزئي.
    #[test]
    fn content_hash_uniqueness_makes_the_version_bump_collision_free() {
        let fixture = Fixture::new();
        // محاولة إنشاء صفّ ثانٍ بنفس (المستأجر، البصمة، الإصدار) — يرفضها المخطط.
        let duplicate_insert = fixture
            .state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح")
            .execute(
                "INSERT INTO evidence_items (id, tenant_id, obligation_id, classification, status, content_hash, version, created_at, updated_at, label, expires_at) VALUES ('evidence-duplicate-hash', ?1, ?2, 'INTERNAL', 'REQUIRES_REVIEW', (SELECT content_hash FROM evidence_items WHERE id = ?3), (SELECT version FROM evidence_items WHERE id = ?3), '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z', 'Duplicate hash evidence', NULL)",
                (TENANT_A, OBLIGATION_A, EVIDENCE_A),
            );
        assert!(
            duplicate_insert.is_err(),
            "قيد (tenant_id, content_hash, version) يجب أن يمنع الصفّ المكرر"
        );

        // ولا يوجد دليل بالمستأجر يحجز الإصدار 2 بنفس البصمة.
        let conflicting_rows: i64 = fixture
            .state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح")
            .query_row(
                "SELECT COUNT(*) FROM evidence_items
                  WHERE tenant_id = ?1
                    AND content_hash = (SELECT content_hash FROM evidence_items WHERE id = ?2)
                    AND version = 2",
                (TENANT_A, EVIDENCE_A),
                |row| row.get(0),
            )
            .expect("فحص التعارض يجب أن ينجح");
        assert_eq!(conflicting_rows, 0);

        // لذلك الانتقال ينجح فعلاً ويرفع النسخة، والتدقيق يبقى حدثاً واحداً.
        update(&fixture, EVIDENCE_A, "VALID").expect("الانتقال يجب أن ينجح");
        assert_eq!(fixture.evidence_version(EVIDENCE_A), 2);
        assert_eq!(fixture.evidence_status(EVIDENCE_A), "VALID");
        assert_eq!(
            fixture.audit_count("EVIDENCE_STATUS_UPDATED", EVIDENCE_A),
            1
        );
    }

    #[test]
    fn denies_evidence_detail_read_without_the_read_permission() {
        let fixture = Fixture::new();
        fixture.revoke(ROLE_A, "permission_evidence_read");

        assert!(matches!(
            get_evidence_detail(&fixture.state, EVIDENCE_A.to_owned()),
            Err(AppError::AuthorizationDenied)
        ));
    }

    // سحب الصلاحية يسري فوراً على الطلب التالي — لا اعتماد على نتيجة سابقة.
    #[test]
    fn revoking_the_update_permission_stops_the_next_transition() {
        let fixture = Fixture::new();
        fixture.grant(ROLE_A, &UPDATE_PERMISSIONS);
        update(&fixture, EVIDENCE_A, "VALID").expect("التحديث المصرح به يجب أن ينجح");
        assert_eq!(fixture.evidence_status(EVIDENCE_A), "VALID");

        fixture.revoke(ROLE_A, "permission_evidence_update");

        assert!(matches!(
            update(&fixture, EVIDENCE_A, "INVALID"),
            Err(AppError::AuthorizationDenied)
        ));
        assert_eq!(fixture.evidence_status(EVIDENCE_A), "VALID");
        assert_eq!(
            fixture.audit_count("EVIDENCE_STATUS_UPDATED", EVIDENCE_A),
            1
        );
    }

    // ===== 0026 / UAT-04: فصل من قدّم الدليل عن من يقرر حالته =====
    #[test]
    fn the_registrant_cannot_decide_the_verification_status_of_their_own_evidence() {
        let fixture = Fixture::new();
        fixture.grant(ROLE_A, &UPDATE_PERMISSIONS);
        // منفّذ العمل يسجّل الدليل بنفسه — التسجيل يثبت هويته في الصف.
        let registered = register_evidence_reference(
            &fixture.state,
            RegisterEvidenceReferenceInput {
                obligation_id: OBLIGATION_A.to_owned(),
                label: "Self-registered evidence".to_owned(),
                classification: "INTERNAL".to_owned(),
                expires_at: None,
                content_fingerprint: "self-registration-fingerprint".to_owned(),
            },
        )
        .expect("تسجيل الدليل يجب أن ينجح");

        // محاولة ترقية ذاتية: رفض خلفي صريح وبلا كتابة.
        assert!(matches!(
            update(&fixture, &registered.evidence_id, "VALID"),
            Err(AppError::EvidenceSelfVerificationForbidden)
        ));
        assert_eq!(
            fixture.evidence_status(&registered.evidence_id),
            "REQUIRES_REVIEW"
        );
        assert_eq!(fixture.evidence_version(&registered.evidence_id), 1);
        assert_eq!(
            fixture.audit_count("EVIDENCE_STATUS_UPDATED", &registered.evidence_id),
            0
        );

        // مراجع مستقل داخل المؤسسة نفسها يقرر الحالة فعلياً.
        fixture.add_tenant_a_reviewer();
        fixture.switch_to_tenant_a_reviewer();
        update(&fixture, &registered.evidence_id, "VALID")
            .expect("قرار المراجع المستقل يجب أن ينجح");
        assert_eq!(fixture.evidence_status(&registered.evidence_id), "VALID");
        assert_eq!(fixture.evidence_version(&registered.evidence_id), 2);
        assert_eq!(
            fixture.audit_count("EVIDENCE_STATUS_UPDATED", &registered.evidence_id),
            1
        );
    }

    // ===== دورة حياة صلاحية الدليل (جولة الإغلاق النهائية) =====
    // `expires_at` كان يُخزَّن ولا يُفرض: يُقبل تاريخ ماضٍ، ويُعتمد دليل منتهٍ.
    #[test]
    fn rejects_an_expiry_in_the_past_or_a_malformed_one() {
        let fixture = Fixture::new();
        let past = register_evidence_reference(
            &fixture.state,
            RegisterEvidenceReferenceInput {
                obligation_id: OBLIGATION_A.to_owned(),
                label: "Evidence with a past expiry".to_owned(),
                classification: "INTERNAL".to_owned(),
                expires_at: Some("2000-01-01T00:00:00Z".to_owned()),
                content_fingerprint: "past-expiry-fingerprint".to_owned(),
            },
        );
        assert!(matches!(past, Err(AppError::EvidenceExpired)));

        let malformed = register_evidence_reference(
            &fixture.state,
            RegisterEvidenceReferenceInput {
                obligation_id: OBLIGATION_A.to_owned(),
                label: "Evidence with an invalid expiry".to_owned(),
                classification: "INTERNAL".to_owned(),
                expires_at: Some("not-a-date".to_owned()),
                content_fingerprint: "invalid-expiry-fingerprint".to_owned(),
            },
        );
        assert!(matches!(malformed, Err(AppError::ValidationFailed)));

        // تاريخ مستقبلي صحيح يُقبل ويُخزَّن كما هو.
        let future = register_evidence_reference(
            &fixture.state,
            RegisterEvidenceReferenceInput {
                obligation_id: OBLIGATION_A.to_owned(),
                label: "Evidence with a future expiry".to_owned(),
                classification: "INTERNAL".to_owned(),
                expires_at: Some("2999-01-01T00:00:00Z".to_owned()),
                content_fingerprint: "future-expiry-fingerprint".to_owned(),
            },
        )
        .expect("التاريخ المستقبلي الصحيح يجب أن يُقبل");
        let detail = super::get_evidence_detail(&fixture.state, future.evidence_id)
            .expect("قراءة التفاصيل يجب أن تنجح");
        assert_eq!(detail.expires_at.as_deref(), Some("2999-01-01T00:00:00Z"));
        assert!(!detail.is_expired);
    }

    #[test]
    fn refuses_to_approve_expired_evidence_without_rewriting_the_stored_status() {
        let fixture = Fixture::new();
        fixture.grant(ROLE_A, &UPDATE_PERMISSIONS);
        let original_status = fixture.evidence_status(EVIDENCE_A);
        // EVIDENCE_A سجل قديم بلا `registered_by` فالمراجعة الذاتية ليست ما يُختبر هنا.
        fixture
            .state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .execute(
                "UPDATE evidence_items SET expires_at = '2000-01-01T00:00:00Z' WHERE id = ?1",
                [EVIDENCE_A],
            )
            .expect("ضبط أجل منقضٍ يجب أن ينجح");

        assert!(matches!(
            update(&fixture, EVIDENCE_A, "VALID"),
            Err(AppError::EvidenceExpired)
        ));
        assert_eq!(
            fixture.evidence_status(EVIDENCE_A),
            original_status,
            "الرفض يجب ألا يكتب حالة جديدة"
        );
        assert_eq!(
            fixture.audit_count("EVIDENCE_STATUS_UPDATED", EVIDENCE_A),
            0,
            "الرفض يجب ألا ينتج حدث تدقيق"
        );

        // تسجيل الانتهاء نفسه مسموح: هو وصف للواقع لا اعتماد له.
        update(&fixture, EVIDENCE_A, "EXPIRED").expect("تسجيل الانتهاء يجب أن ينجح");
        assert_eq!(fixture.evidence_status(EVIDENCE_A), "EXPIRED");
    }

    #[test]
    fn classifies_legacy_evidence_without_registered_by_and_derives_expiry() {
        let fixture = Fixture::new();
        let legacy = get_evidence_detail(&fixture.state, EVIDENCE_A.to_owned())
            .expect("قراءة السجل القديم يجب أن تنجح");
        assert_eq!(
            legacy.registered_by_state, "UNKNOWN_LEGACY",
            "غياب مقدّم الدليل لا يُنسب إلى مستخدم مخترع"
        );
        assert!(legacy.registered_by.is_none());

        // الحالة المخزَّنة VALID مع أجل منقضٍ تُعرض EXPIRED دون تعديل الصف.
        fixture
            .state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .execute(
                "UPDATE evidence_items SET status = 'VALID', expires_at = '2000-01-01T00:00:00Z' WHERE id = ?1",
                [EVIDENCE_A],
            )
            .expect("ضبط حالة منتهية يجب أن ينجح");
        let expired = get_evidence_detail(&fixture.state, EVIDENCE_A.to_owned())
            .expect("قراءة السجل المنتهي يجب أن تنجح");
        assert!(expired.is_expired);
        assert_eq!(expired.status, crate::domain::models::EvidenceStatus::Valid);
        assert_eq!(
            expired.effective_status,
            crate::domain::models::EvidenceStatus::Expired
        );
    }
}
