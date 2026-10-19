use rusqlite::{params, Connection};

use crate::domain::{
    error::AppError,
    models::{
        parse_data_mode, parse_risk_severity, parse_risk_status, RiskRecord, RiskSummary, RisksData,
    },
};

pub fn load_risks(
    connection: &Connection,
    tenant_id: &str,
    generated_at: &str,
) -> Result<RisksData, AppError> {
    let data_mode = connection
        .query_row(
            "SELECT data_mode FROM tenants WHERE id = ?1",
            [tenant_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| AppError::DataAccess)?;

    let summary = load_summary(connection, tenant_id)?;
    let items = load_items(connection, tenant_id)?;

    Ok(RisksData {
        has_data: !items.is_empty(),
        items,
        summary,
        data_mode: parse_data_mode(data_mode)?,
        generated_at: generated_at.to_owned(),
    })
}

fn load_summary(connection: &Connection, tenant_id: &str) -> Result<RiskSummary, AppError> {
    connection
        .query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN status = 'OPEN' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'MITIGATING' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'ACCEPTED' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'CLOSED' THEN 1 ELSE 0 END), 0)
             FROM risks
             WHERE tenant_id = ?1",
            [tenant_id],
            |row| {
                Ok(RiskSummary {
                    total: as_count(row.get(0)?),
                    open: as_count(row.get(1)?),
                    mitigating: as_count(row.get(2)?),
                    accepted: as_count(row.get(3)?),
                    closed: as_count(row.get(4)?),
                })
            },
        )
        .map_err(|_| AppError::DataAccess)
}

fn load_items(connection: &Connection, tenant_id: &str) -> Result<Vec<RiskRecord>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT id, title, severity, likelihood, impact, status, owner, created_at, updated_at
             FROM risks
             WHERE tenant_id = ?1
             ORDER BY
                CASE severity
                    WHEN 'CRITICAL' THEN 0
                    WHEN 'HIGH' THEN 1
                    WHEN 'MEDIUM' THEN 2
                    WHEN 'LOW' THEN 3
                    ELSE 4
                END ASC,
                updated_at DESC,
                id ASC",
        )
        .map_err(|_| AppError::DataAccess)?;

    let rows = statement
        .query_map(params![tenant_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })
        .map_err(|_| AppError::DataAccess)?;

    rows.map(|row| {
        let (id, title, severity, likelihood, impact, status, owner, created_at, updated_at) =
            row.map_err(|_| AppError::DataAccess)?;
        Ok(RiskRecord {
            id,
            title,
            severity: parse_risk_severity(severity)?,
            likelihood: u8::try_from(likelihood).map_err(|_| AppError::DataAccess)?,
            impact: u8::try_from(impact).map_err(|_| AppError::DataAccess)?,
            status: parse_risk_status(status)?,
            owner,
            created_at,
            updated_at,
        })
    })
    .collect::<Result<Vec<_>, AppError>>()
}

fn as_count(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
