import { useEffect, useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Building2, LogOut, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { AttentionItem, ModuleAvailability, ObligationSummary } from '@nexus/contracts';

import { nexusApi } from '../lib/nexus-api';
import { severityRank } from '../lib/format';
import { BrandMark } from '../components/BrandMark';
import { ModuleView } from './ModuleView';
import { Topbar } from './Topbar';
import { navigationItems, type NavigationKey } from './navigation';
import { AttentionDetailsDialog, ObligationDetailsDialog } from './dialogs/DetailDialogs';
import { ApprovalDecisionDialog, EvidenceDialog, TaskDialog } from './dialogs/WorkflowDialogs';
import { WorkspaceSearchDialog } from './dialogs/WorkspaceSearchDialog';
import { OverviewPage } from '../features/overview/OverviewPage';
import { AttentionPage } from '../features/attention/AttentionPage';
import { ObligationsPage } from '../features/obligations/ObligationsPage';
import { RegulatoryIntelligencePage } from '../features/regulatory/RegulatoryIntelligencePage';
import { OrganizationalIntelligencePage } from '../features/organizational-intelligence/OrganizationalIntelligencePage';
import { RisksPage } from '../features/risks/RisksPage';
import { EvidencePage } from '../features/evidence/EvidencePage';
import { TasksPage } from '../features/tasks/TasksPage';
import { ControlsPage } from '../features/controls/ControlsPage';
import { AgentsPage } from '../features/agents/AgentsPage';
import { KnowledgePage } from '../features/knowledge/KnowledgePage';
import { AuditTrailPage } from '../features/audit/AuditTrailPage';
import { ReportsPage } from '../features/reports/ReportsPage';import { IntegrationsPage } from '../features/integrations/IntegrationsPage';
import { SettingsPage } from '../features/settings/SettingsPage';

export const App = () => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [activeNavigation, setActiveNavigation] = useState<NavigationKey>('overview');
  const [taskDialogOpen, setTaskDialogOpen] = useState(false);
  const [evidenceDialogOpen, setEvidenceDialogOpen] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [selectedObligation, setSelectedObligation] = useState<ObligationSummary | null>(null);
  const [selectedAttention, setSelectedAttention] = useState<AttentionItem | null>(null);
  const [selectedApprovalId, setSelectedApprovalId] = useState<string | null>(null);
  const [workflowObligationId, setWorkflowObligationId] = useState<string | undefined>();
  const [feedback, setFeedback] = useState<
    'taskCreated' | 'evidenceCreated' | 'approvalDecided' | 'noObligations' | null
  >(null);

  // بيانات اللوحة مشتركة بين القشرة والحواريات — نفس مفتاح التخزين المؤقت بلا تكرار طلبات.
  const authStatusQuery = useQuery({
    queryKey: ['auth-status'],
    queryFn: nexusApi.getAuthStatus,
    staleTime: 5_000,
  });
  const authStatus = authStatusQuery.data;
  const isDemo = authStatus?.mode === 'DEMO';
  const dashboardQuery = useQuery({
    queryKey: ['dashboard'],
    queryFn: nexusApi.getDashboard,
    staleTime: 30_000,
  });
  const moduleAvailabilityQuery = useQuery({
    queryKey: ['module-availability'],
    queryFn: nexusApi.getModuleAvailability,
    staleTime: 30_000,
  });
  const demoActorsQuery = useQuery({
    queryKey: ['demo-actors'],
    queryFn: nexusApi.getDemoActors,
    staleTime: 30_000,
  });
  const switchActorMutation = useMutation({
    mutationFn: nexusApi.switchDemoActor,
    // تبديل الفاعل يغيّر سياق التفويض بالكامل — إبطال كل الاستعلامات هو السلوك الآمن.
    onSuccess: () => {
      void queryClient.invalidateQueries();
    },
  });
  // الخروج ينهي الجلسة الإنتاجية كلها — إبطال كل الاستعلامات يعيد البوابة إلى شاشة الدخول.
  const logoutMutation = useMutation({
    mutationFn: nexusApi.logout,
    onSuccess: () => {
      void queryClient.invalidateQueries();
    },
  });
  const activeModuleAvailability: ModuleAvailability | undefined = moduleAvailabilityQuery.data?.find((module) => module.key === activeNavigation);

  const attentionItems = useMemo(
    () => [...(dashboardQuery.data?.attentionItems ?? [])].sort((left, right) => severityRank[left.severity] - severityRank[right.severity]),
    [dashboardQuery.data?.attentionItems],
  );

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent): void => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setSearchOpen(true);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  const openObligation = (obligation: ObligationSummary): void => {
    setSelectedAttention(null);
    setSelectedObligation(obligation);
  };

  const openAttention = (item: AttentionItem): void => {
    setSelectedObligation(null);
    setSelectedAttention(item);
  };

  // R7 / UAT-08: لا يُفتح حوار إنشاء بلا التزام قابل للربط. الإجراء كان يفتح
  // قائمة فارغة وزراً معطلاً — «إجراء بلا مخرج» في مساحة إنتاج جديدة. الآن
  // يُشرح السبب ويُبقى المستخدم في مساره بدل حوار مسدود.
  const hasLinkableObligations = (dashboardQuery.data?.obligations.length ?? 0) > 0;

  const openTaskFor = (obligationId?: string): void => {
    setSelectedObligation(null);
    if (obligationId === undefined && !hasLinkableObligations) {
      setFeedback('noObligations');
      return;
    }
    setWorkflowObligationId(obligationId);
    setTaskDialogOpen(true);
  };

  const openEvidenceFor = (obligationId?: string): void => {
    setSelectedObligation(null);
    if (obligationId === undefined && !hasLinkableObligations) {
      setFeedback('noObligations');
      return;
    }
    setWorkflowObligationId(obligationId);
    setEvidenceDialogOpen(true);
  };

  const completeWorkflow = (kind: 'taskCreated' | 'evidenceCreated' | 'approvalDecided'): void => {
    setTaskDialogOpen(false);
    setEvidenceDialogOpen(false);
    setWorkflowObligationId(undefined);
    setFeedback(kind);
    void queryClient.invalidateQueries({ queryKey: ['dashboard'] });
    void queryClient.invalidateQueries({ queryKey: ['attention'] });
    void queryClient.invalidateQueries({ queryKey: ['obligations'] });
    void queryClient.invalidateQueries({ queryKey: ['obligation'] });
    void queryClient.invalidateQueries({ queryKey: ['evidence'] });
    void queryClient.invalidateQueries({ queryKey: ['audit-trail'] });
    if (kind === 'taskCreated') {
      void queryClient.invalidateQueries({ queryKey: ['tasks'] });
    }
  };

  const openApprovalDecision = (approvalId: string): void => {
    setSelectedAttention(null);
    setSelectedApprovalId(approvalId);
  };

  const openAttentionObligation = (obligationId: string): void => {
    const obligation = dashboardQuery.data?.obligations.find((item) => item.id === obligationId);
    if (obligation) {
      openObligation(obligation);
    }
  };

  return (
    <div className="app-shell">
      <aside className={`sidebar ${sidebarOpen ? 'sidebar--open' : ''}`}>
        <div className="brand">
          <div className="brand__mark"><BrandMark /></div>
          <div>
            <strong>{t('product.name')}</strong>
            <span>{t('product.subtitle')}</span>
          </div>
          <button className="icon-button sidebar__dismiss" type="button" onClick={() => setSidebarOpen(false)} aria-label={t('actions.close')}>
            <X size={18} />
          </button>
        </div>
        <div className="tenant-chip">
          <Building2 size={16} aria-hidden="true" />
          <span>{isDemo ? t('system.demoTenant') : (authStatus?.actorDisplayName ?? t('system.productionTenant'))}</span>
        </div>
        <nav aria-label={t('accessibility.mainNavigation')} className="navigation">
          {navigationItems.map(({ key, icon: Icon }) => (
            <button
              className={`navigation__item ${activeNavigation === key ? 'navigation__item--active' : ''}`}
              key={key}
              type="button"
              /* العنصر النشط يجب أن يُعلن برمجياً لا بلون فقط: بلا `aria-current`
                 لا يستطيع قارئ الشاشة معرفة الوحدة الحالية (WCAG 4.1.2). */
              aria-current={activeNavigation === key ? 'page' : undefined}
              onClick={() => {
                setActiveNavigation(key);
                setSidebarOpen(false);
              }}
            >
              <Icon size={16} aria-hidden="true" />
              <span>{t(`navigation.${key}`)}</span>
            </button>
          ))}
        </nav>
        <div className="sidebar__footer">
          {authStatus?.mode === 'PRODUCTION' ? (
            <button
              className="icon-button sidebar__logout"
              type="button"
              onClick={() => { void logoutMutation.mutate(); }}
              aria-label={t('actions.logout')}
              title={t('actions.logout')}
              disabled={logoutMutation.isPending}
            >
              <LogOut size={16} aria-hidden="true" />
            </button>
          ) : null}
          {isDemo && demoActorsQuery.data && demoActorsQuery.data.actors.length > 0 ? (
            <label className="actor-switcher">
              <span>{t('identity.activeActor')}</span>
              <select
                value={demoActorsQuery.data.activeActorId}
                onChange={(event) => { void switchActorMutation.mutateAsync({ actorId: event.currentTarget.value }); }}
                disabled={switchActorMutation.isPending}
              >
                {demoActorsQuery.data.actors.map((actor) => (
                  <option key={actor.actorId} value={actor.actorId}>
                    {`${t(`identity.responsibility.${actor.responsibility}`)} · ${actor.displayName}`}
                  </option>
                ))}
              </select>
            </label>
          ) : null}
          <div className="mode-indicator"><span className="mode-indicator__dot" /><span>{t('system.localMode')}</span></div>
          {isDemo ? <span className="demo-label">{t('product.demo')}</span> : null}
        </div>
      </aside>
      {sidebarOpen ? <button className="backdrop" type="button" aria-label={t('actions.close')} onClick={() => setSidebarOpen(false)} /> : null}

      <main className="main-content">
        <Topbar onOpenSearch={() => setSearchOpen(true)} onOpenSidebar={() => setSidebarOpen(true)} />

        {activeNavigation === 'overview' ? (
          <OverviewPage
            onOpenAttention={openAttention}
            onOpenObligation={openObligation}
            onOpenTaskFor={() => openTaskFor()}
            onOpenEvidenceFor={() => openEvidenceFor()}
          />
        ) : activeNavigation === 'attention' ? (
          <AttentionPage
            obligationIds={dashboardQuery.data?.obligations.map((obligation) => obligation.id) ?? []}
            onCreateTask={openTaskFor}
            onRegisterEvidence={openEvidenceFor}
            onDecideApproval={openApprovalDecision}
            onOpenObligation={openAttentionObligation}
          />
        ) : activeNavigation === 'obligations' ? (
          <ObligationsPage
            onCreateTask={openTaskFor}
            onRegisterEvidence={openEvidenceFor}
          />
        ) : activeNavigation === 'regulatory' ? (
          <RegulatoryIntelligencePage />
        ) : activeNavigation === 'organization' ? (
          <OrganizationalIntelligencePage />
        ) : activeNavigation === 'risks' ? (
          <RisksPage />
        ) : activeNavigation === 'evidence' ? (
          <EvidencePage onRegisterEvidence={openEvidenceFor} />
        ) : activeNavigation === 'tasks' ? (
          <TasksPage />
        ) : activeNavigation === 'controls' ? (
          <ControlsPage />
        ) : activeNavigation === 'agents' ? (
          <AgentsPage />
        ) : activeNavigation === 'knowledge' ? (
          <KnowledgePage />
        ) : activeNavigation === 'reports' ? (
          <ReportsPage />
        ) : activeNavigation === 'audit' ? (
          <AuditTrailPage />
        ) : activeNavigation === 'integrations' ? (
          <IntegrationsPage />
        ) : activeNavigation === 'settings' ? (
          <SettingsPage />
        ) : (
          <ModuleView
            moduleKey={activeNavigation}
            availability={activeModuleAvailability}
            isLoading={moduleAvailabilityQuery.isLoading}
            isUnavailable={moduleAvailabilityQuery.isError}
          />
        )}

        {feedback ? <div className="operation-feedback" role="status"><span>{t(`feedback.${feedback}`)}</span><button className="icon-button" type="button" onClick={() => setFeedback(null)} aria-label={t('actions.close')}><X size={16} /></button></div> : null}

        {searchOpen && dashboardQuery.data ? (
          <WorkspaceSearchDialog
            obligations={dashboardQuery.data.obligations}
            attentionItems={attentionItems}
            onClose={() => setSearchOpen(false)}
            onNavigate={(key) => { setSearchOpen(false); setActiveNavigation(key); }}
            onOpenObligation={(obligation) => { setSearchOpen(false); openObligation(obligation); }}
            onReviewAttention={(item) => { setSearchOpen(false); openAttention(item); }}
          />
        ) : null}

        {selectedObligation ? (
          <ObligationDetailsDialog
            obligation={selectedObligation}
            onClose={() => setSelectedObligation(null)}
            onCreateTask={openTaskFor}
            onRegisterEvidence={openEvidenceFor}
          />
        ) : null}
        {(selectedApprovalId ?? (selectedAttention?.kind === 'APPROVAL' ? selectedAttention.resourceId : null)) ? (
          <ApprovalDecisionDialog
            approvalId={selectedApprovalId ?? selectedAttention!.resourceId}
            onClose={() => { setSelectedAttention(null); setSelectedApprovalId(null); }}
            onDecided={() => { setSelectedAttention(null); setSelectedApprovalId(null); completeWorkflow('approvalDecided'); }}
          />
        ) : null}
        {selectedAttention && selectedAttention.kind !== 'APPROVAL' && dashboardQuery.data ? (
          <AttentionDetailsDialog
            item={selectedAttention}
            obligation={dashboardQuery.data.obligations.find((obligation) => obligation.id === selectedAttention.resourceId)}
            onClose={() => setSelectedAttention(null)}
            onOpenObligation={(obligation) => { setSelectedAttention(null); openObligation(obligation); }}
          />
        ) : null}
        {taskDialogOpen && dashboardQuery.data ? (
          <TaskDialog
            obligations={dashboardQuery.data.obligations}
            initialObligationId={workflowObligationId}
            onClose={() => { setTaskDialogOpen(false); setWorkflowObligationId(undefined); }}
            onCreated={() => completeWorkflow('taskCreated')}
          />
        ) : null}
        {evidenceDialogOpen && dashboardQuery.data ? (
          <EvidenceDialog
            obligations={dashboardQuery.data.obligations}
            initialObligationId={workflowObligationId}
            onClose={() => { setEvidenceDialogOpen(false); setWorkflowObligationId(undefined); }}
            onCreated={() => completeWorkflow('evidenceCreated')}
          />
        ) : null}
      </main>
    </div>
  );
};
