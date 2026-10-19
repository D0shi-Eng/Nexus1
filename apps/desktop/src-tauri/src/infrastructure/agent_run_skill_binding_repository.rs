use rusqlite::{params, OptionalExtension, Transaction};

use crate::domain::{error::AppError, trusted_skill_selection::TrustedSkillSelection};

pub struct AgentRunSkillBindingRepository;

impl AgentRunSkillBindingRepository {
    pub fn insert(
        transaction: &Transaction<'_>,
        run_id: &str,
        revision: i64,
        selection: &TrustedSkillSelection,
        bound_at: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO agent_run_skill_bindings
                 (agent_run_id, tenant_id, agent_run_revision, agent_id, skill_id, skill_version_id, bound_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    run_id,
                    selection.tenant_id(),
                    revision,
                    selection.agent_id(),
                    selection.skill_id(),
                    selection.skill_version_id(),
                    bound_at,
                ],
            )
            .map_err(|error| {
                if matches!(error, rusqlite::Error::SqliteFailure(_, _)) {
                    AppError::Conflict
                } else {
                    AppError::StorageOperation
                }
            })?;
        Ok(())
    }

    pub fn find(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        run_id: &str,
    ) -> Result<Option<TrustedSkillSelection>, AppError> {
        transaction
            .query_row(
                "SELECT tenant_id, agent_id, skill_id, skill_version_id
                 FROM agent_run_skill_bindings
                 WHERE tenant_id = ?1 AND agent_run_id = ?2",
                params![tenant_id, run_id],
                |row| {
                    TrustedSkillSelection::new(row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                0,
                                rusqlite::types::Type::Text,
                                Box::new(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    error.to_string(),
                                )),
                            )
                        })
                },
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find_revision(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        run_id: &str,
    ) -> Result<Option<i64>, AppError> {
        transaction
            .query_row(
                "SELECT agent_run_revision
                 FROM agent_run_skill_bindings
                 WHERE tenant_id = ?1 AND agent_run_id = ?2",
                params![tenant_id, run_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }
}
