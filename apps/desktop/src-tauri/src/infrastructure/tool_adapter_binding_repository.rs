use rusqlite::{params, Transaction};

use crate::domain::error::AppError;

// سجل ربط المحوّلات الموثوق — الكتابة عبر البذر الخلفي فقط (لا IPC تحرير)،
// والصفوف محصنة ضد التعديل والحذف بمشغلات SQL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolAdapterBindingRecord {
    pub id: String,
    pub tenant_id: String,
    pub tool_version_id: String,
    pub adapter_key: String,
    pub bound_at: String,
}

pub struct ToolAdapterBindingRepository;

impl ToolAdapterBindingRepository {
    pub fn find_for_version(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        tool_version_id: &str,
    ) -> Result<Option<ToolAdapterBindingRecord>, AppError> {
        let mut statement = transaction
            .prepare(
                "SELECT id, tenant_id, tool_version_id, adapter_key, bound_at
                 FROM tool_adapter_bindings
                 WHERE tenant_id = ?1 AND tool_version_id = ?2",
            )
            .map_err(|_| AppError::StorageOperation)?;
        let mut rows = statement
            .query_map(params![tenant_id, tool_version_id], map_binding)
            .map_err(|_| AppError::StorageOperation)?;
        rows.next().transpose().map_err(|_| AppError::DataAccess)
    }

    pub fn insert(
        transaction: &Transaction<'_>,
        record: &ToolAdapterBindingRecord,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO tool_adapter_bindings (id, tenant_id, tool_version_id, adapter_key, bound_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    record.id,
                    record.tenant_id,
                    record.tool_version_id,
                    record.adapter_key,
                    record.bound_at
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }
}

fn map_binding(row: &rusqlite::Row<'_>) -> Result<ToolAdapterBindingRecord, rusqlite::Error> {
    Ok(ToolAdapterBindingRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        tool_version_id: row.get(2)?,
        adapter_key: row.get(3)?,
        bound_at: row.get(4)?,
    })
}
