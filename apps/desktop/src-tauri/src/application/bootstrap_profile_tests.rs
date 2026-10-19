// =====================================================================
// GAP-05-OPEN — جناح إغلاق فجوة بيانات المؤسسة في الإقلاع الإنتاجي
// ---------------------------------------------------------------------
// التغطية بحسب معايير الإغلاق المعلنة:
//   A — إقلاع صالح: دولة وقطاع يعلنهما المستخدم، حفظ فعلي، وبقاء بعد إعادة الفتح.
//   B — دولة غير صالحة: فارغة/غير مدعومة/تنسيق خاطئ/شبيهة بالحقن.
//   C — قطاع غير صالح: فارغ/طويل/ترميز فاسد/مسافات فقط/محارف ممنوعة/العنصر النائب.
//   D — لا اختلاق: لا `SA` ولا `UNSPECIFIED` تُزرع تلقائياً ولا أي حقيقة لم تُدخل.
//   E — سباق الإقلاع: 8 محاولات متزامنة ⇒ نجاح واحد بالضبط وبلا صفوف جزئية.
//   F — توافق قاعدة البيانات: ملف حقيقي، سلسلة هجرات، سلامة، وحفظ صف قديم كما هو.
//
// كل الاختبارات على قواعد معزولة (ذاكرة أو مجلد حرارة) — لا مساس بقاعدة المستخدم.
// =====================================================================

use chrono::Utc;
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;

use super::{bootstrap_production, login, BootstrapProductionInput, LoginInput};
use crate::{
    domain::{error::AppError, models::RuntimeMode},
    infrastructure::database::{initialize_workspace_database, AppState},
};

const DECLARED_COUNTRY: &str = "QA";
const DECLARED_INDUSTRY: &str = "Financial Services";
const ADMIN_EMAIL: &str = "gap05.admin@example.test";
const ADMIN_PASSWORD: &str = "Str0ng!Passw0rd#2026";

fn bootstrap_input() -> BootstrapProductionInput {
    BootstrapProductionInput {
        organization_name: "Gap Five Trading Group".to_owned(),
        country_code: DECLARED_COUNTRY.to_owned(),
        industry: DECLARED_INDUSTRY.to_owned(),
        admin_display_name: "Gap Five Admin".to_owned(),
        admin_email: ADMIN_EMAIL.to_owned(),
        password: ADMIN_PASSWORD.to_owned(),
    }
}

// قاعدة معزولة على القرص: تشمل مسار الهجرات الحقيقي كاملاً (لا جدولاً مصغّراً).
fn isolated_database_path(label: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("nexus-gap05-{label}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).expect("مجلد الاختبار يجب أن ينشأ");
    directory.join("nexus-local.db")
}

fn open_database(path: &PathBuf) -> Connection {
    let connection = Connection::open(path).expect("قاعدة الملف يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
    connection
}

fn unprovisioned_memory_state() -> AppState {
    let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
    initialize_workspace_database(&connection).expect("تهيئة مساحة العمل يجب أن تنجح");
    let state = AppState::test_instance(connection, "", "");
    state
        .set_runtime_mode(RuntimeMode::Unprovisioned)
        .expect("ضبط وضع الاختبار يجب أن ينجح");
    state
}

fn count(connection: &Connection, sql: &str) -> i64 {
    connection
        .query_row(sql, [], |row| row.get(0))
        .unwrap_or_else(|_| panic!("عدّ الاستعلام يجب أن ينجح: {sql}"))
}

fn organization_row(connection: &Connection) -> (String, String, String) {
    connection
        .query_row(
            "SELECT tenant_id, country_code, industry FROM organizations",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("صف المؤسسة يجب أن يُقرأ")
}

// ===== A — إقلاع صالح =====

#[test]
fn valid_bootstrap_writes_exactly_the_declared_country_and_industry() {
    let state = unprovisioned_memory_state();
    let session = bootstrap_production(&state, bootstrap_input()).expect("الإقلاع يجب أن ينجح");

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let (tenant_id, country_code, industry) = organization_row(&connection);
    assert_eq!(country_code, DECLARED_COUNTRY);
    assert_eq!(industry, DECLARED_INDUSTRY);
    // المؤسسة مرتبطة بمستأجر جلسة المسؤول الأول نفسه — لا بمستأجر آخر.
    let session_tenant: String = connection
        .query_row(
            "SELECT tenant_id FROM local_sessions WHERE user_id = ?1",
            [session.context.actor_id.as_str()],
            |row| row.get(0),
        )
        .expect("مستأجر الجلسة يجب أن يُقرأ");
    assert_eq!(tenant_id, session_tenant, "المؤسسة والجلسة في مستأجر واحد");
    // المؤسسة واحدة، والمستأجر واحد، والمسؤول الأول واحد، والجلسة واحدة.
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM organizations"), 1);
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM tenants"), 1);
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM local_users"), 1);
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM local_sessions"), 1);
}

#[test]
fn declared_values_are_persisted_and_survive_reopening_the_database() {
    let path = isolated_database_path("reopen");

    // الجلسة الأولى: قاعدة جديدة على القرص بمسار الهجرات الحقيقي ثم إقلاع حقيقي.
    {
        let connection = open_database(&path);
        let mode = initialize_workspace_database(&connection).expect("التهيئة يجب أن تنجح");
        assert_eq!(
            mode,
            RuntimeMode::Unprovisioned,
            "قاعدة جديدة تبدأ غير مهيأة"
        );
        let state = AppState::test_instance(connection, "", "");
        state
            .set_runtime_mode(RuntimeMode::Unprovisioned)
            .expect("ضبط الوضع");
        bootstrap_production(&state, bootstrap_input()).expect("الإقلاع يجب أن ينجح");
    }

    // الجلسة الثانية: إعادة فتح الملف — القيم تُقرأ من القرص لا من الذاكرة.
    {
        let connection = open_database(&path);
        let mode = initialize_workspace_database(&connection).expect("إعادة التهيئة يجب أن تنجح");
        assert_eq!(mode, RuntimeMode::Production);
        let (_, country_code, industry) = organization_row(&connection);
        assert_eq!(country_code, DECLARED_COUNTRY);
        assert_eq!(industry, DECLARED_INDUSTRY);

        let integrity: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("فحص السلامة");
        assert_eq!(integrity, "ok");
        let foreign_key_violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("فحص المفاتيح الأجنبية");
        assert_eq!(foreign_key_violations, 0);
    }

    // الجلسة الثالثة: أول مسؤول يستطيع الدخول فعلاً بعد إعادة التشغيل.
    {
        let connection = open_database(&path);
        let state = AppState::test_instance(connection, "", "");
        state
            .set_runtime_mode(RuntimeMode::Production)
            .expect("ضبط الوضع");
        let session = login(
            &state,
            LoginInput {
                email: ADMIN_EMAIL.to_owned(),
                password: ADMIN_PASSWORD.to_owned(),
            },
        )
        .expect("دخول المسؤول الأول يجب أن ينجح بعد إعادة الفتح");
        assert_eq!(session.context.responsibility, "ADMINISTRATOR");
    }

    let directory = path.parent().expect("المجلد الأب").to_path_buf();
    std::fs::remove_dir_all(directory).ok();
}

#[test]
fn declared_values_are_normalized_by_the_backend_not_trusted_verbatim() {
    let state = unprovisioned_memory_state();
    let mut input = bootstrap_input();
    input.country_code = "  qa ".to_owned();
    input.industry = "  Financial   Services  ".to_owned();
    bootstrap_production(&state, input).expect("الإقلاع يجب أن ينجح");

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let (_, country_code, industry) = organization_row(&connection);
    assert_eq!(country_code, "QA", "الحالة والفراغ يُطبّعان في الخلف");
    assert_eq!(industry, "Financial Services", "المسافات المكررة تُوحَّد");
}

// ===== B — دولة غير صالحة =====

#[test]
fn invalid_country_is_rejected_before_any_write() {
    for invalid in [
        "",
        "   ",
        "S",
        "SAU",
        "1A",
        "S1",
        "s a",
        "XX",
        "ZZ",
        "SA'",
        "SA; DROP TABLE organizations;--",
        "<script>alert(1)</script>",
        "سع",
        "\u{0}SA",
    ] {
        let state = unprovisioned_memory_state();
        let mut input = bootstrap_input();
        input.country_code = invalid.to_owned();
        assert!(
            matches!(
                bootstrap_production(&state, input),
                Err(AppError::BootstrapCountryInvalid)
            ),
            "الدولة غير الصالحة يجب أن تُرفض: {invalid:?}"
        );

        // الفشل مغلق: لا مستأجر ولا مؤسسة ولا مستخدم ولا أثر تدقيقي ولا تغيير وضع.
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        assert_eq!(count(&connection, "SELECT COUNT(*) FROM tenants"), 0);
        assert_eq!(count(&connection, "SELECT COUNT(*) FROM organizations"), 0);
        assert_eq!(count(&connection, "SELECT COUNT(*) FROM local_users"), 0);
        assert_eq!(count(&connection, "SELECT COUNT(*) FROM audit_events"), 0);
        drop(connection);
        assert!(matches!(
            state.runtime_mode(),
            Ok(RuntimeMode::Unprovisioned)
        ));
    }
}

#[test]
fn country_that_is_not_iso_assigned_is_rejected_even_when_two_letters() {
    let state = unprovisioned_memory_state();
    // رموز حرفية الشكل لكنها محجوزة/غير مخصصة في ISO 3166-1.
    for reserved in ["AA", "QO", "XK", "EU", "UK", "AN", "CS", "ZR", "SU", "YU"] {
        let mut input = bootstrap_input();
        input.country_code = reserved.to_owned();
        assert!(
            matches!(
                bootstrap_production(&state, input),
                Err(AppError::BootstrapCountryInvalid)
            ),
            "الرمز غير المخصص يجب أن يُرفض: {reserved}"
        );
    }
}

// ===== C — قطاع غير صالح =====

#[test]
fn invalid_industry_is_rejected_before_any_write() {
    for invalid in [
        "",
        "   ",
        "\t\n ",
        "A",
        "a".repeat(81).as_str(),
        "UNSPECIFIED",
        "unspecified",
        " unspecified ",
        "<script>alert(1)</script>",
        "Banking & <b>Insurance</b>",
        "Quoted \"value\"",
        "Semi;colon",
        "Back\\slash",
        "Emoji \u{1F600}",
        "Zero\u{200B}width",
        "---",
    ] {
        let state = unprovisioned_memory_state();
        let mut input = bootstrap_input();
        input.industry = invalid.to_owned();
        assert!(
            matches!(
                bootstrap_production(&state, input),
                Err(AppError::BootstrapIndustryInvalid)
            ),
            "القطاع غير الصالح يجب أن يُرفض: {invalid:?}"
        );

        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        assert_eq!(count(&connection, "SELECT COUNT(*) FROM tenants"), 0);
        assert_eq!(count(&connection, "SELECT COUNT(*) FROM organizations"), 0);
        assert_eq!(count(&connection, "SELECT COUNT(*) FROM audit_events"), 0);
    }
}

#[test]
fn valid_arabic_industry_and_maximum_length_are_accepted() {
    let state = unprovisioned_memory_state();
    let mut input = bootstrap_input();
    input.industry = "  الخدمات   المالية  ".to_owned();
    bootstrap_production(&state, input).expect("القطاع العربي يجب أن يُقبل");
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let (_, _, industry) = organization_row(&connection);
    assert_eq!(industry, "الخدمات المالية");
    drop(connection);

    let state = unprovisioned_memory_state();
    let mut input = bootstrap_input();
    input.industry = "a".repeat(80);
    bootstrap_production(&state, input).expect("الحد الأقصى المسموح يجب أن يُقبل");
}

// ===== D — لا اختلاق =====

#[test]
fn bootstrap_never_seeds_default_country_or_unspecified_industry() {
    let state = unprovisioned_memory_state();
    let mut input = bootstrap_input();
    input.country_code = "AE".to_owned();
    input.industry = "Consulting".to_owned();
    bootstrap_production(&state, input).expect("الإقلاع يجب أن ينجح");

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");

    // لا وجود لأي قيمة من الافتراض السابق في أي جدول المؤسسات.
    let placeholders = count(
        &connection,
        "SELECT COUNT(*) FROM organizations WHERE country_code = 'SA' OR industry = 'UNSPECIFIED'",
    );
    assert_eq!(placeholders, 0, "لا زرع تلقائي للسعودية ولا للعنصر النائب");

    let (_, country_code, industry) = organization_row(&connection);
    assert_eq!(country_code, "AE");
    assert_eq!(industry, "Consulting");

    // إقلاع الإنتاج لا يزرع أي بيانات أعمال ولا مستأجر ديمو.
    assert_eq!(
        count(
            &connection,
            "SELECT COUNT(*) FROM tenants WHERE data_mode = 'DEMO_SYNTHETIC'"
        ),
        0
    );
    assert_eq!(
        count(
            &connection,
            "SELECT COUNT(*) FROM audit_events WHERE event_type = 'PRODUCTION_TENANT_BOOTSTRAPPED'"
        ),
        1
    );

    // سلسلة التدقيق سليمة بعد الكتابة الحقيقية.
    assert_eq!(
        count(&connection, "SELECT COUNT(*) FROM audit_events"),
        1,
        "أثر الإقلاع وحده — لا أثر بيانات مؤسسة مختلقة"
    );
}

// ===== E — سباق الإقلاع =====

#[test]
fn eight_concurrent_bootstraps_leave_one_tenant_one_organization_one_admin() {
    use std::sync::{Arc, Barrier};

    let state = Arc::new(unprovisioned_memory_state());
    let threads = 8usize;
    let barrier = Arc::new(Barrier::new(threads));
    let outcomes = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let state = Arc::clone(&state);
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    barrier.wait();
                    bootstrap_production(&state, bootstrap_input()).map(|_| ())
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("الخيط يجب أن ينتهي"))
            .collect::<Vec<_>>()
    });

    let succeeded = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
    let refused = outcomes
        .iter()
        .filter(|outcome| matches!(outcome, Err(AppError::BootstrapUnavailable)))
        .count();
    assert_eq!(succeeded, 1, "إقلاع واحد فقط يجب أن ينجح");
    assert_eq!(refused, 7, "السبعة الباقية تُرفض بالضبط");
    assert!(outcomes
        .iter()
        .all(|outcome| outcome.is_ok() || matches!(outcome, Err(AppError::BootstrapUnavailable))));

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM tenants"), 1);
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM organizations"), 1);
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM local_users"), 1);
    assert_eq!(count(&connection, "SELECT COUNT(*) FROM local_sessions"), 1);
    assert_eq!(
        count(
            &connection,
            "SELECT COUNT(*) FROM audit_events WHERE event_type = 'PRODUCTION_TENANT_BOOTSTRAPPED'"
        ),
        1
    );
    // لا صفوف جزئية: المسؤول الأول يملك دوراً واحداً وكل الأذونات الفعلية.
    assert_eq!(
        count(&connection, "SELECT COUNT(*) FROM user_role_assignments"),
        1
    );
    let admin_permissions = count(
        &connection,
        "SELECT COUNT(*) FROM role_permissions rp INNER JOIN roles r ON r.id = rp.role_id WHERE r.code = 'TENANT_ADMIN'",
    );
    assert!(
        admin_permissions > 0,
        "دور المسؤول الأول يجب أن يُمنح الأذونات"
    );
    drop(connection);

    // عزل المستأجر: المستأجر الوحيد هو الإنتاجي، والقيم المعلنة هي المخزنة.
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let (tenant_id, country_code, industry) = organization_row(&connection);
    assert!(tenant_id.starts_with("tenant_prod_"), "مستأجر إنتاجي واحد");
    assert_eq!(country_code, DECLARED_COUNTRY);
    assert_eq!(industry, DECLARED_INDUSTRY);
}

// ===== F — توافق قاعدة البيانات وحفظ البيانات القديمة =====

#[test]
fn legacy_bootstrap_rows_are_preserved_without_automatic_rewrite() {
    let path = isolated_database_path("legacy");
    let legacy_timestamp = "2026-01-01T00:00:00Z";

    // قاعدة "قديمة" تحمل قيم الافتراض السابق (SA / UNSPECIFIED) لوضع إنتاجي.
    {
        let connection = open_database(&path);
        initialize_workspace_database(&connection).expect("التهيئة يجب أن تنجح");
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant_legacy_001', 'Legacy Organization', 'PRODUCTION', ?1, ?1)",
                [legacy_timestamp],
            )
            .expect("المستأجر القديم");
        connection
            .execute(
                "INSERT INTO organizations (id, tenant_id, legal_name, country_code, industry, created_at, updated_at) VALUES ('org_legacy_001', 'tenant_legacy_001', 'Legacy Organization', 'SA', 'UNSPECIFIED', ?1, ?1)",
                [legacy_timestamp],
            )
            .expect("المؤسسة القديمة");
        connection
            .execute(
                "UPDATE app_runtime_state SET mode = 'PRODUCTION', provisioned_at = ?1, updated_at = ?1 WHERE id = 1",
                [legacy_timestamp],
            )
            .expect("وضع الإنتاج");
    }

    // إعادة فتح + ترقية: مسار الإقلاع الحقيقي يجب ألا يمسّ الصف القديم.
    {
        let connection = open_database(&path);
        let mode = initialize_workspace_database(&connection).expect("الترقية يجب أن تنجح");
        assert_eq!(mode, RuntimeMode::Production);
        let row: (String, String, String) = connection
            .query_row(
                "SELECT country_code, industry, updated_at FROM organizations WHERE id = 'org_legacy_001'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("الصف القديم يجب أن يبقى");
        assert_eq!(row.0, "SA", "لا إعادة تعيين تلقائية للدولة");
        assert_eq!(row.1, "UNSPECIFIED", "لا تصحيح تخميني للقطاع");
        assert_eq!(row.2, legacy_timestamp, "لا لمس وقت التحديث");
        assert_eq!(count(&connection, "SELECT COUNT(*) FROM organizations"), 1);
    }

    // الإقلاع الإنتاجي يبقى مغلقاً على قاعدة مهيأة — لا مؤسسة ثانية.
    {
        let connection = open_database(&path);
        let state = AppState::test_instance(connection, "", "");
        state
            .set_runtime_mode(RuntimeMode::Production)
            .expect("ضبط الوضع");
        assert!(matches!(
            bootstrap_production(&state, bootstrap_input()),
            Err(AppError::BootstrapUnavailable)
        ));
    }

    // فحص سلامة نهائي على الملف بفتح للقراءة فقط.
    {
        let connection = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .expect("فتح للقراءة فقط");
        let integrity: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("فحص السلامة");
        assert_eq!(integrity, "ok");
        let violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("فحص المفاتيح الأجنبية");
        assert_eq!(violations, 0);
    }

    std::fs::remove_dir_all(path.parent().expect("المجلد الأب")).ok();
}

#[test]
fn fresh_database_and_existing_database_reach_the_same_schema_without_data_loss() {
    // قاعدة جديدة: سلسلة الهجرات كاملة مرّة واحدة.
    let fresh_path = isolated_database_path("fresh");
    let fresh_version = {
        let connection = open_database(&fresh_path);
        initialize_workspace_database(&connection).expect("التهيئة يجب أن تنجح");
        count(
            &connection,
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        )
    };

    // قاعدة قائمة: صف حقيقي يُكتب ثم تُعاد التهيئة — لا حذف ولا إعادة بناء.
    let existing_path = isolated_database_path("existing");
    {
        let connection = open_database(&existing_path);
        initialize_workspace_database(&connection).expect("التهيئة الأولى");
        let now = Utc::now().to_rfc3339();
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant_existing_001', 'Existing Organization', 'PRODUCTION', ?1, ?1)",
                [&now],
            )
            .expect("مستأجر قائم");
        connection
            .execute(
                "INSERT INTO organizations (id, tenant_id, legal_name, country_code, industry, created_at, updated_at) VALUES ('org_existing_001', 'tenant_existing_001', 'Existing Organization', 'OM', 'Logistics', ?1, ?1)",
                [&now],
            )
            .expect("مؤسسة قائمة");
    }
    {
        let connection = open_database(&existing_path);
        initialize_workspace_database(&connection).expect("إعادة التهيئة على قاعدة قائمة");
        let version = count(
            &connection,
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        );
        assert_eq!(version, fresh_version, "نفس إصدار المخطط على المسارين");
        let row: (String, String) = connection
            .query_row(
                "SELECT country_code, industry FROM organizations WHERE id = 'org_existing_001'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("الصف القائم يجب أن يبقى");
        assert_eq!(row, ("OM".to_owned(), "Logistics".to_owned()));
    }

    std::fs::remove_dir_all(fresh_path.parent().expect("المجلد الأب")).ok();
    std::fs::remove_dir_all(existing_path.parent().expect("المجلد الأب")).ok();
}
