use tauri::State;

use crate::{
    application::{
        agent_execution_profile_service, agent_execution_request_service, agent_run_service,
        agent_runtime_coordinator, agent_service, ai_provider_registry_service, approval_service,
        attention_service, audit_service, authentication_service, backup_service,
        bootstrap_options_service, control_service, dashboard_service, demo_identity_service,
        evidence_service, health_service, integration_status_service, knowledge_service,
        memory_service, model_configuration_service, module_service, obligation_service,
        organizational_intelligence_service, planner_service, regulatory_source_service,
        report_service, risk_service, skill_service, supervisor_service, task_service,
        tool_invocation_service, tool_service, user_administration_service, verifier_service,
    },
    domain::{
        error::{AppError, SafeError},
        models::{
            AgentRunRecord, AgentRunsData, AgentsData, AiProviderCheck, ArchivedObligation,
            AttentionData, ControlsData, DashboardData, EvidenceData, HealthStatus,
            IntegrationStatusData, ObligationDetails, ObligationsData,
            OrganizationalIntelligenceData, RegulatorySourcesData, RisksData, SessionContext,
            TaskTransitionRecord, TaskTransitionResult, TasksData, UpdatedObligation,
            VerifiedObligation,
        },
    },
    infrastructure::database::AppState,
};

fn to_safe_error(error: AppError) -> SafeError {
    error.into()
}

// =====================================================================
// MASTER PHASE 05 — الهوية الإنتاجية والإدارة والنسخ الاحتياطي
// أوامر الإقلاع/الدخول بلا هوية سابقة (نطاقها ضيق ومقيد بالوضع في الخلف)؛
// ما عداها يمر ببوابة التفويض الصارمة داخل الخدمات.
// =====================================================================

#[tauri::command]
pub fn get_auth_status(
    state: State<'_, AppState>,
) -> Result<authentication_service::AuthStatusData, SafeError> {
    authentication_service::get_auth_status(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn start_demo_experience(
    state: State<'_, AppState>,
) -> Result<authentication_service::AuthenticatedSession, SafeError> {
    authentication_service::start_demo_experience(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn bootstrap_production(
    state: State<'_, AppState>,
    input: authentication_service::BootstrapProductionInput,
) -> Result<authentication_service::AuthenticatedSession, SafeError> {
    authentication_service::bootstrap_production(&state, input).map_err(to_safe_error)
}

// GAP-05-OPEN: كتالوج خيارات شاشة التأسيس (الدول المعتمدة) — قراءة فقط،
// وبدون صلاحية سابقة لأنه لا يُستدعى إلا في وضع UNPROVISIONED قبل أي هوية.
#[tauri::command]
pub fn get_bootstrap_options(
    state: State<'_, AppState>,
) -> Result<bootstrap_options_service::BootstrapOptionsData, SafeError> {
    bootstrap_options_service::get_bootstrap_options(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn login(
    state: State<'_, AppState>,
    input: authentication_service::LoginInput,
) -> Result<authentication_service::AuthenticatedSession, SafeError> {
    authentication_service::login(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn logout(state: State<'_, AppState>) -> Result<(), SafeError> {
    authentication_service::logout(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn list_workspace_users(
    state: State<'_, AppState>,
) -> Result<user_administration_service::WorkspaceUsersData, SafeError> {
    user_administration_service::list_workspace_users(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn set_user_status(
    state: State<'_, AppState>,
    input: user_administration_service::SetUserStatusInput,
) -> Result<user_administration_service::UserMutationResult, SafeError> {
    user_administration_service::set_user_status(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn assign_user_role(
    state: State<'_, AppState>,
    input: user_administration_service::UserRoleInput,
) -> Result<user_administration_service::UserMutationResult, SafeError> {
    user_administration_service::assign_user_role(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn revoke_user_role(
    state: State<'_, AppState>,
    input: user_administration_service::UserRoleInput,
) -> Result<user_administration_service::UserMutationResult, SafeError> {
    user_administration_service::revoke_user_role(&state, input).map_err(to_safe_error)
}

// ===== 0026: إدارة المستخدمين الإنتاجية وكلمة المرور =====
// الهوية والمستأجر من الجلسة الموثوقة حصراً؛ لا يقبل أي أمر منهما tenantId ولا هوية.

#[tauri::command]
pub fn create_workspace_user(
    state: State<'_, AppState>,
    input: user_administration_service::CreateWorkspaceUserInput,
) -> Result<user_administration_service::CreatedWorkspaceUser, SafeError> {
    user_administration_service::create_workspace_user(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn revoke_user_sessions(
    state: State<'_, AppState>,
    input: user_administration_service::RevokeUserSessionsInput,
) -> Result<user_administration_service::RevokedUserSessions, SafeError> {
    user_administration_service::revoke_user_sessions(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn change_password(
    state: State<'_, AppState>,
    input: authentication_service::ChangePasswordInput,
) -> Result<authentication_service::ChangedPassword, SafeError> {
    authentication_service::change_password(&state, input).map_err(to_safe_error)
}

// ===== 0026: تأليف المصادر التنظيمية والالتزامات في وضع الإنتاج =====
// لا يقبل أي منهما مستأجراً ولا منشأً ولا حالة تحقق من العميل: كلها قرارات خلفية.

#[tauri::command]
pub fn create_regulatory_source(
    state: State<'_, AppState>,
    input: regulatory_source_service::CreateRegulatorySourceInput,
) -> Result<regulatory_source_service::RegulatorySourceMutationResult, SafeError> {
    regulatory_source_service::create_regulatory_source(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn create_obligation(
    state: State<'_, AppState>,
    input: obligation_service::CreateObligationInput,
) -> Result<crate::domain::models::CreatedObligation, SafeError> {
    obligation_service::create_obligation(&state, input).map_err(to_safe_error)
}

// تحقق فعلي من سلسلة التدقيق: إعادة تشغيل SHA-256 على كل حدث، لا علامة ثابتة.
#[tauri::command]
pub fn verify_audit_chain(
    state: State<'_, AppState>,
) -> Result<audit_service::AuditChainVerificationResult, SafeError> {
    audit_service::verify_audit_chain_report(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn create_backup(
    state: State<'_, AppState>,
    input: backup_service::CreateBackupInput,
) -> Result<backup_service::BackupResult, SafeError> {
    backup_service::create_backup(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn restore_backup(
    state: State<'_, AppState>,
    input: backup_service::RestoreBackupInput,
) -> Result<backup_service::RestoreResult, SafeError> {
    backup_service::restore_backup(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn list_backups(
    state: State<'_, AppState>,
    input: backup_service::CreateBackupInput,
) -> Result<Vec<backup_service::BackupManifest>, SafeError> {
    backup_service::list_backups(&state, input.target_directory).map_err(to_safe_error)
}

// فحص المزود الحي الصريح (C10): إجراء مستخدم بمهلة قصيرة — لا يُنفّذ في كل استعلام صحة.
// التفويض مفروض داخل الخدمة (INTEGRATION/READ) فلا يمنح الأمر أي سلطة بذاته.
#[tauri::command]
pub fn check_ai_provider(state: State<'_, AppState>) -> Result<AiProviderCheck, SafeError> {
    health_service::check_ai_provider(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_agent_runs(state: State<'_, AppState>) -> Result<AgentRunsData, SafeError> {
    agent_run_service::list_agent_runs(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_agent_run(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<AgentRunRecord, SafeError> {
    agent_run_service::get_agent_run(&state, run_id).map_err(to_safe_error)
}

#[tauri::command]
pub fn create_agent_run(
    state: State<'_, AppState>,
    input: agent_run_service::CreateAgentRunInput,
) -> Result<agent_run_service::AgentRunMutationResult, SafeError> {
    agent_run_service::create_agent_run(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn cancel_agent_run(
    state: State<'_, AppState>,
    input: agent_run_service::CancelAgentRunInput,
) -> Result<agent_run_service::AgentRunMutationResult, SafeError> {
    agent_run_service::cancel_agent_run(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_agent_run_result(
    state: State<'_, AppState>,
    input: agent_run_service::CancelAgentRunInput,
) -> Result<crate::domain::models::AgentRunResultData, SafeError> {
    agent_run_service::get_agent_run_result(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_agents(state: State<'_, AppState>) -> Result<AgentsData, SafeError> {
    agent_service::list_agents(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn create_agent(
    state: State<'_, AppState>,
    input: agent_service::CreateAgentInput,
) -> Result<agent_service::AgentMutationResult, SafeError> {
    agent_service::create_agent(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn update_agent(
    state: State<'_, AppState>,
    input: agent_service::UpdateAgentInput,
) -> Result<agent_service::AgentMutationResult, SafeError> {
    agent_service::update_agent(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn set_agent_status(
    state: State<'_, AppState>,
    input: agent_service::SetAgentStatusInput,
) -> Result<agent_service::AgentMutationResult, SafeError> {
    agent_service::set_agent_status(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_dashboard(state: State<'_, AppState>) -> Result<DashboardData, SafeError> {
    dashboard_service::load_dashboard(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_attention(state: State<'_, AppState>) -> Result<AttentionData, SafeError> {
    attention_service::load_attention(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_obligations(state: State<'_, AppState>) -> Result<ObligationsData, SafeError> {
    obligation_service::list_obligations(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_obligation(
    state: State<'_, AppState>,
    obligation_id: String,
) -> Result<ObligationDetails, SafeError> {
    obligation_service::get_obligation(&state, &obligation_id).map_err(to_safe_error)
}

#[tauri::command]
pub fn update_obligation(
    state: State<'_, AppState>,
    input: obligation_service::UpdateObligationInput,
) -> Result<UpdatedObligation, SafeError> {
    obligation_service::update_obligation(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn archive_obligation(
    state: State<'_, AppState>,
    input: obligation_service::ArchiveObligationInput,
) -> Result<ArchivedObligation, SafeError> {
    obligation_service::archive_obligation(&state, input).map_err(to_safe_error)
}

// 0025: خطوة التحقق البشري — الصلاحية منفصلة والقرار في الخلف.
#[tauri::command]
pub fn verify_obligation(
    state: State<'_, AppState>,
    input: obligation_service::VerifyObligationInput,
) -> Result<VerifiedObligation, SafeError> {
    obligation_service::verify_obligation(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_system_health(state: State<'_, AppState>) -> Result<HealthStatus, SafeError> {
    health_service::get_health(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn create_remediation_task(
    state: State<'_, AppState>,
    input: task_service::CreateTaskInput,
) -> Result<task_service::CreatedTask, SafeError> {
    task_service::create_remediation_task(&state, input).map_err(to_safe_error)
}

// 0025: الإسناد ونقل الحالة وسجل الانتقالات — أوامر صريحة بدل تعديل حالة
// المهمة من الواجهة. كل قرار سلطة يُتخذ في الخلف.
#[tauri::command]
pub fn assign_task(
    state: State<'_, AppState>,
    input: task_service::AssignTaskInput,
) -> Result<TaskTransitionResult, SafeError> {
    task_service::assign_task(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn transition_task(
    state: State<'_, AppState>,
    input: task_service::TransitionTaskInput,
) -> Result<TaskTransitionResult, SafeError> {
    task_service::transition_task(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_task_history(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<Vec<TaskTransitionRecord>, SafeError> {
    task_service::load_task_history(&state, &task_id).map_err(to_safe_error)
}

#[tauri::command]
pub fn decide_approval(
    state: State<'_, AppState>,
    input: approval_service::DecideApprovalInput,
) -> Result<approval_service::ApprovalDecisionResult, SafeError> {
    approval_service::decide_approval(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_evidence(state: State<'_, AppState>) -> Result<EvidenceData, SafeError> {
    evidence_service::load_evidence(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn register_evidence_reference(
    state: State<'_, AppState>,
    input: evidence_service::RegisterEvidenceReferenceInput,
) -> Result<evidence_service::RegisteredEvidenceReference, SafeError> {
    evidence_service::register_evidence_reference(&state, input).map_err(to_safe_error)
}

// ===== دورة حياة الأدلة وتأليف الضوابط (0022) =====
// أوامر مطبوعة صغيرة: لا تقبل أي هوية/مستأجر/تصنيف/حقل ملكية — السلطة في الخلف حصراً.

#[tauri::command]
pub fn update_evidence_status(
    state: State<'_, AppState>,
    input: evidence_service::UpdateEvidenceStatusInput,
) -> Result<evidence_service::UpdatedEvidenceStatus, SafeError> {
    evidence_service::update_evidence_status(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_evidence_detail(
    state: State<'_, AppState>,
    evidence_id: String,
) -> Result<crate::domain::models::EvidenceRecord, SafeError> {
    evidence_service::get_evidence_detail(&state, evidence_id).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_control_detail(
    state: State<'_, AppState>,
    control_id: String,
) -> Result<crate::domain::models::ControlDetail, SafeError> {
    control_service::get_control_detail(&state, control_service::ControlIdInput { control_id })
        .map_err(to_safe_error)
}

#[tauri::command]
pub fn create_control(
    state: State<'_, AppState>,
    input: control_service::CreateControlInput,
) -> Result<control_service::ControlMutationResult, SafeError> {
    control_service::create_control(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn update_control(
    state: State<'_, AppState>,
    input: control_service::UpdateControlInput,
) -> Result<control_service::ControlMutationResult, SafeError> {
    control_service::update_control(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn disable_control(
    state: State<'_, AppState>,
    input: control_service::SetControlStatusInput,
) -> Result<control_service::ControlMutationResult, SafeError> {
    control_service::disable_control(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn record_control_assessment(
    state: State<'_, AppState>,
    input: control_service::RecordControlAssessmentInput,
) -> Result<control_service::ControlAssessmentResult, SafeError> {
    control_service::record_control_assessment(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn link_control_obligation(
    state: State<'_, AppState>,
    input: control_service::ControlObligationLinkInput,
) -> Result<control_service::ControlLinkMutationResult, SafeError> {
    control_service::link_control_obligation(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn unlink_control_obligation(
    state: State<'_, AppState>,
    input: control_service::ControlObligationLinkInput,
) -> Result<control_service::ControlLinkMutationResult, SafeError> {
    control_service::unlink_control_obligation(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn link_control_risk(
    state: State<'_, AppState>,
    input: control_service::ControlRiskLinkInput,
) -> Result<control_service::ControlLinkMutationResult, SafeError> {
    control_service::link_control_risk(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn unlink_control_risk(
    state: State<'_, AppState>,
    input: control_service::ControlRiskLinkInput,
) -> Result<control_service::ControlLinkMutationResult, SafeError> {
    control_service::unlink_control_risk(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn link_control_evidence(
    state: State<'_, AppState>,
    input: control_service::ControlEvidenceLinkInput,
) -> Result<control_service::ControlLinkMutationResult, SafeError> {
    control_service::link_control_evidence(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn unlink_control_evidence(
    state: State<'_, AppState>,
    input: control_service::ControlEvidenceLinkInput,
) -> Result<control_service::ControlLinkMutationResult, SafeError> {
    control_service::unlink_control_evidence(&state, input).map_err(to_safe_error)
}
#[tauri::command]
pub fn get_organizational_intelligence(
    state: State<'_, AppState>,
) -> Result<OrganizationalIntelligenceData, SafeError> {
    organizational_intelligence_service::load_organizational_intelligence(&state)
        .map_err(to_safe_error)
}

#[tauri::command]
pub fn get_regulatory_sources(
    state: State<'_, AppState>,
) -> Result<RegulatorySourcesData, SafeError> {
    regulatory_source_service::load_regulatory_sources(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_audit_trail(
    state: State<'_, AppState>,
    input: audit_service::AuditTrailPageInput,
) -> Result<crate::domain::models::AuditTrailData, SafeError> {
    audit_service::list_audit_events(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_integration_status(
    state: State<'_, AppState>,
) -> Result<IntegrationStatusData, SafeError> {
    integration_status_service::load_integration_status(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_session_context(state: State<'_, AppState>) -> Result<SessionContext, SafeError> {
    demo_identity_service::get_session_context(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_risks(state: State<'_, AppState>) -> Result<RisksData, SafeError> {
    risk_service::load_risks(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_tasks(state: State<'_, AppState>) -> Result<TasksData, SafeError> {
    task_service::load_tasks(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_controls(state: State<'_, AppState>) -> Result<ControlsData, SafeError> {
    control_service::load_controls(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_module_availability(
    state: State<'_, AppState>,
) -> Result<Vec<module_service::ModuleAvailability>, SafeError> {
    module_service::get_module_availability(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_demo_actors(
    state: State<'_, AppState>,
) -> Result<demo_identity_service::DemoActorsData, SafeError> {
    demo_identity_service::list_demo_actors(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn switch_demo_actor(
    state: State<'_, AppState>,
    input: demo_identity_service::SwitchDemoActorInput,
) -> Result<demo_identity_service::DemoActorsData, SafeError> {
    demo_identity_service::switch_demo_actor(&state, input).map_err(to_safe_error)
}

// =====================================================================
// MASTER PHASE 03 — GOVERNED AGENT & TOOL RUNTIME
// أوامر مطبوعة صغيرة: لا يقبل أي أمن منها هوية/دوراً/صلاحيات/عنوان مزود.
// =====================================================================

#[tauri::command]
pub fn get_agent_execution_profiles(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<agent_execution_profile_service::AgentExecutionProfilesData, SafeError> {
    agent_execution_profile_service::list_agent_execution_profiles(&state, agent_id)
        .map_err(to_safe_error)
}

#[tauri::command]
pub fn create_agent_execution_profile(
    state: State<'_, AppState>,
    input: agent_execution_profile_service::CreateAgentExecutionProfileInput,
) -> Result<agent_execution_profile_service::AgentExecutionProfileMutationResult, SafeError> {
    agent_execution_profile_service::create_agent_execution_profile(&state, input)
        .map_err(to_safe_error)
}

#[tauri::command]
pub fn request_agent_execution(
    state: State<'_, AppState>,
    input: agent_execution_request_service::RequestAgentExecutionInput,
) -> Result<agent_execution_request_service::AgentExecutionRequestResult, SafeError> {
    agent_execution_request_service::request_agent_execution(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn start_queued_execution(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<agent_runtime_coordinator::StartExecutionOutcome, SafeError> {
    agent_runtime_coordinator::start_queued_execution(&state, &run_id).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_agent_run_execution_binding(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<agent_execution_request_service::AgentRunExecutionBindingView, SafeError> {
    agent_execution_request_service::get_agent_run_execution_binding(&state, run_id)
        .map_err(to_safe_error)
}

#[tauri::command]
pub fn get_tool_invocations_for_run(
    state: State<'_, AppState>,
    run_id: String,
) -> Result<tool_invocation_service::ToolInvocationsData, SafeError> {
    tool_invocation_service::list_tool_invocations_for_run(&state, run_id).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_skill_catalog(
    state: State<'_, AppState>,
) -> Result<skill_service::SkillCatalogData, SafeError> {
    skill_service::get_skill_catalog(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn create_skill(
    state: State<'_, AppState>,
    input: skill_service::CreateSkillInput,
) -> Result<skill_service::SkillMutationResult, SafeError> {
    skill_service::create_skill(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn create_skill_version(
    state: State<'_, AppState>,
    input: skill_service::CreateSkillVersionInput,
) -> Result<skill_service::SkillVersionMutationResult, SafeError> {
    skill_service::create_skill_version(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn disable_skill(
    state: State<'_, AppState>,
    input: skill_service::SkillIdInput,
) -> Result<skill_service::SkillMutationResult, SafeError> {
    skill_service::disable_skill(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn assign_skill(
    state: State<'_, AppState>,
    input: skill_service::AssignSkillInput,
) -> Result<(), SafeError> {
    skill_service::assign_skill(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn unassign_skill(
    state: State<'_, AppState>,
    input: skill_service::UnassignSkillInput,
) -> Result<(), SafeError> {
    skill_service::unassign_skill(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_tool_catalog(
    state: State<'_, AppState>,
) -> Result<tool_service::ToolCatalogData, SafeError> {
    tool_service::get_tool_catalog(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn create_tool(
    state: State<'_, AppState>,
    input: tool_service::CreateToolInput,
) -> Result<tool_service::ToolMutationResult, SafeError> {
    tool_service::create_tool(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn create_tool_version(
    state: State<'_, AppState>,
    input: tool_service::CreateToolVersionInput,
) -> Result<tool_service::ToolVersionMutationResult, SafeError> {
    tool_service::create_tool_version(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn disable_tool(
    state: State<'_, AppState>,
    input: tool_service::ToolIdInput,
) -> Result<tool_service::ToolMutationResult, SafeError> {
    tool_service::disable_tool(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn assign_tool(
    state: State<'_, AppState>,
    input: tool_service::AssignToolInput,
) -> Result<tool_service::ToolAssignmentMutationResult, SafeError> {
    tool_service::assign_tool(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn unassign_tool(
    state: State<'_, AppState>,
    input: tool_service::ToolAssignmentIdInput,
) -> Result<(), SafeError> {
    tool_service::unassign_tool(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_agent_tool_assignments(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<tool_service::AgentToolAssignmentsData, SafeError> {
    tool_service::list_agent_tool_assignments(&state, agent_id).map_err(to_safe_error)
}

#[tauri::command]
pub fn configure_local_model(
    state: State<'_, AppState>,
    input: model_configuration_service::ConfigureLocalModelInput,
) -> Result<model_configuration_service::LocalModelConfigurationResult, SafeError> {
    model_configuration_service::configure_local_model(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn clear_local_model_configuration(
    state: State<'_, AppState>,
) -> Result<model_configuration_service::LocalModelConfigurationResult, SafeError> {
    model_configuration_service::clear_local_model_configuration(&state).map_err(to_safe_error)
}

// =====================================================================
// MASTER PHASE 04 — INTELLIGENCE / KNOWLEDGE / MEMORY / PLANS
// أوامر مطبوعة صغيرة: لا يقبل أي أمن منها هوية/دوراً/صلاحية/سلطة.
// =====================================================================

#[tauri::command]
pub fn ingest_knowledge_source(
    state: State<'_, AppState>,
    input: knowledge_service::IngestKnowledgeSourceInput,
) -> Result<knowledge_service::IngestKnowledgeSourceResult, SafeError> {
    knowledge_service::ingest_user_document(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_knowledge_sources(
    state: State<'_, AppState>,
) -> Result<knowledge_service::KnowledgeSourcesData, SafeError> {
    knowledge_service::list_sources(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_knowledge_source(
    state: State<'_, AppState>,
    source_id: String,
) -> Result<knowledge_service::KnowledgeSourceDetails, SafeError> {
    knowledge_service::get_source(&state, source_id).map_err(to_safe_error)
}

#[tauri::command]
pub fn search_knowledge(
    state: State<'_, AppState>,
    input: knowledge_service::SearchKnowledgeInput,
) -> Result<knowledge_service::KnowledgeSearchResultData, SafeError> {
    knowledge_service::search(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn disable_knowledge_source(
    state: State<'_, AppState>,
    input: knowledge_service::DisableKnowledgeSourceInput,
) -> Result<(), SafeError> {
    knowledge_service::disable_source(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_memory(state: State<'_, AppState>) -> Result<memory_service::MemoryData, SafeError> {
    memory_service::list_memory(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn disable_memory_item(
    state: State<'_, AppState>,
    input: memory_service::DisableMemoryItemInput,
) -> Result<(), SafeError> {
    memory_service::disable_memory_item(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn propose_semantic_memory(
    state: State<'_, AppState>,
    input: memory_service::ProposeSemanticMemoryInput,
) -> Result<memory_service::ProposeSemanticMemoryResult, SafeError> {
    memory_service::propose_semantic(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn promote_memory_item(
    state: State<'_, AppState>,
    input: memory_service::PromoteMemoryItemInput,
) -> Result<(), SafeError> {
    memory_service::promote_memory_item(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn create_plan(
    state: State<'_, AppState>,
    input: planner_service::CreatePlanInput,
) -> Result<planner_service::CreatePlanResult, SafeError> {
    planner_service::create_plan(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_plans(state: State<'_, AppState>) -> Result<planner_service::PlansData, SafeError> {
    planner_service::list_plans(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_plan(
    state: State<'_, AppState>,
    plan_id: String,
) -> Result<planner_service::PlanDetails, SafeError> {
    planner_service::get_plan(&state, plan_id).map_err(to_safe_error)
}

#[tauri::command]
pub fn cancel_plan(
    state: State<'_, AppState>,
    input: planner_service::CancelPlanInput,
) -> Result<(), SafeError> {
    planner_service::cancel_plan(&state, &input.plan_id).map_err(to_safe_error)
}

#[tauri::command]
pub fn start_plan_execution(
    state: State<'_, AppState>,
    input: supervisor_service::StartPlanExecutionInput,
) -> Result<supervisor_service::PlanExecutionResult, SafeError> {
    supervisor_service::start_plan_execution(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn verify_agent_run(
    state: State<'_, AppState>,
    input: verifier_service::VerifyRunInput,
) -> Result<verifier_service::VerificationView, SafeError> {
    verifier_service::verify_agent_run(&state, input.run_id).map_err(to_safe_error)
}

// =====================================================================
// سجل موفري الذكاء الاصطناعي وسجل النماذج (0023)
// ---------------------------------------------------------------------
// عشرة أوامر مطبوعة صغيرة مطابقة للعقد المجمّد حرفياً:
//   * القارئات (get_ai_providers · get_ai_models) بلا مدخلات وبصلاحية
//     INTEGRATION/READ.
//   * الطفرات (configure · remove · select · clear selection · test ·
//     discover) بصلاحية INTEGRATION/UPDATE.
//   * الأسرار وحدها (set/clear_ai_provider_credential) بصلاحية منفصلة
//     INTEGRATION/CREDENTIAL: من يقرأ الموفرين لا يكتب الأسرار تلقائياً.
// لا يقبل أي أمر منها هوية/مستأجراً/دوراً/صلاحية — السلطة خلفية حصراً،
// ولا يُعاد أي سر عبر أي منها.
// =====================================================================

#[tauri::command]
pub fn get_ai_providers(
    state: State<'_, AppState>,
) -> Result<ai_provider_registry_service::AiProvidersData, SafeError> {
    ai_provider_registry_service::list_providers(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn configure_ai_provider(
    state: State<'_, AppState>,
    input: ai_provider_registry_service::ConfigureAiProviderInput,
) -> Result<ai_provider_registry_service::AiProviderMutationResult, SafeError> {
    ai_provider_registry_service::configure_provider(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn remove_ai_provider(
    state: State<'_, AppState>,
    input: ai_provider_registry_service::AiProviderIdInput,
) -> Result<ai_provider_registry_service::AiProviderMutationResult, SafeError> {
    ai_provider_registry_service::remove_provider(&state, &input.provider_id).map_err(to_safe_error)
}

#[tauri::command]
pub fn set_ai_provider_credential(
    state: State<'_, AppState>,
    input: ai_provider_registry_service::SetAiProviderCredentialInput,
) -> Result<ai_provider_registry_service::AiProviderCredentialResult, SafeError> {
    ai_provider_registry_service::set_credential(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn clear_ai_provider_credential(
    state: State<'_, AppState>,
    input: ai_provider_registry_service::AiProviderIdInput,
) -> Result<ai_provider_registry_service::AiProviderCredentialResult, SafeError> {
    ai_provider_registry_service::clear_credential(&state, &input.provider_id)
        .map_err(to_safe_error)
}

#[tauri::command]
pub fn test_ai_provider(
    state: State<'_, AppState>,
    input: ai_provider_registry_service::AiProviderIdInput,
) -> Result<ai_provider_registry_service::AiProviderTestResult, SafeError> {
    ai_provider_registry_service::test_provider(&state, &input.provider_id).map_err(to_safe_error)
}

#[tauri::command]
pub fn discover_ai_models(
    state: State<'_, AppState>,
    input: ai_provider_registry_service::AiProviderIdInput,
) -> Result<ai_provider_registry_service::AiModelDiscoveryResult, SafeError> {
    ai_provider_registry_service::discover_models(&state, &input.provider_id).map_err(to_safe_error)
}

#[tauri::command]
pub fn get_ai_models(
    state: State<'_, AppState>,
) -> Result<ai_provider_registry_service::AiModelsData, SafeError> {
    ai_provider_registry_service::list_models(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn select_ai_model(
    state: State<'_, AppState>,
    input: ai_provider_registry_service::SelectAiModelInput,
) -> Result<ai_provider_registry_service::AiModelsData, SafeError> {
    ai_provider_registry_service::select_model(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn clear_ai_model_selection(
    state: State<'_, AppState>,
) -> Result<ai_provider_registry_service::AiModelsData, SafeError> {
    ai_provider_registry_service::clear_model_selection(&state).map_err(to_safe_error)
}

// =====================================================================
// التقارير المحلية — ثلاث أوامر مطبوعة صغيرة مطابقة للعقد المجمّد حرفياً:
//   * get_report_catalog بلا مدخلات، وgenerate_report بمدخل { reportId }
//     — كلاهما بصلاحية REPORT/READ.
//   * export_report بمدخل { reportId, format, targetPath } بصلاحية
//     REPORT/EXPORT المنفصلة: الكتابة على القرص سلطة مستقلة عن القراءة.
// لا يقبل أي أمر منها مستأجراً ولا هوية ولا صلاحية — السلطة خلفية حصراً،
// والمسار يأتي من حوار الحفظ الأصلي في الواجهة ويُتحقق منه في الخلف.
// =====================================================================

#[tauri::command]
pub fn get_report_catalog(
    state: State<'_, AppState>,
) -> Result<crate::domain::reports::ReportCatalogData, SafeError> {
    report_service::get_report_catalog(&state).map_err(to_safe_error)
}

#[tauri::command]
pub fn generate_report(
    state: State<'_, AppState>,
    input: report_service::GenerateReportInput,
) -> Result<crate::domain::reports::ReportData, SafeError> {
    report_service::generate_report(&state, input).map_err(to_safe_error)
}

#[tauri::command]
pub fn export_report(
    state: State<'_, AppState>,
    input: report_service::ExportReportInput,
) -> Result<crate::domain::reports::ReportExportResult, SafeError> {
    report_service::export_report(&state, input).map_err(to_safe_error)
}
