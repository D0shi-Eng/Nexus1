use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HealthState {
    Healthy,
    Degraded,
    NotConfigured,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReference {
    pub authority: String,
    pub document_title: String,
    pub source_url: Option<String>,
    pub version: Option<String>,
    pub section: Option<String>,
    pub retrieved_at: String,
    pub official: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObligationSummary {
    pub id: String,
    pub tenant_id: String,
    pub title: String,
    pub authority: String,
    pub owner: String,
    pub deadline: Option<String>,
    pub severity: String,
    pub status: String,
    pub applicability: String,
    pub evidence_status: String,
    pub verification_state: String,
    pub source: SourceReference,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionRelatedEntity {
    pub kind: String,
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionItem {
    pub id: String,
    pub kind: String,
    pub severity: String,
    pub title: String,
    pub status: String,
    pub reason_code: String,
    pub source: String,
    pub due_at: Option<String>,
    pub resource_id: String,
    pub related_entity: Option<AttentionRelatedEntity>,
    pub owner: Option<String>,
    pub updated_at: String,
    pub available_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionSummary {
    pub total: u32,
    pub critical: u32,
    pub high: u32,
    pub medium: u32,
    pub low: u32,
    pub pending: u32,
    pub overdue: u32,
    pub requires_review: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttentionData {
    pub items: Vec<AttentionItem>,
    pub summary: AttentionSummary,
    pub data_mode: String,
    pub generated_at: String,
}

// تقدير الجاهزية. كل نسبة `Option<u8>`: غيابها يعني «لا توجد بيانات كافية»
// لا صفراً. عرض صفر عند غياب البيانات ادعاء كاذب بأن القياس جرى وفشل.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessSnapshot {
    pub tenant_id: String,
    pub readiness_score: Option<u8>,
    pub verified_coverage: Option<u8>,
    pub evidence_completeness: Option<u8>,
    pub open_critical_items: u32,
    /// مقام النسبة المعلن: الالتزامات التي ينطبق عليها الالتزام فعلاً.
    pub eligible_obligations: u32,
    /// الالتزامات المؤرشفة المستبعدة من كل مؤشرات الحالة النشطة. تُعرض صراحة
    /// كي يفهم المستخدم سبب اختلاف العدد عن قائمة الالتزامات الكاملة.
    pub archived_obligations: u32,
    /// هل البيانات كافية لاحتساب نسبة أصلاً؟
    pub sufficient_data: bool,
    pub generated_at: String,
    /// إفصاح يتبع وضع البيانات الفعلي (ديمو/إنتاج) بدل وسم ثابت.
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEventSummary {
    pub id: String,
    pub event_type: String,
    pub actor_name: String,
    pub resource_type: String,
    pub resource_id: String,
    pub occurred_at: String,
    /// محسوبة من التحقق الفعلي لسلسلة التجزئة، لا قيمة ثابتة.
    pub integrity_status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardData {
    pub attention_items: Vec<AttentionItem>,
    pub obligations: Vec<ObligationSummary>,
    pub readiness: ReadinessSnapshot,
    pub recent_audit_events: Vec<AuditEventSummary>,
    /// سلامة سلسلة التدقيق للمستأجر كاملاً: VALID أو INVALID محسوبة.
    pub audit_chain_integrity: String,
    /// وضع بيانات المستأجر الفعلي — تُبنى عليه ملاحظات المنشأ في الواجهة بدل
    /// وسم ثابت يدّعي أن كل البيانات تركيبية حتى في بيئة إنتاجية حقيقية.
    pub data_mode: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentFailureCounts {
    pub agent_runs_failed_24h: i64,
    pub provider_errors_24h: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
    pub application: HealthState,
    pub database: HealthState,
    pub ai: HealthState,
    pub checked_at: String,
    // حقول رصد MP05 — هوية البيئة وإحصاءات إخفاق محلية بلا أي بيانات حساسة.
    pub app_version: String,
    pub schema_version: i64,
    pub runtime_mode: String,
    pub configured_model_id: Option<String>,
    pub recent_failures: RecentFailureCounts,
}

// نتيجة فحص المزود الحي الصريح — إجراء مستخدم بمهلة قصيرة لا ضمن كل استعلام صحة.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderCheck {
    pub reachable: bool,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentRunStatus {
    Queued,
    Running,
    WaitingApproval,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunRecord {
    pub id: String,
    pub tenant_id: String,
    pub agent_id: String,
    pub requested_by_user_id: String,
    pub status: AgentRunStatus,
    pub request_summary: String,
    pub revision: i64,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunsData {
    pub has_data: bool,
    pub items: Vec<AgentRunRecord>,
}

pub fn is_valid_agent_run_transition(from: &AgentRunStatus, to: &AgentRunStatus) -> bool {
    matches!(
        (from, to),
        (AgentRunStatus::Queued, AgentRunStatus::Running)
            | (AgentRunStatus::Queued, AgentRunStatus::WaitingApproval)
            | (AgentRunStatus::Queued, AgentRunStatus::Cancelled)
            | (AgentRunStatus::Running, AgentRunStatus::WaitingApproval)
            | (AgentRunStatus::Running, AgentRunStatus::Completed)
            | (AgentRunStatus::Running, AgentRunStatus::Failed)
            | (AgentRunStatus::Running, AgentRunStatus::Cancelled)
            | (AgentRunStatus::WaitingApproval, AgentRunStatus::Queued)
            | (AgentRunStatus::WaitingApproval, AgentRunStatus::Running)
            | (AgentRunStatus::WaitingApproval, AgentRunStatus::Cancelled)
    )
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentStatus {
    Active,
    Disabled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRecord {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub description: String,
    pub status: AgentStatus,
    pub capability: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentsData {
    pub has_data: bool,
    pub items: Vec<AgentRecord>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ObligationStatus {
    Open,
    InProgress,
    AtRisk,
    Completed,
    RequiresReview,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ObligationPriority {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ComplianceStatus {
    Compliant,
    PartiallyCompliant,
    NonCompliant,
    NotAssessed,
    RequiresReview,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewStatus {
    NotReviewed,
    InReview,
    Reviewed,
    Rejected,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SourceType {
    Official,
    Internal,
    Demo,
    Synthetic,
    UserProvided,
    Unknown,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataMode {
    DemoSynthetic,
    Production,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskStatus {
    Open,
    Mitigating,
    Accepted,
    Closed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskSeverity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskRecord {
    pub id: String,
    pub title: String,
    pub severity: RiskSeverity,
    pub likelihood: u8,
    pub impact: u8,
    pub status: RiskStatus,
    pub owner: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskSummary {
    pub total: u32,
    pub open: u32,
    pub mitigating: u32,
    pub accepted: u32,
    pub closed: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RisksData {
    pub items: Vec<RiskRecord>,
    pub summary: RiskSummary,
    pub data_mode: DataMode,
    pub generated_at: String,
    pub has_data: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    Open,
    InProgress,
    Blocked,
    ReadyForReview,
    Approved,
    Rejected,
    Completed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskPriority {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub id: String,
    pub obligation_id: String,
    pub obligation_title: String,
    pub title: String,
    pub description: String,
    pub due_at: Option<String>,
    pub priority: TaskPriority,
    pub status: TaskStatus,
    /// الإسناد الحقيقي: من يحمل المهمة فعلاً. غيابه يعني مهمة غير مُسنَدة،
    /// وهي حالة صادقة تُعرض كما هي بدل نسبة العمل إلى من أنشأه.
    pub assignee_user_id: Option<String>,
    pub assignee_display_name: Option<String>,
    pub version: u32,
    pub completed_at: Option<String>,
    /// الانتقالات المسموحة لهذا العارض بعينه كما يحسبها الخلف — لا تُخمَّن في
    /// العميل. فارغة تعني «لا إجراء متاح لك»، وهي إجابة صادقة لا تعطيل واجهة.
    pub available_transitions: Vec<TaskStatus>,
    /// هل يملك العارض إسناد هذه المهمة (إدارة أو التقاط ذاتي لمهمة بلا مالك)؟
    pub can_assign: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub total: u32,
    /// المهام المُسنَدة إلى العارض الحالي — أساس «مهامي».
    pub assigned_to_me: u32,
    pub open: u32,
    pub in_progress: u32,
    pub blocked: u32,
    pub ready_for_review: u32,
    pub approved: u32,
    pub rejected: u32,
    pub completed: u32,
    /// مهام تجاوزت موعدها ولم تُغلق — محسوبة من القاعدة لا من ثابت.
    pub overdue: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskTransitionRecord {
    pub from_status: TaskStatus,
    pub to_status: TaskStatus,
    pub actor_display_name: String,
    pub reason: Option<String>,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TasksData {
    pub items: Vec<TaskRecord>,
    pub summary: TaskSummary,
    pub data_mode: DataMode,
    pub generated_at: String,
    pub has_data: bool,
}

// نتيجة انتقال حالة مهمة — تُعاد للواجهة لتحديث العرض بلا تخمين.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskTransitionResult {
    pub task_id: String,
    pub previous_status: TaskStatus,
    pub status: TaskStatus,
    pub version: u32,
    /// الاعتماد المعلّق الناتج عن طلب المراجعة (إن وُجد) — يقرر الجهة المصرح لها.
    pub pending_approval_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ControlStatus {
    Active,
    Inactive,
    Disabled,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ControlEffectiveness {
    NotAssessed,
    Effective,
    PartiallyEffective,
    Ineffective,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ControlRecord {
    pub id: String,
    pub title: String,
    pub status: ControlStatus,
    pub category: Option<String>,
    pub owner: Option<String>,
    pub effectiveness: ControlEffectiveness,
    pub last_assessed_at: Option<String>,
    pub linked_obligation_count: u32,
    pub linked_risk_count: u32,
    pub linked_evidence_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ControlObligationLink {
    pub obligation_id: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ControlRiskLink {
    pub risk_id: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ControlEvidenceLink {
    pub evidence_id: String,
    pub label: String,
    pub status: String,
    pub classification: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlDetail {
    #[serde(flatten)]
    pub control: ControlRecord,
    pub assessment_note: Option<String>,
    pub linked_obligations: Vec<ControlObligationLink>,
    pub linked_risks: Vec<ControlRiskLink>,
    pub linked_evidence: Vec<ControlEvidenceLink>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ControlSummary {
    pub total: u32,
    pub active: u32,
    pub inactive: u32,
    pub disabled: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlsData {
    pub items: Vec<ControlRecord>,
    pub summary: ControlSummary,
    pub data_mode: DataMode,
    pub generated_at: String,
    pub has_data: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceClassification {
    Public,
    Internal,
    Confidential,
    Restricted,
    HighlyRestricted,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceStatus {
    Missing,
    Uploaded,
    Processing,
    Analyzed,
    Valid,
    Expired,
    Invalid,
    RequiresReview,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceRecord {
    pub id: String,
    pub obligation_id: String,
    pub obligation_title: String,
    pub label: String,
    pub classification: EvidenceClassification,
    pub status: EvidenceStatus,
    pub expires_at: Option<String>,
    /// الحالة الفعلية المعروضة: تساوي `status` إلا إذا انقضى `expires_at`
    /// فتصبح EXPIRED. مشتقة من القاعدة والوقت الحالي، ولا تُكتب فوق الحالة
    /// المخزَّنة كي لا يُعاد كتابة تاريخ الدليل نفسه.
    pub effective_status: EvidenceStatus,
    /// هل الدليل منتهي الصلاحية في اللحظة الحالية؟
    pub is_expired: bool,
    /// هوية من قدّم الدليل — غيابها يعني سجلاً تاريخياً سابقاً لعمود `registered_by`.
    pub registered_by: Option<String>,
    /// تصنيف صريح للمنشأ: RECORDED أو UNKNOWN_LEGACY (سجل قديم بلا هوية مقدّم).
    pub registered_by_state: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceSummary {
    pub total: u32,
    pub requires_review: u32,
    pub expired: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceData {
    pub items: Vec<EvidenceRecord>,
    pub summary: EvidenceSummary,
    pub data_mode: DataMode,
    pub generated_at: String,
    pub has_data: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObligationListItem {
    pub id: String,
    pub tenant_id: String,
    pub title: String,
    pub description: String,
    pub status: ObligationStatus,
    pub priority: ObligationPriority,
    pub source_id: String,
    pub source_reference: String,
    pub source_type: SourceType,
    pub source: SourceReference,
    pub owner: String,
    pub due_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub review_status: ReviewStatus,
    pub compliance_status: ComplianceStatus,
    pub data_mode: DataMode,
    pub is_archived: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObligationEvidenceReference {
    pub id: String,
    pub label: String,
    pub classification: String,
    pub status: String,
    pub expires_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObligationTaskReference {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub due_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObligationRiskReference {
    pub id: String,
    pub title: String,
    pub status: String,
    pub severity: String,
    pub owner: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObligationApprovalReference {
    pub id: String,
    pub resource_type: String,
    pub decision: String,
    pub required_role_code: String,
    pub created_at: String,
    pub decided_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObligationRelatedResources {
    pub evidence: Vec<ObligationEvidenceReference>,
    pub tasks: Vec<ObligationTaskReference>,
    pub risks: Vec<ObligationRiskReference>,
    pub approvals: Vec<ObligationApprovalReference>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObligationDetails {
    #[serde(flatten)]
    pub obligation: ObligationListItem,
    pub related_resources: ObligationRelatedResources,
    pub version: i64,
}

// نتيجة التحقق البشري من التزام — تُعاد للواجهة لتحديث العرض بلا تخمين.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedObligation {
    pub id: String,
    pub verification_state: String,
    pub verified_at: String,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObligationSummaryMetrics {
    pub total: u32,
    pub critical: u32,
    pub due_soon: u32,
    pub overdue: u32,
    pub non_compliant: u32,
    pub under_review: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObligationsData {
    pub items: Vec<ObligationListItem>,
    pub summary: ObligationSummaryMetrics,
    pub data_mode: DataMode,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatedObligation {
    pub id: String,
    pub updated_at: String,
    pub version: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedObligation {
    pub id: String,
    pub archived_at: String,
}

// نتيجة تأليف التزام جديد: تُعاد للواجهة لتفتح السجل المنشأ فعلاً.
// `sourceType` و`verificationState` صريحان هنا حتى لا تفترض الواجهة منشأ رسمياً.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedObligation {
    pub id: String,
    pub status: String,
    pub source_type: String,
    pub verification_state: String,
    pub created_at: String,
}

pub fn parse_obligation_status(
    value: String,
) -> Result<ObligationStatus, crate::domain::error::AppError> {
    match value.as_str() {
        "OPEN" => Ok(ObligationStatus::Open),
        "IN_PROGRESS" => Ok(ObligationStatus::InProgress),
        "AT_RISK" => Ok(ObligationStatus::AtRisk),
        "COMPLETED" => Ok(ObligationStatus::Completed),
        "REQUIRES_REVIEW" => Ok(ObligationStatus::RequiresReview),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

pub fn parse_obligation_priority(
    value: String,
) -> Result<ObligationPriority, crate::domain::error::AppError> {
    match value.as_str() {
        "CRITICAL" => Ok(ObligationPriority::Critical),
        "HIGH" => Ok(ObligationPriority::High),
        "MEDIUM" => Ok(ObligationPriority::Medium),
        "LOW" => Ok(ObligationPriority::Low),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

pub fn parse_compliance_status(
    value: String,
) -> Result<ComplianceStatus, crate::domain::error::AppError> {
    match value.as_str() {
        "COMPLIANT" => Ok(ComplianceStatus::Compliant),
        "PARTIALLY_COMPLIANT" => Ok(ComplianceStatus::PartiallyCompliant),
        "NON_COMPLIANT" => Ok(ComplianceStatus::NonCompliant),
        "NOT_ASSESSED" => Ok(ComplianceStatus::NotAssessed),
        "REQUIRES_REVIEW" => Ok(ComplianceStatus::RequiresReview),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

pub fn parse_review_status(value: String) -> Result<ReviewStatus, crate::domain::error::AppError> {
    match value.as_str() {
        "NOT_REVIEWED" => Ok(ReviewStatus::NotReviewed),
        "IN_REVIEW" => Ok(ReviewStatus::InReview),
        "REVIEWED" => Ok(ReviewStatus::Reviewed),
        "REJECTED" => Ok(ReviewStatus::Rejected),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

pub fn parse_source_type(value: String) -> Result<SourceType, crate::domain::error::AppError> {
    match value.as_str() {
        "OFFICIAL" => Ok(SourceType::Official),
        "INTERNAL" => Ok(SourceType::Internal),
        "DEMO" => Ok(SourceType::Demo),
        "SYNTHETIC" => Ok(SourceType::Synthetic),
        "USER_PROVIDED" => Ok(SourceType::UserProvided),
        "UNKNOWN" => Ok(SourceType::Unknown),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

pub fn parse_data_mode(value: String) -> Result<DataMode, crate::domain::error::AppError> {
    match value.as_str() {
        "DEMO_SYNTHETIC" => Ok(DataMode::DemoSynthetic),
        "PRODUCTION" => Ok(DataMode::Production),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

pub fn parse_agent_status(value: String) -> Result<AgentStatus, crate::domain::error::AppError> {
    match value.as_str() {
        "ACTIVE" => Ok(AgentStatus::Active),
        "DISABLED" => Ok(AgentStatus::Disabled),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

pub fn parse_agent_run_status(
    value: String,
) -> Result<AgentRunStatus, crate::domain::error::AppError> {
    match value.as_str() {
        "QUEUED" => Ok(AgentRunStatus::Queued),
        "RUNNING" => Ok(AgentRunStatus::Running),
        "WAITING_APPROVAL" => Ok(AgentRunStatus::WaitingApproval),
        "COMPLETED" => Ok(AgentRunStatus::Completed),
        "FAILED" => Ok(AgentRunStatus::Failed),
        "CANCELLED" => Ok(AgentRunStatus::Cancelled),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

pub fn parse_risk_status(value: String) -> Result<RiskStatus, crate::domain::error::AppError> {
    match value.as_str() {
        "OPEN" => Ok(RiskStatus::Open),
        "MITIGATING" => Ok(RiskStatus::Mitigating),
        "ACCEPTED" => Ok(RiskStatus::Accepted),
        "CLOSED" => Ok(RiskStatus::Closed),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

pub fn parse_risk_severity(value: String) -> Result<RiskSeverity, crate::domain::error::AppError> {
    match value.as_str() {
        "CRITICAL" => Ok(RiskSeverity::Critical),
        "HIGH" => Ok(RiskSeverity::High),
        "MEDIUM" => Ok(RiskSeverity::Medium),
        "LOW" => Ok(RiskSeverity::Low),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

pub fn parse_task_status(value: String) -> Result<TaskStatus, crate::domain::error::AppError> {
    match value.as_str() {
        "OPEN" => Ok(TaskStatus::Open),
        "IN_PROGRESS" => Ok(TaskStatus::InProgress),
        "BLOCKED" => Ok(TaskStatus::Blocked),
        "READY_FOR_REVIEW" => Ok(TaskStatus::ReadyForReview),
        "APPROVED" => Ok(TaskStatus::Approved),
        "REJECTED" => Ok(TaskStatus::Rejected),
        "COMPLETED" => Ok(TaskStatus::Completed),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

// رمز الحالة المخزَّن — المصدر الوحيد لتسمية الحالة في التخزين.
pub fn task_status_code(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Open => "OPEN",
        TaskStatus::InProgress => "IN_PROGRESS",
        TaskStatus::Blocked => "BLOCKED",
        TaskStatus::ReadyForReview => "READY_FOR_REVIEW",
        TaskStatus::Approved => "APPROVED",
        TaskStatus::Rejected => "REJECTED",
        TaskStatus::Completed => "COMPLETED",
    }
}

pub fn parse_task_priority(value: String) -> Result<TaskPriority, crate::domain::error::AppError> {
    match value.as_str() {
        "CRITICAL" => Ok(TaskPriority::Critical),
        "HIGH" => Ok(TaskPriority::High),
        "MEDIUM" => Ok(TaskPriority::Medium),
        "LOW" => Ok(TaskPriority::Low),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

pub fn parse_evidence_classification(
    value: String,
) -> Result<EvidenceClassification, crate::domain::error::AppError> {
    match value.as_str() {
        "PUBLIC" => Ok(EvidenceClassification::Public),
        "INTERNAL" => Ok(EvidenceClassification::Internal),
        "CONFIDENTIAL" => Ok(EvidenceClassification::Confidential),
        "RESTRICTED" => Ok(EvidenceClassification::Restricted),
        "HIGHLY_RESTRICTED" => Ok(EvidenceClassification::HighlyRestricted),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

pub fn parse_evidence_status(
    value: String,
) -> Result<EvidenceStatus, crate::domain::error::AppError> {
    match value.as_str() {
        "MISSING" => Ok(EvidenceStatus::Missing),
        "UPLOADED" => Ok(EvidenceStatus::Uploaded),
        "PROCESSING" => Ok(EvidenceStatus::Processing),
        "ANALYZED" => Ok(EvidenceStatus::Analyzed),
        "VALID" => Ok(EvidenceStatus::Valid),
        "EXPIRED" => Ok(EvidenceStatus::Expired),
        "INVALID" => Ok(EvidenceStatus::Invalid),
        "REQUIRES_REVIEW" => Ok(EvidenceStatus::RequiresReview),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

// الترميز العكسي لحالة الدليل — يُستخدم في التحقق من مدخلات الطفرات قبل أي كتابة.
pub fn evidence_status_code(value: &EvidenceStatus) -> &'static str {
    match value {
        EvidenceStatus::Missing => "MISSING",
        EvidenceStatus::Uploaded => "UPLOADED",
        EvidenceStatus::Processing => "PROCESSING",
        EvidenceStatus::Analyzed => "ANALYZED",
        EvidenceStatus::Valid => "VALID",
        EvidenceStatus::Expired => "EXPIRED",
        EvidenceStatus::Invalid => "INVALID",
        EvidenceStatus::RequiresReview => "REQUIRES_REVIEW",
    }
}

// تحليل حالة الضابط المخزنة — نفس نمط محلّلات النطاق: قيمة غير معروفة تُرفض فشلاً مغلقاً.
pub fn parse_control_status(
    value: String,
) -> Result<ControlStatus, crate::domain::error::AppError> {
    match value.as_str() {
        "ACTIVE" => Ok(ControlStatus::Active),
        "INACTIVE" => Ok(ControlStatus::Inactive),
        "DISABLED" => Ok(ControlStatus::Disabled),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

pub fn parse_control_effectiveness(
    value: String,
) -> Result<ControlEffectiveness, crate::domain::error::AppError> {
    match value.as_str() {
        "NOT_ASSESSED" => Ok(ControlEffectiveness::NotAssessed),
        "EFFECTIVE" => Ok(ControlEffectiveness::Effective),
        "PARTIALLY_EFFECTIVE" => Ok(ControlEffectiveness::PartiallyEffective),
        "INEFFECTIVE" => Ok(ControlEffectiveness::Ineffective),
        _ => Err(crate::domain::error::AppError::DataAccess),
    }
}

// الترميز العكسي لحالة الضابط — يُستخدم في التحقق من مدخلات الطفرات قبل أي كتابة.
pub fn control_status_code(value: &ControlStatus) -> &'static str {
    match value {
        ControlStatus::Active => "ACTIVE",
        ControlStatus::Inactive => "INACTIVE",
        ControlStatus::Disabled => "DISABLED",
    }
}

pub fn control_effectiveness_code(value: &ControlEffectiveness) -> &'static str {
    match value {
        ControlEffectiveness::NotAssessed => "NOT_ASSESSED",
        ControlEffectiveness::Effective => "EFFECTIVE",
        ControlEffectiveness::PartiallyEffective => "PARTIALLY_EFFECTIVE",
        ControlEffectiveness::Ineffective => "INEFFECTIVE",
    }
}

pub fn obligation_status_code(value: &ObligationStatus) -> &'static str {
    match value {
        ObligationStatus::Open => "OPEN",
        ObligationStatus::InProgress => "IN_PROGRESS",
        ObligationStatus::AtRisk => "AT_RISK",
        ObligationStatus::Completed => "COMPLETED",
        ObligationStatus::RequiresReview => "REQUIRES_REVIEW",
    }
}

pub fn obligation_priority_code(value: &ObligationPriority) -> &'static str {
    match value {
        ObligationPriority::Critical => "CRITICAL",
        ObligationPriority::High => "HIGH",
        ObligationPriority::Medium => "MEDIUM",
        ObligationPriority::Low => "LOW",
    }
}

pub fn compliance_status_code(value: &ComplianceStatus) -> &'static str {
    match value {
        ComplianceStatus::Compliant => "COMPLIANT",
        ComplianceStatus::PartiallyCompliant => "PARTIALLY_COMPLIANT",
        ComplianceStatus::NonCompliant => "NON_COMPLIANT",
        ComplianceStatus::NotAssessed => "NOT_ASSESSED",
        ComplianceStatus::RequiresReview => "REQUIRES_REVIEW",
    }
}

pub fn review_status_code(value: &ReviewStatus) -> &'static str {
    match value {
        ReviewStatus::NotReviewed => "NOT_REVIEWED",
        ReviewStatus::InReview => "IN_REVIEW",
        ReviewStatus::Reviewed => "REVIEWED",
        ReviewStatus::Rejected => "REJECTED",
    }
}

#[cfg(test)]
mod obligation_contract_tests {
    use super::{
        ComplianceStatus, DataMode, ObligationPriority, ObligationStatus, ReviewStatus,
        RiskSeverity, RiskStatus, SourceType,
    };

    #[test]
    fn obligation_enum_serialization_matches_shared_contract_values() {
        assert_eq!(
            serde_json::to_value(ObligationStatus::Open).expect("تسلسل الحالة يجب أن ينجح"),
            "OPEN"
        );
        assert_eq!(
            serde_json::to_value(ObligationPriority::Critical).expect("تسلسل الأولوية يجب أن ينجح"),
            "CRITICAL"
        );
        assert_eq!(
            serde_json::to_value(ComplianceStatus::PartiallyCompliant)
                .expect("تسلسل الامتثال يجب أن ينجح"),
            "PARTIALLY_COMPLIANT"
        );
        assert_eq!(
            serde_json::to_value(ReviewStatus::InReview).expect("تسلسل المراجعة يجب أن ينجح"),
            "IN_REVIEW"
        );
        assert_eq!(
            serde_json::to_value(SourceType::Synthetic).expect("تسلسل نوع المصدر يجب أن ينجح"),
            "SYNTHETIC"
        );
        assert_eq!(
            serde_json::to_value(DataMode::DemoSynthetic).expect("تسلسل وضع البيانات يجب أن ينجح"),
            "DEMO_SYNTHETIC"
        );
        assert_eq!(
            serde_json::to_value(RiskStatus::Mitigating).expect("تسلسل حالة المخاطر يجب أن ينجح"),
            "MITIGATING"
        );
        assert_eq!(
            serde_json::to_value(RiskSeverity::Critical).expect("تسلسل شدة المخاطر يجب أن ينجح"),
            "CRITICAL"
        );
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrganizationalIntelligencePriorityKind {
    Obligation,
    Task,
    Risk,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrganizationalIntelligencePriorityState {
    AtRisk,
    Missing,
    RequiresReview,
    Expired,
    Invalid,
    Blocked,
    Open,
    Mitigating,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrganizationalIntelligencePriorityReason {
    ObligationAtRisk,
    EvidenceGap,
    TaskBlocked,
    RiskActive,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrganizationalIntelligenceSeverity {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrganizationalIntelligenceDataMode {
    DemoSynthetic,
    Production,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationalIntelligenceOrganization {
    pub legal_name: String,
    pub country_code: String,
    pub industry: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationalIntelligenceSourceSummary {
    pub total: u32,
    pub human_verified: u32,
    pub requires_review: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationalIntelligenceOperationalSummary {
    pub active_obligations: u32,
    pub at_risk_obligations: u32,
    pub overdue_obligations: u32,
    pub evidence_gaps: u32,
    pub active_tasks: u32,
    pub blocked_tasks: u32,
    pub active_risks: u32,
    pub pending_approvals: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationalIntelligencePriority {
    pub id: String,
    pub kind: OrganizationalIntelligencePriorityKind,
    pub title: String,
    pub severity: OrganizationalIntelligenceSeverity,
    pub state: OrganizationalIntelligencePriorityState,
    pub reason: OrganizationalIntelligencePriorityReason,
    pub owner: Option<String>,
    pub due_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationalIntelligenceData {
    pub organization: Option<OrganizationalIntelligenceOrganization>,
    pub sources: OrganizationalIntelligenceSourceSummary,
    pub operations: OrganizationalIntelligenceOperationalSummary,
    pub priorities: Vec<OrganizationalIntelligencePriority>,
    pub data_mode: OrganizationalIntelligenceDataMode,
    pub generated_at: String,
    pub has_data: bool,
}

// ===== الوحدة 04 (الذكاء التنظيمي): سطح قراءة المصادر التنظيمية المسجلة =====
// الدلالات من مخطط regulatory_sources القائم — لا اختراع تغذية تنظيمية أو مراقبة خارجية.

// تصنيف منشأ المصادر كما يستنتج فعلياً من بيانات المخطط القائم
// (وضع بيانات المستأجر + عمود official) — بلا قيم لا يمكن اشتقاقها من مصدر حقيقي.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RegulatorySourceProvenance {
    Official,
    DemoSynthetic,
    // مصدر سجّله المستخدم بنفسه ولم يُتحقق منه: لا يُعرض رسمياً ولا يُسمى كذلك.
    UserProvidedUnverified,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegulatorySourceRecord {
    pub id: String,
    pub authority: String,
    pub document_title: String,
    pub document_version: Option<String>,
    pub source_url: Option<String>,
    pub retrieved_at: String,
    pub official: bool,
    pub verification_state: String,
    pub active_obligation_count: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegulatorySourceSummary {
    pub total: u32,
    pub official: u32,
    pub human_verified: u32,
    pub requires_review: u32,
    pub linked_active_obligations: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RegulatorySourcesData {
    pub items: Vec<RegulatorySourceRecord>,
    pub summary: RegulatorySourceSummary,
    pub provenance: RegulatorySourceProvenance,
    pub data_mode: DataMode,
    pub generated_at: String,
    pub has_data: bool,
}

// نتيجة تسجيل مصدر تنظيمي من المستخدم: المنشأ صريح في الاستجابة نفسها.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CreatedRegulatorySource {
    pub id: String,
    pub authority: String,
    pub document_title: String,
    pub official: bool,
    pub verification_state: String,
    pub provenance: RegulatorySourceProvenance,
    pub created_at: String,
}

// ===== نتيجة التحقق الفعلي من سلسلة التدقيق (0026) =====
// تُعلن في `application::audit_service` كعقد عرض مستقل لأنها إعادة تشغيل
// حقيقية لسلسلة SHA-256 (لا قيمة محفوظة)، فلا تُخزَّن في طبقة النموذج.

// ===== الوحدة 12 (التدقيق): قارئ أحداث قراءة فقط بترقيم مفتاحي محدود =====

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditTrailItem {
    pub id: String,
    pub event_type: String,
    pub actor_name: String,
    pub resource_type: String,
    pub resource_id: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditTrailCursor {
    pub occurred_at: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AuditTrailData {
    pub items: Vec<AuditTrailItem>,
    pub chain_integrity: String,
    /// حالة مرساة الذيل: MATCH أو NO_REFERENCE أو TRUNCATED أو PREFIX_MISMATCH.
    /// مشتقة من مقارنة السلسلة الفعلية بالمرجع المحفوظ عند آخر تحقق ناجح.
    pub chain_anchor_state: String,
    pub has_more: bool,
    pub next_cursor: Option<AuditTrailCursor>,
    pub generated_at: String,
}

// ===== نتائج تنفيذ الوكلاء (قراءة آمنة عبر وحدة الوكلاء) =====

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunResultData {
    pub run_id: String,
    pub run_status: AgentRunStatus,
    pub outcome: String,
    pub provider_id: String,
    pub model_id: Option<String>,
    pub failure_code: Option<String>,
    pub output: Option<String>,
    pub provider_generated_at: Option<String>,
    pub created_at: String,
    // MP04: الاقتباسات المتحققة (JSON مطبوع) وtruth_class — يبقى AI_INTERPRETATION
    // افتراضاً ولا يرقى النظام به تلقائياً (C-6/C-8).
    pub citations_json: Option<String>,
    pub truth_class: Option<String>,
}

// ===== الوحدة 14 (التكاملات): حالة الحقيقة للأسس المدعومة فقط =====

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegrationProviderState {
    Connected,
    Unavailable,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationProviderStatus {
    pub provider_id: String,
    pub provider_kind: String,
    pub state: IntegrationProviderState,
    pub discovered_model_ids: Vec<String>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatusData {
    pub providers: Vec<IntegrationProviderStatus>,
    pub enabled_local_models: Vec<String>,
    pub model_registry_configuration_available: bool,
    pub checked_at: String,
}

// ===== سياق الجلسة للعرض فقط: فاعل نشط + صلاحياته الفعلية من الخلف =====

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext {
    pub actor_id: String,
    pub display_name: String,
    pub responsibility: String,
    pub permissions: Vec<String>,
}

// وضع التشغيل الصريح للبيئة — مصدره الخلف وحده (app_runtime_state) ولا يتلقاه أي IPC.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeMode {
    Unprovisioned,
    Demo,
    Production,
}

impl RuntimeMode {
    /// قراءة الوضع من نص قاعدة البيانات — أي قيمة مجهولة تُغلق إلى UNPROVISIONED.
    pub fn from_database(value: &str) -> Self {
        match value {
            "DEMO" => Self::Demo,
            "PRODUCTION" => Self::Production,
            _ => Self::Unprovisioned,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unprovisioned => "UNPROVISIONED",
            Self::Demo => "DEMO",
            Self::Production => "PRODUCTION",
        }
    }
}
