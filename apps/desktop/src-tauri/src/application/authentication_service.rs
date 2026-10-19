use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::{Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    application::demo_identity_service,
    domain::{
        error::AppError,
        models::{RuntimeMode, SessionContext},
        organization_profile,
    },
    infrastructure::database::{
        append_audit_event, latest_audit_event_hash, read_runtime_mode,
        seed_demo_governed_tool_runtime, seed_demo_identity, seed_demo_intelligence_runtime,
        seed_demo_tenant, seed_demo_workflow, AppState, DEMO_OPERATOR_USER_ID, DEMO_TENANT_ID,
    },
};

// عمر جلسة الإنتاج — 12 ساعة عمل معقولة لسطح مكتب مؤسسي بلا "تذكرني".
pub const PRODUCTION_SESSION_HOURS: i64 = 12;
// سياسة كلمة المرور: الطول ثم تركيب الأحرف — بلا قوائم كلمات مرفوضة (حد أدنى واعٍ).
const PASSWORD_MIN_CHARS: usize = 12;
const PASSWORD_MAX_CHARS: usize = 128;
// خنق المحاولات: عتبات القفل المتصاعدة — بلا كشف وجود الحساب.
const LOCK_THRESHOLD_ONE: i32 = 5;
const LOCK_THRESHOLD_TWO: i32 = 10;
const LOCK_THRESHOLD_MAX: i32 = 15;
const LOCK_SECONDS_ONE: i64 = 60;
const LOCK_SECONDS_TWO: i64 = 300;
const LOCK_SECONDS_MAX: i64 = 900;
// أثر تشخيصي للمسؤولين المخوّلين عند رفض دخول حساب مقفل: حدث تدقيق واحد
// لكل حساب في هذه النافذة، فلا يضخّم المهاجم سلسلة التدقيق بمحاولات متكررة.
const LOCKOUT_DIAGNOSTIC_WINDOW_SECONDS: i64 = 60;
const LOCKOUT_DIAGNOSTIC_EVENT: &str = "LOGIN_REJECTED_ACCOUNT_LOCKED";

// هاش تجريبي بمعاملات حقيقية — يُتحقق منه عند بريد غير موجود لتساوي التوقيت.
const DUMMY_PASSWORD_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$OXR4NXNhbHQtc3RyaW5nMTIz$5Fj0PZ3dKq7bUvE2mYQ6cR8sT1wXyZ9hN4kL0pV7dJg";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatusData {
    pub mode: String,
    pub authenticated: bool,
    pub needs_bootstrap: bool,
    pub actor_display_name: Option<String>,
    pub session_expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapProductionInput {
    pub organization_name: String,
    // بيانات المؤسسة يعلنها المستخدم صراحةً — لا قيمة افتراضية لأي منهما.
    // الرمز بصيغة ISO 3166-1 alpha-2، والقطاع نص يكتبه المستخدم.
    pub country_code: String,
    pub industry: String,
    pub admin_display_name: String,
    pub admin_email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticatedSession {
    pub context: SessionContext,
    pub session_expires_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

// حالة الدخول كما يحتاجها محرك الواجهة قبل أي هوية — لا بيانات حساسة إطلاقاً.
pub fn get_auth_status(state: &AppState) -> Result<AuthStatusData, AppError> {
    let mode = state.runtime_mode()?;
    let tenant_id = state.tenant_id();
    let user_id = state.current_user_id().unwrap_or_default();
    let session_id = state.current_session_id().unwrap_or(None);

    let (authenticated, actor_display_name, session_expires_at) = if mode == RuntimeMode::Demo {
        (true, None, None)
    } else {
        match (user_id.is_empty(), session_id) {
            (false, Some(session_id)) => {
                let now = Utc::now().to_rfc3339();
                let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
                let row: Option<(String, String)> = connection
                    .query_row(
                        "SELECT u.display_name, s.expires_at
                         FROM local_users u
                         INNER JOIN local_sessions s ON s.id = ?1 AND s.user_id = u.id AND s.tenant_id = u.tenant_id
                         WHERE u.tenant_id = ?2 AND u.id = ?3 AND u.status = 'ACTIVE'
                           AND s.revoked_at IS NULL AND s.expires_at > ?4",
                        params![session_id, tenant_id, user_id, now],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .optional()
                    .map_err(|_| AppError::DataAccess)?;
                match row {
                    Some((display_name, expires_at)) => {
                        (true, Some(display_name), Some(expires_at))
                    }
                    None => (false, None, None),
                }
            }
            _ => (false, None, None),
        }
    };

    Ok(AuthStatusData {
        mode: mode.as_str().to_owned(),
        authenticated,
        needs_bootstrap: mode == RuntimeMode::Unprovisioned,
        actor_display_name,
        session_expires_at,
    })
}

// بدء تجربة الديمو صراحة من بيئة غير مهيأة — انتقال أحادي الاتجاه مع بذر كامل.
pub fn start_demo_experience(state: &AppState) -> Result<AuthenticatedSession, AppError> {
    let now = Utc::now().to_rfc3339();
    {
        let connection = state
            .connection
            .lock()
            .map_err(|_| AppError::StorageOperation)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        let mode = read_runtime_mode(&transaction)?;
        if mode != RuntimeMode::Unprovisioned {
            return Err(AppError::BootstrapUnavailable);
        }
        transaction
            .execute(
                "UPDATE app_runtime_state SET mode = 'DEMO', provisioned_at = ?1, updated_at = ?1 WHERE id = 1",
                [&now],
            )
            .map_err(|_| AppError::StorageOperation)?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
        seed_demo_tenant(&connection)?;
        seed_demo_identity(&connection)?;
        seed_demo_workflow(&connection)?;
        seed_demo_governed_tool_runtime(&connection)?;
        seed_demo_intelligence_runtime(&connection)?;
        seed_demo_audit_start(&connection, &now)?;
    }
    crate::infrastructure::local_logger::info("DEMO_EXPERIENCE_STARTED", DEMO_TENANT_ID, "");
    state.set_runtime_mode(RuntimeMode::Demo)?;
    let session_id = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        crate::infrastructure::database::resolve_active_session(
            &connection,
            DEMO_TENANT_ID,
            DEMO_OPERATOR_USER_ID,
            &now,
        )
        .ok_or(AppError::AuthorizationDenied)?
    };
    state.activate_identity(DEMO_TENANT_ID, DEMO_OPERATOR_USER_ID, Some(session_id))?;
    let context = demo_identity_service::get_session_context(state)?;
    Ok(AuthenticatedSession {
        context,
        session_expires_at: "2030-01-01T00:00:00Z".to_owned(),
    })
}

// إقلاع بيئة الإنتاج: مستأجر + منظمة + مسؤول أول بجدار Argon2id — مرة واحدة فقط.
// دولة المؤسسة وقطاعها يأتيان من المستخدم في هذا الطلب نفسه: لا افتراض ولا
// استنتاج، وأي مدخل غير صالح يُرفض هنا قبل فتح أي معاملة أو كتابة أي صف.
pub fn bootstrap_production(
    state: &AppState,
    input: BootstrapProductionInput,
) -> Result<AuthenticatedSession, AppError> {
    let organization_name = input.organization_name.trim();
    let admin_display_name = input.admin_display_name.trim();
    let admin_email = input.admin_email.trim().to_lowercase();

    if organization_name.chars().count() < 3 || organization_name.chars().count() > 120 {
        return Err(AppError::BootstrapInvalid);
    }
    // التحقق من بيانات المؤسسة المعلنة أولاً: أرخص من Argon2id وأوضح رمز خطأ.
    let profile =
        organization_profile::validate_organization_profile(&input.country_code, &input.industry)?;
    if admin_display_name.chars().count() < 2 || admin_display_name.chars().count() > 80 {
        return Err(AppError::BootstrapInvalid);
    }
    if !is_valid_email(&admin_email) {
        return Err(AppError::BootstrapInvalid);
    }
    // سياسة كلمة المرور مشتركة مع مسار تغيير كلمة المرور؛ في التأسيس تُترجم
    // إلى رمز التهيئة العام حتى تبقى رسالة شاشة التأسيس واحدة ومفهومة.
    validate_password_policy(&input.password).map_err(|_| AppError::BootstrapInvalid)?;

    let password_hash = hash_password(&input.password)?;
    let now = Utc::now().to_rfc3339();
    let tenant_suffix = short_uuid();
    let tenant_id = format!("tenant_prod_{tenant_suffix}");
    let organization_id = format!("org_prod_{tenant_suffix}");
    let admin_suffix = short_uuid();
    let admin_user_id = format!("user_prod_admin_{admin_suffix}");
    let admin_role_id = format!("role_prod_admin_{admin_suffix}");
    let session_id = format!("session_{}", uuid::Uuid::new_v4());
    let expires_at = (Utc::now() + Duration::hours(PRODUCTION_SESSION_HOURS)).to_rfc3339();

    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    let mode = read_runtime_mode(&transaction)?;
    if mode != RuntimeMode::Unprovisioned {
        return Err(AppError::BootstrapUnavailable);
    }

    transaction
        .execute(
            "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?2, 'PRODUCTION', ?3, ?3)",
            (&tenant_id, organization_name, &now),
        )
        .map_err(|_| AppError::StorageOperation)?;
    // الدولة والقطاع يُكتبان من مدخلات المستخدم بعد التطبيع — لا قيم ثابتة.
    transaction
        .execute(
            "INSERT INTO organizations (id, tenant_id, legal_name, country_code, industry, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            (
                &organization_id,
                &tenant_id,
                organization_name,
                &profile.country_code,
                &profile.industry,
                &now,
            ),
        )
        .map_err(|_| AppError::StorageOperation)?;
    transaction
        .execute(
            "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, password_hash, failed_attempts, locked_until, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, NULL, 'ACTIVE', ?5, 0, NULL, ?6, ?6)",
            (&admin_user_id, &tenant_id, admin_display_name, &admin_email, &password_hash, &now),
        )
        .map_err(|_| AppError::StorageOperation)?;
    transaction
        .execute(
            "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES (?1, ?2, 'TENANT_ADMIN', ?3, ?4, ?4)",
            (&admin_role_id, &tenant_id, format!("{admin_display_name} (Tenant Admin)"), &now),
        )
        .map_err(|_| AppError::StorageOperation)?;
    // المشرف الأول يملك كتالوج الأذونات كاملاً — كل منح لاحقة تظل تحت سقف الخلف.
    transaction
        .execute(
            "INSERT INTO role_permissions (role_id, permission_id) SELECT ?1, id FROM permissions",
            [&admin_role_id],
        )
        .map_err(|_| AppError::StorageOperation)?;
    transaction
        .execute(
            "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, ?2, ?3, 'NEXUS Bootstrap')",
            (&admin_user_id, &admin_role_id, &now),
        )
        .map_err(|_| AppError::StorageOperation)?;
    transaction
        .execute(
            "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'NEXUS Desktop')",
            (&session_id, &tenant_id, &admin_user_id, &now, &expires_at),
        )
        .map_err(|_| AppError::StorageOperation)?;

    // كتالوج الأدوار المعلن للمؤسسة الجديدة: دور المشرف أعلاه + دور منفّذ العمل،
    // فتصبح إضافة الموظفين ممكنة من أول لحظة بلا ترحيل لاحق.
    crate::infrastructure::database::ensure_production_role_catalog(&transaction)?;

    // تدقيق الإقلاع ثم تثبيت الوضع إنتاجاً داخل المعاملة نفسها — لا إنتاج بلا أثر.
    let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
    append_audit_event(
        &transaction,
        &tenant_id,
        &format!("audit_bootstrap_{tenant_suffix}"),
        "PRODUCTION_TENANT_BOOTSTRAPPED",
        &admin_user_id,
        "Tenant",
        &tenant_id,
        &now,
        &previous_hash,
    )?;
    transaction
        .execute(
            "UPDATE app_runtime_state SET mode = 'PRODUCTION', provisioned_at = ?1, updated_at = ?1 WHERE id = 1",
            [&now],
        )
        .map_err(|_| AppError::StorageOperation)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    drop(connection);

    state.set_runtime_mode(RuntimeMode::Production)?;
    state.activate_identity(&tenant_id, &admin_user_id, Some(session_id))?;
    let context = demo_identity_service::get_session_context(state)?;
    Ok(AuthenticatedSession {
        context,
        session_expires_at: expires_at,
    })
}

// دخول الإنتاج المحلي: تحقق Argon2id + خنق محاولات + جلسة جديدة معتمة.
pub fn login(state: &AppState, input: LoginInput) -> Result<AuthenticatedSession, AppError> {
    if state.runtime_mode()? != RuntimeMode::Production {
        return Err(AppError::AuthorizationDenied);
    }
    let email = input.email.trim().to_lowercase();
    // حد مبكر للطول قبل أي هاش — كلمة مرور بميغابايتات لا تستحق كلفة Argon2id.
    if email.is_empty() || input.password.is_empty() || input.password.chars().count() > 128 {
        return Err(AppError::AuthInvalidCredentials);
    }
    let now = Utc::now();
    let now_stamp = now.to_rfc3339();

    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    // مجال البحث: المستأجر الإنتاجي وحده (data_mode=PRODUCTION) — الديمو ليس داخله أبداً.
    let account: Option<(String, String, String, String, i32, Option<String>, Option<String>)> =
        transaction
            .query_row(
                "SELECT u.id, u.tenant_id, u.display_name, u.status, u.failed_attempts, u.locked_until, u.password_hash
                 FROM local_users u
                 INNER JOIN tenants t ON t.id = u.tenant_id AND t.data_mode = 'PRODUCTION'
                 WHERE u.email = ?1",
                [&email],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| AppError::DataAccess)?;

    let Some((
        user_id,
        tenant_id,
        _display_name,
        status,
        failed_attempts,
        locked_until,
        password_hash,
    )) = account
    else {
        // بريد غير موجود: تحقق تجريبي بنفس الكلفة ثم نفس الخطأ — لا كشف وجود.
        verify_password(DUMMY_PASSWORD_HASH, &input.password);
        return Err(AppError::AuthInvalidCredentials);
    };

    if let Some(locked_until_stamp) = locked_until.as_deref() {
        if locked_until_stamp > now_stamp.as_str() {
            // ===== منع كشف وجود الحساب =====
            // الحساب المقفل كان يعيد AUTH_LOCKED بينما البريد غير الموجود يعيد
            // AUTH_INVALID_CREDENTIALS ⇒ فرقٌ يكشف وجود الحساب بلا مصادقة.
            // الآن: نفس الرمز العام، ونفس كلفة تحقق Argon2id (هاش وهمي)، فلا
            // يفترق المساران في الرسالة ولا في الزمن. القفل نفسه لم يُضعف:
            // الحساب يبقى مرفوضاً حتى انتهاء المدة، والعداد لا يُصفَّر هنا.
            verify_password(DUMMY_PASSWORD_HASH, &input.password);
            record_lockout_diagnostic(&transaction, &tenant_id, &user_id, &now)?;
            transaction
                .commit()
                .map_err(|_| AppError::StorageOperation)?;
            crate::infrastructure::local_logger::warn("LOGIN_REJECTED_LOCKED", &tenant_id, "");
            return Err(AppError::AuthInvalidCredentials);
        }
    }
    if status != "ACTIVE" {
        // حساب معطل/مقفل إدارياً — نفس خطأ الدخول العام بلا تمييز.
        verify_password(DUMMY_PASSWORD_HASH, &input.password);
        return Err(AppError::AuthInvalidCredentials);
    }
    let Some(hash) = password_hash else {
        verify_password(DUMMY_PASSWORD_HASH, &input.password);
        return Err(AppError::AuthInvalidCredentials);
    };

    if !verify_password(&hash, &input.password) {
        register_failed_attempt(&transaction, &user_id, failed_attempts, &now)?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
        crate::infrastructure::local_logger::warn("LOGIN_FAILURE", &tenant_id, "");
        return Err(AppError::AuthInvalidCredentials);
    }

    // نجاح: تصفير العداد + إصدار جلسة معتمة جديدة (uuid v4 — غير قابلة للتخمين).
    transaction
        .execute(
            "UPDATE local_users SET failed_attempts = 0, locked_until = NULL, updated_at = ?2 WHERE id = ?1",
            [&user_id, &now_stamp],
        )
        .map_err(|_| AppError::StorageOperation)?;
    let session_id = format!("session_{}", uuid::Uuid::new_v4());
    let expires_at = (now + Duration::hours(PRODUCTION_SESSION_HOURS)).to_rfc3339();
    transaction
        .execute(
            "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'NEXUS Desktop')",
            (&session_id, &tenant_id, &user_id, &now_stamp, &expires_at),
        )
        .map_err(|_| AppError::StorageOperation)?;
    // ذيل السلسلة يُقرأ من **مستأجر الحساب المتحقق منه** لا من الحالة النشطة:
    // قبل الدخول لا هوية نشطة (tenant_id فارغ)، فقراءة الذيل من الحالة كانت
    // تبدأ سلسلة جديدة بـprevious_hash فارغ وتكسر سلسلة تدقيق المستأجر.
    let previous_hash = latest_audit_event_hash(&transaction, &tenant_id)?;
    append_audit_event(
        &transaction,
        &tenant_id,
        &format!("audit_{}", uuid::Uuid::new_v4()),
        "USER_AUTHENTICATED",
        &user_id,
        "Session",
        &session_id,
        &now_stamp,
        &previous_hash,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    drop(connection);

    state.activate_identity(&tenant_id, &user_id, Some(session_id))?;
    let context = demo_identity_service::get_session_context(state)?;
    Ok(AuthenticatedSession {
        context,
        session_expires_at: expires_at,
    })
}

// خروج الإنتاج: إبطال صف الجلسة ثم محو الهوية — كل طلب لاحق يُرفض فشلاً مغلقاً.
pub fn logout(state: &AppState) -> Result<(), AppError> {
    if state.runtime_mode()? != RuntimeMode::Production {
        return Err(AppError::AuthorizationDenied);
    }
    let user_id = state.current_user_id()?;
    let tenant_id = state.tenant_id();
    let Some(session_id) = state.current_session_id()? else {
        return Err(AppError::AuthorizationDenied);
    };
    let now = Utc::now().to_rfc3339();

    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let revoked = transaction
        .execute(
            "UPDATE local_sessions SET revoked_at = ?3 WHERE id = ?1 AND tenant_id = ?2 AND user_id = ?4 AND revoked_at IS NULL",
            [&session_id, &tenant_id, &now, &user_id],
        )
        .map_err(|_| AppError::StorageOperation)?;
    if revoked == 0 {
        return Err(AppError::AuthorizationDenied);
    }
    let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
    append_audit_event(
        &transaction,
        &tenant_id,
        &format!("audit_{}", uuid::Uuid::new_v4()),
        "USER_LOGOUT",
        &user_id,
        "Session",
        &session_id,
        &now,
        &previous_hash,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    drop(connection);
    crate::infrastructure::local_logger::info("LOGOUT", &tenant_id, "");

    state.clear_active_identity()
}

// ===== تغيير كلمة المرور (0026) =====
// كلمة المرور لا تُسجَّل ولا تُعاد ولا تُقارن نصياً بعد التخزين: التحقق بـArgon2id،
// والكتابة داخل معاملة واحدة تُبطل بقية جلسات المستخدم في نفس اللحظة.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangePasswordInput {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedPassword {
    pub changed_at: String,
    // عدد الجلسات الأخرى التي أُبطلت — الجلسة الحالية تبقى صالحة حتى لا يُطرد المستخدم.
    pub revoked_sessions: u32,
}

pub fn change_password(
    state: &AppState,
    input: ChangePasswordInput,
) -> Result<ChangedPassword, AppError> {
    // كلمات المرور مسار إنتاجي فقط: جلسات الديمو تركيبية بلا هاش.
    if state.runtime_mode()? != RuntimeMode::Production {
        return Err(AppError::AuthorizationDenied);
    }
    if input.current_password.is_empty()
        || input.current_password.chars().count() > PASSWORD_MAX_CHARS
        || input.new_password.is_empty()
        || input.new_password.chars().count() > PASSWORD_MAX_CHARS
    {
        return Err(AppError::ValidationFailed);
    }
    // سياسة كلمة المرور الجديدة تُفحص **قبل** أي عمل تخزيني: أرخص وأوضح للمستخدم.
    validate_password_policy(&input.new_password)?;
    if input.new_password == input.current_password {
        return Err(AppError::PasswordPolicyViolation);
    }

    let user_id = state.current_user_id()?;
    let tenant_id = state.tenant_id();
    let Some(current_session_id) = state.current_session_id()? else {
        return Err(AppError::AuthorizationDenied);
    };
    let now = Utc::now();
    let now_stamp = now.to_rfc3339();
    // الهاش الجديد يُحسب خارج المعاملة: Argon2id كلفته مقصودة، وحجز قفل الكتابة
    // أثناءها يمنع أي عملية أخرى من التقدم بلا سبب.
    let new_hash = hash_password(&input.new_password)?;

    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    // الإبطال يجب أن يسري فوراً على هذا الطلب أيضاً: الجلسة النشطة في الذاكرة
    // قد تكون أُبطلت أو انتهت في القاعدة، وتغيير كلمة المرور بعملية سابقة لذلك
    // يمنح جلسة مسروقة/مسحوبة قدرة على تدوير السر. الفحص داخل المعاملة نفسها
    // حتى لا تنفصل القراءة عن الكتابة.
    let session_alive: bool = transaction
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM local_sessions
                WHERE id = ?1
                  AND tenant_id = ?2
                  AND user_id = ?3
                  AND revoked_at IS NULL
                  AND expires_at > ?4
            )",
            params![&current_session_id, &tenant_id, &user_id, &now_stamp],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)?;
    if !session_alive {
        return Err(AppError::AuthorizationDenied);
    }

    let account: Option<(Option<String>, i32, Option<String>, String)> = transaction
        .query_row(
            "SELECT password_hash, failed_attempts, locked_until, status
             FROM local_users WHERE id = ?1 AND tenant_id = ?2",
            params![&user_id, &tenant_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|_| AppError::DataAccess)?;
    let Some((password_hash, failed_attempts, locked_until, status)) = account else {
        return Err(AppError::AuthorizationDenied);
    };
    if status != "ACTIVE" {
        return Err(AppError::AuthorizationDenied);
    }
    if let Some(locked_until_stamp) = locked_until.as_deref() {
        if locked_until_stamp > now_stamp.as_str() {
            return Err(AppError::AuthLocked);
        }
    }
    let Some(stored_hash) = password_hash else {
        return Err(AppError::AuthorizationDenied);
    };

    if !verify_password(&stored_hash, &input.current_password) {
        // نفس خنق المحاولات المستخدم في الدخول: كلمة المرور الحالية خطأً ليست
        // مجانية، ومن يخمّنها من جلسة مسروقة يُقفل الحساب تدريجياً.
        register_failed_attempt(&transaction, &user_id, failed_attempts, &now)?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
        crate::infrastructure::local_logger::warn("PASSWORD_CHANGE_REJECTED", &tenant_id, "");
        return Err(AppError::CurrentPasswordInvalid);
    }

    transaction
        .execute(
            "UPDATE local_users SET password_hash = ?3, failed_attempts = 0, locked_until = NULL, updated_at = ?4 WHERE id = ?1 AND tenant_id = ?2",
            params![&user_id, &tenant_id, &new_hash, &now_stamp],
        )
        .map_err(|_| AppError::StorageOperation)?;
    // إبطال بقية الجلسات: تغيير كلمة المرور يعني أن أي جلسة سابقة لم تعد موثوقة.
    let revoked_sessions = transaction
        .execute(
            "UPDATE local_sessions SET revoked_at = ?3
             WHERE tenant_id = ?1 AND user_id = ?2 AND id <> ?4 AND revoked_at IS NULL",
            params![&tenant_id, &user_id, &now_stamp, &current_session_id],
        )
        .map_err(|_| AppError::StorageOperation)? as u32;

    let previous_hash = latest_audit_event_hash(&transaction, &tenant_id)?;
    append_audit_event(
        &transaction,
        &tenant_id,
        &format!("audit_{}", uuid::Uuid::new_v4()),
        "USER_PASSWORD_CHANGED",
        &user_id,
        "User",
        &user_id,
        &now_stamp,
        &previous_hash,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(ChangedPassword {
        changed_at: now_stamp,
        revoked_sessions,
    })
}

// ===== أدوات المصادقة الداخلية =====

fn short_uuid() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_owned()
}

// تحقق البريد بهوية الدخول — يُستعمل في التأسيس وفي إنشاء المستخدمين، فلا
// يوجد تعريفان متباعدان لقاعدة واحدة.
pub(crate) fn is_valid_email(email: &str) -> bool {
    if email.chars().count() < 5 || email.chars().count() > 120 {
        return false;
    }
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

// سياسة كلمة المرور الوحيدة للمنتج: الطول ثم تركيب الأحرف. أي مسار يكتب
// كلمة مرور (التأسيس أو تغيير كلمة المرور أو إنشاء مستخدم) يمر بها حصراً.
pub(crate) fn validate_password_policy(password: &str) -> Result<(), AppError> {
    let chars = password.chars().count();
    if !(PASSWORD_MIN_CHARS..=PASSWORD_MAX_CHARS).contains(&chars) {
        return Err(AppError::PasswordPolicyViolation);
    }
    let has_upper = password.chars().any(|c| c.is_uppercase());
    let has_lower = password.chars().any(|c| c.is_lowercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_symbol = password
        .chars()
        .any(|c| !c.is_alphanumeric() && !c.is_whitespace());
    if has_upper && has_lower && has_digit && has_symbol {
        Ok(())
    } else {
        Err(AppError::PasswordPolicyViolation)
    }
}

// هاش Argon2id بصيغة PHC كاملة (الملح والمعاملات داخلها) — لا ملح منفصل ولا تشفير مخصص.
pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AppError::StorageOperation)
}

// تحقق ثابت الزمن عبر مكتبة Argon2id — أي فشل تحليل يعني عدم تطابق.
pub fn verify_password(stored_hash: &str, password: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(stored_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

// ===== أثر تشخيصي مقنَّن لرفض الدخول على حساب مقفل =====
// الرد للمستخدم غير المصادق يبقى عاماً (منع كشف وجود الحساب)، بينما يبقى
// للمسؤول المخوّل — عبر قارئ التدقيق المحمي بـ`AUDIT:READ` — أثرٌ صريح يشرح
// سبب الرفض. لا يُدوَّن أي سر أو كلمة مرور أو هاش في هذا الحدث.
fn record_lockout_diagnostic(
    transaction: &rusqlite::Transaction<'_>,
    tenant_id: &str,
    user_id: &str,
    now: &chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    let last_stamp: Option<String> = transaction
        .query_row(
            "SELECT occurred_at FROM audit_events
             WHERE tenant_id = ?1 AND event_type = ?2 AND resource_type = 'User' AND resource_id = ?3
             ORDER BY rowid DESC LIMIT 1",
            params![tenant_id, LOCKOUT_DIAGNOSTIC_EVENT, user_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| AppError::DataAccess)?;
    if let Some(last_stamp) = last_stamp {
        if let Ok(last) = chrono::DateTime::parse_from_rfc3339(&last_stamp) {
            if now
                .signed_duration_since(last.with_timezone(&Utc))
                .num_seconds()
                < LOCKOUT_DIAGNOSTIC_WINDOW_SECONDS
            {
                return Ok(());
            }
        }
    }
    let previous_hash = latest_audit_event_hash(transaction, tenant_id)?;
    append_audit_event(
        transaction,
        tenant_id,
        &format!("audit_{}", uuid::Uuid::new_v4()),
        LOCKOUT_DIAGNOSTIC_EVENT,
        user_id,
        "User",
        user_id,
        &now.to_rfc3339(),
        &previous_hash,
    )?;
    Ok(())
}

fn register_failed_attempt(
    transaction: &rusqlite::Transaction<'_>,
    user_id: &str,
    failed_attempts: i32,
    now: &chrono::DateTime<Utc>,
) -> Result<(), AppError> {
    let attempts = failed_attempts + 1;
    let lock_seconds = if attempts >= LOCK_THRESHOLD_MAX {
        Some(LOCK_SECONDS_MAX)
    } else if attempts >= LOCK_THRESHOLD_TWO {
        Some(LOCK_SECONDS_TWO)
    } else if attempts >= LOCK_THRESHOLD_ONE {
        Some(LOCK_SECONDS_ONE)
    } else {
        None
    };
    match lock_seconds {
        Some(seconds) => {
            let locked_until = (*now + Duration::seconds(seconds)).to_rfc3339();
            transaction
                .execute(
                    "UPDATE local_users SET failed_attempts = ?2, locked_until = ?3, updated_at = ?4 WHERE id = ?1",
                    params![user_id, attempts, locked_until, now.to_rfc3339()],
                )
                .map_err(|_| AppError::StorageOperation)?;
        }
        None => {
            transaction
                .execute(
                    "UPDATE local_users SET failed_attempts = ?2, updated_at = ?3 WHERE id = ?1",
                    params![user_id, attempts, now.to_rfc3339()],
                )
                .map_err(|_| AppError::StorageOperation)?;
        }
    }
    Ok(())
}

// أثر تدقيق البداية للديمو المبدوء صراحة — سلسلة مستقلة داخل مستأجر الديمو.
fn seed_demo_audit_start(connection: &Connection, now: &str) -> Result<(), AppError> {
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    // أثر البداية يخص مستأجر الديمو صراحةً — لا نستنتج المستأجر من حالة غير متاحة في هذا النطاق.
    let previous_hash = latest_audit_event_hash(&transaction, DEMO_TENANT_ID)?;
    append_audit_event(
        &transaction,
        DEMO_TENANT_ID,
        &format!("audit_{}", uuid::Uuid::new_v4()),
        "DEMO_EXPERIENCE_STARTED",
        DEMO_OPERATOR_USER_ID,
        "Tenant",
        DEMO_TENANT_ID,
        now,
        &previous_hash,
    )?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

#[cfg(test)]
#[path = "authentication_service_tests.rs"]
mod tests;

// GAP-05-OPEN: جناح إغلاق فجوة بيانات المؤسسة (A–F) — مفصول عن جناح المصادقة
// العام ليبقى كل ملف اختبار مركزاً على نطاقه المعلن.
#[cfg(test)]
#[path = "bootstrap_profile_tests.rs"]
mod profile_tests;
