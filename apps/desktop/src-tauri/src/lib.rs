mod application;
mod domain;
mod infrastructure;
mod presentation;

pub mod approval_lifecycle {
    pub use crate::application::agent_run_approval_service::{
        request_agent_run_approval, AgentRunApprovalRequestResult, AgentRunApprovalRequirement,
    };
}

pub mod provider_runtime {
    pub use crate::application::provider_runtime_service::{
        ProviderResolver, ProviderRuntimeService,
    };
    pub use crate::domain::provider::{
        DiscoveredModel, ModelId, ModelProvider, ProviderFailure, ProviderGeneration,
        ProviderHealth, ProviderId, RuntimeExecutionInput,
    };
}

pub mod governed_execution {
    pub use crate::application::agent_runtime_coordinator::MAX_TOOL_CALLS_PER_RUN;
    pub use crate::application::execution_dispatcher::ExecutionDispatcher;
    pub use crate::domain::execution_policy::{
        ApprovalRequirement, ExecutionDecision, ExecutionDecisionOutcome, ExecutionDecisionReason,
        ExecutionEnvironment, ExecutionIntentReference, ExecutionPolicyInput, ExecutionRequest,
        PolicyGate,
    };
    pub use crate::domain::execution_policy_governor::ExecutionPolicyGovernor;
}

use infrastructure::database::AppState;
use presentation::commands::{
    archive_obligation, assign_skill, assign_task, assign_tool, assign_user_role,
    bootstrap_production, cancel_agent_run, cancel_plan, check_ai_provider,
    clear_ai_model_selection, clear_ai_provider_credential, clear_local_model_configuration,
    configure_ai_provider, configure_local_model, create_agent, create_agent_execution_profile,
    create_agent_run, create_backup, create_control, create_plan, create_remediation_task,
    create_skill, create_skill_version, create_tool, create_tool_version, decide_approval,
    disable_control, disable_knowledge_source, disable_memory_item, disable_skill, disable_tool,
    discover_ai_models, export_report, generate_report, get_agent_execution_profiles,
    get_agent_run, get_agent_run_execution_binding, get_agent_run_result, get_agent_runs,
    get_agent_tool_assignments, get_agents, get_ai_models, get_ai_providers, get_attention,
    get_audit_trail, get_auth_status, get_bootstrap_options, get_control_detail, get_controls,
    get_dashboard, get_demo_actors, get_evidence, get_evidence_detail, get_integration_status,
    get_knowledge_source, get_knowledge_sources, get_memory, get_module_availability,
    get_obligation, get_obligations, get_organizational_intelligence, get_plan, get_plans,
    get_regulatory_sources, get_report_catalog, get_risks, get_session_context, get_skill_catalog,
    get_system_health, get_task_history, get_tasks, get_tool_catalog, get_tool_invocations_for_run,
    ingest_knowledge_source, link_control_evidence, link_control_obligation, link_control_risk,
    list_backups, list_workspace_users, login, logout, promote_memory_item,
    propose_semantic_memory, record_control_assessment, register_evidence_reference,
    remove_ai_provider, request_agent_execution, restore_backup, revoke_user_role,
    search_knowledge, select_ai_model, set_agent_status, set_ai_provider_credential,
    set_user_status, start_demo_experience, start_plan_execution, start_queued_execution,
    switch_demo_actor, test_ai_provider, transition_task, unassign_skill, unassign_tool,
    unlink_control_evidence, unlink_control_obligation, unlink_control_risk, update_agent,
    update_control, update_evidence_status, update_obligation, verify_agent_run, verify_obligation,
};
// ===== 0026: أوامر إغلاق عيوب UAT الإنتاجية =====
use presentation::commands::{
    change_password, create_obligation, create_regulatory_source, create_workspace_user,
    revoke_user_sessions, verify_audit_chain,
};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // مكون الحوار الرسمي: اختيار مجلد/ملف النسخ الاحتياطي فقط — لا يمنح وصول ملفات بذاته.
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let state = AppState::initialize(&app.handle())
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard,
            get_attention,
            get_obligations,
            get_obligation,
            update_obligation,
            archive_obligation,
            verify_obligation,
            get_organizational_intelligence,
            get_regulatory_sources,
            get_audit_trail,
            get_integration_status,
            get_session_context,
            get_risks,
            get_tasks,
            get_controls,
            get_evidence,
            get_module_availability,
            get_system_health,
            // فحص المزود الحي الصريح (MP05 / C10) — إجراء مستخدم لا استعلام دوري.
            check_ai_provider,
            create_remediation_task,
            assign_task,
            transition_task,
            get_task_history,
            decide_approval,
            register_evidence_reference,
            get_agent_runs,
            get_agent_run,
            get_agent_run_result,
            create_agent_run,
            cancel_agent_run,
            get_agents,
            create_agent,
            update_agent,
            set_agent_status,
            get_demo_actors,
            switch_demo_actor,
            // ===== MASTER PHASE 03 — GOVERNED AGENT & TOOL RUNTIME =====
            get_agent_execution_profiles,
            create_agent_execution_profile,
            request_agent_execution,
            start_queued_execution,
            get_agent_run_execution_binding,
            get_tool_invocations_for_run,
            get_skill_catalog,
            create_skill,
            create_skill_version,
            disable_skill,
            assign_skill,
            unassign_skill,
            get_tool_catalog,
            create_tool,
            create_tool_version,
            disable_tool,
            assign_tool,
            unassign_tool,
            get_agent_tool_assignments,
            configure_local_model,
            clear_local_model_configuration,
            // ===== MASTER PHASE 04 — INTELLIGENCE / KNOWLEDGE / PLANS =====
            ingest_knowledge_source,
            get_knowledge_sources,
            get_knowledge_source,
            search_knowledge,
            disable_knowledge_source,
            get_memory,
            disable_memory_item,
            propose_semantic_memory,
            promote_memory_item,
            create_plan,
            get_plans,
            get_plan,
            cancel_plan,
            start_plan_execution,
            verify_agent_run,
            // ===== MASTER PHASE 05 — ENTERPRISE RELEASE =====
            get_auth_status,
            // GAP-05-OPEN: كتالوج دول التأسيس قبل أي هوية (UNPROVISIONED).
            get_bootstrap_options,
            start_demo_experience,
            bootstrap_production,
            login,
            logout,
            list_workspace_users,
            set_user_status,
            assign_user_role,
            revoke_user_role,
            // ===== 0026: إدارة المستخدمين الإنتاجية وكلمة المرور وتأليف الأعمال =====
            create_workspace_user,
            revoke_user_sessions,
            change_password,
            create_regulatory_source,
            create_obligation,
            verify_audit_chain,
            create_backup,
            restore_backup,
            list_backups,
            // ===== دورة حياة الأدلة وتأليف الضوابط (0022) =====
            update_evidence_status,
            get_evidence_detail,
            get_control_detail,
            create_control,
            update_control,
            disable_control,
            record_control_assessment,
            link_control_obligation,
            unlink_control_obligation,
            link_control_risk,
            unlink_control_risk,
            link_control_evidence,
            unlink_control_evidence,
            // ===== سجل موفري الذكاء الاصطناعي وسجل النماذج (0023) =====
            get_ai_providers,
            configure_ai_provider,
            remove_ai_provider,
            set_ai_provider_credential,
            clear_ai_provider_credential,
            test_ai_provider,
            discover_ai_models,
            get_ai_models,
            select_ai_model,
            clear_ai_model_selection,
            // ===== التقارير المحلية (عقد مجمّد — بلا هجرة: التقارير محسوبة لا مخزَّنة) =====
            get_report_catalog,
            generate_report,
            export_report
        ])
        .build(tauri::generate_context!())
        .expect("تعذر تشغيل NEXUS")
        .run(|_app_handle, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                crate::infrastructure::local_logger::info("APP_SHUTDOWN", "", "");
            }
        });
}
