use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::{
    application::{authentication_service, authorization_service::require_permission},
    domain::error::AppError,
    infrastructure::database::{append_audit_event, latest_audit_event_hash, AppState},
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceUser {
    pub user_id: String,
    pub display_name: String,
    pub email: String,
    pub status: String,
    pub roles: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceUsersData {
    pub users: Vec<WorkspaceUser>,
    pub available_roles: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetUserStatusInput {
    pub user_id: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserRoleInput {
    pub user_id: String,
    pub role_code: String,
}

// ===== إنشاء مستخدم مؤسسي (0026) =====
// الحدود الملزمة: الجهة الفاعلة تأتي من الجلسة الموثوقة (لا tenantId من العميل)،
// والدور من كتالوج المستأجر نفسه، ولا يُمنح أحدٌ صلاحيةً تتجاوز صلاحيات المُسنِد.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceUserInput {
    pub display_name: String,
    pub email: String,
    // كلمة مرور أولية يسلّمها المسؤول للعضو عبر قناة داخلية؛ لا تُسجَّل ولا تُعاد.
    pub initial_password: String,
    pub role_codes: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedWorkspaceUser {
    pub user_id: String,
    pub email: String,
    pub status: String,
    pub roles: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokeUserSessionsInput {
    pub user_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RevokedUserSessions {
    pub user_id: String,
    pub revoked_sessions: u32,
}

// الأدوار المسموح إدارياً — من كتالوج أدوار المستأجر نفسه فقط، لا برمجة صلاحيات حرة.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserMutationResult {
    pub user_id: String,
}

pub fn list_workspace_users(state: &AppState) -> Result<WorkspaceUsersData, AppError> {
    require_permission(state, "USER", "READ")?;
    let tenant_id = state.tenant_id();
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;

    let mut statement = connection
        .prepare(
            "SELECT u.id, u.display_name, u.email, u.status,
                    COALESCE((SELECT GROUP_CONCAT(r.code, ',')
                              FROM user_role_assignments ura
                              INNER JOIN roles r ON r.id = ura.role_id
                              WHERE ura.user_id = u.id), '') AS role_codes
             FROM local_users u
             WHERE u.tenant_id = ?1
             ORDER BY u.display_name ASC",
        )
        .map_err(|_| AppError::DataAccess)?;
    let users = statement
        .query_map([&tenant_id], |row| {
            Ok(WorkspaceUser {
                user_id: row.get(0)?,
                display_name: row.get(1)?,
                email: row.get(2)?,
                status: row.get(3)?,
                roles: row
                    .get::<_, String>(4)?
                    .split(',')
                    .filter(|code| !code.is_empty())
                    .map(str::to_owned)
                    .collect(),
            })
        })
        .map_err(|_| AppError::DataAccess)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::DataAccess)?;

    let mut roles_statement = connection
        .prepare("SELECT code FROM roles WHERE tenant_id = ?1 ORDER BY code ASC")
        .map_err(|_| AppError::DataAccess)?;
    let available_roles = roles_statement
        .query_map([&tenant_id], |row| row.get::<_, String>(0))
        .map_err(|_| AppError::DataAccess)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::DataAccess)?;

    Ok(WorkspaceUsersData {
        users,
        available_roles,
    })
}

// ===== إنشاء مستخدم مؤسسي =====
//
// دورة العمل: تفويض ← تحقق مدخلات ← تحقق كتالوج الأدوار ← منع رفع الصلاحيات ←
// تحقق تكرار البريد ← هاش Argon2id خارج المعاملة ← معاملة واحدة (مستخدم + أدوار +
// تدقيق) ← تثبيت. لا يقبل أي مصدر هوية من العميل: المستأجر من الجلسة الموثوقة.
pub fn create_workspace_user(
    state: &AppState,
    input: CreateWorkspaceUserInput,
) -> Result<CreatedWorkspaceUser, AppError> {
    require_permission(state, "USER", "CREATE")?;
    let actor_id = state.current_user_id()?;
    let tenant_id = state.tenant_id();

    let display_name = input.display_name.trim();
    let email = input.email.trim().to_lowercase();
    if display_name.chars().count() < 2
        || display_name.chars().count() > 80
        || !authentication_service::is_valid_email(&email)
    {
        return Err(AppError::ValidationFailed);
    }
    authentication_service::validate_password_policy(&input.initial_password)?;

    // تطبيع الأدوار: بلا تكرار، وبعدد محدود صريح — الأدوار تأتي من كتالوج المستأجر.
    let mut role_codes: Vec<String> = Vec::new();
    for code in &input.role_codes {
        let code = code.trim();
        if code.is_empty() || code.chars().count() > 64 {
            return Err(AppError::ValidationFailed);
        }
        if !role_codes.iter().any(|existing| existing == code) {
            role_codes.push(code.to_owned());
        }
    }
    if role_codes.is_empty() || role_codes.len() > 4 {
        return Err(AppError::ValidationFailed);
    }

    // الهاش خارج المعاملة: Argon2id مقصود الكلفة، ولا يجوز حجز قفل الكتابة أثناءه.
    let password_hash = authentication_service::hash_password(&input.initial_password)?;
    let now = Utc::now().to_rfc3339();
    let user_id = format!("user_{}", uuid::Uuid::new_v4());

    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    // كتالوج أدوار المستأجر: كل دور مطلوب يجب أن يكون من صفوف هذا المستأجر حصراً.
    let mut role_ids: Vec<(String, String)> = Vec::new();
    for code in &role_codes {
        let role_id: Option<String> = transaction
            .query_row(
                "SELECT id FROM roles WHERE tenant_id = ?1 AND code = ?2",
                params![&tenant_id, code],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| AppError::DataAccess)?;
        let Some(role_id) = role_id else {
            return Err(AppError::UserRoleUnknown);
        };
        role_ids.push((code.clone(), role_id));
    }

    // منع رفع الصلاحيات: مجموع أذونات الأدوار المطلوبة يجب أن يكون جزءاً من
    // أذونات المُسنِد نفسه. القارئ هنا من نفس مسار منح التفويض الوحيد.
    if !role_permissions_are_grantable(&transaction, &tenant_id, &actor_id, &role_ids)? {
        return Err(AppError::UserRoleNotGrantable);
    }

    let email_exists: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM local_users WHERE tenant_id = ?1 AND email = ?2)",
            params![&tenant_id, &email],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)?;
    if email_exists {
        return Err(AppError::UserEmailExists);
    }

    let inserted = transaction
        .execute(
            "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, password_hash, failed_attempts, locked_until, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, NULL, 'ACTIVE', ?5, 0, NULL, ?6, ?6)",
            params![&user_id, &tenant_id, display_name, &email, &password_hash, &now],
        )
        .map_err(|error| match error {
            // قيد التفرّد قد يفوز به طلب متزامن بين الفحص والكتابة — يُترجم لرمزه.
            rusqlite::Error::SqliteFailure(inner, _)
                if inner.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                AppError::UserEmailExists
            }
            _ => AppError::StorageOperation,
        })?;
    if inserted != 1 {
        return Err(AppError::StorageOperation);
    }

    for (_, role_id) in &role_ids {
        transaction
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, ?2, ?3, ?4)",
                params![&user_id, role_id, &now, &actor_id],
            )
            .map_err(|_| AppError::StorageOperation)?;
    }

    let previous_hash = latest_audit_event_hash(&transaction, &tenant_id)?;
    append_audit_event(
        &transaction,
        &tenant_id,
        &format!("audit_{}", uuid::Uuid::new_v4()),
        "USER_PROVISIONED",
        &actor_id,
        "User",
        &user_id,
        &now,
        &previous_hash,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(CreatedWorkspaceUser {
        user_id,
        email,
        status: "ACTIVE".to_owned(),
        roles: role_codes,
    })
}

// إلغاء كل جلسات مستخدم عند الحاجة (فقد جهاز، اشتباه تسريب جلسة).
// منع تعديل الذات قائم هنا أيضاً: المسؤول لا يُبطل جلسته الحالية من هذا المسار.
pub fn revoke_user_sessions(
    state: &AppState,
    input: RevokeUserSessionsInput,
) -> Result<RevokedUserSessions, AppError> {
    require_permission(state, "USER", "UPDATE")?;
    let actor_id = state.current_user_id()?;
    let tenant_id = state.tenant_id();
    let user_id = input.user_id.trim();
    if user_id.is_empty() || user_id.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    if user_id == actor_id {
        return Err(AppError::SelfModificationForbidden);
    }

    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let exists_in_tenant: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM local_users WHERE id = ?1 AND tenant_id = ?2)",
            params![user_id, &tenant_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)?;
    if !exists_in_tenant {
        return Err(AppError::ResourceNotFound);
    }
    let revoked = transaction
        .execute(
            "UPDATE local_sessions SET revoked_at = ?3 WHERE tenant_id = ?1 AND user_id = ?2 AND revoked_at IS NULL",
            params![&tenant_id, user_id, &now],
        )
        .map_err(|_| AppError::StorageOperation)? as u32;

    let previous_hash = latest_audit_event_hash(&transaction, &tenant_id)?;
    append_audit_event(
        &transaction,
        &tenant_id,
        &format!("audit_{}", uuid::Uuid::new_v4()),
        "USER_SESSIONS_REVOKED",
        &actor_id,
        "User",
        user_id,
        &now,
        &previous_hash,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(RevokedUserSessions {
        user_id: user_id.to_owned(),
        revoked_sessions: revoked,
    })
}

// =====================================================================
// ضابط «آخر مسؤول فعال» (جولة الإغلاق النهائية)
// ---------------------------------------------------------------------
// السلطة الإدارية تُعرَّف بالصلاحية الفعلية لا باسم الدور: أي دور يمنح
// `USER:CREATE` أو `USER:UPDATE` يُمكّن صاحبه من إدارة المستخدمين والأدوار،
// ومن يملكها يستطيع دائماً إعادة تأهيل غيره. لذلك المعيار هو: بعد العملية
// يجب أن يبقى **مستخدم واحد نشط على الأقل** يملك إحدى هاتين الصلاحيتين.
//
// لماذا داخل المعاملة: الفحص قبل الكتابة في معاملة واحدة يمنع أن يقرأ
// طلبان الحالة نفسها ثم يُنفّذان معاً فيُفرغان المؤسسة من مسؤوليها.
// النموذج المحلي يستخدم اتصالاً واحداً محمياً بقفل، والقيد هنا طبقة دفاع
// إضافية صريحة تبقى صحيحة حتى لو تعددت الاتصالات/النسخ على الملف نفسه.
// =====================================================================
const USER_ADMINISTRATION_RESOURCE: &str = "USER";

fn count_active_administrators(
    transaction: &rusqlite::Transaction<'_>,
    tenant_id: &str,
) -> Result<i64, AppError> {
    transaction
        .query_row(
            "SELECT COUNT(DISTINCT u.id)
             FROM local_users u
             INNER JOIN user_role_assignments ura ON ura.user_id = u.id
             INNER JOIN roles r ON r.id = ura.role_id AND r.tenant_id = u.tenant_id
             INNER JOIN role_permissions rp ON rp.role_id = r.id
             INNER JOIN permissions p ON p.id = rp.permission_id
             WHERE u.tenant_id = ?1
               AND u.status = 'ACTIVE'
               AND p.resource_type = ?2
               AND p.action IN ('CREATE', 'UPDATE')",
            params![tenant_id, USER_ADMINISTRATION_RESOURCE],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)
}

// هل يحمل المستخدم سلطة إدارية من دور آخر غير الدور المطلوب نزعه؟
fn has_administration_from_other_role(
    transaction: &rusqlite::Transaction<'_>,
    tenant_id: &str,
    user_id: &str,
    excluded_role_id: &str,
) -> Result<bool, AppError> {
    transaction
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM local_users u
                INNER JOIN user_role_assignments ura ON ura.user_id = u.id
                INNER JOIN roles r ON r.id = ura.role_id AND r.tenant_id = u.tenant_id
                INNER JOIN role_permissions rp ON rp.role_id = r.id
                INNER JOIN permissions p ON p.id = rp.permission_id
                WHERE u.id = ?1
                  AND u.tenant_id = ?2
                  AND u.status = 'ACTIVE'
                  AND ura.role_id <> ?3
                  AND p.resource_type = ?4
                  AND p.action IN ('CREATE', 'UPDATE'))",
            params![
                user_id,
                tenant_id,
                excluded_role_id,
                USER_ADMINISTRATION_RESOURCE
            ],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)
}

// هل يمنح هذا الدور سلطة إدارية للمستخدمين؟
fn role_grants_user_administration(
    transaction: &rusqlite::Transaction<'_>,
    role_id: &str,
) -> Result<bool, AppError> {
    transaction
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM role_permissions rp
                INNER JOIN permissions p ON p.id = rp.permission_id
                WHERE rp.role_id = ?1
                  AND p.resource_type = ?2
                  AND p.action IN ('CREATE', 'UPDATE'))",
            params![role_id, USER_ADMINISTRATION_RESOURCE],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)
}

fn user_is_active(
    transaction: &rusqlite::Transaction<'_>,
    tenant_id: &str,
    user_id: &str,
) -> Result<bool, AppError> {
    transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM local_users WHERE id = ?1 AND tenant_id = ?2 AND status = 'ACTIVE')",
            params![user_id, tenant_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)
}

// هل يملك المستخدم النشط سلطة إدارية من أي دور؟
fn user_holds_administration(
    transaction: &rusqlite::Transaction<'_>,
    tenant_id: &str,
    user_id: &str,
) -> Result<bool, AppError> {
    transaction
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM local_users u
                INNER JOIN user_role_assignments ura ON ura.user_id = u.id
                INNER JOIN roles r ON r.id = ura.role_id AND r.tenant_id = u.tenant_id
                INNER JOIN role_permissions rp ON rp.role_id = r.id
                INNER JOIN permissions p ON p.id = rp.permission_id
                WHERE u.id = ?1
                  AND u.tenant_id = ?2
                  AND u.status = 'ACTIVE'
                  AND p.resource_type = ?3
                  AND p.action IN ('CREATE', 'UPDATE'))",
            params![user_id, tenant_id, USER_ADMINISTRATION_RESOURCE],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)
}

// هل كل أذونات الأدوار المطلوبة محتواة في أذونات الفاعل داخل نفس المستأجر؟
fn role_permissions_are_grantable(
    transaction: &rusqlite::Transaction<'_>,
    tenant_id: &str,
    actor_id: &str,
    role_ids: &[(String, String)],
) -> Result<bool, AppError> {
    let mut actor_permissions: std::collections::HashSet<String> = std::collections::HashSet::new();
    {
        let mut statement = transaction
            .prepare(
                "SELECT DISTINCT rp.permission_id
                 FROM user_role_assignments ura
                 INNER JOIN roles r ON r.id = ura.role_id AND r.tenant_id = ?2
                 INNER JOIN role_permissions rp ON rp.role_id = r.id
                 WHERE ura.user_id = ?1",
            )
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![actor_id, tenant_id], |row| row.get::<_, String>(0))
            .map_err(|_| AppError::DataAccess)?;
        for row in rows {
            actor_permissions.insert(row.map_err(|_| AppError::DataAccess)?);
        }
    }

    for (_, role_id) in role_ids {
        let mut statement = transaction
            .prepare("SELECT permission_id FROM role_permissions WHERE role_id = ?1")
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![role_id], |row| row.get::<_, String>(0))
            .map_err(|_| AppError::DataAccess)?;
        for row in rows {
            let permission_id = row.map_err(|_| AppError::DataAccess)?;
            if !actor_permissions.contains(&permission_id) {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

// تغيير حالة حساب: تفعيل/تعطيل — لا لمس الذات إطلاقاً (لا قفل النفس خارج النظام).
pub fn set_user_status(
    state: &AppState,
    input: SetUserStatusInput,
) -> Result<UserMutationResult, AppError> {
    require_permission(state, "USER", "UPDATE")?;
    let actor_id = state.current_user_id()?;
    let tenant_id = state.tenant_id();
    let user_id = input.user_id.trim();
    if user_id.is_empty() || user_id.len() > 128 {
        return Err(AppError::ValidationFailed);
    }
    if user_id == actor_id {
        // لا يستطيع المسؤول تعطيل/تنشيط حسابه بنفسه — يمنع الانقطاع الجماعي والرفع الذاتي.
        return Err(AppError::SelfModificationForbidden);
    }
    if !matches!(input.status.as_str(), "ACTIVE" | "DISABLED") {
        return Err(AppError::ValidationFailed);
    }

    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    // تعطيل حساب نشط يملك سلطة إدارية: يُرفض إن كان هو المسؤول الفعلي الأخير.
    if input.status == "DISABLED"
        && user_is_active(&transaction, &tenant_id, user_id)?
        && user_holds_administration(&transaction, &tenant_id, user_id)?
        && count_active_administrators(&transaction, &tenant_id)? <= 1
    {
        return Err(AppError::LastAdministratorProtected);
    }
    let updated = transaction
        .execute(
            "UPDATE local_users SET status = ?3, updated_at = ?4 WHERE id = ?1 AND tenant_id = ?2",
            params![user_id, tenant_id, input.status, now],
        )
        .map_err(|_| AppError::DataAccess)?;
    if updated == 0 {
        return Err(AppError::ResourceNotFound);
    }
    // تعطيل الحساب يبطل جلساته فوراً — لا جلسة معلقة بعد التعطيل.
    if input.status == "DISABLED" {
        transaction
            .execute(
                "UPDATE local_sessions SET revoked_at = ?3 WHERE user_id = ?1 AND tenant_id = ?2 AND revoked_at IS NULL",
                params![user_id, tenant_id, now],
            )
            .map_err(|_| AppError::DataAccess)?;
    }
    let previous_hash = latest_audit_hash(&transaction, &tenant_id)?;
    append_audit_event(
        &transaction,
        &tenant_id,
        &format!("audit_{}", uuid::Uuid::new_v4()),
        "USER_STATUS_CHANGED",
        &actor_id,
        "User",
        user_id,
        &now,
        &previous_hash,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(UserMutationResult {
        user_id: user_id.to_owned(),
    })
}

// إسناد دور من كتالوج المستأجر — لا يمنح المُسند ما لا يملكه نظامه بذاته.
pub fn assign_user_role(
    state: &AppState,
    input: UserRoleInput,
) -> Result<UserMutationResult, AppError> {
    mutate_role_assignment(state, input, true)
}

pub fn revoke_user_role(
    state: &AppState,
    input: UserRoleInput,
) -> Result<UserMutationResult, AppError> {
    mutate_role_assignment(state, input, false)
}

fn mutate_role_assignment(
    state: &AppState,
    input: UserRoleInput,
    assign: bool,
) -> Result<UserMutationResult, AppError> {
    require_permission(state, "USER", "UPDATE")?;
    let actor_id = state.current_user_id()?;
    let tenant_id = state.tenant_id();
    let user_id = input.user_id.trim();
    let role_code = input.role_code.trim();
    if user_id.is_empty() || user_id.len() > 128 || role_code.is_empty() || role_code.len() > 64 {
        return Err(AppError::ValidationFailed);
    }
    if user_id == actor_id {
        // لا رفع ذاتي: المسؤول لا يعدل أدوار حسابه بنفسه.
        return Err(AppError::SelfModificationForbidden);
    }

    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    let role_id: Option<String> = transaction
        .query_row(
            "SELECT id FROM roles WHERE tenant_id = ?1 AND code = ?2",
            params![tenant_id, role_code],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| AppError::DataAccess)?;
    let Some(role_id) = role_id else {
        return Err(AppError::ValidationFailed);
    };
    let target_in_tenant: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM local_users WHERE id = ?1 AND tenant_id = ?2)",
            params![user_id, tenant_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)?;
    if !target_in_tenant {
        return Err(AppError::ResourceNotFound);
    }

    // منع رفع الصلاحيات يسري على **الإسناد** كما يسري على الإنشاء: كان الفحص
    // موجوداً في `create_workspace_user` وحده، فأمكن لمن يملك `user:update`
    // إنشاء حساب محدود ثم إسناد دور أعلى منه — تجاوز غير مباشر للحد.
    // الإلغاء لا يحتاج فحصاً لأنه نزع صلاحية لا منحها.
    if assign
        && !role_permissions_are_grantable(
            &transaction,
            &tenant_id,
            &actor_id,
            &[(role_code.to_owned(), role_id.clone())],
        )?
    {
        return Err(AppError::UserRoleNotGrantable);
    }

    // نزع دور إداري عن نشط لا يحمل سلطة إدارية من دور آخر: يُرفض إن كان آخر مسؤول.
    if !assign
        && user_is_active(&transaction, &tenant_id, user_id)?
        && role_grants_user_administration(&transaction, &role_id)?
        && !has_administration_from_other_role(&transaction, &tenant_id, user_id, &role_id)?
        && count_active_administrators(&transaction, &tenant_id)? <= 1
    {
        return Err(AppError::LastAdministratorProtected);
    }

    if assign {
        transaction
            .execute(
                "INSERT OR IGNORE INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, ?2, ?3, ?4)",
                params![user_id, role_id, now, actor_id],
            )
            .map_err(|_| AppError::DataAccess)?;
    } else {
        transaction
            .execute(
                "DELETE FROM user_role_assignments WHERE user_id = ?1 AND role_id = ?2",
                params![user_id, role_id],
            )
            .map_err(|_| AppError::DataAccess)?;
    }

    let previous_hash = latest_audit_hash(&transaction, &tenant_id)?;
    append_audit_event(
        &transaction,
        &tenant_id,
        &format!("audit_{}", uuid::Uuid::new_v4()),
        if assign {
            "USER_ROLE_ASSIGNED"
        } else {
            "USER_ROLE_REVOKED"
        },
        &actor_id,
        "User",
        user_id,
        &now,
        &previous_hash,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(UserMutationResult {
        user_id: user_id.to_owned(),
    })
}

fn latest_audit_hash(
    transaction: &rusqlite::Transaction<'_>,
    tenant_id: &str,
) -> Result<String, AppError> {
    latest_audit_event_hash(transaction, tenant_id)
}

#[cfg(test)]
#[path = "user_administration_service_tests.rs"]
mod tests;
