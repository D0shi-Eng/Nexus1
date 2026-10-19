use rusqlite::{params, Connection, Transaction};
use serde::Serialize;

use crate::domain::error::AppError;

// ربط هوية التنفيذ الثابت — صف واحد لكل تشغيل وغير قابل للتعديل إطلاقاً
// (مشغلات SQL تمنع التحديث والحذف)؛ يبقى عبر إعادة تشغيل التطبيق (§20).
//
// 0024: الربط لم يعد معرّف النموذج وحده. الحقول الأربعة الجديدة تُثبّت **وجهة**
// التنفيذ (الموفر وعائلته وبصمة عنوانه) وبصمة الربط الكاملة، فيُكشف تبديل
// الوجهة بعد الاعتماد ويُرفض التنفيذ (StaleApproval). الصفوف التاريخية تحمل
// `None` في هذه الحقول، وهذا يعني «هوية وجهة غير مثبتة» ⇒ فشل مغلق لا توافق.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunExecutionBindingRecord {
    pub agent_run_id: String,
    pub tenant_id: String,
    pub agent_run_revision: i64,
    pub agent_id: String,
    pub execution_profile_version_id: String,
    pub task_content: String,
    pub task_content_hash: String,
    pub model_id_at_request: Option<String>,
    pub provider_id_at_request: Option<String>,
    pub provider_kind_at_request: Option<String>,
    pub endpoint_identity_at_request: Option<String>,
    pub approval_binding_hash: Option<String>,
    pub bound_at: String,
}

pub struct AgentRunExecutionBindingRepository;

const BINDING_COLUMNS: &str = "agent_run_id, tenant_id, agent_run_revision, agent_id, \
     execution_profile_version_id, task_content, task_content_hash, model_id_at_request, \
     provider_id_at_request, provider_kind_at_request, endpoint_identity_at_request, \
     approval_binding_hash, bound_at";

impl AgentRunExecutionBindingRepository {
    pub fn insert(
        transaction: &Transaction<'_>,
        record: &AgentRunExecutionBindingRecord,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO agent_run_execution_bindings (agent_run_id, tenant_id, agent_run_revision, agent_id, execution_profile_version_id, task_content, task_content_hash, model_id_at_request, provider_id_at_request, provider_kind_at_request, endpoint_identity_at_request, approval_binding_hash, bound_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    record.agent_run_id,
                    record.tenant_id,
                    record.agent_run_revision,
                    record.agent_id,
                    record.execution_profile_version_id,
                    record.task_content,
                    record.task_content_hash,
                    record.model_id_at_request,
                    record.provider_id_at_request,
                    record.provider_kind_at_request,
                    record.endpoint_identity_at_request,
                    record.approval_binding_hash,
                    record.bound_at
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn find(
        connection: &Connection,
        tenant_id: &str,
        agent_run_id: &str,
    ) -> Result<Option<AgentRunExecutionBindingRecord>, AppError> {
        let mut statement = connection
            .prepare(&format!(
                "SELECT {BINDING_COLUMNS}
                 FROM agent_run_execution_bindings
                 WHERE tenant_id = ?1 AND agent_run_id = ?2"
            ))
            .map_err(|_| AppError::StorageOperation)?;
        let mut rows = statement
            .query_map(params![tenant_id, agent_run_id], map_binding)
            .map_err(|_| AppError::StorageOperation)?;
        rows.next().transpose().map_err(|_| AppError::DataAccess)
    }

    pub fn find_in_transaction(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        agent_run_id: &str,
    ) -> Result<Option<AgentRunExecutionBindingRecord>, AppError> {
        let mut statement = transaction
            .prepare(&format!(
                "SELECT {BINDING_COLUMNS}
                 FROM agent_run_execution_bindings
                 WHERE tenant_id = ?1 AND agent_run_id = ?2"
            ))
            .map_err(|_| AppError::StorageOperation)?;
        let mut rows = statement
            .query_map(params![tenant_id, agent_run_id], map_binding)
            .map_err(|_| AppError::StorageOperation)?;
        rows.next().transpose().map_err(|_| AppError::DataAccess)
    }
}

fn map_binding(row: &rusqlite::Row<'_>) -> Result<AgentRunExecutionBindingRecord, rusqlite::Error> {
    Ok(AgentRunExecutionBindingRecord {
        agent_run_id: row.get(0)?,
        tenant_id: row.get(1)?,
        agent_run_revision: row.get(2)?,
        agent_id: row.get(3)?,
        execution_profile_version_id: row.get(4)?,
        task_content: row.get(5)?,
        task_content_hash: row.get(6)?,
        model_id_at_request: row.get(7)?,
        provider_id_at_request: row.get(8)?,
        provider_kind_at_request: row.get(9)?,
        endpoint_identity_at_request: row.get(10)?,
        approval_binding_hash: row.get(11)?,
        bound_at: row.get(12)?,
    })
}
