use rusqlite::{params, Connection, Transaction};

use crate::domain::{agent_execution_profile::AgentExecutionProfileVersion, error::AppError};

pub struct AgentExecutionProfileRepository;

impl AgentExecutionProfileRepository {
    // قراءة كل نسخ بروفايل الوكيل داخل المستأجر — الترتيب تنازلياً بالنسخة.
    pub fn list_for_agent(
        connection: &Connection,
        tenant_id: &str,
        agent_id: &str,
    ) -> Result<Vec<AgentExecutionProfileVersion>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT id, tenant_id, agent_id, version_number, instruction, created_at
                 FROM agent_execution_profiles
                 WHERE tenant_id = ?1 AND agent_id = ?2
                 ORDER BY version_number DESC",
            )
            .map_err(|_| AppError::StorageOperation)?;
        let rows = statement
            .query_map(params![tenant_id, agent_id], map_profile)
            .map_err(|_| AppError::StorageOperation)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn latest(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        agent_id: &str,
    ) -> Result<Option<AgentExecutionProfileVersion>, AppError> {
        let mut statement = transaction
            .prepare(
                "SELECT id, tenant_id, agent_id, version_number, instruction, created_at
                 FROM agent_execution_profiles
                 WHERE tenant_id = ?1 AND agent_id = ?2
                 ORDER BY version_number DESC LIMIT 1",
            )
            .map_err(|_| AppError::StorageOperation)?;
        let mut rows = statement
            .query_map(params![tenant_id, agent_id], map_profile)
            .map_err(|_| AppError::StorageOperation)?;
        rows.next().transpose().map_err(|_| AppError::DataAccess)
    }

    pub fn find(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        profile_version_id: &str,
    ) -> Result<Option<AgentExecutionProfileVersion>, AppError> {
        let mut statement = transaction
            .prepare(
                "SELECT id, tenant_id, agent_id, version_number, instruction, created_at
                 FROM agent_execution_profiles
                 WHERE tenant_id = ?1 AND id = ?2",
            )
            .map_err(|_| AppError::StorageOperation)?;
        let mut rows = statement
            .query_map(params![tenant_id, profile_version_id], map_profile)
            .map_err(|_| AppError::StorageOperation)?;
        rows.next().transpose().map_err(|_| AppError::DataAccess)
    }

    pub fn latest_version_number(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        agent_id: &str,
    ) -> Result<i64, AppError> {
        transaction
            .query_row(
                "SELECT COALESCE(MAX(version_number), 0) FROM agent_execution_profiles
                 WHERE tenant_id = ?1 AND agent_id = ?2",
                params![tenant_id, agent_id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::StorageOperation)
    }

    pub fn insert(
        transaction: &Transaction<'_>,
        record: &AgentExecutionProfileVersion,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO agent_execution_profiles (id, tenant_id, agent_id, version_number, instruction, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    record.id,
                    record.tenant_id,
                    record.agent_id,
                    record.version_number,
                    record.instruction,
                    record.created_at
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }
}

fn map_profile(row: &rusqlite::Row<'_>) -> Result<AgentExecutionProfileVersion, rusqlite::Error> {
    Ok(AgentExecutionProfileVersion {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        agent_id: row.get(2)?,
        version_number: row.get(3)?,
        instruction: row.get(4)?,
        created_at: row.get(5)?,
    })
}
