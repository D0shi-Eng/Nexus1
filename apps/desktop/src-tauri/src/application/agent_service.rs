use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        models::{parse_agent_status, AgentRecord, AgentStatus, AgentsData},
    },
    infrastructure::{
        agent_repository::{AgentRepository, AgentUpdateValues},
        database::{append_audit_event, latest_audit_event_hash, AppState},
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAgentInput {
    pub name: String,
    pub description: String,
    pub capability: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAgentInput {
    pub agent_id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetAgentStatusInput {
    pub agent_id: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMutationResult {
    pub agent: AgentRecord,
}

pub fn list_agents(state: &AppState) -> Result<AgentsData, AppError> {
    authorization_service::require_permission(state, "AGENT", "READ")?;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let items = AgentRepository::list(&connection, &state.tenant_id())?;
    Ok(AgentsData {
        has_data: !items.is_empty(),
        items,
    })
}

pub fn create_agent(
    state: &AppState,
    input: CreateAgentInput,
) -> Result<AgentMutationResult, AppError> {
    authorization_service::require_permission(state, "AGENT", "CREATE")?;
    validate_descriptive_fields(&input.name, &input.description)?;
    // القدرة من تعداد مغلق حصراً (مع GENERAL الافتراضية) — لا قدرة حرة من العميل
    let capability = match input.capability.as_deref() {
        None => "GENERAL",
        Some(value) => validate_capability(value)?,
    };

    let now = Utc::now().to_rfc3339();
    let mut record = AgentRecord {
        id: format!("agent_{}", Uuid::new_v4()),
        tenant_id: state.tenant_id(),
        name: input.name.trim().to_owned(),
        description: input.description.trim().to_owned(),
        status: AgentStatus::Disabled,
        capability: capability.to_owned(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    AgentRepository::insert(&transaction, &record, capability)?;
    append_agent_audit(&transaction, state, "AGENT_CREATED", &record.id, &now)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    record.capability = capability.to_owned();

    Ok(AgentMutationResult { agent: record })
}

fn validate_capability(value: &str) -> Result<&'static str, AppError> {
    match value.trim() {
        "GENERAL" => Ok("GENERAL"),
        "RESEARCH" => Ok("RESEARCH"),
        "COMPLIANCE" => Ok("COMPLIANCE"),
        "RISK" => Ok("RISK"),
        "EVIDENCE" => Ok("EVIDENCE"),
        "CYBERSECURITY" => Ok("CYBERSECURITY"),
        "SYNTHESIS" => Ok("SYNTHESIS"),
        _ => Err(AppError::ValidationFailed),
    }
}

pub fn update_agent(
    state: &AppState,
    input: UpdateAgentInput,
) -> Result<AgentMutationResult, AppError> {
    authorization_service::require_permission(state, "AGENT", "UPDATE")?;
    validate_identifier(&input.agent_id)?;
    validate_descriptive_fields(&input.name, &input.description)?;

    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let Some(_) = AgentRepository::find(&transaction, &state.tenant_id(), &input.agent_id)? else {
        return Err(AppError::ResourceNotFound);
    };
    let changed = AgentRepository::update(
        &transaction,
        &state.tenant_id(),
        &input.agent_id,
        &AgentUpdateValues {
            name: input.name.trim(),
            description: input.description.trim(),
            updated_at: &now,
        },
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    let record = AgentRepository::find(&transaction, &state.tenant_id(), &input.agent_id)?
        .ok_or(AppError::ResourceNotFound)?;
    append_agent_audit(&transaction, state, "AGENT_UPDATED", &record.id, &now)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(AgentMutationResult { agent: record })
}

pub fn set_agent_status(
    state: &AppState,
    input: SetAgentStatusInput,
) -> Result<AgentMutationResult, AppError> {
    authorization_service::require_permission(state, "AGENT", "UPDATE")?;
    validate_identifier(&input.agent_id)?;
    let status = parse_agent_status(input.status).map_err(|_| AppError::ValidationFailed)?;

    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let Some(existing) = AgentRepository::find(&transaction, &state.tenant_id(), &input.agent_id)?
    else {
        return Err(AppError::ResourceNotFound);
    };
    if existing.status == status {
        return Err(AppError::InvalidStateTransition);
    }
    let changed = AgentRepository::set_status(
        &transaction,
        &state.tenant_id(),
        &input.agent_id,
        &status,
        &now,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    let record = AgentRepository::find(&transaction, &state.tenant_id(), &input.agent_id)?
        .ok_or(AppError::ResourceNotFound)?;
    let event_type = match status {
        AgentStatus::Active => "AGENT_ENABLED",
        AgentStatus::Disabled => "AGENT_DISABLED",
    };
    append_agent_audit(&transaction, state, event_type, &record.id, &now)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(AgentMutationResult { agent: record })
}

fn validate_identifier(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

fn validate_descriptive_fields(name: &str, description: &str) -> Result<(), AppError> {
    if name.trim().is_empty()
        || name.chars().count() > 160
        || description.trim().is_empty()
        || description.chars().count() > 2_000
    {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

fn append_agent_audit(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    event_type: &str,
    agent_id: &str,
    occurred_at: &str,
) -> Result<(), AppError> {
    let audit_id = format!("audit_{}", Uuid::new_v4());
    let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
    append_audit_event(
        transaction,
        &state.tenant_id(),
        &audit_id,
        event_type,
        &state.current_user_id()?,
        "Agent",
        agent_id,
        occurred_at,
        &previous_hash,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {

    use chrono::Utc;
    use rusqlite::Connection;

    use super::{
        create_agent, list_agents, set_agent_status, update_agent, CreateAgentInput,
        SetAgentStatusInput, UpdateAgentInput,
    };
    use crate::{
        domain::{error::AppError, models::AgentStatus},
        infrastructure::database::AppState,
    };

    fn state_with_permissions(permissions: &[&str]) -> AppState {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        crate::infrastructure::database::apply_migrations(&connection)
            .expect("هجرة الاختبار يجب أن تنجح");
        let now = Utc::now().to_rfc3339();
        create_tenant(&connection, "tenant-agent-a", &now);
        create_tenant(&connection, "tenant-agent-b", &now);
        create_identity(
            &connection,
            "tenant-agent-a",
            "user-agent-a",
            "role-agent-a",
            "session-agent-a",
            permissions,
            &now,
        );

        AppState::test_instance(
            connection,
            "tenant-agent-a".to_owned(),
            "user-agent-a".to_owned(),
        )
    }

    fn create_tenant(connection: &Connection, tenant_id: &str, now: &str) {
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?1, 'DEMO_SYNTHETIC', ?2, ?2)",
                (tenant_id, now),
            )
            .expect("المستأجر يجب أن ينشأ");
    }

    fn create_identity(
        connection: &Connection,
        tenant_id: &str,
        user_id: &str,
        role_id: &str,
        session_id: &str,
        permissions: &[&str],
        now: &str,
    ) {
        connection
            .execute(
                "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, ?2, ?1, ?3, NULL, 'ACTIVE', ?4, ?4)",
                (user_id, tenant_id, format!("{user_id}@test.local"), now),
            )
            .expect("المستخدم يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3, ?4, ?4)",
                (role_id, tenant_id, role_id, now),
            )
            .expect("الدور يجب أن ينشأ");
        for action in permissions {
            let permission_id = format!("permission-agent-{}", action.to_lowercase());
            let permission_code = format!("agent:{}", action.to_lowercase());
            connection
                .execute(
                    "INSERT OR IGNORE INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES (?1, ?2, 'AGENT', ?3, 'HIGHLY_RESTRICTED', ?4)",
                    (&permission_id, &permission_code, action, now),
                )
                .expect("الصلاحية يجب أن تنشأ");
            connection
                .execute(
                    "INSERT INTO role_permissions (role_id, permission_id) VALUES (?1, ?2)",
                    (role_id, &permission_id),
                )
                .expect("تعيين الصلاحية يجب أن ينجح");
        }
        connection
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, ?2, ?3, 'system')",
                (user_id, role_id, now),
            )
            .expect("تعيين الدور يجب أن ينجح");
        connection
            .execute(
                "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, ?2, ?3, ?4, '2030-01-01T00:00:00Z', NULL, 'test')",
                (session_id, tenant_id, user_id, now),
            )
            .expect("الجلسة يجب أن تنشأ");
    }

    fn create_test_agent(state: &AppState) -> String {
        create_agent(
            state,
            CreateAgentInput {
                name: "Governance registry entry".to_owned(),
                description: "A governed agent definition used for tests only.".to_owned(),
                capability: None,
            },
        )
        .expect("إنشاء Agent يجب أن ينجح")
        .agent
        .id
    }

    #[test]
    fn migration_enforces_agent_constraints_foreign_key_and_index() {
        let state = state_with_permissions(&[]);
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let indexes: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_index_list('agents') WHERE name = 'idx_agents_tenant_status_updated'",
                [],
                |row| row.get(0),
            )
            .expect("الفهرس يجب أن يقرأ");
        assert_eq!(indexes, 1);
        assert!(connection.execute("INSERT INTO agents (id, tenant_id, name, description, status, created_at, updated_at) VALUES ('invalid-status', 'tenant-agent-a', 'Name', 'Description', 'UNKNOWN', 'now', 'now')", []).is_err());
        assert!(connection.execute("INSERT INTO agents (id, tenant_id, name, description, status, created_at, updated_at) VALUES ('invalid-tenant', 'unknown-tenant', 'Name', 'Description', 'DISABLED', 'now', 'now')", []).is_err());
        let foreign_key_errors: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("فحص المفاتيح الأجنبية يجب أن يقرأ");
        assert_eq!(foreign_key_errors, 0);
    }

    #[test]
    fn creates_disabled_agent_and_audits_the_mutation() {
        let state = state_with_permissions(&["CREATE"]);
        let result = create_agent(
            &state,
            CreateAgentInput {
                name: "Compliance definition".to_owned(),
                description: "A controlled registry definition without execution runtime."
                    .to_owned(),
                capability: None,
            },
        )
        .expect("الإنشاء المصرح به يجب أن ينجح");

        assert_eq!(result.agent.status, AgentStatus::Disabled);
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let stored: String = connection
            .query_row(
                "SELECT status FROM agents WHERE id = ?1 AND tenant_id = 'tenant-agent-a'",
                [&result.agent.id],
                |row| row.get(0),
            )
            .expect("Agent يجب أن يخزن");
        let audit_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM audit_events WHERE resource_id = ?1 AND event_type = 'AGENT_CREATED'", [&result.agent.id], |row| row.get(0))
            .expect("حدث التدقيق يجب أن يقرأ");
        assert_eq!(stored, "DISABLED");
        assert_eq!(audit_count, 1);
    }

    #[test]
    fn reads_empty_tenant_without_audit_event() {
        let state = state_with_permissions(&["READ"]);
        let before: i64 = state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
            .expect("عدد التدقيق يجب أن يقرأ");
        let result = list_agents(&state).expect("القراءة المصرح بها يجب أن تنجح");
        let after: i64 = state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
            .expect("عدد التدقيق يجب أن يقرأ");
        assert!(!result.has_data);
        assert!(result.items.is_empty());
        assert_eq!(before, after);
    }

    #[test]
    fn updates_and_changes_status_with_separate_audit_events() {
        let state = state_with_permissions(&["CREATE", "UPDATE"]);
        let agent_id = create_test_agent(&state);
        let updated = update_agent(
            &state,
            UpdateAgentInput {
                agent_id: agent_id.clone(),
                name: "Updated compliance definition".to_owned(),
                description: "An updated controlled registry definition without execution runtime."
                    .to_owned(),
            },
        )
        .expect("التحديث يجب أن ينجح");
        let enabled = set_agent_status(
            &state,
            SetAgentStatusInput {
                agent_id: agent_id.clone(),
                status: "ACTIVE".to_owned(),
            },
        )
        .expect("التفعيل يجب أن ينجح");
        assert_eq!(updated.agent.name, "Updated compliance definition");
        assert_eq!(enabled.agent.status, AgentStatus::Active);
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let audit_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM audit_events WHERE resource_id = ?1 AND event_type IN ('AGENT_CREATED', 'AGENT_UPDATED', 'AGENT_ENABLED')", [&agent_id], |row| row.get(0))
            .expect("أحداث التدقيق يجب أن تقرأ");
        assert_eq!(audit_count, 3);
    }

    #[test]
    fn rejects_unauthorized_expired_revoked_locked_and_wrong_tenant_requests() {
        let state = state_with_permissions(&[]);
        assert!(matches!(
            list_agents(&state),
            Err(AppError::AuthorizationDenied)
        ));

        let expired_state = state_with_permissions(&["READ"]);
        expired_state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .execute(
                "UPDATE local_sessions SET expires_at = '2020-01-01T00:00:00Z'",
                [],
            )
            .expect("الجلسة يجب أن تحدث");
        assert!(matches!(
            list_agents(&expired_state),
            Err(AppError::AuthorizationDenied)
        ));

        let revoked_state = state_with_permissions(&["READ"]);
        revoked_state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .execute(
                "UPDATE local_sessions SET revoked_at = '2026-01-01T00:00:00Z'",
                [],
            )
            .expect("الجلسة يجب أن تحدث");
        assert!(matches!(
            list_agents(&revoked_state),
            Err(AppError::AuthorizationDenied)
        ));

        let locked_state = state_with_permissions(&["READ"]);
        locked_state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .execute("UPDATE local_users SET status = 'LOCKED'", [])
            .expect("المستخدم يجب أن يحدث");
        assert!(matches!(
            list_agents(&locked_state),
            Err(AppError::AuthorizationDenied)
        ));

        let wrong_tenant_state = state_with_permissions(&["READ"]);
        wrong_tenant_state.set_active_tenant_for_tests("tenant-agent-b");
        assert!(matches!(
            list_agents(&wrong_tenant_state),
            Err(AppError::AuthorizationDenied)
        ));
    }

    #[test]
    fn isolates_agents_between_tenants_and_rejects_cross_tenant_mutation() {
        let state = state_with_permissions(&["CREATE", "READ", "UPDATE"]);
        let agent_a = create_test_agent(&state);
        let now = Utc::now().to_rfc3339();
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            create_identity(
                &connection,
                "tenant-agent-b",
                "user-agent-b",
                "role-agent-b",
                "session-agent-b",
                &["READ", "UPDATE"],
                &now,
            );
            connection.execute("INSERT INTO agents (id, tenant_id, name, description, status, created_at, updated_at) VALUES ('agent-b', 'tenant-agent-b', 'Tenant B agent', 'Tenant B isolated definition.', 'DISABLED', ?1, ?1)", [&now]).expect("Agent ب يجب أن ينشأ");
        }
        state.set_active_tenant_for_tests("tenant-agent-b");
        state.set_active_user_for_tests("user-agent-b");

        let list = list_agents(&state).expect("قراءة المستأجر ب يجب أن تنجح");
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].id, "agent-b");
        let mutation = set_agent_status(
            &state,
            SetAgentStatusInput {
                agent_id: agent_a.clone(),
                status: "ACTIVE".to_owned(),
            },
        );
        assert!(matches!(mutation, Err(AppError::ResourceNotFound)));
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let status: String = connection
            .query_row(
                "SELECT status FROM agents WHERE id = ?1",
                [&agent_a],
                |row| row.get(0),
            )
            .expect("Agent أ يجب أن يقرأ");
        assert_eq!(status, "DISABLED");
    }

    #[test]
    fn rejects_invalid_persisted_status_and_invalid_transition() {
        let state = state_with_permissions(&["CREATE", "READ", "UPDATE"]);
        let agent_id = create_test_agent(&state);
        assert!(matches!(
            set_agent_status(
                &state,
                SetAgentStatusInput {
                    agent_id: agent_id.clone(),
                    status: "DISABLED".to_owned()
                }
            ),
            Err(AppError::InvalidStateTransition)
        ));
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute_batch("PRAGMA ignore_check_constraints = ON;")
                .expect("تعطيل القيود مؤقتاً يجب أن ينجح");
            connection
                .execute(
                    "UPDATE agents SET status = 'UNKNOWN' WHERE id = ?1",
                    [&agent_id],
                )
                .expect("تخزين القيمة غير الصالحة للاختبار يجب أن ينجح");
            connection
                .execute_batch("PRAGMA ignore_check_constraints = OFF;")
                .expect("استعادة القيود يجب أن تنجح");
        }
        assert!(matches!(list_agents(&state), Err(AppError::DataAccess)));
    }

    #[test]
    fn rolls_back_agent_insert_when_audit_write_fails() {
        let state = state_with_permissions(&["CREATE"]);
        state.connection.lock().expect("القفل يجب أن يتاح").execute_batch("CREATE TRIGGER reject_agent_audit BEFORE INSERT ON audit_events WHEN NEW.event_type = 'AGENT_CREATED' BEGIN SELECT RAISE(ABORT, 'audit rejected'); END;").expect("مشغل الاختبار يجب أن ينشأ");
        let result = create_agent(
            &state,
            CreateAgentInput {
                name: "Rollback definition".to_owned(),
                description: "This record must roll back when its audit event cannot be stored."
                    .to_owned(),
                capability: None,
            },
        );
        assert!(matches!(result, Err(AppError::StorageOperation)));
        let count: i64 = state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .query_row("SELECT COUNT(*) FROM agents", [], |row| row.get(0))
            .expect("عدد Agents يجب أن يقرأ");
        assert_eq!(count, 0);
    }
}
