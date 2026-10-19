use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::domain::{
    error::AppError,
    models::{parse_agent_run_status, AgentRunRecord, AgentRunStatus},
};

pub struct AgentRunRepository;

impl AgentRunRepository {
    pub fn list(connection: &Connection, tenant_id: &str) -> Result<Vec<AgentRunRecord>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT id, tenant_id, agent_id, requested_by_user_id, status, request_summary,
                        revision, started_at, completed_at, created_at, updated_at
                 FROM agent_runs
                 WHERE tenant_id = ?1
                 ORDER BY updated_at DESC, id ASC",
            )
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id], map_agent_run)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find(
        connection: &Connection,
        tenant_id: &str,
        run_id: &str,
    ) -> Result<Option<AgentRunRecord>, AppError> {
        connection
            .query_row(
                "SELECT id, tenant_id, agent_id, requested_by_user_id, status, request_summary,
                        revision, started_at, completed_at, created_at, updated_at
                 FROM agent_runs
                 WHERE id = ?1 AND tenant_id = ?2",
                params![run_id, tenant_id],
                map_agent_run,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find_in_transaction(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        run_id: &str,
    ) -> Result<Option<AgentRunRecord>, AppError> {
        transaction
            .query_row(
                "SELECT id, tenant_id, agent_id, requested_by_user_id, status, request_summary,
                        revision, started_at, completed_at, created_at, updated_at
                 FROM agent_runs
                 WHERE id = ?1 AND tenant_id = ?2",
                params![run_id, tenant_id],
                map_agent_run,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn insert(transaction: &Transaction<'_>, record: &AgentRunRecord) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO agent_runs (
                    id, tenant_id, agent_id, requested_by_user_id, status, request_summary,
                    revision, started_at, completed_at, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    &record.id,
                    &record.tenant_id,
                    &record.agent_id,
                    &record.requested_by_user_id,
                    agent_run_status_code(&record.status),
                    &record.request_summary,
                    record.revision,
                    &record.started_at,
                    &record.completed_at,
                    &record.created_at,
                    &record.updated_at,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn transition_to_running(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        run_id: &str,
        expected_revision: i64,
        started_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE agent_runs
                 SET status = 'RUNNING',
                     revision = ?1,
                     started_at = ?2,
                     completed_at = NULL,
                     updated_at = ?2
                 WHERE id = ?3
                   AND tenant_id = ?4
                   AND status = 'QUEUED'
                   AND revision = ?5",
                params![
                    expected_revision + 1,
                    started_at,
                    run_id,
                    tenant_id,
                    expected_revision
                ],
            )
            .map_err(|_| AppError::StorageOperation)
            .map(|changed| changed == 1)
    }

    pub fn transition(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        run_id: &str,
        expected_status: &AgentRunStatus,
        expected_revision: i64,
        next_status: &AgentRunStatus,
        next_revision: i64,
        completed_at: Option<&str>,
        updated_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE agent_runs
                 SET status = ?1,
                     revision = ?2,
                     completed_at = ?3,
                     updated_at = ?4
                 WHERE id = ?5
                   AND tenant_id = ?6
                   AND status = ?7
                   AND revision = ?8",
                params![
                    agent_run_status_code(next_status),
                    next_revision,
                    completed_at,
                    updated_at,
                    run_id,
                    tenant_id,
                    agent_run_status_code(expected_status),
                    expected_revision,
                ],
            )
            .map_err(|_| AppError::StorageOperation)
            .map(|changed| changed == 1)
    }
}

fn map_agent_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRunRecord> {
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

    Ok(AgentRunRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        agent_id: row.get(2)?,
        requested_by_user_id: row.get(3)?,
        status: parse_agent_run_status(status).map_err(parse_error)?,
        request_summary: row.get(5)?,
        revision: row.get(6)?,
        started_at: row.get(7)?,
        completed_at: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn agent_run_status_code(value: &AgentRunStatus) -> &'static str {
    match value {
        AgentRunStatus::Queued => "QUEUED",
        AgentRunStatus::Running => "RUNNING",
        AgentRunStatus::WaitingApproval => "WAITING_APPROVAL",
        AgentRunStatus::Completed => "COMPLETED",
        AgentRunStatus::Failed => "FAILED",
        AgentRunStatus::Cancelled => "CANCELLED",
    }
}
