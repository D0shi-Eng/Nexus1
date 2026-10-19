use crate::domain::models::TaskStatus;

// =====================================================================
// آلة حالة المهمة — قواعد الانتقال الحتمية
// ---------------------------------------------------------------------
// لماذا وحدة مستقلة بلا قاعدة بيانات: آلة الحالة هذه هي «القانون» الذي
// يُختبر حتمياً بلا تهيئة، ويستدعيها الخلف داخل معاملة. فصلها يمنع تسرّب
// قاعدة انتقال واحدة إلى الواجهة أو إلى مسار آخر.
//
// العيب الذي أغلقت هذه الوحدة: قبلها لم تكن هناك أي آلة حالة إطلاقاً —
// المهمة تُنشأ OPEN ولا يوجد أي أمر ينقلها، وقرار الاعتماد كان يُكتب في
// جدول الموافقات ولا يحرّك المهمة. أي أن سلسلة العمل كانت تنقطع عند أول
// خطوة بعد الإنشاء.
// =====================================================================

// مسار الاعتماد المحكوم: من يطلب الاعتماد ومن يقرر. التصريح بقرار
// APPROVED/REJECTED عبر آلة الحالة ممنوع عمداً — قرار الاعتماد يجب أن يمر
// بمسار الموافقات (approval_service) لأنه هو من يفرض منع الاعتماد الذاتي
// وأهلية الدور. هذا هو مبدأ «انضباط الاعتماد» في المنتج.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskTransitionActor {
    /// منفّذ العمل: المُسنَد إليه أو مدير المستأجر.
    Performer,
    /// جهة الاعتماد: تمر عبر مسار الموافقات حصراً.
    Approver,
}

// نتيجة تقييم انتقال: إما مسموح، أو مرفوض بسبب صريح يُترجم إلى خطأ آمن.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskTransitionRejection {
    /// الانتقال غير موجود في المخطط.
    NotAllowed,
    /// الحالة المصدر نهائية (COMPLETED) — لا خروج منها.
    TerminalState,
    /// الانتقال حكر على جهة الاعتماد ويجب أن يمر بمسار الموافقات.
    ApprovalPathRequired,
}

/// الحالات النهائية: لا انتقال خارجها إطلاقاً.
pub fn is_terminal(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Completed)
}

/// تقييم مشروعية انتقال واحد. لا يقرأ قاعدة بيانات ولا يعرف هوية الفاعل
/// الدقيقة — يقرر المخطط وحده؛ أما الأهلية والبيانات فمحلها الخدمة.
pub fn evaluate_transition(
    from: TaskStatus,
    to: TaskStatus,
    actor: TaskTransitionActor,
) -> Result<(), TaskTransitionRejection> {
    if is_terminal(from) {
        return Err(TaskTransitionRejection::TerminalState);
    }
    if from == to {
        return Err(TaskTransitionRejection::NotAllowed);
    }

    // جهة الاعتماد تملك مسار الاعتماد وحده، ولا تحرّك عملاً بيدها.
    if actor == TaskTransitionActor::Approver {
        return match (from, to) {
            (TaskStatus::ReadyForReview, TaskStatus::Approved)
            | (TaskStatus::ReadyForReview, TaskStatus::Rejected) => Ok(()),
            _ => Err(TaskTransitionRejection::NotAllowed),
        };
    }

    // قرار الاعتماد لا يُنفَّذ من مسار منفّذ العمل أبداً — حتى لو كان مصرحاً.
    // هذا يمنع تجاوز فحص الاعتماد الذاتي عبر نداء مباشر لآلة الحالة.
    if matches!(to, TaskStatus::Approved | TaskStatus::Rejected) {
        return Err(TaskTransitionRejection::ApprovalPathRequired);
    }

    let allowed = match (from, to) {
        // بدء العمل أو تعليقه
        (TaskStatus::Open, TaskStatus::InProgress) => true,
        (TaskStatus::Open, TaskStatus::Blocked) => true,
        (TaskStatus::InProgress, TaskStatus::Blocked) => true,
        (TaskStatus::Blocked, TaskStatus::InProgress) => true,
        // طلب الاعتماد
        (TaskStatus::InProgress, TaskStatus::ReadyForReview) => true,
        // سحب الطلب قبل القرار (إعادة إلى العمل بلا اعتماد مرفوض)
        (TaskStatus::ReadyForReview, TaskStatus::InProgress) => true,
        // إعادة العمل بعد رفضه
        (TaskStatus::Rejected, TaskStatus::InProgress) => true,
        // إغلاق ما اعتُمد
        (TaskStatus::Approved, TaskStatus::Completed) => true,
        _ => false,
    };
    if !allowed {
        return Err(TaskTransitionRejection::NotAllowed);
    }
    Ok(())
}

/// الانتقالات المتاحة لمنفّذ العمل من حالة معيّنة — تُعرض في الواجهة كما هي
/// بدل تخمين الأزرار في العميل.
pub fn performer_transitions(from: TaskStatus) -> Vec<TaskStatus> {
    [
        TaskStatus::Open,
        TaskStatus::InProgress,
        TaskStatus::Blocked,
        TaskStatus::ReadyForReview,
        TaskStatus::Approved,
        TaskStatus::Rejected,
        TaskStatus::Completed,
    ]
    .into_iter()
    .filter(|candidate| {
        evaluate_transition(from, *candidate, TaskTransitionActor::Performer).is_ok()
    })
    .collect()
}

// حالات الدليل التي تُعدّ دليلاً مقبولاً لطلب الاعتماد. MISSING وEXPIRED
// وINVALID ليست أدلة — ولذلك لا يجوز أن يُطلب اعتماد إنجاز بلا دليل.
pub const ACCEPTABLE_EVIDENCE_STATUSES: [&str; 4] = ["UPLOADED", "PROCESSING", "ANALYZED", "VALID"];

pub fn is_acceptable_evidence_status(value: &str) -> bool {
    ACCEPTABLE_EVIDENCE_STATUSES.contains(&value)
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_transition, is_terminal, performer_transitions, TaskTransitionActor,
        TaskTransitionRejection,
    };
    use crate::domain::models::TaskStatus;

    const PERFORMER: TaskTransitionActor = TaskTransitionActor::Performer;
    const APPROVER: TaskTransitionActor = TaskTransitionActor::Approver;

    #[test]
    fn happy_path_runs_open_to_completed_through_approval_states() {
        assert!(evaluate_transition(TaskStatus::Open, TaskStatus::InProgress, PERFORMER).is_ok());
        assert!(evaluate_transition(
            TaskStatus::InProgress,
            TaskStatus::ReadyForReview,
            PERFORMER
        )
        .is_ok());
        assert!(
            evaluate_transition(TaskStatus::Approved, TaskStatus::Completed, PERFORMER).is_ok()
        );
    }

    #[test]
    fn approval_decisions_must_go_through_the_approval_path() {
        // مسار منفّذ العمل لا يملك قرار الاعتماد — ولو كان الفاعل معتمداً في دوره.
        assert_eq!(
            evaluate_transition(TaskStatus::ReadyForReview, TaskStatus::Approved, PERFORMER),
            Err(TaskTransitionRejection::ApprovalPathRequired)
        );
        assert_eq!(
            evaluate_transition(TaskStatus::ReadyForReview, TaskStatus::Rejected, PERFORMER),
            Err(TaskTransitionRejection::ApprovalPathRequired)
        );
        // جهة الاعتماد تملكه من READY_FOR_REVIEW حصراً.
        assert!(
            evaluate_transition(TaskStatus::ReadyForReview, TaskStatus::Approved, APPROVER).is_ok()
        );
        assert!(
            evaluate_transition(TaskStatus::ReadyForReview, TaskStatus::Rejected, APPROVER).is_ok()
        );
    }

    #[test]
    fn approver_actor_cannot_perform_or_skip_work_transitions() {
        assert_eq!(
            evaluate_transition(TaskStatus::Open, TaskStatus::InProgress, APPROVER),
            Err(TaskTransitionRejection::NotAllowed)
        );
        assert_eq!(
            evaluate_transition(TaskStatus::Approved, TaskStatus::Completed, APPROVER),
            Err(TaskTransitionRejection::NotAllowed)
        );
        assert_eq!(
            evaluate_transition(TaskStatus::Completed, TaskStatus::Approved, APPROVER),
            Err(TaskTransitionRejection::TerminalState)
        );
    }

    #[test]
    fn completed_is_terminal_and_cannot_be_reopened() {
        assert!(is_terminal(TaskStatus::Completed));
        for target in [
            TaskStatus::Open,
            TaskStatus::InProgress,
            TaskStatus::Blocked,
            TaskStatus::ReadyForReview,
            TaskStatus::Approved,
            TaskStatus::Rejected,
        ] {
            assert_eq!(
                evaluate_transition(TaskStatus::Completed, target, PERFORMER),
                Err(TaskTransitionRejection::TerminalState),
                "المهمة المكتملة يجب ألا تتحرك إلى {target:?}"
            );
        }
    }

    #[test]
    fn rejection_returns_work_to_the_performer() {
        assert!(
            evaluate_transition(TaskStatus::Rejected, TaskStatus::InProgress, PERFORMER).is_ok()
        );
    }

    #[test]
    fn self_transition_and_unmapped_jumps_are_refused() {
        assert_eq!(
            evaluate_transition(TaskStatus::Open, TaskStatus::Open, PERFORMER),
            Err(TaskTransitionRejection::NotAllowed)
        );
        assert_eq!(
            evaluate_transition(TaskStatus::Open, TaskStatus::Completed, PERFORMER),
            Err(TaskTransitionRejection::NotAllowed)
        );
        assert_eq!(
            evaluate_transition(TaskStatus::Blocked, TaskStatus::ReadyForReview, PERFORMER),
            Err(TaskTransitionRejection::NotAllowed)
        );
    }

    #[test]
    fn offered_transitions_never_include_approval_states_or_illegal_jumps() {
        let offered = performer_transitions(TaskStatus::ReadyForReview);
        assert_eq!(offered, vec![TaskStatus::InProgress]);
        let from_open = performer_transitions(TaskStatus::Open);
        assert_eq!(from_open, vec![TaskStatus::InProgress, TaskStatus::Blocked]);
        assert!(performer_transitions(TaskStatus::Completed).is_empty());
    }
}
