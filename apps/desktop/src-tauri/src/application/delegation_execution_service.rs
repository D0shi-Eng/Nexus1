use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::{
    application::agent_run_service,
    domain::{
        error::AppError,
        models::AgentRecord,
        plan::{PlanRecord, PlanStepRecord},
        provider::ModelProvider,
    },
    infrastructure::{
        agent_execution_profile_repository::AgentExecutionProfileRepository,
        agent_run_execution_binding_repository::{
            AgentRunExecutionBindingRecord, AgentRunExecutionBindingRepository,
        },
        database::AppState,
        execution_result_repository::ExecutionResultRepository,
        knowledge_retrieval_binding_repository::{
            KnowledgeRetrievalBindingRecord, KnowledgeRetrievalBindingRepository,
        },
        model_registry_repository::ModelRegistryRepository,
    },
};

// نتيجة تنفيذ تفويض واحدة — مخرجات محدودة + اقتباسات متحققة + رمز فشل مغلق
pub(crate) struct DelegationExecutionOutcome {
    pub agent_run_id: String,
    pub output: Option<String>,
    pub citations_json: Option<String>,
    pub failure_code: Option<String>,
    pub completed: bool,
}

// تنفيذ تفويض محكوم: تشغيل جديد عبر مسار MP03 نفسه (ربط هوية + CAS + سياسة
// أدوات + تدقيق) تحت موافقة الخطة المربوطة — إذن مستقل لكل تفويض (C-11/§71)
// مع إعادة محاولة واحدة مشروطة لفشل مزود عابر حصراً (§72).
pub(crate) fn execute_delegation(
    state: &AppState,
    plan: &PlanRecord,
    step: &PlanStepRecord,
    agent: &AgentRecord,
    provider: &dyn ModelProvider,
) -> Result<DelegationExecutionOutcome, AppError> {
    let first = run_delegation_attempt(state, plan, step, agent, provider)?;
    if first.completed {
        return Ok(first);
    }
    // إعادة المحاولة المشروطة: مهلة/انقطاع مزود عابر فقط — لا إعادة بعد رفض
    let transient = matches!(
        first.failure_code.as_deref(),
        Some("PROVIDER_TIMEOUT") | Some("PROVIDER_UNAVAILABLE")
    );
    if !transient {
        return Ok(first);
    }
    let _second = run_delegation_attempt(state, plan, step, agent, provider)?;
    Ok(_second)
}

fn run_delegation_attempt(
    state: &AppState,
    plan: &PlanRecord,
    step: &PlanStepRecord,
    agent: &AgentRecord,
    provider: &dyn ModelProvider,
) -> Result<DelegationExecutionOutcome, AppError> {
    // 1) تشغيل جديد عبر المسار القياسي (صلاحية AGENT/EXECUTE + وكيل نشط + تدقيق)
    let created = agent_run_service::create_agent_run(
        state,
        agent_run_service::CreateAgentRunInput {
            agent_id: agent.id.clone(),
        },
    )?;

    // 2) ربط هوية التنفيذ الدقيقة: بروفايل الهدف + هدف الخطوة كمحتوى مهمة
    let model_id = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        let enabled =
            ModelRegistryRepository::list_enabled_local_models(&connection, &state.tenant_id())?;
        match enabled.as_slice() {
            [model_id] => model_id.clone(),
            _ => return Err(AppError::ModelUnavailable),
        }
    };
    let profile_id = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        let latest =
            AgentExecutionProfileRepository::latest(&transaction, &state.tenant_id(), &agent.id)?
                .ok_or(AppError::ExecutionNotConfigured)?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
        latest.id
    };
    let now = Utc::now().to_rfc3339();
    // 0024: تنفيذ التفويض يربط هوية الوجهة أيضاً، وإلا رفضه المنسق (فشل مغلق).
    let provider_identity =
        crate::application::ai_provider_registry_service::resolve_active_provider_identity(state)?;
    let task_content = step.objective.clone();
    let task_content_hash = format!("{:x}", Sha256::digest(task_content.as_bytes()));
    let approval_binding_hash = provider_identity.approval_binding_hash(
        model_id.as_str(),
        &profile_id,
        &task_content_hash,
        created.run.revision,
    );
    let binding = AgentRunExecutionBindingRecord {
        agent_run_id: created.run.id.clone(),
        tenant_id: state.tenant_id(),
        agent_run_revision: created.run.revision,
        agent_id: agent.id.clone(),
        execution_profile_version_id: profile_id,
        task_content,
        task_content_hash,
        model_id_at_request: Some(model_id.as_str().to_owned()),
        provider_id_at_request: Some(provider_identity.provider_id.clone()),
        provider_kind_at_request: Some(provider_identity.provider_kind.clone()),
        endpoint_identity_at_request: Some(provider_identity.endpoint_identity()),
        approval_binding_hash: Some(approval_binding_hash),
        bound_at: now.clone(),
    };
    {
        let connection = state
            .connection
            .lock()
            .map_err(|_| AppError::StorageOperation)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        AgentRunExecutionBindingRepository::insert(&transaction, &binding)?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
    }

    // 3) سياق معرفة محدود عند طلب الخطوة — استعلام الخطة/الخطوة فقط (§67)
    if step.requires_knowledge {
        let mut query = format!("{} {}", plan.goal, step.objective);
        query.truncate(
            query
                .char_indices()
                .nth(500)
                .map_or(query.len(), |(index, _)| index),
        );
        let retriever = crate::application::knowledge_service::FtsKnowledgeRetriever;
        let items = crate::application::context_engine::KnowledgeRetriever::retrieve(
            &retriever,
            state,
            query.trim(),
        )?;
        if items.is_empty() {
            return Err(AppError::KnowledgeRetrievalFailed);
        }
        let chunk_ids: Vec<String> = items.iter().map(|item| item.chunk_id.clone()).collect();
        let binding = KnowledgeRetrievalBindingRecord {
            agent_run_id: created.run.id.clone(),
            tenant_id: state.tenant_id(),
            agent_run_revision: created.run.revision,
            query: query.trim().to_owned(),
            query_hash: format!("{:x}", Sha256::digest(query.trim().as_bytes())),
            chunk_ids_json: serde_json::to_string(&chunk_ids)
                .map_err(|_| AppError::StorageOperation)?,
            bound_at: Utc::now().to_rfc3339(),
        };
        let connection = state
            .connection
            .lock()
            .map_err(|_| AppError::StorageOperation)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        KnowledgeRetrievalBindingRepository::insert(&transaction, &binding)?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
    }

    // 4) التنفيذ عبر منسق MP03 (بلا انتظار موافقة تشغيل — موافقة الخطة تغطي
    //    هوية الخطة/الخطوات، وصلاحيات وإجراءات كل نداء تبقى مفروضة هنا)
    let outcome =
        crate::application::agent_runtime_coordinator::start_queued_execution_with_provider(
            state,
            &created.run.id,
            provider,
        )?;

    let (citations_json, completed, failure_code, output) = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        let (citations, _truth) = ExecutionResultRepository::find_intelligence_fields(
            &connection,
            &state.tenant_id(),
            &created.run.id,
        )?
        .unwrap_or((None, "AI_INTERPRETATION".to_owned()));
        let result = outcome.result.as_ref();
        (
            citations,
            result
                .map(|value| value.outcome == "COMPLETED")
                .unwrap_or(false),
            result.and_then(|value| value.failure_code.clone()),
            result.and_then(|value| value.output.clone()),
        )
    };
    Ok(DelegationExecutionOutcome {
        agent_run_id: created.run.id,
        output,
        citations_json,
        failure_code,
        completed,
    })
}
