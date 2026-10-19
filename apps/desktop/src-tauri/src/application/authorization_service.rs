use chrono::Utc;
use rusqlite::params;

use crate::{
    domain::{error::AppError, models::RuntimeMode},
    infrastructure::database::{
        AppState, DEMO_ADMIN_USER_ID, DEMO_OPERATOR_USER_ID, DEMO_REVIEWER_USER_ID, DEMO_TENANT_ID,
    },
};

// البوابة الصارمة لكل أوامر IPC المحمية: هوية نشطة + جلسة نشطة بعينها + إذن في الدور.
// لا يقبل أي مكوّن من الواجهة: هوية/مستأجر/دوراً/قائمة صلاحيات — كلها من الخلف حصراً.
pub fn require_permission(
    state: &AppState,
    resource_type: &str,
    action: &str,
) -> Result<(), AppError> {
    // فشل قراءة الهوية النشطة يُترجم إلى هوية فارغة تُرفض في الاستعلام — سلوك مغلق.
    let user_id = state.current_user_id().unwrap_or_default();
    let tenant_id = state.tenant_id();

    // في الإنتاج: رفض هويات الديمو وجلساته ومستأجره — دفاع متدرج فوق عزل المستأجرين.
    if state.runtime_mode().unwrap_or(RuntimeMode::Unprovisioned) == RuntimeMode::Production
        && (tenant_id == DEMO_TENANT_ID
            || matches!(
                user_id.as_str(),
                DEMO_OPERATOR_USER_ID | DEMO_REVIEWER_USER_ID | DEMO_ADMIN_USER_ID
            ))
    {
        return Err(AppError::AuthorizationDenied);
    }

    // ربط التفويض بالجلسة النشطة نفسها — إبطالها/تعطيلها يسري فوراً على الطلب التالي.
    let Some(session_id) = state.current_session_id().unwrap_or(None) else {
        return Err(AppError::AuthorizationDenied);
    };
    has_permission_for_session(
        state,
        &tenant_id,
        &user_id,
        &session_id,
        resource_type,
        action,
    )
    .then_some(())
    .ok_or(AppError::AuthorizationDenied)
}

pub fn require_role_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    role_code: &str,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    // ربط الفحص بالجلسة النشطة نفسها — لا يكفي وجود أي جلسة صالحة للمستخدم.
    let session_id = state
        .current_session_id()?
        .ok_or(AppError::AuthorizationDenied)?;
    let has_role = transaction
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM local_users u
                INNER JOIN local_sessions s ON s.user_id = u.id AND s.tenant_id = u.tenant_id
                INNER JOIN user_role_assignments ura ON ura.user_id = u.id
                INNER JOIN roles r ON r.id = ura.role_id AND r.tenant_id = u.tenant_id
                WHERE u.id = ?1
                  AND u.tenant_id = ?2
                  AND u.status = 'ACTIVE'
                  AND s.revoked_at IS NULL
                  AND s.expires_at > ?3
                  AND s.id = ?4
                  AND r.code = ?5
            )",
            params![
                &state.current_user_id()?,
                &state.tenant_id(),
                now,
                session_id,
                role_code
            ],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| AppError::DataAccess)?;

    has_role.then_some(()).ok_or(AppError::AuthorizationDenied)
}

// نفس مسار الحقيقة الوحيد (جلسة بعينها + دور + منح) — الجلسة النشطة شرط صريح هنا.
pub fn has_permission_for_session(
    state: &AppState,
    tenant_id: &str,
    user_id: &str,
    session_id: &str,
    resource_type: &str,
    action: &str,
) -> bool {
    let now = Utc::now().to_rfc3339();
    let connection = match state.connection.lock() {
        Ok(connection) => connection,
        Err(_) => return false,
    };

    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM local_users u
                INNER JOIN local_sessions s ON s.user_id = u.id AND s.tenant_id = u.tenant_id
                INNER JOIN user_role_assignments ura ON ura.user_id = u.id
                INNER JOIN role_permissions rp ON rp.role_id = ura.role_id
                INNER JOIN permissions p ON p.id = rp.permission_id
                WHERE u.id = ?1
                  AND u.tenant_id = ?2
                  AND u.status = 'ACTIVE'
                  AND s.revoked_at IS NULL
                  AND s.expires_at > ?3
                  AND s.id = ?4
                  AND p.resource_type = ?5
                  AND p.action = ?6
            )",
            params![user_id, tenant_id, now, session_id, resource_type, action],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false)
}

pub fn has_permission(
    state: &AppState,
    tenant_id: &str,
    user_id: &str,
    resource_type: &str,
    action: &str,
) -> bool {
    let now = Utc::now().to_rfc3339();
    let connection = match state.connection.lock() {
        Ok(connection) => connection,
        Err(_) => return false,
    };

    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM local_users u
                INNER JOIN local_sessions s ON s.user_id = u.id AND s.tenant_id = u.tenant_id
                INNER JOIN user_role_assignments ura ON ura.user_id = u.id
                INNER JOIN role_permissions rp ON rp.role_id = ura.role_id
                INNER JOIN permissions p ON p.id = rp.permission_id
                WHERE u.id = ?1
                  AND u.tenant_id = ?2
                  AND u.status = 'ACTIVE'
                  AND s.revoked_at IS NULL
                  AND s.expires_at > ?3
                  AND p.resource_type = ?4
                  AND p.action = ?5
            )",
            params![user_id, tenant_id, now, resource_type, action],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {

    use chrono::Utc;
    use rusqlite::Connection;

    use super::has_permission;
    use crate::infrastructure::database::AppState;

    fn state_with_two_tenants() -> AppState {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .execute_batch(include_str!("../../migrations/0001_initial.sql"))
            .expect("هجرة الأساس يجب أن تنجح");
        connection
            .execute_batch(include_str!(
                "../../migrations/0002_identity_and_access.sql"
            ))
            .expect("هجرة الهوية يجب أن تنجح");
        let now = Utc::now().to_rfc3339();

        for tenant_id in ["tenant-a", "tenant-b"] {
            connection
                .execute(
                    "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?2, 'DEMO_SYNTHETIC', ?3, ?3)",
                    (tenant_id, tenant_id, &now),
                )
                .expect("المستأجر التجريبي يجب أن ينشأ");
        }
        connection
            .execute(
                "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES ('user-a', 'tenant-a', 'User A', 'a@test.local', NULL, 'ACTIVE', ?1, ?1)",
                [&now],
            )
            .expect("المستخدم يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-a', 'tenant-a', 'READER', 'Reader', ?1, ?1)",
                [&now],
            )
            .expect("الدور يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES ('perm-read', 'obligation:read', 'OBLIGATION', 'READ', 'INTERNAL', ?1)",
                [&now],
            )
            .expect("الصلاحية يجب أن تنشأ");
        connection
            .execute("INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user-a', 'role-a', ?1, 'system')", [&now])
            .expect("تعيين الدور يجب أن ينجح");
        connection
            .execute("INSERT INTO role_permissions (role_id, permission_id) VALUES ('role-a', 'perm-read')", [])
            .expect("تعيين الصلاحية يجب أن ينجح");
        connection
            .execute(
                "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session-a', 'tenant-a', 'user-a', ?1, '2030-01-01T00:00:00Z', NULL, 'test')",
                [&now],
            )
            .expect("الجلسة يجب أن تنشأ");

        AppState::test_instance(connection, "tenant-a", "user-a")
    }

    #[test]
    fn tenant_scope_prevents_permission_reuse_across_tenants() {
        let state = state_with_two_tenants();

        assert!(has_permission(
            &state,
            "tenant-a",
            "user-a",
            "OBLIGATION",
            "READ"
        ));
        assert!(!has_permission(
            &state,
            "tenant-b",
            "user-a",
            "OBLIGATION",
            "READ"
        ));
    }
}
