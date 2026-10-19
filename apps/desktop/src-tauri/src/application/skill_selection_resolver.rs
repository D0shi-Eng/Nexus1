use rusqlite::{params, OptionalExtension, Transaction};

use crate::{
    domain::{error::AppError, trusted_skill_selection::TrustedSkillSelection},
    infrastructure::database::AppState,
};

pub struct SkillSelectionResolver;

impl SkillSelectionResolver {
    pub fn resolve(
        transaction: &Transaction<'_>,
        state: &AppState,
        agent_id: &str,
        requested_skill_version_id: Option<&str>,
    ) -> Result<Option<TrustedSkillSelection>, AppError> {
        validate_identifier(agent_id)?;
        let Some(version_id) = requested_skill_version_id else {
            return Ok(None);
        };
        validate_identifier(version_id)?;

        transaction
            .query_row(
                "SELECT a.tenant_id, a.agent_id, s.id, v.id
                 FROM agent_skill_assignments a
                 INNER JOIN agents ag
                   ON ag.tenant_id = a.tenant_id AND ag.id = a.agent_id
                 INNER JOIN skill_versions v
                   ON v.tenant_id = a.tenant_id AND v.id = a.skill_version_id
                 INNER JOIN skills s
                   ON s.tenant_id = v.tenant_id AND s.id = v.skill_id
                 WHERE a.tenant_id = ?1
                   AND a.agent_id = ?2
                   AND a.skill_version_id = ?3
                   AND a.unassigned_at IS NULL
                   AND ag.status = 'ACTIVE'
                   AND s.status = 'ACTIVE'",
                params![&state.tenant_id(), agent_id, version_id],
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
            .map_err(|_| AppError::DataAccess)?
            .ok_or(AppError::ResourceNotFound)
            .map(Some)
    }
}

fn validate_identifier(value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() || value.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

#[cfg(test)]
#[path = "skill_selection_resolver_tests.rs"]
mod tests;
