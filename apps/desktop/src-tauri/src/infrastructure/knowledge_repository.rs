use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::domain::{
    error::AppError,
    knowledge::{
        KnowledgeChunkPlan, KnowledgeDocumentFormat, KnowledgeProvenance, KnowledgeRetrievalItem,
        KnowledgeSourceStatus, KnowledgeSourceType, MAX_RETRIEVAL_ITEMS,
    },
};

// مستودع المعرفة: كل استعلام مقيد tenant_id حصراً — لا مسار بلا نطاق مستأجر.
pub struct KnowledgeRepository;

#[derive(Debug, Clone)]
pub struct KnowledgeSourceRecord {
    pub id: String,
    pub source_type: String,
    pub title: String,
    pub origin: String,
    pub provenance: String,
    pub status: KnowledgeSourceStatus,
    pub origin_ref_resource_type: Option<String>,
    pub origin_ref_resource_id: Option<String>,
    pub created_at: String,
    pub ingested_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct KnowledgeDocumentRecord {
    pub id: String,
    pub format: String,
    pub original_filename: Option<String>,
    pub char_count: i64,
    pub extraction_status: String,
    pub created_at: String,
}

pub struct KnowledgeSourceInsert<'a> {
    pub id: &'a str,
    pub source_type: KnowledgeSourceType,
    pub title: &'a str,
    pub origin: &'a str,
    pub provenance: KnowledgeProvenance,
    pub content_hash: Option<&'a str>,
    pub origin_ref_resource_type: Option<&'a str>,
    pub origin_ref_resource_id: Option<&'a str>,
}

impl KnowledgeRepository {
    pub fn insert_source(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        created_by: &str,
        insert: &KnowledgeSourceInsert<'_>,
        now: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO knowledge_sources (
                    id, tenant_id, source_type, title, origin, provenance, status,
                    content_hash, origin_ref_resource_type, origin_ref_resource_id,
                    created_by, created_at, ingested_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'ACTIVE', ?7, ?8, ?9, ?10, ?11, ?12, ?11)",
                params![
                    insert.id,
                    tenant_id,
                    insert.source_type.as_code(),
                    insert.title,
                    insert.origin,
                    insert.provenance.as_code(),
                    insert.content_hash,
                    insert.origin_ref_resource_type,
                    insert.origin_ref_resource_id,
                    created_by,
                    now,
                    now,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn find_source(
        connection: &Connection,
        tenant_id: &str,
        source_id: &str,
    ) -> Result<Option<KnowledgeSourceRecord>, AppError> {
        connection
            .query_row(
                "SELECT id, tenant_id, source_type, title, origin, provenance, status, content_hash,
                        origin_ref_resource_type, origin_ref_resource_id, created_by, created_at,
                        ingested_at, updated_at
                 FROM knowledge_sources
                 WHERE id = ?1 AND tenant_id = ?2",
                params![source_id, tenant_id],
                map_source,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn find_source_in_transaction(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        source_id: &str,
    ) -> Result<Option<KnowledgeSourceRecord>, AppError> {
        transaction
            .query_row(
                "SELECT id, tenant_id, source_type, title, origin, provenance, status, content_hash,
                        origin_ref_resource_type, origin_ref_resource_id, created_by, created_at,
                        ingested_at, updated_at
                 FROM knowledge_sources
                 WHERE id = ?1 AND tenant_id = ?2",
                params![source_id, tenant_id],
                map_source,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    // البحث عن مصدر بنفس هاش المحتوى داخل المستأجر — إزالة التكرار الحتمية
    pub fn find_source_by_content_hash(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        content_hash: &str,
    ) -> Result<Option<KnowledgeSourceRecord>, AppError> {
        transaction
            .query_row(
                "SELECT id, tenant_id, source_type, title, origin, provenance, status, content_hash,
                        origin_ref_resource_type, origin_ref_resource_id, created_by, created_at,
                        ingested_at, updated_at
                 FROM knowledge_sources
                 WHERE tenant_id = ?1 AND content_hash = ?2",
                params![tenant_id, content_hash],
                map_source,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn list_sources(
        connection: &Connection,
        tenant_id: &str,
    ) -> Result<Vec<KnowledgeSourceRecord>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT id, tenant_id, source_type, title, origin, provenance, status, content_hash,
                        origin_ref_resource_type, origin_ref_resource_id, created_by, created_at,
                        ingested_at, updated_at
                 FROM knowledge_sources
                 WHERE tenant_id = ?1
                 ORDER BY created_at DESC, id ASC",
            )
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id], map_source)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    // التعطيل هو التغيير الوحيد المسموح على المصدر (الأصل محصن بمشغّلات المخطط)
    pub fn set_source_status(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        source_id: &str,
        status: KnowledgeSourceStatus,
        updated_at: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE knowledge_sources
                 SET status = ?1, updated_at = ?2
                 WHERE id = ?3 AND tenant_id = ?4",
                params![status.as_code(), updated_at, source_id, tenant_id],
            )
            .map_err(|_| AppError::StorageOperation)
            .map(|changed| changed == 1)
    }

    pub fn insert_document(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        document_id: &str,
        source_id: &str,
        format: KnowledgeDocumentFormat,
        original_filename: Option<&str>,
        extracted_text: &str,
        extraction_status: &str,
        now: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO knowledge_documents (
                    id, tenant_id, source_id, format, original_filename, extracted_text,
                    char_count, extraction_status, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    document_id,
                    tenant_id,
                    source_id,
                    format.as_code(),
                    original_filename,
                    extracted_text,
                    extracted_text.chars().count() as i64,
                    extraction_status,
                    now,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn list_documents_for_source(
        connection: &Connection,
        tenant_id: &str,
        source_id: &str,
    ) -> Result<Vec<KnowledgeDocumentRecord>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT id, tenant_id, source_id, format, original_filename, char_count,
                        extraction_status, created_at
                 FROM knowledge_documents
                 WHERE tenant_id = ?1 AND source_id = ?2
                 ORDER BY created_at ASC, id ASC",
            )
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id, source_id], map_document)
            .map_err(|_| AppError::DataAccess)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn insert_chunks(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        document_id: &str,
        chunks: &[KnowledgeChunkPlan],
        now: &str,
    ) -> Result<(), AppError> {
        for chunk in chunks {
            transaction
                .execute(
                    "INSERT INTO knowledge_chunks (
                        id, tenant_id, document_id, chunk_index, text, text_hash,
                        char_start, char_end, char_count, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        format!("chunk_{}", uuid::Uuid::new_v4()),
                        tenant_id,
                        document_id,
                        chunk.chunk_index as i64,
                        chunk.text,
                        chunk.text_hash,
                        chunk.char_start as i64,
                        chunk.char_end as i64,
                        chunk.text.chars().count() as i64,
                        now,
                    ],
                )
                .map_err(|_| AppError::StorageOperation)?;
        }
        Ok(())
    }

    pub fn count_chunks_for_document(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        document_id: &str,
    ) -> Result<i64, AppError> {
        transaction
            .query_row(
                "SELECT COUNT(*) FROM knowledge_chunks WHERE tenant_id = ?1 AND document_id = ?2",
                params![tenant_id, document_id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::DataAccess)
    }

    pub fn count_chunks_for_source(
        connection: &Connection,
        tenant_id: &str,
        source_id: &str,
    ) -> Result<i64, AppError> {
        connection
            .query_row(
                "SELECT COUNT(*)
                 FROM knowledge_chunks c
                 INNER JOIN knowledge_documents d ON d.tenant_id = c.tenant_id AND d.id = c.document_id
                 WHERE c.tenant_id = ?1 AND d.source_id = ?2",
                params![tenant_id, source_id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::DataAccess)
    }

    // الاسترجاع المعجمي (FTS5): مقيد بالمستأجر وبالمصدر النشط حصراً —
    // مصدر معطَّل مستبعد كلياً حتى لو بقي فهرسه (T12/C-3).
    pub fn search_chunks(
        connection: &Connection,
        tenant_id: &str,
        fts_query: &str,
    ) -> Result<Vec<KnowledgeRetrievalItem>, AppError> {
        let mut statement = connection
            .prepare(
                "SELECT c.id, c.document_id, s.id, s.title, s.provenance,
                        c.text, bm25(knowledge_chunks_fts) AS score
                 FROM knowledge_chunks_fts f
                 INNER JOIN knowledge_chunks c ON c.id = f.chunk_id
                 INNER JOIN knowledge_documents d ON d.tenant_id = c.tenant_id AND d.id = c.document_id
                 INNER JOIN knowledge_sources s ON s.tenant_id = d.tenant_id AND s.id = d.source_id
                 WHERE knowledge_chunks_fts MATCH ?1
                   AND c.tenant_id = ?2
                   AND s.status = 'ACTIVE'
                 ORDER BY score ASC, c.id ASC
                 LIMIT ?3",
            )
            .map_err(|_| AppError::KnowledgeRetrievalFailed)?;
        let rows = statement
            .query_map(params![fts_query, tenant_id, MAX_RETRIEVAL_ITEMS], |row| {
                Ok(KnowledgeRetrievalItem {
                    chunk_id: row.get(0)?,
                    document_id: row.get(1)?,
                    source_id: row.get(2)?,
                    source_title: row.get(3)?,
                    provenance: row.get(4)?,
                    content: row.get(5)?,
                    score: row.get(6)?,
                    rank: 0,
                })
            })
            .map_err(|_| AppError::KnowledgeRetrievalFailed)?;
        let mut items = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::KnowledgeRetrievalFailed)?;
        for (index, item) in items.iter_mut().enumerate() {
            item.rank = index as u32;
        }
        Ok(items)
    }

    // إعادة تحميل القطع المرتبطة بهوية تشغيل — مع تحقق أن مصادرها لا تزال نشطة
    pub fn find_chunks_by_ids_active_only(
        connection: &Connection,
        tenant_id: &str,
        chunk_ids: &[String],
    ) -> Result<Vec<KnowledgeRetrievalItem>, AppError> {
        if chunk_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = chunk_ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "SELECT c.id, c.document_id, s.id, s.title, s.provenance, c.text
             FROM knowledge_chunks c
             INNER JOIN knowledge_documents d ON d.tenant_id = c.tenant_id AND d.id = c.document_id
             INNER JOIN knowledge_sources s ON s.tenant_id = d.tenant_id AND s.id = d.source_id
             WHERE c.tenant_id = ? AND c.id IN ({placeholders})
               AND s.status = 'ACTIVE'
             ORDER BY c.id ASC"
        );
        let mut parameters: Vec<&dyn rusqlite::ToSql> = vec![&tenant_id];
        for chunk_id in chunk_ids {
            parameters.push(chunk_id);
        }
        let mut statement = connection
            .prepare(sql.as_str())
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(parameters.as_slice(), |row| {
                Ok(KnowledgeRetrievalItem {
                    chunk_id: row.get(0)?,
                    document_id: row.get(1)?,
                    source_id: row.get(2)?,
                    source_title: row.get(3)?,
                    provenance: row.get(4)?,
                    content: row.get(5)?,
                    score: 0.0,
                    rank: 0,
                })
            })
            .map_err(|_| AppError::DataAccess)?;
        let mut items = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)?;
        // الحفاظ على ترتيب الربط الأصلي (ترتيب الاسترجاع وقت الطلب)
        let mut ordered = Vec::with_capacity(items.len());
        for chunk_id in chunk_ids {
            if let Some(position) = items.iter().position(|item| &item.chunk_id == chunk_id) {
                ordered.push(items.remove(position));
            }
        }
        for (index, item) in ordered.iter_mut().enumerate() {
            item.rank = index as u32;
        }
        Ok(ordered)
    }
}

fn map_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeSourceRecord> {
    let status: String = row.get(6)?;
    let parse_error = |message: String| {
        rusqlite::Error::FromSqlConversionFailure(
            6,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                message,
            )),
        )
    };
    let status = KnowledgeSourceStatus::from_code(status.as_str())
        .map_err(|error| parse_error(error.to_string()))?;
    let _ = (
        row.get::<_, String>(1)?,
        row.get::<_, Option<String>>(7)?,
        row.get::<_, String>(10)?,
    );
    Ok(KnowledgeSourceRecord {
        id: row.get(0)?,
        source_type: row.get(2)?,
        title: row.get(3)?,
        origin: row.get(4)?,
        provenance: row.get(5)?,
        status,
        origin_ref_resource_type: row.get(8)?,
        origin_ref_resource_id: row.get(9)?,
        created_at: row.get(11)?,
        ingested_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn map_document(row: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgeDocumentRecord> {
    Ok(KnowledgeDocumentRecord {
        id: row.get(0)?,
        format: row.get(3)?,
        original_filename: row.get(4)?,
        char_count: row.get(5)?,
        extraction_status: row.get(6)?,
        created_at: row.get(7)?,
    })
}
