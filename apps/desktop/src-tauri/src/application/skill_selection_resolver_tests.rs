use chrono::Utc;
use rusqlite::{params, Connection};

use super::SkillSelectionResolver;
use crate::{domain::error::AppError, infrastructure::database::AppState};

// بيئة الاختبار: مستأجران، وكلاء (نشط/معطل/أجنبي)، مهارة بنسختين، وإسناد نشط مثبت على النسخة الأولى.
struct ResolverFixture {
    state: AppState,
}

impl ResolverFixture {
    fn with_tenant(tenant_id: &str) -> Self {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
        crate::infrastructure::database::apply_migrations(&connection)
            .expect("هجرات الاختبار يجب أن تنجح");
        let now = Utc::now().to_rfc3339();
        for tenant in ["tenant-resolve-a", "tenant-resolve-b"] {
            connection
                .execute(
                    "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?2, 'DEMO_SYNTHETIC', ?3, ?3)",
                    (tenant, tenant, &now),
                )
                .expect("المستأجر يجب أن ينشأ");
        }
        for (agent_id, tenant, status) in [
            ("agent-resolve-active", "tenant-resolve-a", "ACTIVE"),
            ("agent-resolve-disabled", "tenant-resolve-a", "DISABLED"),
            ("agent-resolve-foreign", "tenant-resolve-b", "ACTIVE"),
        ] {
            connection
                .execute(
                    "INSERT INTO agents (id, tenant_id, name, description, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'Test agent', ?4, ?5, ?5)",
                    params![agent_id, tenant, agent_id, status, &now],
                )
                .expect("الوكيل يجب أن ينشأ");
        }
        // مهارتان في المستأجر أ وواحدة في المستأجر ب — لاختبار علاقة النسخة بالمهارة والعزل.
        for (skill_id, tenant) in [
            ("skill-resolve-primary", "tenant-resolve-a"),
            ("skill-resolve-secondary", "tenant-resolve-a"),
            ("skill-resolve-foreign", "tenant-resolve-b"),
        ] {
            connection
                .execute(
                    "INSERT INTO skills (id, tenant_id, name, description, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'Test skill', 'ACTIVE', ?4, ?4)",
                    params![skill_id, tenant, skill_id, &now],
                )
                .expect("المهارة يجب أن تنشأ");
        }
        for (version_id, tenant, skill_id, number) in [
            (
                "skill-version-primary-1",
                "tenant-resolve-a",
                "skill-resolve-primary",
                1,
            ),
            (
                "skill-version-primary-2",
                "tenant-resolve-a",
                "skill-resolve-primary",
                2,
            ),
            (
                "skill-version-secondary-1",
                "tenant-resolve-a",
                "skill-resolve-secondary",
                1,
            ),
            (
                "skill-version-foreign-1",
                "tenant-resolve-b",
                "skill-resolve-foreign",
                1,
            ),
        ] {
            connection
                .execute(
                    "INSERT INTO skill_versions (id, tenant_id, skill_id, version_number, instruction, input_contract, output_contract, constraints, created_at) VALUES (?1, ?2, ?3, ?4, 'i', 'in', 'out', 'none', ?5)",
                    params![version_id, tenant, skill_id, number, &now],
                )
                .expect("نسخة المهارة يجب أن تنشأ");
        }
        // إسناد نشط واحد: الوكيل النشط ← النسخة الأولى من المهارة الأساسية (مثبتة حرفياً).
        connection
            .execute(
                "INSERT INTO agent_skill_assignments (id, tenant_id, agent_id, skill_version_id, assigned_at, unassigned_at) VALUES ('assignment-resolve-1', 'tenant-resolve-a', 'agent-resolve-active', 'skill-version-primary-1', ?1, NULL)",
                [&now],
            )
            .expect("الإسناد يجب أن ينشأ");
        // إسناد مقابل في المستأجر الآخر لإثبات أن الحل يعمل داخل حدود مستأجره فقط.
        connection
            .execute(
                "INSERT INTO agent_skill_assignments (id, tenant_id, agent_id, skill_version_id, assigned_at, unassigned_at) VALUES ('assignment-resolve-foreign', 'tenant-resolve-b', 'agent-resolve-foreign', 'skill-version-foreign-1', ?1, NULL)",
                [&now],
            )
            .expect("إسناد المستأجر الآخر يجب أن ينشأ");

        Self {
            state: AppState::test_instance(
                connection,
                tenant_id.to_owned(),
                "user-resolve-probe".to_owned(),
            ),
        }
    }

    fn new() -> Self {
        Self::with_tenant("tenant-resolve-a")
    }

    fn resolve(
        &self,
        agent_id: &str,
        version_id: Option<&str>,
    ) -> Result<Option<crate::domain::trusted_skill_selection::TrustedSkillSelection>, AppError>
    {
        let connection = self.state.connection.lock().expect("القفل يجب أن يتاح");
        let transaction = connection
            .unchecked_transaction()
            .expect("معاملة القراءة يجب أن تنشأ");
        SkillSelectionResolver::resolve(&transaction, &self.state, agent_id, version_id)
    }
}

#[test]
fn missing_selection_allows_zero_skills() {
    let fixture = ResolverFixture::new();
    let resolved = fixture
        .resolve("agent-resolve-active", None)
        .expect("غياب الاختيار مسموح — صفر مهارات حالة مشروعة");
    assert!(resolved.is_none());
}

#[test]
fn resolves_exact_pinned_active_assignment_version() {
    let fixture = ResolverFixture::new();
    let resolved = fixture
        .resolve("agent-resolve-active", Some("skill-version-primary-1"))
        .expect("النسخة المثبتة النشطة يجب أن تُحل")
        .expect("نتيجة موجودة");
    // الهوية المرجعة تبقى النسخة المثبتة بالضبط — لا استبدال ولا تخمين.
    assert_eq!(resolved.tenant_id(), "tenant-resolve-a");
    assert_eq!(resolved.agent_id(), "agent-resolve-active");
    assert_eq!(resolved.skill_id(), "skill-resolve-primary");
    assert_eq!(resolved.skill_version_id(), "skill-version-primary-1");
}

#[test]
fn no_latest_version_substitution() {
    let fixture = ResolverFixture::new();
    // النسخة الثانية موجودة ومتاحة لكن الإسناد مثبت على الأولى — لا ترقية تلقائية.
    assert!(matches!(
        fixture.resolve("agent-resolve-active", Some("skill-version-primary-2")),
        Err(AppError::ResourceNotFound)
    ));
    // ومع ذلك تبقى الأولى قابلة للحل رغم وجود أحدث منها.
    assert!(fixture
        .resolve("agent-resolve-active", Some("skill-version-primary-1"))
        .is_ok());
}

#[test]
fn rejects_missing_or_disabled_agents_and_wrong_versions() {
    let fixture = ResolverFixture::new();
    assert!(matches!(
        fixture.resolve("agent-missing", Some("skill-version-primary-1")),
        Err(AppError::ResourceNotFound)
    ));
    assert!(matches!(
        fixture.resolve("agent-resolve-disabled", Some("skill-version-primary-1")),
        Err(AppError::ResourceNotFound)
    ));
    // نسخة غير موجودة إطلاقاً.
    assert!(matches!(
        fixture.resolve("agent-resolve-active", Some("skill-version-missing")),
        Err(AppError::ResourceNotFound)
    ));
    // نسخة مهارة أخرى — ليست ضمن إسناد الوكيل.
    assert!(matches!(
        fixture.resolve("agent-resolve-active", Some("skill-version-secondary-1")),
        Err(AppError::ResourceNotFound)
    ));
}

#[test]
fn rejects_unassigned_and_foreign_tenant_selections() {
    let fixture = ResolverFixture::new();
    {
        let connection = fixture.state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "UPDATE agent_skill_assignments SET unassigned_at = '2026-01-01T00:00:00Z' WHERE id = 'assignment-resolve-1'",
                [],
            )
            .expect("إلغاء الإسناد يجب أن ينجح");
    }
    // إسناد ملغى لا يمنح اختياراً موثوقاً.
    assert!(matches!(
        fixture.resolve("agent-resolve-active", Some("skill-version-primary-1")),
        Err(AppError::ResourceNotFound)
    ));

    // وكيل ومهارة ونسخة سليمة لكن في المستأجر الآخر — الرفض من بوابة المستأجر.
    assert!(matches!(
        fixture.resolve("agent-resolve-foreign", Some("skill-version-foreign-1")),
        Err(AppError::ResourceNotFound)
    ));
    let foreign_fixture = ResolverFixture::with_tenant("tenant-resolve-b");
    let resolved = foreign_fixture
        .resolve("agent-resolve-foreign", Some("skill-version-foreign-1"))
        .expect("بيانات المستأجر نفسه تُحل");
    assert_eq!(
        resolved.expect("نتيجة موجودة").tenant_id(),
        "tenant-resolve-b"
    );
    // بيانات المستأجر الآخر لا تُحل من حالة مستأجر الاختبار.
    assert!(matches!(
        fixture.resolve("agent-resolve-active", Some("skill-version-foreign-1")),
        Err(AppError::ResourceNotFound)
    ));
}

#[test]
fn rejects_disabled_skill_and_invalid_identifiers() {
    let fixture = ResolverFixture::new();
    {
        let connection = fixture.state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "UPDATE skills SET status = 'DISABLED' WHERE id = 'skill-resolve-primary'",
                [],
            )
            .expect("تعطيل المهارة يجب أن ينجح");
        // إسناد أجنبي مزروع مباشرة لوكيل المستأجر الآخر على نسخة المستأجر أ لاختبار رفض العبور.
        connection
            .execute(
                "INSERT INTO agent_skill_assignments (id, tenant_id, agent_id, skill_version_id, assigned_at, unassigned_at) VALUES ('assignment-cross', 'tenant-resolve-b', 'agent-resolve-foreign', 'skill-version-primary-1', '2026-01-01T00:00:00Z', NULL)",
                [],
            )
            .expect_err("قيد المفتاح الأجنبي المركب يجب أن يمنع إسناداً عابراً للمستأجرين");
    }
    // مهارة معطلة تسقط الحل حتى مع إسناد نشط.
    assert!(matches!(
        fixture.resolve("agent-resolve-active", Some("skill-version-primary-1")),
        Err(AppError::ResourceNotFound)
    ));

    // معرفات فارغة أو طويلة جداً تُرفض كتحقق إدخال قبل أي استعلام.
    assert!(matches!(
        fixture.resolve("   ", Some("skill-version-primary-1")),
        Err(AppError::ValidationFailed)
    ));
    let long_id = "x".repeat(129);
    assert!(matches!(
        fixture.resolve(&long_id, Some("skill-version-primary-1")),
        Err(AppError::ValidationFailed)
    ));
    assert!(matches!(
        fixture.resolve("agent-resolve-active", Some(&long_id)),
        Err(AppError::ValidationFailed)
    ));
}
