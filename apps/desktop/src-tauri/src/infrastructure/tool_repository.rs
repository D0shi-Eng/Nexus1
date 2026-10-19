use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::domain::{
    error::AppError,
    tool::{ToolContract, ToolEffectProfile, ToolRecord, ToolStatus, ToolVersionRecord},
};

pub struct ToolRepository;

impl ToolRepository {
    pub fn list(connection: &Connection, tenant_id: &str) -> Result<Vec<ToolRecord>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT id, tenant_id, name, description, status, created_at, updated_at
                 FROM tools WHERE tenant_id = ?1 ORDER BY updated_at DESC, id ASC",
            )
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id], map_tool)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        tool_id: &str,
    ) -> Result<Option<ToolRecord>, AppError> {
        transaction
            .query_row(
                "SELECT id, tenant_id, name, description, status, created_at, updated_at
                 FROM tools WHERE id = ?1 AND tenant_id = ?2",
                params![tool_id, tenant_id],
                map_tool,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn insert(transaction: &Transaction<'_>, tool: &ToolRecord) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO tools (id, tenant_id, name, description, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    &tool.id,
                    &tool.tenant_id,
                    &tool.name,
                    &tool.description,
                    tool.status.as_code(),
                    &tool.created_at,
                    &tool.updated_at,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn insert_version(
        transaction: &Transaction<'_>,
        version: &ToolVersionRecord,
    ) -> Result<(), AppError> {
        let input_contract = version.input_contract.to_json()?;
        let output_contract = version.output_contract.to_json()?;
        let (mutation, externality, privilege, reversibility) = version.effect.codes();
        transaction
            .execute(
                "INSERT INTO tool_versions
                 (id, tenant_id, tool_id, version_number, input_contract, output_contract,
                  mutation, externality, privilege, reversibility, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    &version.id,
                    &version.tenant_id,
                    &version.tool_id,
                    version.version_number,
                    input_contract,
                    output_contract,
                    mutation,
                    externality,
                    privilege,
                    reversibility,
                    &version.created_at,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn latest_version_number(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        tool_id: &str,
    ) -> Result<i64, AppError> {
        transaction
            .query_row(
                "SELECT COALESCE(MAX(version_number), 0) FROM tool_versions
                 WHERE tenant_id = ?1 AND tool_id = ?2",
                params![tenant_id, tool_id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find_version(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        version_id: &str,
    ) -> Result<Option<ToolVersionRecord>, AppError> {
        transaction
            .query_row(
                "SELECT v.id, v.tenant_id, v.tool_id, t.status, v.version_number,
                        v.input_contract, v.output_contract, v.mutation, v.externality,
                        v.privilege, v.reversibility, v.created_at
                 FROM tool_versions v
                 INNER JOIN tools t ON t.tenant_id = v.tenant_id AND t.id = v.tool_id
                 WHERE v.id = ?1 AND v.tenant_id = ?2",
                params![version_id, tenant_id],
                map_version,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn list_versions_for_tool(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        tool_id: &str,
    ) -> Result<Vec<ToolVersionRecord>, AppError> {
        let mut statement = transaction
            .prepare(
                "SELECT v.id, v.tenant_id, v.tool_id, t.status, v.version_number,
                        v.input_contract, v.output_contract, v.mutation, v.externality,
                        v.privilege, v.reversibility, v.created_at
                 FROM tool_versions v
                 INNER JOIN tools t ON t.tenant_id = v.tenant_id AND t.id = v.tool_id
                 WHERE v.tenant_id = ?1 AND v.tool_id = ?2
                 ORDER BY v.version_number DESC",
            )
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id, tool_id], map_version)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn set_status(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        tool_id: &str,
        status: ToolStatus,
        updated_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE tools SET status = ?1, updated_at = ?2
                 WHERE id = ?3 AND tenant_id = ?4",
                params![status.as_code(), updated_at, tool_id, tenant_id],
            )
            .map(|changed| changed == 1)
            .map_err(|_| AppError::StorageOperation)
    }
}

fn map_tool(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolRecord> {
    Ok(ToolRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        name: row.get(2)?,
        description: row.get(3)?,
        status: ToolStatus::from_code(row.get::<_, String>(4)?.as_str()).map_err(|error| {
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

fn map_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolVersionRecord> {
    let status = ToolStatus::from_code(row.get::<_, String>(3)?.as_str()).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;
    let input_contract = ToolContract::from_json(&row.get::<_, String>(5)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;
    let output_contract = ToolContract::from_json(&row.get::<_, String>(6)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            6,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;
    let effect = ToolEffectProfile::from_codes(
        &row.get::<_, String>(7)?,
        &row.get::<_, String>(8)?,
        &row.get::<_, String>(9)?,
        &row.get::<_, String>(10)?,
    )
    .map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            7,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    })?;
    Ok(ToolVersionRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        tool_id: row.get(2)?,
        tool_status: status,
        version_number: row.get(4)?,
        input_contract,
        output_contract,
        effect,
        created_at: row.get(11)?,
    })
}
