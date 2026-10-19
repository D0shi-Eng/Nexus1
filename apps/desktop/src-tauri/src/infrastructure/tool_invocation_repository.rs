use rusqlite::{params, Connection, Transaction};

use crate::domain::{
    error::AppError,
    tool_invocation::{ToolInvocationRecord, ToolInvocationStatus},
};

pub struct ToolInvocationRepository;

impl ToolInvocationRepository {
    pub fn insert(
        transaction: &Transaction<'_>,
        record: &ToolInvocationRecord,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO tool_invocations (id, tenant_id, agent_run_id, agent_run_revision, tool_id, tool_version_id, adapter_key, input_json, input_hash, policy_decision, policy_reason, status, revision, failure_code, created_at, started_at, finished_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                params![
                    record.id,
                    record.tenant_id,
                    record.agent_run_id,
                    record.agent_run_revision,
                    record.tool_id,
                    record.tool_version_id,
                    record.adapter_key,
                    record.input_json,
                    record.input_hash,
                    record.policy_decision,
                    record.policy_reason,
                    record.status.as_code(),
                    record.revision,
                    record.failure_code,
                    record.created_at,
                    record.started_at,
                    record.finished_at,
                    record.updated_at
                ],
            )
            .map_err(map_insert_error)?;
        Ok(())
    }

    pub fn find(
        connection: &Connection,
        tenant_id: &str,
        invocation_id: &str,
    ) -> Result<Option<ToolInvocationRecord>, AppError> {
        let mut statement = connection
            .prepare(&format!("{} AND id = ?2", select_sql()))
            .map_err(|_| AppError::StorageOperation)?;
        let mut rows = statement
            .query_map(params![tenant_id, invocation_id], map_invocation)
            .map_err(|_| AppError::StorageOperation)?;
        rows.next().transpose().map_err(|_| AppError::DataAccess)
    }

    pub fn find_in_transaction(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        invocation_id: &str,
    ) -> Result<Option<ToolInvocationRecord>, AppError> {
        let mut statement = transaction
            .prepare(&format!("{} AND id = ?2", select_sql()))
            .map_err(|_| AppError::StorageOperation)?;
        let mut rows = statement
            .query_map(params![tenant_id, invocation_id], map_invocation)
            .map_err(|_| AppError::StorageOperation)?;
        rows.next().transpose().map_err(|_| AppError::DataAccess)
    }

    // البحث بالهوية الدقيقة (تشغيل + نسخة + هاش مدخل) — أساس منع الازدواج
    // وربط الموافقة الدقيقة (§42).
    pub fn find_exact(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        agent_run_id: &str,
        tool_version_id: &str,
        input_hash: &str,
    ) -> Result<Option<ToolInvocationRecord>, AppError> {
        let mut statement = transaction
            .prepare(&format!(
                "{} AND agent_run_id = ?2 AND tool_version_id = ?3 AND input_hash = ?4",
                select_sql()
            ))
            .map_err(|_| AppError::StorageOperation)?;
        let mut rows = statement
            .query_map(
                params![tenant_id, agent_run_id, tool_version_id, input_hash],
                map_invocation,
            )
            .map_err(|_| AppError::StorageOperation)?;
        rows.next().transpose().map_err(|_| AppError::DataAccess)
    }

    pub fn list_for_run(
        connection: &Connection,
        tenant_id: &str,
        agent_run_id: &str,
    ) -> Result<Vec<ToolInvocationRecord>, AppError> {
        let mut statement = connection
            .prepare(&format!(
                "{} AND agent_run_id = ?2 ORDER BY created_at ASC, id ASC",
                select_sql()
            ))
            .map_err(|_| AppError::StorageOperation)?;
        let rows = statement
            .query_map(params![tenant_id, agent_run_id], map_invocation)
            .map_err(|_| AppError::StorageOperation)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    // كل الاستدعاءات غير المنتهية لتشغيل — أساس تنظيف اليتامى عند الإنهاء/الإلغاء.
    pub fn list_non_terminal_for_run(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        agent_run_id: &str,
    ) -> Result<Vec<ToolInvocationRecord>, AppError> {
        let mut statement = transaction
            .prepare(&format!(
                "{} AND agent_run_id = ?2 AND status IN ('WAITING_APPROVAL', 'RUNNING') ORDER BY created_at ASC, id ASC",
                select_sql()
            ))
            .map_err(|_| AppError::StorageOperation)?;
        let rows = statement
            .query_map(params![tenant_id, agent_run_id], map_invocation)
            .map_err(|_| AppError::StorageOperation)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    // انتقال قيد التشغيل (CAS بالحالة والمراجعة) — الاستدعاء الداخلي بعد
    // إنشاء الصف بحالة WAITING_APPROVAL أو RUNNING حسب قرار السياسة.
    pub fn transition(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        invocation_id: &str,
        expected_status: ToolInvocationStatus,
        expected_revision: i64,
        next_status: ToolInvocationStatus,
        next_revision: i64,
        started_at: Option<&str>,
        finished_at: Option<&str>,
        failure_code: Option<&str>,
        updated_at: &str,
    ) -> Result<bool, AppError> {
        // حارس الدومين عند نقطة الـCAS المركزية: أي انتقال خارج المصفوفة
        // المسموحة يُرجع عدم التغيير (يفشل المستدعي بتعارض) لا أن يمر للـSQL.
        if !crate::domain::tool_invocation::is_valid_tool_invocation_transition(
            expected_status,
            next_status,
        ) {
            return Ok(false);
        }
        let changed = transaction
            .execute(
                "UPDATE tool_invocations
                 SET status = ?6, revision = ?7, started_at = COALESCE(?8, started_at), finished_at = ?9, failure_code = ?10, updated_at = ?11
                 WHERE tenant_id = ?1 AND id = ?2 AND status = ?3 AND revision = ?4 AND status <> ?5",
                params![
                    tenant_id,
                    invocation_id,
                    expected_status.as_code(),
                    expected_revision,
                    next_status.as_code(),
                    next_status.as_code(),
                    next_revision,
                    started_at,
                    finished_at,
                    failure_code,
                    updated_at
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(changed == 1)
    }
}

// عمودا الترشيح تضيفها كل دالة — القاعدة المشتركة تقتصر على الأعمدة وشرط
// المستأجر حتى لا تتضارب المعاملات المواضعية بين الاستعلامات.
fn select_sql() -> String {
    "SELECT id, tenant_id, agent_run_id, agent_run_revision, tool_id, tool_version_id, adapter_key, input_json, input_hash, policy_decision, policy_reason, status, revision, failure_code, created_at, started_at, finished_at, updated_at
     FROM tool_invocations
     WHERE tenant_id = ?1"
        .to_owned()
}

fn map_insert_error(error: rusqlite::Error) -> AppError {
    match &error {
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            AppError::Conflict
        }
        _ => AppError::StorageOperation,
    }
}

fn map_invocation(row: &rusqlite::Row<'_>) -> Result<ToolInvocationRecord, rusqlite::Error> {
    let status_code: String = row.get(11)?;
    Ok(ToolInvocationRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        agent_run_id: row.get(2)?,
        agent_run_revision: row.get(3)?,
        tool_id: row.get(4)?,
        tool_version_id: row.get(5)?,
        adapter_key: row.get(6)?,
        input_json: row.get(7)?,
        input_hash: row.get(8)?,
        policy_decision: row.get(9)?,
        policy_reason: row.get(10)?,
        status: ToolInvocationStatus::from_code(&status_code).map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                11,
                rusqlite::types::Type::Text,
                Box::new(std::fmt::Error),
            )
        })?,
        revision: row.get(12)?,
        failure_code: row.get(13)?,
        created_at: row.get(14)?,
        started_at: row.get(15)?,
        finished_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
}
