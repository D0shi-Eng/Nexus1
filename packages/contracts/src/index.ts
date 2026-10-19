export const dataClassifications = [
  'PUBLIC',
  'INTERNAL',
  'CONFIDENTIAL',
  'RESTRICTED',
  'HIGHLY_RESTRICTED',
] as const;

export type DataClassification = (typeof dataClassifications)[number];

export const obligationStatuses = [
  'OPEN',
  'IN_PROGRESS',
  'AT_RISK',
  'COMPLETED',
  'REQUIRES_REVIEW',
] as const;

export type ObligationStatus = (typeof obligationStatuses)[number];

export const severityLevels = ['CRITICAL', 'HIGH', 'MEDIUM', 'LOW'] as const;

export type SeverityLevel = (typeof severityLevels)[number];

export const riskStatuses = ['OPEN', 'MITIGATING', 'ACCEPTED', 'CLOSED'] as const;
export type RiskStatus = (typeof riskStatuses)[number];
export type RiskSeverity = SeverityLevel;

export interface RiskRecord {
  readonly id: string;
  readonly title: string;
  readonly severity: RiskSeverity;
  readonly likelihood: number;
  readonly impact: number;
  readonly status: RiskStatus;
  readonly owner: string;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface RiskSummary {
  readonly total: number;
  readonly open: number;
  readonly mitigating: number;
  readonly accepted: number;
  readonly closed: number;
}

export interface RisksData {
  readonly items: readonly RiskRecord[];
  readonly summary: RiskSummary;
  readonly dataMode: DataMode;
  readonly generatedAt: string;
  readonly hasData: boolean;
}

// 0025: دورة حياة المهمة المحكومة. الحالتان APPROVED وREJECTED نتاج قرار
// اعتماد ولا تُبلَغ من الواجهة مباشرة — الخلف يرفض أي محاولة تجاوز لمسار الموافقات.
export const taskStatuses = [
  'OPEN',
  'IN_PROGRESS',
  'BLOCKED',
  'READY_FOR_REVIEW',
  'APPROVED',
  'REJECTED',
  'COMPLETED',
] as const;
export type TaskStatus = (typeof taskStatuses)[number];

export const taskPriorities = ['CRITICAL', 'HIGH', 'MEDIUM', 'LOW'] as const;
export type TaskPriority = (typeof taskPriorities)[number];

export interface TaskRecord {
  readonly id: string;
  readonly obligationId: string;
  readonly obligationTitle: string;
  readonly title: string;
  readonly description: string;
  readonly dueAt: string | null;
  readonly priority: TaskPriority;
  readonly status: TaskStatus;
  /** الإسناد الحقيقي؛ غيابه يعني مهمة بلا مالك معلن. */
  readonly assigneeUserId: string | null;
  readonly assigneeDisplayName: string | null;
  readonly version: number;
  readonly completedAt: string | null;
  /** الإجراءات المسموحة لهذا العارض كما يحسبها الخلف — لا تُخمَّن في العميل. */
  readonly availableTransitions: readonly TaskStatus[];
  /** هل يملك العارض إسناد هذه المهمة (إدارة أو التقاط ذاتي لمهمة بلا مالك)؟ */
  readonly canAssign: boolean;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface TaskSummary {
  readonly total: number;
  readonly assignedToMe: number;
  readonly open: number;
  readonly inProgress: number;
  readonly blocked: number;
  readonly readyForReview: number;
  readonly approved: number;
  readonly rejected: number;
  readonly completed: number;
  readonly overdue: number;
}

export interface TasksData {
  readonly items: readonly TaskRecord[];
  readonly summary: TaskSummary;
  readonly dataMode: DataMode;
  readonly generatedAt: string;
  readonly hasData: boolean;
}

export interface AssignTaskInput {
  readonly taskId: string;
  readonly assigneeUserId: string;
  readonly dueAt: string | null;
  readonly expectedVersion: number;
}

export interface TransitionTaskInput {
  readonly taskId: string;
  readonly targetStatus: TaskStatus;
  readonly reason: string | null;
  /** الإصدار الذي رآه المستخدم — تغيّر المهمة يعني رفضاً لا كتابة عمياء. */
  readonly expectedVersion: number;
}

export interface TaskTransitionResult {
  readonly taskId: string;
  readonly previousStatus: TaskStatus;
  readonly status: TaskStatus;
  readonly version: number;
  readonly pendingApprovalId: string | null;
}

export interface TaskTransitionRecord {
  readonly fromStatus: TaskStatus;
  readonly toStatus: TaskStatus;
  readonly actorDisplayName: string;
  readonly reason: string | null;
  readonly occurredAt: string;
}

// توسعة إضافية غير كاسرة (0022): الحقول الجديدة اختيارية حتى لا تُبطل أي عقد قائم،
// والخلف يرسلها دائماً بعد الترحيل.
export interface ControlRecord {
  readonly id: string;
  readonly title: string;
  readonly createdAt: string;
  readonly updatedAt: string;
  readonly status?: ControlStatus;
  readonly category?: string | null;
  readonly owner?: string | null;
  readonly effectiveness?: ControlEffectiveness;
  readonly lastAssessedAt?: string | null;
  readonly linkedObligationCount?: number;
  readonly linkedRiskCount?: number;
  readonly linkedEvidenceCount?: number;
}

export const controlStatuses = ['ACTIVE', 'INACTIVE', 'DISABLED'] as const;

export type ControlStatus = (typeof controlStatuses)[number];

export const controlEffectivenessValues = [
  'NOT_ASSESSED',
  'EFFECTIVE',
  'PARTIALLY_EFFECTIVE',
  'INEFFECTIVE',
] as const;

export type ControlEffectiveness = (typeof controlEffectivenessValues)[number];

export interface ControlSummary {
  readonly total: number;
  readonly active?: number;
  readonly inactive?: number;
  readonly disabled?: number;
}

export interface ControlsData {
  readonly items: readonly ControlRecord[];
  readonly summary: ControlSummary;
  readonly dataMode: DataMode;
  readonly generatedAt: string;
  readonly hasData: boolean;
}

export interface EvidenceRecord {
  readonly id: string;
  readonly obligationId: string;
  readonly obligationTitle: string;
  readonly label: string;
  readonly classification: DataClassification;
  readonly status: EvidenceStatus;
  readonly expiresAt: string | null;
  /** الحالة الفعلية المعروضة: تساوي status إلا إذا انقضى الأجل فتصبح EXPIRED. */
  readonly effectiveStatus: EvidenceStatus;
  /** هل الدليل منتهي الصلاحية في اللحظة الحالية؟ */
  readonly isExpired: boolean;
  /** هوية من قدّم الدليل؛ null يعني سجلاً تاريخياً سابقاً لعمود registered_by. */
  readonly registeredBy: string | null;
  /** تصنيف صريح للمنشأ: RECORDED أو UNKNOWN_LEGACY. */
  readonly registeredByState: 'RECORDED' | 'UNKNOWN_LEGACY';
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface EvidenceSummary {
  readonly total: number;
  readonly requiresReview: number;
  readonly expired: number;
}

export interface EvidenceData {
  readonly items: readonly EvidenceRecord[];
  readonly summary: EvidenceSummary;
  readonly dataMode: DataMode;
  readonly generatedAt: string;
  readonly hasData: boolean;
}

export const verificationStates = [
  'UNVERIFIED',
  'AI_ANALYZED',
  'HUMAN_VERIFIED',
  'REQUIRES_REVIEW',
] as const;

export type VerificationState = (typeof verificationStates)[number];

export const evidenceStatuses = [
  'MISSING',
  'UPLOADED',
  'PROCESSING',
  'ANALYZED',
  'VALID',
  'EXPIRED',
  'INVALID',
  'REQUIRES_REVIEW',
] as const;

export type EvidenceStatus = (typeof evidenceStatuses)[number];

export const applicabilityStates = [
  'APPLIES',
  'DOES_NOT_APPLY',
  'UNCERTAIN',
  'REQUIRES_REVIEW',
] as const;

export type ApplicabilityState = (typeof applicabilityStates)[number];

export interface SourceReference {
  readonly authority: string;
  readonly documentTitle: string;
  readonly sourceUrl?: string;
  readonly version?: string;
  readonly section?: string;
  readonly retrievedAt: string;
  readonly official: boolean;
}

export interface TruthClaim {
  readonly claimType: 'FACT' | 'AI_INTERPRETATION' | 'COMPANY_DATA' | 'EVIDENCE' | 'HUMAN_VERIFIED' | 'UNKNOWN';
  readonly confidence: number;
  readonly verificationState: VerificationState;
  readonly source?: SourceReference;
  readonly createdAt: string;
}

export interface ObligationSummary {
  readonly id: string;
  readonly tenantId: string;
  readonly title: string;
  readonly authority: string;
  readonly owner: string;
  readonly deadline: string | null;
  readonly severity: SeverityLevel;
  readonly status: ObligationStatus;
  readonly applicability: ApplicabilityState;
  readonly evidenceStatus: EvidenceStatus;
  readonly verificationState: VerificationState;
  readonly source: SourceReference;
  readonly updatedAt: string;
}

export const obligationPriorities = ['CRITICAL', 'HIGH', 'MEDIUM', 'LOW'] as const;
export type ObligationPriority = (typeof obligationPriorities)[number];

export const complianceStatuses = ['COMPLIANT', 'PARTIALLY_COMPLIANT', 'NON_COMPLIANT', 'NOT_ASSESSED', 'REQUIRES_REVIEW'] as const;
export type ComplianceStatus = (typeof complianceStatuses)[number];

export const reviewStatuses = ['NOT_REVIEWED', 'IN_REVIEW', 'REVIEWED', 'REJECTED'] as const;
export type ReviewStatus = (typeof reviewStatuses)[number];

export const sourceTypes = ['OFFICIAL', 'INTERNAL', 'DEMO', 'SYNTHETIC', 'USER_PROVIDED', 'UNKNOWN'] as const;
export type SourceType = (typeof sourceTypes)[number];

export const dataModes = ['DEMO_SYNTHETIC', 'PRODUCTION'] as const;
export type DataMode = (typeof dataModes)[number];

export interface ObligationListItem {
  readonly id: string;
  readonly tenantId: string;
  readonly title: string;
  readonly description: string;
  readonly status: ObligationStatus;
  readonly priority: ObligationPriority;
  readonly sourceId: string;
  readonly sourceReference: string;
  readonly sourceType: SourceType;
  readonly source: SourceReference;
  readonly owner: string;
  readonly dueDate: string | null;
  readonly createdAt: string;
  readonly updatedAt: string;
  readonly reviewStatus: ReviewStatus;
  readonly complianceStatus: ComplianceStatus;
  readonly dataMode: DataMode;
  readonly isArchived: boolean;
}

export interface ObligationEvidenceReference {
  readonly id: string;
  readonly label: string;
  readonly classification: DataClassification;
  readonly status: EvidenceStatus;
  readonly expiresAt: string | null;
  readonly createdAt: string;
}

export interface ObligationTaskReference {
  readonly id: string;
  readonly title: string;
  readonly status: 'OPEN' | 'IN_PROGRESS' | 'BLOCKED' | 'COMPLETED';
  readonly priority: ObligationPriority;
  readonly dueAt: string | null;
  readonly createdAt: string;
}

export interface ObligationRiskReference {
  readonly id: string;
  readonly title: string;
  readonly status: 'OPEN' | 'MITIGATING' | 'ACCEPTED' | 'CLOSED';
  readonly severity: SeverityLevel;
  readonly owner: string;
  readonly createdAt: string;
}

export interface ObligationApprovalReference {
  readonly id: string;
  readonly resourceType: string;
  readonly decision: 'PENDING' | 'APPROVED' | 'REJECTED' | 'CANCELLED';
  readonly requiredRoleCode: string;
  readonly createdAt: string;
  readonly decidedAt: string | null;
}

export interface ObligationRelatedResources {
  readonly evidence: readonly ObligationEvidenceReference[];
  readonly tasks: readonly ObligationTaskReference[];
  readonly risks: readonly ObligationRiskReference[];
  readonly approvals: readonly ObligationApprovalReference[];
}

export interface ObligationDetails extends ObligationListItem {
  readonly relatedResources: ObligationRelatedResources;
  readonly version: number;
}

export interface ObligationSummaryMetrics {
  readonly total: number;
  readonly critical: number;
  readonly dueSoon: number;
  readonly overdue: number;
  readonly nonCompliant: number;
  readonly underReview: number;
}

export interface ObligationsData {
  readonly items: readonly ObligationListItem[];
  readonly summary: ObligationSummaryMetrics;
  readonly dataMode: DataMode;
  readonly generatedAt: string;
}

export interface UpdatedObligation {
  readonly id: string;
  readonly updatedAt: string;
  readonly version: number;
}

export interface ArchivedObligation {
  readonly id: string;
  readonly archivedAt: string;
}

// 0025: خطوة التحقق البشري — فعل مراجعة مستقل عن تحديث الالتزام.
export interface VerifyObligationInput {
  readonly obligationId: string;
  readonly expectedVersion: number;
}

export interface VerifiedObligation {
  readonly id: string;
  readonly verificationState: string;
  readonly verifiedAt: string;
  readonly version: number;
}

export const attentionItemKinds = ['OBLIGATION', 'EVIDENCE', 'RISK', 'TASK', 'APPROVAL'] as const;
export type AttentionItemKind = (typeof attentionItemKinds)[number];

export const attentionItemStatuses = [
  'OPEN',
  'IN_PROGRESS',
  'AT_RISK',
  'REQUIRES_REVIEW',
  'MISSING',
  'EXPIRED',
  'INVALID',
  'BLOCKED',
  'MITIGATING',
  'PENDING',
] as const;
export type AttentionItemStatus = (typeof attentionItemStatuses)[number];

export const attentionReasonCodes = [
  'OBLIGATION_AT_RISK',
  'OBLIGATION_REQUIRES_REVIEW',
  'EVIDENCE_MISSING',
  'EVIDENCE_REQUIRES_REVIEW',
  'EVIDENCE_EXPIRED',
  'EVIDENCE_INVALID',
  'TASK_OPEN',
  'TASK_IN_PROGRESS',
  'TASK_BLOCKED',
  'RISK_OPEN',
  'RISK_MITIGATING',
  'APPROVAL_PENDING',
] as const;
export type AttentionReasonCode = (typeof attentionReasonCodes)[number];

export const attentionActions = ['OPEN', 'REVIEW', 'CREATE_TASK', 'REGISTER_EVIDENCE', 'DECIDE_APPROVAL'] as const;
export type AttentionAction = (typeof attentionActions)[number];

export interface AttentionRelatedEntity {
  readonly kind: AttentionItemKind;
  readonly id: string;
  readonly title: string;
}

export interface AttentionItem {
  readonly id: string;
  readonly kind: AttentionItemKind;
  readonly severity: SeverityLevel;
  readonly title: string;
  readonly status: AttentionItemStatus;
  readonly reasonCode: AttentionReasonCode;
  readonly source: 'LOCAL_WORKSPACE';
  readonly dueAt: string | null;
  readonly resourceId: string;
  readonly relatedEntity: AttentionRelatedEntity | null;
  readonly owner: string | null;
  readonly updatedAt: string;
  readonly availableActions: readonly AttentionAction[];
}

export interface AttentionSummary {
  readonly total: number;
  readonly critical: number;
  readonly high: number;
  readonly medium: number;
  readonly low: number;
  readonly pending: number;
  readonly overdue: number;
  readonly requiresReview: number;
}

export interface AttentionData {
  readonly items: readonly AttentionItem[];
  readonly summary: AttentionSummary;
  readonly dataMode: 'DEMO_SYNTHETIC' | 'PRODUCTION';
  readonly generatedAt: string;
}

// كل نسبة `null` تعني «لا توجد بيانات كافية» — لا صفراً. عرض 0% عند غياب
// البيانات ادعاء كاذب بأن القياس جرى وفشل.
export interface ReadinessSnapshot {
  readonly tenantId: string;
  readonly readinessScore: number | null;
  readonly verifiedCoverage: number | null;
  readonly evidenceCompleteness: number | null;
  readonly openCriticalItems: number;
  /** مقام النسبة المعلن: الالتزامات التي ينطبق عليها الالتزام فعلاً. */
  readonly eligibleObligations: number;
  /** الالتزامات المؤرشفة المستبعدة من مؤشرات الحالة النشطة (سياسة عد معلنة). */
  readonly archivedObligations: number;
  readonly sufficientData: boolean;
  readonly generatedAt: string;
  readonly disclaimer: string;
}

export interface AuditEventSummary {
  readonly id: string;
  readonly eventType: string;
  readonly actorName: string;
  readonly resourceType: string;
  readonly resourceId: string;
  readonly occurredAt: string;
  /** محسوبة من التحقق الفعلي لسلسلة التجزئة لكل حدث. */
  readonly integrityStatus: 'VALID' | 'UNKNOWN' | 'INVALID';
}

export interface DashboardData {
  readonly attentionItems: readonly AttentionItem[];
  readonly obligations: readonly ObligationSummary[];
  readonly readiness: ReadinessSnapshot;
  readonly recentAuditEvents: readonly AuditEventSummary[];
  /** سلامة سلسلة التدقيق للمستأجر كاملاً — محسوبة لا ممنوحة. */
  readonly auditChainIntegrity: 'VALID' | 'INVALID';
  /** وضع بيانات المستأجر الفعلي — تُبنى عليه ملاحظات المنشأ في الواجهة. */
  readonly dataMode: DataMode;
}

export interface HealthStatus {
  readonly application: 'HEALTHY' | 'DEGRADED' | 'UNAVAILABLE';
  readonly database: 'HEALTHY' | 'DEGRADED' | 'UNAVAILABLE';
  readonly ai: 'NOT_CONFIGURED' | 'HEALTHY' | 'DEGRADED' | 'UNAVAILABLE';
  readonly checkedAt: string;
  // حقول رصد MP05 — يرسلها الخلف دائماً؛ أُبقيت اختيارية حتى لا تُلزم أي حامل اختبار قائم.
  readonly appVersion?: string;
  readonly schemaVersion?: number;
  readonly runtimeMode?: 'UNPROVISIONED' | 'DEMO' | 'PRODUCTION';
  readonly configuredModelId?: string | null;
  readonly recentFailures?: {
    readonly agentRunsFailed24h: number;
    readonly providerErrors24h: number;
  };
}

// نتيجة فحص المزود الحي الصريح — إجراء مستخدم بمهلة قصيرة (2 ثانية)، لا استعلام دوري.
export interface AiProviderCheck {
  readonly reachable: boolean;
  readonly checkedAt: string;
}

export const agentStatuses = ['ACTIVE', 'DISABLED'] as const;
export type AgentStatus = (typeof agentStatuses)[number];

export interface AgentRecord {
  readonly id: string;
  readonly tenantId: string;
  readonly name: string;
  readonly description: string;
  readonly status: AgentStatus;
  readonly capability: string;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface AgentsData {
  readonly hasData: boolean;
  readonly items: readonly AgentRecord[];
}

export interface CreateAgentInput {
  readonly name: string;
  readonly description: string;
  readonly capability?: string | null;
}

export interface UpdateAgentInput extends CreateAgentInput {
  readonly agentId: string;
}

export interface SetAgentStatusInput {
  readonly agentId: string;
  readonly status: AgentStatus;
}

export interface AgentMutationResult {
  readonly agent: AgentRecord;
}

export const agentRunStatuses = ['QUEUED', 'RUNNING', 'WAITING_APPROVAL', 'COMPLETED', 'FAILED', 'CANCELLED'] as const;
export type AgentRunStatus = (typeof agentRunStatuses)[number];

export interface AgentRunRecord {
  readonly id: string;
  readonly tenantId: string;
  readonly agentId: string;
  readonly requestedByUserId: string;
  readonly status: AgentRunStatus;
  readonly requestSummary: 'GOVERNED_EXECUTION_REQUEST';
  readonly revision: number;
  readonly startedAt: string | null;
  readonly completedAt: string | null;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface AgentRunsData {
  readonly hasData: boolean;
  readonly items: readonly AgentRunRecord[];
}

export interface CreateAgentRunInput {
  readonly agentId: string;
}

export interface CancelAgentRunInput {
  readonly runId: string;
}

export interface AgentRunMutationResult {
  readonly run: AgentRunRecord;
}

export const demoActorResponsibilities = ['REQUESTER', 'REVIEWER'] as const;
export type DemoActorResponsibility = (typeof demoActorResponsibilities)[number];

export interface DemoActor {
  readonly actorId: string;
  readonly displayName: string;
  readonly responsibility: DemoActorResponsibility;
}

export interface DemoActorsData {
  readonly actors: readonly DemoActor[];
  readonly activeActorId: string;
}

// رموز SafeError المرئية للواجهة — يجب أن تبقى متطابقة مع كتالوج SafeError في نواة Rust
// وأن يكون لكل رمز ترجمة عربية وإنجليزية في موارد اللغتين.
export const safeErrorCodes = [
  'STORAGE_ERROR',
  'LOCAL_SERVICE_UNAVAILABLE',
  'AUTHORIZATION_DENIED',
  'VALIDATION_FAILED',
  'RESOURCE_NOT_FOUND',
  'CONFLICT',
  'INVALID_STATE_TRANSITION',
  'AGENT_DISABLED',
  'SELF_APPROVAL_FORBIDDEN',
  'STALE_APPROVAL',
  'PROVIDER_UNAVAILABLE',
  'PROVIDER_TIMEOUT',
  'PROVIDER_RESPONSE_INVALID',
  'MODEL_UNAVAILABLE',
  'EXECUTION_NOT_CONFIGURED',
  'KNOWLEDGE_SOURCE_INVALID',
  'KNOWLEDGE_SOURCE_TOO_LARGE',
  'KNOWLEDGE_DUPLICATE',
  'KNOWLEDGE_EXTRACTION_FAILED',
  'KNOWLEDGE_RETRIEVAL_FAILED',
  'MEMORY_WRITE_DENIED',
  'MEMORY_SCOPE_INVALID',
  'PLAN_INVALID',
  'PLAN_TOO_LARGE',
  'PLAN_CAPABILITY_DENIED',
  'DELEGATION_DENIED',
  'DELEGATION_LIMIT_EXCEEDED',
  'CITATION_INVALID',
  'VERIFICATION_INSUFFICIENT_EVIDENCE',
  'EXECUTION_INFRASTRUCTURE_ERROR',
  'TOOL_AUTHORIZATION_DENIED',
  'TOOL_INPUT_INVALID',
  'TOOL_CALL_LIMIT_EXCEEDED',
  'TOOL_EXECUTION_FAILED',
  'TOOL_RESULT_TOO_LARGE',
  'AUTH_INVALID_CREDENTIALS',
  'AUTH_LOCKED',
  'BOOTSTRAP_UNAVAILABLE',
  'BOOTSTRAP_INVALID',
  // GAP-05-OPEN: رفض الدولة أو القطاع المُعلن له رمز مستقل قابل للترجمة.
  'BOOTSTRAP_COUNTRY_INVALID',
  'BOOTSTRAP_INDUSTRY_INVALID',
  'SELF_MODIFICATION_FORBIDDEN',
  'BACKUP_INVALID',
  'BACKUP_CORRUPTED',
  'BACKUP_INCOMPATIBLE',
  'BACKUP_STORAGE_ERROR',
  // 0025: رمزا رفض دورة حياة المهمة — لهما رسالة مستقلة حتى يفهم الموظف السبب.
  'TASK_EVIDENCE_REQUIRED',
  'TASK_ASSIGNMENT_INVALID',
  // 0025: التحقق البشري بلا دليل مسجَّل مرفوض برمز صريح.
  'OBLIGATION_EVIDENCE_REQUIRED',
  // 0026: رموز إدارة المستخدمين وكلمة المرور وفصل الأدلة وتأليف الأعمال.
  'USER_EMAIL_EXISTS',
  'USER_ROLE_NOT_GRANTABLE',
  'USER_ROLE_UNKNOWN',
  'CURRENT_PASSWORD_INVALID',
  'PASSWORD_POLICY_VIOLATION',
  'EVIDENCE_SELF_VERIFICATION_FORBIDDEN',
  'REGULATORY_SOURCE_INVALID',
  'REGULATORY_SOURCE_DUPLICATE',
  'OBLIGATION_SOURCE_INVALID',
  // جولة الإغلاق النهائية: حماية آخر مسؤول فعال وانتهاء صلاحية الدليل.
  'LAST_ADMINISTRATOR_PROTECTED',
  'EVIDENCE_EXPIRED',
] as const;
export type SafeErrorCode = (typeof safeErrorCodes)[number];

export const nexusModuleKeys = [
  'overview', 'attention', 'obligations', 'regulatory', 'risks', 'evidence', 'tasks', 'controls',
  'agents', 'knowledge', 'reports', 'audit', 'organization', 'integrations', 'settings',
] as const;
export type NexusModuleKey = (typeof nexusModuleKeys)[number];

export const moduleAvailabilityStatuses = ['AVAILABLE', 'READ_ONLY', 'NOT_IMPLEMENTED'] as const;
export type ModuleAvailabilityStatus = (typeof moduleAvailabilityStatuses)[number];

export interface ModuleAvailability {
  readonly key: NexusModuleKey;
  readonly status: ModuleAvailabilityStatus;
  readonly messageKey: string;
}

export const organizationalIntelligencePriorityKinds = ['OBLIGATION', 'TASK', 'RISK'] as const;
export type OrganizationalIntelligencePriorityKind = (typeof organizationalIntelligencePriorityKinds)[number];

export const organizationalIntelligencePriorityStates = [
  'AT_RISK',
  'MISSING',
  'REQUIRES_REVIEW',
  'EXPIRED',
  'INVALID',
  'BLOCKED',
  'OPEN',
  'MITIGATING',
] as const;
export type OrganizationalIntelligencePriorityState = (typeof organizationalIntelligencePriorityStates)[number];

export const organizationalIntelligencePriorityReasons = [
  'OBLIGATION_AT_RISK',
  'EVIDENCE_GAP',
  'TASK_BLOCKED',
  'RISK_ACTIVE',
] as const;
export type OrganizationalIntelligencePriorityReason = (typeof organizationalIntelligencePriorityReasons)[number];

export interface OrganizationalIntelligenceOrganization {
  readonly legalName: string;
  readonly countryCode: string;
  readonly industry: string;
}

export interface OrganizationalIntelligenceSourceSummary {
  readonly total: number;
  readonly humanVerified: number;
  readonly requiresReview: number;
}

export interface OrganizationalIntelligenceOperationalSummary {
  readonly activeObligations: number;
  readonly atRiskObligations: number;
  readonly overdueObligations: number;
  readonly evidenceGaps: number;
  readonly activeTasks: number;
  readonly blockedTasks: number;
  readonly activeRisks: number;
  readonly pendingApprovals: number;
}

export interface OrganizationalIntelligencePriority {
  readonly id: string;
  readonly kind: OrganizationalIntelligencePriorityKind;
  readonly title: string;
  readonly severity: SeverityLevel;
  readonly state: OrganizationalIntelligencePriorityState;
  readonly reason: OrganizationalIntelligencePriorityReason;
  readonly owner: string | null;
  readonly dueAt: string | null;
  readonly updatedAt: string;
}

export interface OrganizationalIntelligenceData {
  readonly organization: OrganizationalIntelligenceOrganization | null;
  readonly sources: OrganizationalIntelligenceSourceSummary;
  readonly operations: OrganizationalIntelligenceOperationalSummary;
  readonly priorities: readonly OrganizationalIntelligencePriority[];
  readonly dataMode: DataMode;
  readonly generatedAt: string;
  readonly hasData: boolean;
}

// ===== الوحدة 04: المصادر التنظيمية المسجلة (قراءة فقط) =====

export const regulatorySourceProvenances = ['OFFICIAL', 'DEMO_SYNTHETIC', 'USER_PROVIDED_UNVERIFIED', 'UNKNOWN'] as const;
export type RegulatorySourceProvenance = (typeof regulatorySourceProvenances)[number];

export interface RegulatorySourceRecord {
  readonly id: string;
  readonly authority: string;
  readonly documentTitle: string;
  readonly documentVersion: string | null;
  readonly sourceUrl: string | null;
  readonly retrievedAt: string;
  readonly official: boolean;
  readonly verificationState: VerificationState;
  readonly activeObligationCount: number;
}

export interface RegulatorySourceSummary {
  readonly total: number;
  readonly official: number;
  readonly humanVerified: number;
  readonly requiresReview: number;
  readonly linkedActiveObligations: number;
}

export interface RegulatorySourcesData {
  readonly items: readonly RegulatorySourceRecord[];
  readonly summary: RegulatorySourceSummary;
  readonly provenance: RegulatorySourceProvenance;
  readonly dataMode: DataMode;
  readonly generatedAt: string;
  readonly hasData: boolean;
}

// 0026 — تسجيل مصدر تنظيمي يقدّمه المستخدم: لا جلب ولا تحقق تلقائي.
export interface CreateRegulatorySourceInput {
  readonly authority: string;
  readonly documentTitle: string;
  readonly documentVersion: string | null;
  readonly sourceUrl: string | null;
}

export interface CreatedRegulatorySource {
  readonly id: string;
  readonly authority: string;
  readonly documentTitle: string;
  readonly official: boolean;
  readonly verificationState: VerificationState;
  readonly provenance: RegulatorySourceProvenance;
  readonly createdAt: string;
}

export interface RegulatorySourceMutationResult {
  readonly source: CreatedRegulatorySource;
}

// ===== الوحدة 12: مسار التدقيق (قراءة فقط، ترقيم مفتاحي) =====

export interface AuditTrailItem {
  readonly id: string;
  readonly eventType: string;
  readonly actorName: string;
  readonly resourceType: string;
  readonly resourceId: string;
  readonly occurredAt: string;
}

export interface AuditTrailCursor {
  readonly occurredAt: string;
  readonly id: string;
}

export interface AuditTrailPageInput {
  readonly before: AuditTrailCursor | null;
}

export interface AuditTrailData {
  readonly items: readonly AuditTrailItem[];
  readonly chainIntegrity: 'VALID' | 'INVALID' | 'UNKNOWN';
  /** حالة مرساة الذيل: MATCH / NO_REFERENCE / TRUNCATED / PREFIX_MISMATCH. */
  readonly chainAnchorState: 'MATCH' | 'NO_REFERENCE' | 'TRUNCATED' | 'PREFIX_MISMATCH';
  readonly hasMore: boolean;
  readonly nextCursor: AuditTrailCursor | null;
  readonly generatedAt: string;
}

// ===== نتائج تنفيذ الوكلاء (قراءة آمنة) =====

export interface AgentRunResultData {
  readonly runId: string;
  readonly runStatus: AgentRunStatus;
  readonly outcome: 'COMPLETED' | 'FAILED';
  readonly providerId: string;
  readonly modelId: string | null;
  readonly failureCode: string | null;
  readonly output: string | null;
  readonly providerGeneratedAt: string | null;
  readonly createdAt: string;
  readonly citationsJson: string | null;
  readonly truthClass: string | null;
}

// ===== الوحدة 14: التكاملات (حالة قراءة فقط) =====

export const integrationProviderStates = ['CONNECTED', 'UNAVAILABLE', 'UNKNOWN'] as const;
export type IntegrationProviderState = (typeof integrationProviderStates)[number];

export interface IntegrationProviderStatus {
  readonly providerId: string;
  readonly providerKind: string;
  readonly state: IntegrationProviderState;
  readonly discoveredModelIds: readonly string[];
  readonly checkedAt: string;
}

export interface IntegrationStatusData {
  readonly providers: readonly IntegrationProviderStatus[];
  readonly enabledLocalModels: readonly string[];
  readonly modelRegistryConfigurationAvailable: boolean;
  readonly checkedAt: string;
}

// ===== سياق الجلسة (عرض فقط — التفويض مصدره الخلف) =====

export interface SessionContext {
  readonly actorId: string;
  readonly displayName: string;
  readonly responsibility: string;
  readonly permissions: readonly string[];
}

// ===== عقود مدخلات IPC المنتقلة إلى الحزمة المشتركة (MP02) =====

export interface SwitchDemoActorInput {
  readonly actorId: string;
}

export interface CreateRemediationTaskInput {
  readonly obligationId: string;
  readonly title: string;
  readonly description: string;
  readonly dueAt: string | null;
  readonly priority: 'CRITICAL' | 'HIGH' | 'MEDIUM' | 'LOW';
  /** الإسناد اختياري؛ غيابه يعني مهمة بلا مالك تُسند لاحقاً. */
  readonly assigneeUserId?: string | null;
}

export interface CreatedTask {
  readonly taskId: string;
  readonly status: string;
  /** لا موافقة عند الإنشاء — تُنشأ عند طلب المراجعة. */
  readonly pendingApprovalId: string | null;
}

export interface DecideApprovalInput {
  readonly approvalId: string;
  readonly decision: 'APPROVED' | 'REJECTED';
  readonly reason: string;
}

export interface ApprovalDecisionResult {
  readonly approvalId: string;
  readonly decision: 'APPROVED' | 'REJECTED';
  readonly resourceId: string;
}

export interface UpdateObligationInput {  readonly obligationId: string;
  readonly title: string;
  readonly description: string;
  readonly owner: string;
  readonly dueDate: string | null;
  readonly priority: ObligationPriority;
  readonly status: ObligationStatus;
  readonly complianceStatus: ComplianceStatus;
  readonly reviewStatus: ReviewStatus;
  readonly expectedVersion: number;
}

export interface ArchiveObligationInput {
  readonly obligationId: string;
  readonly expectedVersion: number;
}

export interface RegisterEvidenceReferenceInput {
  readonly obligationId: string;
  readonly label: string;
  readonly classification: 'PUBLIC' | 'INTERNAL' | 'CONFIDENTIAL' | 'RESTRICTED' | 'HIGHLY_RESTRICTED';
  readonly expiresAt: string | null;
  readonly contentFingerprint: string;
}

export interface RegisteredEvidenceReference {
  readonly evidenceId: string;
  readonly status: string;
  readonly verificationState: string;
}

// =====================================================================
// MASTER PHASE 03 — GOVERNED AGENT & TOOL RUNTIME
// أنواع مطبوعة لأسطح التنفيذ المحكوم والمهارات والأدوات وتكوين النموذج.
// لا يقبل أي عقد منها هوية/دوراً/صلاحيات/عنوان مزود — السلطة خلفية حصراً.
// =====================================================================

// ===== بروفايل تنفيذ الوكيل (تعليمة تنفيذ إصدارية منفصلة عن الوصف) =====

export interface AgentExecutionProfileVersion {
  readonly id: string;
  readonly tenantId: string;
  readonly agentId: string;
  readonly versionNumber: number;
  readonly instruction: string;
  readonly createdAt: string;
}

export interface AgentExecutionProfilesData {
  readonly hasData: boolean;
  readonly items: readonly AgentExecutionProfileVersion[];
  readonly latestVersionId: string | null;
}

export interface CreateAgentExecutionProfileInput {
  readonly agentId: string;
  readonly instruction: string;
}

export interface AgentExecutionProfileMutationResult {
  readonly profile: AgentExecutionProfileVersion;
}

// ===== طلب التنفيذ العام المحكوم =====

export interface RequestAgentExecutionInput {
  readonly agentId: string;
  readonly taskContent: string;
  readonly skillVersionId?: string | null;
  readonly knowledgeQuery?: string | null;
}

export interface AgentExecutionRequestResult {
  readonly run: AgentRunRecord;
  readonly approvalId: string;
}

export interface AgentRunExecutionBindingView {
  readonly agentRunId: string;
  readonly agentRunRevision: number;
  readonly executionProfileVersionId: string;
  readonly taskContentHash: string;
  readonly modelIdAtRequest: string | null;
  readonly boundAt: string;
}

export interface StartExecutionOutcome {
  readonly run: AgentRunRecord;
  readonly result: AgentRunResultData | null;
}

// ===== استدعاءات الأدوات (قراءة حالة فقط) =====

export const toolInvocationStatuses = [
  'WAITING_APPROVAL',
  'RUNNING',
  'COMPLETED',
  'FAILED',
  'CANCELLED',
] as const;

export type ToolInvocationStatus = (typeof toolInvocationStatuses)[number];

export const toolPolicyDecisions = ['ALLOW', 'REQUIRE_APPROVAL', 'DENY'] as const;

export type ToolPolicyDecision = (typeof toolPolicyDecisions)[number];

export interface ToolInvocationRecord {
  readonly id: string;
  readonly tenantId: string;
  readonly agentRunId: string;
  readonly agentRunRevision: number;
  readonly toolId: string;
  readonly toolVersionId: string;
  readonly adapterKey: string | null;
  readonly inputJson: string;
  readonly inputHash: string;
  readonly policyDecision: ToolPolicyDecision;
  readonly policyReason: string;
  readonly status: ToolInvocationStatus;
  readonly revision: number;
  readonly failureCode: string | null;
  readonly createdAt: string;
  readonly startedAt: string | null;
  readonly finishedAt: string | null;
  readonly updatedAt: string;
}

export interface ToolInvocationsData {
  readonly hasData: boolean;
  readonly items: readonly ToolInvocationRecord[];
}

// ===== سجل المهارات =====

export const skillStatuses = ['ACTIVE', 'DISABLED'] as const;

export type SkillStatus = (typeof skillStatuses)[number];

export interface SkillRecord {
  readonly id: string;
  readonly tenantId: string;
  readonly name: string;
  readonly description: string;
  readonly status: SkillStatus;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface SkillVersionRecord {
  readonly id: string;
  readonly tenantId: string;
  readonly skillId: string;
  readonly skillStatus: SkillStatus;
  readonly versionNumber: number;
  readonly instruction: string;
  readonly inputContract: string;
  readonly outputContract: string;
  readonly constraints: string;
  readonly createdAt: string;
}

export interface SkillAssignmentRow {
  readonly id: string;
  readonly agentId: string;
  readonly skillVersionId: string;
  readonly assignedAt: string;
  readonly unassignedAt: string | null;
}

export interface SkillCatalogItem {
  readonly skill: SkillRecord;
  readonly versions: readonly SkillVersionRecord[];
  readonly assignments: readonly SkillAssignmentRow[];
}

export interface SkillCatalogData {
  readonly hasData: boolean;
  readonly items: readonly SkillCatalogItem[];
}

export interface CreateSkillInput {
  readonly name: string;
  readonly description: string;
  readonly instruction: string;
  readonly inputContract: string;
  readonly outputContract: string;
  readonly constraints: string;
}

export interface CreateSkillVersionInput {
  readonly skillId: string;
  readonly instruction: string;
  readonly inputContract: string;
  readonly outputContract: string;
  readonly constraints: string;
}

export interface SkillIdInput {
  readonly skillId: string;
}

export interface AssignSkillInput {
  readonly agentId: string;
  readonly skillVersionId: string;
}

export interface UnassignSkillInput {
  readonly assignmentId: string;
}

export interface SkillMutationResult {
  readonly skill: SkillRecord;
}

export interface SkillVersionMutationResult {
  readonly version: SkillVersionRecord;
}

// ===== سجل الأدوات =====

export const toolStatuses = ['ACTIVE', 'DISABLED'] as const;

export type ToolStatus = (typeof toolStatuses)[number];

export interface ToolRecord {
  readonly id: string;
  readonly tenantId: string;
  readonly name: string;
  readonly description: string;
  readonly status: ToolStatus;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export const toolMutations = ['NONE', 'STATE_CHANGE'] as const;
export const toolExternalities = ['INTERNAL', 'EXTERNAL'] as const;
export const toolPrivileges = ['STANDARD', 'PRIVILEGED'] as const;
export const toolReversibilities = [
  'NOT_APPLICABLE',
  'REVERSIBLE',
  'CONDITIONAL',
  'IRREVERSIBLE',
] as const;

export interface ToolEffectProfile {
  readonly mutation: (typeof toolMutations)[number];
  readonly externality: (typeof toolExternalities)[number];
  readonly privilege: (typeof toolPrivileges)[number];
  readonly reversibility: (typeof toolReversibilities)[number];
}

export interface ToolVersionView {
  readonly id: string;
  readonly versionNumber: number;
  readonly effect: ToolEffectProfile;
  /** صدق المنتج: تسجيل أداة لا يعني قابلية التنفيذ — تنفيذ يتطلب محوّلاً موثوقاً. */
  readonly executable: boolean;
}

export interface ToolCatalogItem {
  readonly tool: ToolRecord;
  readonly versions: readonly ToolVersionView[];
}

export interface ToolCatalogData {
  readonly hasData: boolean;
  readonly items: readonly ToolCatalogItem[];
}

export interface CreateToolContractField {
  readonly name: string;
  readonly dataType: string;
  readonly required: boolean;
  readonly description: string;
}

export interface CreateToolContract {
  readonly fields: readonly CreateToolContractField[];
  readonly allowsAdditionalFields: boolean;
}

export interface CreateToolInput {
  readonly name: string;
  readonly description: string;
  readonly inputContract: CreateToolContract;
  readonly outputContract: CreateToolContract;
  readonly effect: ToolEffectProfile;
}

export interface CreateToolVersionInput {
  readonly toolId: string;
  readonly inputContract: CreateToolContract;
  readonly outputContract: CreateToolContract;
  readonly effect: ToolEffectProfile;
}

export interface ToolIdInput {
  readonly toolId: string;
}

export interface ToolMutationResult {
  readonly tool: ToolRecord;
}

export interface ToolVersionMutationResult {
  readonly version: {
    readonly id: string;
    readonly versionNumber: number;
    readonly effect: ToolEffectProfile;
  };
}

export interface AssignToolInput {
  readonly agentId: string;
  readonly toolVersionId: string;
}

export interface ToolAssignmentIdInput {
  readonly assignmentId: string;
}

export interface AgentToolAssignmentView {
  readonly id: string;
  readonly agentId: string;
  readonly toolId: string;
  readonly toolName: string;
  readonly toolVersionId: string;
  readonly versionNumber: number;
  readonly assignedAt: string;
  readonly unassignedAt: string | null;
}

export interface AgentToolAssignmentsData {
  readonly hasData: boolean;
  readonly items: readonly AgentToolAssignmentView[];
}

export interface ToolAssignmentMutationResult {
  readonly assignment: AgentToolAssignmentView;
}

// ===== تكوين النموذج المحلي المحكوم =====

export interface ConfigureLocalModelInput {
  readonly modelId: string;
}

export interface LocalModelConfigurationResult {
  readonly configuredModelId: string | null;
}


// =====================================================================
// MASTER PHASE 04 - INTELLIGENCE / KNOWLEDGE / MEMORY / PLANS
// =====================================================================

export const knowledgeSourceStatuses = ['ACTIVE', 'DISABLED', 'FAILED'] as const;
export type KnowledgeSourceStatus = (typeof knowledgeSourceStatuses)[number];

export interface IngestKnowledgeSourceInput {
  readonly title: string;
  readonly origin: string;
  readonly text: string;
  readonly filenameHint?: string | null;
}

export interface IngestKnowledgeSourceResult {
  readonly sourceId: string;
  readonly documentId: string;
  readonly chunkCount: number;
  readonly duplicate: boolean;
}

export interface KnowledgeSourceView {
  readonly id: string;
  readonly sourceType: string;
  readonly title: string;
  readonly origin: string;
  readonly provenance: string;
  readonly status: KnowledgeSourceStatus | string;
  readonly chunkCount: number;
  readonly documentCount: number;
  readonly createdAt: string;
  readonly ingestedAt: string | null;
  readonly updatedAt: string;
}

export interface KnowledgeSourcesData {
  readonly items: readonly KnowledgeSourceView[];
  readonly generatedAt: string;
}

export interface KnowledgeDocumentView {
  readonly id: string;
  readonly format: string;
  readonly originalFilename: string | null;
  readonly charCount: number;
  readonly extractionStatus: string;
  readonly createdAt: string;
}

export interface KnowledgeSourceDetails {
  readonly id: string;
  readonly sourceType: string;
  readonly title: string;
  readonly origin: string;
  readonly provenance: string;
  readonly status: KnowledgeSourceStatus | string;
  readonly chunkCount: number;
  readonly createdAt: string;
  readonly ingestedAt: string | null;
  readonly updatedAt: string;
  readonly originRefResourceType: string | null;
  readonly originRefResourceId: string | null;
  readonly documents: readonly KnowledgeDocumentView[];
}

export interface DisableKnowledgeSourceInput {
  readonly sourceId: string;
}

export interface SearchKnowledgeInput {
  readonly query: string;
}

export interface KnowledgeRetrievalItem {
  readonly sourceId: string;
  readonly documentId: string;
  readonly chunkId: string;
  readonly sourceTitle: string;
  readonly provenance: string;
  readonly rank: number;
  readonly score: number;
  readonly content: string;
}

export interface KnowledgeSearchResultData {
  readonly items: readonly KnowledgeRetrievalItem[];
  readonly generatedAt: string;
}

export interface MemorySummaryEntry {
  readonly memoryType: string;
  readonly status: string;
  readonly count: number;
}

export interface MemoryRecord {
  readonly id: string;
  readonly tenantId: string;
  readonly memoryType: string;
  readonly scopeResourceType: string;
  readonly scopeResourceId: string | null;
  readonly agentId: string | null;
  readonly content: string;
  readonly truthClass: string;
  readonly provenanceKind: string;
  readonly provenanceRef: string | null;
  readonly status: string;
  readonly createdBy: string;
  readonly expiresAt: string | null;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface MemoryData {
  readonly summary: readonly MemorySummaryEntry[];
  readonly items: readonly MemoryRecord[];
  readonly generatedAt: string;
}

export interface DisableMemoryItemInput {
  readonly memoryId: string;
}

export interface ProposeSemanticMemoryInput {
  readonly content: string;
  readonly agentRunId?: string | null;
}

export interface ProposeSemanticMemoryResult {
  readonly memoryId: string;
}

export interface PromoteMemoryItemInput {
  readonly memoryId: string;
}

export const planStatuses = [
  'PENDING_APPROVAL', 'ACTIVE', 'RUNNING', 'COMPLETED', 'FAILED', 'REJECTED', 'CANCELLED',
] as const;
export type PlanStatus = (typeof planStatuses)[number];

export const planStepStatuses = ['PENDING', 'RUNNING', 'COMPLETED', 'FAILED', 'CANCELLED'] as const;
export type PlanStepStatus = (typeof planStepStatuses)[number];

export const delegationStatuses = ['PENDING', 'RUNNING', 'COMPLETED', 'FAILED', 'CANCELLED'] as const;
export type DelegationStatus = (typeof delegationStatuses)[number];

export interface ProposedPlanStep {
  readonly objective: string;
  readonly requiredCapability: string;
  readonly requiresKnowledge: boolean;
  readonly evidenceRequirement: string;
}

export interface CreatePlanInput {
  readonly goal: string;
}

export interface CreatePlanResult {
  readonly planId: string;
  readonly approvalId: string;
  readonly steps: readonly ProposedPlanStep[];
}

export interface CancelPlanInput {
  readonly planId: string;
}

export interface PlanRecord {
  readonly id: string;
  readonly tenantId: string;
  readonly goal: string;
  readonly status: PlanStatus;
  readonly stepsHash: string;
  readonly modelIdAtRequest: string | null;
  readonly revision: number;
  readonly requestedByUserId: string;
  readonly synthesisOutput: string | null;
  readonly conflictFlag: boolean;
  readonly createdAt: string;
  readonly updatedAt: string;
  readonly completedAt: string | null;
}

export interface PlansData {
  readonly items: readonly PlanRecord[];
}

export interface PlanStepRecord {
  readonly id: string;
  readonly tenantId: string;
  readonly planId: string;
  readonly stepIndex: number;
  readonly objective: string;
  readonly requiredCapability: string;
  readonly requiresKnowledge: boolean;
  readonly evidenceRequirement: string;
  readonly status: PlanStepStatus;
  readonly createdAt: string;
  readonly updatedAt: string;
}

export interface AgentDelegationRecord {
  readonly id: string;
  readonly tenantId: string;
  readonly planId: string;
  readonly planRevision: number;
  readonly stepId: string;
  readonly targetAgentId: string;
  readonly agentRunId: string | null;
  readonly objective: string;
  readonly allowedSkillVersionId: string | null;
  readonly status: DelegationStatus;
  readonly revision: number;
  readonly output: string | null;
  readonly citationsJson: string | null;
  readonly truthClass: string | null;
  readonly failureCode: string | null;
  readonly createdAt: string;
  readonly updatedAt: string;
  readonly finishedAt: string | null;
}

export interface VerificationRunRecord {
  readonly id: string;
  readonly tenantId: string;
  readonly subjectKind: string;
  readonly agentRunId: string | null;
  readonly planId: string | null;
  readonly outcome: string;
  readonly method: string;
  readonly deterministicChecksJson: string;
  readonly aiEvaluationJson: string | null;
  readonly aiAnalysisRunId: string | null;
  readonly createdBy: string;
  readonly createdAt: string;
}

export interface DelegationWithVerification {
  readonly delegation: AgentDelegationRecord;
  readonly verification: VerificationRunRecord | null;
}

export interface PlanDetails {
  readonly plan: PlanRecord;
  readonly steps: readonly PlanStepRecord[];
  readonly delegations: readonly DelegationWithVerification[];
  readonly pendingApprovalId: string | null;
}

export interface StartPlanExecutionInput {
  readonly planId: string;
}

export interface PlanExecutionResult {
  readonly plan: PlanRecord;
  readonly delegations: readonly DelegationWithVerification[];
}

export interface VerifyRunInput {
  readonly runId: string;
}

export interface VerificationView {
  readonly verification: VerificationRunRecord;
}

// =====================================================================
// MASTER PHASE 05 — PROVISIONING / AUTHENTICATION / USER ADMIN / BACKUP
// هوية حقيقية خلف النافذة: وضع تشغيل وجلسات وتهيئة إنتاج وإدارة مستخدمين
// ونسخ احتياطي — كلها بأشكال serde camelCase مطابقة لعقود Rust حرفياً.
// =====================================================================

export const authModes = ['UNPROVISIONED', 'DEMO', 'PRODUCTION'] as const;
export type AuthMode = (typeof authModes)[number];

export const workspaceUserStatuses = ['ACTIVE', 'DISABLED'] as const;
export type WorkspaceUserStatus = (typeof workspaceUserStatuses)[number];

export interface AuthStatusData {
  readonly mode: AuthMode;
  readonly authenticated: boolean;
  readonly needsBootstrap: boolean;
  readonly actorDisplayName: string | null;
  readonly sessionExpiresAt: string | null;
}

export interface AuthenticatedSession {
  readonly context: SessionContext;
  readonly sessionExpiresAt: string;
}

export interface BootstrapProductionInput {
  readonly organizationName: string;
  // GAP-05-OPEN: بيانات المؤسسة يعلنها المستخدم — رمز ISO 3166-1 alpha-2
  // وقطاع مكتوب. لا قيمة افتراضية في العقد ولا في الخلف.
  readonly countryCode: string;
  readonly industry: string;
  readonly adminDisplayName: string;
  readonly adminEmail: string;
  readonly password: string;
}

// كتالوج خيارات شاشة تأسيس المؤسسة: مصدره الخلف ليبقى كتالوج التحقق واحداً.
export interface BootstrapOptionsData {
  readonly countryCodes: readonly string[];
}

export interface LoginInput {
  readonly email: string;
  readonly password: string;
}

export interface WorkspaceUser {
  readonly userId: string;
  readonly displayName: string;
  readonly email: string;
  readonly status: WorkspaceUserStatus;
  readonly roles: readonly string[];
}

export interface WorkspaceUsersData {
  readonly users: readonly WorkspaceUser[];
  readonly availableRoles: readonly string[];
}

export interface SetUserStatusInput {
  readonly userId: string;
  readonly status: WorkspaceUserStatus;
}

export interface UserRoleInput {
  readonly userId: string;
  readonly roleCode: string;
}

export interface UserMutationResult {
  readonly userId: string;
}

// ===== 0026: إدارة المستخدمين الإنتاجية وكلمة المرور وتأليف الأعمال =====

export interface CreateWorkspaceUserInput {
  readonly displayName: string;
  readonly email: string;
  // كلمة مرور أولية يسلّمها المسؤول للعضو؛ لا تعيدها النواة ولا تسجّلها.
  readonly initialPassword: string;
  readonly roleCodes: readonly string[];
}

export interface CreatedWorkspaceUser {
  readonly userId: string;
  readonly email: string;
  readonly status: WorkspaceUserStatus;
  readonly roles: readonly string[];
}

export interface RevokeUserSessionsInput {
  readonly userId: string;
}

export interface RevokedUserSessions {
  readonly userId: string;
  readonly revokedSessions: number;
}

export interface ChangePasswordInput {
  readonly currentPassword: string;
  readonly newPassword: string;
}

export interface ChangedPassword {
  readonly changedAt: string;
  readonly revokedSessions: number;
}

// تأليف التزام مؤسسي: الانطباق يعلنه المستخدم؛ المنشأ والحالة يقررهما الخلف.
export interface CreateObligationInput {
  readonly sourceId: string;
  readonly title: string;
  readonly description: string;
  readonly owner: string;
  readonly dueDate: string | null;
  readonly priority: ObligationPriority;
  readonly applicability: ObligationApplicability;
}

export const obligationApplicabilities = ['APPLIES', 'DOES_NOT_APPLY', 'UNCERTAIN', 'REQUIRES_REVIEW'] as const;
export type ObligationApplicability = (typeof obligationApplicabilities)[number];

export interface CreatedObligation {
  readonly id: string;
  readonly status: ObligationStatus;
  readonly sourceType: SourceType;
  readonly verificationState: VerificationState;
  readonly createdAt: string;
}

// تحقق فعلي من سلسلة التدقيق — إعادة تشغيل SHA-256 على كل حدث.
export interface AuditChainVerification {
  readonly valid: boolean;
  readonly events: number;
  readonly brokenLinks: number;
  readonly checkedAt: string;
  /** حالة مرساة الذيل مقارنةً بآخر تحقق ناجح. */
  readonly anchorState: 'MATCH' | 'NO_REFERENCE' | 'TRUNCATED' | 'PREFIX_MISMATCH';
  /** عدد الأحداث التي غطتها المرساة وقت تثبيتها. */
  readonly anchoredEvents: number;
}

export interface CreateBackupInput {
  readonly targetDirectory: string;
}

export interface BackupResult {
  readonly fileName: string;
  readonly sha256: string;
  readonly bytes: number;
  readonly schemaVersion: number;
  readonly mode: string;
  readonly createdAt: string;
}

export interface BackupManifest {
  readonly format: string;
  readonly appVersion: string;
  readonly schemaVersion: number;
  readonly createdAt: string;
  readonly mode: string;
  readonly sha256: string;
  readonly bytes: number;
}

export interface RestoreBackupInput {
  readonly backupFile: string;
  readonly confirm: boolean;
}

export interface RestoreResult {
  readonly restoredMode: string;
  readonly restoredSchemaVersion: number;
  readonly safetyCopyFile: string;
  readonly authenticated: boolean;
}

// =====================================================================
// دورة حياة الأدلة وتأليف الضوابط (0022) — أنواع إضافية بحتة.
// لا يغيّر أي عقد قائم ولا يقبل أي منها هوية/مستأجراً/تصنيفاً أو حقل ملكية.
// =====================================================================

// ===== الأدلة: نقل الحالة والتفاصيل =====

export interface UpdateEvidenceStatusInput {
  readonly evidenceId: string;
  readonly status: EvidenceStatus;
}

export interface UpdatedEvidenceStatus {
  readonly evidenceId: string;
  readonly obligationId: string;
  readonly status: EvidenceStatus;
  readonly version: number;
  readonly obligationEvidenceStatus: EvidenceStatus;
  readonly updatedAt: string;
}

// ===== الضوابط: المدخلات =====

export interface CreateControlInput {
  readonly title: string;
  readonly category?: string | null;
  readonly owner?: string | null;
}

export interface UpdateControlInput {
  readonly controlId: string;
  readonly title: string;
  readonly category?: string | null;
  readonly owner?: string | null;
}

export interface ControlIdInput {
  readonly controlId: string;
}

export interface SetControlStatusInput {
  readonly controlId: string;
  readonly status: ControlStatus;
}

export interface ControlObligationLinkInput {
  readonly controlId: string;
  readonly obligationId: string;
}

export interface ControlRiskLinkInput {
  readonly controlId: string;
  readonly riskId: string;
}

export interface ControlEvidenceLinkInput {
  readonly controlId: string;
  readonly evidenceId: string;
}

export interface RecordControlAssessmentInput {
  readonly controlId: string;
  readonly effectiveness: ControlEffectiveness;
  readonly assessmentNote?: string | null;
}

// ===== الضوابط: النتائج =====

export interface ControlMutationResult {
  readonly controlId: string;
  readonly status: ControlStatus;
  readonly updatedAt: string;
}

export interface ControlAssessmentResult {
  readonly controlId: string;
  readonly effectiveness: ControlEffectiveness;
  readonly lastAssessedAt: string;
}

// linkKind يقابل عمود الهدف في جدول الربط: obligation_id أو risk_id أو evidence_id.
export type ControlLinkKind = 'obligation_id' | 'risk_id' | 'evidence_id';

export interface ControlLinkMutationResult {
  readonly controlId: string;
  readonly linkKind: ControlLinkKind;
  readonly targetId: string;
  readonly linkCount: number;
  readonly changed: boolean;
}

export interface ControlObligationLink {
  readonly obligationId: string;
  readonly title: string;
  readonly severity: string;
  readonly status: string;
  readonly createdAt: string;
}

export interface ControlRiskLink {
  readonly riskId: string;
  readonly title: string;
  readonly severity: string;
  readonly status: string;
  readonly createdAt: string;
}

export interface ControlEvidenceLink {
  readonly evidenceId: string;
  readonly label: string;
  readonly status: string;
  readonly classification: string;
  readonly createdAt: string;
}

export interface ControlDetail extends ControlRecord {
  readonly assessmentNote: string | null;
  readonly linkedObligations: readonly ControlObligationLink[];
  readonly linkedRisks: readonly ControlRiskLink[];
  readonly linkedEvidence: readonly ControlEvidenceLink[];
}


/* =============================================================================
   سجل الموفرين وسجل النماذج — عقد موحّد واحد لكل مسارات الذكاء الاصطناعي.
   قواعد ملزمة يعبّر عنها هذا العقد صراحةً:
   - لا يُعاد أي سر إلى الواجهة إطلاقاً؛ الحقل الوحيد المتاح هو `credentialConfigured`.
   - لا تُخترع قدرات نموذج ولا نافذة سياق: القيم غير المتحقق منها `null` لا صفر،
     و`capabilitySource` يعلن مصدر المعرفة (اكتشاف حي · إعلان الموفر · مجهول).
   - الحالة `UNVERIFIED` تعني «لم يُختبر الاتصال»، وليست نجاحاً؛ لا يتحول أي موفر
     إلى `CONNECTED` بمجرد حفظ مفتاح.
   ============================================================================= */

/** أنواع الموفرين المدعومة. `OPENAI_COMPATIBLE` يخدم LM Studio وأي خادم بعقد OpenAI. */
export const aiProviderKinds = [
  'OLLAMA',
  'LM_STUDIO',
  'OPENAI_COMPATIBLE',
  'OPENAI',
  'DEEPSEEK',
  'ANTHROPIC',
  'QWEN',
  'ZAI',
  'XAI',
  'OPENROUTER',
] as const;
export type AiProviderKind = (typeof aiProviderKinds)[number];

/** حالات الاتصال: لا `CONNECTED` إلا بعد اختبار ناجح فعلي. */
export const aiProviderConnectionStates = ['CONNECTED', 'DISCONNECTED', 'UNAVAILABLE', 'UNVERIFIED'] as const;
export type AiProviderConnectionState = (typeof aiProviderConnectionStates)[number];

/** مصدر معرفة القدرات — يمنع تقديم التخمين كحقيقة. */
export const aiCapabilitySources = ['DISCOVERED', 'DECLARED', 'UNKNOWN'] as const;
export type AiCapabilitySource = (typeof aiCapabilitySources)[number];

export interface AiProviderRecord {
  readonly providerId: string;
  readonly kind: AiProviderKind;
  readonly displayName: string;
  /** عنوان أساسي بلا أي مادة سرية؛ يُعرض للمستخدم للتحقق البصري. */
  readonly baseUrl: string;
  readonly enabled: boolean;
  readonly connectionState: AiProviderConnectionState;
  readonly connectionCheckedAt: string | null;
  /** هل توجد مادة سرية محفوظة في المخزن الآمن؟ لا يُعاد السر نفسه أبداً. */
  readonly credentialConfigured: boolean;
  readonly requiresCredential: boolean;
  readonly supportsDiscovery: boolean;
  readonly modelCount: number;
  readonly builtIn: boolean;
}

export interface AiProvidersData {
  readonly providers: readonly AiProviderRecord[];
  readonly selectedProviderId: string | null;
  readonly selectedModelId: string | null;
  readonly dataMode: string;
  readonly generatedAt: string;
}

export interface AiProviderMutationResult {
  readonly providerId: string;
  readonly connectionState: AiProviderConnectionState;
  readonly credentialConfigured: boolean;
}

export interface AiProviderCredentialResult {
  readonly providerId: string;
  readonly credentialConfigured: boolean;
}

export interface AiProviderTestResult {
  readonly providerId: string;
  readonly connectionState: AiProviderConnectionState;
  readonly checkedAt: string;
  readonly latencyMs: number;
  readonly discoveredModelCount: number;
  /** رمز خطأ آمن فقط — لا نص استجابة ولا تفصيل شبكي ولا أي مادة سرية. */
  readonly safeErrorCode: string | null;
}

export interface AiModelRecord {
  readonly modelId: string;
  readonly providerId: string;
  readonly providerKind: AiProviderKind;
  readonly providerDisplayName: string;
  readonly displayName: string;
  readonly available: boolean;
  readonly selected: boolean;
  /** `null` يعني غير معروف بثقة — لا قيمة مخترعة. */
  readonly contextLimit: number | null;
  readonly supportsTools: boolean | null;
  readonly supportsStructuredOutput: boolean | null;
  readonly supportsVision: boolean | null;
  readonly capabilitySource: AiCapabilitySource;
  readonly lastDiscoveredAt: string | null;
}

export interface AiModelsData {
  readonly models: readonly AiModelRecord[];
  readonly selectedProviderId: string | null;
  readonly selectedModelId: string | null;
  readonly dataMode: string;
  readonly generatedAt: string;
}

export interface AiModelDiscoveryResult {
  readonly providerId: string;
  readonly models: readonly AiModelRecord[];
  readonly discoveredAt: string;
  readonly safeErrorCode: string | null;
}

export interface ConfigureAiProviderInput {
  readonly providerId: string;
  readonly kind: AiProviderKind;
  readonly displayName: string;
  readonly baseUrl: string;
  readonly enabled: boolean;
}

export interface SetAiProviderCredentialInput {
  readonly providerId: string;
  /** يُستهلك مرة واحدة ويُخزَّن في مخزن آمن مدعوم بنظام التشغيل؛ لا يُسجَّل ولا يُعاد. */
  readonly credential: string;
}

export interface AiProviderIdInput {
  readonly providerId: string;
}

export interface SelectAiModelInput {
  readonly providerId: string;
  readonly modelId: string;
}

/* =============================================================================
   التقارير المحلية — وحدات التقارير كانت معلنة كغير منفّذة، وهذا عقدها الحقيقي.
   قواعد ملزمة يعبّر عنها العقد:
   - كل تقرير يُبنى من سجلات محلية حقيقية داخل نطاق المستأجر؛ لا رقم مُختلق.
   - كل تقرير يحمل منشأه: وضع البيانات والجداول المصدر ولحظة التوليد ووسم صريح
     بأنه **ملخص تشغيلي محلي وليس إقرار امتثال**.
   - الصفوف محدودة بحدّ أعلى، و`truncated` يعلن أن هناك ما لم يُدرج بدل التلميح.
   ============================================================================= */

export const reportIds = [
  'OBLIGATION_STATUS',
  'RISK_REGISTER',
  'EVIDENCE_REGISTER',
  'CONTROL_EFFECTIVENESS',
  'TASK_WORKLOAD',
  'AUDIT_ACTIVITY',
] as const;
export type ReportId = (typeof reportIds)[number];

export const reportExportFormats = ['CSV', 'MARKDOWN'] as const;
export type ReportExportFormat = (typeof reportExportFormats)[number];

export interface ReportColumn {
  readonly key: string;
  readonly label: string;
}

export interface ReportSection {
  readonly title: string;
  readonly columns: readonly ReportColumn[];
  readonly rows: readonly (readonly string[])[];
  /** العدد الكامل قبل الحدّ الأعلى — يميّز «لا بيانات» من «اقتُطع». */
  readonly totalRows: number;
  readonly truncated: boolean;
}

export interface ReportProvenance {
  readonly dataMode: string;
  readonly sourceTables: readonly string[];
  readonly generatedAt: string;
  /** وسم صريح: ملخص تشغيلي محلي لا إقرار امتثال قانوني. */
  readonly disclaimer: string;
}

export interface ReportCatalogEntry {
  readonly reportId: ReportId;
  readonly title: string;
  readonly description: string;
}

export interface ReportCatalogData {
  readonly reports: readonly ReportCatalogEntry[];
  /** الحدّ الأعلى لصفوف أي قسم — يُعرض للمستخدم بصدق. */
  readonly maxRowsPerSection: number;
  readonly generatedAt: string;
}

export interface GenerateReportInput {
  readonly reportId: ReportId;
}

export interface ReportData {
  readonly reportId: ReportId;
  readonly title: string;
  readonly sections: readonly ReportSection[];
  readonly provenance: ReportProvenance;
}

export interface ExportReportInput {
  readonly reportId: ReportId;
  readonly format: ReportExportFormat;
  /** مسار مطلق يختاره المستخدم من حوار الحفظ الأصلي — لا مسار من الخلف. */
  readonly targetPath: string;
}

export interface ReportExportResult {
  readonly fileName: string;
  readonly bytes: number;
  readonly sha256: string;
  readonly format: ReportExportFormat;
}
