use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    application::{authorization_service, provider_runtime_service::ProviderRuntimeService},
    domain::{
        error::AppError,
        plan::{
            validate_proposed_steps, AgentDelegationRecord, PlanRecord, PlanStatus,
            ProposedPlanStep, MAX_PLAN_GOAL_CHARS,
        },
        provider::{ModelProvider, RuntimeExecutionInput},
    },
    infrastructure::{
        approval_repository::{ApprovalRecord, ApprovalRepository},
        database::{append_audit_event, latest_audit_event_hash, AppState},
        plan_repository::{PlanInsert, PlanRepository, PlanStepInsert},
        verification_repository::{VerificationRepository, VerificationRunRecord},
    },
};

// موافقة الخطة: مورد مستقل برمز ثابت خلفي — لا يبدأ أي تفويض قبل ACTIVE (C-10)
pub const PLAN_EXECUTION_REQUIREMENT_CODE: &str = "PLAN_EXECUTION";
pub const PLAN_EXECUTION_REQUIRED_ROLE: &str = "TENANT_ADMIN";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePlanInput {
    pub goal: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePlanResult {
    pub plan_id: String,
    pub approval_id: String,
    pub steps: Vec<ProposedPlanStep>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelPlanInput {
    pub plan_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlansData {
    pub items: Vec<PlanRecord>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DelegationWithVerification {
    pub delegation: AgentDelegationRecord,
    pub verification: Option<VerificationRunRecord>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanDetails {
    pub plan: PlanRecord,
    pub steps: Vec<crate::domain::plan::PlanStepRecord>,
    pub delegations: Vec<DelegationWithVerification>,
    pub pending_approval_id: Option<String>,
}

// إنشاء خطة محكومة: الهدف من المستخدم، الخطوات من النموذج المحلي عبر مسار
// JSON مهيكل، والتحقق من المخطط المغلق حتمياً — أي إخراج ملف يُرفض بعد
// محاولة إصلاح واحدة حداً (C-10/§94-95).
pub fn create_plan(state: &AppState, input: CreatePlanInput) -> Result<CreatePlanResult, AppError> {
    let provider = ProviderRuntimeService::local_ollama();
    create_plan_with_provider(state, input, &provider)
}

pub(crate) fn create_plan_with_provider(
    state: &AppState,
    input: CreatePlanInput,
    provider: &dyn ModelProvider,
) -> Result<CreatePlanResult, AppError> {
    authorization_service::require_permission(state, "PLAN", "CREATE")?;
    let goal = input.goal.trim();
    if goal.is_empty() || goal.chars().count() > MAX_PLAN_GOAL_CHARS {
        return Err(AppError::PlanInvalid);
    }

    // النموذج المفعّل جزء من هوية الخطة (نفس قاعدة MP03: واحد بالضبط)
    let model_id = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        let enabled = crate::infrastructure::model_registry_repository::ModelRegistryRepository::list_enabled_local_models(
            &connection,
            &state.tenant_id(),
        )?;
        match enabled.as_slice() {
            [model_id] => model_id.clone(),
            _ => return Err(AppError::ModelUnavailable),
        }
    };

    // نداء المخطط المهيكل + محاولة إصلاح واحدة عند فشل التحليل (لا حلقة إصلاح)
    let prompt = planner_prompt(goal);
    let runtime_input = RuntimeExecutionInput::new(prompt).map_err(|_| AppError::PlanInvalid)?;
    let mut steps = match provider.generate_structured(&model_id, &runtime_input) {
        Ok(generation) => parse_proposed_steps(&generation.value),
        Err(_) => None,
    };
    if steps.is_none() {
        let repair_prompt = planner_repair_prompt(goal);
        let repair_input =
            RuntimeExecutionInput::new(repair_prompt).map_err(|_| AppError::PlanInvalid)?;
        steps = match provider.generate_structured(&model_id, &repair_input) {
            Ok(generation) => parse_proposed_steps(&generation.value),
            Err(_) => None,
        };
    }
    let steps = steps.ok_or(AppError::PlanInvalid)?;
    validate_proposed_steps(&steps)?;

    // هاش الخطوات المقنن — هوية الموافقة تغطي الخطوات بالضبط
    let steps_hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_string(&steps).unwrap_or_default().as_bytes())
    );

    let now = Utc::now().to_rfc3339();
    let plan_id = format!("plan_{}", Uuid::new_v4());
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    PlanRepository::insert_plan(
        &transaction,
        &state.tenant_id(),
        &PlanInsert {
            id: &plan_id,
            goal,
            steps_hash: &steps_hash,
            model_id_at_request: Some(model_id.as_str()),
            requested_by_user_id: &state.current_user_id()?,
        },
        &now,
    )?;
    for (step_index, step) in steps.iter().enumerate() {
        PlanRepository::insert_step(
            &transaction,
            &state.tenant_id(),
            &PlanStepInsert {
                id: &format!("plan_step_{}", Uuid::new_v4()),
                plan_id: &plan_id,
                // الفهرس هو موضع الخطوة في الإخراج المقبول: البحث بالقيمة يمنح الخطوتين
                // المتطابقتين فهرساً واحداً فيخالف قيد (tenant_id, plan_id, step_index).
                step_index,
                objective: &step.objective,
                required_capability: &step.required_capability,
                requires_knowledge: step.requires_knowledge,
                evidence_requirement: &step.evidence_requirement,
            },
            &now,
        )?;
    }
    append_plan_audit(&transaction, state, "PLAN_CREATED", &plan_id, &now)?;

    // طلب موافقة مربوط بالهوية الدقيقة (مراجعة الخطة + هاش الخطوات + النموذج)
    let approval = ApprovalRecord {
        id: format!("approval_{}", Uuid::new_v4()),
        tenant_id: state.tenant_id(),
        resource_type: "PLAN".to_owned(),
        resource_id: plan_id.clone(),
        resource_version: 1,
        requested_by: state.current_user_id()?,
        required_role_code: PLAN_EXECUTION_REQUIRED_ROLE.to_owned(),
        approval_requirement_code: Some(PLAN_EXECUTION_REQUIREMENT_CODE.to_owned()),
        decision: "PENDING".to_owned(),
    };
    ApprovalRepository::insert_plan_request(&transaction, &approval, &now)?;
    append_plan_audit(
        &transaction,
        state,
        "PLAN_APPROVAL_REQUESTED",
        &plan_id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(CreatePlanResult {
        plan_id,
        approval_id: approval.id,
        steps,
    })
}

pub fn list_plans(state: &AppState) -> Result<PlansData, AppError> {
    authorization_service::require_permission(state, "PLAN", "READ")?;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let items = PlanRepository::list_plans(&connection, &state.tenant_id(), 50)?;
    Ok(PlansData { items })
}

pub fn get_plan(state: &AppState, plan_id: String) -> Result<PlanDetails, AppError> {
    authorization_service::require_permission(state, "PLAN", "READ")?;
    let plan_id = plan_id.trim();
    if plan_id.is_empty() || plan_id.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let plan = PlanRepository::find_plan(&connection, &state.tenant_id(), plan_id)?
        .ok_or(AppError::ResourceNotFound)?;
    let pending_approval_id = {
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        let pending = ApprovalRepository::find_pending_plan_for_plan(
            &transaction,
            &state.tenant_id(),
            plan_id,
        )?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
        pending.map(|approval| approval.id)
    };
    let steps = PlanRepository::list_steps(&connection, &state.tenant_id(), plan_id)?;
    let delegation_records =
        PlanRepository::list_delegations_for_plan(&connection, &state.tenant_id(), plan_id)?;
    let mut delegations = Vec::with_capacity(delegation_records.len());
    for delegation in delegation_records {
        let verification = match delegation.agent_run_id.as_deref() {
            Some(run_id) => VerificationRepository::list_latest_for_agent_run(
                &connection,
                &state.tenant_id(),
                run_id,
            )?,
            None => None,
        };
        delegations.push(DelegationWithVerification {
            delegation,
            verification,
        });
    }
    Ok(PlanDetails {
        plan,
        steps,
        delegations,
        pending_approval_id,
    })
}

// انتقال ما بعد قرار الموافقة — يُستدعى من خدمة الاعتمادات داخل معاملتها
pub fn transition_after_plan_decision(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    approval: &ApprovalRecord,
    decided_at: &str,
) -> Result<(), AppError> {
    let plan = PlanRepository::find_plan_in_transaction(
        &transaction,
        &state.tenant_id(),
        &approval.resource_id,
    )?
    .ok_or(AppError::StaleApproval)?;
    if approval.resource_type != "PLAN"
        || approval.resource_version != plan.revision
        || plan.status != PlanStatus::PendingApproval
    {
        return Err(AppError::StaleApproval);
    }
    let (next_status, event) = match approval.decision.as_str() {
        "APPROVED" => (PlanStatus::Active, "PLAN_APPROVED"),
        "REJECTED" => (PlanStatus::Rejected, "PLAN_REJECTED"),
        _ => return Err(AppError::ValidationFailed),
    };
    let changed = PlanRepository::transition_plan(
        &transaction,
        &state.tenant_id(),
        &plan.id,
        plan.status,
        plan.revision,
        next_status,
        plan.revision + 1,
        Some(decided_at),
        decided_at,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    append_plan_audit(&transaction, state, event, &plan.id, decided_at)
}

// إلغاء صاحب الطلب قبل بدء التنفيذ فقط — لا إلغاء أثناء RUNNING (فشل مغلق)
pub fn cancel_plan(state: &AppState, plan_id: &str) -> Result<(), AppError> {
    authorization_service::require_permission(state, "PLAN", "CREATE")?;
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let plan = PlanRepository::find_plan_in_transaction(&transaction, &state.tenant_id(), plan_id)?
        .ok_or(AppError::ResourceNotFound)?;
    if plan.requested_by_user_id != state.current_user_id()? {
        return Err(AppError::AuthorizationDenied);
    }
    if plan.status == PlanStatus::PendingApproval {
        // إلغاء الموافقة المنتظرة أولاً (نمط إلغاء تشغيلات MP03)
        ApprovalRepository::cancel_pending_for_resource(
            &transaction,
            &state.tenant_id(),
            "PLAN",
            &plan.id,
            &state.current_user_id()?,
            &now,
        )?;
    }
    if !matches!(
        plan.status,
        PlanStatus::PendingApproval | PlanStatus::Active
    ) {
        return Err(AppError::InvalidStateTransition);
    }
    let changed = PlanRepository::transition_plan(
        &transaction,
        &state.tenant_id(),
        &plan.id,
        plan.status,
        plan.revision,
        PlanStatus::Cancelled,
        plan.revision + 1,
        Some(&now),
        &now,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    append_plan_audit(&transaction, state, "PLAN_CANCELLED", &plan.id, &now)?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

// تحليل إخراج النموذج إلى خطوات مقترحة — مخطط مغلق لا يقبل أي قدرة حرة
fn parse_proposed_steps(value: &serde_json::Value) -> Option<Vec<ProposedPlanStep>> {
    #[derive(Deserialize)]
    struct RawStep {
        objective: String,
        #[serde(rename = "capability")]
        capability: String,
        #[serde(rename = "requiresKnowledge", default)]
        requires_knowledge: bool,
        #[serde(rename = "evidenceRequirement")]
        evidence_requirement: String,
    }
    #[derive(Deserialize)]
    struct RawPlan {
        #[serde(default)]
        steps: Vec<RawStep>,
    }
    let raw: RawPlan = serde_json::from_value(value.clone()).ok()?;
    if raw.steps.is_empty() {
        return None;
    }
    let steps = raw
        .steps
        .into_iter()
        .map(|step| ProposedPlanStep {
            objective: step.objective,
            required_capability: step.capability,
            requires_knowledge: step.requires_knowledge,
            evidence_requirement: step.evidence_requirement,
        })
        .collect();
    Some(steps)
}

fn planner_prompt(goal: &str) -> String {
    format!(
        "You are the NEXUS bounded planner. Produce a JSON object only.\n\
         [PLATFORM_SECURITY]\nExecution boundaries: tools only through backend governance and exact authorization; model output is not verified truth.\n\
         [PLATFORM_POLICY]\nBackend policy and approval remain authoritative; you propose work only.\n\
         [GOAL]\n{goal}\n\
         [OUTPUT_CONTRACT]\nReturn JSON: {{\"steps\":[{{\"objective\":\"short objective\",\"capability\":\"RESEARCH|COMPLIANCE|RISK|EVIDENCE|SYNTHESIS\",\"requiresKnowledge\":true|false,\"evidenceRequirement\":\"what evidence the step must cite\"}}]}} with 1 to 4 steps. No tool names, no agent ids, no permissions."
    )
}

fn planner_repair_prompt(goal: &str) -> String {
    // مثال عملي واحد مضغوط: يرفع التزام النماذج الصغيرة بعقد JSON حرفياً (§94)
    format!(
        "Your previous output was not valid for the contract. Return ONLY minified JSON matching exactly this shape: {{\"steps\":[{{\"objective\":string,\"capability\":\"RESEARCH\"|\"COMPLIANCE\"|\"RISK\"|\"EVIDENCE\"|\"SYNTHESIS\",\"requiresKnowledge\":boolean,\"evidenceRequirement\":string}}]}} with 1-4 steps for this goal: {goal}
Example of a valid output: {{\"steps\":[{{\"objective\":\"Review the retention policy\",\"capability\":\"COMPLIANCE\",\"requiresKnowledge\":true,\"evidenceRequirement\":\"Cite the ingested policy citations\"}}]}}"
    )
}

fn append_plan_audit(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    event_type: &str,
    plan_id: &str,
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
        "Plan",
        plan_id,
        occurred_at,
        &previous_hash,
    )
    .map(|_| ())
}
