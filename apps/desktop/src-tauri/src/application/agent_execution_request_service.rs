use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    application::{
        agent_execution_profile_service::validate_task_content,
        agent_run_approval_service::{
            request_agent_run_approval_with_skill, AgentRunApprovalRequirement,
        },
        agent_run_service, ai_provider_registry_service, authorization_service,
    },
    domain::{
        error::AppError,
        models::{AgentRunRecord, AgentStatus},
    },
    infrastructure::{
        agent_execution_profile_repository::AgentExecutionProfileRepository,
        agent_repository::AgentRepository,
        agent_run_execution_binding_repository::{
            AgentRunExecutionBindingRecord, AgentRunExecutionBindingRepository,
        },
        database::{append_audit_event, latest_audit_event_hash, AppState},
        model_registry_repository::ModelRegistryRepository,
    },
};

// طلب التنفيذ العام يتطلب موافقة مراجع مستقلة دائماً قبل أي تشغيل —
// رمز المتطلب ودوره ثابتان خلفياً ولا يقبلهما العميل.
pub const AGENT_EXECUTION_REQUIREMENT_CODE: &str = "AGENT_EXECUTION";
pub const AGENT_EXECUTION_REQUIRED_ROLE: &str = "TENANT_ADMIN";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestAgentExecutionInput {
    pub agent_id: String,
    pub task_content: String,
    pub skill_version_id: Option<String>,
    pub knowledge_query: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecutionRequestResult {
    pub run: AgentRunRecord,
    pub approval_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunExecutionBindingView {
    pub agent_run_id: String,
    pub agent_run_revision: i64,
    pub execution_profile_version_id: String,
    pub task_content_hash: String,
    pub model_id_at_request: Option<String>,
    /// 0024: هوية الوجهة المُثبتة. `None` يعني ربطاً تاريخياً بلا هوية وجهة
    /// (لا يقبل التنفيذ) — يُعرض بصدق ولا يُقنَّع.
    pub provider_id_at_request: Option<String>,
    pub provider_kind_at_request: Option<String>,
    pub endpoint_identity_at_request: Option<String>,
    pub approval_binding_hash: Option<String>,
    pub bound_at: String,
}

// حالة الاستخدام العامة الواحدة: تشغيل + ربط هوية التنفيذ الدقيق + طلب موافقة
// مرتبط بالهوية (نسخة البروفايل + هاش المهمة + النموذج المرصود) في استدعاء واحد.
pub fn request_agent_execution(
    state: &AppState,
    input: RequestAgentExecutionInput,
) -> Result<AgentExecutionRequestResult, AppError> {
    authorization_service::require_permission(state, "AGENT", "EXECUTE")?;
    if input.agent_id.trim().is_empty() || input.agent_id.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    validate_task_content(&input.task_content)?;
    if let Some(skill_version_id) = input.skill_version_id.as_deref() {
        if skill_version_id.trim().is_empty() || skill_version_id.chars().count() > 128 {
            return Err(AppError::ValidationFailed);
        }
    }

    // النموذج التنفيذي جزء من هوية التنفيذ المرتبطة — لا طلب بلا نموذج مفعّل.
    let configured_model = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        let enabled =
            ModelRegistryRepository::list_enabled_local_models(&connection, &state.tenant_id())?;
        match enabled.as_slice() {
            [model_id] => Some(model_id.clone()),
            _ => None,
        }
    };
    let Some(configured_model) = configured_model else {
        return Err(AppError::ModelUnavailable);
    };

    // 0024: هوية **وجهة** التنفيذ تُثبَّت الآن، قبل أي أثر جانبي. العيبة التي
    // أُغلقت: كان الربط على معرّف النموذج وحده، فتبديل الموفر بعد الاعتماد إلى
    // وجهة تعلن النموذج نفسه كان يمرّ بلا اعتراض.
    let provider_identity = ai_provider_registry_service::resolve_active_provider_identity(state)?;

    // 1) إنشاء التشغيل عبر المسار القائم (تدقيق AGENT_RUN_CREATED ذرياً).
    let created = agent_run_service::create_agent_run(
        state,
        agent_run_service::CreateAgentRunInput {
            agent_id: input.agent_id.clone(),
        },
    )?;

    // 2) ربط هوية التنفيذ الثابت (يصمد عبر إعادة تشغيل التطبيق) + تدقيق الطلب.
    //    الربط يشمل الآن: الوجهة + النموذج + نسخة البروفايل + هاش المهمة +
    //    مراجعة التشغيل، وبصمةً واحدة تجمعها فيُكشف أي تغيّر لاحق.
    let now = Utc::now().to_rfc3339();
    let task_hash = format!("{:x}", Sha256::digest(input.task_content.trim().as_bytes()));
    let execution_profile_version_id = latest_profile_version_id(state, &created.run.agent_id)?;
    let approval_binding_hash = provider_identity.approval_binding_hash(
        configured_model.as_str(),
        &execution_profile_version_id,
        &task_hash,
        created.run.revision,
    );
    let binding = AgentRunExecutionBindingRecord {
        agent_run_id: created.run.id.clone(),
        tenant_id: state.tenant_id(),
        agent_run_revision: created.run.revision,
        agent_id: created.run.agent_id.clone(),
        execution_profile_version_id,
        task_content: input.task_content.trim().to_owned(),
        task_content_hash: task_hash,
        model_id_at_request: Some(configured_model.as_str().to_owned()),
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
        append_execution_request_audit(
            &transaction,
            state,
            "AGENT_EXECUTION_REQUESTED",
            &created.run.id,
            &now,
        )?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
    }

    // 3) ربط استرجاع المعرفة (MP04): الاستعلام اختياري وصريح — الاسترجاع يحدث
    //    الآن وقبل الموافقة حتى تغطي الموافقة هوية الاسترجاع الدقيقة (استعلام
    //    + قطع). لا نتائج ⇒ لا ربط ولا إخفاء فشل الاسترجاع.
    if let Some(knowledge_query) = input.knowledge_query.as_deref() {
        bind_knowledge_retrieval(state, &created.run, knowledge_query)?;
    }

    // 4) طلب الموافقة المرتبط (باختيار Skill الموثوق عند وجوده) — الانتقال
    //    إلى WAITING_APPROVAL يحدث هنا قبل أي تنفيذ.
    let selection = match input.skill_version_id.as_deref() {
        Some(skill_version_id) => {
            let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
            let transaction = connection
                .unchecked_transaction()
                .map_err(|_| AppError::StorageOperation)?;
            let resolved =
                crate::application::skill_selection_resolver::SkillSelectionResolver::resolve(
                    &transaction,
                    state,
                    &created.run.agent_id,
                    Some(skill_version_id),
                )?;
            transaction
                .commit()
                .map_err(|_| AppError::StorageOperation)?;
            resolved
        }
        None => None,
    };
    let approval = request_agent_run_approval_with_skill(
        state,
        &created.run.id,
        AgentRunApprovalRequirement {
            approval_requirement_code: AGENT_EXECUTION_REQUIREMENT_CODE.to_owned(),
            required_role_code: AGENT_EXECUTION_REQUIRED_ROLE.to_owned(),
        },
        selection,
    )?;

    Ok(AgentExecutionRequestResult {
        run: approval.run,
        approval_id: approval.approval_id,
    })
}

pub fn get_agent_run_execution_binding(
    state: &AppState,
    run_id: String,
) -> Result<AgentRunExecutionBindingView, AppError> {
    authorization_service::require_permission(state, "AGENT", "READ")?;
    if run_id.trim().is_empty() || run_id.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let binding =
        AgentRunExecutionBindingRepository::find(&connection, &state.tenant_id(), &run_id)?
            .ok_or(AppError::ResourceNotFound)?;
    Ok(AgentRunExecutionBindingView {
        agent_run_id: binding.agent_run_id,
        agent_run_revision: binding.agent_run_revision,
        execution_profile_version_id: binding.execution_profile_version_id,
        task_content_hash: binding.task_content_hash,
        model_id_at_request: binding.model_id_at_request,
        provider_id_at_request: binding.provider_id_at_request,
        provider_kind_at_request: binding.provider_kind_at_request,
        endpoint_identity_at_request: binding.endpoint_identity_at_request,
        approval_binding_hash: binding.approval_binding_hash,
        bound_at: binding.bound_at,
    })
}

// ربط الاسترجاع بهوية التشغيل: استرجاع فعلي عبر محرك السياق ثم ربط ثابت
// للقطع المسترجعة (معاملة قصيرة + تدقيق) — الموافقة اللاحقة تغطي هذه الهوية.
fn bind_knowledge_retrieval(
    state: &AppState,
    run: &AgentRunRecord,
    knowledge_query: &str,
) -> Result<(), AppError> {
    let query = knowledge_query.trim();
    if query.is_empty() || query.chars().count() > crate::domain::knowledge::MAX_QUERY_CHARS {
        return Err(AppError::ValidationFailed);
    }
    let retriever = crate::application::knowledge_service::FtsKnowledgeRetriever;
    let items =
        crate::application::context_engine::KnowledgeRetriever::retrieve(&retriever, state, query)?;
    if items.is_empty() {
        // لا نربط هوية فارغة — فشل مغلق صريح لا تشغيل RAG زائف (§26)
        return Err(AppError::KnowledgeRetrievalFailed);
    }
    let chunk_ids: Vec<String> = items.iter().map(|item| item.chunk_id.clone()).collect();
    let now = Utc::now().to_rfc3339();
    let binding = crate::infrastructure::knowledge_retrieval_binding_repository::KnowledgeRetrievalBindingRecord {
        agent_run_id: run.id.clone(),
        tenant_id: state.tenant_id(),
        agent_run_revision: run.revision,
        query: query.to_owned(),
        query_hash: crate::application::knowledge_service::query_hash(query),
        chunk_ids_json: serde_json::to_string(&chunk_ids)
            .map_err(|_| AppError::StorageOperation)?,
        bound_at: now.clone(),
    };
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    crate::infrastructure::knowledge_retrieval_binding_repository::KnowledgeRetrievalBindingRepository::insert(
        &transaction, &binding,
    )?;
    append_execution_request_audit(
        &transaction,
        state,
        "AGENT_RUN_KNOWLEDGE_BOUND",
        &run.id,
        &now,
    )?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

fn latest_profile_version_id(state: &AppState, agent_id: &str) -> Result<String, AppError> {
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let agent = AgentRepository::find(&transaction, &state.tenant_id(), agent_id)?
        .ok_or(AppError::ResourceNotFound)?;
    if agent.status != AgentStatus::Active {
        return Err(AppError::AgentDisabled);
    }
    let latest =
        AgentExecutionProfileRepository::latest(&transaction, &state.tenant_id(), agent_id)?
            .ok_or(AppError::ResourceNotFound)?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;
    Ok(latest.id)
}

fn append_execution_request_audit(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    event_type: &str,
    run_id: &str,
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
        "AgentRun",
        run_id,
        occurred_at,
        &previous_hash,
    )
    .map(|_| ())
}
