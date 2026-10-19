use chrono::Utc;

use serde::{Deserialize, Serialize};

use crate::{
    domain::{error::AppError, models::RuntimeMode, models::SessionContext},
    infrastructure::database::{
        append_audit_event, latest_audit_event_hash, resolve_active_session, AppState,
        DEMO_OPERATOR_USER_ID, DEMO_REVIEWER_USER_ID, DEMO_TENANT_ID,
    },
};

// مسؤوليات الفاعلين في نموذج الديمو — قيم ثابتة تُعرض للواجهة كما هي.
pub const REQUESTER_RESPONSIBILITY: &str = "REQUESTER";
pub const REVIEWER_RESPONSIBILITY: &str = "REVIEWER";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoActor {
    pub actor_id: String,
    pub display_name: String,
    pub responsibility: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DemoActorsData {
    pub actors: Vec<DemoActor>,
    pub active_actor_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchDemoActorInput {
    pub actor_id: String,
}

// كتالوج فاعلي الديمو الثابتين — لا يجوز تبديل الهوية النشطة إلى خارج هذين المعرّفين.
fn demo_actor_catalog() -> [(&'static str, &'static str); 2] {
    [
        (DEMO_OPERATOR_USER_ID, REQUESTER_RESPONSIBILITY),
        (DEMO_REVIEWER_USER_ID, REVIEWER_RESPONSIBILITY),
    ]
}

pub fn list_demo_actors(state: &AppState) -> Result<DemoActorsData, AppError> {
    // كتالوج فاعلي الديمو غير متاح خارج وضع الديمو — رفض خلفي لا إخفاء واجهة.
    if state.runtime_mode()? != RuntimeMode::Demo {
        return Err(AppError::AuthorizationDenied);
    }
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let mut actors = Vec::new();
    for (actor_id, responsibility) in demo_actor_catalog() {
        let display_name = connection
            .query_row(
                "SELECT display_name FROM local_users WHERE tenant_id = ?1 AND id = ?2 AND status = 'ACTIVE'",
                [&state.tenant_id(), actor_id],
                |row| row.get::<_, String>(0),
            )
            .map_err(|_| AppError::DataAccess)?;
        actors.push(DemoActor {
            actor_id: actor_id.to_owned(),
            display_name,
            responsibility: responsibility.to_owned(),
        });
    }
    Ok(DemoActorsData {
        actors,
        active_actor_id: state.current_user_id()?,
    })
}

pub fn switch_demo_actor(
    state: &AppState,
    input: SwitchDemoActorInput,
) -> Result<DemoActorsData, AppError> {
    // تبديل فاعل الديمو خارج وضع الديمو مرفوض من الخلف — سلطة الديمو لا تصل الإنتاج.
    if state.runtime_mode()? != RuntimeMode::Demo {
        return Err(AppError::AuthorizationDenied);
    }
    let actor_id = input.actor_id.trim();
    match actor_id {
        DEMO_OPERATOR_USER_ID | DEMO_REVIEWER_USER_ID => {}
        // فشل مغلق: رفض أي معرّف خارج كتالوج فاعلي الديمو — لا انتحال هويات حرة.
        _ => return Err(AppError::ValidationFailed),
    }
    if actor_id == state.current_user_id()? {
        return list_demo_actors(state);
    }

    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let target_active: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM local_users WHERE tenant_id = ?1 AND id = ?2 AND status = 'ACTIVE')",
            [&state.tenant_id(), actor_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)?;
    if !target_active {
        return Err(AppError::ResourceNotFound);
    }
    // جلسة نشطة للفاعل الهدف شرط للتبديل — التفويض يربط الجلسة النشطة بعينها.
    let target_session = resolve_active_session(&transaction, DEMO_TENANT_ID, actor_id, &now)
        .ok_or(AppError::AuthorizationDenied)?;

    // تدقيق تبديل الفاعل داخل معاملة قبل تفعيل الهوية — فشل التدقيق يمنع التبديل كلياً.
    let previous_identity = state.current_user_id()?;
    let audit_id = format!("audit_{}", uuid::Uuid::new_v4());
    let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
    append_audit_event(
        &transaction,
        &state.tenant_id(),
        &audit_id,
        "DEMO_ACTOR_SWITCHED",
        &previous_identity,
        "DemoIdentity",
        actor_id,
        &now,
        &previous_hash,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    // تحرير قفل الاتصال قبل أي نداء لاحق على الحالة — قفل std غير معاد الدخول.
    drop(connection);

    state.activate_identity(DEMO_TENANT_ID, actor_id, Some(target_session))?;
    list_demo_actors(state)
}

// سياق الجلسة للعرض فقط: الفاعل النشط وصلاحياته الفعلية كما يحسبها الخلف.
// الواجهة تستخدمه لإخفاء ما لا يملكه الفاعل (تجربة استخدام) —
// والتفويض اللامع ظل مصدره الخلف وحده؛ هذه القراءة لا تمنح شيئاً.
pub fn get_session_context(state: &AppState) -> Result<SessionContext, AppError> {
    let actor_id = state.current_user_id()?;
    let now = Utc::now().to_rfc3339();
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;

    let display_name = connection
        .query_row(
            "SELECT display_name FROM local_users WHERE tenant_id = ?1 AND id = ?2 AND status = 'ACTIVE'",
            [&state.tenant_id(), actor_id.as_str()],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| AppError::DataAccess)?;

    // مسؤولية العرض: كتالوج الديمو لفاعليه؛ وللمستخدم الإنتاجي بحسب دوره الأول.
    let responsibility = if let Some((_, responsibility)) = demo_actor_catalog()
        .iter()
        .find(|(catalog_id, _)| *catalog_id == actor_id)
    {
        (*responsibility).to_owned()
    } else {
        let has_admin_role: bool = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM user_role_assignments ura
                    INNER JOIN roles r ON r.id = ura.role_id
                    WHERE ura.user_id = ?1 AND r.tenant_id = ?2 AND r.code = 'TENANT_ADMIN'
                )",
                [&actor_id, &state.tenant_id()],
                |row| row.get(0),
            )
            .unwrap_or(false);
        if has_admin_role {
            "ADMINISTRATOR".to_owned()
        } else {
            "MEMBER".to_owned()
        }
    };

    // نفس مسار التفويض نفسه (جلسة نشطة + دور + منح) — لا مصدر ثانٍ للحقيقة.
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT p.code
             FROM permissions p
             INNER JOIN role_permissions rp ON rp.permission_id = p.id
             INNER JOIN user_role_assignments ura ON ura.role_id = rp.role_id
             INNER JOIN local_users u ON u.id = ura.user_id AND u.tenant_id = ?1 AND u.status = 'ACTIVE'
             INNER JOIN local_sessions s ON s.user_id = u.id AND s.tenant_id = u.tenant_id
                  AND s.revoked_at IS NULL AND s.expires_at > ?2
             WHERE u.id = ?3
             ORDER BY p.code ASC",
        )
        .map_err(|_| AppError::DataAccess)?;
    let permissions = statement
        .query_map(
            rusqlite::params![&state.tenant_id(), &now, &actor_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| AppError::DataAccess)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::DataAccess)?;

    Ok(SessionContext {
        actor_id,
        display_name,
        responsibility,
        permissions,
    })
}

#[cfg(test)]
#[path = "demo_identity_service_tests.rs"]
mod tests;
