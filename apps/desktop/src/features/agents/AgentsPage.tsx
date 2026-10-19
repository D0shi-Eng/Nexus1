import { useMemo, useRef, useState, type FormEvent } from 'react';
import { CheckCircle2, CircleAlert, Filter, LoaderCircle, Pencil, Play, Plus, RefreshCw, Search, Sparkles, ToggleLeft, ToggleRight, X } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { AgentRecord, AgentStatus } from '@nexus/contracts';
import { agentStatuses } from '@nexus/contracts';

import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { PageHeader } from '../../components/PageHeader';
import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { useModalFocus } from '../../lib/use-modal-focus';
import { useSession } from '../../lib/use-session';
import { AgentRunsPanel } from './AgentRunsPanel';
import { AgentPlansPanel } from './AgentPlansPanel';
import { AgentSkillsPanel } from './AgentSkillsPanel';
import { AgentToolsPanel } from './AgentToolsPanel';
import { RunAgentDialog } from './RunAgentDialog';

type AgentStatusFilter = 'ALL' | AgentStatus;

const formatDate = (value: string, language: string): string => {
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return '—';
  }
  return new Intl.DateTimeFormat(language, { day: 'numeric', month: 'short', year: 'numeric' }).format(parsed);
};

const toAgentStatusFilter = (value: string): AgentStatusFilter =>
  value === 'ALL' ? 'ALL' : agentStatuses.find((status) => status === value) ?? 'ALL';

const matchesAgent = (agent: AgentRecord, query: string, status: AgentStatusFilter): boolean => {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const matchesQuery = !normalizedQuery || `${agent.name} ${agent.description}`.toLocaleLowerCase().includes(normalizedQuery);
  return matchesQuery && (status === 'ALL' || agent.status === status);
};

const AgentStatusBadge = ({ status }: { readonly status: AgentStatus }) => {
  const { t } = useTranslation();
  return <span className={`badge badge--${status.toLowerCase()}`}>{t(`status.${status}`)}</span>;
};

const AgentDialog = ({
  agent,
  onClose,
  onSaved,
}: {
  readonly agent?: AgentRecord;
  readonly onClose: () => void;
  readonly onSaved: () => void;
}) => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  // إدارة تركيز الحوار الموحدة: دخول التركيز، حصر Tab، Escape، واستعادة تركيز المشغّل.
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const [name, setName] = useState(agent?.name ?? '');
  const [description, setDescription] = useState(agent?.description ?? '');
  const [validationError, setValidationError] = useState(false);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const createMutation = useMutation({ mutationFn: nexusApi.createAgent });
  const updateMutation = useMutation({ mutationFn: nexusApi.updateAgent });
  const isPending = createMutation.isPending || updateMutation.isPending;

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setValidationError(false);
    setErrorCode(null);
    if (!name.trim() || name.length > 160 || !description.trim() || description.length > 2_000) {
      setValidationError(true);
      return;
    }
    try {
      if (agent) {
        await updateMutation.mutateAsync({ agentId: agent.id, name, description });
      } else {
        await createMutation.mutateAsync({ name, description });
      }
      await queryClient.invalidateQueries({ queryKey: ['agents'] });
      onSaved();
    } catch (error) {
      setErrorCode(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="agent-dialog-title" aria-describedby="agent-dialog-description" tabIndex={-1}>
        <header className="task-dialog__header">
          <div>
            <h2 id="agent-dialog-title">{agent ? t('agents.dialog.editTitle') : t('agents.dialog.createTitle')}</h2>
            <p id="agent-dialog-description">{t('agents.dialog.subtitle')}</p>
          </div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}>
            <X size={18} aria-hidden="true" />
          </button>
        </header>
        <form className="task-form" onSubmit={(event) => { void submit(event); }} noValidate>
          <label>
            <span>{t('agents.dialog.name')}</span>
            <input value={name} onChange={(event) => setName(event.currentTarget.value)} maxLength={160} required aria-invalid={validationError && !name.trim()} />
          </label>
          <label>
            <span>{t('agents.dialog.description')}</span>
            <textarea value={description} onChange={(event) => setDescription(event.currentTarget.value)} maxLength={2_000} rows={5} required aria-invalid={validationError && !description.trim()} />
          </label>
          {validationError ? <p className="form-error" role="alert">{t('agents.dialog.validation')}</p> : null}
          {errorCode ? <p className="form-error" role="alert">{t(`errors.${errorCode}`, { defaultValue: t('agents.dialog.failure') })}</p> : null}
          <footer className="task-form__actions">
            <button className="button button--secondary" type="button" onClick={onClose} disabled={isPending}>{t('actions.cancel')}</button>
            <button className="button button--primary" type="submit" disabled={isPending}>
              {isPending ? <LoaderCircle className="spin" size={16} aria-hidden="true" /> : <CheckCircle2 size={16} aria-hidden="true" />}
              {agent ? t('agents.dialog.save') : t('agents.dialog.create')}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
};

type AgentTab = 'AGENTS' | 'SKILLS' | 'TOOLS' | 'PLANS';
// ترتيب ثابت لأقسام الوكلاء — يُستخدم للعرض وللتنقل بأسهم لوحة المفاتيح في tablist.
const AGENT_TABS: readonly AgentTab[] = ['AGENTS', 'SKILLS', 'TOOLS', 'PLANS'];

export const AgentsPage = () => {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const language = i18n.resolvedLanguage ?? 'ar';
  const [query, setQuery] = useState('');
  const [status, setStatus] = useState<AgentStatusFilter>('ALL');
  const [dialogAgent, setDialogAgent] = useState<AgentRecord | null | undefined>(undefined);
  const [runAgent, setRunAgent] = useState<AgentRecord | null>(null);
  const [activeTab, setActiveTab] = useState<AgentTab>('AGENTS');
  const tabRefs = useRef<Partial<Record<AgentTab, HTMLButtonElement | null>>>({});

  // نمط tablist الكامل: الأسهم تنقل التركيز والتحديد معاً (Home/End للطرفين)،
  // ولا يدخل أي قسم آخر في دورة Tab — القسم النشط وحده tabIndex=0.
  const onTabListKeyDown = (event: React.KeyboardEvent<HTMLDivElement>): void => {
    const currentIndex = AGENT_TABS.indexOf(activeTab);
    let nextIndex: number | null = null;
    if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
      nextIndex = (currentIndex + 1) % AGENT_TABS.length;
    } else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
      nextIndex = (currentIndex - 1 + AGENT_TABS.length) % AGENT_TABS.length;
    } else if (event.key === 'Home') {
      nextIndex = 0;
    } else if (event.key === 'End') {
      nextIndex = AGENT_TABS.length - 1;
    }
    if (nextIndex === null) {
      return;
    }
    event.preventDefault();
    const nextTab = AGENT_TABS[nextIndex];
    setActiveTab(nextTab);
    tabRefs.current[nextTab]?.focus();
  };
  const [feedback, setFeedback] = useState<string | null>(null);
  const [mutationError, setMutationError] = useState<string | null>(null);
  const { can } = useSession();
  const agentsQuery = useQuery({ queryKey: ['agents'], queryFn: nexusApi.getAgents, staleTime: 30_000 });
  const statusMutation = useMutation({ mutationFn: nexusApi.setAgentStatus });
  const errorCode = resolveErrorCode(agentsQuery.error);
  const visibleAgents = useMemo(
    () => (agentsQuery.data?.items ?? []).filter((agent) => matchesAgent(agent, query, status)),
    [agentsQuery.data?.items, query, status],
  );
  const noData = Boolean(agentsQuery.data && !agentsQuery.data.hasData);
  const hasFilteredOutAgents = Boolean(agentsQuery.data?.items.length) && visibleAgents.length === 0;

  const clearFilters = (): void => {
    setQuery('');
    setStatus('ALL');
  };

  const changeStatus = async (agent: AgentRecord): Promise<void> => {
    setFeedback(null);
    setMutationError(null);
    const nextStatus: AgentStatus = agent.status === 'ACTIVE' ? 'DISABLED' : 'ACTIVE';
    try {
      await statusMutation.mutateAsync({ agentId: agent.id, status: nextStatus });
      await queryClient.invalidateQueries({ queryKey: ['agents'] });
      // رسالة الحالة تتبع الإجراء الفعلي (تفعيل/تعطيل) لا نص «حفظ التعريف».
      setFeedback(t('agents.statusSuccess'));
    } catch (error) {
      setMutationError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  return (
    <section id="agents" className="tasks-page content-container" aria-labelledby="agents-title" aria-live="polite">
      <PageHeader
        eyebrow={t('agents.eyebrow')}
        eyebrowIcon={Sparkles}
        title={t('agents.title')}
        titleId="agents-title"
        description={t('agents.subtitle')}
        actions={
          <>
            <button className="button button--secondary" type="button" onClick={() => { void agentsQuery.refetch(); }} disabled={agentsQuery.isFetching}>
              <RefreshCw size={16} className={agentsQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
              {t('agents.actions.refresh')}
            </button>
            {can('agent:create') ? (
              <button className="button button--primary" type="button" onClick={() => { setFeedback(null); setMutationError(null); setDialogAgent(null); }}>
                <Plus size={16} aria-hidden="true" />
                {t('agents.actions.create')}
              </button>
            ) : null}
          </>
        }
      />

      <div className="agents-tabs" role="tablist" aria-label={t('agents.tabs.label')} onKeyDown={onTabListKeyDown}>
        {AGENT_TABS.map((tab) => (
          <button
            key={tab}
            id={`agents-tab-${tab.toLowerCase()}`}
            ref={(element) => { tabRefs.current[tab] = element; }}
            type="button"
            role="tab"
            aria-selected={activeTab === tab}
            aria-controls="agents-tabpanel"
            tabIndex={activeTab === tab ? 0 : -1}
            className={activeTab === tab ? 'agents-tab agents-tab--active' : 'agents-tab'}
            onClick={() => setActiveTab(tab)}
          >
            {t(`agents.tabs.${tab.toLowerCase()}`)}
          </button>
        ))}
      </div>
      <p className="form-note">{t('agents.runtimeNote')}</p>
      <AgentRunsPanel />
      {feedback ? <p className="operation-feedback" role="status">{feedback}</p> : null}
      {mutationError ? <div className="error-panel" role="alert"><CircleAlert size={20} aria-hidden="true" /><p>{t(`errors.${mutationError}`, { defaultValue: t('agents.dialog.failure') })}</p></div> : null}
      {agentsQuery.isLoading ? <LoadingState label={t('agents.loading')} /> : null}
      {agentsQuery.isError ? <ErrorState code={errorCode} deniedMessage={t('agents.permissionDenied')} /> : null}
      {noData ? <EmptyState icon={Sparkles} title={t('agents.empty')} /> : null}

      <div id="agents-tabpanel" role="tabpanel" aria-labelledby={`agents-tab-${activeTab.toLowerCase()}`} tabIndex={-1}>
      {activeTab === 'SKILLS' ? <AgentSkillsPanel /> : null}
      {activeTab === 'PLANS' ? <AgentPlansPanel /> : null}
      {activeTab === 'TOOLS' ? <AgentToolsPanel /> : null}
      {activeTab === 'AGENTS' && agentsQuery.data?.hasData ? (
        <section className="panel panel--table" aria-labelledby="agents-list-title">
          <div className="panel__header"><div><h2 id="agents-list-title">{t('agents.listTitle')}</h2><span>{visibleAgents.length}</span></div><Sparkles size={18} aria-hidden="true" /></div>
          <div className="attention-toolbar" aria-label={t('agents.filters.title')}>
            <div className="attention-toolbar__heading"><Filter size={16} aria-hidden="true" /><h3>{t('agents.filters.title')}</h3></div>
            <label className="attention-search"><span className="visually-hidden">{t('actions.search')}</span><Search size={16} aria-hidden="true" /><input value={query} onChange={(event) => setQuery(event.currentTarget.value)} placeholder={t('agents.searchPlaceholder')} /></label>
            <label>
              <span>{t('agents.filters.status')}</span>
              <select value={status} onChange={(event) => setStatus(toAgentStatusFilter(event.currentTarget.value))}>
                <option value="ALL">{t('agents.filters.all')}</option>
                {agentStatuses.map((value) => <option value={value} key={value}>{t(`status.${value}`)}</option>)}
              </select>
            </label>
            <button className="button button--quiet" type="button" onClick={clearFilters} disabled={!query && status === 'ALL'}>{t('agents.actions.clearFilters')}</button>
          </div>
          {visibleAgents.length > 0 ? (
            <div className="table-scroll">
              <table>
                <thead><tr><th>{t('agents.table.name')}</th><th>{t('agents.table.description')}</th><th>{t('agents.table.status')}</th><th>{t('agents.table.created')}</th><th>{t('agents.table.updated')}</th><th>{t('agents.table.action')}</th></tr></thead>
                <tbody>{visibleAgents.map((agent) => {
                  const isActive = agent.status === 'ACTIVE';
                  return <tr key={agent.id}>
                    <td><strong>{agent.name}</strong></td>
                    <td>{agent.description}</td>
                    <td><AgentStatusBadge status={agent.status} /></td>
                    <td><time dateTime={agent.createdAt}>{formatDate(agent.createdAt, language)}</time></td>
                    <td><time dateTime={agent.updatedAt}>{formatDate(agent.updatedAt, language)}</time></td>
                    <td>
                      <div className="table-actions">
                        {isActive && can('agent:execute') ? (
                          <button className="button button--quiet" type="button" onClick={() => { setFeedback(null); setMutationError(null); setRunAgent(agent); }}>
                            <Play size={16} aria-hidden="true" />{t('agents.actions.run')}
                          </button>
                        ) : null}
                        {can('agent:update') ? (
                          <>
                            <button className="button button--quiet" type="button" onClick={() => { setFeedback(null); setMutationError(null); setDialogAgent(agent); }}><Pencil size={16} aria-hidden="true" />{t('agents.actions.edit')}</button>
                            <button className="button button--quiet" type="button" onClick={() => { void changeStatus(agent); }} disabled={statusMutation.isPending}>
                              {isActive ? <ToggleLeft size={16} aria-hidden="true" /> : <ToggleRight size={16} aria-hidden="true" />}
                              {isActive ? t('agents.actions.disable') : t('agents.actions.enable')}
                            </button>
                          </>
                        ) : null}
                      </div>
                    </td>
                  </tr>;
                })}</tbody>
              </table>
            </div>
          ) : <EmptyState icon={CheckCircle2} variant="inline" title={hasFilteredOutAgents ? t('agents.noMatches') : t('agents.empty')} />}
        </section>
      ) : null}
      </div>
      {dialogAgent !== undefined ? <AgentDialog agent={dialogAgent ?? undefined} onClose={() => setDialogAgent(undefined)} onSaved={() => { setFeedback(t('agents.dialog.success')); setDialogAgent(undefined); }} /> : null}
      {runAgent ? <RunAgentDialog agent={runAgent} onClose={() => setRunAgent(null)} /> : null}
    </section>
  );
};
