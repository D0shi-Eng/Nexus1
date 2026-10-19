use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::domain::{
    error::AppError,
    skill::{SkillRecord, SkillStatus, SkillVersionRecord},
};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillAssignmentRow {
    pub id: String,
    pub agent_id: String,
    pub skill_version_id: String,
    pub assigned_at: String,
    pub unassigned_at: Option<String>,
}

pub struct SkillRepository;

impl SkillRepository {
    pub fn list(connection: &Connection, tenant_id: &str) -> Result<Vec<SkillRecord>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT id, tenant_id, name, description, status, created_at, updated_at
                 FROM skills WHERE tenant_id = ?1 ORDER BY updated_at DESC, id ASC",
            )
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id], map_skill)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        skill_id: &str,
    ) -> Result<Option<SkillRecord>, AppError> {
        transaction
            .query_row(
                "SELECT id, tenant_id, name, description, status, created_at, updated_at
                 FROM skills WHERE id = ?1 AND tenant_id = ?2",
                params![skill_id, tenant_id],
                map_skill,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn insert(transaction: &Transaction<'_>, skill: &SkillRecord) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO skills (id, tenant_id, name, description, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    &skill.id,
                    &skill.tenant_id,
                    &skill.name,
                    &skill.description,
                    skill.status.as_code(),
                    &skill.created_at,
                    &skill.updated_at,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn insert_version(
        transaction: &Transaction<'_>,
        version: &SkillVersionRecord,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO skill_versions
                 (id, tenant_id, skill_id, version_number, instruction, input_contract, output_contract, constraints, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    &version.id,
                    &version.tenant_id,
                    &version.skill_id,
                    version.version_number,
                    &version.instruction,
                    &version.input_contract,
                    &version.output_contract,
                    &version.constraints,
                    &version.created_at,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn latest_version_number(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        skill_id: &str,
    ) -> Result<i64, AppError> {
        transaction
            .query_row(
                "SELECT COALESCE(MAX(version_number), 0) FROM skill_versions
                 WHERE tenant_id = ?1 AND skill_id = ?2",
                params![tenant_id, skill_id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find_version(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        version_id: &str,
    ) -> Result<Option<SkillVersionRecord>, AppError> {
        transaction
            .query_row(
                "SELECT v.id, v.tenant_id, v.skill_id, s.status, v.version_number,
                        v.instruction, v.input_contract, v.output_contract, v.constraints, v.created_at
                 FROM skill_versions v
                 INNER JOIN skills s ON s.tenant_id = v.tenant_id AND s.id = v.skill_id
                 WHERE v.id = ?1 AND v.tenant_id = ?2",
                params![version_id, tenant_id],
                map_version,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn set_status(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        skill_id: &str,
        status: SkillStatus,
        updated_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE skills SET status = ?1, updated_at = ?2
                 WHERE id = ?3 AND tenant_id = ?4",
                params![status.as_code(), updated_at, skill_id, tenant_id],
            )
            .map(|changed| changed == 1)
            .map_err(|_| AppError::StorageOperation)
    }

    // نسخ مهارة معينة للكتالوج (الأحدث أولاً).
    pub fn list_versions_for_skill(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        skill_id: &str,
    ) -> Result<Vec<SkillVersionRecord>, AppError> {
        let mut statement = transaction
            .prepare(
                "SELECT v.id, v.tenant_id, v.skill_id, s.status, v.version_number,
                        v.instruction, v.input_contract, v.output_contract, v.constraints, v.created_at
                 FROM skill_versions v
                 INNER JOIN skills s ON s.tenant_id = v.tenant_id AND s.id = v.skill_id
                 WHERE v.tenant_id = ?1 AND v.skill_id = ?2
                 ORDER BY v.version_number DESC",
            )
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id, skill_id], map_version)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    // تعيينات مهارة (نشطة ومؤرَّخة) لعرض الكتالوج.
    pub fn list_assignments_for_skill(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        skill_id: &str,
    ) -> Result<Vec<SkillAssignmentRow>, AppError> {
        let mut statement = transaction
            .prepare(
                "SELECT a.id, a.agent_id, a.skill_version_id, a.assigned_at, a.unassigned_at
                 FROM agent_skill_assignments a
                 INNER JOIN skill_versions v ON v.tenant_id = a.tenant_id AND v.id = a.skill_version_id
                 WHERE a.tenant_id = ?1 AND v.skill_id = ?2
                 ORDER BY a.assigned_at DESC, a.id ASC",
            )
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id, skill_id], |row| {
                Ok(SkillAssignmentRow {
                    id: row.get(0)?,
                    agent_id: row.get(1)?,
                    skill_version_id: row.get(2)?,
                    assigned_at: row.get(3)?,
                    unassigned_at: row.get(4)?,
                })
            })
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn assign(
        transaction: &Transaction<'_>,
        assignment_id: &str,
        tenant_id: &str,
        agent_id: &str,
        version_id: &str,
        assigned_at: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO agent_skill_assignments
                 (id, tenant_id, agent_id, skill_version_id, assigned_at, unassigned_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
                params![assignment_id, tenant_id, agent_id, version_id, assigned_at],
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

    pub fn unassign(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        assignment_id: &str,
        unassigned_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE agent_skill_assignments SET unassigned_at = ?1
                 WHERE id = ?2 AND tenant_id = ?3 AND unassigned_at IS NULL",
                params![unassigned_at, assignment_id, tenant_id],
            )
            .map(|changed| changed == 1)
            .map_err(|_| AppError::StorageOperation)
    }
}

fn map_skill(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillRecord> {
    Ok(SkillRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        status: SkillStatus::from_code(row.get::<_, String>(4)?.as_str()).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    error.to_string(),
                )),
            )
        })?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn map_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<SkillVersionRecord> {
    Ok(SkillVersionRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        skill_id: row.get(2)?,
        skill_status: SkillStatus::from_code(row.get::<_, String>(3)?.as_str()).map_err(
            |error| {
                rusqlite::Error::FromSqlConversionFailure(
                    3,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        error.to_string(),
                    )),
                )
            },
        )?,
        version_number: row.get(4)?,
        instruction: row.get(5)?,
        input_contract: row.get(6)?,
        output_contract: row.get(7)?,
        constraints: row.get(8)?,
        created_at: row.get(9)?,
    })
}
