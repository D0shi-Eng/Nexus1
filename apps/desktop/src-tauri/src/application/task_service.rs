use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        models::{
            parse_task_status, task_status_code, TaskStatus, TaskTransitionResult, TasksData,
        },
        task_lifecycle::{self, TaskTransitionActor, TaskTransitionRejection},
    },
    infrastructure::{
        database::{append_audit_event, latest_audit_event_hash, AppState},
        task_repository::{self, TaskViewer},
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskInput {
    pub obligation_id: String,
    pub title: String,
    pub description: String,
    pub due_at: Option<String>,
    pub priority: String,
    /// الإسناد اختياري عند الإنشاء. غيابه يعني مهمة بلا مالك — حالة صادقة
    /// تُعرض كما هي، ولا تُنسب صامتةً إلى من أنشأها.
    pub assignee_user_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedTask {
    pub task_id: String,
    pub status: String,
    /// لا تُنشأ موافقة عند الإنشاء — تُنشأ عند طلب المراجعة. الحقل صريح
    /// بدل قيمة ثابتة مضلِّلة.
    pub pending_approval_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignTaskInput {
    pub task_id: String,
    pub assignee_user_id: String,
    pub due_at: Option<String>,
    pub expected_version: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionTaskInput {
    pub task_id: String,
    pub target_status: String,
    pub reason: Option<String>,
    /// حماية من التزامن: الإصدار الذي رآه المستخدم. تغيّر المهمة يعني رفضاً.
    pub expected_version: u32,
}

pub fn load_tasks(state: &AppState) -> Result<TasksData, AppError> {
    authorization_service::require_permission(state, "TASK", "READ")?;

    let generated_at = Utc::now().to_rfc3339();
    let user_id = state.current_user_id()?;
    let tenant_id = state.tenant_id();
    // أهلية الإدارة وقدرة النقل تُقرآن من نفس مسار الصلاحيات الوحيد —
    // لا مصدر ثانٍ للحقيقة، وما تعرضه الواجهة هو ما يقبله الخلف.
    let is_manager =
        authorization_service::has_permission(state, &tenant_id, &user_id, "TASK", "ASSIGN");
    let can_move =
        authorization_service::has_permission(state, &tenant_id, &user_id, "TASK", "UPDATE");
    let viewer = TaskViewer {
        user_id: &user_id,
        is_manager,
        can_move,
    };
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    task_repository::load_tasks(&connection, &tenant_id, &generated_at, &viewer)
}

// سجل الانتقالات الحقيقي لمهمة — دليل قابل للتدقيق على من حرّكها ومتى ولِمَ.
pub fn load_task_history(
    state: &AppState,
    task_id: &str,
) -> Result<Vec<crate::domain::models::TaskTransitionRecord>, AppError> {
    authorization_service::require_permission(state, "TASK", "READ")?;
    if task_id.trim().is_empty() {
        return Err(AppError::ValidationFailed);
    }
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = ?1 AND tenant_id = ?2)",
            params![task_id, &state.tenant_id()],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)?;
    if !exists {
        return Err(AppError::ResourceNotFound);
    }
    task_repository::load_task_transitions(&connection, &state.tenant_id(), task_id)
}

pub fn create_remediation_task(
    state: &AppState,
    input: CreateTaskInput,
) -> Result<CreatedTask, AppError> {
    authorization_service::require_permission(state, "TASK", "CREATE")?;
    validate_create_input(&input)?;

    let now = Utc::now().to_rfc3339();
    let task_id = format!("task_{}", Uuid::new_v4());
    let audit_id = format!("audit_{}", Uuid::new_v4());
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    // عزل المستأجرين: الالتزام الأب يجب أن يكون في مستأجر الطالب نفسه.
    let obligation_exists = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM obligations WHERE id = ?1 AND tenant_id = ?2)",
            params![&input.obligation_id, &state.tenant_id()],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| AppError::StorageOperation)?;
    if !obligation_exists {
        return Err(AppError::ResourceNotFound);
    }

    // الإسناد عند الإنشاء: يُقبل فقط لعضو نشط في نفس المؤسسة.
    let assignee = match input.assignee_user_id.as_deref().map(str::trim) {
        Some(value) if !value.is_empty() => {
            if !is_active_member(&transaction, &state.tenant_id(), value)? {
                return Err(AppError::TaskAssignmentInvalid);
            }
            Some(value.to_owned())
        }
        _ => None,
    };

    transaction
        .execute(
            "INSERT INTO tasks (id, tenant_id, obligation_id, title, description, assignee_user_id, due_at, priority, status, version, completed_at, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'OPEN', 1, NULL, ?9, ?9)",
            params![
                &task_id,
                &state.tenant_id(),
                &input.obligation_id,
                input.title.trim(),
                input.description.trim(),
                &assignee,
                &input.due_at,
                input.priority,
                &now,
            ],
        )
        .map_err(|_| AppError::StorageOperation)?;

    let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
    append_audit_event(
        &transaction,
        &state.tenant_id(),
        &audit_id,
        "TASK_CREATED",
        &state.current_user_id()?,
        "Task",
        &task_id,
        &now,
        &previous_hash,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(CreatedTask {
        task_id,
        status: task_code(TaskStatus::Open).to_owned(),
        // لا موافقة عند الإنشاء — تُنشأ عند طلب المراجعة (انظر transition_task).
        pending_approval_id: None,
    })
}

pub fn assign_task(
    state: &AppState,
    input: AssignTaskInput,
) -> Result<TaskTransitionResult, AppError> {
    let now = Utc::now().to_rfc3339();
    let actor_id = state.current_user_id()?;
    // تُحسب قبل قفل الاتصال: `has_permission` يقفل الاتصال بنفسه، وقفل std
    // غير معاد الدخول فنداؤه تحت القفل جمود.
    let can_assign_anyone = authorization_service::has_permission(
        state,
        &state.tenant_id(),
        &actor_id,
        "TASK",
        "ASSIGN",
    );
    // الالتقاط الذاتي فعل تنفيذي لا إداري: يستلزم قدرة نقل حالة المهمة.
    let can_move_task = authorization_service::has_permission(
        state,
        &state.tenant_id(),
        &actor_id,
        "TASK",
        "UPDATE",
    );
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    let task = load_task_for_update(&transaction, &state.tenant_id(), input.task_id.trim())?;
    if task.version != input.expected_version {
        return Err(AppError::Conflict);
    }

    let target = input.assignee_user_id.trim();
    if !is_active_member(&transaction, &state.tenant_id(), target)? {
        return Err(AppError::TaskAssignmentInvalid);
    }

    // الأهلية: صلاحية إسناد صريحة، أو — عند غيابها — التقاط ذاتي لمهمة بلا
    // مالك بشرط قدرة نقل حالة المهمة. لا مسار يمنح إسناداً بلا أهلية.
    let is_self_claim = task.assignee_user_id.is_none() && target == actor_id;
    if can_assign_anyone {
        // مسار إداري: أي عضو نشط في المستأجر.
    } else if is_self_claim && can_move_task {
        // مسار التقاط ذاتي مشروع.
    } else {
        return Err(AppError::AuthorizationDenied);
    }

    // حالة نهائية: لا إعادة توجيه عمل مُغلق.
    if task_lifecycle::is_terminal(task.status) {
        return Err(AppError::InvalidStateTransition);
    }

    let new_version = task.version + 1;
    let changed = transaction
        .execute(
            "UPDATE tasks SET assignee_user_id = ?1, due_at = COALESCE(?2, due_at), version = ?3, updated_at = ?4
             WHERE id = ?5 AND tenant_id = ?6 AND version = ?7",
            params![
                target,
                &input.due_at,
                new_version,
                &now,
                &task.id,
                &state.tenant_id(),
                task.version,
            ],
        )
        .map_err(|_| AppError::StorageOperation)?;
    if changed != 1 {
        return Err(AppError::Conflict);
    }

    let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
    append_audit_event(
        &transaction,
        &state.tenant_id(),
        &format!("audit_{}", Uuid::new_v4()),
        "TASK_ASSIGNED",
        &actor_id,
        "Task",
        &task.id,
        &now,
        &previous_hash,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(TaskTransitionResult {
        task_id: task.id,
        previous_status: task.status,
        status: task.status,
        version: new_version,
        pending_approval_id: None,
    })
}

/// نقل حالة المهمة داخل آلة الحالة. كل ما يخصّ الاعتماد (APPROVED/REJECTED)
/// يمر بمسار الموافقات ولا يُنفَّذ هنا — وهذا مقصود ومُختبر.
pub fn transition_task(
    state: &AppState,
    input: TransitionTaskInput,
) -> Result<TaskTransitionResult, AppError> {
    authorization_service::require_permission(state, "TASK", "UPDATE")?;
    let target = parse_task_status(input.target_status.trim().to_owned())?;
    let reason = normalize_reason(&input.reason)?;

    let now = Utc::now().to_rfc3339();
    let actor_id = state.current_user_id()?;
    let tenant_id = state.tenant_id();
    // الأهلية تُحسب **قبل** قفل الاتصال: `has_permission` يقفل الاتصال بنفسه،
    // وقفل std غير معاد الدخول فنداؤه تحت القفل جمود محتم.
    let is_manager =
        authorization_service::has_permission(state, &tenant_id, &actor_id, "TASK", "ASSIGN");
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    let task = load_task_for_update(&transaction, &tenant_id, input.task_id.trim())?;

    // تزامن متفائل: أي تغيّر منذ أن رأى المستخدم المهمة يُرفض بلا كتابة.
    if task.version != input.expected_version {
        return Err(AppError::Conflict);
    }

    // الأهلية: المُسنَد إليه أو مدير المستأجر. غيرهما لا يحرّك عملاً ليس له.
    let is_performer = task.assignee_user_id.as_deref() == Some(actor_id.as_str());
    if !is_performer && !is_manager {
        return Err(AppError::AuthorizationDenied);
    }

    // آلة الحالة تقرر المخطط، ثم تُفرض الشروط المسبقة للبيانات.
    task_lifecycle::evaluate_transition(task.status, target, TaskTransitionActor::Performer)
        .map_err(map_transition_rejection)?;

    // شرط الملكية: لا يبدأ عملٌ بلا مالك معلوم.
    if !task_lifecycle::is_terminal(task.status)
        && task.assignee_user_id.is_none()
        && matches!(target, TaskStatus::InProgress)
    {
        return Err(AppError::TaskAssignmentInvalid);
    }

    // طلب المراجعة يشترط دليلاً مسجَّلاً على الالتزام، وينشئ الموافقة المعلّقة.
    let mut pending_approval_id = None;
    if target == TaskStatus::ReadyForReview {
        if !has_acceptable_evidence(&transaction, &tenant_id, &task.obligation_id)? {
            return Err(AppError::TaskEvidenceRequired);
        }
        let approval_id = format!("approval_{}", Uuid::new_v4());
        // الإصدار يُثبَّت على الإصدار الجديد بعد الانتقال: هو الإصدار الذي
        // يراجعه المعتمد، فأي تغيّر لاحق للمهمة يُبطل الموافقة (StaleApproval).
        let bound_version = task.version + 1;
        transaction
            .execute(
                "INSERT INTO approvals (id, tenant_id, resource_type, resource_id, resource_version, requested_by, required_role_code, decision, decision_by, decision_reason, decided_at, created_at, updated_at) VALUES (?1, ?2, 'TASK', ?3, ?4, ?5, 'TENANT_ADMIN', 'PENDING', NULL, NULL, NULL, ?6, ?6)",
                params![
                    &approval_id,
                    &tenant_id,
                    &task.id,
                    bound_version,
                    &actor_id,
                    &now,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        pending_approval_id = Some(approval_id);
    }

    let new_version = task.version + 1;
    let completed_at = (target == TaskStatus::Completed).then(|| now.clone());
    let changed = transaction
        .execute(
            "UPDATE tasks SET status = ?1, version = ?2, updated_at = ?3, completed_at = ?4
             WHERE id = ?5 AND tenant_id = ?6 AND version = ?7",
            params![
                task_code(target),
                new_version,
                &now,
                &completed_at,
                &task.id,
                &tenant_id,
                task.version,
            ],
        )
        .map_err(|_| AppError::StorageOperation)?;
    if changed != 1 {
        return Err(AppError::Conflict);
    }

    record_transition(
        &transaction,
        &tenant_id,
        &task.id,
        task.status,
        target,
        &actor_id,
        reason.as_deref(),
        new_version,
        &now,
    )?;

    let previous_hash = latest_audit_event_hash(&transaction, &tenant_id)?;
    append_audit_event(
        &transaction,
        &tenant_id,
        &format!("audit_{}", Uuid::new_v4()),
        transition_event_type(target),
        &actor_id,
        "Task",
        &task.id,
        &now,
        &previous_hash,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(TaskTransitionResult {
        task_id: task.id,
        previous_status: task.status,
        status: target,
        version: new_version,
        pending_approval_id,
    })
}

/// أثر قرار الموافقة على المهمة — يُستدعى من مسار الموافقات داخل معاملته.
/// لا يفحص الاعتماد الذاتي (فقد فُحص قبله في approval_service) بل يفحص أن
/// الموافقة ما زالت مطابقة لإصدار المهمة الحالي، فلا يُعتمد عمل تغيّر بعده.
pub(crate) fn transition_after_task_approval_decision(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    approval: &crate::infrastructure::approval_repository::ApprovalRecord,
    decision_reason: &str,
    occurred_at: &str,
) -> Result<(), AppError> {
    let tenant_id = state.tenant_id();
    let task = load_task_for_update(transaction, &tenant_id, &approval.resource_id)?;
    // الموافقة يجب أن تطابق الإصدار الذي رآه المعتمد: أي تغيّر لاحق للمهمة
    // (سحب الطلب، رفض سابق، إعادة عمل) يُبطل الموافقة فلا يُعتمد عمل قديم.
    if task.status != TaskStatus::ReadyForReview
        || i64::from(task.version) != approval.resource_version
    {
        return Err(AppError::StaleApproval);
    }
    let target = if approval.decision == "APPROVED" {
        TaskStatus::Approved
    } else {
        TaskStatus::Rejected
    };
    // نفس آلة الحالة تحكم هذا المسار أيضاً — لا قاعدة قرار مكرّرة في مكانين.
    task_lifecycle::evaluate_transition(task.status, target, TaskTransitionActor::Approver)
        .map_err(map_transition_rejection)?;
    let actor_id = state.current_user_id()?;
    let new_version = task.version + 1;
    let changed = transaction
        .execute(
            "UPDATE tasks SET status = ?1, version = ?2, updated_at = ?3
             WHERE id = ?4 AND tenant_id = ?5 AND version = ?6",
            params![
                task_code(target),
                new_version,
                occurred_at,
                &task.id,
                &tenant_id,
                task.version,
            ],
        )
        .map_err(|_| AppError::StorageOperation)?;
    if changed != 1 {
        return Err(AppError::Conflict);
    }
    record_transition(
        transaction,
        &tenant_id,
        &task.id,
        task.status,
        target,
        &actor_id,
        Some(decision_reason),
        new_version,
        occurred_at,
    )?;
    let previous_hash = latest_audit_event_hash(transaction, &tenant_id)?;
    append_audit_event(
        transaction,
        &tenant_id,
        &format!("audit_{}", Uuid::new_v4()),
        transition_event_type(target),
        &actor_id,
        "Task",
        &task.id,
        occurred_at,
        &previous_hash,
    )
    .map(|_| ())
}

// =====================================================================
// أدوات داخلية
// =====================================================================

struct TaskForUpdate {
    id: String,
    obligation_id: String,
    status: TaskStatus,
    version: u32,
    assignee_user_id: Option<String>,
}

fn load_task_for_update(
    connection: &Connection,
    tenant_id: &str,
    task_id: &str,
) -> Result<TaskForUpdate, AppError> {
    if task_id.is_empty() {
        return Err(AppError::ValidationFailed);
    }
    let row: Option<(String, String, String, i64, Option<String>)> = connection
        .query_row(
            "SELECT id, obligation_id, status, version, assignee_user_id
             FROM tasks WHERE id = ?1 AND tenant_id = ?2",
            params![task_id, tenant_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(|_| AppError::DataAccess)?;
    // مهمة مستأجر آخر تُعامل كغير موجودة — لا تسريب وجود عبر رمز خطأ مختلف.
    let (id, obligation_id, status, version, assignee_user_id) =
        row.ok_or(AppError::ResourceNotFound)?;
    Ok(TaskForUpdate {
        id,
        obligation_id,
        status: parse_task_status(status)?,
        version: u32::try_from(version).map_err(|_| AppError::DataAccess)?,
        assignee_user_id,
    })
}

fn is_active_member(
    connection: &Connection,
    tenant_id: &str,
    user_id: &str,
) -> Result<bool, AppError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM local_users WHERE id = ?1 AND tenant_id = ?2 AND status = 'ACTIVE')",
            params![user_id, tenant_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| AppError::DataAccess)
}

// شرط الدليل: وجود دليل واحد على الأقل على التزام المهمة بحالة تُعدّ دليلاً.
// MISSING وEXPIRED وINVALID ليست أدلة، فلا يجوز أن تُجيز طلب الاعتماد.
fn has_acceptable_evidence(
    connection: &Connection,
    tenant_id: &str,
    obligation_id: &str,
) -> Result<bool, AppError> {
    let mut statement = connection
        .prepare("SELECT status FROM evidence_items WHERE tenant_id = ?1 AND obligation_id = ?2")
        .map_err(|_| AppError::DataAccess)?;
    let rows = statement
        .query_map(params![tenant_id, obligation_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|_| AppError::DataAccess)?;
    for row in rows {
        let status = row.map_err(|_| AppError::DataAccess)?;
        if task_lifecycle::is_acceptable_evidence_status(&status) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn record_transition(
    transaction: &rusqlite::Transaction<'_>,
    tenant_id: &str,
    task_id: &str,
    from: TaskStatus,
    to: TaskStatus,
    actor_user_id: &str,
    reason: Option<&str>,
    resource_version: u32,
    occurred_at: &str,
) -> Result<(), AppError> {
    transaction
        .execute(
            "INSERT INTO task_state_transitions (id, tenant_id, task_id, from_status, to_status, actor_user_id, reason, resource_version, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                format!("transition_{}", Uuid::new_v4()),
                tenant_id,
                task_id,
                task_code(from),
                task_code(to),
                actor_user_id,
                reason,
                resource_version,
                occurred_at,
            ],
        )
        .map_err(|_| AppError::StorageOperation)?;
    Ok(())
}

fn map_transition_rejection(rejection: TaskTransitionRejection) -> AppError {
    match rejection {
        // القرار ليس انتقالاً يدوياً — محاولة تنفيذه من مسار المنفّذ رفض صريح.
        TaskTransitionRejection::ApprovalPathRequired => AppError::SelfApprovalForbidden,
        TaskTransitionRejection::TerminalState | TaskTransitionRejection::NotAllowed => {
            AppError::InvalidStateTransition
        }
    }
}

fn transition_event_type(target: TaskStatus) -> &'static str {
    match target {
        TaskStatus::Open => "TASK_STATUS_CHANGED",
        TaskStatus::InProgress => "TASK_STARTED",
        TaskStatus::Blocked => "TASK_BLOCKED",
        TaskStatus::ReadyForReview => "TASK_SUBMITTED_FOR_REVIEW",
        TaskStatus::Approved => "TASK_APPROVED",
        TaskStatus::Rejected => "TASK_REJECTED",
        TaskStatus::Completed => "TASK_COMPLETED",
    }
}

fn task_code(status: TaskStatus) -> &'static str {
    task_status_code(status)
}

fn normalize_reason(reason: &Option<String>) -> Result<Option<String>, AppError> {
    match reason.as_deref().map(str::trim) {
        Some(value) if value.is_empty() => Ok(None),
        Some(value) if value.chars().count() > 1_000 => Err(AppError::ValidationFailed),
        Some(value) => Ok(Some(value.to_owned())),
        None => Ok(None),
    }
}

fn validate_create_input(input: &CreateTaskInput) -> Result<(), AppError> {
    let priority_is_valid = matches!(
        input.priority.as_str(),
        "CRITICAL" | "HIGH" | "MEDIUM" | "LOW"
    );
    if input.obligation_id.trim().is_empty()
        || input.title.trim().is_empty()
        || input.title.chars().count() > 160
        || input.description.trim().is_empty()
        || input.description.chars().count() > 2_000
        || !priority_is_valid
    {
        return Err(AppError::ValidationFailed);
    }
    Ok(())
}

// جناح اختبارات دورة حياة المهمة في ملف مجاور (نفس نمط بقية خدمات MP05).
#[cfg(test)]
#[path = "task_service_tests.rs"]
mod tests;
