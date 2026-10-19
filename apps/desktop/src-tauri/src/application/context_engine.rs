use crate::domain::{
    context_bundle::RetrievedContextBundle, error::AppError, knowledge::KnowledgeRetrievalItem,
    memory::MAX_MEMORY_ITEMS_PER_CONTEXT,
};
use crate::infrastructure::{
    database::AppState, knowledge_repository::KnowledgeRepository,
    memory_repository::MemoryRepository,
};

// تجريد الاسترجاع (§22/C-3): runtime الوكيل يعتمد على هذا التجريد حصراً —
// أي تنفيذ مستقبلي (هجين/دلالي) يستبدل الخلفية دون لمس الوكيل.
pub trait KnowledgeRetriever {
    fn retrieve(
        &self,
        state: &AppState,
        query: &str,
    ) -> Result<Vec<KnowledgeRetrievalItem>, AppError>;
}

// محرك السياق (§46): يقرر أي سياق موثوق يدخل التنفيذ ويطبق الميزانية
// الحتمية — لا تجميع سياق في React ولا في المزود ولا في المخطط.
pub struct ContextEngine;

impl ContextEngine {
    // بناء حزمة سياق التنفيذ من الهوية المرتبطة: قطع المعرفة المرتبطة تُعاد
    // تحميلها بشرط بقاء مصادرها نشطة + ذاكرة الوكيل المصرح بها فقط، ثم تُطبَّق
    // الميزانية الحتمية (المعرفة والذاكرة بيانات غير موثوقة داخل فواصل C-4/§48).
    pub fn build_bound_context(
        state: &AppState,
        chunk_ids: &[String],
        agent_id: &str,
    ) -> Result<RetrievedContextBundle, AppError> {
        let items = Self::reload_bound_chunks(state, chunk_ids)?;
        let memory = {
            let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
            MemoryRepository::list_active_for_agent_context(
                &connection,
                &state.tenant_id(),
                agent_id,
                &chrono::Utc::now().to_rfc3339(),
                MAX_MEMORY_ITEMS_PER_CONTEXT as i64,
            )?
        };
        Ok(RetrievedContextBundle::build(items, memory))
    }

    // إعادة تحميل القطع المرتبطة بهوية تشغيل عند التنفيذ — أي انقضاء
    // (تعطيل مصدر/حذف منطقي) يعني أن الهوية لم تعد صالحة: فشل مغلق (C-4).
    pub fn reload_bound_chunks(
        state: &AppState,
        chunk_ids: &[String],
    ) -> Result<Vec<KnowledgeRetrievalItem>, AppError> {
        if chunk_ids.is_empty() {
            return Err(AppError::KnowledgeRetrievalFailed);
        }
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        let items = KnowledgeRepository::find_chunks_by_ids_active_only(
            &connection,
            &state.tenant_id(),
            chunk_ids,
        )?;
        if items.len() != chunk_ids.len() {
            // قطعة مرتبطة لم تعد متاحة (مصدر معطل/محجوب) — لا تنفيذ بهوية منقضية
            return Err(AppError::StaleApproval);
        }
        Ok(items)
    }
}
