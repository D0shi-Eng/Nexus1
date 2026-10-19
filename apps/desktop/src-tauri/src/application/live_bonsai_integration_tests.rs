// =====================================================================
// اختبار تكامل حي — Bonsai 2 عبر المسار المحكوم في NEXUS
// ---------------------------------------------------------------------
// هذا الاختبار **ليس** جزءاً من الجناح الافتراضي: `#[ignore]` لأن صحته مشروطة
// بخادم استنتاج محلي يعمل فعلاً على هذا الجهاز. يُشغَّل صراحةً:
//
//   cargo test --lib -- --ignored --nocapture live_bonsai
//
// ما يثبته فعلياً (لا محاكاة، ولا خادم وهمي، ولا مزوّد اختبار):
//   1. تسجيل موفر متوافق مع OpenAI على 127.0.0.1:8080 (llama.cpp يحمل Bonsai 2).
//   2. اختبار اتصال حقيقي ⇒ CONNECTED.
//   3. استكشاف النماذج حيّاً والتقاط **المعرّف الذي يعلنه الخادم فعلاً** — وهو
//      مسار ملف ويندوز بفواصل عكسية. هذا الاستكشاف كان **مستحيلاً** قبل إصلاح
//      `ModelId`: معرّف واحد مرفوض كان يُسقط تعداد النماذج كاملاً.
//   4. انتخاب النموذج وحفظه في السجل.
//   5. طلب تنفيذ محكوم + اعتماد مستقل، ثم تنفيذ فعلي عبر منسق الإنتاج
//      `start_queued_execution` — أي أن المحوّل يُبنى من سجل الموفرين، ويُولَّد
//      نص حقيقي من النموذج، ولا يُحقن أي مزوّد بديل.
//   6. إثبات المنشأ: معرّف الموفر ومعرّف النموذج في نتيجة التنفيذ.
//   7. إثبات التدقيق: حدثا البدء والإكمال.
//   8. إثبات ربط هوية الوجهة (هجرة 0024): provider/kind/بصمة/هاش الربط.
//
// لا يرسل هذا الاختبار أي وثيقة أو بيانات مستأجر: مطالبة اصطناعية واحدة قصيرة.
// =====================================================================

use chrono::Utc;
use rusqlite::{params, Connection};

use crate::{
    application::{
        agent_execution_request_service::{request_agent_execution, RequestAgentExecutionInput},
        ai_provider_registry_service::{
            configure_provider, discover_models, select_model, set_credential, test_provider,
            ConfigureAiProviderInput, SelectAiModelInput,
        },
        approval_service::{decide_approval, DecideApprovalInput},
    },
    domain::{
        models::AgentRunStatus,
        provider_registry::{AiProviderConnectionState, SetAiProviderCredentialInput},
    },
    infrastructure::database::{
        initialize_demo_database, AppState, DEMO_OPERATOR_USER_ID, DEMO_REVIEWER_USER_ID,
        DEMO_TENANT_ID,
    },
};

/// الوجهة الافتراضية: خادم Bonsai 2 المحلي. قابلة للتجاوز بلا تعديل كود.
const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8080";
const PROVIDER_ID: &str = "BONSAI_LOCAL";
const AGENT_ID: &str = "agent_live_bonsai";
const PROFILE_ID: &str = "profile_live_bonsai_v1";
/// مفتاح موضعي غير حقيقي: llama.cpp لا يطلب مصادقة، والقيمة تُستهلك محلياً فقط
/// ولا تُسجَّل. وجودها يرضي عقد «الموفر المتوافق مع OpenAI» بلا كشف أي سر حقيقي.
const LOCAL_PLACEHOLDER_CREDENTIAL: &str = "local-no-auth-placeholder";

fn base_url() -> String {
    std::env::var("NEXUS_LIVE_LOCAL_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_owned())
}

fn live_state() -> AppState {
    let connection = Connection::open_in_memory().expect("قاعدة الاختبار يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("تفعيل المفاتيح يجب أن ينجح");
    initialize_demo_database(&connection).expect("تهيئة الديمو يجب أن تنجح");
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "INSERT INTO agents (id, tenant_id, name, description, status, created_at, updated_at) VALUES (?1, ?2, 'Live Bonsai Agent', 'Governed live runtime agent', 'ACTIVE', ?3, ?3)",
            params![AGENT_ID, DEMO_TENANT_ID, now],
        )
        .expect("الوكيل يجب أن يُزرع");
    connection
        .execute(
            "INSERT INTO agent_execution_profiles (id, tenant_id, agent_id, version_number, instruction, created_at) VALUES (?1, ?2, ?3, 1, 'Answer the bounded task in one short sentence.', ?4)",
            params![PROFILE_ID, DEMO_TENANT_ID, AGENT_ID, now],
        )
        .expect("البروفايل يجب أن يُزرع");
    let state = AppState::test_instance(
        connection,
        DEMO_TENANT_ID.to_owned(),
        DEMO_OPERATOR_USER_ID.to_owned(),
    );
    // مخزن أسرار في الذاكرة: لا نكتب أي ملف على قرص المستخدم من اختبار حي.
    state.set_credential_vault_for_tests(std::sync::Arc::new(
        crate::infrastructure::credential_vault::InMemoryCredentialVault::new(),
    ));
    state
}

fn pending_approval_id(state: &AppState) -> String {
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    connection
        .query_row(
            "SELECT id FROM approvals WHERE resource_type = 'AGENT_RUN' AND decision = 'PENDING' ORDER BY rowid DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .expect("طلب الاعتماد يجب أن يوجد")
}

#[test]
#[ignore = "يحتاج خادم استنتاج محلياً على 127.0.0.1:8080 (Bonsai 2 عبر llama.cpp)"]
fn live_bonsai_runs_through_the_governed_nexus_path() {
    let state = live_state();
    let base_url = base_url();
    println!("[LIVE] destination = {base_url}");

    // ===== 1) تسجيل الموفر =====
    configure_provider(
        &state,
        ConfigureAiProviderInput {
            provider_id: PROVIDER_ID.to_owned(),
            kind: crate::domain::provider_registry::AiProviderKind::OpenAiCompatible,
            display_name: "Bonsai 2 (local llama.cpp)".to_owned(),
            base_url: base_url.clone(),
            enabled: true,
        },
    )
    .expect("تسجيل الموفر المتوافق مع OpenAI يجب أن ينجح");
    println!("[LIVE] provider configured: {PROVIDER_ID}");

    // llama.cpp لا يطلب مفتاحاً؛ العقد الحالي يطلب مادة سرية لكل موفر غير محلي
    // النوع، فنضع قيمة موضعية محلية حتى يكتمل العقد (لا سر حقيقي ولا خروج للشبكة).
    set_credential(
        &state,
        SetAiProviderCredentialInput {
            provider_id: PROVIDER_ID.to_owned(),
            credential: LOCAL_PLACEHOLDER_CREDENTIAL.to_owned(),
        },
    )
    .expect("تثبيت المادة السرية الموضعية يجب أن ينجح");

    // ===== 2) اختبار اتصال حقيقي =====
    let probe = test_provider(&state, PROVIDER_ID).expect("اختبار الاتصال يجب أن يُنفَّذ");
    println!(
        "[LIVE] probe state={:?} latency_ms={} models={} error={:?}",
        probe.connection_state,
        probe.latency_ms,
        probe.discovered_model_count,
        probe.safe_error_code
    );
    assert_eq!(
        probe.connection_state,
        AiProviderConnectionState::Connected,
        "الخادم المحلي يجب أن يُعلن CONNECTED (رمز الخطأ: {:?})",
        probe.safe_error_code
    );
    assert!(probe.discovered_model_count >= 1);

    // ===== 3) استكشاف النماذج — التقاط المعرّف الحقيقي =====
    let discovery = discover_models(&state, PROVIDER_ID).expect("الاستكشاف يجب أن يُنفَّذ");
    assert!(discovery.safe_error_code.is_none());
    let model_id = discovery
        .models
        .first()
        .map(|model| model.model_id.clone())
        .expect("الخادم يجب أن يعلن نموذجاً واحداً على الأقل");
    println!("[LIVE] discovered model id = [{model_id}]");
    assert_eq!(discovery.models.len(), 1, "خادم Bonsai يقدّم نموذجاً واحداً");
    // المعرّف الذي يعلنه llama.cpp مسار ملف؛ نجاح الوصول إلى هنا يثبت أن النطاق
    // المغلق الأبجدي القديم لم يعد يُسقط التعداد.
    assert!(
        model_id.contains('\\') || model_id.contains('/') || model_id.contains(':'),
        "المعرّف المُعلن يجب أن يُقبل كما هو: {model_id}"
    );

    // ===== 4) الانتخاب والحفظ =====
    let selected = select_model(
        &state,
        SelectAiModelInput {
            provider_id: PROVIDER_ID.to_owned(),
            model_id: model_id.clone(),
        },
    )
    .expect("انتخاب النموذج يجب أن ينجح");
    assert_eq!(
        selected.selected_model_id.as_deref(),
        Some(model_id.as_str())
    );
    assert_eq!(selected.selected_provider_id.as_deref(), Some(PROVIDER_ID));
    println!("[LIVE] model selected and persisted");

    // ===== 5) طلب تنفيذ محكوم + اعتماد مستقل + تنفيذ فعلي =====
    let requested = request_agent_execution(
        &state,
        RequestAgentExecutionInput {
            agent_id: AGENT_ID.to_owned(),
            task_content: "Reply with the word NEXUS and one short sentence.".to_owned(),
            skill_version_id: None,
            knowledge_query: None,
        },
    )
    .expect("طلب التنفيذ يجب أن ينجح");
    assert_eq!(requested.run.status, AgentRunStatus::WaitingApproval);

    // ربط هوية الوجهة مثبَّت لحظة الطلب (0024).
    {
        let binding =
            crate::application::agent_execution_request_service::get_agent_run_execution_binding(
                &state,
                requested.run.id.clone(),
            )
            .expect("قراءة الربط يجب أن تنجح");
        println!(
            "[LIVE] binding provider={:?} kind={:?} endpoint={:?} hash={:?}",
            binding.provider_id_at_request,
            binding.provider_kind_at_request,
            binding.endpoint_identity_at_request,
            binding.approval_binding_hash
        );
        assert_eq!(binding.provider_id_at_request.as_deref(), Some(PROVIDER_ID));
        assert_eq!(
            binding.model_id_at_request.as_deref(),
            Some(model_id.as_str())
        );
        assert!(binding.endpoint_identity_at_request.is_some());
        assert!(binding.approval_binding_hash.is_some());
    }

    // اعتماد مستقل بمستخدم مختلف (منع الذات سارٍ).
    state.set_active_user_for_tests(DEMO_REVIEWER_USER_ID);
    let approval_id = pending_approval_id(&state);
    let decision = decide_approval(
        &state,
        DecideApprovalInput {
            approval_id,
            decision: "APPROVED".to_owned(),
            reason: "Independent reviewer approved the bounded live run.".to_owned(),
        },
    )
    .expect("الاعتماد يجب أن ينجح");
    assert_eq!(decision.decision, "APPROVED");
    state.set_active_user_for_tests(DEMO_OPERATOR_USER_ID);

    // التنفيذ عبر مسار الإنتاج: لا مزوّد محقون — المحوّل يُبنى من سجل الموفرين.
    let started = std::time::Instant::now();
    let outcome = crate::application::agent_runtime_coordinator::start_queued_execution(
        &state,
        &requested.run.id,
    )
    .expect("التنفيذ المحكوم يجب أن ينجح");
    let elapsed_ms = started.elapsed().as_millis();
    println!(
        "[LIVE] execution state={:?} elapsed_ms={elapsed_ms}",
        outcome.run.status
    );
    assert_eq!(outcome.run.status, AgentRunStatus::Completed);

    // ===== 6) إثبات المنشأ من الوجهة الفعلية =====
    let result = outcome.result.as_ref().expect("نتيجة التنفيذ يجب أن تُعاد");
    println!(
        "[LIVE] result provider_id={} model_id={:?} outcome={} output_chars={}",
        result.provider_id,
        result.model_id,
        result.outcome,
        result.output.as_deref().map(str::len).unwrap_or(0)
    );
    assert_eq!(
        result.provider_id, PROVIDER_ID,
        "منشأ الموفر يجب أن يُشتق من المحوّل المستعمل فعلاً"
    );
    assert_eq!(result.model_id.as_deref(), Some(model_id.as_str()));
    assert_eq!(result.outcome, "COMPLETED");
    let output = result.output.as_deref().expect("نص التوليد يجب أن يوجد");
    assert!(!output.trim().is_empty(), "التوليد يجب ألا يكون فارغاً");
    println!(
        "[LIVE] generated output (first 200 chars): {}",
        &output.chars().take(200).collect::<String>()
    );

    // ===== 7) إثبات التدقيق =====
    let (started_events, completed_events) = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let count = |event_type: &str| -> i64 {
            connection
                .query_row(
                    "SELECT COUNT(*) FROM audit_events WHERE event_type = ?1",
                    [event_type],
                    |row| row.get(0),
                )
                .expect("عد التدقيق يجب أن ينجح")
        };
        (
            count("AGENT_RUN_EXECUTION_STARTED"),
            count("AGENT_RUN_EXECUTION_COMPLETED"),
        )
    };
    println!("[LIVE] audit started={started_events} completed={completed_events}");
    assert_eq!(started_events, 1);
    assert_eq!(completed_events, 1);

    // ===== 8) إثبات أن التنفيذ مرّ فعلاً على الخادم المحلي =====
    // نص التوليد غير حتمي؛ الدليل الحتمي هو طول النص + حالة الإكمال + المنشأ
    // + زمن التنفيذ. لا نؤكد صياغة الرد إطلاقاً.
    println!("[LIVE] VERIFIED_LIVE: Bonsai 2 generated a real completion through the governed NEXUS path");
}
