use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    application::{authorization_service, context_engine::KnowledgeRetriever},
    domain::{
        error::AppError,
        knowledge::{
            chunk_deterministic, content_hash, sanitize_fts_query, validate_source_text,
            KnowledgeDocumentFormat, KnowledgeProvenance, KnowledgeRetrievalItem,
            KnowledgeSourceStatus, KnowledgeSourceType, MAX_FILENAME_CHARS, MAX_ORIGIN_CHARS,
            MAX_QUERY_CHARS, MAX_TITLE_CHARS,
        },
    },
    infrastructure::{
        database::{append_audit_event, latest_audit_event_hash, AppState},
        knowledge_repository::{KnowledgeRepository, KnowledgeSourceInsert},
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestKnowledgeSourceInput {
    pub title: String,
    pub origin: String,
    pub text: String,
    pub filename_hint: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestKnowledgeSourceResult {
    pub source_id: String,
    pub document_id: String,
    pub chunk_count: usize,
    pub duplicate: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSourceView {
    pub id: String,
    pub source_type: String,
    pub title: String,
    pub origin: String,
    pub provenance: String,
    pub status: String,
    pub chunk_count: i64,
    pub document_count: i64,
    pub created_at: String,
    pub ingested_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSourcesData {
    pub items: Vec<KnowledgeSourceView>,
    pub generated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSourceDetails {
    #[serde(flatten)]
    pub source: KnowledgeSourceView,
    pub origin_ref_resource_type: Option<String>,
    pub origin_ref_resource_id: Option<String>,
    pub documents: Vec<KnowledgeDocumentView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeDocumentView {
    pub id: String,
    pub format: String,
    pub original_filename: Option<String>,
    pub char_count: i64,
    pub extraction_status: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchKnowledgeInput {
    pub query: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSearchResultData {
    pub items: Vec<KnowledgeRetrievalItem>,
    pub generated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisableKnowledgeSourceInput {
    pub source_id: String,
}

// إدخال مصدر معرفة منتج (USER_PROVIDED_DOCUMENT حصراً عبر IPC): تحقق محتوى
// حتمي ثم هاش/إزالة تكرار ثم تقسيم حتمي وفهرسة FTS — كل ذلك معاملة قصيرة
// واحدة، والأصل بعد الإدخال محصن ضد إعادة الكتابة (C-1/C-2).
pub fn ingest_user_document(
    state: &AppState,
    input: IngestKnowledgeSourceInput,
) -> Result<IngestKnowledgeSourceResult, AppError> {
    authorization_service::require_permission(state, "KNOWLEDGE", "INGEST")?;
    let title = validate_bounded(&input.title, MAX_TITLE_CHARS)?.to_owned();
    let origin = validate_bounded(&input.origin, MAX_ORIGIN_CHARS)?.to_owned();
    let text = input.text.trim();
    let format = match input.filename_hint.as_deref() {
        Some(hint) => {
            let hint = hint.trim();
            if hint.chars().count() > MAX_FILENAME_CHARS {
                return Err(AppError::KnowledgeSourceInvalid);
            }
            KnowledgeDocumentFormat::from_filename_hint(hint)
                .ok_or(AppError::KnowledgeSourceInvalid)?
        }
        None => KnowledgeDocumentFormat::Txt,
    };
    validate_source_text(text)?;

    let normalized = text.replace('\r', "");
    let normalized = normalized.trim();
    let hash = content_hash(normalized);
    let now = Utc::now().to_rfc3339();

    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    // إزالة التكرار الحتمي: نفس هاش المحتوى داخل المستأجر يعيد المصدر القائم
    if let Some(existing) =
        KnowledgeRepository::find_source_by_content_hash(&transaction, &state.tenant_id(), &hash)?
    {
        let document = KnowledgeRepository::list_documents_for_source(
            &transaction,
            &state.tenant_id(),
            &existing.id,
        )?
        .pop()
        .ok_or(AppError::DataAccess)?;
        let chunk_count = KnowledgeRepository::count_chunks_for_document(
            &transaction,
            &state.tenant_id(),
            &document.id,
        )?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
        return Ok(IngestKnowledgeSourceResult {
            source_id: existing.id,
            document_id: document.id,
            chunk_count: chunk_count.max(0) as usize,
            duplicate: true,
        });
    }

    let chunks = chunk_deterministic(normalized)?;
    let source_id = format!("knowledge_source_{}", Uuid::new_v4());
    let document_id = format!("knowledge_document_{}", Uuid::new_v4());
    KnowledgeRepository::insert_source(
        &transaction,
        &state.tenant_id(),
        &state.current_user_id()?,
        &KnowledgeSourceInsert {
            id: &source_id,
            source_type: KnowledgeSourceType::UserProvidedDocument,
            title: &title,
            origin: &origin,
            provenance: KnowledgeProvenance::UserProvided,
            content_hash: Some(&hash),
            origin_ref_resource_type: None,
            origin_ref_resource_id: None,
        },
        &now,
    )?;
    KnowledgeRepository::insert_document(
        &transaction,
        &state.tenant_id(),
        &document_id,
        &source_id,
        format,
        input.filename_hint.as_deref().map(str::trim),
        normalized,
        "EXTRACTED",
        &now,
    )?;
    KnowledgeRepository::insert_chunks(
        &transaction,
        &state.tenant_id(),
        &document_id,
        &chunks,
        &now,
    )?;
    append_audit(
        &transaction,
        state,
        "KNOWLEDGE_SOURCE_INGESTED",
        &source_id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(IngestKnowledgeSourceResult {
        source_id,
        document_id,
        chunk_count: chunks.len(),
        duplicate: false,
    })
}

pub fn list_sources(state: &AppState) -> Result<KnowledgeSourcesData, AppError> {
    authorization_service::require_permission(state, "KNOWLEDGE", "READ")?;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let sources = KnowledgeRepository::list_sources(&connection, &state.tenant_id())?;
    let mut items = Vec::with_capacity(sources.len());
    for source in sources {
        let chunk_count = KnowledgeRepository::count_chunks_for_source(
            &connection,
            &state.tenant_id(),
            &source.id,
        )?;
        let document_count = KnowledgeRepository::list_documents_for_source(
            &connection,
            &state.tenant_id(),
            &source.id,
        )?
        .len() as i64;
        items.push(KnowledgeSourceView {
            id: source.id.clone(),
            source_type: source.source_type.clone(),
            title: source.title.clone(),
            origin: source.origin.clone(),
            provenance: source.provenance.clone(),
            status: source.status.as_code().to_owned(),
            chunk_count,
            document_count,
            created_at: source.created_at.clone(),
            ingested_at: source.ingested_at.clone(),
            updated_at: source.updated_at.clone(),
        });
    }
    Ok(KnowledgeSourcesData {
        items,
        generated_at: Utc::now().to_rfc3339(),
    })
}

pub fn get_source(state: &AppState, source_id: String) -> Result<KnowledgeSourceDetails, AppError> {
    authorization_service::require_permission(state, "KNOWLEDGE", "READ")?;
    let source_id = validate_bounded(&source_id, 128)?.to_owned();
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let source = KnowledgeRepository::find_source(&connection, &state.tenant_id(), &source_id)?
        .ok_or(AppError::ResourceNotFound)?;
    let documents = KnowledgeRepository::list_documents_for_source(
        &connection,
        &state.tenant_id(),
        &source.id,
    )?;
    let chunk_count =
        KnowledgeRepository::count_chunks_for_source(&connection, &state.tenant_id(), &source.id)?;
    Ok(KnowledgeSourceDetails {
        origin_ref_resource_type: source.origin_ref_resource_type.clone(),
        origin_ref_resource_id: source.origin_ref_resource_id.clone(),
        documents: documents
            .into_iter()
            .map(|document| KnowledgeDocumentView {
                id: document.id,
                format: document.format,
                original_filename: document.original_filename,
                char_count: document.char_count,
                extraction_status: document.extraction_status,
                created_at: document.created_at,
            })
            .collect(),
        source: KnowledgeSourceView {
            id: source.id,
            source_type: source.source_type,
            title: source.title,
            origin: source.origin,
            provenance: source.provenance,
            status: source.status.as_code().to_owned(),
            chunk_count,
            document_count: 0,
            created_at: source.created_at,
            ingested_at: source.ingested_at,
            updated_at: source.updated_at,
        },
    })
}

// تعطيل المصدر: التغيير الوحيد المسموح — يستبعد المصدر من كل استرجاع لاحق
pub fn disable_source(
    state: &AppState,
    input: DisableKnowledgeSourceInput,
) -> Result<(), AppError> {
    authorization_service::require_permission(state, "KNOWLEDGE", "DISABLE")?;
    let source_id = validate_bounded(&input.source_id, 128)?.to_owned();
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let source = KnowledgeRepository::find_source_in_transaction(
        &transaction,
        &state.tenant_id(),
        &source_id,
    )?
    .ok_or(AppError::ResourceNotFound)?;
    if source.status == KnowledgeSourceStatus::Disabled {
        return Err(AppError::InvalidStateTransition);
    }
    let changed = KnowledgeRepository::set_source_status(
        &transaction,
        &state.tenant_id(),
        &source_id,
        KnowledgeSourceStatus::Disabled,
        &now,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    append_audit(
        &transaction,
        state,
        "KNOWLEDGE_SOURCE_DISABLED",
        &source_id,
        &now,
    )?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

// بحث المعاينة للواجهة — نفس مسار الاسترجاع المحكوم (تجريد Retriever)
pub fn search(
    state: &AppState,
    input: SearchKnowledgeInput,
) -> Result<KnowledgeSearchResultData, AppError> {
    authorization_service::require_permission(state, "KNOWLEDGE", "READ")?;
    let query = input.query.trim();
    if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS {
        return Err(AppError::KnowledgeSourceInvalid);
    }
    let items = FtsKnowledgeRetriever.retrieve(state, query)?;
    Ok(KnowledgeSearchResultData {
        items,
        generated_at: Utc::now().to_rfc3339(),
    })
}

// تنفيذ KnowledgeRetriever الوحيد حالياً: FTS5 محلي مقيد بالمستأجر
// والمصادر النشطة — الوكيل يعتمد على التجريد لا هذا النوع (C-3).
pub struct FtsKnowledgeRetriever;

impl KnowledgeRetriever for FtsKnowledgeRetriever {
    fn retrieve(
        &self,
        state: &AppState,
        query: &str,
    ) -> Result<Vec<KnowledgeRetrievalItem>, AppError> {
        let fts_query = sanitize_fts_query(query).ok_or(AppError::KnowledgeSourceInvalid)?;
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        KnowledgeRepository::search_chunks(&connection, &state.tenant_id(), &fts_query)
    }
}

fn validate_bounded<'a>(value: &'a str, max_chars: usize) -> Result<&'a str, AppError> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.chars().count() > max_chars {
        return Err(AppError::KnowledgeSourceInvalid);
    }
    Ok(trimmed)
}

fn append_audit(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    event_type: &str,
    resource_id: &str,
    occurred_at: &str,
) -> Result<(), AppError> {
    let audit_id = format!("audit_{}", Uuid::new_v4());
    let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
    append_audit_event(
        transaction,
        &state.tenant_id(),
        &audit_id,
        event_type,
        &state.current_user_id()?,
        "KnowledgeSource",
        resource_id,
        occurred_at,
        &previous_hash,
    )
    .map(|_| ())
}

// هاش استعلام المعرفة لربط الهوية الدقيقة (مع الربط في request service)
pub fn query_hash(query: &str) -> String {
    format!("{:x}", Sha256::digest(query.trim().as_bytes()))
}

#[cfg(test)]
#[path = "knowledge_service_tests.rs"]
mod tests;
