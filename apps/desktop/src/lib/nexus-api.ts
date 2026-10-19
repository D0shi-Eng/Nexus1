import { invoke } from '@tauri-apps/api/core';
import type {
  AgentExecutionProfileMutationResult, AgentExecutionProfilesData, AgentMutationResult,
  AgentRunExecutionBindingView, AgentRunMutationResult, AgentRunResultData, AgentRunsData,
  AgentsData, AgentToolAssignmentsData, AiModelDiscoveryResult, AiModelsData,
  AiProviderCheck, AiProviderCredentialResult, AiProviderIdInput, AiProviderMutationResult,
  AiProviderTestResult, AiProvidersData, ConfigureAiProviderInput,
  ApprovalDecisionResult, ArchiveObligationInput,
  ArchivedObligation, AssignSkillInput, AssignTaskInput, AssignToolInput, AttentionData,
  AuditTrailData,
  AuditTrailPageInput, AuthStatusData, AuthenticatedSession, BackupManifest, BackupResult,
  BootstrapOptionsData, BootstrapProductionInput, CancelAgentRunInput, CancelPlanInput,
  ConfigureLocalModelInput,
  ControlAssessmentResult, ControlDetail, ControlEvidenceLinkInput,
  ControlLinkMutationResult, ControlMutationResult, ControlObligationLinkInput,
  ControlRiskLinkInput, ControlsData, CreateAgentExecutionProfileInput, CreateAgentInput,
  CreateBackupInput, CreateControlInput,
  CreatePlanInput, CreatePlanResult, CreateRemediationTaskInput, CreateSkillInput,
  CreateSkillVersionInput, CreateToolInput, CreateToolVersionInput, CreatedTask,
  DashboardData, DecideApprovalInput, DemoActorsData, DisableKnowledgeSourceInput,
  DisableMemoryItemInput, EvidenceData, HealthStatus, IngestKnowledgeSourceInput,
  IngestKnowledgeSourceResult, IntegrationStatusData, KnowledgeSearchResultData,
  KnowledgeSourceDetails, KnowledgeSourcesData, LocalModelConfigurationResult,
  LoginInput, MemoryData, ModuleAvailability, ObligationDetails, ObligationsData,
  OrganizationalIntelligenceData, PlanDetails, PlanExecutionResult, PlansData,
  PromoteMemoryItemInput, ProposeSemanticMemoryInput, ProposeSemanticMemoryResult,
  RecordControlAssessmentInput, RegisteredEvidenceReference, RegisterEvidenceReferenceInput,
  ReportCatalogData, ReportData, ReportExportResult, ExportReportInput,
  RestoreBackupInput,
  RestoreResult, RegulatorySourcesData, RequestAgentExecutionInput, RisksData,
  SearchKnowledgeInput, SelectAiModelInput, SessionContext, SetAgentStatusInput,
  GenerateReportInput,
  SetAiProviderCredentialInput, SetControlStatusInput, SetUserStatusInput,
  SkillCatalogData, SkillIdInput, SkillMutationResult, SkillVersionMutationResult,
  StartExecutionOutcome, StartPlanExecutionInput, SwitchDemoActorInput, TasksData,
  TaskTransitionRecord, TaskTransitionResult, TransitionTaskInput,
  ToolAssignmentMutationResult, ToolAssignmentIdInput, ToolCatalogData, ToolIdInput,
  ToolInvocationsData, ToolMutationResult, ToolVersionMutationResult, UnassignSkillInput,
  UpdatedEvidenceStatus, UpdatedObligation, UpdateAgentInput, UpdateControlInput,
  UpdateEvidenceStatusInput, UpdateObligationInput, UserMutationResult, UserRoleInput,
  VerificationView, VerifyObligationInput, VerifiedObligation, VerifyRunInput, WorkspaceUsersData,
  // 0026: إدارة المستخدمين وكلمة المرور وتأليف الأعمال
  AuditChainVerification, ChangePasswordInput, ChangedPassword, CreateObligationInput,
  CreatedObligation, CreatedWorkspaceUser, CreateRegulatorySourceInput,
  CreateWorkspaceUserInput, RegulatorySourceMutationResult, RevokedUserSessions,
  RevokeUserSessionsInput,
} from '@nexus/contracts';

export type {
  ArchiveObligationInput, AssignTaskInput, CreateRemediationTaskInput, CreatedTask,
  DecideApprovalInput, TaskTransitionRecord, TaskTransitionResult, TransitionTaskInput,
  ApprovalDecisionResult, RegisterEvidenceReferenceInput, RegisteredEvidenceReference,
  RequestAgentExecutionInput, SwitchDemoActorInput, UpdateObligationInput,
  VerifiedObligation, VerifyObligationInput,
} from '@nexus/contracts';

export class NexusApiError extends Error {
  public constructor(public readonly code: string, message: string) {
    super(message);
    this.name = 'NexusApiError';
  }
}

interface TauriSafeErrorPayload {
  readonly code: string;
  readonly message: string;
}

const isTauriSafeError = (value: unknown): value is TauriSafeErrorPayload =>
  typeof value === 'object'
  && value !== null
  && typeof (value as { readonly code?: unknown }).code === 'string'
  && typeof (value as { readonly message?: unknown }).message === 'string';

const invokeSafely = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    // لا نمرر رسالة Rust غير الآمنة؛ نحتفظ فقط بالعقد الآمن المصنف الصادر من النواة.
    if (isTauriSafeError(error)) {
      throw new NexusApiError(error.code, error.message);
    }
    throw new NexusApiError('LOCAL_SERVICE_UNAVAILABLE', 'Local application service is unavailable.');
  }
};

export const nexusApi = {
  getAgentRuns: (): Promise<AgentRunsData> => invokeSafely<AgentRunsData>('get_agent_runs'),
  cancelAgentRun: (input: CancelAgentRunInput): Promise<AgentRunMutationResult> =>
    invokeSafely<AgentRunMutationResult>('cancel_agent_run', { input }),
  getAgentRunResult: (input: CancelAgentRunInput): Promise<AgentRunResultData> =>
    invokeSafely<AgentRunResultData>('get_agent_run_result', { input }),
  getAgents: (): Promise<AgentsData> => invokeSafely<AgentsData>('get_agents'),
  createAgent: (input: CreateAgentInput): Promise<AgentMutationResult> =>
    invokeSafely<AgentMutationResult>('create_agent', { input }),
  updateAgent: (input: UpdateAgentInput): Promise<AgentMutationResult> =>
    invokeSafely<AgentMutationResult>('update_agent', { input }),
  setAgentStatus: (input: SetAgentStatusInput): Promise<AgentMutationResult> =>
    invokeSafely<AgentMutationResult>('set_agent_status', { input }),
  getDashboard: (): Promise<DashboardData> => invokeSafely<DashboardData>('get_dashboard'),
  getAttention: (): Promise<AttentionData> => invokeSafely<AttentionData>('get_attention'),
  getObligations: (): Promise<ObligationsData> => invokeSafely<ObligationsData>('get_obligations'),
  getOrganizationalIntelligence: (): Promise<OrganizationalIntelligenceData> =>
    invokeSafely<OrganizationalIntelligenceData>('get_organizational_intelligence'),
  getRegulatorySources: (): Promise<RegulatorySourcesData> =>
    invokeSafely<RegulatorySourcesData>('get_regulatory_sources'),
  getAuditTrail: (input: AuditTrailPageInput): Promise<AuditTrailData> =>
    invokeSafely<AuditTrailData>('get_audit_trail', { input }),
  getIntegrationStatus: (): Promise<IntegrationStatusData> =>
    invokeSafely<IntegrationStatusData>('get_integration_status'),
  getSessionContext: (): Promise<SessionContext> =>
    invokeSafely<SessionContext>('get_session_context'),
  getRisks: (): Promise<RisksData> => invokeSafely<RisksData>('get_risks'),
  getTasks: (): Promise<TasksData> => invokeSafely<TasksData>('get_tasks'),
  getControls: (): Promise<ControlsData> => invokeSafely<ControlsData>('get_controls'),
  getEvidence: (): Promise<EvidenceData> => invokeSafely<EvidenceData>('get_evidence'),
  // دورة حياة الأدلة: نقل حالة الدليل ضمن عقده الحالي (لا تصنيف ولا هوية من العميل).
  updateEvidenceStatus: (input: UpdateEvidenceStatusInput): Promise<UpdatedEvidenceStatus> =>
    invokeSafely<UpdatedEvidenceStatus>('update_evidence_status', { input }),

  // ===== تأليف الضوابط وربطها وتقييمها =====
  getControlDetail: (controlId: string): Promise<ControlDetail> =>
    invokeSafely<ControlDetail>('get_control_detail', { controlId }),

  // ===== التقارير المحلية =====
  // التقارير تُبنى من سجلات محلية حقيقية؛ والتصدير يكتب إلى مسار مطلق يختاره
  // المستخدم من حوار الحفظ الأصلي، ويتحقق منه الخلف قبل أي كتابة.
  getReportCatalog: (): Promise<ReportCatalogData> => invokeSafely<ReportCatalogData>('get_report_catalog'),
  generateReport: (input: GenerateReportInput): Promise<ReportData> =>
    invokeSafely<ReportData>('generate_report', { input }),
  exportReport: (input: ExportReportInput): Promise<ReportExportResult> =>
    invokeSafely<ReportExportResult>('export_report', { input }),
  createControl: (input: CreateControlInput): Promise<ControlMutationResult> =>
    invokeSafely<ControlMutationResult>('create_control', { input }),
  updateControl: (input: UpdateControlInput): Promise<ControlMutationResult> =>
    invokeSafely<ControlMutationResult>('update_control', { input }),
  disableControl: (input: SetControlStatusInput): Promise<ControlMutationResult> =>
    invokeSafely<ControlMutationResult>('disable_control', { input }),
  recordControlAssessment: (input: RecordControlAssessmentInput): Promise<ControlAssessmentResult> =>
    invokeSafely<ControlAssessmentResult>('record_control_assessment', { input }),
  linkControlObligation: (input: ControlObligationLinkInput): Promise<ControlLinkMutationResult> =>
    invokeSafely<ControlLinkMutationResult>('link_control_obligation', { input }),
  unlinkControlObligation: (input: ControlObligationLinkInput): Promise<ControlLinkMutationResult> =>
    invokeSafely<ControlLinkMutationResult>('unlink_control_obligation', { input }),
  linkControlRisk: (input: ControlRiskLinkInput): Promise<ControlLinkMutationResult> =>
    invokeSafely<ControlLinkMutationResult>('link_control_risk', { input }),
  unlinkControlRisk: (input: ControlRiskLinkInput): Promise<ControlLinkMutationResult> =>
    invokeSafely<ControlLinkMutationResult>('unlink_control_risk', { input }),
  linkControlEvidence: (input: ControlEvidenceLinkInput): Promise<ControlLinkMutationResult> =>
    invokeSafely<ControlLinkMutationResult>('link_control_evidence', { input }),
  unlinkControlEvidence: (input: ControlEvidenceLinkInput): Promise<ControlLinkMutationResult> =>
    invokeSafely<ControlLinkMutationResult>('unlink_control_evidence', { input }),
  getObligation: (obligationId: string): Promise<ObligationDetails> => invokeSafely<ObligationDetails>('get_obligation', { obligationId }),
  updateObligation: (input: UpdateObligationInput): Promise<UpdatedObligation> => invokeSafely<UpdatedObligation>('update_obligation', { input }),
  archiveObligation: (input: ArchiveObligationInput): Promise<ArchivedObligation> => invokeSafely<ArchivedObligation>('archive_obligation', { input }),
  getModuleAvailability: (): Promise<readonly ModuleAvailability[]> => invokeSafely<readonly ModuleAvailability[]>('get_module_availability'),
  getHealth: (): Promise<HealthStatus> => invokeSafely<HealthStatus>('get_system_health'),
  // فحص المزود الحي الصريح — إجراء مستخدم من الواجهة، لا استعلام دوري في كل تحميل.
  checkAiProvider: (): Promise<AiProviderCheck> =>
    invokeSafely<AiProviderCheck>('check_ai_provider'),

  // ===== سجل الموفرين وسجل النماذج =====
  // قاعدة ملزمة: لا تعيد هذه الواجهات أي مادة سرية؛ الحقل الوحيد المتاح
  // `credentialConfigured` وهو علامة وجود لا قيمة سر.
  getAiProviders: (): Promise<AiProvidersData> => invokeSafely<AiProvidersData>('get_ai_providers'),
  configureAiProvider: (input: ConfigureAiProviderInput): Promise<AiProviderMutationResult> =>
    invokeSafely<AiProviderMutationResult>('configure_ai_provider', { input }),
  removeAiProvider: (input: AiProviderIdInput): Promise<AiProviderMutationResult> =>
    invokeSafely<AiProviderMutationResult>('remove_ai_provider', { input }),
  setAiProviderCredential: (input: SetAiProviderCredentialInput): Promise<AiProviderCredentialResult> =>
    invokeSafely<AiProviderCredentialResult>('set_ai_provider_credential', { input }),
  clearAiProviderCredential: (input: AiProviderIdInput): Promise<AiProviderCredentialResult> =>
    invokeSafely<AiProviderCredentialResult>('clear_ai_provider_credential', { input }),
  testAiProvider: (input: AiProviderIdInput): Promise<AiProviderTestResult> =>
    invokeSafely<AiProviderTestResult>('test_ai_provider', { input }),
  discoverAiModels: (input: AiProviderIdInput): Promise<AiModelDiscoveryResult> =>
    invokeSafely<AiModelDiscoveryResult>('discover_ai_models', { input }),
  getAiModels: (): Promise<AiModelsData> => invokeSafely<AiModelsData>('get_ai_models'),
  selectAiModel: (input: SelectAiModelInput): Promise<AiModelsData> =>
    invokeSafely<AiModelsData>('select_ai_model', { input }),
  clearAiModelSelection: (): Promise<AiModelsData> => invokeSafely<AiModelsData>('clear_ai_model_selection'),
  createRemediationTask: (input: CreateRemediationTaskInput): Promise<CreatedTask> =>
    invokeSafely<CreatedTask>('create_remediation_task', { input }),
  // 0025: أوامر دورة حياة المهمة — الانتقال والإسناد والسجل.
  assignTask: (input: AssignTaskInput): Promise<TaskTransitionResult> =>
    invokeSafely<TaskTransitionResult>('assign_task', { input }),
  transitionTask: (input: TransitionTaskInput): Promise<TaskTransitionResult> =>
    invokeSafely<TaskTransitionResult>('transition_task', { input }),
  getTaskHistory: (taskId: string): Promise<readonly TaskTransitionRecord[]> =>
    invokeSafely<readonly TaskTransitionRecord[]>('get_task_history', { taskId }),
  // 0025: التحقق البشري من الالتزام — خطوة المراجعة المستقلة.
  verifyObligation: (input: VerifyObligationInput): Promise<VerifiedObligation> =>
    invokeSafely<VerifiedObligation>('verify_obligation', { input }),
  decideApproval: (input: DecideApprovalInput): Promise<ApprovalDecisionResult> =>
    invokeSafely<ApprovalDecisionResult>('decide_approval', { input }),
  registerEvidenceReference: (input: RegisterEvidenceReferenceInput): Promise<RegisteredEvidenceReference> =>
    invokeSafely<RegisteredEvidenceReference>('register_evidence_reference', { input }),
  getDemoActors: (): Promise<DemoActorsData> => invokeSafely<DemoActorsData>('get_demo_actors'),
  switchDemoActor: (input: SwitchDemoActorInput): Promise<DemoActorsData> =>
    invokeSafely<DemoActorsData>('switch_demo_actor', { input }),

  // ===== MASTER PHASE 03 — التنفيذ المحكوم والمهارات والأدوات وتكوين النموذج =====
  getAgentExecutionProfiles: (agentId: string): Promise<AgentExecutionProfilesData> =>
    invokeSafely<AgentExecutionProfilesData>('get_agent_execution_profiles', { agentId }),
  createAgentExecutionProfile: (
    input: CreateAgentExecutionProfileInput,
  ): Promise<AgentExecutionProfileMutationResult> =>
    invokeSafely<AgentExecutionProfileMutationResult>('create_agent_execution_profile', { input }),
  requestAgentExecution: (input: RequestAgentExecutionInput): Promise<{ run: AgentRunMutationResult['run']; approvalId: string }> =>
    invokeSafely('request_agent_execution', { input }),
  startQueuedExecution: (runId: string): Promise<StartExecutionOutcome> =>
    invokeSafely<StartExecutionOutcome>('start_queued_execution', { runId }),
  getAgentRunExecutionBinding: (runId: string): Promise<AgentRunExecutionBindingView> =>
    invokeSafely<AgentRunExecutionBindingView>('get_agent_run_execution_binding', { runId }),
  getToolInvocationsForRun: (runId: string): Promise<ToolInvocationsData> =>
    invokeSafely<ToolInvocationsData>('get_tool_invocations_for_run', { runId }),
  getSkillCatalog: (): Promise<SkillCatalogData> => invokeSafely<SkillCatalogData>('get_skill_catalog'),
  createSkill: (input: CreateSkillInput): Promise<SkillMutationResult> =>
    invokeSafely<SkillMutationResult>('create_skill', { input }),
  createSkillVersion: (input: CreateSkillVersionInput): Promise<SkillVersionMutationResult> =>
    invokeSafely<SkillVersionMutationResult>('create_skill_version', { input }),
  disableSkill: (input: SkillIdInput): Promise<SkillMutationResult> =>
    invokeSafely<SkillMutationResult>('disable_skill', { input }),
  assignSkill: (input: AssignSkillInput): Promise<void> =>
    invokeSafely<void>('assign_skill', { input }),
  unassignSkill: (input: UnassignSkillInput): Promise<void> =>
    invokeSafely<void>('unassign_skill', { input }),
  getToolCatalog: (): Promise<ToolCatalogData> => invokeSafely<ToolCatalogData>('get_tool_catalog'),
  createTool: (input: CreateToolInput): Promise<ToolMutationResult> =>
    invokeSafely<ToolMutationResult>('create_tool', { input }),
  createToolVersion: (input: CreateToolVersionInput): Promise<ToolVersionMutationResult> =>
    invokeSafely<ToolVersionMutationResult>('create_tool_version', { input }),
  disableTool: (input: ToolIdInput): Promise<ToolMutationResult> =>
    invokeSafely<ToolMutationResult>('disable_tool', { input }),
  assignTool: (input: AssignToolInput): Promise<ToolAssignmentMutationResult> =>
    invokeSafely<ToolAssignmentMutationResult>('assign_tool', { input }),
  unassignTool: (input: ToolAssignmentIdInput): Promise<void> =>
    invokeSafely<void>('unassign_tool', { input }),
  getAgentToolAssignments: (agentId: string): Promise<AgentToolAssignmentsData> =>
    invokeSafely<AgentToolAssignmentsData>('get_agent_tool_assignments', { agentId }),
  configureLocalModel: (input: ConfigureLocalModelInput): Promise<LocalModelConfigurationResult> =>
    invokeSafely<LocalModelConfigurationResult>('configure_local_model', { input }),
  clearLocalModelConfiguration: (): Promise<LocalModelConfigurationResult> =>
    invokeSafely<LocalModelConfigurationResult>('clear_local_model_configuration'),

  // ===== MASTER PHASE 04 — المعرفة والذاكرة والخطط والتحقق =====
  ingestKnowledgeSource: (input: IngestKnowledgeSourceInput): Promise<IngestKnowledgeSourceResult> =>
    invokeSafely<IngestKnowledgeSourceResult>('ingest_knowledge_source', { input }),
  getKnowledgeSources: (): Promise<KnowledgeSourcesData> =>
    invokeSafely<KnowledgeSourcesData>('get_knowledge_sources'),
  getKnowledgeSource: (sourceId: string): Promise<KnowledgeSourceDetails> =>
    invokeSafely<KnowledgeSourceDetails>('get_knowledge_source', { sourceId }),
  searchKnowledge: (input: SearchKnowledgeInput): Promise<KnowledgeSearchResultData> =>
    invokeSafely<KnowledgeSearchResultData>('search_knowledge', { input }),
  disableKnowledgeSource: (input: DisableKnowledgeSourceInput): Promise<void> =>
    invokeSafely<void>('disable_knowledge_source', { input }),
  getMemory: (): Promise<MemoryData> => invokeSafely<MemoryData>('get_memory'),
  disableMemoryItem: (input: DisableMemoryItemInput): Promise<void> =>
    invokeSafely<void>('disable_memory_item', { input }),
  proposeSemanticMemory: (input: ProposeSemanticMemoryInput): Promise<ProposeSemanticMemoryResult> =>
    invokeSafely<ProposeSemanticMemoryResult>('propose_semantic_memory', { input }),
  promoteMemoryItem: (input: PromoteMemoryItemInput): Promise<void> =>
    invokeSafely<void>('promote_memory_item', { input }),
  createPlan: (input: CreatePlanInput): Promise<CreatePlanResult> =>
    invokeSafely<CreatePlanResult>('create_plan', { input }),
  getPlans: (): Promise<PlansData> => invokeSafely<PlansData>('get_plans'),
  getPlan: (planId: string): Promise<PlanDetails> =>
    invokeSafely<PlanDetails>('get_plan', { planId }),
  cancelPlan: (input: CancelPlanInput): Promise<void> =>
    invokeSafely<void>('cancel_plan', { input }),
  startPlanExecution: (input: StartPlanExecutionInput): Promise<PlanExecutionResult> =>
    invokeSafely<PlanExecutionResult>('start_plan_execution', { input }),
  verifyAgentRun: (input: VerifyRunInput): Promise<VerificationView> =>
    invokeSafely<VerificationView>('verify_agent_run', { input }),

  // ===== MASTER PHASE 05 — التهيئة والدخول والجلسات وإدارة المستخدمين والنسخ الاحتياطي =====
  getAuthStatus: (): Promise<AuthStatusData> => invokeSafely<AuthStatusData>('get_auth_status'),
  // GAP-05-OPEN: كتالوج دول التأسيس من الخلف — الواجهة لا تخترع قائمة بنفسها.
  getBootstrapOptions: (): Promise<BootstrapOptionsData> =>
    invokeSafely<BootstrapOptionsData>('get_bootstrap_options'),
  startDemoExperience: (): Promise<AuthenticatedSession> =>
    invokeSafely<AuthenticatedSession>('start_demo_experience'),
  bootstrapProduction: (input: BootstrapProductionInput): Promise<AuthenticatedSession> =>
    invokeSafely<AuthenticatedSession>('bootstrap_production', { input }),
  login: (input: LoginInput): Promise<AuthenticatedSession> =>
    invokeSafely<AuthenticatedSession>('login', { input }),
  logout: (): Promise<void> => invokeSafely<void>('logout'),
  listWorkspaceUsers: (): Promise<WorkspaceUsersData> =>
    invokeSafely<WorkspaceUsersData>('list_workspace_users'),
  setUserStatus: (input: SetUserStatusInput): Promise<UserMutationResult> =>
    invokeSafely<UserMutationResult>('set_user_status', { input }),
  assignUserRole: (input: UserRoleInput): Promise<UserMutationResult> =>
    invokeSafely<UserMutationResult>('assign_user_role', { input }),
  revokeUserRole: (input: UserRoleInput): Promise<UserMutationResult> =>
    invokeSafely<UserMutationResult>('revoke_user_role', { input }),

  // ===== 0026: إدارة المستخدمين الإنتاجية وكلمة المرور وتأليف الأعمال =====
  // كل ما يخصّ الهوية والمستأجر يُشتقّ في الخلف من الجلسة، ولا يُرسل من هنا.
  createWorkspaceUser: (input: CreateWorkspaceUserInput): Promise<CreatedWorkspaceUser> =>
    invokeSafely<CreatedWorkspaceUser>('create_workspace_user', { input }),
  revokeUserSessions: (input: RevokeUserSessionsInput): Promise<RevokedUserSessions> =>
    invokeSafely<RevokedUserSessions>('revoke_user_sessions', { input }),
  changePassword: (input: ChangePasswordInput): Promise<ChangedPassword> =>
    invokeSafely<ChangedPassword>('change_password', { input }),
  createRegulatorySource: (input: CreateRegulatorySourceInput): Promise<RegulatorySourceMutationResult> =>
    invokeSafely<RegulatorySourceMutationResult>('create_regulatory_source', { input }),
  createObligation: (input: CreateObligationInput): Promise<CreatedObligation> =>
    invokeSafely<CreatedObligation>('create_obligation', { input }),
  verifyAuditChain: (): Promise<AuditChainVerification> =>
    invokeSafely<AuditChainVerification>('verify_audit_chain'),
  createBackup: (input: CreateBackupInput): Promise<BackupResult> =>
    invokeSafely<BackupResult>('create_backup', { input }),
  listBackups: (input: CreateBackupInput): Promise<readonly BackupManifest[]> =>
    invokeSafely<readonly BackupManifest[]>('list_backups', { input }),
  restoreBackup: (input: RestoreBackupInput): Promise<RestoreResult> =>
    invokeSafely<RestoreResult>('restore_backup', { input }),
} as const;
