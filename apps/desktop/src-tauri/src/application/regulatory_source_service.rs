use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        models::{CreatedRegulatorySource, RegulatorySourceProvenance, RegulatorySourcesData},
    },
    infrastructure::{
        database::{append_audit_event, latest_audit_event_hash, AppState},
        regulatory_source_repository::RegulatorySourceRepository,
    },
};

// الوحدة 04 — قراءة فقط لسجل المصادر التنظيمية المسجل محلياً.
// لا تغذية تنظيمية خارجية ولا مراقبة ولا تحليل آلي في هذه المرحلة.
pub fn load_regulatory_sources(state: &AppState) -> Result<RegulatorySourcesData, AppError> {
    authorization_service::require_permission(state, "REGULATORY_INTELLIGENCE", "READ")?;

    let now = Utc::now().to_rfc3339();
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    RegulatorySourceRepository::load_regulatory_sources(&connection, &state.tenant_id(), &now)
}

// ===== تسجيل مصدر تنظيمي يقدّمه المستخدم (0026) =====
//
// هذا ليس جلبًا من جهة رسمية: الرابط يُحفظ كمرجع **أدخله المستخدم** فقط،
// ولا يُفتح ولا يُجلب محتواه، ويبقى `official = 0` و`REQUIRES_REVIEW`.
// البيانات المهيكلة الوحيدة المطلوبة: الجهة والوثيقة، والإصدار/الرابط اختياريان.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRegulatorySourceInput {
    pub authority: String,
    pub document_title: String,
    pub document_version: Option<String>,
    pub source_url: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegulatorySourceMutationResult {
    pub source: CreatedRegulatorySource,
}

pub fn create_regulatory_source(
    state: &AppState,
    input: CreateRegulatorySourceInput,
) -> Result<RegulatorySourceMutationResult, AppError> {
    authorization_service::require_permission(state, "REGULATORY_INTELLIGENCE", "CREATE")?;
    let tenant_id = state.tenant_id();
    let actor_id = state.current_user_id()?;

    let authority = input.authority.trim();
    let document_title = input.document_title.trim();
    let document_version = normalize_optional(input.document_version.as_deref());
    if authority.chars().count() < 2
        || authority.chars().count() > 160
        || document_title.chars().count() < 2
        || document_title.chars().count() > 240
        || document_version
            .as_deref()
            .map(|value| value.chars().count() > 64)
            .unwrap_or(false)
    {
        return Err(AppError::RegulatorySourceInvalid);
    }
    let source_url = normalize_optional(input.source_url.as_deref());
    if let Some(url) = source_url.as_deref() {
        validate_reference_url(url)?;
    }

    let now = Utc::now().to_rfc3339();
    let source_id = format!("source_{}", uuid::Uuid::new_v4());
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    // منع التكرار غير المقصود بنفس قاعدة التفرّد في المخطط (المستأجر + الجهة +
    // الوثيقة + الإصدار). الفحص هنا يعطي رمزاً مفهوماً بدل خطأ قيد تخزين عام.
    let duplicate: bool = transaction
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM regulatory_sources
                WHERE tenant_id = ?1 AND authority = ?2 AND document_title = ?3
                  AND COALESCE(document_version, '') = COALESCE(?4, '')
            )",
            rusqlite::params![&tenant_id, authority, document_title, &document_version],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)?;
    if duplicate {
        return Err(AppError::RegulatorySourceDuplicate);
    }

    transaction
        .execute(
            "INSERT INTO regulatory_sources (
                id, tenant_id, authority, document_title, source_url, document_version,
                retrieved_at, official, verification_state, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 'REQUIRES_REVIEW', ?7, ?7)",
            rusqlite::params![
                &source_id,
                &tenant_id,
                authority,
                document_title,
                &source_url,
                &document_version,
                &now,
            ],
        )
        .map_err(|_| AppError::StorageOperation)?;

    let previous_hash = latest_audit_event_hash(&transaction, &tenant_id)?;
    append_audit_event(
        &transaction,
        &tenant_id,
        &format!("audit_{}", uuid::Uuid::new_v4()),
        "REGULATORY_SOURCE_REGISTERED",
        &actor_id,
        "RegulatorySource",
        &source_id,
        &now,
        &previous_hash,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(RegulatorySourceMutationResult {
        source: CreatedRegulatorySource {
            id: source_id,
            authority: authority.to_owned(),
            document_title: document_title.to_owned(),
            official: false,
            verification_state: "REQUIRES_REVIEW".to_owned(),
            provenance: RegulatorySourceProvenance::UserProvidedUnverified,
            created_at: now,
        },
    })
}

fn normalize_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

// تحقق صرفي لمرجع أدخله المستخدم — لا اتصال ولا جلب. يمنع المخططات الخطرة
// (javascript: · data: · file:) وبيانات اعتماد مضمنة في العنوان.
fn validate_reference_url(value: &str) -> Result<(), AppError> {
    if value.chars().count() > 500 {
        return Err(AppError::RegulatorySourceInvalid);
    }
    let parsed = url::Url::parse(value).map_err(|_| AppError::RegulatorySourceInvalid)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::RegulatorySourceInvalid);
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::RegulatorySourceInvalid);
    }
    if parsed.host_str().map(str::is_empty).unwrap_or(true) {
        return Err(AppError::RegulatorySourceInvalid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    use chrono::Utc;
    use rusqlite::{params, Connection};

    use super::{create_regulatory_source, load_regulatory_sources, CreateRegulatorySourceInput};
    use crate::{
        domain::{
            error::AppError,
            models::{DataMode, RegulatorySourceProvenance},
        },
        infrastructure::database::AppState,
    };

    fn test_state() -> AppState {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        for migration in [
            include_str!("../../migrations/0001_initial.sql"),
            include_str!("../../migrations/0002_identity_and_access.sql"),
            include_str!("../../migrations/0003_workflow_and_risk.sql"),
            include_str!("../../migrations/0004_evidence_metadata.sql"),
            include_str!("../../migrations/0006_correct_demo_truth_labels.sql"),
            include_str!("../../migrations/0007_obligation_management.sql"),
            include_str!("../../migrations/0025_task_lifecycle.sql"),
            include_str!("../../migrations/0026_production_user_and_authoring.sql"),
        ] {
            connection
                .execute_batch(migration)
                .expect("هجرة الاختبار يجب أن تنجح");
        }

        let now = Utc::now().to_rfc3339();
        for (tenant_id, data_mode) in [
            ("tenant-reg-a", "DEMO_SYNTHETIC"),
            ("tenant-reg-b", "PRODUCTION"),
        ] {
            connection
                .execute(
                    "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?1, ?2, ?3, ?3)",
                    params![tenant_id, data_mode, &now],
                )
                .expect("المستأجر يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, NULL, '1.0', ?5, ?6, ?7, ?5, ?5)",
                    params![
                        format!("source-{tenant_id}"),
                        tenant_id,
                        format!("هيئة {tenant_id}"),
                        "مرجع اختبار",
                        &now,
                        if tenant_id == "tenant-reg-b" { 1 } else { 0 },
                        "REQUIRES_REVIEW"
                    ],
                )
                .expect("المصدر يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, source_type, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'مالك', NULL, 'HIGH', 'OPEN', 'APPLIES', 'MISSING', 'UNVERIFIED', 'SYNTHETIC', ?5, ?5)",
                    params![
                        format!("obligation-{tenant_id}"),
                        tenant_id,
                        format!("source-{tenant_id}"),
                        format!("التزام {tenant_id}"),
                        &now
                    ],
                )
                .expect("الالتزام يجب أن ينشأ");
        }

        create_authorized_user(&connection, "tenant-reg-a", &now);

        AppState::test_instance(
            connection,
            "tenant-reg-a".to_owned(),
            "user-reg-a".to_owned(),
        )
    }

    fn create_authorized_user(connection: &Connection, tenant_id: &str, now: &str) {
        connection
            .execute(
                "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES ('user-reg-a', ?1, 'قارئ تنظيمي', 'reg@test.local', NULL, 'ACTIVE', ?2, ?2)",
                params![tenant_id, now],
            )
            .expect("المستخدم يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES ('perm-reg-read', 'regulatory_intelligence:read', 'REGULATORY_INTELLIGENCE', 'READ', 'HIGHLY_RESTRICTED', ?1)",
                [now],
            )
            .expect("الصلاحية يجب أن تنشأ");
        connection
            .execute(
                "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-reg-a', ?1, 'READER', 'قارئ', ?2, ?2)",
                params![tenant_id, now],
            )
            .expect("الدور يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO role_permissions (role_id, permission_id) VALUES ('role-reg-a', 'perm-reg-read')",
                [],
            )
            .expect("المنح يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user-reg-a', 'role-reg-a', ?1, 'system')",
                [now],
            )
            .expect("الإسناد يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session-reg-a', ?1, 'user-reg-a', ?2, '2030-01-01T00:00:00Z', NULL, 'test')",
                params![tenant_id, now],
            )
            .expect("الجلسة يجب أن تنشأ");
    }

    #[test]
    fn returns_tenant_scoped_sources_with_linked_obligation_counts() {
        let state = test_state();
        let data = load_regulatory_sources(&state).expect("القراءة المأذونة يجب أن تنجح");

        assert!(data.has_data);
        assert_eq!(data.summary.total, 1);
        assert_eq!(data.summary.linked_active_obligations, 1);
        assert_eq!(data.items[0].active_obligation_count, 1);
        // مستأجر الديمو يوسم تركيبياً دائماً حتى لو اشتركت الأعمدة.
        assert_eq!(data.provenance, RegulatorySourceProvenance::DemoSynthetic);
        assert_eq!(data.data_mode, DataMode::DemoSynthetic);
    }

    #[test]
    fn isolates_sources_across_tenants() {
        let state = test_state();
        // فاعل المستأجر الثاني غير مأذون لهذه الحالة — الاستعلام يظل مقيداً بمستأجر الحالة.
        state.set_active_tenant_for_tests("tenant-reg-b");
        assert!(matches!(
            load_regulatory_sources(&state),
            Err(AppError::AuthorizationDenied)
        ));

        // بعد منح أهلية للمستأجر الثاني تبقى القراءة محصورة في مصادره فقط.
        let now = Utc::now().to_rfc3339();
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute(
                    "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES ('user-reg-b', 'tenant-reg-b', 'قارئ ب', 'b@test.local', NULL, 'ACTIVE', ?1, ?1)",
                    params![&now],
                )
                .expect("المستخدم يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-reg-b', 'tenant-reg-b', 'READER', 'قارئ', ?1, ?1)",
                    params![&now],
                )
                .expect("الدور يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO role_permissions (role_id, permission_id) VALUES ('role-reg-b', 'perm-reg-read')",
                    [],
                )
                .expect("المنح يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user-reg-b', 'role-reg-b', ?1, 'system')",
                    params![&now],
                )
                .expect("الإسناد يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session-reg-b', 'tenant-reg-b', 'user-reg-b', ?1, '2030-01-01T00:00:00Z', NULL, 'test')",
                    params![&now],
                )
                .expect("الجلسة يجب أن تنشأ");
        }
        state.set_active_user_for_tests("user-reg-b");
        let data = load_regulatory_sources(&state).expect("قراءة المستأجر الثاني يجب أن تنجح");
        assert_eq!(data.summary.total, 1);
        assert_eq!(data.items[0].id, "source-tenant-reg-b");
        // إنتاجي وكل المصادر رسمية → منشأ رسمي.
        assert_eq!(data.provenance, RegulatorySourceProvenance::Official);
    }

    #[test]
    fn denies_read_without_permission_and_reports_empty_tenant_truthfully() {
        let state = test_state();
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute("DELETE FROM role_permissions", [])
                .expect("حذف المنح يجب أن ينجح");
        }
        assert!(matches!(
            load_regulatory_sources(&state),
            Err(AppError::AuthorizationDenied)
        ));

        // مستأجر فارغ تماماً: hasData=false بلا اختراع أرقام.
        let empty_state = test_state();
        empty_state.set_active_tenant_for_tests("tenant-empty");
        {
            let connection = empty_state.connection.lock().expect("القفل يجب أن يتاح");
            let now = Utc::now().to_rfc3339();
            connection
                .execute(
                    "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant-empty', 'فارغ', 'PRODUCTION', ?1, ?1)",
                    params![&now],
                )
                .expect("المستأجر الفارغ يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES ('user-reg-empty', 'tenant-empty', 'قارئ فارغ', 'empty@test.local', NULL, 'ACTIVE', ?1, ?1)",
                    params![&now],
                )
                .expect("المستخدم يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-reg-empty', 'tenant-empty', 'READER', 'قارئ', ?1, ?1)",
                    params![&now],
                )
                .expect("الدور يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO role_permissions (role_id, permission_id) VALUES ('role-reg-empty', 'perm-reg-read')",
                    [],
                )
                .expect("المنح يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user-reg-empty', 'role-reg-empty', ?1, 'system')",
                    params![&now],
                )
                .expect("الإسناد يجب أن ينشأ");
            connection
                .execute(
                    "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session-reg-empty', 'tenant-empty', 'user-reg-empty', ?1, '2030-01-01T00:00:00Z', NULL, 'test')",
                    params![&now],
                )
                .expect("الجلسة يجب أن تنشأ");
        }
        empty_state.set_active_user_for_tests("user-reg-empty");
        let data =
            load_regulatory_sources(&empty_state).expect("قراءة المستأجر الفارغ يجب أن تنجح");
        assert!(!data.has_data);
        assert_eq!(data.summary.total, 0);
        assert!(data.items.is_empty());
        assert_eq!(data.provenance, RegulatorySourceProvenance::Unknown);
    }

    // ===== 0026: تسجيل مصدر تنظيمي يقدّمه المستخدم =====

    fn grant_create_permission(state: &AppState) {
        let now = Utc::now().to_rfc3339();
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "INSERT OR IGNORE INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES ('perm-reg-create', 'regulatory_intelligence:create', 'REGULATORY_INTELLIGENCE', 'CREATE', 'HIGHLY_RESTRICTED', ?1)",
                [&now],
            )
            .expect("صلاحية الإنشاء يجب أن تنشأ");
        connection
            .execute(
                "INSERT OR IGNORE INTO role_permissions (role_id, permission_id) VALUES ('role-reg-a', 'perm-reg-create')",
                [],
            )
            .expect("منح صلاحية الإنشاء يجب أن ينجح");
    }

    fn valid_input() -> CreateRegulatorySourceInput {
        CreateRegulatorySourceInput {
            authority: "هيئة حماية البيانات".to_owned(),
            document_title: "لائحة حماية البيانات الشخصية".to_owned(),
            document_version: Some("1.0".to_owned()),
            source_url: Some("https://authority.example/regulation/1".to_owned()),
        }
    }

    #[test]
    fn registers_a_user_provided_source_with_unverified_provenance_and_audit() {
        let state = test_state();
        grant_create_permission(&state);

        let created = create_regulatory_source(&state, valid_input()).expect("التسجيل يجب أن ينجح");
        assert!(!created.source.official, "مصدر المستخدم ليس رسمياً");
        assert_eq!(created.source.verification_state, "REQUIRES_REVIEW");
        assert_eq!(
            created.source.provenance,
            RegulatorySourceProvenance::UserProvidedUnverified
        );

        let data = load_regulatory_sources(&state).expect("القراءة يجب أن تنجح");
        assert_eq!(data.summary.total, 2);
        assert_eq!(data.summary.official, 0);
        let registered = data
            .items
            .iter()
            .find(|item| item.id == created.source.id)
            .expect("المصدر المسجل يجب أن يظهر في السجل");
        assert!(!registered.official);
        assert_eq!(registered.verification_state, "REQUIRES_REVIEW");
        // مستأجر الديمو يوسم تركيبياً دائماً مهما كان المصدر المسجّل — لا يتحول
        // إلى «رسمي» ولا حتى إلى «مصدر مستخدم» معروض كأنه بيانات إنتاج.
        assert_eq!(data.provenance, RegulatorySourceProvenance::DemoSynthetic);

        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let audits: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE event_type = 'REGULATORY_SOURCE_REGISTERED' AND resource_id = ?1",
                [&created.source.id],
                |row| row.get(0),
            )
            .expect("التدقيق يجب أن يُقرأ");
        assert_eq!(audits, 1);
    }

    #[test]
    fn rejects_duplicate_and_unsafe_references_without_partial_rows() {
        let state = test_state();
        grant_create_permission(&state);
        create_regulatory_source(&state, valid_input()).expect("التسجيل الأول يجب أن ينجح");
        let before: i64 = {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .query_row("SELECT COUNT(*) FROM regulatory_sources", [], |row| {
                    row.get(0)
                })
                .expect("العدّ يجب أن ينجح")
        };

        // نفس الجهة والوثيقة والإصدار: تكرار غير مقصود يُرفض برمزه.
        assert!(matches!(
            create_regulatory_source(&state, valid_input()),
            Err(AppError::RegulatorySourceDuplicate)
        ));
        // مخططان خطيران: ليسا مرجعاً تنظيمياً صالحاً.
        for unsafe_url in [
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "file:///C:/Windows/System32",
            "https://user:secret@authority.example/regulation",
            "not a url",
        ] {
            assert!(
                matches!(
                    create_regulatory_source(
                        &state,
                        CreateRegulatorySourceInput {
                            authority: "جهة أخرى".to_owned(),
                            document_title: "وثيقة أخرى".to_owned(),
                            document_version: None,
                            source_url: Some(unsafe_url.to_owned()),
                        },
                    ),
                    Err(AppError::RegulatorySourceInvalid)
                ),
                "الرابط غير الآمن يجب أن يُرفض: {unsafe_url}"
            );
        }
        // جهة قصيرة جداً.
        assert!(matches!(
            create_regulatory_source(
                &state,
                CreateRegulatorySourceInput {
                    authority: " ".to_owned(),
                    document_title: "وثيقة".to_owned(),
                    document_version: None,
                    source_url: None,
                },
            ),
            Err(AppError::RegulatorySourceInvalid)
        ));

        let after: i64 = {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .query_row("SELECT COUNT(*) FROM regulatory_sources", [], |row| {
                    row.get(0)
                })
                .expect("العدّ يجب أن ينجح")
        };
        assert_eq!(before, after, "كل رفض يجب ألا يترك صفاً جزئياً");
    }

    #[test]
    fn creation_requires_its_own_permission() {
        let state = test_state();
        assert!(matches!(
            create_regulatory_source(&state, valid_input()),
            Err(AppError::AuthorizationDenied)
        ));
    }
}
