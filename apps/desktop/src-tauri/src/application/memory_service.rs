use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        memory::{
            MemoryItemStatus, MemoryRecord, MemoryType, MemoryWriteDecision, MemoryWritePolicy,
            MAX_MEMORY_CONTENT_CHARS,
        },
    },
    infrastructure::{
        database::{append_audit_event, latest_audit_event_hash, AppState},
        memory_repository::{MemoryInsert, MemoryRepository},
    },
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySummaryEntry {
    pub memory_type: String,
    pub status: String,
    pub count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryData {
    pub summary: Vec<MemorySummaryEntry>,
    pub items: Vec<MemoryRecord>,
    pub generated_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisableMemoryItemInput {
    pub memory_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposeSemanticMemoryInput {
    pub content: String,
    pub agent_run_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposeSemanticMemoryResult {
    pub memory_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromoteMemoryItemInput {
    pub memory_id: String,
}

// قراءة سطح إدارة الذاكرة (§102): ملخص بالأنواع/الحالات + عناصر محدودة —
// لا عناصر working خام ولا داخلية prompt — قراءة فقط لا غير هنا.
pub fn list_memory(state: &AppState) -> Result<MemoryData, AppError> {
    authorization_service::require_permission(state, "MEMORY", "READ")?;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let summary = MemoryRepository::count_by_type(&connection, &state.tenant_id())?;
    let items = MemoryRepository::list(&connection, &state.tenant_id(), 50)?;
    Ok(MemoryData {
        summary: summary
            .into_iter()
            .map(|(memory_type, status, count)| MemorySummaryEntry {
                memory_type,
                status,
                count,
            })
            .collect(),
        items,
        generated_at: Utc::now().to_rfc3339(),
    })
}

// حدث حلقي: إنهاء تشغيل/تفويض مع أثر جانبي — معرفات وأحداث لا نصوص كاملة (C-7)
pub const EPISODIC_RETENTION_DAYS: i64 = 30;

pub fn record_episodic_run_event(
    state: &AppState,
    scope_resource_id: &str,
    content: &str,
    provenance_kind: &str,
) -> Result<(), AppError> {
    let decision = MemoryWritePolicy::decide_for_run_outcome(true);
    if decision != MemoryWriteDecision::StoreEpisodic {
        return Ok(());
    }
    let content = content.trim();
    if content.is_empty() || content.chars().count() > MAX_MEMORY_CONTENT_CHARS {
        return Err(AppError::MemoryScopeInvalid);
    }
    // أحداث حلقية محدودة الاحتفاظ — لا تراكم أبدي (§45/C-7)
    let expires_at = (Utc::now() + chrono::Duration::days(EPISODIC_RETENTION_DAYS)).to_rfc3339();
    let truth = MemoryWritePolicy::initial_truth_class(MemoryType::Episodic)
        .as_code()
        .to_owned();
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let memory_id = format!("memory_{}", Uuid::new_v4());
    MemoryRepository::insert(
        &transaction,
        &state.tenant_id(),
        &MemoryInsert {
            id: &memory_id,
            memory_type: MemoryType::Episodic,
            scope_resource_type: "AGENT_RUN",
            scope_resource_id: Some(scope_resource_id),
            agent_id: None,
            content,
            truth_class: &truth,
            provenance_kind,
            provenance_ref: Some(scope_resource_id),
            initial_status: MemoryItemStatus::Active,
            created_by: &state.current_user_id()?,
            expires_at: Some(&expires_at),
        },
        &now,
    )?;
    append_memory_audit(&transaction, state, "MEMORY_ITEM_CREATED", &memory_id, &now)?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

// حدث حلقي لنطاق الخطة: إنهاء خطة — معرفات وحالة لا نصوص كاملة (C-7)
pub fn record_episodic_plan_event(
    state: &AppState,
    plan_id: &str,
    final_status: crate::domain::plan::PlanStatus,
) -> Result<(), AppError> {
    let content = format!(
        "Plan {} finished with status {}",
        plan_id,
        final_status.as_code()
    );
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let memory_id = format!("memory_{}", Uuid::new_v4());
    MemoryRepository::insert(
        &transaction,
        &state.tenant_id(),
        &MemoryInsert {
            id: &memory_id,
            memory_type: MemoryType::Episodic,
            scope_resource_type: "PLAN",
            scope_resource_id: Some(plan_id),
            agent_id: None,
            content: &content,
            truth_class: "FACT",
            provenance_kind: "PLAN",
            provenance_ref: Some(plan_id),
            initial_status: MemoryItemStatus::Active,
            created_by: &state.current_user_id()?,
            expires_at: None,
        },
        &now,
    )?;
    append_memory_audit(&transaction, state, "MEMORY_ITEM_CREATED", &memory_id, &now)?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

// اقتراح دلالي (AI): PROPOSED بلا أي سلطة — لا يُقرأ في سياق الوكلاء ولا
// يرقى إلا عبر بوابة بشرية صريحة (§38/C-7). المقترح يُسجل بمنشأ تشغيله.
pub fn propose_semantic(
    state: &AppState,
    input: ProposeSemanticMemoryInput,
) -> Result<ProposeSemanticMemoryResult, AppError> {
    authorization_service::require_permission(state, "KNOWLEDGE", "READ")?;
    let content = input.content.trim();
    if content.is_empty() || content.chars().count() > MAX_MEMORY_CONTENT_CHARS {
        return Err(AppError::MemoryScopeInvalid);
    }
    if !MemoryWritePolicy::content_grants_no_authority(content) {
        // محتوى يطلب سلطة — يُرفض كلياً ولا يُخزن حتى كاقتراح (T5)
        let rejection_ref = format!("rejected_{:x}", Sha256::digest(content.as_bytes()));
        append_rejection_audit(state, &rejection_ref)?;
        return Err(AppError::MemoryWriteDenied);
    }
    let provenance_ref = input
        .agent_run_id
        .as_deref()
        .map(|run_id| {
            if run_id.trim().is_empty() || run_id.chars().count() > 128 {
                Err(AppError::MemoryScopeInvalid)
            } else {
                Ok(run_id.trim().to_owned())
            }
        })
        .transpose()?;

    // بوابة السياسة: AI يقترح فقط — لا تخزين موثقاً ولا سلطة (C-7/§42)
    if MemoryWritePolicy::decide_for_ai_proposal() != MemoryWriteDecision::ProposeSemantic {
        return Err(AppError::MemoryWriteDenied);
    }
    let initial_truth = MemoryWritePolicy::initial_truth_class(MemoryType::Semantic)
        .as_code()
        .to_owned();

    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    // provenance يجب أن يشير إلى تشغيل حقيقي داخل المستأجر النشط: مرجع مُختلق يجعل
    // أثر التدقيق مضلِّلاً، فيُرفض بدل تخزينه كدعوى بلا أصل.
    if let Some(run_id) = provenance_ref.as_deref() {
        let run_exists =
            crate::infrastructure::agent_run_repository::AgentRunRepository::find_in_transaction(
                &transaction,
                &state.tenant_id(),
                run_id,
            )?
            .is_some();
        if !run_exists {
            return Err(AppError::ResourceNotFound);
        }
    }
    let memory_id = format!("memory_{}", Uuid::new_v4());
    MemoryRepository::insert(
        &transaction,
        &state.tenant_id(),
        &MemoryInsert {
            id: &memory_id,
            memory_type: MemoryType::Semantic,
            scope_resource_type: "TENANT",
            scope_resource_id: None,
            agent_id: None,
            content,
            truth_class: &initial_truth,
            provenance_kind: "AGENT_RUN",
            provenance_ref: provenance_ref.as_deref(),
            initial_status: MemoryItemStatus::Proposed,
            created_by: &state.current_user_id()?,
            expires_at: None,
        },
        &now,
    )?;
    append_memory_audit(&transaction, state, "MEMORY_ITEM_CREATED", &memory_id, &now)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    Ok(ProposeSemanticMemoryResult { memory_id })
}

// البوابة البشرية: ترقية اقتراح دلالي/تنظيمي إلى VERIFIED — دور TENANT_ADMIN
// + صلاحية MEMORY/MANAGE حصراً؛ الانتقال محصور CLAIM→VERIFIED ببشر (C-8).
pub fn promote_memory_item(
    state: &AppState,
    input: PromoteMemoryItemInput,
) -> Result<(), AppError> {
    authorization_service::require_permission(state, "MEMORY", "MANAGE")?;
    let memory_id = input.memory_id.trim();
    if memory_id.is_empty() || memory_id.chars().count() > 128 {
        return Err(AppError::MemoryScopeInvalid);
    }
    let existing = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        MemoryRepository::find(&connection, &state.tenant_id(), memory_id)?
            .ok_or(AppError::ResourceNotFound)?
    };
    // بوابة النوع + انتقال الحقيقة الصريح: CLAIM→VERIFIED بفاعل بشري حصراً (C-8)
    if !MemoryWritePolicy::requires_human_gate(
        MemoryType::from_code(&existing.memory_type).map_err(|_| AppError::MemoryScopeInvalid)?,
    ) {
        return Err(AppError::MemoryScopeInvalid);
    }
    let current_truth = crate::domain::truth::TruthClass::from_code(&existing.truth_class)
        .map_err(|_| AppError::MemoryScopeInvalid)?;
    if !crate::domain::truth::can_promote(
        current_truth,
        crate::domain::truth::TruthClass::Verified,
        crate::domain::truth::TruthPromotionActor::HumanReviewer,
    ) {
        return Err(AppError::InvalidStateTransition);
    }

    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    authorization_service::require_role_in_transaction(&transaction, state, "TENANT_ADMIN")?;
    let changed =
        MemoryRepository::promote_semantic(&transaction, &state.tenant_id(), memory_id, &now)?;
    if !changed {
        return Err(AppError::InvalidStateTransition);
    }
    append_memory_audit(&transaction, state, "MEMORY_ITEM_PROMOTED", memory_id, &now)?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

pub fn disable_memory_item(
    state: &AppState,
    input: DisableMemoryItemInput,
) -> Result<(), AppError> {
    authorization_service::require_permission(state, "MEMORY", "MANAGE")?;
    let memory_id = input.memory_id.trim();
    if memory_id.is_empty() || memory_id.chars().count() > 128 {
        return Err(AppError::MemoryScopeInvalid);
    }
    let now = Utc::now().to_rfc3339();
    let existing = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        MemoryRepository::find(&connection, &state.tenant_id(), memory_id)?
            .ok_or(AppError::ResourceNotFound)?
    };
    if existing.status == "DISABLED" {
        return Err(AppError::InvalidStateTransition);
    }
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let changed = MemoryRepository::set_status(
        &transaction,
        &state.tenant_id(),
        memory_id,
        MemoryItemStatus::Disabled,
        &now,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    append_memory_audit(&transaction, state, "MEMORY_ITEM_DISABLED", memory_id, &now)?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

fn append_rejection_audit(state: &AppState, rejection_ref: &str) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    append_memory_audit(
        &transaction,
        state,
        "MEMORY_ITEM_REJECTED",
        rejection_ref,
        &now,
    )?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

fn append_memory_audit(
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
        "MemoryItem",
        resource_id,
        occurred_at,
        &previous_hash,
    )
    .map(|_| ())
}

#[cfg(test)]
#[path = "memory_service_tests.rs"]
mod tests;
