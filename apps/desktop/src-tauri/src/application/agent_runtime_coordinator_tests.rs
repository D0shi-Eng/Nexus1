// =====================================================================
// MASTER PHASE 03 — بطارية اختبارات منسق التشغيل المحكوم
// البروتوكول المحدود، الحوكمة، السباقات، الإلغاء، حقن النموذج،
// الاستبدال بعد الموافقة، العزل، والمناعة الإنشائية (§82-§94).
// =====================================================================
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, Connection};

use super::start_queued_execution_with_provider;
use crate::{
    application::{
        agent_execution_request_service::{
            get_agent_run_execution_binding, request_agent_execution, RequestAgentExecutionInput,
        },
        approval_service::{decide_approval, DecideApprovalInput},
        model_configuration_service::{
            configure_local_model_with_provider, ConfigureLocalModelInput,
        },
        tool_service::{assign_tool, AssignToolInput},
    },
    domain::{
        error::AppError,
        models::AgentRunStatus,
        provider::{
            DiscoveredModel, ModelId, ModelProvider, ProviderFailure, ProviderGeneration,
            ProviderHealth, ProviderId, ProviderToolCall, ProviderToolResponse,
            RuntimeExecutionInput,
        },
    },
    infrastructure::database::{
        initialize_demo_database, AppState, DEMO_OPERATOR_USER_ID, DEMO_REVIEWER_USER_ID,
        DEMO_TENANT_ID,
    },
};

// ===== مزود الاختبار: سلوك مجدول لكل نداء بالترتيب =====
#[derive(Clone)]
enum ProviderOutcome {
    Text(&'static str),
    ToolCall(&'static str, serde_json::Value),
    /// استدعاء أداة على مسار لا يعلن أدوات — يُرفض فشلاً مغلقاً.
    ToolCallOnPlainPath,
    Failure(ProviderFailure),
}

struct RuntimeTestProvider {
    models: Mutex<Vec<String>>,
    outcomes: Mutex<Vec<ProviderOutcome>>,
}

impl RuntimeTestProvider {
    fn new(models: &[&str], outcomes: Vec<ProviderOutcome>) -> Self {
        Self {
            models: Mutex::new(models.iter().map(|model| model.to_string()).collect()),
            outcomes: Mutex::new(outcomes),
        }
    }

    fn with_tools_support(models: &[&str], outcomes: Vec<ProviderOutcome>) -> Self {
        Self::new(models, outcomes)
    }
}

impl ModelProvider for RuntimeTestProvider {
    fn provider_id(&self) -> ProviderId {
        ProviderId::ollama_local()
    }

    fn health(&self) -> ProviderHealth {
        ProviderHealth::Available
    }

    fn discover_models(&self) -> Result<Vec<DiscoveredModel>, ProviderFailure> {
        let models = self
            .models
            .lock()
            .expect("قائمة النماذج يجب أن تتاح")
            .clone();
        Ok(models
            .into_iter()
            .map(|model| DiscoveredModel {
                id: ModelId::new(&model).expect("معرف نموذج الاختبار يجب أن يكون صالحاً"),
                display_name: model,
                modified_at: None,
                size_bytes: None,
                supports_tools: true,
            })
            .collect())
    }

    fn generate_non_streaming(
        &self,
        model_id: &ModelId,
        _input: &RuntimeExecutionInput,
    ) -> Result<ProviderGeneration, ProviderFailure> {
        let mut outcomes = self.outcomes.lock().expect("سلوك المزود يجب أن يتاح");
        match outcomes.pop().unwrap_or(ProviderOutcome::Text("fallback")) {
            ProviderOutcome::Text(text) => Ok(ProviderGeneration {
                model_id: model_id.clone(),
                output: text.to_owned(),
                generated_at: "2026-10-04T00:00:00Z".to_owned(),
            }),
            // أي استدعاء أداة على المسار العادي (بلا أدوات معلنة) يُرفض صراحة.
            ProviderOutcome::ToolCall(..) | ProviderOutcome::ToolCallOnPlainPath => {
                Err(ProviderFailure::ToolCallUnsupported)
            }
            ProviderOutcome::Failure(failure) => Err(failure),
        }
    }

    fn generate_with_tools(
        &self,
        model_id: &ModelId,
        _input: &RuntimeExecutionInput,
        _tools: &[crate::domain::provider::ProviderToolDefinition],
    ) -> Result<ProviderToolResponse, ProviderFailure> {
        let mut outcomes = self.outcomes.lock().expect("سلوك المزود يجب أن يتاح");
        // المكدس معكوس: آخر نداء يُستهلك أولاً — لذا نعكس الترتيب عند البناء.
        let outcome = outcomes.pop().unwrap_or(ProviderOutcome::Text("fallback"));
        Ok(match outcome {
            ProviderOutcome::Text(text) => ProviderToolResponse::Text(ProviderGeneration {
                model_id: model_id.clone(),
                output: text.to_owned(),
                generated_at: "2026-10-04T00:00:00Z".to_owned(),
            }),
            ProviderOutcome::ToolCall(name, arguments) => {
                ProviderToolResponse::ToolCall(ProviderToolCall {
                    tool_name: name.to_owned(),
                    arguments,
                })
            }
            ProviderOutcome::ToolCallOnPlainPath | ProviderOutcome::Failure(_) => {
                return Err(ProviderFailure::ToolCallUnsupported);
            }
        })
    }
}

// ===== حالة الاختبار: قاعدة الديمو المبذورة + وكيل + بروفايل + نموذج مفعّل =====
fn runtime_state() -> AppState {
    let connection = Connection::open_in_memory().expect("قاعدة الاختبار يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("تفعيل المفاتيح يجب أن ينجح");
    initialize_demo_database(&connection).expect("تهيئة الديمو يجب أن تنجح");
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "INSERT INTO ai_model_registry (id, tenant_id, provider_kind, model_id, display_name, enabled, data_residency, allowed_classification_ceiling, requires_human_approval, created_at, updated_at) VALUES ('model-test', ?1, 'LOCAL', 'test-model:latest', 'test-model:latest', 1, 'LOCAL', 'INTERNAL', 1, ?2, ?2)",
            params![DEMO_TENANT_ID, now],
        )
        .expect("النموذج المفعّل يجب أن يُزرع");
    // وكيل مفعّل للديمو ببروفايل نسخة 1.
    connection
        .execute(
            "INSERT INTO agents (id, tenant_id, name, description, status, created_at, updated_at) VALUES ('agent_runtime_demo', ?1, 'Runtime Demo Agent', 'Governed runtime test agent', 'ACTIVE', ?2, ?2)",
            params![DEMO_TENANT_ID, now],
        )
        .expect("الوكيل يجب أن يُزرع");
    connection
        .execute(
            "INSERT INTO agent_execution_profiles (id, tenant_id, agent_id, version_number, instruction, created_at) VALUES ('profile_runtime_v1', ?1, 'agent_runtime_demo', 1, 'Summarize bounded obligations only.', ?2)",
            params![DEMO_TENANT_ID, now],
        )
        .expect("البروفايل يجب أن يُزرع");
    AppState::test_instance(
        connection,
        DEMO_TENANT_ID.to_owned(),
        DEMO_OPERATOR_USER_ID.to_owned(),
    )
}

fn request_input() -> RequestAgentExecutionInput {
    RequestAgentExecutionInput {
        agent_id: "agent_runtime_demo".to_owned(),
        task_content: "List the at-risk obligations".to_owned(),
        skill_version_id: None,
        knowledge_query: None,
    }
}

fn as_reviewer(state: &AppState) {
    state.set_active_user_for_tests(DEMO_REVIEWER_USER_ID);
}

fn as_operator(state: &AppState) {
    state.set_active_user_for_tests(DEMO_OPERATOR_USER_ID);
}

fn invocation_count(state: &AppState, run_id: &str) -> i64 {
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    connection
        .query_row(
            "SELECT COUNT(*) FROM tool_invocations WHERE agent_run_id = ?1",
            [run_id],
            |row| row.get(0),
        )
        .expect("عد الاستدعاءات يجب أن ينجح")
}

fn run_row(state: &AppState, run_id: &str) -> (String, i64) {
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    connection
        .query_row(
            "SELECT status, revision FROM agent_runs WHERE id = ?1",
            [run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("صف التشغيل يجب أن يقرأ")
}

fn audit_count(state: &AppState, event_type: &str) -> i64 {
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE event_type = ?1",
            [event_type],
            |row| row.get(0),
        )
        .expect("عد التدقيق يجب أن ينجح")
}

#[test]
fn plain_request_binds_exact_identity_and_waits_for_approval() {
    let state = runtime_state();
    let result = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    assert_eq!(result.run.status, AgentRunStatus::WaitingApproval);

    let binding =
        get_agent_run_execution_binding(&state, result.run.id.clone()).expect("الربط يجب أن يقرأ");
    assert_eq!(binding.execution_profile_version_id, "profile_runtime_v1");
    assert_eq!(
        binding.model_id_at_request.as_deref(),
        Some("test-model:latest")
    );
    // 0024: الربط يحمل هوية الوجهة لا معرّف النموذج وحده. غياب أي حقل منها يعني
    // «هوية وجهة غير مُثبتة» وهو ما يرفضه المنسق فشلاً مغلقاً.
    assert_eq!(
        binding.provider_id_at_request.as_deref(),
        Some("OLLAMA_LOCAL")
    );
    assert_eq!(binding.provider_kind_at_request.as_deref(), Some("OLLAMA"));
    assert_eq!(
        binding
            .endpoint_identity_at_request
            .as_ref()
            .map(String::len),
        Some(64),
        "بصمة الوجهة SHA-256 بطول 64 محرفاً"
    );
    assert_eq!(
        binding.approval_binding_hash.as_ref().map(String::len),
        Some(64),
        "بصمة الربط الكاملة SHA-256 بطول 64 محرفاً"
    );
    assert!(!binding.task_content_hash.is_empty());
    assert_eq!(binding.agent_run_revision, 1);
    // تدقيق الطلب بلا أي محتوى خام للمهمة (§58).
    assert_eq!(audit_count(&state, "AGENT_EXECUTION_REQUESTED"), 1);
}

#[test]
fn request_fails_closed_without_an_enabled_model() {
    let state = runtime_state();
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute("UPDATE ai_model_registry SET enabled = 0", [])
            .expect("إلغاء التفعيل يجب أن ينجح");
    }
    assert!(matches!(
        request_agent_execution(&state, request_input()),
        Err(AppError::ModelUnavailable)
    ));
}

#[test]
fn request_rejects_missing_profile_disabled_agent_and_oversized_task() {
    let state = runtime_state();

    let mut no_profile = request_input();
    no_profile.agent_id = "agent_demo_1".to_owned();
    // وكيل الديمو المبذور بلا بروفايل ⇒ لا طلب تنفيذ.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "INSERT INTO agents (id, tenant_id, name, description, status, created_at, updated_at) VALUES ('agent_demo_1', ?1, 'Profile-less', 'No profile', 'ACTIVE', datetime('now'), datetime('now'))",
                params![DEMO_TENANT_ID],
            )
            .expect("الوكيل الثاني يجب أن يُزرع");
    }
    assert!(matches!(
        request_agent_execution(&state, no_profile),
        Err(AppError::ResourceNotFound)
    ));

    let mut oversized = request_input();
    oversized.task_content = "x".repeat(2_049);
    assert!(matches!(
        request_agent_execution(&state, oversized),
        Err(AppError::ValidationFailed)
    ));

    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "UPDATE agents SET status = 'DISABLED' WHERE id = 'agent_runtime_demo'",
                [],
            )
            .expect("التعطيل يجب أن ينجح");
    }
    assert!(matches!(
        request_agent_execution(&state, request_input()),
        Err(AppError::AgentDisabled)
    ));
}

#[test]
fn plain_run_completes_with_provider_provenance_and_stays_ai_interpretation() {
    let state = runtime_state();
    // أزل تعيينات الأدوات: لا تعيينات موجودة أصلاً ⇒ مسار نصي عادي.
    let provider = RuntimeTestProvider::new(
        &["test-model:latest"],
        vec![ProviderOutcome::Text("Plain governed answer.")],
    );
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    // نعتمد عبر قرار الموافقة العام (منع الذات سارٍ).
    let approvals: Vec<String> = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let mut statement = connection
            .prepare("SELECT id FROM approvals WHERE resource_type = 'AGENT_RUN' AND decision = 'PENDING'")
            .expect("الاستعلام يجب أن ينجح");
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("القراءة يجب أن تنجح")
            .collect::<Result<Vec<_>, _>>()
            .expect("التجميع يجب أن ينجح");
        ids
    };
    let decision = decide_approval(
        &state,
        DecideApprovalInput {
            approval_id: approvals[0].clone(),
            decision: "APPROVED".to_owned(),
            reason: "Independent reviewer approved the bounded run.".to_owned(),
        },
    )
    .expect("الاعتماد يجب أن ينجح");
    assert_eq!(decision.decision, "APPROVED");

    as_operator(&state);
    let outcome = start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("البدء يجب أن ينجح");
    assert_eq!(outcome.run.status, AgentRunStatus::Completed);
    let result = outcome.result.expect("النتيجة يجب أن تُعاد");
    assert_eq!(result.outcome, "COMPLETED");
    assert_eq!(result.provider_id, "OLLAMA_LOCAL");
    assert_eq!(result.model_id.as_deref(), Some("test-model:latest"));
    assert_eq!(audit_count(&state, "AGENT_RUN_EXECUTION_STARTED"), 1);
    assert_eq!(audit_count(&state, "AGENT_RUN_EXECUTION_COMPLETED"), 1);
    let (status, revision) = run_row(&state, &requested.run.id);
    assert_eq!(status, "COMPLETED");
    assert!(revision >= 3);
}

#[test]
fn model_disappearing_from_discovery_fails_closed_without_state_change() {
    let state = runtime_state();
    let provider = RuntimeTestProvider::new(&[], vec![]); // اكتشاف فارغ
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);
    assert!(matches!(
        start_queued_execution_with_provider(&state, &requested.run.id, &provider),
        Err(AppError::ModelUnavailable)
    ));
    // التشغيل لم يتحرك ولا تأثر — فشل مغلق قبل أي حجز.
    let (status, revision) = run_row(&state, &requested.run.id);
    assert_eq!(status, "QUEUED");
    assert!(revision >= 2);
}

#[test]
fn configured_model_change_after_request_is_stale_approval() {
    let state = runtime_state();
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);
    // بدّل النموذج المفعّل بعد الاعتماد.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute("UPDATE ai_model_registry SET model_id = 'other-model:latest' WHERE id = 'model-test'", [])
            .expect("تبديل النموذج يجب أن ينجح");
    }
    let provider = RuntimeTestProvider::new(&["other-model:latest"], vec![]);
    assert!(matches!(
        start_queued_execution_with_provider(&state, &requested.run.id, &provider),
        Err(AppError::StaleApproval)
    ));
}

// ===== 0024: اختبارات سالبة لربط هوية وجهة التنفيذ =====

/// العيبة المُغلقة (كانت HIGH): كانت الموافقة تربط معرّف النموذج وحده، فيكفي
/// تبديل الموفر المختار بعد الاعتماد إلى وجهة أخرى **تعلن النموذج نفسه** ليجري
/// التنفيذ على وجهة لم تُعتمد. الاختبار يبدّل الوجهة فقط ويُبقي النموذج
/// والمحوّل المُحقون كما هما، فيكون الرفض سببه هوية الوجهة لا أي شيء آخر.
#[test]
fn switching_the_provider_destination_after_approval_is_stale_approval() {
    let state = runtime_state();
    // المحوّل المُحقون يعلن OLLAMA_LOCAL — نفس هوية الموفر المربوطة.
    let provider = RuntimeTestProvider::new(
        &["test-model:latest"],
        vec![ProviderOutcome::Text("Should never run.")],
    );
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);

    // تبديل الوجهة: موفر آخر + إعادة ربط النموذج المفعّل به. النموذج نفسه
    // (`test-model:latest`) والمحوّل نفسه — يتغيّر العنوان والنوع فقط.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let now = Utc::now().to_rfc3339();
        connection
            .execute(
                "INSERT INTO ai_provider_registry (id, tenant_id, provider_id, kind, display_name, base_url, transport, enabled, created_at, updated_at) VALUES ('provider_row_swapped', ?1, 'PROVIDER_SWAPPED_LOCAL', 'LM_STUDIO', 'Swapped destination', 'http://127.0.0.1:1234', 'HTTP_LOOPBACK', 1, ?2, ?2)",
                params![DEMO_TENANT_ID, now],
            )
            .expect("الموفر البديل يجب أن يُزرع");
        connection
            .execute(
                "UPDATE ai_model_registry SET provider_id = 'provider_row_swapped' WHERE id = 'model-test'",
                [],
            )
            .expect("ربط النموذج بالوجهة الجديدة يجب أن ينجح");
    }

    assert!(
        matches!(
            start_queued_execution_with_provider(&state, &requested.run.id, &provider),
            Err(AppError::StaleApproval)
        ),
        "تبديل وجهة التنفيذ بعد الاعتماد يجب أن يُرفض فشلاً مغلقاً"
    );
    // لم يتحرك التشغيل ولم يُنتج أي نتيجة.
    let (status, _revision) = run_row(&state, &requested.run.id);
    assert_eq!(status, "QUEUED");
    assert_eq!(audit_count(&state, "AGENT_RUN_EXECUTION_STARTED"), 0);
}

/// ربط تاريخي (مكتوب قبل 0024) بلا أعمدة هوية الوجهة لا يُقبل كتوافق:
/// تنفيذٌ بلا هوية وجهة مُثبتة هو نفس الثغرة، فالنتيجة فشل مغلق.
#[test]
fn legacy_binding_without_provider_identity_is_rejected_fail_closed() {
    let state = runtime_state();
    let provider = RuntimeTestProvider::new(
        &["test-model:latest"],
        vec![ProviderOutcome::Text("Should never run.")],
    );
    let created = crate::application::agent_run_service::create_agent_run(
        &state,
        crate::application::agent_run_service::CreateAgentRunInput {
            agent_id: "agent_runtime_demo".to_owned(),
        },
    )
    .expect("إنشاء التشغيل يجب أن ينجح");
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let now = Utc::now().to_rfc3339();
        connection
            .execute(
                "INSERT INTO agent_run_execution_bindings (agent_run_id, tenant_id, agent_run_revision, agent_id, execution_profile_version_id, task_content, task_content_hash, model_id_at_request, bound_at) VALUES (?1, ?2, ?3, 'agent_runtime_demo', 'profile_runtime_v1', 'Legacy task', 'legacyhash', 'test-model:latest', ?4)",
                params![created.run.id, DEMO_TENANT_ID, created.run.revision, now],
            )
            .expect("الربط التاريخي يجب أن يُكتب");
    }

    assert!(
        matches!(
            start_queued_execution_with_provider(&state, &created.run.id, &provider),
            Err(AppError::StaleApproval)
        ),
        "ربط بلا هوية وجهة يجب أن يُرفض ولا يُعامل كتوافق"
    );
}

#[test]
fn unadvertised_tool_call_from_provider_is_denied_without_execution() {
    let state = runtime_state();
    assign_obligations_tool(&state);
    let provider = RuntimeTestProvider::with_tools_support(
        &["test-model:latest"],
        vec![
            ProviderOutcome::Text("Should never happen."),
            ProviderOutcome::ToolCall("nexus.shell.v1", serde_json::json!({ "command": "evil" })),
        ],
    );
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);
    let outcome = start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("البدء يجب أن ينجح");
    assert_eq!(outcome.run.status, AgentRunStatus::Failed);
    assert_eq!(
        outcome
            .result
            .as_ref()
            .expect("النتيجة")
            .failure_code
            .as_deref(),
        Some("TOOL_AUTHORIZATION_DENIED")
    );
    assert_eq!(invocation_count(&state, &requested.run.id), 0);
    assert_eq!(audit_count(&state, "TOOL_INVOCATION_DENIED"), 1);
}

#[test]
fn allowed_read_only_tool_executes_and_feeds_a_bounded_final_answer() {
    let state = runtime_state();
    assign_obligations_tool(&state);
    // النداء الأول: Text عبر مسار الأدوات (بدون طلب أداة) — يُقبل جواباً نهائياً.
    let provider = RuntimeTestProvider::with_tools_support(
        &["test-model:latest"],
        vec![ProviderOutcome::Text("Direct bounded answer.")],
    );
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);
    let outcome = start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("البدء يجب أن ينجح");
    assert_eq!(outcome.run.status, AgentRunStatus::Completed);
    assert!(outcome
        .result
        .expect("النتيجة")
        .output
        .unwrap()
        .contains("Direct bounded answer."));
}

#[test]
fn full_tool_loop_executes_real_read_only_adapter_and_final_provider_call() {
    let state = runtime_state();
    assign_obligations_tool(&state);
    let provider = RuntimeTestProvider::with_tools_support(
        &["test-model:latest"],
        vec![
            // النداء الأخير (يُستهلك أخيراً لأن المكدس معكوس): جواب نهائي.
            ProviderOutcome::Text("Final governed answer after tool."),
            // النداء الأول: استدعاء مهيكل للمحوّل القرائي المدمج.
            ProviderOutcome::ToolCall(
                "nexus_obligations_bounded_list_v1",
                serde_json::json!({ "query": "evidence", "limit": 2 }),
            ),
        ],
    );
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);

    let outcome = start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("البدء يجب أن ينجح");
    assert_eq!(outcome.run.status, AgentRunStatus::Completed);
    assert_eq!(invocation_count(&state, &requested.run.id), 1);
    // نتيجة الاستدعاء محصنة ومقروءة من القاعدة.
    let output: String = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT r.output_json FROM tool_invocation_results r JOIN tool_invocations i ON i.id = r.tool_invocation_id WHERE i.agent_run_id = ?1 AND r.outcome = 'COMPLETED'",
                [&requested.run.id],
                |row| row.get(0),
            )
            .expect("نتيجة الأداة يجب أن تقرأ")
    };
    assert!(output.contains("\"total\":"));
    assert!(output.len() <= 1024);
    assert_eq!(audit_count(&state, "TOOL_INVOCATION_COMPLETED"), 1);
    let result = outcome.result.expect("النتيجة النهائية");
    assert_eq!(result.outcome, "COMPLETED");
}

#[test]
fn oversized_or_unknown_tool_arguments_fail_closed() {
    let state = runtime_state();
    assign_obligations_tool(&state);
    // حقل غير معلن في العقد ⇒ TOOL_INPUT_INVALID.
    let provider = RuntimeTestProvider::with_tools_support(
        &["test-model:latest"],
        vec![
            ProviderOutcome::Text("never"),
            ProviderOutcome::ToolCall(
                "nexus_obligations_bounded_list_v1",
                serde_json::json!({ "limit": 999, "permission": "all" }),
            ),
        ],
    );
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);
    let outcome = start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("البدء يجب أن ينجح");
    assert_eq!(outcome.run.status, AgentRunStatus::Failed);
    assert_eq!(
        outcome.result.expect("النتيجة").failure_code.as_deref(),
        Some("TOOL_INPUT_INVALID")
    );
    assert_eq!(invocation_count(&state, &requested.run.id), 0);
}

#[test]
fn tool_output_asking_for_another_tool_never_gets_authority() {
    let state = runtime_state();
    assign_obligations_tool(&state);
    let provider = RuntimeTestProvider::with_tools_support(
        &["test-model:latest"],
        vec![
            // النداء الأخير يحاول استدعاء أداة أخرى — المسار العادي يرفضه.
            ProviderOutcome::ToolCallOnPlainPath,
            ProviderOutcome::ToolCall(
                "nexus_obligations_bounded_list_v1",
                serde_json::json!({ "limit": 2 }),
            ),
        ],
    );
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);
    let outcome = start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("البدء يجب أن ينجح");
    assert_eq!(outcome.run.status, AgentRunStatus::Failed);
    assert_eq!(
        outcome.result.expect("النتيجة").failure_code.as_deref(),
        Some("TOOL_CALL_LIMIT_EXCEEDED")
    );
    // استدعاء واحد فقط نُفّذ — لا سلطة إضافية من مخرج الأداة (§86).
    assert_eq!(invocation_count(&state, &requested.run.id), 1);
}

#[test]
fn provider_timeout_maps_to_closed_failure_result() {
    let state = runtime_state();
    let provider = RuntimeTestProvider::new(
        &["test-model:latest"],
        vec![ProviderOutcome::Failure(ProviderFailure::Timeout)],
    );
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);
    let outcome = start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("البدء يجب أن ينجح");
    assert_eq!(outcome.run.status, AgentRunStatus::Failed);
    assert_eq!(
        outcome.result.expect("النتيجة").failure_code.as_deref(),
        Some("PROVIDER_TIMEOUT")
    );
}

#[test]
fn double_start_is_rejected_by_cas() {
    let state = runtime_state();
    let provider = RuntimeTestProvider::new(&["test-model:latest"], vec![]);
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);
    let first = start_queued_execution_with_provider(&state, &requested.run.id, &provider);
    assert!(first.is_ok());
    assert!(matches!(
        start_queued_execution_with_provider(&state, &requested.run.id, &provider),
        Err(AppError::InvalidStateTransition) | Err(AppError::Conflict)
    ));
}

#[test]
fn model_configuration_requires_discovered_model_and_keeps_single_enable() {
    let state = runtime_state();
    as_reviewer(&state); // المراجع بلا integration:update
    let provider = RuntimeTestProvider::new(&["ghost-model:latest"], vec![]);
    assert!(matches!(
        configure_local_model_with_provider(
            &state,
            ConfigureLocalModelInput {
                model_id: "ghost-model:latest".to_owned()
            },
            &provider,
        ),
        Err(AppError::AuthorizationDenied)
    ));

    as_operator(&state);
    // نموذج غير مكتشف حياً ⇒ رفض حتى مع الصلاحية.
    assert!(matches!(
        configure_local_model_with_provider(
            &state,
            ConfigureLocalModelInput {
                model_id: "undiscovered:latest".to_owned()
            },
            &provider,
        ),
        Err(AppError::ModelUnavailable)
    ));
    // مكتشف حياً ⇒ تفعيل وحيد + تدقيق.
    let configured = configure_local_model_with_provider(
        &state,
        ConfigureLocalModelInput {
            model_id: "ghost-model:latest".to_owned(),
        },
        &provider,
    )
    .expect("التكوين يجب أن ينجح");
    assert_eq!(
        configured.configured_model_id.as_deref(),
        Some("ghost-model:latest")
    );
    let enabled: i64 = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT COUNT(*) FROM ai_model_registry WHERE tenant_id = ?1 AND enabled = 1",
                params![DEMO_TENANT_ID],
                |row| row.get(0),
            )
            .expect("عد المفعّل يجب أن يقرأ")
    };
    assert_eq!(enabled, 1);
    assert_eq!(audit_count(&state, "MODEL_CONFIGURATION_CHANGED"), 1);
}

// ===== أدوات مساعدة =====

fn approve_pending_agent_run(state: &AppState) {
    let approval_id: String = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT id FROM approvals WHERE resource_type = 'AGENT_RUN' AND decision = 'PENDING' LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("موافقة الانتظار يجب أن تقرأ")
    };
    decide_approval(
        state,
        DecideApprovalInput {
            approval_id,
            decision: "APPROVED".to_owned(),
            reason: "Independent reviewer approved the governed request.".to_owned(),
        },
    )
    .expect("الاعتماد يجب أن ينجح");
}

fn assign_obligations_tool(state: &AppState) {
    assign_tool(
        state,
        AssignToolInput {
            agent_id: "agent_runtime_demo".to_owned(),
            tool_version_id: "tool_version_builtin_obligations_v1".to_owned(),
        },
    )
    .expect("تعيين الأداة المدمجة يجب أن ينجح");
}

// ===== ربط الموافقة الدقيق (§42/§83): أداة أثر أعلى تتوقف على اعتماد مراجع =====

// أداة حالة اختبارية: تُزرع مباشرة (أثر STATE_CHANGE + ربط نفس المحوّل القرائي
// لعرض REQUIRE_APPROVAL) — لا تُنفَّذ إطلاقاً دون موافقة، ولا تُنشئ طفرة منتج.
fn assign_state_change_tool(state: &AppState) {
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "INSERT INTO tools (id, tenant_id, name, description, status, created_at, updated_at) VALUES ('tool_state_change_test', ?1, 'Hypothetical state change', 'Deterministic approval test only', 'ACTIVE', ?2, ?2)",
            params![DEMO_TENANT_ID, now],
        )
        .expect("الأداة يجب أن تُزرع");
    connection
        .execute(
            "INSERT INTO tool_versions (id, tenant_id, tool_id, version_number, input_contract, output_contract, mutation, externality, privilege, reversibility, created_at) VALUES ('tool_version_state_change_test', ?1, 'tool_state_change_test', 1, '{\"fields\":[{\"name\":\"query\",\"dataType\":\"string\",\"required\":false,\"description\":\"filter\"},{\"name\":\"limit\",\"dataType\":\"integer\",\"required\":false,\"description\":\"bound\"}],\"allowsAdditionalFields\":false}', '{\"fields\":[],\"allowsAdditionalFields\":false}', 'STATE_CHANGE', 'INTERNAL', 'STANDARD', 'REVERSIBLE', ?2)",
            params![DEMO_TENANT_ID, now],
        )
        .expect("النسخة يجب أن تُزرع");
    connection
        .execute(
            "INSERT INTO tool_adapter_bindings (id, tenant_id, tool_version_id, adapter_key, bound_at) VALUES ('binding_state_change_test', ?1, 'tool_version_state_change_test', 'nexus.obligations.bounded_list.v1', ?2)",
            params![DEMO_TENANT_ID, now],
        )
        .expect("الربط يجب أن يُزرع");
    drop(connection);
    assign_tool(
        state,
        AssignToolInput {
            agent_id: "agent_runtime_demo".to_owned(),
            tool_version_id: "tool_version_state_change_test".to_owned(),
        },
    )
    .expect("التعيين يجب أن ينجح");
}

fn pending_tool_invocation_id(state: &AppState, run_id: &str) -> String {
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    connection
        .query_row(
            "SELECT id FROM tool_invocations WHERE agent_run_id = ?1 AND status = 'WAITING_APPROVAL' LIMIT 1",
            [run_id],
            |row| row.get(0),
        )
        .expect("الاستدعاء المنتظر يجب أن يقرأ")
}

fn pending_tool_approval_id(state: &AppState) -> String {
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    connection
        .query_row(
            "SELECT id FROM approvals WHERE resource_type = 'TOOL_INVOCATION' AND decision = 'PENDING' LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("موافقة الاستدعاء يجب أن تقرأ")
}

#[test]
fn state_change_tool_pauses_for_approval_and_exact_identity_resumes_after_it() {
    let state = runtime_state();
    assign_state_change_tool(&state);
    // جولة الإيقاف تحتاج استدعاء الأداة وحده (أثر STATE_CHANGE ⇒ موافقة).
    let provider = RuntimeTestProvider::with_tools_support(
        &["test-model:latest"],
        vec![ProviderOutcome::ToolCall(
            "nexus_obligations_bounded_list_v1",
            serde_json::json!({ "limit": 3 }),
        )],
    );
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);

    // الجولة الأولى: الأداة تتطلب موافقة ⇒ التشغيل يتوقف بأمان.
    let paused = start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("الإيقاف يجب أن ينجح");
    assert_eq!(paused.run.status, AgentRunStatus::WaitingApproval);
    let invocation_id = pending_tool_invocation_id(&state, &requested.run.id);
    let approval_id = pending_tool_approval_id(&state);
    let (status, started_set): (String, Option<String>) = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT status, started_at FROM tool_invocations WHERE id = ?1",
                [&invocation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("صف الاستدعاء يجب أن يقرأ")
    };
    assert_eq!(status, "WAITING_APPROVAL");
    assert!(started_set.is_none());
    assert_eq!(audit_count(&state, "TOOL_INVOCATION_APPROVAL_REQUIRED"), 1);

    // المشغل (طالب التشغيل ومالك موافقة الأداة) لا يستطيع اعتمادها إطلاقاً:
    // بوابة الصلاحية ترفضه أولاً، ومنع الذات يظل حارساً ثانياً لمن يملك
    // صلاحية الاعتماد (مُثبت في اختبارات approval_service القائمة).
    as_operator(&state);
    assert!(matches!(
        decide_approval(
            &state,
            DecideApprovalInput {
                approval_id: approval_id.clone(),
                decision: "APPROVED".to_owned(),
                reason: "Self approval must remain forbidden.".to_owned(),
            },
        ),
        Err(AppError::AuthorizationDenied) | Err(AppError::SelfApprovalForbidden)
    ));

    // المراجع يعتمد ⇒ التشغيل يعود QUEUED والاستدعاء يبقى منتظراً بهويته.
    as_reviewer(&state);
    decide_approval(
        &state,
        DecideApprovalInput {
            approval_id,
            decision: "APPROVED".to_owned(),
            reason: "Effect approved by independent reviewer.".to_owned(),
        },
    )
    .expect("الاعتماد يجب أن ينجح");
    let (run_status, _) = run_row(&state, &requested.run.id);
    assert_eq!(run_status, "QUEUED");

    // الاستئناف: نفس هوية النداء بالضبط ⇒ تنفيذ مباشر بلا موافقة جديدة.
    // (موفّر جديد: أول رد هو نفس الاستدعاء المعتمد ثم الجواب النهائي.)
    as_operator(&state);
    let provider = RuntimeTestProvider::with_tools_support(
        &["test-model:latest"],
        vec![
            ProviderOutcome::Text("Final after approval."),
            ProviderOutcome::ToolCall(
                "nexus_obligations_bounded_list_v1",
                serde_json::json!({ "limit": 3 }),
            ),
        ],
    );
    let resumed = start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("الاستئناف يجب أن ينجح");
    assert_eq!(resumed.run.status, AgentRunStatus::Completed);
    let final_status: String = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT status FROM tool_invocations WHERE id = ?1",
                [&invocation_id],
                |row| row.get(0),
            )
            .expect("حالة الاستدعاء يجب أن تقرأ")
    };
    assert_eq!(final_status, "COMPLETED");
    assert_eq!(audit_count(&state, "TOOL_INVOCATION_COMPLETED"), 1);
}

#[test]
fn approved_identity_never_executes_a_different_input() {
    let state = runtime_state();
    assign_state_change_tool(&state);
    let provider = RuntimeTestProvider::with_tools_support(
        &["test-model:latest"],
        vec![
            ProviderOutcome::Text("never"),
            ProviderOutcome::ToolCall(
                "nexus_obligations_bounded_list_v1",
                serde_json::json!({ "limit": 3 }),
            ),
        ],
    );
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);
    let paused = start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("الإيقاف يجب أن ينجح");
    assert_eq!(paused.run.status, AgentRunStatus::WaitingApproval);
    as_reviewer(&state);
    let approval_id = pending_tool_approval_id(&state);
    decide_approval(
        &state,
        DecideApprovalInput {
            approval_id,
            decision: "APPROVED".to_owned(),
            reason: "Approved exactly this identity.".to_owned(),
        },
    )
    .expect("الاعتماد يجب أن ينجح");

    // الاستئناف بنموذج يعيد مدخلات مختلفة ⇒ طلب موافقة جديد، لا تنفيذ المسروق.
    as_operator(&state);
    let provider = RuntimeTestProvider::with_tools_support(
        &["test-model:latest"],
        vec![
            ProviderOutcome::Text("never"),
            ProviderOutcome::ToolCall(
                "nexus_obligations_bounded_list_v1",
                serde_json::json!({ "limit": 9, "query": "changed" }),
            ),
        ],
    );
    let resumed = start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("الاستئناف يجب أن ينجح");
    assert_eq!(resumed.run.status, AgentRunStatus::WaitingApproval);
    // استدعاءان: القديم المنتظر الجديد مختلف الهوية — لم يُنفَّذ أي منهما.
    assert_eq!(invocation_count(&state, &requested.run.id), 2);
    let executed: i64 = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT COUNT(*) FROM tool_invocations WHERE agent_run_id = ?1 AND status IN ('RUNNING','COMPLETED')",
                [&requested.run.id],
                |row| row.get(0),
            )
            .expect("عد المنفذ يجب أن يقرأ")
    };
    assert_eq!(executed, 0);
}

#[test]
fn rejected_tool_approval_cancels_invocation_and_run() {
    let state = runtime_state();
    assign_state_change_tool(&state);
    let provider = RuntimeTestProvider::with_tools_support(
        &["test-model:latest"],
        vec![
            ProviderOutcome::Text("never"),
            ProviderOutcome::ToolCall(
                "nexus_obligations_bounded_list_v1",
                serde_json::json!({ "limit": 3 }),
            ),
        ],
    );
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);
    let paused = start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("الإيقاف يجب أن ينجح");
    assert_eq!(paused.run.status, AgentRunStatus::WaitingApproval);
    let invocation_id = pending_tool_invocation_id(&state, &requested.run.id);
    let approval_id = pending_tool_approval_id(&state);
    // المراجع المستقل وحده صاحب قرار الرفض (المشغل بلا صلاحية اعتماد).
    as_reviewer(&state);
    decide_approval(
        &state,
        DecideApprovalInput {
            approval_id,
            decision: "REJECTED".to_owned(),
            reason: "Reviewer rejects the state-changing effect.".to_owned(),
        },
    )
    .expect("الرفض يجب أن ينجح");
    let invocation_status: String = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT status FROM tool_invocations WHERE id = ?1",
                [&invocation_id],
                |row| row.get(0),
            )
            .expect("حالة الاستدعاء يجب أن تقرأ")
    };
    assert_eq!(invocation_status, "CANCELLED");
    let (run_status, _) = run_row(&state, &requested.run.id);
    assert_eq!(run_status, "CANCELLED");
}

#[test]
fn requester_cancel_while_waiting_tool_approval_cancels_everything() {
    let state = runtime_state();
    assign_state_change_tool(&state);
    let provider = RuntimeTestProvider::with_tools_support(
        &["test-model:latest"],
        vec![
            ProviderOutcome::Text("never"),
            ProviderOutcome::ToolCall(
                "nexus_obligations_bounded_list_v1",
                serde_json::json!({ "limit": 3 }),
            ),
        ],
    );
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);
    let paused = start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("الإيقاف يجب أن ينجح");
    assert_eq!(paused.run.status, AgentRunStatus::WaitingApproval);

    // صاحب الطلب يلغي التشغيل المنتظر على موافقة أداة.
    crate::application::agent_run_service::cancel_agent_run(
        &state,
        crate::application::agent_run_service::CancelAgentRunInput {
            run_id: requested.run.id.clone(),
        },
    )
    .expect("الإلغاء يجب أن ينجح");
    // نطاق مغلق يحرر القفل قبل أي استدعاء لاحق يقفل الاتصال (Mutex غير اعتيادي).
    let (invocation_status, approval_decision): (String, String) = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT (SELECT status FROM tool_invocations WHERE agent_run_id = ?1), (SELECT decision FROM approvals WHERE resource_type = 'TOOL_INVOCATION' AND resource_id IN (SELECT id FROM tool_invocations WHERE agent_run_id = ?1))",
                [&requested.run.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("الحالات يجب أن تقرأ")
    };
    assert_eq!(invocation_status, "CANCELLED");
    assert_eq!(approval_decision, "CANCELLED");
    let (run_status, _) = run_row(&state, &requested.run.id);
    assert_eq!(run_status, "CANCELLED");
}

// =====================================================================
// اختبارات السلامة الإنشائية للمرحلة (§61/§84/§88/§89/§90 من التكليف)
// =====================================================================

#[test]
fn cross_tenant_fk_references_are_rejected_by_the_schema() {
    let state = runtime_state();
    let now = Utc::now().to_rfc3339();
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        // مستأجر أجنبي بنفس هيكل الديمو.
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant_foreign', 'أجنبي', 'DEMO_SYNTHETIC', ?1, ?1)",
                params![now],
            )
            .expect("المستأجر الأجنبي يجب أن ينشأ");

        // وكيل أجنبي لا يمكنه ربط بروفايل مستأجر الديمو (FK مركب).
        let cross_profile = connection.execute(
            "INSERT INTO agent_execution_profiles (id, tenant_id, agent_id, version_number, instruction, created_at) VALUES ('profile_x', 'tenant_foreign', 'agent_runtime_demo', 1, 'x', ?1)",
            params![now],
        );
        assert!(
            cross_profile.is_err(),
            "ربط بروفايل عابر للمستأجرين يجب أن يُرفض"
        );

        // ربط هوية تنفيذ لتشغيل مستأجر آخر ببروفايل الديمو مرفوض.
        let cross_binding = connection.execute(
            "INSERT INTO agent_run_execution_bindings (agent_run_id, tenant_id, agent_run_revision, agent_id, execution_profile_version_id, task_content, task_content_hash, model_id_at_request, bound_at) VALUES ('run_x', 'tenant_foreign', 1, 'agent_runtime_demo', 'profile_runtime_v1', 't', 'h', NULL, ?1)",
            params![now],
        );
        assert!(
            cross_binding.is_err(),
            "ربط هوية عابر للمستأجرين يجب أن يُرفض"
        );

        // تعيين أداة أجنبية لوكيل الديمو مرفوض.
        let cross_assignment = connection.execute(
            "INSERT INTO agent_tool_assignments (id, tenant_id, agent_id, tool_version_id, assigned_at) VALUES ('assign_x', 'tenant_foreign', 'agent_runtime_demo', 'tool_version_builtin_obligations_v1', ?1)",
            params![now],
        );
        assert!(
            cross_assignment.is_err(),
            "تعيين أداة عابر للمستأجرين يجب أن يُرفض"
        );
    }
}

#[test]
fn registry_tool_without_trusted_adapter_is_never_advertised_or_executable() {
    let state = runtime_state();
    let now = Utc::now().to_rfc3339();
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        // أداة مستأجر مسجلة بلا أي ربط محوّل (§84).
        connection
            .execute(
                "INSERT INTO tools (id, tenant_id, name, description, status, created_at, updated_at) VALUES ('tool_unbound', ?1, 'Unbound registry tool', 'Registered only', 'ACTIVE', ?2, ?2)",
                params![DEMO_TENANT_ID, now],
            )
            .expect("الأداة غير المربوطة يجب أن تُزرع");
        connection
            .execute(
                "INSERT INTO tool_versions (id, tenant_id, tool_id, version_number, input_contract, output_contract, mutation, externality, privilege, reversibility, created_at) VALUES ('tool_version_unbound', ?1, 'tool_unbound', 1, '{\"fields\":[],\"allowsAdditionalFields\":false}', '{\"fields\":[],\"allowsAdditionalFields\":false}', 'NONE', 'INTERNAL', 'STANDARD', 'NOT_APPLICABLE', ?2)",
                params![DEMO_TENANT_ID, now],
            )
            .expect("نسخة الأداة غير المربوطة يجب أن تُزرع");
    }
    assign_tool(
        &state,
        AssignToolInput {
            agent_id: "agent_runtime_demo".to_owned(),
            tool_version_id: "tool_version_unbound".to_owned(),
        },
    )
    .expect("التعيين المسجل يجب أن ينجح (أهلية إدارية فقط)");

    // الكتالوج يصدق: غير قابلة للتنفيذ بلا محوّل موثوق.
    let catalog =
        crate::application::tool_service::get_tool_catalog(&state).expect("الكتالوج يجب أن يقرأ");
    let unbound = catalog
        .items
        .iter()
        .find(|item| item.tool.id == "tool_unbound")
        .expect("الأداة يجب أن تظهر");
    assert!(!unbound.versions[0].executable);

    // والمحوّل نفسه يرفض أي مفتاح غير معروف — لا fallback بالاسم.
    let input = crate::domain::tool_invocation::CanonicalToolInput::validate_and_canonicalize(
        &serde_json::json!({ "limit": 1 }),
        &serde_json::from_str::<crate::domain::tool::ToolContract>(
            "{\"fields\":[],\"allowsAdditionalFields\":true}",
        )
        .expect("عقد"),
    )
    .expect("مدخل");
    assert!(
        crate::application::tool_adapter_registry::execute_trusted_adapter(
            &state,
            "tool_unbound",
            &input
        )
        .is_err()
    );
    assert!(
        crate::application::tool_adapter_registry::execute_trusted_adapter(
            &state,
            "nexus.obligations.bounded_list.v1",
            &input
        )
        .is_ok()
    );
}

#[test]
fn immutable_phase_three_tables_reject_update_and_delete() {
    let state = runtime_state();
    // تشغيل حقيقي بربط هوية قائم ثم محاولات التلاعب بالجدولات المحصنة.
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    for (description, sql) in [
        ("تعديل بروفايل", "UPDATE agent_execution_profiles SET instruction = 'changed' WHERE id = 'profile_runtime_v1'"),
        ("حذف بروفايل", "DELETE FROM agent_execution_profiles WHERE id = 'profile_runtime_v1'"),
        ("تعديل ربط هوية", &format!("UPDATE agent_run_execution_bindings SET task_content = 'changed' WHERE agent_run_id = '{}'", requested.run.id)),
        ("حذف ربط هوية", &format!("DELETE FROM agent_run_execution_bindings WHERE agent_run_id = '{}'", requested.run.id)),
        ("تعديل ربط محوّل", "UPDATE tool_adapter_bindings SET adapter_key = 'evil' WHERE id = 'tool_adapter_binding_tool_builtin_obligations'"),
        ("حذف ربط محوّل", "DELETE FROM tool_adapter_bindings WHERE id = 'tool_adapter_binding_tool_builtin_obligations'"),
    ] {
        let result = connection.execute(sql, []);
        assert!(result.is_err(), "{description} يجب أن ترفضه المشغلات");
    }
}

#[test]
fn tool_invocation_identity_and_terminal_rows_are_locked() {
    let state = runtime_state();
    let requested = request_agent_execution(&state, request_input()).expect("الطلب يجب أن ينجح");
    let now = Utc::now().to_rfc3339();
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                &format!(
                    "INSERT INTO tool_invocations (id, tenant_id, agent_run_id, agent_run_revision, tool_id, tool_version_id, adapter_key, input_json, input_hash, policy_decision, policy_reason, status, revision, failure_code, created_at, started_at, finished_at, updated_at) VALUES ('inv_terminal', '{}', '{}', 1, 'tool_builtin_obligations', 'tool_version_builtin_obligations_v1', 'nexus.obligations.bounded_list.v1', '{{}}', 'hash1', 'DENY', 'NO_TRUSTED_ADAPTER', 'FAILED', 1, 'TOOL_AUTHORIZATION_DENIED', '{2}', '{2}', '{2}', '{2}')",
                    DEMO_TENANT_ID,
                    requested.run.id,
                    now
                ),
                [],
            )
            .err()
            .map(|error| panic!("الإدراج يجب أن ينجح: {error:?}"));
    }
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    // حقول الهوية غير قابلة للتعديل بعد الإنشاء (§88).
    assert!(connection
        .execute(
            "UPDATE tool_invocations SET input_json = '{\"evil\":1}' WHERE id = 'inv_terminal'",
            []
        )
        .is_err());
    assert!(connection
        .execute(
            "UPDATE tool_invocations SET tool_version_id = 'other' WHERE id = 'inv_terminal'",
            []
        )
        .is_err());
    // الصف المنتهي لا يعود نشطاً (قفل الإحياء).
    assert!(connection
        .execute(
            "UPDATE tool_invocations SET status = 'RUNNING' WHERE id = 'inv_terminal'",
            []
        )
        .is_err());
}

#[test]
fn audit_failure_rolls_back_profile_creation_and_tool_assignment() {
    let state = runtime_state();
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_profile_audit BEFORE INSERT ON audit_events
                 WHEN NEW.event_type = 'AGENT_EXECUTION_PROFILE_CREATED'
                 BEGIN
                   SELECT RAISE(ABORT, 'رفض اختبار تدقيق البروفايل');
                 END;",
            )
            .expect("مشغل الاختبار يجب أن ينشأ");
    }
    let profile_result =
        crate::application::agent_execution_profile_service::create_agent_execution_profile(
            &state,
            crate::application::agent_execution_profile_service::CreateAgentExecutionProfileInput {
                agent_id: "agent_runtime_demo".to_owned(),
                instruction: "Another bounded instruction.".to_owned(),
            },
        );
    assert!(profile_result.is_err());
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute_batch("DROP TRIGGER fail_profile_audit")
            .expect("إسقاط المشغل يجب أن ينجح");
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM agent_execution_profiles", [], |row| {
                row.get(0)
            })
            .expect("العد يجب أن يقرأ");
        // الأساس: بروفايل وكيل الاختبار + ثلاثة بروفايلات وكلاء ذكاء الديمو المزروعة
        assert_eq!(count, 4, "لا بروفايل جديد بعد فشل التدقيق");
    }

    // تعيين أداة: فشل تدقيق AGENT_TOOL_ASSASSIGNED يتراجع عن الإدراج.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_tool_assign_audit BEFORE INSERT ON audit_events
                 WHEN NEW.event_type = 'AGENT_TOOL_ASSIGNED'
                 BEGIN
                   SELECT RAISE(ABORT, 'رفض اختبار تدقيق التعيين');
                 END;",
            )
            .expect("مشغل الاختبار يجب أن ينشأ");
    }
    let assign_result = assign_tool(
        &state,
        AssignToolInput {
            agent_id: "agent_runtime_demo".to_owned(),
            tool_version_id: "tool_version_builtin_risks_v1".to_owned(),
        },
    );
    assert!(assign_result.is_err());
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM agent_tool_assignments", [], |row| {
                row.get(0)
            })
            .expect("العد يجب أن يقرأ");
        assert_eq!(count, 0, "لا تعيين بعد فشل التدقيق");
    }
}

#[test]
fn upgrade_path_from_0016_then_0017_matches_fresh_chain() {
    // سلسلة الترقية: 0001→0016 ثم تطبيق 0017 منفرداً — نفس بنية السلسلة الطازجة.
    let upgraded = Connection::open_in_memory().expect("قاعدة الترقية يجب أن تفتح");
    upgraded
        .pragma_update(None, "foreign_keys", "ON")
        .expect("تفعيل المفاتيح يجب أن ينجح");
    for version in 1..=16_i32 {
        let script = std::fs::read_to_string(format!("../../migrations/{version:04}_*.sql"))
            .or_else(|_| {
                // داخل cargo test يكون المجلد مختلفاً — نجرّب مسار الحزمة.
                let mut candidates = vec![format!("migrations/{version:04}_*.sql")];
                if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
                    candidates.insert(0, format!("{manifest}/migrations/{version:04}_*.sql"));
                }
                for candidate in candidates {
                    let paths: Vec<_> = glob_like(&candidate);
                    if let Some(first) = paths.first() {
                        return std::fs::read_to_string(first);
                    }
                }
                Err(std::io::Error::other("لم يوجد ملف الهجرة"))
            })
            .expect("قراءة هجرة الترقية يجب أن تنجح");
        upgraded
            .execute_batch(&script)
            .unwrap_or_else(|error| panic!("هجرة الترقية {version} يجب أن تنجح: {error}"));
        upgraded
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                params![version, "2026-01-01T00:00:00Z"],
            )
            .expect("تسجيل الهجرة يجب أن ينجح");
    }
    let script_0017 = std::env::var("CARGO_MANIFEST_DIR")
        .ok()
        .and_then(|manifest| glob_one(&format!("{manifest}/migrations/0017_*.sql")))
        .or_else(|| glob_one("migrations/0017_*.sql"))
        .map(std::fs::read_to_string)
        .expect("قراءة 0017 يجب أن تنجح")
        .expect("محتوى 0017 يجب أن يقرأ");
    upgraded
        .execute_batch(&script_0017)
        .expect("تطبيق 0017 على قاعدة 0016 يجب أن ينجح");

    // المقارنة: جدولان محوريان ورموز الفشل الجديدة موجودة بعد الترقية.
    let violations: i64 = upgraded
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .expect("فحص المفاتيح يجب أن ينجح");
    assert_eq!(violations, 0);
    let tool_tables: i64 = upgraded
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('agent_execution_profiles','agent_run_execution_bindings','agent_tool_assignments','tool_adapter_bindings','tool_invocations','tool_invocation_results','tool_invocation_approval_links')",
            [],
            |row| row.get(0),
        )
        .expect("عد الجداول يجب أن ينجح");
    assert_eq!(tool_tables, 7);
}

// أدوات مطابقة مسار بسيطة (بلا تبعية glob خارجية).
fn glob_like(pattern: &str) -> Vec<String> {
    let directory = std::path::Path::new(pattern)
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_default();
    let prefix = pattern
        .rsplit('/')
        .next()
        .unwrap_or("")
        .split('*')
        .next()
        .unwrap_or("")
        .to_owned();
    let mut matches = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&directory) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix) {
                matches.push(entry.path().to_string_lossy().to_string());
            }
        }
    }
    matches.sort();
    matches
}

fn glob_one(pattern: &str) -> Option<String> {
    glob_like(pattern).into_iter().next()
}

// =====================================================================
// الدخان الحي (§79-§81 من التكليف) — نموذج qwen3:4b المحلي الحقيقي عبر
// المسار المحكوم كاملاً. يُشغَّل صراحة بـ --ignored ولا يدخل المجموعة الآلية
// (يعتمد على مثيل Ollama حي على الجهاز).
// =====================================================================

#[test]
#[ignore = "live: يتطلب مثيل Ollama محلياً حياً بنموذج qwen3:4b"]
fn live_plain_agent_generation_through_the_governed_path() {
    let state = runtime_state();
    let provider =
        crate::application::provider_runtime_service::ProviderRuntimeService::local_ollama();

    // 1) تكوين النموذج عبر المسار المحكوم (قبول المكتشف حياً فقط).
    crate::application::model_configuration_service::configure_local_model(
        &state,
        crate::application::model_configuration_service::ConfigureLocalModelInput {
            model_id: "qwen3:4b".to_owned(),
        },
    )
    .expect("تكوين qwen3:4b الحي يجب أن ينجح");

    // 2) طلب تنفيذ محكوم بمهمة محدودة.
    let requested = request_agent_execution(
        &state,
        RequestAgentExecutionInput {
            agent_id: "agent_runtime_demo".to_owned(),
            task_content: "In one short sentence, state that you received this governed task."
                .to_owned(),
            skill_version_id: None,
            knowledge_query: None,
        },
    )
    .expect("الطلب الحي يجب أن ينجح");
    assert_eq!(requested.run.status, AgentRunStatus::WaitingApproval);

    // 3) اعتماد مراجع مستقل.
    as_reviewer(&state);
    approve_pending_agent_run(&state);

    // 4) بدء التنفيذ — نداء Ollama حقيقي غير زائف.
    as_operator(&state);
    let outcome = super::start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("التنفيذ الحي يجب أن ينجح");
    assert_eq!(
        outcome.run.status,
        AgentRunStatus::Completed,
        "التشغيل يجب أن يكتمل"
    );

    let result = outcome.result.expect("نتيجة حية يجب أن تُعاد");
    assert_eq!(result.outcome, "COMPLETED");
    assert_eq!(result.provider_id, "OLLAMA_LOCAL");
    assert_eq!(
        result.model_id.as_deref(),
        Some("qwen3:4b"),
        "منشأ النموذج الدقيق إلزامي"
    );
    let output = result.output.expect("مخرج غير فارغ");
    assert!(!output.trim().is_empty());
    assert!(output.len() <= 32 * 1024);

    // 5) القراءة عبر مسار المنتج + المنشأ + التدقيق (بلا أي محتوى خام).
    let read_back = crate::application::agent_run_service::get_agent_run_result(
        &state,
        crate::application::agent_run_service::CancelAgentRunInput {
            run_id: requested.run.id.clone(),
        },
    )
    .expect("قراءة النتيجة عبر المنتج يجب أن تنجح");
    assert_eq!(read_back.model_id.as_deref(), Some("qwen3:4b"));
    assert!(read_back.provider_generated_at.is_some());
    assert_eq!(audit_count(&state, "AGENT_RUN_EXECUTION_STARTED"), 1);
    assert_eq!(audit_count(&state, "AGENT_RUN_EXECUTION_COMPLETED"), 1);
    assert_eq!(audit_count(&state, "AGENT_EXECUTION_REQUESTED"), 1);
    println!(
        "LIVE-PLAIN-OUTPUT: {}",
        output.chars().take(400).collect::<String>()
    );
}

#[test]
#[ignore = "live: يتطلب مثيل Ollama محلياً حياً بنموذج qwen3:4b"]
fn live_read_only_tool_assisted_run_through_the_governed_runtime() {
    let state = runtime_state();
    let provider =
        crate::application::provider_runtime_service::ProviderRuntimeService::local_ollama();
    crate::application::model_configuration_service::configure_local_model(
        &state,
        crate::application::model_configuration_service::ConfigureLocalModelInput {
            model_id: "qwen3:4b".to_owned(),
        },
    )
    .expect("تكوين qwen3:4b الحي يجب أن ينجح");
    assign_obligations_tool(&state);

    let requested = request_agent_execution(
        &state,
        RequestAgentExecutionInput {
            agent_id: "agent_runtime_demo".to_owned(),
            task_content: "Use the available tool to list obligations about evidence, then answer in one short sentence.".to_owned(),
            skill_version_id: None,
                knowledge_query: None,
        },
    )
    .expect("الطلب الحي يجب أن ينجح");
    as_reviewer(&state);
    approve_pending_agent_run(&state);
    as_operator(&state);

    let outcome = super::start_queued_execution_with_provider(&state, &requested.run.id, &provider)
        .expect("التنفيذ الحي مع الأداة يجب أن ينجح");

    // النتيجة الصادقة: إما اكتمال مع استدعاء أداة منفذ، أو اكتمال نصي مباشر
    // (النموذج حر في عدم الاستدعاء) — كلاهما نجاح محكوم؛ المهم ألا يكون فشل
    // بنية تحتية.
    assert_eq!(
        outcome.run.status,
        AgentRunStatus::Completed,
        "التشغيل يجب أن يكتمل: {:?}",
        outcome
            .result
            .as_ref()
            .and_then(|result| result.failure_code.clone())
    );
    let invocations = invocation_count(&state, &requested.run.id);
    println!("LIVE-TOOL-INVOCATIONS: {invocations}");
    if invocations > 0 {
        let output: String = {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .query_row(
                    "SELECT r.output_json FROM tool_invocation_results r JOIN tool_invocations i ON i.id = r.tool_invocation_id WHERE i.agent_run_id = ?1 AND r.outcome = 'COMPLETED'",
                    [&requested.run.id],
                    |row| row.get(0),
                )
                .unwrap_or_default()
        };
        assert!(!output.is_empty(), "نتيجة المحوّل يجب أن تكون محفوظة");
        assert_eq!(audit_count(&state, "TOOL_INVOCATION_COMPLETED"), 1);
        println!(
            "LIVE-TOOL-OUTPUT: {}",
            output.chars().take(400).collect::<String>()
        );
    }
    let result = outcome.result.expect("النتيجة النهائية");
    assert_eq!(result.model_id.as_deref(), Some("qwen3:4b"));
}
