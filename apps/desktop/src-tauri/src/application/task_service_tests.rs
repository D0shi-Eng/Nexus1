use chrono::Utc;
use rusqlite::Connection;

use super::{
    assign_task, create_remediation_task, load_task_history, load_tasks, transition_task,
    AssignTaskInput, CreateTaskInput, TransitionTaskInput,
};
use crate::{
    application::approval_service::{decide_approval, DecideApprovalInput},
    domain::{error::AppError, models::TaskStatus},
    infrastructure::database::AppState,
};

// =====================================================================
// حامل اختبار دورة حياة المهمة — مخطط كامل حتى 0025.
// الفاعلون الثلاثة مقصودون: مدير (TENANT_ADMIN + كل صلاحيات المهمة)،
// منفّذ (بلا إسناد وبلا قرار)، وعضو في مستأجر آخر لاختبار العزل.
// =====================================================================
fn state_with_task_lifecycle() -> AppState {
    let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
    for migration in [
        include_str!("../../migrations/0001_initial.sql"),
        include_str!("../../migrations/0002_identity_and_access.sql"),
        include_str!("../../migrations/0003_workflow_and_risk.sql"),
        include_str!("../../migrations/0004_evidence_metadata.sql"),
        // 0007 يضيف أعمدة الالتزام المستخدمة فعلاً في مسار التحقق (archived_at)
        // ومشغّلات العزل؛ تركها خارج التركيبة يجعل الاختبار يقيس مخططاً غير حقيقي.
        include_str!("../../migrations/0007_obligation_management.sql"),
        // 0010–0012 مطلوبة لأن مسار قرار الموافقة يقرأ approvals.approval_requirement_code
        // (أُضيف في 0012)، و0012 يعيد بناء agent_runs الذي يشير إلى agents (0010)
        // وإلى الفهرس الفريد على local_users (0011).
        include_str!("../../migrations/0010_agent_foundation.sql"),
        include_str!("../../migrations/0011_agent_runs.sql"),
        include_str!("../../migrations/0012_agent_run_approval_lifecycle.sql"),
        include_str!("../../migrations/0025_task_lifecycle.sql"),
        include_str!("../../migrations/0026_production_user_and_authoring.sql"),
    ] {
        connection
            .execute_batch(migration)
            .expect("هجرة الاختبار يجب أن تنجح");
    }
    let now = Utc::now().to_rfc3339();
    for tenant_id in ["tenant-a", "tenant-b"] {
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?1, 'DEMO_SYNTHETIC', ?2, ?2)",
                (tenant_id, &now),
            )
            .expect("المستأجر يجب أن ينشأ");
    }
    for user_id in ["user-manager", "user-worker", "user-idle"] {
        connection
            .execute(
                "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, 'tenant-a', ?1, ?2, NULL, 'ACTIVE', ?3, ?3)",
                (user_id, format!("{user_id}@test.local"), &now),
            )
            .expect("المستخدم يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, 'tenant-a', ?2, ?3, '2030-01-01T00:00:00Z', NULL, 'test')",
                (format!("session-{user_id}"), user_id, &now),
            )
            .expect("الجلسة يجب أن تنشأ");
    }
    connection
        .execute(
            "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES ('user-other', 'tenant-b', 'Other', 'other@test.local', NULL, 'ACTIVE', ?1, ?1)",
            [&now],
        )
        .expect("مستخدم المستأجر الآخر يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session-user-other', 'tenant-b', 'user-other', ?1, '2030-01-01T00:00:00Z', NULL, 'test')",
            [&now],
        )
        .expect("جلسة المستأجر الآخر يجب أن تنشأ");

    // كتالوج الصلاحيات المستخدم في هذا الجناح (مسارات المهمة والاعتماد + قراءات
    // اللوحة والتقارير اللازمة لاختبار السلسلة الكاملة).
    for (id, code, resource_type, action) in [
        ("p-task-read", "task:read", "TASK", "READ"),
        ("p-task-create", "task:create", "TASK", "CREATE"),
        ("p-task-update", "task:update", "TASK", "UPDATE"),
        ("p-task-assign", "task:assign", "TASK", "ASSIGN"),
        ("p-approval-decide", "approval:decide", "APPROVAL", "DECIDE"),
        ("p-evidence-create", "evidence:create", "EVIDENCE", "CREATE"),
        ("p-obligation-read", "obligation:read", "OBLIGATION", "READ"),
        (
            "p-obligation-verify",
            "obligation:verify",
            "OBLIGATION",
            "VERIFY",
        ),
        ("p-attention-read", "attention:read", "ATTENTION", "READ"),
        ("p-report-read", "report:read", "REPORT", "READ"),
        ("p-audit-read", "audit:read", "AUDIT", "READ"),
        ("p-risk-read", "risk:read", "RISK", "READ"),
        ("p-evidence-read", "evidence:read", "EVIDENCE", "READ"),
    ] {
        connection
            .execute(
                "INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES (?1, ?2, ?3, ?4, 'HIGHLY_RESTRICTED', ?5)",
                (id, code, resource_type, action, &now),
            )
            .expect("الصلاحية يجب أن تنشأ");
    }

    connection
        .execute(
            "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-manager', 'tenant-a', 'TENANT_ADMIN', 'Manager', ?1, ?1)",
            [&now],
        )
        .expect("دور المدير يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-worker', 'tenant-a', 'WORKER', 'Worker', ?1, ?1)",
            [&now],
        )
        .expect("دور المنفّذ يجب أن ينشأ");
    // قراءات مشتركة للفاعلين: الموظف يقرأ التزامه ودليله ولوحته.
    for role_id in ["role-manager", "role-worker"] {
        for permission_id in [
            "p-obligation-read",
            "p-attention-read",
            "p-report-read",
            "p-audit-read",
            "p-risk-read",
            "p-evidence-read",
        ] {
            connection
                .execute(
                    "INSERT INTO role_permissions (role_id, permission_id) VALUES (?1, ?2)",
                    (role_id, permission_id),
                )
                .expect("منح صلاحية القراءة يجب أن ينجح");
        }
    }
    for (role_id, permission_id) in [
        ("role-manager", "p-task-read"),
        ("role-manager", "p-task-update"),
        ("role-manager", "p-task-assign"),
        ("role-manager", "p-approval-decide"),
        ("role-manager", "p-evidence-create"),
        // التحقق البشري لجهة المراجعة وحدها — منفّذ العمل لا يملكه.
        ("role-manager", "p-obligation-verify"),
        ("role-worker", "p-task-read"),
        ("role-worker", "p-task-create"),
        ("role-worker", "p-task-update"),
        ("role-worker", "p-evidence-create"),
    ] {
        connection
            .execute(
                "INSERT INTO role_permissions (role_id, permission_id) VALUES (?1, ?2)",
                (role_id, permission_id),
            )
            .expect("منح الصلاحية يجب أن ينجح");
    }
    for (user_id, role_id) in [
        ("user-manager", "role-manager"),
        ("user-worker", "role-worker"),
        ("user-idle", "role-worker"),
    ] {
        connection
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, ?2, ?3, 'system')",
                (user_id, role_id, &now),
            )
            .expect("تعيين الدور يجب أن ينجح");
    }

    for tenant_id in ["tenant-a", "tenant-b"] {
        connection
            .execute(
                "INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES (?1, ?2, 'Synthetic', 'Synthetic source', NULL, '1', ?3, 0, 'UNVERIFIED', ?3, ?3)",
                (format!("source-{tenant_id}"), tenant_id, &now),
            )
            .expect("المصدر يجب أن ينشأ");
    }
    for (id, title) in [
        ("ob-with-evidence", "With evidence"),
        ("ob-no-evidence", "No evidence"),
    ] {
        connection
            .execute(
                "INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, created_at, updated_at) VALUES (?1, 'tenant-a', 'source-tenant-a', ?2, 'Owner', NULL, 'HIGH', 'OPEN', 'APPLIES', 'MISSING', 'UNVERIFIED', ?3, ?3)",
                (id, title, &now),
            )
            .expect("الالتزام يجب أن ينشأ");
    }
    connection
        .execute(
            "INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, created_at, updated_at) VALUES ('ob-tenant-b', 'tenant-b', 'source-tenant-b', 'Other', 'Owner', NULL, 'HIGH', 'OPEN', 'APPLIES', 'MISSING', 'UNVERIFIED', ?1, ?1)",
            [&now],
        )
        .expect("التزام المستأجر الآخر يجب أن ينشأ");

    // دليل مقبول على الالتزام الأول — شرط طلب المراجعة.
    connection
        .execute(
            "INSERT INTO evidence_items (id, tenant_id, obligation_id, classification, status, content_hash, version, created_at, updated_at) VALUES ('evidence-1', 'tenant-a', 'ob-with-evidence', 'INTERNAL', 'VALID', 'hash-1', 1, ?1, ?1)",
            [&now],
        )
        .expect("الدليل يجب أن ينشأ");

    // مهمة مُسنَدة إلى المنفّذ، ومهمة بلا مالك، ومهمة في مستأجر آخر.
    for (id, obligation_id, assignee, status, version) in [
        (
            "task-assigned",
            "ob-with-evidence",
            Some("user-worker"),
            "OPEN",
            1_i64,
        ),
        ("task-unassigned", "ob-with-evidence", None, "OPEN", 1_i64),
        (
            "task-no-evidence",
            "ob-no-evidence",
            Some("user-worker"),
            "OPEN",
            1_i64,
        ),
    ] {
        connection
            .execute(
                "INSERT INTO tasks (id, tenant_id, obligation_id, title, description, assignee_user_id, due_at, priority, status, version, completed_at, created_at, updated_at) VALUES (?1, 'tenant-a', ?2, ?1, 'Synthetic task', ?3, NULL, 'HIGH', ?4, ?5, NULL, ?6, ?6)",
                rusqlite::params![id, obligation_id, assignee, status, version, &now],
            )
            .expect("المهمة يجب أن تنشأ");
    }
    connection
        .execute(
            "INSERT INTO tasks (id, tenant_id, obligation_id, title, description, assignee_user_id, due_at, priority, status, version, completed_at, created_at, updated_at) VALUES ('task-tenant-b', 'tenant-b', 'ob-tenant-b', 'Other task', 'Synthetic task', 'user-other', NULL, 'HIGH', 'OPEN', 1, NULL, ?1, ?1)",
            [&now],
        )
        .expect("مهمة المستأجر الآخر يجب أن تنشأ");

    AppState::test_instance(connection, "tenant-a".to_owned(), "user-worker".to_owned())
}

fn as_actor(state: &AppState, user_id: &str) {
    state.set_active_user_for_tests(user_id);
}

fn transition(
    state: &AppState,
    task_id: &str,
    target: &str,
    expected_version: u32,
) -> Result<super::TaskTransitionResult, AppError> {
    transition_task(
        state,
        TransitionTaskInput {
            task_id: task_id.to_owned(),
            target_status: target.to_owned(),
            reason: Some("Test reason".to_owned()),
            expected_version,
        },
    )
}

fn stored_status(state: &AppState, task_id: &str) -> (String, i64) {
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    connection
        .query_row(
            "SELECT status, version FROM tasks WHERE id = ?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("قراءة المهمة يجب أن تنجح")
}

// =====================================================================
// الإنشاء
// =====================================================================

#[test]
fn cannot_create_a_task_for_another_tenants_obligation() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-worker");
    let result = create_remediation_task(
        &state,
        CreateTaskInput {
            obligation_id: "ob-tenant-b".to_owned(),
            title: "Cross tenant".to_owned(),
            description: "Must be rejected.".to_owned(),
            due_at: None,
            priority: "HIGH".to_owned(),
            assignee_user_id: None,
        },
    );
    assert!(matches!(result, Err(AppError::ResourceNotFound)));
}

#[test]
fn creation_writes_no_approval_and_leaves_the_task_unassigned_when_unspecified() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-worker");
    let created = create_remediation_task(
        &state,
        CreateTaskInput {
            obligation_id: "ob-with-evidence".to_owned(),
            title: "Prepare evidence".to_owned(),
            description: "Real work item.".to_owned(),
            due_at: Some("2026-09-01T00:00:00Z".to_owned()),
            priority: "HIGH".to_owned(),
            assignee_user_id: None,
        },
    )
    .expect("الإنشاء المصرح به يجب أن ينجح");

    assert_eq!(created.status, "OPEN");
    // لا موافقة معلّقة عند الإنشاء — العيب الذي أُغلق: كل مهمة كانت تبذر
    // موافقة معلّقة لا يمكن أن تُقرَّر أبداً فتظل معلّقة للأبد.
    assert!(created.pending_approval_id.is_none());
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let approval_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM approvals", [], |row| row.get(0))
        .expect("عدّ الموافقات يجب أن ينجح");
    let assignee: Option<String> = connection
        .query_row(
            "SELECT assignee_user_id FROM tasks WHERE id = ?1",
            [&created.task_id],
            |row| row.get(0),
        )
        .expect("قراءة الإسناد يجب أن تنجح");
    let audit_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE resource_id = ?1 AND event_type = 'TASK_CREATED'",
            [&created.task_id],
            |row| row.get(0),
        )
        .expect("عدّ التدقيق يجب أن ينجح");
    assert_eq!(approval_count, 0, "لا موافقة عند الإنشاء");
    assert_eq!(assignee, None, "لا إسناد صامت إلى المنشئ");
    assert_eq!(audit_count, 1);
}

#[test]
fn creation_rejects_an_assignee_from_another_tenant() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-worker");
    let result = create_remediation_task(
        &state,
        CreateTaskInput {
            obligation_id: "ob-with-evidence".to_owned(),
            title: "Cross tenant assignee".to_owned(),
            description: "Must be rejected.".to_owned(),
            due_at: None,
            priority: "HIGH".to_owned(),
            assignee_user_id: Some("user-other".to_owned()),
        },
    );
    assert!(matches!(result, Err(AppError::TaskAssignmentInvalid)));
}

// =====================================================================
// المسار الكامل والانتقالات
// =====================================================================

#[test]
fn happy_path_runs_from_open_to_completed_through_a_real_approval() {
    let state = state_with_task_lifecycle();

    // المنفّذ يبدأ العمل.
    as_actor(&state, "user-worker");
    let started = transition(&state, "task-assigned", "IN_PROGRESS", 1).expect("بدء العمل");
    assert_eq!(started.status, TaskStatus::InProgress);
    assert_eq!(started.version, 2);

    // ثم يطلب المراجعة — هذا يُنشئ الموافقة المعلّقة.
    let submitted =
        transition(&state, "task-assigned", "READY_FOR_REVIEW", 2).expect("طلب المراجعة");
    let approval_id = submitted
        .pending_approval_id
        .clone()
        .expect("طلب المراجعة يجب أن ينشئ موافقة");
    assert_eq!(stored_status(&state, "task-assigned").0, "READY_FOR_REVIEW");

    // المدير يعتمد — والاعتماد يجب أن يحرّك المهمة فعلاً.
    as_actor(&state, "user-manager");
    decide_approval(
        &state,
        DecideApprovalInput {
            approval_id: approval_id.clone(),
            decision: "APPROVED".to_owned(),
            reason: "Evidence accepted by the reviewer.".to_owned(),
        },
    )
    .expect("الاعتماد المصرح به يجب أن ينجح");
    assert_eq!(
        stored_status(&state, "task-assigned").0,
        "APPROVED",
        "قرار الاعتماد يجب أن يحدّث حالة المهمة تلقائياً"
    );

    // ثم الإغلاق النهائي.
    as_actor(&state, "user-worker");
    let closed = transition(&state, "task-assigned", "COMPLETED", 4).expect("الإغلاق");
    assert_eq!(closed.status, TaskStatus::Completed);
    let (_, version) = stored_status(&state, "task-assigned");
    assert_eq!(version, 5);
}

#[test]
fn review_request_is_refused_without_registered_evidence() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-worker");
    transition(&state, "task-no-evidence", "IN_PROGRESS", 1).expect("بدء العمل");
    let result = transition(&state, "task-no-evidence", "READY_FOR_REVIEW", 2);
    assert!(matches!(result, Err(AppError::TaskEvidenceRequired)));
    assert_eq!(stored_status(&state, "task-no-evidence").0, "IN_PROGRESS");
}

#[test]
fn rejection_returns_the_task_for_rework_and_rework_can_resubmit() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-worker");
    transition(&state, "task-assigned", "IN_PROGRESS", 1).expect("بدء");
    let submitted = transition(&state, "task-assigned", "READY_FOR_REVIEW", 2).expect("طلب");
    let approval_id = submitted.pending_approval_id.expect("موافقة");

    as_actor(&state, "user-manager");
    decide_approval(
        &state,
        DecideApprovalInput {
            approval_id,
            decision: "REJECTED".to_owned(),
            reason: "The evidence does not cover the full scope.".to_owned(),
        },
    )
    .expect("الرفض المصرح به يجب أن ينجح");
    assert_eq!(stored_status(&state, "task-assigned").0, "REJECTED");

    // إعادة العمل ثم إعادة الطلب — موافقة جديدة بإصدار جديد.
    as_actor(&state, "user-worker");
    transition(&state, "task-assigned", "IN_PROGRESS", 4).expect("إعادة العمل");
    let resubmitted =
        transition(&state, "task-assigned", "READY_FOR_REVIEW", 5).expect("إعادة الطلب");
    let new_approval = resubmitted.pending_approval_id.expect("موافقة جديدة");
    assert_ne!(new_approval, "", "يجب إنشاء موافقة جديدة للطلب الجديد");
    let connection = state.connection.lock().expect("القفل");
    let pending: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM approvals WHERE resource_id = 'task-assigned' AND decision = 'PENDING' AND resource_version = 6",
            [],
            |row| row.get(0),
        )
        .expect("عدّ الموافقات المعلّقة");
    assert_eq!(pending, 1, "الموافقة الجديدة مربوطة بالإصدار الجديد");
}

#[test]
fn completed_is_terminal_and_history_records_every_transition() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-worker");
    transition(&state, "task-assigned", "IN_PROGRESS", 1).expect("بدء");
    let submitted = transition(&state, "task-assigned", "READY_FOR_REVIEW", 2).expect("طلب");
    as_actor(&state, "user-manager");
    decide_approval(
        &state,
        DecideApprovalInput {
            approval_id: submitted.pending_approval_id.expect("موافقة"),
            decision: "APPROVED".to_owned(),
            reason: "Accepted.".to_owned(),
        },
    )
    .expect("اعتماد");
    as_actor(&state, "user-worker");
    transition(&state, "task-assigned", "COMPLETED", 4).expect("إغلاق");

    // حالة نهائية: لا خروج.
    let reopen = transition(&state, "task-assigned", "IN_PROGRESS", 5);
    assert!(matches!(reopen, Err(AppError::InvalidStateTransition)));

    // سجل الانتقالات الحقيقي: أربعة انتقالات بمصدر وهدف وموثّق.
    let history = load_task_history(&state, "task-assigned").expect("قراءة السجل");
    let pairs: Vec<(TaskStatus, TaskStatus)> = history
        .iter()
        .map(|record| (record.from_status, record.to_status))
        .collect();
    assert_eq!(
        pairs,
        vec![
            (TaskStatus::Open, TaskStatus::InProgress),
            (TaskStatus::InProgress, TaskStatus::ReadyForReview),
            (TaskStatus::ReadyForReview, TaskStatus::Approved),
            (TaskStatus::Approved, TaskStatus::Completed),
        ]
    );
    assert!(history
        .iter()
        .all(|record| !record.actor_display_name.is_empty()));
}

#[test]
fn concurrent_stale_transition_is_refused_without_writing() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-worker");
    transition(&state, "task-assigned", "IN_PROGRESS", 1).expect("بدء");
    // إصدار قديم (1) بعد أن صار 2 — التزامن المتفائل يرفض بلا كتابة.
    let stale = transition(&state, "task-assigned", "BLOCKED", 1);
    assert!(matches!(stale, Err(AppError::Conflict)));
    assert_eq!(stored_status(&state, "task-assigned").0, "IN_PROGRESS");
}

#[test]
fn a_stale_approval_cannot_decide_work_that_moved_on() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-worker");
    transition(&state, "task-assigned", "IN_PROGRESS", 1).expect("بدء");
    let submitted = transition(&state, "task-assigned", "READY_FOR_REVIEW", 2).expect("طلب");
    let approval_id = submitted.pending_approval_id.expect("موافقة");

    // المنفّذ يسحب الطلب — الموافقة صارت قديمة.
    as_actor(&state, "user-worker");
    transition(&state, "task-assigned", "IN_PROGRESS", 3).expect("سحب الطلب");

    as_actor(&state, "user-manager");
    let result = decide_approval(
        &state,
        DecideApprovalInput {
            approval_id,
            decision: "APPROVED".to_owned(),
            reason: "Must be refused as stale.".to_owned(),
        },
    );
    assert!(matches!(result, Err(AppError::StaleApproval)));
}

#[test]
fn the_requester_cannot_approve_their_own_submitted_work() {
    let state = state_with_task_lifecycle();
    // المدير منفّذ ومُسنَد إليه أيضاً — فهو طالب ومراجِع في الوقت نفسه.
    let connection = state.connection.lock().expect("القفل");
    connection
        .execute(
            "UPDATE tasks SET assignee_user_id = 'user-manager' WHERE id = 'task-assigned'",
            [],
        )
        .expect("نقل الإسناد");
    drop(connection);

    as_actor(&state, "user-manager");
    transition(&state, "task-assigned", "IN_PROGRESS", 1).expect("بدء");
    let submitted = transition(&state, "task-assigned", "READY_FOR_REVIEW", 2).expect("طلب");
    let result = decide_approval(
        &state,
        DecideApprovalInput {
            approval_id: submitted.pending_approval_id.expect("موافقة"),
            decision: "APPROVED".to_owned(),
            reason: "Self approval must fail.".to_owned(),
        },
    );
    assert!(matches!(result, Err(AppError::SelfApprovalForbidden)));
    assert_eq!(stored_status(&state, "task-assigned").0, "READY_FOR_REVIEW");
}

#[test]
fn approval_states_cannot_be_reached_by_a_direct_transition_call() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-worker");
    transition(&state, "task-assigned", "IN_PROGRESS", 1).expect("بدء");
    let submitted = transition(&state, "task-assigned", "READY_FOR_REVIEW", 2).expect("طلب");
    assert!(submitted.pending_approval_id.is_some());

    // تجاوز مسار الموافقات مرفوض حتى للمنفّذ نفسه.
    let bypass = transition(&state, "task-assigned", "APPROVED", 3);
    assert!(matches!(bypass, Err(AppError::SelfApprovalForbidden)));
    assert_eq!(stored_status(&state, "task-assigned").0, "READY_FOR_REVIEW");
}

#[test]
fn an_unassigned_task_cannot_start_before_it_has_an_owner() {
    let state = state_with_task_lifecycle();
    // الترتيب مقصود: الأهلية تُفحص أولاً فلا يرى عضو غير مأذون شيئاً عن المهمة.
    as_actor(&state, "user-worker");
    assert!(matches!(
        transition(&state, "task-unassigned", "IN_PROGRESS", 1),
        Err(AppError::AuthorizationDenied)
    ));

    // المدير يمر من بوابة الأهلية، فيُكشف الشرط المسبق الحقيقي: لا مالك ⇒ لا بدء.
    as_actor(&state, "user-manager");
    assert!(matches!(
        transition(&state, "task-unassigned", "IN_PROGRESS", 1),
        Err(AppError::TaskAssignmentInvalid)
    ));
    assert_eq!(stored_status(&state, "task-unassigned").0, "OPEN");
}

// =====================================================================
// الإسناد
// =====================================================================

#[test]
fn a_manager_assigns_work_and_the_assignee_can_then_work_it() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-manager");
    let assigned = assign_task(
        &state,
        AssignTaskInput {
            task_id: "task-unassigned".to_owned(),
            assignee_user_id: "user-idle".to_owned(),
            due_at: Some("2026-10-01T00:00:00Z".to_owned()),
            expected_version: 1,
        },
    )
    .expect("إسناد المدير يجب أن ينجح");
    assert_eq!(assigned.version, 2);

    as_actor(&state, "user-idle");
    transition(&state, "task-unassigned", "IN_PROGRESS", 2).expect("المنفّذ الجديد يبدأ");
    assert_eq!(stored_status(&state, "task-unassigned").0, "IN_PROGRESS");
}

#[test]
fn a_worker_cannot_reassign_work_to_someone_else() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-worker");
    let result = assign_task(
        &state,
        AssignTaskInput {
            task_id: "task-assigned".to_owned(),
            assignee_user_id: "user-idle".to_owned(),
            due_at: None,
            expected_version: 1,
        },
    );
    assert!(matches!(result, Err(AppError::AuthorizationDenied)));
}

#[test]
fn a_worker_may_claim_an_unassigned_task_but_not_a_task_owned_by_another() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-idle");
    assign_task(
        &state,
        AssignTaskInput {
            task_id: "task-unassigned".to_owned(),
            assignee_user_id: "user-idle".to_owned(),
            due_at: None,
            expected_version: 1,
        },
    )
    .expect("الالتقاط الذاتي لمهمة بلا مالك يجب أن ينجح");

    // محاولة انتزاع مهمة يملكها غيره.
    let steal = assign_task(
        &state,
        AssignTaskInput {
            task_id: "task-assigned".to_owned(),
            assignee_user_id: "user-idle".to_owned(),
            due_at: None,
            expected_version: 1,
        },
    );
    assert!(matches!(steal, Err(AppError::AuthorizationDenied)));
}

#[test]
fn a_manager_cannot_assign_work_across_tenants() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-manager");
    let result = assign_task(
        &state,
        AssignTaskInput {
            task_id: "task-assigned".to_owned(),
            assignee_user_id: "user-other".to_owned(),
            due_at: None,
            expected_version: 1,
        },
    );
    assert!(matches!(result, Err(AppError::TaskAssignmentInvalid)));
}

// =====================================================================
// العزل والأهلية والقراءة
// =====================================================================

#[test]
fn another_tenants_task_is_invisible_to_transition_and_assignment() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-manager");
    assert!(matches!(
        transition(&state, "task-tenant-b", "IN_PROGRESS", 1),
        Err(AppError::ResourceNotFound)
    ));
    assert!(matches!(
        assign_task(
            &state,
            AssignTaskInput {
                task_id: "task-tenant-b".to_owned(),
                assignee_user_id: "user-worker".to_owned(),
                due_at: None,
                expected_version: 1,
            }
        ),
        Err(AppError::ResourceNotFound)
    ));
    assert!(matches!(
        load_task_history(&state, "task-tenant-b"),
        Err(AppError::ResourceNotFound)
    ));
}

#[test]
fn an_unrelated_member_cannot_move_someone_elses_task() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-idle");
    let result = transition(&state, "task-assigned", "IN_PROGRESS", 1);
    assert!(matches!(result, Err(AppError::AuthorizationDenied)));
}

#[test]
fn the_read_model_reports_real_counts_and_per_viewer_actions() {
    let state = state_with_task_lifecycle();

    // المنفّذ: يرى مهمتين مُسنَدتين إليه فعلاً وإجراءات متاحة.
    as_actor(&state, "user-worker");
    let worker_view = load_tasks(&state).expect("قراءة المهام");
    assert_eq!(worker_view.items.len(), 3);
    assert_eq!(worker_view.summary.total, 3);
    assert_eq!(worker_view.summary.assigned_to_me, 2);
    assert_eq!(worker_view.summary.open, 3);
    let assigned = worker_view
        .items
        .iter()
        .find(|item| item.id == "task-assigned")
        .expect("المهمة المُسنَدة يجب أن تُقرأ");
    assert_eq!(assigned.status, TaskStatus::Open);
    assert_eq!(assigned.version, 1);
    assert_eq!(
        assigned.available_transitions,
        vec![TaskStatus::InProgress, TaskStatus::Blocked]
    );

    // عضو لا يملك المهمة ولا أهلية الإدارة: صفر إجراءات — إجابة صادقة.
    as_actor(&state, "user-idle");
    let idle_view = load_tasks(&state).expect("قراءة المهام");
    let not_mine = idle_view
        .items
        .iter()
        .find(|item| item.id == "task-assigned")
        .expect("المهمة يجب أن تُقرأ قراءةً فقط");
    assert!(not_mine.available_transitions.is_empty());
    assert_eq!(idle_view.summary.assigned_to_me, 0);
}

#[test]
fn task_state_transitions_are_immutable_evidence() {
    let state = state_with_task_lifecycle();
    as_actor(&state, "user-worker");
    transition(&state, "task-assigned", "IN_PROGRESS", 1).expect("بدء");

    let connection = state.connection.lock().expect("القفل");
    let update = connection.execute(
        "UPDATE task_state_transitions SET to_status = 'COMPLETED' WHERE task_id = 'task-assigned'",
        [],
    );
    let delete = connection.execute(
        "DELETE FROM task_state_transitions WHERE task_id = 'task-assigned'",
        [],
    );
    assert!(update.is_err(), "سجل الانتقالات لا يُعدَّل");
    assert!(delete.is_err(), "سجل الانتقالات لا يُحذف");
}

// =====================================================================
// السلسلة المؤسسية الكاملة — §20 و §31 (Scenario A→E) في اختبار واحد
// ---------------------------------------------------------------------
// من الالتزام إلى المهمة إلى الإسناد إلى التنفيذ إلى الاعتماد إلى الإغلاق،
// ثم التحقق أن اللوحة التنفيذية والتقرير يعكسان الحالة الناتجة فعلاً.
// هذا اختبار تكامل حقيقي: كل خطوة تمر بمسار الإنتاج نفسه (لا كتابة مباشرة).
// =====================================================================
#[test]
fn full_business_chain_from_obligation_to_executive_dashboard_and_report() {
    use crate::application::{
        dashboard_service::load_dashboard, report_service::generate_report,
        report_service::GenerateReportInput,
    };

    let state = state_with_task_lifecycle();
    let obligation_id = "ob-with-evidence";

    // 1) المنفّذ يُنشئ مهمة على التزام حقيقي — بلا إسناد صامت.
    as_actor(&state, "user-worker");
    let created = create_remediation_task(
        &state,
        CreateTaskInput {
            obligation_id: obligation_id.to_owned(),
            title: "Document retention controls".to_owned(),
            description: "Produce and reference the retention control evidence.".to_owned(),
            due_at: None,
            priority: "HIGH".to_owned(),
            assignee_user_id: None,
        },
    )
    .expect("إنشاء المهمة");
    let task_id = created.task_id.clone();
    assert_eq!(created.status, "OPEN");
    assert!(created.pending_approval_id.is_none());

    // 2) المدير يُسندها إلى المنفّذ ويحدد موعداً.
    as_actor(&state, "user-manager");
    let assigned = assign_task(
        &state,
        AssignTaskInput {
            task_id: task_id.clone(),
            assignee_user_id: "user-worker".to_owned(),
            due_at: Some("2027-01-15T00:00:00Z".to_owned()),
            expected_version: 1,
        },
    )
    .expect("الإسناد");
    assert_eq!(assigned.version, 2);

    // 3) المنفّذ يعمل ثم يطلب المراجعة (الدليل مسجَّل على الالتزام مسبقاً).
    as_actor(&state, "user-worker");
    transition(&state, &task_id, "IN_PROGRESS", 2).expect("بدء العمل");
    let submitted = transition(&state, &task_id, "READY_FOR_REVIEW", 3).expect("طلب المراجعة");
    let approval_id = submitted.pending_approval_id.expect("موافقة معلّقة");

    // 4) المدير يعتمد — والقرار يحرّك الحالة.
    as_actor(&state, "user-manager");
    decide_approval(
        &state,
        DecideApprovalInput {
            approval_id,
            decision: "APPROVED".to_owned(),
            reason: "Evidence reviewed and accepted.".to_owned(),
        },
    )
    .expect("الاعتماد");
    assert_eq!(stored_status(&state, &task_id).0, "APPROVED");

    // 5) المنفّذ يغلق المهمة.
    as_actor(&state, "user-worker");
    transition(&state, &task_id, "COMPLETED", 5).expect("الإغلاق");

    // 6) خطوة التحقق البشري (§14): جهة المراجعة تتحقق من الالتزام.
    //    بلا هذا المسار كانت «تغطية تم التحقق منها» صفراً بنيوياً للأبد.
    as_actor(&state, "user-manager");
    let obligation_version: i64 = {
        let connection = state.connection.lock().expect("القفل");
        connection
            .query_row(
                "SELECT version FROM obligations WHERE id = ?1",
                [obligation_id],
                |row| row.get(0),
            )
            .expect("إصدار الالتزام يجب أن يقرأ")
    };
    let verified = crate::application::obligation_service::verify_obligation(
        &state,
        crate::application::obligation_service::VerifyObligationInput {
            obligation_id: obligation_id.to_owned(),
            expected_version: u32::try_from(obligation_version).expect("إصدار صالح"),
        },
    )
    .expect("التحقق البشري يجب أن ينجح");
    assert_eq!(verified.verification_state, "HUMAN_VERIFIED");

    // 7) اللوحة التنفيذية تعكس الحالة من قاعدة البيانات نفسها.
    let dashboard = load_dashboard(&state).expect("قراءة اللوحة");
    assert!(dashboard
        .obligations
        .iter()
        .any(|obligation| obligation.id == obligation_id));
    // المقام المؤهل = التزامان في هذه التركيبة (كلاهما APPLIES)، والمتحقق منهما
    // واحد ⇒ 50%. الرقم يعكس المحفظة الفعلية لا التزاماً واحداً مختاراً.
    // قبل الإصلاح كان كلا المؤشرين صفراً بنيوياً: عمود الالتزام
    // المشتق لا يصل إلى VALID، وعمود التحقق لا يصل إلى HUMAN_VERIFIED من أي أمر.
    assert!(dashboard.readiness.sufficient_data);
    assert_eq!(dashboard.readiness.eligible_obligations, 2);
    assert_eq!(dashboard.readiness.verified_coverage, Some(50));
    assert_eq!(dashboard.readiness.evidence_completeness, Some(50));
    assert_eq!(dashboard.readiness.readiness_score, Some(50));
    // سلسلة التدقيق سليمة فعلاً بعد كل هذه الكتابات.
    assert_eq!(dashboard.audit_chain_integrity, "VALID");

    // 8) سجل المهام كما يراه المالك: مكتملة، وعدّادات حقيقية.
    as_actor(&state, "user-worker");
    let tasks = load_tasks(&state).expect("قراءة المهام");
    let closed = tasks
        .items
        .iter()
        .find(|item| item.id == task_id)
        .expect("المهمة يجب أن تُقرأ");
    assert_eq!(closed.status, TaskStatus::Completed);
    assert_eq!(closed.assignee_display_name.as_deref(), Some("user-worker"));
    assert!(closed.completed_at.is_some());
    assert!(closed.available_transitions.is_empty(), "حالة نهائية");
    assert_eq!(tasks.summary.completed, 1);
    // المالك يحمل مهمتين مزروعتين في التركيبة + المهمة المكتملة الجديدة = 3.
    assert_eq!(tasks.summary.assigned_to_me, 3);
    assert_eq!(tasks.summary.overdue, 0);

    // 8) سجل الانتقالات: أربعة انتقالات موثّقة بالترتيب وبفاعل حقيقي.
    let history = load_task_history(&state, &task_id).expect("قراءة السجل");
    let pairs: Vec<(TaskStatus, TaskStatus)> = history
        .iter()
        .map(|record| (record.from_status, record.to_status))
        .collect();
    assert_eq!(
        pairs,
        vec![
            (TaskStatus::Open, TaskStatus::InProgress),
            (TaskStatus::InProgress, TaskStatus::ReadyForReview),
            (TaskStatus::ReadyForReview, TaskStatus::Approved),
            (TaskStatus::Approved, TaskStatus::Completed),
        ]
    );
    assert!(history
        .iter()
        .all(|record| !record.actor_display_name.is_empty()));

    // 9) التقرير التشغيلي يُبنى من السجلات نفسها ويحمل منشأ معلناً.
    let report = generate_report(
        &state,
        GenerateReportInput {
            report_id: "TASK_WORKLOAD".to_owned(),
        },
    )
    .expect("توليد التقرير");
    assert_eq!(report.report_id, "TASK_WORKLOAD");
    assert!(!report.sections.is_empty());
    let rendered = format!("{:?}", report.sections);
    assert!(
        rendered.contains(&task_id) || rendered.contains("Document retention controls"),
        "التقرير يجب أن يعكس المهمة الحقيقية"
    );
}
