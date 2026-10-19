use chrono::Utc;
use rusqlite::params;

use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        models::{AttentionData, AttentionItem, AttentionRelatedEntity, AttentionSummary},
    },
    infrastructure::database::AppState,
};

pub fn load_attention(state: &AppState) -> Result<AttentionData, AppError> {
    authorization_service::require_permission(state, "ATTENTION", "READ")?;

    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let data_mode = connection
        .query_row(
            "SELECT data_mode FROM tenants WHERE id = ?1",
            [&state.tenant_id()],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| AppError::ResourceNotFound)?;

    let mut items = load_obligation_attention_items(&connection, &state.tenant_id())?;
    items.extend(load_evidence_attention_items(
        &connection,
        &state.tenant_id(),
    )?);
    items.extend(load_task_attention_items(&connection, &state.tenant_id())?);
    items.extend(load_risk_attention_items(&connection, &state.tenant_id())?);
    items.extend(load_approval_attention_items(
        &connection,
        &state.tenant_id(),
    )?);
    items.sort_by(attention_order);

    let summary = summarize_attention(&items);

    Ok(AttentionData {
        items,
        summary,
        data_mode,
        generated_at: Utc::now().to_rfc3339(),
    })
}

fn load_obligation_attention_items(
    connection: &rusqlite::Connection,
    tenant_id: &str,
) -> Result<Vec<AttentionItem>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT id, title, severity, status, owner, deadline, updated_at
             FROM obligations
             WHERE tenant_id = ?1
               AND archived_at IS NULL
               AND (severity = 'CRITICAL' OR status IN ('AT_RISK', 'REQUIRES_REVIEW'))",
        )
        .map_err(|_| AppError::DataAccess)?;

    let rows = statement
        .query_map(params![tenant_id], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let status: String = row.get(3)?;
            let reason_code = if status == "AT_RISK" {
                "OBLIGATION_AT_RISK"
            } else {
                "OBLIGATION_REQUIRES_REVIEW"
            };

            Ok(AttentionItem {
                id: format!("attention_obligation_{id}"),
                kind: "OBLIGATION".to_owned(),
                severity: row.get(2)?,
                title: title.clone(),
                status,
                reason_code: reason_code.to_owned(),
                source: "LOCAL_WORKSPACE".to_owned(),
                due_at: row.get(5)?,
                resource_id: id.clone(),
                related_entity: Some(AttentionRelatedEntity {
                    kind: "OBLIGATION".to_owned(),
                    id,
                    title,
                }),
                owner: row.get(4)?,
                updated_at: row.get(6)?,
                available_actions: vec![
                    "OPEN".to_owned(),
                    "REVIEW".to_owned(),
                    "CREATE_TASK".to_owned(),
                    "REGISTER_EVIDENCE".to_owned(),
                ],
            })
        })
        .map_err(|_| AppError::DataAccess)?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::DataAccess)
}

fn load_evidence_attention_items(
    connection: &rusqlite::Connection,
    tenant_id: &str,
) -> Result<Vec<AttentionItem>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT o.id, o.title, o.severity, o.evidence_status, o.owner, o.deadline, o.updated_at
             FROM obligations o
             WHERE o.tenant_id = ?1
               AND o.archived_at IS NULL
               AND o.evidence_status IN ('MISSING', 'REQUIRES_REVIEW', 'EXPIRED', 'INVALID')",
        )
        .map_err(|_| AppError::DataAccess)?;

    let rows = statement
        .query_map(params![tenant_id], |row| {
            let obligation_id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let status: String = row.get(3)?;
            let reason_code = match status.as_str() {
                "MISSING" => "EVIDENCE_MISSING",
                "REQUIRES_REVIEW" => "EVIDENCE_REQUIRES_REVIEW",
                "EXPIRED" => "EVIDENCE_EXPIRED",
                _ => "EVIDENCE_INVALID",
            };

            Ok(AttentionItem {
                id: format!("attention_evidence_{obligation_id}"),
                kind: "EVIDENCE".to_owned(),
                severity: row.get(2)?,
                title: title.clone(),
                status,
                reason_code: reason_code.to_owned(),
                source: "LOCAL_WORKSPACE".to_owned(),
                due_at: row.get(5)?,
                resource_id: obligation_id.clone(),
                related_entity: Some(AttentionRelatedEntity {
                    kind: "OBLIGATION".to_owned(),
                    id: obligation_id,
                    title,
                }),
                owner: row.get(4)?,
                updated_at: row.get(6)?,
                available_actions: vec![
                    "OPEN".to_owned(),
                    "REVIEW".to_owned(),
                    "CREATE_TASK".to_owned(),
                    "REGISTER_EVIDENCE".to_owned(),
                ],
            })
        })
        .map_err(|_| AppError::DataAccess)?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::DataAccess)
}

fn load_task_attention_items(
    connection: &rusqlite::Connection,
    tenant_id: &str,
) -> Result<Vec<AttentionItem>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT t.id, t.title, t.priority, t.status, t.due_at, t.updated_at, u.display_name,
                    o.id, o.title
             FROM tasks t
             INNER JOIN obligations o ON o.id = t.obligation_id AND o.tenant_id = t.tenant_id
             LEFT JOIN local_users u ON u.id = t.assignee_user_id AND u.tenant_id = t.tenant_id
             WHERE t.tenant_id = ?1
               AND o.archived_at IS NULL
               AND t.status IN ('OPEN', 'IN_PROGRESS', 'BLOCKED')",
        )
        .map_err(|_| AppError::DataAccess)?;

    let rows = statement
        .query_map(params![tenant_id], |row| {
            let id: String = row.get(0)?;
            let status: String = row.get(3)?;
            let reason_code = match status.as_str() {
                "OPEN" => "TASK_OPEN",
                "IN_PROGRESS" => "TASK_IN_PROGRESS",
                _ => "TASK_BLOCKED",
            };

            Ok(AttentionItem {
                id: format!("attention_task_{id}"),
                kind: "TASK".to_owned(),
                severity: row.get(2)?,
                title: row.get(1)?,
                status,
                reason_code: reason_code.to_owned(),
                source: "LOCAL_WORKSPACE".to_owned(),
                due_at: row.get(4)?,
                resource_id: id,
                related_entity: Some(AttentionRelatedEntity {
                    kind: "OBLIGATION".to_owned(),
                    id: row.get(7)?,
                    title: row.get(8)?,
                }),
                owner: row.get(6)?,
                updated_at: row.get(5)?,
                available_actions: vec!["OPEN".to_owned(), "REVIEW".to_owned()],
            })
        })
        .map_err(|_| AppError::DataAccess)?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::DataAccess)
}

fn load_risk_attention_items(
    connection: &rusqlite::Connection,
    tenant_id: &str,
) -> Result<Vec<AttentionItem>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT r.id, r.title, r.severity, r.status, r.owner, r.updated_at, o.id, o.title
             FROM risks r
             INNER JOIN obligations o ON o.id = r.obligation_id AND o.tenant_id = r.tenant_id
             WHERE r.tenant_id = ?1 AND r.status IN ('OPEN', 'MITIGATING')",
        )
        .map_err(|_| AppError::DataAccess)?;

    let rows = statement
        .query_map(params![tenant_id], |row| {
            let id: String = row.get(0)?;
            let status: String = row.get(3)?;
            let reason_code = if status == "OPEN" {
                "RISK_OPEN"
            } else {
                "RISK_MITIGATING"
            };

            Ok(AttentionItem {
                id: format!("attention_risk_{id}"),
                kind: "RISK".to_owned(),
                severity: row.get(2)?,
                title: row.get(1)?,
                status,
                reason_code: reason_code.to_owned(),
                source: "LOCAL_WORKSPACE".to_owned(),
                due_at: None,
                resource_id: id,
                related_entity: Some(AttentionRelatedEntity {
                    kind: "OBLIGATION".to_owned(),
                    id: row.get(6)?,
                    title: row.get(7)?,
                }),
                owner: row.get(4)?,
                updated_at: row.get(5)?,
                available_actions: vec!["OPEN".to_owned(), "REVIEW".to_owned()],
            })
        })
        .map_err(|_| AppError::DataAccess)?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::DataAccess)
}

fn load_approval_attention_items(
    connection: &rusqlite::Connection,
    tenant_id: &str,
) -> Result<Vec<AttentionItem>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT a.id, a.resource_type, a.resource_id, a.requested_by, a.updated_at,
                    t.title, u.display_name
             FROM approvals a
             LEFT JOIN tasks t ON a.resource_type = 'TASK' AND t.id = a.resource_id AND t.tenant_id = a.tenant_id
             LEFT JOIN local_users u ON u.id = a.requested_by AND u.tenant_id = a.tenant_id
             WHERE a.tenant_id = ?1 AND a.decision = 'PENDING'",
        )
        .map_err(|_| AppError::DataAccess)?;

    let rows = statement
        .query_map(params![tenant_id], |row| {
            let approval_id: String = row.get(0)?;
            let resource_type: String = row.get(1)?;
            let resource_id: String = row.get(2)?;
            let resource_title: Option<String> = row.get(5)?;

            Ok(AttentionItem {
                id: format!("attention_approval_{approval_id}"),
                kind: "APPROVAL".to_owned(),
                severity: "HIGH".to_owned(),
                title: resource_title.unwrap_or_else(|| resource_type.clone()),
                status: "PENDING".to_owned(),
                reason_code: "APPROVAL_PENDING".to_owned(),
                source: "LOCAL_WORKSPACE".to_owned(),
                due_at: None,
                resource_id: approval_id,
                related_entity: Some(AttentionRelatedEntity {
                    kind: resource_type,
                    id: resource_id,
                    title: row
                        .get::<_, Option<String>>(5)?
                        .unwrap_or_else(|| "—".to_owned()),
                }),
                owner: row.get(6)?,
                updated_at: row.get(4)?,
                available_actions: vec!["DECIDE_APPROVAL".to_owned(), "OPEN".to_owned()],
            })
        })
        .map_err(|_| AppError::DataAccess)?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::DataAccess)
}

fn attention_order(left: &AttentionItem, right: &AttentionItem) -> std::cmp::Ordering {
    severity_rank(&left.severity)
        .cmp(&severity_rank(&right.severity))
        .then_with(|| is_overdue(right).cmp(&is_overdue(left)))
        .then_with(|| left.due_at.cmp(&right.due_at))
        .then_with(|| left.updated_at.cmp(&right.updated_at))
}

fn severity_rank(severity: &str) -> u8 {
    match severity {
        "CRITICAL" => 0,
        "HIGH" => 1,
        "MEDIUM" => 2,
        _ => 3,
    }
}

fn is_overdue(item: &AttentionItem) -> bool {
    item.due_at
        .as_deref()
        .is_some_and(|due_at| due_at < Utc::now().to_rfc3339().as_str())
}

fn summarize_attention(items: &[AttentionItem]) -> AttentionSummary {
    AttentionSummary {
        total: items.len() as u32,
        critical: items
            .iter()
            .filter(|item| item.severity == "CRITICAL")
            .count() as u32,
        high: items.iter().filter(|item| item.severity == "HIGH").count() as u32,
        medium: items
            .iter()
            .filter(|item| item.severity == "MEDIUM")
            .count() as u32,
        low: items.iter().filter(|item| item.severity == "LOW").count() as u32,
        pending: items.iter().filter(|item| item.status == "PENDING").count() as u32,
        overdue: items.iter().filter(|item| is_overdue(item)).count() as u32,
        requires_review: items
            .iter()
            .filter(|item| item.status == "REQUIRES_REVIEW")
            .count() as u32,
    }
}

#[cfg(test)]
mod tests {

    use chrono::Utc;
    use rusqlite::Connection;

    use super::load_attention;
    use crate::{domain::error::AppError, infrastructure::database::AppState};

    fn state_with_attention_permission() -> AppState {
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
            .execute_batch(concat!(
                include_str!("../../migrations/0007_obligation_management.sql"),
                include_str!("../../migrations/0025_task_lifecycle.sql"),
                include_str!("../../migrations/0026_production_user_and_authoring.sql")
            ))
            .expect("هجرة دورة حياة المهمة وعيوب UAT يجب أن تنجح");
        let now = Utc::now().to_rfc3339();

        for tenant_id in ["tenant-attention-a", "tenant-attention-b"] {
            connection.execute("INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?1, 'DEMO_SYNTHETIC', ?2, ?2)", (tenant_id, &now)).expect("المستأجر يجب أن ينشأ");
            connection.execute("INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES (?1, ?2, 'Synthetic', 'Synthetic source', NULL, '1', ?3, 0, 'UNVERIFIED', ?3, ?3)", (format!("source-{tenant_id}"), tenant_id, &now)).expect("المصدر يجب أن ينشأ");
            connection.execute("INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'Owner', ?5, 'CRITICAL', 'AT_RISK', 'APPLIES', 'MISSING', 'REQUIRES_REVIEW', ?6, ?6)", (format!("obligation-{tenant_id}"), tenant_id, format!("source-{tenant_id}"), format!("Obligation {tenant_id}"), "2020-01-01T00:00:00Z", &now)).expect("الالتزام يجب أن ينشأ");
        }

        connection.execute("INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES ('user-attention-a', 'tenant-attention-a', 'User A', 'attention-a@test.local', NULL, 'ACTIVE', ?1, ?1)", [&now]).expect("المستخدم يجب أن ينشأ");
        connection.execute("INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-attention-a', 'tenant-attention-a', 'READER', 'Reader', ?1, ?1)", [&now]).expect("الدور يجب أن ينشأ");
        connection.execute("INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES ('permission-attention-read', 'attention:read', 'ATTENTION', 'READ', 'INTERNAL', ?1)", [&now]).expect("الصلاحية يجب أن تنشأ");
        connection.execute("INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user-attention-a', 'role-attention-a', ?1, 'system')", [&now]).expect("تعيين الدور يجب أن ينجح");
        connection.execute("INSERT INTO role_permissions (role_id, permission_id) VALUES ('role-attention-a', 'permission-attention-read')", []).expect("تعيين الصلاحية يجب أن ينجح");
        connection.execute("INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session-attention-a', 'tenant-attention-a', 'user-attention-a', ?1, '2030-01-01T00:00:00Z', NULL, 'test')", [&now]).expect("الجلسة يجب أن تنشأ");

        AppState::test_instance(
            connection,
            "tenant-attention-a".to_owned(),
            "user-attention-a".to_owned(),
        )
    }

    #[test]
    fn loads_semantic_attention_items_for_current_tenant_only() {
        let state = state_with_attention_permission();
        let attention = load_attention(&state).expect("قراءة عناصر الاهتمام المصرح بها يجب أن تنجح");

        assert_eq!(attention.data_mode, "DEMO_SYNTHETIC");
        assert_eq!(attention.items.len(), 2);
        assert!(attention
            .items
            .iter()
            .all(|item| item.resource_id == "obligation-tenant-attention-a"));
        assert_eq!(attention.summary.total, 2);
        assert_eq!(attention.summary.critical, 2);
        assert_eq!(attention.summary.overdue, 2);
    }

    #[test]
    fn each_authorized_tenant_reads_only_its_own_attention_items() {
        let state = state_with_attention_permission();
        let attention_a = load_attention(&state).expect("قراءة المستأجر أ يجب أن تنجح");
        assert!(attention_a
            .items
            .iter()
            .all(|item| item.resource_id == "obligation-tenant-attention-a"));

        let now = Utc::now().to_rfc3339();
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection.execute("INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES ('user-attention-b', 'tenant-attention-b', 'User B', 'attention-b@test.local', NULL, 'ACTIVE', ?1, ?1)", [&now]).expect("مستخدم المستأجر ب يجب أن ينشأ");
            connection.execute("INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-attention-b', 'tenant-attention-b', 'READER', 'Reader', ?1, ?1)", [&now]).expect("دور المستأجر ب يجب أن ينشأ");
            connection.execute("INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user-attention-b', 'role-attention-b', ?1, 'system')", [&now]).expect("تعيين دور المستأجر ب يجب أن ينجح");
            connection.execute("INSERT INTO role_permissions (role_id, permission_id) VALUES ('role-attention-b', 'permission-attention-read')", []).expect("صلاحية المستأجر ب يجب أن تمنح");
            connection.execute("INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session-attention-b', 'tenant-attention-b', 'user-attention-b', ?1, '2030-01-01T00:00:00Z', NULL, 'test')", [&now]).expect("جلسة المستأجر ب يجب أن تنشأ");
        }

        state.set_active_tenant_for_tests("tenant-attention-b");
        state.set_active_user_for_tests("user-attention-b");
        let attention_b = load_attention(&state).expect("قراءة المستأجر ب يجب أن تنجح");

        assert!(attention_b
            .items
            .iter()
            .all(|item| item.resource_id == "obligation-tenant-attention-b"));
        assert_ne!(
            attention_a.items[0].resource_id,
            attention_b.items[0].resource_id
        );
    }

    #[test]
    fn denies_attention_read_for_an_expired_session() {
        let state = state_with_attention_permission();
        state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .execute(
                "UPDATE local_sessions SET expires_at = '2020-01-01T00:00:00Z'",
                [],
            )
            .expect("إيقاف الجلسة يجب أن ينجح");

        let result = load_attention(&state);

        assert!(matches!(result, Err(AppError::AuthorizationDenied)));
    }

    #[test]
    fn denies_attention_read_without_permission() {
        let state = state_with_attention_permission();
        state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .execute("DELETE FROM role_permissions", [])
            .expect("إزالة الصلاحية يجب أن تنجح");

        let result = load_attention(&state);

        assert!(matches!(result, Err(AppError::AuthorizationDenied)));
    }
}
