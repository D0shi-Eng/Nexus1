use rusqlite::{params, Connection};

use crate::domain::{
    error::AppError,
    models::{
        OrganizationalIntelligenceData, OrganizationalIntelligenceDataMode,
        OrganizationalIntelligenceOperationalSummary, OrganizationalIntelligenceOrganization,
        OrganizationalIntelligencePriority, OrganizationalIntelligencePriorityKind,
        OrganizationalIntelligencePriorityReason, OrganizationalIntelligencePriorityState,
        OrganizationalIntelligenceSeverity, OrganizationalIntelligenceSourceSummary,
    },
};

pub fn load_organizational_intelligence(
    connection: &Connection,
    tenant_id: &str,
    now: &str,
) -> Result<OrganizationalIntelligenceData, AppError> {
    let data_mode = load_data_mode(connection, tenant_id)?;
    let organization = load_organization(connection, tenant_id)?;
    let sources = load_source_summary(connection, tenant_id)?;
    let operations = load_operational_summary(connection, tenant_id, now)?;
    let mut priorities = load_priorities(connection, tenant_id)?;
    priorities.sort_by(|left, right| priority_order(left, right, now));

    let has_data = organization.is_some()
        || sources.total > 0
        || operations.active_obligations > 0
        || operations.active_tasks > 0
        || operations.active_risks > 0
        || operations.pending_approvals > 0;

    Ok(OrganizationalIntelligenceData {
        organization,
        sources,
        operations,
        priorities,
        data_mode,
        generated_at: now.to_owned(),
        has_data,
    })
}

fn load_data_mode(
    connection: &Connection,
    tenant_id: &str,
) -> Result<OrganizationalIntelligenceDataMode, AppError> {
    let data_mode = connection
        .query_row(
            "SELECT data_mode FROM tenants WHERE id = ?1",
            [tenant_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| AppError::ResourceNotFound)?;

    parse_data_mode(&data_mode)
}

fn load_organization(
    connection: &Connection,
    tenant_id: &str,
) -> Result<Option<OrganizationalIntelligenceOrganization>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT legal_name, country_code, industry
             FROM organizations
             WHERE tenant_id = ?1
             ORDER BY legal_name ASC
             LIMIT 1",
        )
        .map_err(|_| AppError::DataAccess)?;

    let mut rows = statement
        .query([tenant_id])
        .map_err(|_| AppError::DataAccess)?;
    let Some(row) = rows.next().map_err(|_| AppError::DataAccess)? else {
        return Ok(None);
    };

    Ok(Some(OrganizationalIntelligenceOrganization {
        legal_name: row.get(0).map_err(|_| AppError::DataAccess)?,
        country_code: row.get(1).map_err(|_| AppError::DataAccess)?,
        industry: row.get(2).map_err(|_| AppError::DataAccess)?,
    }))
}

fn load_source_summary(
    connection: &Connection,
    tenant_id: &str,
) -> Result<OrganizationalIntelligenceSourceSummary, AppError> {
    connection
        .query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN verification_state = 'HUMAN_VERIFIED' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN verification_state = 'REQUIRES_REVIEW' THEN 1 ELSE 0 END), 0)
             FROM regulatory_sources
             WHERE tenant_id = ?1",
            [tenant_id],
            |row| {
                Ok(OrganizationalIntelligenceSourceSummary {
                    total: as_count(row.get(0)?),
                    human_verified: as_count(row.get(1)?),
                    requires_review: as_count(row.get(2)?),
                })
            },
        )
        .map_err(|_| AppError::DataAccess)
}

fn load_operational_summary(
    connection: &Connection,
    tenant_id: &str,
    now: &str,
) -> Result<OrganizationalIntelligenceOperationalSummary, AppError> {
    connection
        .query_row(
            "SELECT
                (SELECT COUNT(*) FROM obligations
                    WHERE tenant_id = ?1 AND archived_at IS NULL),
                (SELECT COUNT(*) FROM obligations
                    WHERE tenant_id = ?1 AND archived_at IS NULL AND status = 'AT_RISK'),
                (SELECT COUNT(*) FROM obligations
                    WHERE tenant_id = ?1 AND archived_at IS NULL AND deadline IS NOT NULL
                      AND deadline < ?2 AND status != 'COMPLETED'),
                (SELECT COUNT(*) FROM obligations
                    WHERE tenant_id = ?1 AND archived_at IS NULL
                      AND evidence_status IN ('MISSING', 'REQUIRES_REVIEW', 'EXPIRED', 'INVALID')),
                (SELECT COUNT(*) FROM tasks
                    WHERE tenant_id = ?1 AND status IN ('OPEN', 'IN_PROGRESS', 'BLOCKED')),
                (SELECT COUNT(*) FROM tasks
                    WHERE tenant_id = ?1 AND status = 'BLOCKED'),
                (SELECT COUNT(*) FROM risks
                    WHERE tenant_id = ?1 AND status IN ('OPEN', 'MITIGATING')),
                (SELECT COUNT(*) FROM approvals
                    WHERE tenant_id = ?1 AND decision = 'PENDING')",
            params![tenant_id, now],
            |row| {
                Ok(OrganizationalIntelligenceOperationalSummary {
                    active_obligations: as_count(row.get(0)?),
                    at_risk_obligations: as_count(row.get(1)?),
                    overdue_obligations: as_count(row.get(2)?),
                    evidence_gaps: as_count(row.get(3)?),
                    active_tasks: as_count(row.get(4)?),
                    blocked_tasks: as_count(row.get(5)?),
                    active_risks: as_count(row.get(6)?),
                    pending_approvals: as_count(row.get(7)?),
                })
            },
        )
        .map_err(|_| AppError::DataAccess)
}

fn load_priorities(
    connection: &Connection,
    tenant_id: &str,
) -> Result<Vec<OrganizationalIntelligencePriority>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT id, kind, title, severity, state, reason, owner, due_at, updated_at
             FROM (
                SELECT
                    id,
                    'OBLIGATION' AS kind,
                    title,
                    severity,
                    status AS state,
                    CASE WHEN status = 'AT_RISK' THEN 'OBLIGATION_AT_RISK' ELSE 'EVIDENCE_GAP' END AS reason,
                    owner,
                    deadline AS due_at,
                    updated_at
                FROM obligations
                WHERE tenant_id = ?1
                  AND archived_at IS NULL
                  AND (status = 'AT_RISK' OR evidence_status IN ('MISSING', 'REQUIRES_REVIEW', 'EXPIRED', 'INVALID'))

                UNION ALL

                SELECT
                    t.id,
                    'TASK' AS kind,
                    t.title,
                    t.priority AS severity,
                    t.status AS state,
                    'TASK_BLOCKED' AS reason,
                    u.display_name AS owner,
                    t.due_at,
                    t.updated_at
                FROM tasks t
                LEFT JOIN local_users u ON u.id = t.assignee_user_id AND u.tenant_id = t.tenant_id
                WHERE t.tenant_id = ?2 AND t.status = 'BLOCKED'

                UNION ALL

                SELECT
                    id,
                    'RISK' AS kind,
                    title,
                    severity,
                    status AS state,
                    'RISK_ACTIVE' AS reason,
                    owner,
                    NULL AS due_at,
                    updated_at
                FROM risks
                WHERE tenant_id = ?3 AND status IN ('OPEN', 'MITIGATING')
             )",
        )
        .map_err(|_| AppError::DataAccess)?;

    let rows = statement
        .query_map(params![tenant_id, tenant_id, tenant_id], |row| {
            let kind: String = row.get(1)?;
            let severity: String = row.get(3)?;
            let state: String = row.get(4)?;
            let reason: String = row.get(5)?;

            Ok((
                row.get::<_, String>(0)?,
                kind,
                row.get::<_, String>(2)?,
                severity,
                state,
                reason,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, String>(8)?,
            ))
        })
        .map_err(|_| AppError::DataAccess)?;

    rows.map(|row| {
        let (id, kind, title, severity, state, reason, owner, due_at, updated_at) =
            row.map_err(|_| AppError::DataAccess)?;
        Ok(OrganizationalIntelligencePriority {
            id,
            kind: parse_priority_kind(&kind)?,
            title,
            severity: parse_severity(&severity)?,
            state: parse_priority_state(&state)?,
            reason: parse_priority_reason(&reason)?,
            owner,
            due_at,
            updated_at,
        })
    })
    .collect()
}

fn priority_order(
    left: &OrganizationalIntelligencePriority,
    right: &OrganizationalIntelligencePriority,
    now: &str,
) -> std::cmp::Ordering {
    severity_rank(&left.severity)
        .cmp(&severity_rank(&right.severity))
        .then_with(|| is_overdue(right, now).cmp(&is_overdue(left, now)))
        .then_with(|| left.due_at.cmp(&right.due_at))
        .then_with(|| right.updated_at.cmp(&left.updated_at))
        .then_with(|| left.id.cmp(&right.id))
}

fn is_overdue(priority: &OrganizationalIntelligencePriority, now: &str) -> bool {
    priority
        .due_at
        .as_deref()
        .is_some_and(|due_at| due_at < now)
}

fn severity_rank(severity: &OrganizationalIntelligenceSeverity) -> u8 {
    match severity {
        OrganizationalIntelligenceSeverity::Critical => 0,
        OrganizationalIntelligenceSeverity::High => 1,
        OrganizationalIntelligenceSeverity::Medium => 2,
        OrganizationalIntelligenceSeverity::Low => 3,
    }
}

fn as_count(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn parse_data_mode(value: &str) -> Result<OrganizationalIntelligenceDataMode, AppError> {
    match value {
        "DEMO_SYNTHETIC" => Ok(OrganizationalIntelligenceDataMode::DemoSynthetic),
        "PRODUCTION" => Ok(OrganizationalIntelligenceDataMode::Production),
        _ => Err(AppError::DataAccess),
    }
}

fn parse_priority_kind(value: &str) -> Result<OrganizationalIntelligencePriorityKind, AppError> {
    match value {
        "OBLIGATION" => Ok(OrganizationalIntelligencePriorityKind::Obligation),
        "TASK" => Ok(OrganizationalIntelligencePriorityKind::Task),
        "RISK" => Ok(OrganizationalIntelligencePriorityKind::Risk),
        _ => Err(AppError::DataAccess),
    }
}

fn parse_severity(value: &str) -> Result<OrganizationalIntelligenceSeverity, AppError> {
    match value {
        "CRITICAL" => Ok(OrganizationalIntelligenceSeverity::Critical),
        "HIGH" => Ok(OrganizationalIntelligenceSeverity::High),
        "MEDIUM" => Ok(OrganizationalIntelligenceSeverity::Medium),
        "LOW" => Ok(OrganizationalIntelligenceSeverity::Low),
        _ => Err(AppError::DataAccess),
    }
}

fn parse_priority_state(value: &str) -> Result<OrganizationalIntelligencePriorityState, AppError> {
    match value {
        "AT_RISK" => Ok(OrganizationalIntelligencePriorityState::AtRisk),
        "MISSING" => Ok(OrganizationalIntelligencePriorityState::Missing),
        "REQUIRES_REVIEW" => Ok(OrganizationalIntelligencePriorityState::RequiresReview),
        "EXPIRED" => Ok(OrganizationalIntelligencePriorityState::Expired),
        "INVALID" => Ok(OrganizationalIntelligencePriorityState::Invalid),
        "BLOCKED" => Ok(OrganizationalIntelligencePriorityState::Blocked),
        "OPEN" => Ok(OrganizationalIntelligencePriorityState::Open),
        "MITIGATING" => Ok(OrganizationalIntelligencePriorityState::Mitigating),
        _ => Err(AppError::DataAccess),
    }
}

fn parse_priority_reason(
    value: &str,
) -> Result<OrganizationalIntelligencePriorityReason, AppError> {
    match value {
        "OBLIGATION_AT_RISK" => Ok(OrganizationalIntelligencePriorityReason::ObligationAtRisk),
        "EVIDENCE_GAP" => Ok(OrganizationalIntelligencePriorityReason::EvidenceGap),
        "TASK_BLOCKED" => Ok(OrganizationalIntelligencePriorityReason::TaskBlocked),
        "RISK_ACTIVE" => Ok(OrganizationalIntelligencePriorityReason::RiskActive),
        _ => Err(AppError::DataAccess),
    }
}
