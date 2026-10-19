use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::domain::{
    error::AppError,
    memory::{MemoryItemStatus, MemoryType},
};

pub struct MemoryRepository;

pub struct MemoryInsert<'a> {
    pub id: &'a str,
    pub memory_type: MemoryType,
    pub scope_resource_type: &'a str,
    pub scope_resource_id: Option<&'a str>,
    pub agent_id: Option<&'a str>,
    pub content: &'a str,
    pub truth_class: &'a str,
    pub provenance_kind: &'a str,
    pub provenance_ref: Option<&'a str>,
    pub initial_status: MemoryItemStatus,
    pub created_by: &'a str,
    pub expires_at: Option<&'a str>,
}

impl MemoryRepository {
    pub fn insert(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        insert: &MemoryInsert<'_>,
        now: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO memory_items (
                    id, tenant_id, memory_type, scope_resource_type, scope_resource_id, agent_id,
                    content, truth_class, provenance_kind, provenance_ref, status, created_by,
                    expires_at, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)",
                params![
                    insert.id,
                    tenant_id,
                    insert.memory_type.as_code(),
                    insert.scope_resource_type,
                    insert.scope_resource_id,
                    insert.agent_id,
                    insert.content,
                    insert.truth_class,
                    insert.provenance_kind,
                    insert.provenance_ref,
                    insert.initial_status.as_code(),
                    insert.created_by,
                    insert.expires_at,
                    now,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn set_status(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        memory_id: &str,
        status: MemoryItemStatus,
        updated_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE memory_items SET status = ?1, updated_at = ?2
                 WHERE id = ?3 AND tenant_id = ?4",
                params![status.as_code(), updated_at, memory_id, tenant_id],
            )
            .map_err(|_| AppError::StorageOperation)
            .map(|changed| changed == 1)
    }

    // الترقية البشرية للذاكرة الدلالية/التنظيمية: CLAIM → VERIFIED + ACTIVE —
    // المسار الوحيد الذي يضبط VERIFIED (بوابة الدور تُطبق في الخدمة C-7).
    pub fn promote_semantic(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        memory_id: &str,
        updated_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE memory_items
                 SET truth_class = 'VERIFIED', status = 'ACTIVE', updated_at = ?1
                 WHERE id = ?2 AND tenant_id = ?3
                   AND memory_type IN ('SEMANTIC', 'ORGANIZATIONAL')
                   AND status = 'PROPOSED'
                   AND truth_class IN ('CLAIM', 'AI_INTERPRETATION')",
                params![updated_at, memory_id, tenant_id],
            )
            .map_err(|_| AppError::StorageOperation)
            .map(|changed| changed == 1)
    }

    pub fn find(
        connection: &Connection,
        tenant_id: &str,
        memory_id: &str,
    ) -> Result<Option<crate::domain::memory::MemoryRecord>, AppError> {
        connection
            .query_row(
                &select_memory_sql("id = ?2"),
                params![tenant_id, memory_id],
                map_memory,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn list(
        connection: &Connection,
        tenant_id: &str,
        limit: i64,
    ) -> Result<Vec<crate::domain::memory::MemoryRecord>, AppError> {
        let mut statement = connection
            .prepare(&format!(
                "{} ORDER BY created_at DESC, id ASC LIMIT ?2",
                select_memory_sql("tenant_id = ?1")
            ))
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id, limit], map_memory)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    // قراءة سياق الوكيل: ذاكرة نشطة غير منقضية فقط — PROPOSED والمعطَّل والمنقضي
    // مستبعدون (§44/C-7). النطاق: ذاكرة الوكيل + الذاكرة العامة النشطة.
    pub fn list_active_for_agent_context(
        connection: &Connection,
        tenant_id: &str,
        agent_id: &str,
        now: &str,
        limit: i64,
    ) -> Result<Vec<crate::domain::memory::MemoryRecord>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT id, tenant_id, memory_type, scope_resource_type, scope_resource_id,
                        agent_id, content, truth_class, provenance_kind, provenance_ref, status,
                        created_by, expires_at, created_at, updated_at
                 FROM memory_items
                 WHERE tenant_id = ?1
                   AND status = 'ACTIVE'
                   AND (expires_at IS NULL OR expires_at > ?2)
                   AND (agent_id = ?3 OR (memory_type IN ('EPISODIC', 'ORGANIZATIONAL') AND agent_id IS NULL))
                 ORDER BY created_at DESC, id ASC
                 LIMIT ?4",
            )
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id, now, agent_id, limit], map_memory)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn count_by_type(
        connection: &Connection,
        tenant_id: &str,
    ) -> Result<Vec<(String, String, i64)>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT memory_type, status, COUNT(*)
                 FROM memory_items
                 WHERE tenant_id = ?1
                 GROUP BY memory_type, status
                 ORDER BY memory_type ASC, status ASC",
            )
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }
}

fn select_memory_sql(filter: &str) -> String {
    format!(
        "SELECT id, tenant_id, memory_type, scope_resource_type, scope_resource_id,
                agent_id, content, truth_class, provenance_kind, provenance_ref, status,
                created_by, expires_at, created_at, updated_at
         FROM memory_items
         WHERE tenant_id = ?1 AND {filter}"
    )
}

fn map_memory(row: &rusqlite::Row<'_>) -> rusqlite::Result<crate::domain::memory::MemoryRecord> {
    Ok(crate::domain::memory::MemoryRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        memory_type: row.get(2)?,
        scope_resource_type: row.get(3)?,
        scope_resource_id: row.get(4)?,
        agent_id: row.get(5)?,
        content: row.get(6)?,
        truth_class: row.get(7)?,
        provenance_kind: row.get(8)?,
        provenance_ref: row.get(9)?,
        status: row.get(10)?,
        created_by: row.get(11)?,
        expires_at: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}
