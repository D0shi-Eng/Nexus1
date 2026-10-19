use rusqlite::Connection;

use super::{
    bootstrap_production, change_password, get_auth_status, hash_password, login, logout,
    start_demo_experience, verify_password, BootstrapProductionInput, ChangePasswordInput,
    LoginInput,
};
use crate::{
    application::authorization_service::require_permission,
    application::demo_identity_service::{
        get_session_context, list_demo_actors, switch_demo_actor, SwitchDemoActorInput,
    },
    domain::error::AppError,
    domain::models::RuntimeMode,
    infrastructure::database::{
        apply_migrations, ensure_permission_catalog, AppState, DEMO_OPERATOR_USER_ID,
        DEMO_TENANT_ID,
    },
};

// قاعدة غير مهيأة: هجرات + كتالوج أذونات فقط — بلا أي بذر ولا هوية نشطة.
fn unprovisioned_state() -> AppState {
    let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
    apply_migrations(&connection).expect("الهجرات يجب أن تنجح");
    ensure_permission_catalog(&connection).expect("الكتالوج يجب أن ينجح");
    let state = AppState::test_instance(connection, "", "");
    state
        .set_runtime_mode(RuntimeMode::Unprovisioned)
        .expect("ضبط وضع الاختبار يجب أن ينجح");
    state
}

fn bootstrap_input() -> BootstrapProductionInput {
    BootstrapProductionInput {
        organization_name: "Acme Compliance Group".to_owned(),
        // GAP-05-OPEN: الدولة والقطاع يعلنهما المستخدم — لا افتراض في العقد.
        country_code: "SA".to_owned(),
        industry: "Technology Services".to_owned(),
        admin_display_name: "Aisha Manager".to_owned(),
        admin_email: "Admin@Acme.example".to_owned(),
        password: "Str0ng!Passw0rd#2026".to_owned(),
    }
}

fn login_input(email: &str, password: &str) -> LoginInput {
    LoginInput {
        email: email.to_owned(),
        password: password.to_owned(),
    }
}

#[test]
fn fresh_database_is_unprovisioned_and_reports_bootstrap_needed() {
    let state = unprovisioned_state();

    let status = get_auth_status(&state).expect("حالة الدخول يجب أن تُقرأ");
    assert_eq!(status.mode, "UNPROVISIONED");
    assert!(!status.authenticated);
    assert!(status.needs_bootstrap);

    // بلا هوية: كل بوابة محمية تُرفض فشلاً مغلقاً.
    assert!(matches!(
        require_permission(&state, "OBLIGATION", "READ"),
        Err(AppError::AuthorizationDenied)
    ));
}

#[test]
fn bootstrap_creates_production_admin_with_full_catalog_and_session() {
    let state = unprovisioned_state();

    let session = bootstrap_production(&state, bootstrap_input()).expect("إقلاع الإنتاج يجب أن ينجح");

    assert_eq!(session.context.responsibility, "ADMINISTRATOR");
    assert!(session
        .context
        .permissions
        .contains(&"obligation:read".to_owned()));
    assert!(session
        .context
        .permissions
        .contains(&"backup:restore".to_owned()));
    assert!(matches!(state.runtime_mode(), Ok(RuntimeMode::Production)));

    // المسؤول الأول يمارس سلطة فعلية عبر البوابة الصارمة مباشرة.
    assert!(require_permission(&state, "OBLIGATION", "READ").is_ok());
    assert!(require_permission(&state, "USER", "UPDATE").is_ok());

    // البريد يُطبّع إلى حروف صغيرة في المخزن.
    let stored_email: String = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row(
            "SELECT email FROM local_users WHERE id LIKE 'user_prod_%'",
            [],
            |row| row.get(0),
        )
        .expect("البريد يجب أن يقرأ");
    assert_eq!(stored_email, "admin@acme.example");

    // كلمة المرور لا تُخزن نصاً إطلاقاً — هاش PHC فقط.
    let stored_hash: String = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT password_hash FROM local_users WHERE id LIKE 'user_prod_%'",
                [],
                |row| row.get(0),
            )
            .expect("الهاش يجب أن يقرأ")
    };
    assert!(stored_hash.starts_with("$argon2id$"));
    assert!(!stored_hash.contains("Str0ng"));

    let status = get_auth_status(&state).expect("الحالة بعد الإقلاع");
    assert_eq!(status.mode, "PRODUCTION");
    assert!(status.authenticated);
    assert!(!status.needs_bootstrap);
}

#[test]
fn bootstrap_rejects_weak_inputs_and_second_bootstrap() {
    let state = unprovisioned_state();

    // كلمات مرور ضعيفة بأشكالها — طول وتركيب.
    for weak in [
        "Sh0rt!x",                    // قصيرة
        "alllowercase123!",           // بلا كبيرة
        "ALLUPPERCASE123!",           // بلا صغيرة
        "NoDigitsHere!!!",            // بلا رقم
        "NoSymbolsHere123",           // بلا رمز
        "يوجد مسافات في الكلمة 123!", // مسافات داخلية
    ] {
        let mut input = bootstrap_input();
        input.password = weak.to_owned();
        assert!(
            matches!(
                bootstrap_production(&state, input),
                Err(AppError::BootstrapInvalid)
            ),
            "كلمة المرور الضعيفة يجب أن تُرفض: {weak}"
        );
    }

    let mut bad_email = bootstrap_input();
    bad_email.admin_email = "not-an-email".to_owned();
    assert!(matches!(
        bootstrap_production(&state, bad_email),
        Err(AppError::BootstrapInvalid)
    ));

    let mut bad_org = bootstrap_input();
    bad_org.organization_name = "  x  ".to_owned();
    assert!(matches!(
        bootstrap_production(&state, bad_org),
        Err(AppError::BootstrapInvalid)
    ));

    // الإقلاع الناجح مرة واحدة ثم الإغلاق الأحادي للأمر.
    bootstrap_production(&state, bootstrap_input()).expect("الإقلاع الأول يجب أن ينجح");
    assert!(matches!(
        bootstrap_production(&state, bootstrap_input()),
        Err(AppError::BootstrapUnavailable)
    ));
    // لا بذر ديمو تسلل إلى بيئة الإنتاج.
    let demo_rows: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row(
            "SELECT COUNT(*) FROM tenants WHERE data_mode = 'DEMO_SYNTHETIC'",
            [],
            |row| row.get(0),
        )
        .expect("عدد مستأجري الديمو يجب أن يُقرأ");
    assert_eq!(demo_rows, 0);

    // §29: إقلاع الإنتاج لا يُنشئ أي بيانات أعمال اصطناعية — جدولاً جدولاً.
    // البيانات المشروعة الوحيدة هي المستأجر والمنظمة والمستخدم الأول وسجل
    // التدقيق؛ وكل ما عداها يجب أن يبدأ صفراً لأن المستخدم لم يُدخله بعد.
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    for table in [
        "obligations",
        "risks",
        "tasks",
        "evidence_items",
        "controls",
        "approvals",
        "regulatory_sources",
        "agents",
        "agent_runs",
        "skills",
        "tools",
        "knowledge_sources",
        "memory_items",
        "task_state_transitions",
    ] {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [table],
                |row| row.get(0),
            )
            .expect("فحص وجود الجدول يجب أن ينجح");
        assert!(exists, "الجدول {table} يجب أن يكون موجوداً بعد الهجرات");
        let rows: i64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap_or_else(|_| panic!("عدّ {table} يجب أن ينجح"));
        assert_eq!(rows, 0, "إقلاع الإنتاج يجب ألا يزرع أي صف في {table}");
    }
    // الأثر التدقيقي الوحيد مطلوب: إقلاع واحد لا أكثر.
    let audit_rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE event_type = 'PRODUCTION_TENANT_BOOTSTRAPPED'",
            [],
            |row| row.get(0),
        )
        .expect("عدّ أحداث الإقلاع يجب أن ينجح");
    assert_eq!(audit_rows, 1);
    drop(connection);
}

// §9 / §30: سباق طلبَي إقلاع متزامنين. المطلوب: إقلاع واحد فقط ينجح،
// ولا يُنشأ مستأجر ولا مسؤول ثانٍ، والبقية تُرفض بـ`BootstrapUnavailable`.
#[test]
fn concurrent_bootstrap_yields_exactly_one_organization_and_one_admin() {
    use std::sync::{Arc, Barrier};

    let state = Arc::new(unprovisioned_state());
    let threads = 8;
    let barrier = Arc::new(Barrier::new(threads));
    let outcomes = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                let state = Arc::clone(&state);
                let barrier = Arc::clone(&barrier);
                scope.spawn(move || {
                    // إطلاق متزامن حقيقي: كل خيط ينتظر البقية ثم يقتحم معاً.
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
    assert_eq!(
        refused,
        threads - 1,
        "كل طلب لاحق يجب أن يُرفض برمز إقلاع غير متاح"
    );

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let tenants: i64 = connection
        .query_row("SELECT COUNT(*) FROM tenants", [], |row| row.get(0))
        .expect("عدّ المستأجرين");
    let organizations: i64 = connection
        .query_row("SELECT COUNT(*) FROM organizations", [], |row| row.get(0))
        .expect("عدّ المنظمات");
    let admins: i64 = connection
        .query_row("SELECT COUNT(*) FROM user_role_assignments", [], |row| {
            row.get(0)
        })
        .expect("عدّ الإسنادات");
    let bootstraps: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE event_type = 'PRODUCTION_TENANT_BOOTSTRAPPED'",
            [],
            |row| row.get(0),
        )
        .expect("عدّ أحداث الإقلاع");
    assert_eq!(tenants, 1, "مستأجر واحد لا أكثر");
    assert_eq!(organizations, 1, "منظمة واحدة لا أكثر");
    assert_eq!(admins, 1, "مسؤول واحد لا أكثر");
    assert_eq!(bootstraps, 1, "أثر تدقيقي واحد للإقلاع");
}

#[test]
fn login_succeeds_with_correct_password_and_fails_closed_with_wrong_one() {
    let state = unprovisioned_state();
    bootstrap_production(&state, bootstrap_input()).expect("الإقلاع يجب أن ينجح");
    state
        .clear_active_identity()
        .expect("محو الهوية يجب أن ينجح");

    let wrong = login(
        &state,
        login_input("admin@acme.example", "Wrong!Passw0rd#2026"),
    );
    assert!(matches!(wrong, Err(AppError::AuthInvalidCredentials)));

    let unknown = login(&state, login_input("ghost@acme.example", "Whatever!123456"));
    // بريد غير موجود = نفس خطأ كلمة المرور الخاطئة تماماً — لا كشف وجود.
    assert!(matches!(unknown, Err(AppError::AuthInvalidCredentials)));

    let ok = login(
        &state,
        login_input("ADMIN@ACME.EXAMPLE", "Str0ng!Passw0rd#2026"),
    )
    .expect("الدخول الصحيح يجب أن ينجح");
    assert_eq!(ok.context.actor_id.len() > 0, true);
    assert!(require_permission(&state, "OBLIGATION", "READ").is_ok());
}

#[test]
fn repeated_failures_lock_the_account_with_backoff_then_success_resets() {
    let state = unprovisioned_state();
    bootstrap_production(&state, bootstrap_input()).expect("الإقلاع يجب أن ينجح");
    state.clear_active_identity().expect("محو الهوية");

    for attempt in 1..=4 {
        let result = login(&state, login_input("admin@acme.example", "Bad!Password123"));
        assert!(
            matches!(result, Err(AppError::AuthInvalidCredentials)),
            "المحاولة {attempt} قبل العتبة تعطي خطأ اعتمادات لا قفلاً"
        );
    }
    // المحاولة الخامسة ترفع القفل — حتى كلمة المرور الصحيحة تُرفض الآن.
    assert!(matches!(
        login(&state, login_input("admin@acme.example", "Bad!Password123")),
        Err(AppError::AuthInvalidCredentials)
    ));
    let locked_until: Option<String> = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row(
            "SELECT locked_until FROM local_users WHERE id LIKE 'user_prod_%'",
            [],
            |row| row.get(0),
        )
        .expect("القفل يجب أن يقرأ");
    assert!(locked_until.is_some());

    assert!(matches!(
        login(
            &state,
            login_input("admin@acme.example", "Str0ng!Passw0rd#2026")
        ),
        // الحساب المقفل يبقى مرفوضاً حتى بكلمة المرور الصحيحة، لكن بالرمز
        // العام نفسه لا برمز يكشف وجود الحساب (انظر اختبار عدم التمييز أدناه).
        Err(AppError::AuthInvalidCredentials)
    ));

    // أثر تشخيصي محمي للمسؤول المخوّل يشرح سبب الرفض، وبلا أي مادة سرية.
    let diagnostics: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE event_type = 'LOGIN_REJECTED_ACCOUNT_LOCKED'",
            [],
            |row| row.get(0),
        )
        .expect("أثر الرفض يجب أن يقرأ");
    assert_eq!(
        diagnostics, 1,
        "أثر رفض الحساب المقفل يجب أن يُدوَّن مرة واحدة"
    );

    // فك القفل يدوي بمحو الطابع الزمني (محاكاة مرور المدة) ثم الدخول يصفر العداد.
    state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .execute("UPDATE local_users SET locked_until = NULL", [])
        .expect("فك القفل يجب أن ينجح");
    login(
        &state,
        login_input("admin@acme.example", "Str0ng!Passw0rd#2026"),
    )
    .expect("الدخول بعد انقضاء القفل يجب أن ينجح");
    let attempts: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row(
            "SELECT failed_attempts FROM local_users WHERE id LIKE 'user_prod_%'",
            [],
            |row| row.get(0),
        )
        .expect("العداد يجب أن يقرأ");
    assert_eq!(attempts, 0);
}

// ===== منع كشف وجود الحساب (جولة الإغلاق النهائية) =====
// الحساب المقفل والبريد غير الموجود يجب أن يعطيا رمز الرفض نفسه تماماً، وإلا
// صار تسجيل الدخول أداة تعداد حسابات لمن لا يملك مصادقة.
#[test]
fn locked_account_is_indistinguishable_from_unknown_account() {
    let state = unprovisioned_state();
    bootstrap_production(&state, bootstrap_input()).expect("الإقلاع يجب أن ينجح");
    state.clear_active_identity().expect("محو الهوية");

    for _ in 0..5 {
        let _ = login(&state, login_input("admin@acme.example", "Bad!Password123"));
    }
    let locked = login(&state, login_input("admin@acme.example", "Bad!Password123"))
        .expect_err("الحساب المقفل يجب أن يُرفض");
    let unknown = login(&state, login_input("ghost@acme.example", "Bad!Password123"))
        .expect_err("البريد غير الموجود يجب أن يُرفض");

    let locked_code = crate::domain::error::SafeError::from(locked).code;
    let unknown_code = crate::domain::error::SafeError::from(unknown).code;
    assert_eq!(
        locked_code, unknown_code,
        "رمز الرفض لا يجوز أن يفرّق بين حساب مقفل وبريد غير موجود"
    );
    assert_eq!(locked_code, "AUTH_INVALID_CREDENTIALS");
}

#[test]
fn locked_account_cannot_authenticate_with_the_correct_password() {
    let state = unprovisioned_state();
    bootstrap_production(&state, bootstrap_input()).expect("الإقلاع يجب أن ينجح");
    state.clear_active_identity().expect("محو الهوية");

    // قفل إداري مباشر: القفل لم يُضعف، والدخول الصحيح لا يتجاوزه.
    state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .execute(
            "UPDATE local_users SET locked_until = '2999-01-01T00:00:00Z' WHERE id LIKE 'user_prod_%'",
            [],
        )
        .expect("ضبط القفل يجب أن ينجح");

    let result = login(
        &state,
        login_input("admin@acme.example", "Str0ng!Passw0rd#2026"),
    );
    assert!(matches!(result, Err(AppError::AuthInvalidCredentials)));

    // جلسة الإقلاع موجودة مسبقاً؛ المطلوب ألا يُصدر المحاولة جلسة جديدة.
    let session_count: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row("SELECT COUNT(*) FROM local_sessions", [], |row| row.get(0))
        .expect("الجلسات يجب أن تُقرأ");
    assert_eq!(session_count, 1, "لا جلسة تُصدر لحساب مقفل");
}

#[test]
fn logout_revokes_session_and_every_later_request_is_denied() {
    let state = unprovisioned_state();
    bootstrap_production(&state, bootstrap_input()).expect("الإقلاع يجب أن ينجح");

    logout(&state).expect("الخروج يجب أن ينجح");
    assert!(matches!(
        require_permission(&state, "OBLIGATION", "READ"),
        Err(AppError::AuthorizationDenied)
    ));
    // خروج ثانٍ بعد محو الهوية: رفض آمن لا انهيار (إدمان على الإلغاء).
    assert!(matches!(logout(&state), Err(AppError::AuthorizationDenied)));

    // الجلسة القديمة نفسها لا تعود صالحة بالدخول من جديد إلا بإصدار جلسة جديدة.
    login(
        &state,
        login_input("admin@acme.example", "Str0ng!Passw0rd#2026"),
    )
    .expect("إعادة الدخول يجب أن تنجح");
    assert!(require_permission(&state, "OBLIGATION", "READ").is_ok());
}

#[test]
fn disabled_or_expired_or_revoked_sessions_fail_closed_immediately() {
    let state = unprovisioned_state();
    bootstrap_production(&state, bootstrap_input()).expect("الإقلاع يجب أن ينجح");

    // تعطيل المستخدم منتصف الجلسة — الطلب التالي يُرفض فوراً.
    state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .execute(
            "UPDATE local_users SET status = 'DISABLED' WHERE id LIKE 'user_prod_%'",
            [],
        )
        .expect("التعطيل يجب أن ينجح");
    assert!(matches!(
        require_permission(&state, "OBLIGATION", "READ"),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(matches!(
        login(
            &state,
            login_input("admin@acme.example", "Str0ng!Passw0rd#2026")
        ),
        Err(AppError::AuthInvalidCredentials)
    ));

    // إعادة التنشيط ثم انتهاء الجلسة — نفس الفشل المغلق.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "UPDATE local_users SET status = 'ACTIVE' WHERE id LIKE 'user_prod_%'",
                [],
            )
            .expect("التنشيط يجب أن ينجح");
        connection
            .execute(
                "UPDATE local_sessions SET expires_at = '2020-01-01T00:00:00Z'",
                [],
            )
            .expect("انتهاء الجلسة يجب أن ينجح");
    }
    assert!(matches!(
        require_permission(&state, "OBLIGATION", "READ"),
        Err(AppError::AuthorizationDenied)
    ));

    // جلسة ملغاة يدوياً (منتصف العمر) — مرفوضة أيضاً.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "UPDATE local_sessions SET expires_at = '2030-01-01T00:00:00Z', revoked_at = NULL",
                [],
            )
            .expect("إصلاح انتهاء الجلسة يجب أن ينجح");
    }
    login(
        &state,
        login_input("admin@acme.example", "Str0ng!Passw0rd#2026"),
    )
    .expect("دخول جديد بجلسة سليمة يجب أن ينجح");
    state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .execute(
            "UPDATE local_sessions SET revoked_at = '2026-01-01T00:00:00Z'",
            [],
        )
        .expect("الإلغاء يجب أن ينجح");
    assert!(matches!(
        require_permission(&state, "OBLIGATION", "READ"),
        Err(AppError::AuthorizationDenied)
    ));
}

#[test]
fn role_removal_drops_permission_immediately_for_live_session() {
    let state = unprovisioned_state();
    bootstrap_production(&state, bootstrap_input()).expect("الإقلاع يجب أن ينجح");

    state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .execute("DELETE FROM user_role_assignments", [])
        .expect("نزع الدور يجب أن ينجح");

    assert!(matches!(
        require_permission(&state, "OBLIGATION", "READ"),
        Err(AppError::AuthorizationDenied)
    ));
}

#[test]
fn production_mode_blocks_demo_authority_completely() {
    let state = unprovisioned_state();
    bootstrap_production(&state, bootstrap_input()).expect("الإقلاع يجب أن ينجح");

    // أوامر الديمو مرفوضة من الخلف — ليس إخفاء واجهة.
    assert!(matches!(
        list_demo_actors(&state),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(matches!(
        switch_demo_actor(
            &state,
            SwitchDemoActorInput {
                actor_id: DEMO_OPERATOR_USER_ID.to_owned(),
            }
        ),
        Err(AppError::AuthorizationDenied)
    ));

    // دفاع متدرج: حتى لو فرضنا هوية الديمو داخل الذاكرة يبقى التفويض مرفوضاً.
    state
        .activate_identity(
            DEMO_TENANT_ID,
            DEMO_OPERATOR_USER_ID,
            Some("session_demo_operator".to_owned()),
        )
        .expect("فرض هوية الاختبار يجب أن ينجح");
    assert!(matches!(
        require_permission(&state, "OBLIGATION", "READ"),
        Err(AppError::AuthorizationDenied)
    ));

    // دخول الديمو بكلمة مرور لا يمر — حسابات الديمو بلا هاش أصلاً.
    state.clear_active_identity().expect("محو الهوية");
    assert!(matches!(
        login(
            &state,
            login_input("demo.operator@nexus.local", "Whatever!123456")
        ),
        Err(AppError::AuthInvalidCredentials)
    ));
}

#[test]
fn demo_experience_is_explicit_one_way_and_excludes_production_bootstrap() {
    let state = unprovisioned_state();

    let session = start_demo_experience(&state).expect("بدء الديمو يجب أن ينجح");
    assert_eq!(session.context.actor_id, DEMO_OPERATOR_USER_ID);
    assert!(matches!(state.runtime_mode(), Ok(RuntimeMode::Demo)));
    assert!(require_permission(&state, "OBLIGATION", "READ").is_ok());

    // أحادي الاتجاه: لا إقلاع إنتاج في قاعدة ديمو، ولا بدء ديمو مرة ثانية.
    assert!(matches!(
        bootstrap_production(&state, bootstrap_input()),
        Err(AppError::BootstrapUnavailable)
    ));
    assert!(matches!(
        start_demo_experience(&state),
        Err(AppError::BootstrapUnavailable)
    ));

    // لا مستأجر إنتاج تسلل إلى بيئة الديمو.
    let production_rows: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row(
            "SELECT COUNT(*) FROM tenants WHERE data_mode = 'PRODUCTION'",
            [],
            |row| row.get(0),
        )
        .expect("عدد المستأجرين الإنتاجيين يجب أن يقرأ");
    assert_eq!(production_rows, 0);
}

#[test]
fn passwords_never_appear_in_audit_or_session_records() {
    let state = unprovisioned_state();
    bootstrap_production(&state, bootstrap_input()).expect("الإقلاع يجب أن ينجح");
    state.clear_active_identity().expect("محو الهوية");
    login(
        &state,
        login_input("admin@acme.example", "Str0ng!Passw0rd#2026"),
    )
    .expect("الدخول يجب أن ينجح");

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let audit_rows: i64 = connection
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .expect("سجل التدقيق يجب أن يُحصى");
    let leaked: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE actor_name LIKE '%Str0ng%' OR resource_id LIKE '%Str0ng%' OR event_type LIKE '%Str0ng%'",
            [],
            |row| row.get(0),
        )
        .expect("فحص التسريب يجب أن ينجح");
    assert!(audit_rows >= 2);
    assert_eq!(leaked, 0);
}

// انحدار مؤكَّد (كُشف أثناء التحقق الحقيقي من تطبيق سطح المكتب): دخول الإنتاج
// بعد الإقلاع كان يقرأ ذيل سلسلة التدقيق من الهوية النشطة — وهي فارغة قبل
// الدخول — فيكتب حدث USER_AUTHENTICATED بـprevious_hash فارغ ويكسر سلسلة
// المستأجر. الصحيح: قراءة الذيل من مستأجر الحساب المتحقق منه نفسه.
#[test]
fn login_extends_the_tenant_audit_chain_instead_of_restarting_it() {
    let state = unprovisioned_state();
    bootstrap_production(&state, bootstrap_input()).expect("الإقلاع يجب أن ينجح");
    state
        .clear_active_identity()
        .expect("محو الهوية قبل الدخول");
    login(
        &state,
        login_input("admin@acme.example", "Str0ng!Passw0rd#2026"),
    )
    .expect("الدخول يجب أن ينجح");

    let events: Vec<(String, String, String)> = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let mut statement = connection
            .prepare(
                "SELECT event_type, previous_hash, event_hash FROM audit_events ORDER BY rowid",
            )
            .expect("تحضير قراءة الأحداث");
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("قراءة الأحداث");
        rows.map(|row| row.expect("صف التدقيق")).collect()
    };
    assert_eq!(events.len(), 2, "حدثا الإقلاع والدخول فقط");
    assert_eq!(events[0].0, "PRODUCTION_TENANT_BOOTSTRAPPED");
    assert_eq!(events[0].1, "", "الحدث الأول يبدأ السلسلة");
    assert_eq!(events[1].0, "USER_AUTHENTICATED");
    assert_eq!(
        events[1].1, events[0].2,
        "كل حدث يجب أن يربط بذيل السلسلة الفعلي لا ببدايتها"
    );

    // المسار الرسمي للتحقق يوافق على السلسلة — لا فحص يدوي فقط.
    assert!(
        crate::application::audit_service::verify_audit_chain(&state).expect("التحقق الرسمي"),
        "سلسلة تدقيق المستأجر يجب أن تبقى سليمة بعد الدخول"
    );
}

#[test]
fn argon2id_hash_and_verify_roundtrip_is_phc_formatted() {
    let hash = hash_password("Another!Passw0rd#9").expect("الهاش يجب أن يتولد");
    assert!(hash.starts_with("$argon2id$v=19$"));
    assert!(verify_password(&hash, "Another!Passw0rd#9"));
    assert!(!verify_password(&hash, "Another!Passw0rd#8"));
    // سلسلة تالفة لا تُفجّر التحقق — تعني عدم تطابق فقط.
    assert!(!verify_password("garbage-not-a-hash", "x"));
}

#[test]
fn session_context_reflects_production_roles_and_permissions() {
    let state = unprovisioned_state();
    let session = bootstrap_production(&state, bootstrap_input()).expect("الإقلاع يجب أن ينجح");

    let context = get_session_context(&state).expect("سياق الجلسة يجب أن يُقرأ");
    assert_eq!(context.actor_id, session.context.actor_id);
    assert_eq!(context.responsibility, "ADMINISTRATOR");
    assert!(!context.permissions.is_empty());
}

// =====================================================================
// 0026 — دورة كلمة المرور الآمنة (UAT-02)
// =====================================================================

fn production_state_with_admin() -> AppState {
    let state = unprovisioned_state();
    bootstrap_production(&state, bootstrap_input()).expect("الإقلاع يجب أن ينجح");
    state
}

fn password_hash_of(state: &AppState, email: &str) -> Option<String> {
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    connection
        .query_row(
            "SELECT password_hash FROM local_users WHERE email = ?1",
            [email],
            |row| row.get::<_, Option<String>>(0),
        )
        .expect("قراءة الهاش يجب أن تنجح")
}

#[test]
fn change_password_verifies_current_rotates_hash_and_revokes_other_sessions() {
    let state = production_state_with_admin();
    let old_email = "admin@acme.example";
    let old_hash = password_hash_of(&state, old_email).expect("هاش المسؤول يجب أن يوجد");
    // الجلسة الأولى من التأسيس ما زالت مفتوحة؛ نفتح جلسة ثانية فتصبح الأخرى «سابقة».
    login(&state, login_input(old_email, "Str0ng!Passw0rd#2026")).expect("الدخول يجب أن ينجح");
    let active_session = state
        .current_session_id()
        .expect("قراءة الجلسة يجب أن تنجح")
        .expect("جلسة نشطة مطلوبة");

    let changed = change_password(
        &state,
        ChangePasswordInput {
            current_password: "Str0ng!Passw0rd#2026".to_owned(),
            new_password: "R0tated!Passw0rd#2026".to_owned(),
        },
    )
    .expect("تغيير كلمة المرور يجب أن ينجح");
    assert_eq!(
        changed.revoked_sessions, 1,
        "بقية جلسات المستخدم يجب أن تُبطل"
    );

    let new_hash = password_hash_of(&state, old_email).expect("الهاش الجديد يجب أن يوجد");
    assert_ne!(old_hash, new_hash, "الهاش يجب أن يتغير فعلياً");
    assert!(verify_password(&new_hash, "R0tated!Passw0rd#2026"));
    assert!(!verify_password(&new_hash, "Str0ng!Passw0rd#2026"));

    // الجلسة الحالية تبقى صالحة…
    assert!(require_permission(&state, "USER", "READ").is_ok());
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let current_revoked: Option<String> = connection
            .query_row(
                "SELECT revoked_at FROM local_sessions WHERE id = ?1",
                [&active_session],
                |row| row.get(0),
            )
            .expect("قراءة الجلسة يجب أن تنجح");
        assert!(current_revoked.is_none(), "الجلسة الحالية يجب ألا تُبطل");
    }

    // الكلمة القديمة لم تعد تعمل، والجديدة تعمل.
    state
        .clear_active_identity()
        .expect("محو الهوية يجب أن ينجح");
    assert!(matches!(
        login(&state, login_input(old_email, "Str0ng!Passw0rd#2026")),
        Err(AppError::AuthInvalidCredentials)
    ));
    login(&state, login_input(old_email, "R0tated!Passw0rd#2026"))
        .expect("الدخول بالكلمة الجديدة يجب أن ينجح");

    let audit_events: i64 = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE event_type = 'USER_PASSWORD_CHANGED'",
                [],
                |row| row.get(0),
            )
            .expect("التدقيق يجب أن يُقرأ")
    };
    assert_eq!(audit_events, 1);
}

#[test]
fn change_password_rejects_wrong_current_and_weak_new_without_writing() {
    let state = production_state_with_admin();
    let email = "admin@acme.example";
    let before = password_hash_of(&state, email).expect("الهاش يجب أن يوجد");

    assert!(matches!(
        change_password(
            &state,
            ChangePasswordInput {
                current_password: "Wr0ng!Passw0rd#2026".to_owned(),
                new_password: "An0ther!Passw0rd#2026".to_owned(),
            },
        ),
        Err(AppError::CurrentPasswordInvalid)
    ));
    assert!(matches!(
        change_password(
            &state,
            ChangePasswordInput {
                current_password: "Str0ng!Passw0rd#2026".to_owned(),
                new_password: "weak".to_owned(),
            },
        ),
        Err(AppError::PasswordPolicyViolation)
    ));
    // نفس الكلمة الحالية ليست تغييراً حقيقياً.
    assert!(matches!(
        change_password(
            &state,
            ChangePasswordInput {
                current_password: "Str0ng!Passw0rd#2026".to_owned(),
                new_password: "Str0ng!Passw0rd#2026".to_owned(),
            },
        ),
        Err(AppError::PasswordPolicyViolation)
    ));
    let after = password_hash_of(&state, email).expect("الهاش يجب أن يوجد");
    assert_eq!(before, after, "الرفض يجب ألا يغيّر الهاش");
    assert!(verify_password(&after, "Str0ng!Passw0rd#2026"));
}

#[test]
fn change_password_is_closed_in_demo_mode() {
    let state = unprovisioned_state();
    start_demo_experience(&state).expect("بدء الديمو يجب أن ينجح");
    assert!(matches!(
        change_password(
            &state,
            ChangePasswordInput {
                current_password: "anything".to_owned(),
                new_password: "An0ther!Passw0rd#2026".to_owned(),
            },
        ),
        Err(AppError::AuthorizationDenied)
    ));
}

// انحدار أمني: كانت الجلسة النشطة في الذاكرة وحدها تكفي لتغيير كلمة المرور،
// فجلسة أُبطلت أو انتهت في القاعدة تبقى قادرة على تدوير السر حتى انتهاء عمرها.
#[test]
fn change_password_fails_closed_when_the_active_session_is_revoked_or_expired() {
    let state = production_state_with_admin();
    let email = "admin@acme.example";
    let before = password_hash_of(&state, email).expect("الهاش يجب أن يوجد");
    let session_id = state
        .current_session_id()
        .expect("قراءة الجلسة يجب أن تنجح")
        .expect("جلسة نشطة مطلوبة");

    // (أ) جلسة ملغاة في القاعدة مع بقاء الهوية النشطة في الذاكرة.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "UPDATE local_sessions SET revoked_at = ?2 WHERE id = ?1",
                rusqlite::params![&session_id, chrono::Utc::now().to_rfc3339()],
            )
            .expect("إبطال الجلسة في الاختبار يجب أن ينجح");
    }
    assert!(matches!(
        change_password(
            &state,
            ChangePasswordInput {
                current_password: "Str0ng!Passw0rd#2026".to_owned(),
                new_password: "R0tated!Passw0rd#2026".to_owned(),
            },
        ),
        Err(AppError::AuthorizationDenied)
    ));

    // (ب) جلسة منتهية الصلاحية — نفس الرفض المغلق.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "UPDATE local_sessions SET revoked_at = NULL, expires_at = '2000-01-01T00:00:00Z' WHERE id = ?1",
                [&session_id],
            )
            .expect("إنهاء الجلسة في الاختبار يجب أن ينجح");
    }
    assert!(matches!(
        change_password(
            &state,
            ChangePasswordInput {
                current_password: "Str0ng!Passw0rd#2026".to_owned(),
                new_password: "R0tated!Passw0rd#2026".to_owned(),
            },
        ),
        Err(AppError::AuthorizationDenied)
    ));

    let after = password_hash_of(&state, email).expect("الهاش يجب أن يوجد");
    assert_eq!(before, after, "الرفض يجب ألا يغيّر الهاش");
    let change_events: i64 = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE event_type = 'USER_PASSWORD_CHANGED'",
                [],
                |row| row.get(0),
            )
            .expect("التدقيق يجب أن يُقرأ")
    };
    assert_eq!(change_events, 0, "الطلب المرفوض يجب ألا يُدقَّق كتغيير");
}
