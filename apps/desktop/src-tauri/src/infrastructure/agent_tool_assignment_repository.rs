use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;

use crate::domain::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolAssignmentRecord {
    pub id: String,
    pub tenant_id: String,
    pub agent_id: String,
    pub tool_version_id: String,
    pub assigned_at: String,
    pub unassigned_at: Option<String>,
}

pub struct AgentToolAssignmentRepository;

impl AgentToolAssignmentRepository {
    pub fn list_active_for_agent(
        connection: &Connection,
        tenant_id: &str,
        agent_id: &str,
    ) -> Result<Vec<AgentToolAssignmentRecord>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT id, tenant_id, agent_id, tool_version_id, assigned_at, unassigned_at
                 FROM agent_tool_assignments
                 WHERE tenant_id = ?1 AND agent_id = ?2 AND unassigned_at IS NULL
                 ORDER BY assigned_at DESC, id ASC",
            )
            .map_err(|_| AppError::StorageOperation)?;
        let rows = statement
            .query_map(params![tenant_id, agent_id], map_assignment)
            .map_err(|_| AppError::StorageOperation)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find_active(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        agent_id: &str,
        tool_version_id: &str,
    ) -> Result<Option<AgentToolAssignmentRecord>, AppError> {
        transaction
            .query_row(
                "SELECT id, tenant_id, agent_id, tool_version_id, assigned_at, unassigned_at
                 FROM agent_tool_assignments
                 WHERE tenant_id = ?1 AND agent_id = ?2 AND tool_version_id = ?3 AND unassigned_at IS NULL",
                params![tenant_id, agent_id, tool_version_id],
                map_assignment,
            )
            .optional()
            .map_err(|_| AppError::StorageOperation)
    }

    pub fn insert(
        transaction: &Transaction<'_>,
        record: &AgentToolAssignmentRecord,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO agent_tool_assignments (id, tenant_id, agent_id, tool_version_id, assigned_at, unassigned_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                params![
                    record.id,
                    record.tenant_id,
                    record.agent_id,
                    record.tool_version_id,
                    record.assigned_at
                ],
            )
            .map_err(map_insert_error)?;
        Ok(())
    }

    // إلغاء منطقي فقط (unassigned_at) — التاريخ يبقى قابلاً للتدقيق.
    pub fn unassign(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        assignment_id: &str,
        unassigned_at: &str,
    ) -> Result<bool, AppError> {
        let changed = transaction
            .execute(
                "UPDATE agent_tool_assignments SET unassigned_at = ?3 WHERE tenant_id = ?1 AND id = ?2 AND unassigned_at IS NULL",
                params![tenant_id, assignment_id, unassigned_at],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(changed == 1)
    }
}

fn map_insert_error(error: rusqlite::Error) -> AppError {
    match &error {
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            AppError::Conflict
        }
        _ => AppError::StorageOperation,
    }
}

fn map_assignment(row: &rusqlite::Row<'_>) -> Result<AgentToolAssignmentRecord, rusqlite::Error> {
    Ok(AgentToolAssignmentRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        agent_id: row.get(2)?,
        tool_version_id: row.get(3)?,
        assigned_at: row.get(4)?,
        unassigned_at: row.get(5)?,
    })
}
