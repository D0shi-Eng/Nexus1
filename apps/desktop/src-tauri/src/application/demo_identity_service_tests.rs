use rusqlite::Connection;

use super::{
    list_demo_actors, switch_demo_actor, SwitchDemoActorInput, REQUESTER_RESPONSIBILITY,
    REVIEWER_RESPONSIBILITY,
};
use crate::{
    application::{
        agent_run_approval_service::{request_agent_run_approval, AgentRunApprovalRequirement},
        agent_run_service::{create_agent_run, CreateAgentRunInput},
        agent_service::{create_agent, set_agent_status, CreateAgentInput, SetAgentStatusInput},
        approval_service::{decide_approval, DecideApprovalInput},
    },
    domain::error::AppError,
    infrastructure::database::{
        initialize_demo_database, AppState, DEMO_OPERATOR_USER_ID, DEMO_REVIEWER_USER_ID,
    },
};

// بناء حالة ديمو حقيقية: قاعدة ذاكرة + الهجرات + بذر الديمو الكامل، ثم AppState بمستخدم البداية المحدد.
fn demo_state(active_user_id: &str) -> AppState {
    let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
    initialize_demo_database(&connection).expect("تهيئة قاعدة الديمو يجب أن تنجح");
    AppState::test_instance(connection, "tenant_demo_sa_001", active_user_id)
}

fn has_permission_for(state: &AppState, user_id: &str, code: &str) -> bool {
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    connection
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM local_users u
                INNER JOIN local_sessions s ON s.user_id = u.id AND s.tenant_id = u.tenant_id
                INNER JOIN user_role_assignments ura ON ura.user_id = u.id
                INNER JOIN role_permissions rp ON rp.role_id = ura.role_id
                INNER JOIN permissions p ON p.id = rp.permission_id
                WHERE u.id = ?1 AND u.tenant_id = 'tenant_demo_sa_001'
                  AND u.status = 'ACTIVE' AND s.revoked_at IS NULL
                  AND s.expires_at > '2029-01-01T00:00:00Z' AND p.code = ?2
            )",
            [user_id, code],
            |row| row.get::<_, bool>(0),
        )
        .expect("استعلام الصلاحية يجب أن ينجح")
}

#[test]
fn bootstrap_seeds_two_distinct_demo_actors_with_separated_authority() {
    let state = demo_state(DEMO_OPERATOR_USER_ID);

    assert!(has_permission_for(
        &state,
        DEMO_OPERATOR_USER_ID,
        "agent:execute"
    ));
    assert!(has_permission_for(
        &state,
        DEMO_OPERATOR_USER_ID,
        "task:create"
    ));
    assert!(!has_permission_for(
        &state,
        DEMO_OPERATOR_USER_ID,
        "approval:decide"
    ));

    assert!(has_permission_for(
        &state,
        DEMO_REVIEWER_USER_ID,
        "approval:decide"
    ));
    assert!(!has_permission_for(
        &state,
        DEMO_REVIEWER_USER_ID,
        "agent:execute"
    ));
    assert!(!has_permission_for(
        &state,
        DEMO_REVIEWER_USER_ID,
        "task:create"
    ));
    assert_ne!(DEMO_OPERATOR_USER_ID, DEMO_REVIEWER_USER_ID);

    let actors = list_demo_actors(&state).expect("قائمة الفاعلين يجب أن تنجح");
    assert_eq!(actors.active_actor_id, DEMO_OPERATOR_USER_ID);
    assert_eq!(actors.actors.len(), 2);
    assert!(actors
        .actors
        .iter()
        .any(|actor| actor.actor_id == DEMO_OPERATOR_USER_ID
            && actor.responsibility == REQUESTER_RESPONSIBILITY));
    assert!(actors
        .actors
        .iter()
        .any(|actor| actor.actor_id == DEMO_REVIEWER_USER_ID
            && actor.responsibility == REVIEWER_RESPONSIBILITY));
}

#[test]
fn switch_rejects_unknown_actor_and_keeps_identity() {
    let state = demo_state(DEMO_OPERATOR_USER_ID);
    assert!(matches!(
        switch_demo_actor(
            &state,
            SwitchDemoActorInput {
                actor_id: "user_demo_admin".to_owned(),
            },
        ),
        Err(AppError::ValidationFailed)
    ));
    assert_eq!(
        state.current_user_id().expect("قراءة الهوية يجب أن تنجح"),
        DEMO_OPERATOR_USER_ID
    );
}

#[test]
fn deterministic_demo_approval_flow_from_requester_to_reviewer() {
    let state = demo_state(DEMO_OPERATOR_USER_ID);

    // الطالب ينشئ وكيلًا وينشّطه ثم يطلب تشغيلاً محكوماً.
    let created_agent = create_agent(
        &state,
        CreateAgentInput {
            name: "Demo Compliance Drafter".to_owned(),
            description: "Synthetic demo agent for the governed approval loop.".to_owned(),
            capability: None,
        },
    )
    .expect("إنشاء الوكيل يجب أن ينجح");
    set_agent_status(
        &state,
        SetAgentStatusInput {
            agent_id: created_agent.agent.id.clone(),
            status: "ACTIVE".to_owned(),
        },
    )
    .expect("تنشيط الوكيل يجب أن ينجح");

    let created_run = create_agent_run(
        &state,
        CreateAgentRunInput {
            agent_id: created_agent.agent.id.clone(),
        },
    )
    .expect("إنشاء التشغيل يجب أن ينجح");

    let approval = request_agent_run_approval(
        &state,
        &created_run.run.id,
        AgentRunApprovalRequirement {
            approval_requirement_code: "HUMAN_REVIEW_REQUIRED".to_owned(),
            required_role_code: "TENANT_ADMIN".to_owned(),
        },
    )
    .expect("طلب الموافقة يجب أن ينجح");
    assert_eq!(
        approval.run.status,
        crate::domain::models::AgentRunStatus::WaitingApproval
    );

    // الطالب ليس له أي صلاحية قرار — الرفض يقع في بوابة التفويض قبل فحص الذات.
    assert!(matches!(
        decide_approval(
            &state,
            DecideApprovalInput {
                approval_id: approval.approval_id.clone(),
                decision: "APPROVED".to_owned(),
                reason: "Requester must not be able to decide.".to_owned(),
            },
        ),
        Err(AppError::AuthorizationDenied)
    ));

    // التبديل إلى المراجِع (هوية مختلفة فعلياً داخل المستأجر الديمو نفسه).
    let switched = switch_demo_actor(
        &state,
        SwitchDemoActorInput {
            actor_id: DEMO_REVIEWER_USER_ID.to_owned(),
        },
    )
    .expect("تبديل الفاعل يجب أن ينجح");
    assert_eq!(switched.active_actor_id, DEMO_REVIEWER_USER_ID);

    // المراجِع لا يملك تنفيذ الوكلاء — المصفوفة مفصولة في الاتجاهين.
    assert!(matches!(
        create_agent_run(
            &state,
            CreateAgentRunInput {
                agent_id: created_agent.agent.id.clone(),
            },
        ),
        Err(AppError::AuthorizationDenied)
    ));

    // المراجِع يمنح القرار فيتحول التشغيل إلى QUEUED بمراجعة أعلى وبتدقيق قرار مكتوب.
    let decision = decide_approval(
        &state,
        DecideApprovalInput {
            approval_id: approval.approval_id.clone(),
            decision: "APPROVED".to_owned(),
            reason: "Demo reviewer approved the governed run.".to_owned(),
        },
    )
    .expect("قرار المراجِع يجب أن ينجح");
    assert_eq!(decision.decision, "APPROVED");

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let run_status: String = connection
        .query_row(
            "SELECT status FROM agent_runs WHERE id = ?1",
            [&created_run.run.id],
            |row| row.get(0),
        )
        .expect("قراءة حالة التشغيل يجب أن تنجح");
    let run_revision: i64 = connection
        .query_row(
            "SELECT revision FROM agent_runs WHERE id = ?1",
            [&created_run.run.id],
            |row| row.get(0),
        )
        .expect("قراءة مراجعة التشغيل يجب أن تنجح");
    let granted_by: String = connection
        .query_row(
            "SELECT decision_by FROM approvals WHERE id = ?1",
            [&approval.approval_id],
            |row| row.get(0),
        )
        .expect("قراءة صاحب القرار يجب أن تنجح");
    let granted_audit: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE event_type = 'APPROVAL_GRANTED' AND resource_id = ?1",
            [&approval.approval_id],
            |row| row.get(0),
        )
        .expect("استعلام تدقيق القرار يجب أن ينجح");
    drop(connection);

    assert_eq!(run_status, "QUEUED");
    assert_eq!(run_revision, created_run.run.revision + 1);
    assert_eq!(granted_by, DEMO_REVIEWER_USER_ID);
    assert_eq!(granted_audit, 1);
}

#[test]
fn reviewer_cannot_decide_own_request() {
    let state = demo_state(DEMO_REVIEWER_USER_ID);
    let now = chrono::Utc::now().to_rfc3339();
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        // حالة صحيحة قابلة للوجود: طلب موافقة طلبه المراجِع نفسه — يجب أن يرفض قراره ذاتياً.
        connection
            .execute(
                "INSERT INTO approvals (id, tenant_id, resource_type, resource_id, resource_version, requested_by, required_role_code, decision, decision_by, decision_reason, decided_at, created_at, updated_at) VALUES ('approval-self', 'tenant_demo_sa_001', 'TASK', 'task_demo_001', 1, ?1, 'TENANT_ADMIN', 'PENDING', NULL, NULL, NULL, ?2, ?2)",
                [DEMO_REVIEWER_USER_ID, &now],
            )
            .expect("طلب الموافقة الذاتي يجب أن ينشأ");
    }

    assert!(matches!(
        decide_approval(
            &state,
            DecideApprovalInput {
                approval_id: "approval-self".to_owned(),
                decision: "APPROVED".to_owned(),
                reason: "Self decision must be forbidden.".to_owned(),
            },
        ),
        Err(AppError::SelfApprovalForbidden)
    ));
}

#[test]
fn demo_task_requires_evidence_and_only_the_reviewer_side_decides() {
    use crate::application::task_service::{transition_task, TransitionTaskInput};
    use crate::domain::models::TaskStatus;

    let state = demo_state(DEMO_OPERATOR_USER_ID);
    let now = chrono::Utc::now().to_rfc3339();

    let submit = |expected_version: u32| {
        transition_task(
            &state,
            TransitionTaskInput {
                task_id: "task_demo_001".to_owned(),
                target_status: "READY_FOR_REVIEW".to_owned(),
                reason: Some("Demo submission".to_owned()),
                expected_version,
            },
        )
    };

    // 1) بدء العمل على المهمة المزروعة (مُسنَدة إلى المشغّل).
    transition_task(
        &state,
        TransitionTaskInput {
            task_id: "task_demo_001".to_owned(),
            target_status: "IN_PROGRESS".to_owned(),
            reason: Some("Demo work started".to_owned()),
            expected_version: 1,
        },
    )
    .expect("المشغّل يبدأ مهمته المزروعة");

    // 2) طلب المراجعة بلا دليل مسجَّل مرفوض — الشرط المسبق الحقيقي.
    assert!(matches!(submit(2), Err(AppError::TaskEvidenceRequired)));

    // 3) تسجيل دليل على التزام المهمة (وسم SYNTHETIC مقصود: مستأجر الديمو).
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "INSERT INTO evidence_items (id, tenant_id, obligation_id, classification, status, content_hash, version, created_at, updated_at) VALUES ('evidence_demo_001', 'tenant_demo_sa_001', 'obl_demo_001', 'INTERNAL', 'VALID', 'hash-demo-001', 1, ?1, ?1)",
                [&now],
            )
            .expect("دليل الديمو يجب أن ينشأ");
    }

    let submitted = submit(2).expect("طلب المراجعة بعد تسجيل الدليل يجب أن ينجح");
    assert_eq!(submitted.status, TaskStatus::ReadyForReview);
    let approval_id = submitted
        .pending_approval_id
        .expect("طلب المراجعة يجب أن ينشئ موافقة معلّقة");

    // 4) المشغّل (بلا صلاحية قرار) لا يعتمد عمله.
    assert!(matches!(
        decide_approval(
            &state,
            DecideApprovalInput {
                approval_id: approval_id.clone(),
                decision: "APPROVED".to_owned(),
                reason: "Operator must not decide.".to_owned(),
            },
        ),
        Err(AppError::AuthorizationDenied)
    ));

    // 5) المراجِع يقرر — والقرار يجب أن يحرّك المهمة فعلاً.
    switch_demo_actor(
        &state,
        SwitchDemoActorInput {
            actor_id: DEMO_REVIEWER_USER_ID.to_owned(),
        },
    )
    .expect("تبديل الفاعل يجب أن ينجح");

    let decision = decide_approval(
        &state,
        DecideApprovalInput {
            approval_id,
            decision: "REJECTED".to_owned(),
            reason: "The synthetic evidence does not cover the full scope.".to_owned(),
        },
    )
    .expect("المراجِع يجب أن يقرر");
    assert_eq!(decision.decision, "REJECTED");

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let (status, version): (String, i64) = connection
        .query_row(
            "SELECT status, version FROM tasks WHERE id = 'task_demo_001'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("قراءة المهمة يجب أن تنجح");
    // العيب القديم: الحالة بقيت OPEN بعد القرار. الآن القرار يحرّكها.
    assert_eq!(status, "REJECTED");
    assert_eq!(version, 4);
    let transitions: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM task_state_transitions WHERE task_id = 'task_demo_001'",
            [],
            |row| row.get(0),
        )
        .expect("عدّ الانتقالات يجب أن ينجح");
    assert_eq!(transitions, 3, "بدء + طلب مراجعة + قرار الاعتماد");
}

#[test]
fn session_context_reflects_the_real_backend_permission_matrix_per_actor() {
    use super::get_session_context;

    let operator_state = demo_state(DEMO_OPERATOR_USER_ID);
    let operator_context = get_session_context(&operator_state).expect("سياق المشغّل يجب أن يقرأ");
    assert_eq!(operator_context.actor_id, DEMO_OPERATOR_USER_ID);
    assert_eq!(operator_context.responsibility, REQUESTER_RESPONSIBILITY);
    assert!(operator_context
        .permissions
        .contains(&"task:create".to_owned()));
    assert!(operator_context
        .permissions
        .contains(&"obligation:update".to_owned()));
    assert!(!operator_context
        .permissions
        .contains(&"approval:decide".to_owned()));
    // قراءات الوحدات الجديدة ممنوحة للفاعلين معاً.
    for read_code in [
        "organization:read",
        "audit:read",
        "integration:read",
        "regulatory_intelligence:read",
    ] {
        assert!(operator_context.permissions.contains(&read_code.to_owned()));
    }

    let reviewer_state = demo_state(DEMO_REVIEWER_USER_ID);
    let reviewer_context = get_session_context(&reviewer_state).expect("سياق المراجِع يجب أن يقرأ");
    assert_eq!(reviewer_context.responsibility, REVIEWER_RESPONSIBILITY);
    assert!(reviewer_context
        .permissions
        .contains(&"approval:decide".to_owned()));
    assert!(!reviewer_context
        .permissions
        .contains(&"task:create".to_owned()));
    assert!(!reviewer_context
        .permissions
        .contains(&"obligation:update".to_owned()));
}
