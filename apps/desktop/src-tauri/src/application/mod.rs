pub mod agent_execution_profile_service;
pub mod agent_execution_request_service;
pub mod agent_run_approval_service;
pub mod agent_run_service;
pub mod agent_runtime_coordinator;
pub mod agent_service;
pub mod ai_governance_service;
pub mod ai_provider_registry_service;
pub mod approval_service;
pub mod attention_service;
pub mod audit_anchor;
pub mod audit_service;
pub mod authentication_service;
pub mod authorization_service;
pub mod backup_service;
// GAP-05-OPEN: كتالوج خيارات شاشة تأسيس المؤسسة (الدول المعتمدة) — قراءة فقط.
pub mod bootstrap_options_service;
pub mod context_engine;
pub mod control_service;
pub mod dashboard_service;
pub mod delegation_execution_service;
pub mod demo_identity_service;
pub mod evidence_service;
pub mod execution_dispatcher;
#[allow(dead_code)]
pub mod execution_input_assembler;
#[allow(dead_code)]
pub mod governed_agent_executor_service;
pub mod health_service;
pub mod integration_status_service;
pub mod knowledge_service;
// اختبار تكامل حي مع خادم استنتاج محلي (Bonsai 2). مُهمَل افتراضياً: يُشغَّل
// صراحةً بـ`--ignored` لأنه يشترط خدمة تعمل على 127.0.0.1:8080.
#[cfg(test)]
#[path = "live_bonsai_integration_tests.rs"]
mod live_bonsai_integration_tests;
pub mod memory_service;
pub mod model_configuration_service;
pub mod module_service;
#[cfg(test)]
#[path = "mp05_performance_tests.rs"]
mod mp05_performance_tests;
#[cfg(test)]
#[path = "mp05_release_security_tests.rs"]
mod mp05_release_security_tests;
pub mod obligation_service;
// اختبارات تحمّل واستقرار محدودة + تحقق حي من سلامة نسخة قاعدة المستخدم.
// مُهمَلة افتراضياً: تُشغَّل صراحةً بـ`--ignored`.
#[cfg(test)]
#[path = "operational_stress_tests.rs"]
mod operational_stress_tests;
pub mod organizational_intelligence_service;
pub mod planner_service;
#[allow(dead_code)]
pub mod provider_agent_executor;
pub mod provider_runtime_service;
pub mod regulatory_source_service;
pub mod report_service;
pub mod risk_service;
pub mod skill_selection_resolver;
pub mod skill_service;
pub mod supervisor_service;
pub mod task_service;
#[path = "tests_test_fixture.rs"]
pub mod tests_test_fixture;
pub mod tool_adapter_registry;
pub mod tool_invocation_service;
pub mod tool_service;
pub mod user_administration_service;
pub mod verifier_service;
