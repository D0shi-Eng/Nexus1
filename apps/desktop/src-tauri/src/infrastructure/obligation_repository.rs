use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::{
    error::AppError,
    models::{
        parse_compliance_status, parse_data_mode, parse_obligation_priority,
        parse_obligation_status, parse_review_status, parse_source_type, ComplianceStatus,
        DataMode, ObligationApprovalReference, ObligationDetails, ObligationEvidenceReference,
        ObligationListItem, ObligationPriority, ObligationRelatedResources,
        ObligationRiskReference, ObligationStatus, ObligationTaskReference, ReviewStatus,
        SourceReference, SourceType,
    },
};

pub struct ObligationRepository;

pub struct ObligationUpdateValues<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub owner: &'a str,
    pub due_date: Option<&'a str>,
    pub priority: &'a str,
    pub status: &'a str,
    pub compliance_status: &'a str,
    pub review_status: &'a str,
    pub expected_version: i64,
    pub updated_at: &'a str,
}

impl ObligationRepository {
    pub fn list_active(
        connection: &Connection,
        tenant_id: &str,
    ) -> Result<Vec<ObligationListItem>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT o.id, o.tenant_id, o.title, o.description, o.status, o.priority, o.source_id,
                        rs.authority, rs.document_title, rs.source_url, rs.document_version, rs.retrieved_at, rs.official,
                        o.source_type, o.owner, o.deadline, o.created_at, o.updated_at, o.review_status,
                        o.compliance_status, t.data_mode, o.archived_at
                 FROM obligations o
                 INNER JOIN regulatory_sources rs ON rs.id = o.source_id AND rs.tenant_id = o.tenant_id
                 INNER JOIN tenants t ON t.id = o.tenant_id
                 WHERE o.tenant_id = ?1 AND o.archived_at IS NULL
                 ORDER BY CASE o.priority WHEN 'CRITICAL' THEN 0 WHEN 'HIGH' THEN 1 WHEN 'MEDIUM' THEN 2 ELSE 3 END,
                          CASE WHEN o.deadline IS NULL THEN 1 ELSE 0 END,
                          o.deadline ASC,
                          o.updated_at DESC",
            )
            .map_err(|_| AppError::DataAccess)?;

        let rows = statement
            .query_map(params![tenant_id], map_list_item)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn get_active_details(
        connection: &Connection,
        tenant_id: &str,
        obligation_id: &str,
    ) -> Result<Option<ObligationDetails>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT o.id, o.tenant_id, o.title, o.description, o.status, o.priority, o.source_id,
                        rs.authority, rs.document_title, rs.source_url, rs.document_version, rs.retrieved_at, rs.official,
                        o.source_type, o.owner, o.deadline, o.created_at, o.updated_at, o.review_status,
                        o.compliance_status, t.data_mode, o.archived_at, o.version
                 FROM obligations o
                 INNER JOIN regulatory_sources rs ON rs.id = o.source_id AND rs.tenant_id = o.tenant_id
                 INNER JOIN tenants t ON t.id = o.tenant_id
                 WHERE o.id = ?1 AND o.tenant_id = ?2 AND o.archived_at IS NULL",
            )
            .map_err(|_| AppError::DataAccess)?;

        let found = statement
            .query_row(params![obligation_id, tenant_id], |row| {
                let item = map_list_item(row)?;
                let version: i64 = row.get(22)?;
                Ok((item, version))
            })
            .optional()
            .map_err(|_| AppError::DataAccess)?;

        let Some((obligation, version)) = found else {
            return Ok(None);
        };

        let related_resources = Self::related_resources(connection, tenant_id, obligation_id)?;
        Ok(Some(ObligationDetails {
            obligation,
            related_resources,
            version,
        }))
    }

    pub fn update_active(
        connection: &Connection,
        tenant_id: &str,
        obligation_id: &str,
        values: &ObligationUpdateValues<'_>,
    ) -> Result<Option<i64>, AppError> {
        let changed = connection
            .execute(
                "UPDATE obligations
                 SET title = ?1,
                     description = ?2,
                     owner = ?3,
                     deadline = ?4,
                     priority = ?5,
                     status = ?6,
                     compliance_status = ?7,
                     review_status = ?8,
                     updated_at = ?9,
                     version = version + 1
                 WHERE id = ?10
                   AND tenant_id = ?11
                   AND archived_at IS NULL
                   AND version = ?12",
                params![
                    values.title,
                    values.description,
                    values.owner,
                    values.due_date,
                    values.priority,
                    values.status,
                    values.compliance_status,
                    values.review_status,
                    values.updated_at,
                    obligation_id,
                    tenant_id,
                    values.expected_version,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;

        if changed == 0 {
            return Ok(None);
        }

        connection
            .query_row(
                "SELECT version FROM obligations WHERE id = ?1 AND tenant_id = ?2",
                params![obligation_id, tenant_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn archive_active(
        connection: &Connection,
        tenant_id: &str,
        obligation_id: &str,
        actor_id: &str,
        expected_version: i64,
        archived_at: &str,
    ) -> Result<bool, AppError> {
        connection
            .execute(
                "UPDATE obligations
                 SET archived_at = ?1,
                     archived_by = ?2,
                     updated_at = ?1,
                     version = version + 1
                 WHERE id = ?3
                   AND tenant_id = ?4
                   AND archived_at IS NULL
                   AND version = ?5",
                params![
                    archived_at,
                    actor_id,
                    obligation_id,
                    tenant_id,
                    expected_version
                ],
            )
            .map_err(|_| AppError::StorageOperation)
            .map(|changed| changed == 1)
    }

    pub fn active_version(
        connection: &Connection,
        tenant_id: &str,
        obligation_id: &str,
    ) -> Result<Option<i64>, AppError> {
        connection
            .query_row(
                "SELECT version FROM obligations WHERE id = ?1 AND tenant_id = ?2 AND archived_at IS NULL",
                params![obligation_id, tenant_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn related_resources(
        connection: &Connection,
        tenant_id: &str,
        obligation_id: &str,
    ) -> Result<ObligationRelatedResources, AppError> {
        Ok(ObligationRelatedResources {
            evidence: load_evidence(connection, tenant_id, obligation_id)?,
            tasks: load_tasks(connection, tenant_id, obligation_id)?,
            risks: load_risks(connection, tenant_id, obligation_id)?,
            approvals: load_approvals(connection, tenant_id, obligation_id)?,
        })
    }
}

fn map_list_item(row: &rusqlite::Row<'_>) -> rusqlite::Result<ObligationListItem> {
    let status: String = row.get(4)?;
    let priority: String = row.get(5)?;
    let source_type: String = row.get(13)?;
    let review_status: String = row.get(18)?;
    let compliance_status: String = row.get(19)?;
    let data_mode: String = row.get(20)?;
    let source_url: Option<String> = row.get(9)?;
    let document_title: String = row.get(8)?;
    let authority: String = row.get(7)?;

    let parse = |error: AppError| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                error.to_string(),
            )),
        )
    };

    Ok(ObligationListItem {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        title: row.get(2)?,
        description: row.get(3)?,
        status: parse_obligation_status(status).map_err(parse)?,
        priority: parse_obligation_priority(priority).map_err(parse)?,
        source_id: row.get(6)?,
        source_reference: source_url
            .clone()
            .unwrap_or_else(|| format!("{authority} · {document_title}")),
        source_type: parse_source_type(source_type).map_err(parse)?,
        source: SourceReference {
            authority,
            document_title,
            source_url,
            version: row.get(10)?,
            section: None,
            retrieved_at: row.get(11)?,
            official: row.get(12)?,
        },
        owner: row.get(14)?,
        due_date: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        review_status: parse_review_status(review_status).map_err(parse)?,
        compliance_status: parse_compliance_status(compliance_status).map_err(parse)?,
        data_mode: parse_data_mode(data_mode).map_err(parse)?,
        is_archived: row.get::<_, Option<String>>(21)?.is_some(),
    })
}

fn load_evidence(
    connection: &Connection,
    tenant_id: &str,
    obligation_id: &str,
) -> Result<Vec<ObligationEvidenceReference>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT id, label, classification, status, expires_at, created_at
             FROM evidence_items
             WHERE tenant_id = ?1 AND obligation_id = ?2
             ORDER BY created_at DESC",
        )
        .map_err(|_| AppError::DataAccess)?;
    let rows = statement
        .query_map(params![tenant_id, obligation_id], |row| {
            Ok(ObligationEvidenceReference {
                id: row.get(0)?,
                label: row.get(1)?,
                classification: row.get(2)?,
                status: row.get(3)?,
                expires_at: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|_| AppError::DataAccess)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::DataAccess)
}

fn load_tasks(
    connection: &Connection,
    tenant_id: &str,
    obligation_id: &str,
) -> Result<Vec<ObligationTaskReference>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT id, title, status, priority, due_at, created_at
             FROM tasks
             WHERE tenant_id = ?1 AND obligation_id = ?2
             ORDER BY created_at DESC",
        )
        .map_err(|_| AppError::DataAccess)?;
    let rows = statement
        .query_map(params![tenant_id, obligation_id], |row| {
            Ok(ObligationTaskReference {
                id: row.get(0)?,
                title: row.get(1)?,
                status: row.get(2)?,
                priority: row.get(3)?,
                due_at: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|_| AppError::DataAccess)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::DataAccess)
}

fn load_risks(
    connection: &Connection,
    tenant_id: &str,
    obligation_id: &str,
) -> Result<Vec<ObligationRiskReference>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT id, title, status, severity, owner, created_at
             FROM risks
             WHERE tenant_id = ?1 AND obligation_id = ?2
             ORDER BY created_at DESC",
        )
        .map_err(|_| AppError::DataAccess)?;
    let rows = statement
        .query_map(params![tenant_id, obligation_id], |row| {
            Ok(ObligationRiskReference {
                id: row.get(0)?,
                title: row.get(1)?,
                status: row.get(2)?,
                severity: row.get(3)?,
                owner: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|_| AppError::DataAccess)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::DataAccess)
}

fn load_approvals(
    connection: &Connection,
    tenant_id: &str,
    obligation_id: &str,
) -> Result<Vec<ObligationApprovalReference>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT a.id, a.resource_type, a.decision, a.required_role_code, a.created_at, a.decided_at
             FROM approvals a
             WHERE a.tenant_id = ?1
               AND a.resource_type = 'TASK'
               AND a.resource_id IN (
                 SELECT id FROM tasks WHERE tenant_id = ?1 AND obligation_id = ?2
               )
             ORDER BY a.created_at DESC",
        )
        .map_err(|_| AppError::DataAccess)?;
    let rows = statement
        .query_map(params![tenant_id, obligation_id], |row| {
            Ok(ObligationApprovalReference {
                id: row.get(0)?,
                resource_type: row.get(1)?,
                decision: row.get(2)?,
                required_role_code: row.get(3)?,
                created_at: row.get(4)?,
                decided_at: row.get(5)?,
            })
        })
        .map_err(|_| AppError::DataAccess)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::DataAccess)
}

#[allow(dead_code)]
fn _typed_values_marker(
    _status: ObligationStatus,
    _priority: ObligationPriority,
    _compliance: ComplianceStatus,
    _review: ReviewStatus,
    _source_type: SourceType,
    _data_mode: DataMode,
) {
}
