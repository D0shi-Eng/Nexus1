use rusqlite::{params, Connection};

use crate::domain::{
    error::AppError,
    models::{
        parse_data_mode, parse_task_priority, parse_task_status, TaskRecord, TaskSummary,
        TaskTransitionRecord, TasksData,
    },
    task_lifecycle,
};

/// بيانات العارض التي تحكم ما يُعرض له من إجراءات. تُمرَّر صراحةً بدل قراءة
/// هوية عامة داخل المستودع — فالمستودع لا يعرف الجلسة، والخدمة هي التي تقرر.
pub struct TaskViewer<'a> {
    pub user_id: &'a str,
    /// هل يملك العارض أهلية إدارة (إسناد/متابعة كل مهام المستأجر)؟
    pub is_manager: bool,
    /// هل يملك العارض قدرة نقل حالة المهمة (task:update)؟
    pub can_move: bool,
}

pub fn load_tasks(
    connection: &Connection,
    tenant_id: &str,
    generated_at: &str,
    viewer: &TaskViewer<'_>,
) -> Result<TasksData, AppError> {
    let data_mode = connection
        .query_row(
            "SELECT data_mode FROM tenants WHERE id = ?1",
            [tenant_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| AppError::DataAccess)?;

    let summary = load_summary(connection, tenant_id, viewer, generated_at)?;
    let items = load_items(connection, tenant_id, viewer)?;

    Ok(TasksData {
        has_data: !items.is_empty(),
        items,
        summary,
        data_mode: parse_data_mode(data_mode)?,
        generated_at: generated_at.to_owned(),
    })
}

// سجل الانتقالات الحقيقي لمهمة واحدة — دليل تدقيقي لا يُعدَّل ولا يُحذف.
pub fn load_task_transitions(
    connection: &Connection,
    tenant_id: &str,
    task_id: &str,
) -> Result<Vec<TaskTransitionRecord>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT transition.from_status, transition.to_status, actor.display_name,
                    transition.reason, transition.created_at
             FROM task_state_transitions transition
             INNER JOIN local_users actor
                ON actor.id = transition.actor_user_id
               AND actor.tenant_id = transition.tenant_id
             WHERE transition.tenant_id = ?1 AND transition.task_id = ?2
             ORDER BY transition.created_at ASC, transition.id ASC",
        )
        .map_err(|_| AppError::DataAccess)?;

    let rows = statement
        .query_map(params![tenant_id, task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|_| AppError::DataAccess)?;

    let mut records = Vec::new();
    for row in rows {
        let (from_status, to_status, actor_display_name, reason, occurred_at) =
            row.map_err(|_| AppError::DataAccess)?;
        records.push(TaskTransitionRecord {
            from_status: parse_task_status(from_status)?,
            to_status: parse_task_status(to_status)?,
            actor_display_name,
            reason,
            occurred_at,
        });
    }
    Ok(records)
}

fn load_summary(
    connection: &Connection,
    tenant_id: &str,
    viewer: &TaskViewer<'_>,
    generated_at: &str,
) -> Result<TaskSummary, AppError> {
    // كل العدّ من نفس الجدول ونفس نطاق المستأجر. «متأخرة» تُحسب بمقارنة
    // الموعد بلحظة التوليد داخل SQLite، ولا تُخزَّن قيمة مشتقة قد تتقادم.
    connection
        .query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN assignee_user_id = ?2 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'OPEN' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'IN_PROGRESS' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'BLOCKED' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'READY_FOR_REVIEW' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'APPROVED' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'REJECTED' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'COMPLETED' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN due_at IS NOT NULL AND due_at < ?3
                                   AND status <> 'COMPLETED' THEN 1 ELSE 0 END), 0)
             FROM tasks
             WHERE tenant_id = ?1",
            params![tenant_id, viewer.user_id, generated_at],
            |row| {
                Ok(TaskSummary {
                    total: as_count(row.get(0)?),
                    assigned_to_me: as_count(row.get(1)?),
                    open: as_count(row.get(2)?),
                    in_progress: as_count(row.get(3)?),
                    blocked: as_count(row.get(4)?),
                    ready_for_review: as_count(row.get(5)?),
                    approved: as_count(row.get(6)?),
                    rejected: as_count(row.get(7)?),
                    completed: as_count(row.get(8)?),
                    overdue: as_count(row.get(9)?),
                })
            },
        )
        .map_err(|_| AppError::DataAccess)
}

fn load_items(
    connection: &Connection,
    tenant_id: &str,
    viewer: &TaskViewer<'_>,
) -> Result<Vec<TaskRecord>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT
                tasks.id,
                tasks.obligation_id,
                obligations.title,
                tasks.title,
                tasks.description,
                tasks.due_at,
                tasks.priority,
                tasks.status,
                tasks.assignee_user_id,
                assignee.display_name,
                tasks.version,
                tasks.completed_at,
                tasks.created_at,
                tasks.updated_at
             FROM tasks
             INNER JOIN obligations
                ON obligations.id = tasks.obligation_id
               AND obligations.tenant_id = tasks.tenant_id
             LEFT JOIN local_users assignee
                ON assignee.id = tasks.assignee_user_id
               AND assignee.tenant_id = tasks.tenant_id
             WHERE tasks.tenant_id = ?1
             ORDER BY
                CASE tasks.priority
                    WHEN 'CRITICAL' THEN 0
                    WHEN 'HIGH' THEN 1
                    WHEN 'MEDIUM' THEN 2
                    WHEN 'LOW' THEN 3
                    ELSE 4
                END ASC,
                tasks.updated_at DESC,
                tasks.id ASC",
        )
        .map_err(|_| AppError::DataAccess)?;

    let rows = statement
        .query_map(params![tenant_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
            ))
        })
        .map_err(|_| AppError::DataAccess)?;

    let mut records = Vec::new();
    for row in rows {
        let (
            id,
            obligation_id,
            obligation_title,
            title,
            description,
            due_at,
            priority,
            status,
            assignee_user_id,
            assignee_display_name,
            version,
            completed_at,
            created_at,
            updated_at,
        ) = row.map_err(|_| AppError::DataAccess)?;

        let status = parse_task_status(status)?;
        // الإجراءات المتاحة تُحسب للعارض بعينه: منفّذ العمل يرى انتقالات
        // التنفيذ، والمدير يرى ما يملكه، ومن لا يملك شيئاً يرى قائمة فارغة.
        let is_performer = assignee_user_id.as_deref() == Some(viewer.user_id);
        let available_transitions = if viewer.can_move && (is_performer || viewer.is_manager) {
            task_lifecycle::performer_transitions(status)
        } else {
            Vec::new()
        };
        // الإسناد: مدير يملك task:assign، أو التقاط ذاتي لمهمة بلا مالك.
        let can_assign = (viewer.is_manager || (assignee_user_id.is_none() && viewer.can_move))
            && !task_lifecycle::is_terminal(status);

        records.push(TaskRecord {
            id,
            obligation_id,
            obligation_title,
            title,
            description,
            due_at,
            priority: parse_task_priority(priority)?,
            status,
            assignee_user_id,
            assignee_display_name,
            version: u32::try_from(version).map_err(|_| AppError::DataAccess)?,
            completed_at,
            available_transitions,
            can_assign,
            created_at,
            updated_at,
        });
    }
    Ok(records)
}

fn as_count(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
