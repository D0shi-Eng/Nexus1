use rusqlite::{params, OptionalExtension, Transaction};

use crate::domain::{
    error::AppError,
    execution_result::{
        ExecutionFailureCode, ExecutionResult, ExecutionResultOutcome, ExecutionResultRecord,
    },
    provider::{ModelId, ProviderId},
};

pub struct ExecutionResultRepository;

impl ExecutionResultRepository {
    pub fn find(
        connection: &rusqlite::Connection,
        tenant_id: &str,
        agent_run_id: &str,
    ) -> Result<Option<ExecutionResultRecord>, AppError> {
        connection
            .query_row(
                "SELECT id, tenant_id, agent_run_id, agent_run_revision, outcome, provider_id,
                        model_id, output, failure_code, provider_generated_at, created_at
                 FROM agent_execution_results
                 WHERE tenant_id = ?1 AND agent_run_id = ?2",
                params![tenant_id, agent_run_id],
                map_execution_result,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find_in_transaction(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        agent_run_id: &str,
    ) -> Result<Option<ExecutionResultRecord>, AppError> {
        transaction
            .query_row(
                "SELECT id, tenant_id, agent_run_id, agent_run_revision, outcome, provider_id,
                        model_id, output, failure_code, provider_generated_at, created_at
                 FROM agent_execution_results
                 WHERE tenant_id = ?1 AND agent_run_id = ?2",
                params![tenant_id, agent_run_id],
                map_execution_result,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    // مسار MP04: نفس النتيجة المحصنة + الاقتباسات المتحققة وtruth_class المطبوع
    // (الافتراضي AI_INTERPRETATION — لا يرقى النظام به تلقائياً C-8).
    pub fn insert_with_intelligence(
        transaction: &Transaction<'_>,
        record: &ExecutionResultRecord,
        citations_json: Option<&str>,
        truth_class: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO agent_execution_results (
                    id, tenant_id, agent_run_id, agent_run_revision, outcome, provider_id,
                    model_id, output, failure_code, provider_generated_at, created_at,
                    citations_json, truth_class
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    &record.id,
                    &record.tenant_id,
                    &record.agent_run_id,
                    record.agent_run_revision,
                    record.result.outcome().as_code(),
                    record.result.provider_id().as_str(),
                    record.result.model_id().map(ModelId::as_str),
                    record.result.output(),
                    record.result.failure_code().map(|value| value.as_code()),
                    record.result.provider_generated_at(),
                    &record.created_at,
                    citations_json,
                    truth_class,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    // قراءة حقول الذكاء للنتيجة — منفصلة عن سجل MP03 المحصن
    pub fn find_intelligence_fields(
        connection: &rusqlite::Connection,
        tenant_id: &str,
        agent_run_id: &str,
    ) -> Result<Option<(Option<String>, String)>, AppError> {
        connection
            .query_row(
                "SELECT citations_json, truth_class FROM agent_execution_results
                 WHERE tenant_id = ?1 AND agent_run_id = ?2",
                params![tenant_id, agent_run_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }
}

fn map_execution_result(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExecutionResultRecord> {
    let map_error = |error: AppError| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    };
    let outcome =
        ExecutionResultOutcome::from_code(row.get::<_, String>(4)?.as_str()).map_err(map_error)?;
    let provider_id = ProviderId::new(row.get::<_, String>(5)?)
        .map_err(|error| map_error(AppError::from(error)))?;
    let model_id = row
        .get::<_, Option<String>>(6)?
        .map(ModelId::new)
        .transpose()
        .map_err(|error| map_error(AppError::from(error)))?;
    let failure_code = row
        .get::<_, Option<String>>(8)?
        .map(|value| ExecutionFailureCode::from_code(value.as_str()))
        .transpose()
        .map_err(map_error)?;
    let result = ExecutionResult::from_persisted(
        outcome,
        provider_id,
        model_id,
        row.get(7)?,
        failure_code,
        row.get(9)?,
    )
    .map_err(map_error)?;

    Ok(ExecutionResultRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        agent_run_id: row.get(2)?,
        agent_run_revision: row.get(3)?,
        result,
        created_at: row.get(10)?,
    })
}
