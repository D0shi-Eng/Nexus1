use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::domain::{
    error::AppError,
    models::{parse_agent_status, AgentRecord, AgentStatus},
};

pub struct AgentRepository;

pub struct AgentUpdateValues<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub updated_at: &'a str,
}

impl AgentRepository {
    pub fn list(connection: &Connection, tenant_id: &str) -> Result<Vec<AgentRecord>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT id, tenant_id, name, description, status, created_at, updated_at, capability
                 FROM agents
                 WHERE tenant_id = ?1
                 ORDER BY updated_at DESC, id ASC",
            )
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id], map_agent)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        agent_id: &str,
    ) -> Result<Option<AgentRecord>, AppError> {
        transaction
            .query_row(
                "SELECT id, tenant_id, name, description, status, created_at, updated_at, capability
                 FROM agents
                 WHERE id = ?1 AND tenant_id = ?2",
                params![agent_id, tenant_id],
                map_agent,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn insert(
        transaction: &Transaction<'_>,
        record: &AgentRecord,
        capability: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO agents (id, tenant_id, name, description, status, capability, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    &record.id,
                    &record.tenant_id,
                    &record.name,
                    &record.description,
                    agent_status_code(&record.status),
                    capability,
                    &record.created_at,
                    &record.updated_at,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn update(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        agent_id: &str,
        values: &AgentUpdateValues<'_>,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE agents
                 SET name = ?1,
                     description = ?2,
                     updated_at = ?3
                 WHERE id = ?4 AND tenant_id = ?5",
                params![
                    values.name,
                    values.description,
                    values.updated_at,
                    agent_id,
                    tenant_id
                ],
            )
            .map_err(|_| AppError::StorageOperation)
            .map(|changed| changed == 1)
    }

    pub fn set_status(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        agent_id: &str,
        status: &AgentStatus,
        updated_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE agents
                 SET status = ?1,
                     updated_at = ?2
                 WHERE id = ?3 AND tenant_id = ?4",
                params![agent_status_code(status), updated_at, agent_id, tenant_id],
            )
            .map_err(|_| AppError::StorageOperation)
            .map(|changed| changed == 1)
    }

    // اختيار المشرف الحتمي: أقدم وكيل نشط بالقدرة المطلوبة داخل المستأجر —
    // لا اختيار من النموذج ولا عبر مستأجرين (C-11).
    pub fn find_oldest_active_by_capability(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        capability_code: &str,
    ) -> Result<Option<AgentRecord>, AppError> {
        transaction
            .query_row(
                "SELECT id, tenant_id, name, description, status, created_at, updated_at, capability
                 FROM agents
                 WHERE tenant_id = ?1 AND capability = ?2 AND status = 'ACTIVE'
                 ORDER BY created_at ASC, id ASC
                 LIMIT 1",
                params![tenant_id, capability_code],
                map_agent,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }
}

fn map_agent(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRecord> {
    let status: String = row.get(4)?;
    let parse_error = |error: AppError| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    };

    Ok(AgentRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        status: parse_agent_status(status).map_err(parse_error)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        capability: row.get(7)?,
    })
}

fn agent_status_code(value: &AgentStatus) -> &'static str {
    match value {
        AgentStatus::Active => "ACTIVE",
        AgentStatus::Disabled => "DISABLED",
    }
}
