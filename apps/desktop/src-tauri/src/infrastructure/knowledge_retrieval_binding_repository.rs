use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::domain::error::AppError;

// ربط استرجاع المعرفة بهوية التشغيل الدقيقة (C-4/§77) — غير قابل للتغيير
// بعد الطلب، والموافقة على التشغيل تغطي هويته (استعلام + قطع) بالضبط.
pub struct KnowledgeRetrievalBindingRepository;

pub struct KnowledgeRetrievalBindingRecord {
    pub agent_run_id: String,
    pub tenant_id: String,
    pub agent_run_revision: i64,
    pub query: String,
    pub query_hash: String,
    pub chunk_ids_json: String,
    pub bound_at: String,
}

impl KnowledgeRetrievalBindingRepository {
    pub fn insert(
        transaction: &Transaction<'_>,
        record: &KnowledgeRetrievalBindingRecord,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO knowledge_retrieval_bindings (
                    agent_run_id, tenant_id, agent_run_revision, query, query_hash,
                    chunk_ids_json, bound_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    record.agent_run_id,
                    record.tenant_id,
                    record.agent_run_revision,
                    record.query,
                    record.query_hash,
                    record.chunk_ids_json,
                    record.bound_at,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn find(
        connection: &Connection,
        tenant_id: &str,
        agent_run_id: &str,
    ) -> Result<Option<KnowledgeRetrievalBindingRecord>, AppError> {
        connection
            .query_row(
                "SELECT agent_run_id, tenant_id, agent_run_revision, query, query_hash,
                        chunk_ids_json, bound_at
                 FROM knowledge_retrieval_bindings
                 WHERE tenant_id = ?1 AND agent_run_id = ?2",
                params![tenant_id, agent_run_id],
                map_binding,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find_in_transaction(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        agent_run_id: &str,
    ) -> Result<Option<KnowledgeRetrievalBindingRecord>, AppError> {
        transaction
            .query_row(
                "SELECT agent_run_id, tenant_id, agent_run_revision, query, query_hash,
                        chunk_ids_json, bound_at
                 FROM knowledge_retrieval_bindings
                 WHERE tenant_id = ?1 AND agent_run_id = ?2",
                params![tenant_id, agent_run_id],
                map_binding,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }
}

fn map_binding(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeRetrievalBindingRecord> {
    Ok(KnowledgeRetrievalBindingRecord {
        agent_run_id: row.get(0)?,
        tenant_id: row.get(1)?,
        agent_run_revision: row.get(2)?,
        query: row.get(3)?,
        query_hash: row.get(4)?,
        chunk_ids_json: row.get(5)?,
        bound_at: row.get(6)?,
    })
}
