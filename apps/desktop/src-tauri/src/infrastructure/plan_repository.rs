use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::domain::{
    error::AppError,
    plan::{
        AgentDelegationRecord, DelegationStatus, PlanRecord, PlanStatus, PlanStepRecord,
        PlanStepStatus,
    },
};

pub struct PlanRepository;

pub struct PlanInsert<'a> {
    pub id: &'a str,
    pub goal: &'a str,
    pub steps_hash: &'a str,
    pub model_id_at_request: Option<&'a str>,
    pub requested_by_user_id: &'a str,
}

pub struct PlanStepInsert<'a> {
    pub id: &'a str,
    pub plan_id: &'a str,
    pub step_index: usize,
    pub objective: &'a str,
    pub required_capability: &'a str,
    pub requires_knowledge: bool,
    pub evidence_requirement: &'a str,
}

pub struct DelegationInsert<'a> {
    pub id: &'a str,
    pub plan_id: &'a str,
    pub plan_revision: i64,
    pub step_id: &'a str,
    pub target_agent_id: &'a str,
    pub agent_run_id: Option<&'a str>,
    pub objective: &'a str,
    pub allowed_context_refs_json: &'a str,
    pub allowed_skill_version_id: Option<&'a str>,
}

impl PlanRepository {
    pub fn insert_plan(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        insert: &PlanInsert<'_>,
        now: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO plans (
                    id, tenant_id, goal, status, steps_hash, model_id_at_request,
                    revision, requested_by_user_id, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, 'PENDING_APPROVAL', ?4, ?5, 1, ?6, ?7, ?7)",
                params![
                    insert.id,
                    tenant_id,
                    insert.goal,
                    insert.steps_hash,
                    insert.model_id_at_request,
                    insert.requested_by_user_id,
                    now,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn insert_step(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        insert: &PlanStepInsert<'_>,
        now: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO plan_steps (
                    id, tenant_id, plan_id, step_index, objective, required_capability,
                    requires_knowledge, evidence_requirement, status, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'PENDING', ?9, ?9)",
                params![
                    insert.id,
                    tenant_id,
                    insert.plan_id,
                    insert.step_index as i64,
                    insert.objective,
                    insert.required_capability,
                    insert.requires_knowledge,
                    insert.evidence_requirement,
                    now,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn find_plan(
        connection: &Connection,
        tenant_id: &str,
        plan_id: &str,
    ) -> Result<Option<PlanRecord>, AppError> {
        connection
            .query_row(
                &select_plan_sql("id = ?2"),
                params![tenant_id, plan_id],
                map_plan,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find_plan_in_transaction(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        plan_id: &str,
    ) -> Result<Option<PlanRecord>, AppError> {
        transaction
            .query_row(
                &select_plan_sql("id = ?2"),
                params![tenant_id, plan_id],
                map_plan,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn list_plans(
        connection: &Connection,
        tenant_id: &str,
        limit: i64,
    ) -> Result<Vec<PlanRecord>, AppError> {
        let mut statement = connection
            .prepare(&format!(
                "{} ORDER BY created_at DESC, id ASC LIMIT ?2",
                select_plan_sql("tenant_id = ?1")
            ))
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id, limit], map_plan)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn list_steps(
        connection: &Connection,
        tenant_id: &str,
        plan_id: &str,
    ) -> Result<Vec<PlanStepRecord>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT id, tenant_id, plan_id, step_index, objective, required_capability,
                        requires_knowledge, evidence_requirement, status, created_at, updated_at
                 FROM plan_steps
                 WHERE tenant_id = ?1 AND plan_id = ?2
                 ORDER BY step_index ASC",
            )
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id, plan_id], map_step)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    // انتقال حالة الخطة بشرط CAS كامل (الحالة والمراجعة المتوقعتان)
    pub fn transition_plan(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        plan_id: &str,
        from_status: PlanStatus,
        from_revision: i64,
        to_status: PlanStatus,
        to_revision: i64,
        completed_at: Option<&str>,
        updated_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE plans
                 SET status = ?1, revision = ?2, completed_at = COALESCE(?3, completed_at),
                     updated_at = ?4
                 WHERE id = ?5 AND tenant_id = ?6 AND status = ?7 AND revision = ?8",
                params![
                    to_status.as_code(),
                    to_revision,
                    completed_at,
                    updated_at,
                    plan_id,
                    tenant_id,
                    from_status.as_code(),
                    from_revision,
                ],
            )
            .map_err(|_| AppError::StorageOperation)
            .map(|changed| changed == 1)
    }

    pub fn transition_step(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        step_id: &str,
        from_status: PlanStepStatus,
        to_status: PlanStepStatus,
        updated_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE plan_steps SET status = ?1, updated_at = ?2
                 WHERE id = ?3 AND tenant_id = ?4 AND status = ?5",
                params![
                    to_status.as_code(),
                    updated_at,
                    step_id,
                    tenant_id,
                    from_status.as_code()
                ],
            )
            .map_err(|_| AppError::StorageOperation)
            .map(|changed| changed == 1)
    }

    pub fn record_synthesis(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        plan_id: &str,
        from_status: PlanStatus,
        from_revision: i64,
        to_status: PlanStatus,
        to_revision: i64,
        synthesis_output: &str,
        conflict_flag: bool,
        updated_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE plans
                 SET status = ?1, revision = ?2, synthesis_output = ?3, conflict_flag = ?4,
                     completed_at = ?5, updated_at = ?5
                 WHERE id = ?6 AND tenant_id = ?7 AND status = ?8 AND revision = ?9",
                params![
                    to_status.as_code(),
                    to_revision,
                    synthesis_output,
                    conflict_flag,
                    updated_at,
                    plan_id,
                    tenant_id,
                    from_status.as_code(),
                    from_revision,
                ],
            )
            .map_err(|_| AppError::StorageOperation)
            .map(|changed| changed == 1)
    }

    // ===== التفويضات =====

    pub fn insert_delegation(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        insert: &DelegationInsert<'_>,
        now: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO agent_delegations (
                    id, tenant_id, plan_id, plan_revision, step_id, source_kind,
                    target_agent_id, agent_run_id, objective, allowed_context_refs_json,
                    allowed_skill_version_id, status, revision, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'SUPERVISOR', ?6, ?7, ?8, ?9, ?10, 'PENDING', 1, ?11, ?11)",
                params![
                    insert.id,
                    tenant_id,
                    insert.plan_id,
                    insert.plan_revision,
                    insert.step_id,
                    insert.target_agent_id,
                    insert.agent_run_id,
                    insert.objective,
                    insert.allowed_context_refs_json,
                    insert.allowed_skill_version_id,
                    now,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn find_delegation(
        connection: &Connection,
        tenant_id: &str,
        delegation_id: &str,
    ) -> Result<Option<AgentDelegationRecord>, AppError> {
        connection
            .query_row(
                &select_delegation_sql("id = ?2"),
                params![tenant_id, delegation_id],
                map_delegation,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn list_delegations_for_plan(
        connection: &Connection,
        tenant_id: &str,
        plan_id: &str,
    ) -> Result<Vec<AgentDelegationRecord>, AppError> {
        let mut statement = connection
            .prepare(&format!(
                "{} ORDER BY created_at ASC, id ASC",
                select_delegation_sql("plan_id = ?2")
            ))
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id, plan_id], map_delegation)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn count_delegations_for_plan_on_connection(
        connection: &Connection,
        tenant_id: &str,
        plan_id: &str,
    ) -> Result<i64, AppError> {
        connection
            .query_row(
                "SELECT COUNT(*) FROM agent_delegations WHERE tenant_id = ?1 AND plan_id = ?2",
                params![tenant_id, plan_id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find_delegation_in_transaction(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        delegation_id: &str,
    ) -> Result<Option<AgentDelegationRecord>, AppError> {
        transaction
            .query_row(
                &select_delegation_sql("id = ?2"),
                params![tenant_id, delegation_id],
                map_delegation,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    // CAS للتفويض: الحالة والمراجعة المتوقعتان معاً — لا نتيجة متأخرة بعد نهائي (§112)
    pub fn transition_delegation(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        delegation_id: &str,
        from_status: DelegationStatus,
        from_revision: i64,
        to_status: DelegationStatus,
        to_revision: i64,
        output: Option<&str>,
        citations_json: Option<&str>,
        truth_class: Option<&str>,
        failure_code: Option<&str>,
        agent_run_id: Option<&str>,
        updated_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE agent_delegations
                 SET status = ?1, revision = ?2, output = ?3, citations_json = ?4,
                     truth_class = ?5, failure_code = ?6, agent_run_id = COALESCE(?7, agent_run_id),
                     finished_at = CASE WHEN ?8 IN ('COMPLETED', 'FAILED', 'CANCELLED') THEN ?9 ELSE finished_at END,
                     updated_at = ?9
                 WHERE id = ?10 AND tenant_id = ?11 AND status = ?12 AND revision = ?13",
                params![
                    to_status.as_code(),
                    to_revision,
                    output,
                    citations_json,
                    truth_class,
                    failure_code,
                    agent_run_id,
                    to_status.as_code(),
                    updated_at,
                    delegation_id,
                    tenant_id,
                    from_status.as_code(),
                    from_revision,
                ],
            )
            .map_err(|_| AppError::StorageOperation)
            .map(|changed| changed == 1)
    }

    // استرداد الانهيار: كل التفويضات/الخطط العالقة RUNNING أو الخطة النشطة
    // المعلقة تُنهى حتمياً (T11/C-11) — قائمة هنا ثم تُنهيها الخدمة.
    pub fn list_stale_running_plans(
        connection: &Connection,
        tenant_id: &str,
    ) -> Result<Vec<PlanRecord>, AppError> {
        let mut statement = connection
            .prepare(&format!(
                "{} AND status IN ('ACTIVE', 'RUNNING')",
                select_plan_sql("tenant_id = ?1")
            ))
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id], map_plan)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }
}

fn select_plan_sql(filter: &str) -> String {
    format!(
        "SELECT id, tenant_id, goal, status, steps_hash, model_id_at_request, revision,
                requested_by_user_id, synthesis_output, conflict_flag, created_at, updated_at,
                completed_at
         FROM plans
         WHERE tenant_id = ?1 AND {filter}"
    )
}

fn map_plan(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlanRecord> {
    let status: String = row.get(3)?;
    let parse_error = |message: String| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                message,
            )),
        )
    };
    let status =
        PlanStatus::from_code(status.as_str()).map_err(|error| parse_error(error.to_string()))?;
    Ok(PlanRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        goal: row.get(2)?,
        status,
        steps_hash: row.get(4)?,
        model_id_at_request: row.get(5)?,
        revision: row.get(6)?,
        requested_by_user_id: row.get(7)?,
        synthesis_output: row.get(8)?,
        conflict_flag: row.get::<_, i64>(9)? == 1,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        completed_at: row.get(12)?,
    })
}

fn map_step(row: &rusqlite::Row<'_>) -> rusqlite::Result<PlanStepRecord> {
    let status: String = row.get(8)?;
    let parse_error = |message: String| {
        rusqlite::Error::FromSqlConversionFailure(
            8,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                message,
            )),
        )
    };
    let status = PlanStepStatus::from_code(status.as_str())
        .map_err(|error| parse_error(error.to_string()))?;
    Ok(PlanStepRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        plan_id: row.get(2)?,
        step_index: row.get::<_, i64>(3)? as usize,
        objective: row.get(4)?,
        required_capability: row.get(5)?,
        requires_knowledge: row.get::<_, i64>(6)? == 1,
        evidence_requirement: row.get(7)?,
        status,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn select_delegation_sql(filter: &str) -> String {
    format!(
        "SELECT id, tenant_id, plan_id, plan_revision, step_id, target_agent_id, agent_run_id,
                objective, allowed_skill_version_id, status, revision, output, citations_json,
                truth_class, failure_code, created_at, updated_at, finished_at
         FROM agent_delegations
         WHERE tenant_id = ?1 AND {filter}"
    )
}

fn map_delegation(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentDelegationRecord> {
    let status: String = row.get(9)?;
    let parse_error = |message: String| {
        rusqlite::Error::FromSqlConversionFailure(
            9,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                message,
            )),
        )
    };
    let status = DelegationStatus::from_code(status.as_str())
        .map_err(|error| parse_error(error.to_string()))?;
    Ok(AgentDelegationRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        plan_id: row.get(2)?,
        plan_revision: row.get(3)?,
        step_id: row.get(4)?,
        target_agent_id: row.get(5)?,
        agent_run_id: row.get(6)?,
        objective: row.get(7)?,
        allowed_skill_version_id: row.get(8)?,
        status,
        revision: row.get(10)?,
        output: row.get(11)?,
        citations_json: row.get(12)?,
        truth_class: row.get(13)?,
        failure_code: row.get(14)?,
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
        finished_at: row.get(17)?,
    })
}
