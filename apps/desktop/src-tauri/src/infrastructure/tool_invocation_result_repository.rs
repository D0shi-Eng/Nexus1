use rusqlite::{params, Connection, Transaction};

use crate::domain::{error::AppError, tool_invocation::ToolInvocationResultRecord};

pub struct ToolInvocationResultRepository;

impl ToolInvocationResultRepository {
    pub fn insert(
        transaction: &Transaction<'_>,
        record: &ToolInvocationResultRecord,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO tool_invocation_results (id, tenant_id, tool_invocation_id, outcome, output_json, failure_code, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    record.id,
                    record.tenant_id,
                    record.tool_invocation_id,
                    record.outcome,
                    record.output_json,
                    record.failure_code,
                    record.created_at
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn find(
        connection: &Connection,
        tenant_id: &str,
        tool_invocation_id: &str,
    ) -> Result<Option<ToolInvocationResultRecord>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT id, tenant_id, tool_invocation_id, outcome, output_json, failure_code, created_at
                 FROM tool_invocation_results
                 WHERE tenant_id = ?1 AND tool_invocation_id = ?2",
            )
            .map_err(|_| AppError::StorageOperation)?;
        let mut rows = statement
            .query_map(params![tenant_id, tool_invocation_id], map_result)
            .map_err(|_| AppError::StorageOperation)?;
        rows.next().transpose().map_err(|_| AppError::DataAccess)
    }
}

fn map_result(row: &rusqlite::Row<'_>) -> Result<ToolInvocationResultRecord, rusqlite::Error> {
    Ok(ToolInvocationResultRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        tool_invocation_id: row.get(2)?,
        outcome: row.get(3)?,
        output_json: row.get(4)?,
        failure_code: row.get(5)?,
        created_at: row.get(6)?,
    })
}
