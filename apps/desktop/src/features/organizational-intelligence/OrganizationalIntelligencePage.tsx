import { useMemo, useState } from 'react';
import { AlertTriangle, Building2, CheckCircle2, ClipboardCheck, Filter, RefreshCw, Search, ShieldCheck, TriangleAlert } from 'lucide-react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { OrganizationalIntelligencePriority, OrganizationalIntelligencePriorityKind } from '@nexus/contracts';

import { PageHeader } from '../../components/PageHeader';
import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { nexusApi } from '../../lib/nexus-api';

type PriorityKindFilter = 'ALL' | OrganizationalIntelligencePriorityKind;

const formatDate = (value: string | null, language: string, unavailable: string): string => {
  if (!value) {
    return unavailable;
  }

  return new Intl.DateTimeFormat(language, { day: 'numeric', month: 'short', year: 'numeric' }).format(new Date(value));
};

const PriorityBadge = ({ value }: { value: string }) => {
  const { t } = useTranslation();
  return <span className="status-badge">{t(`status.${value}`)}</span>;
};

const priorityMatches = (
  priority: OrganizationalIntelligencePriority,
  query: string,
  kind: PriorityKindFilter,
): boolean => {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const matchesQuery = !normalizedQuery || `${priority.title} ${priority.owner ?? ''}`.toLocaleLowerCase().includes(normalizedQuery);
  return matchesQuery && (kind === 'ALL' || priority.kind === kind);
};

export const OrganizationalIntelligencePage = () => {
  const { t, i18n } = useTranslation();
  const [query, setQuery] = useState('');
  const [kind, setKind] = useState<PriorityKindFilter>('ALL');
  const language = i18n.resolvedLanguage ?? 'ar';
  const intelligenceQuery = useQuery({
    queryKey: ['organizational-intelligence'],
    queryFn: nexusApi.getOrganizationalIntelligence,
    staleTime: 30_000,
  });
  const errorCode = resolveErrorCode(intelligenceQuery.error);
  const visiblePriorities = useMemo(
    () => (intelligenceQuery.data?.priorities ?? []).filter((priority) => priorityMatches(priority, query, kind)),
    [intelligenceQuery.data?.priorities, kind, query],
  );
  const noData = Boolean(intelligenceQuery.data && !intelligenceQuery.data.hasData);
  const hasFilteredOutPriorities = Boolean(intelligenceQuery.data?.priorities.length) && visiblePriorities.length === 0;

  const resetFilters = (): void => {
    setQuery('');
    setKind('ALL');
  };

  return (
    <section id="organization" className="organizational-intelligence-page content-container" aria-labelledby="organizational-intelligence-title" aria-live="polite">
      <PageHeader
        eyebrow={t('organizationalIntelligence.eyebrow')}
        eyebrowIcon={Building2}
        title={t('organizationalIntelligence.title')}
        titleId="organizational-intelligence-title"
        description={t('organizationalIntelligence.subtitle')}
        actions={
          <>
            <span className="status-badge">{t('moduleAvailability.status.READ_ONLY')}</span>
            <button className="button button--secondary" type="button" onClick={() => { void intelligenceQuery.refetch(); }} disabled={intelligenceQuery.isFetching}>
              <RefreshCw size={16} className={intelligenceQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
              {t('organizationalIntelligence.actions.refresh')}
            </button>
          </>
        }
      />

      {intelligenceQuery.isLoading ? <LoadingState label={t('organizationalIntelligence.loading')} /> : null}
      {intelligenceQuery.isError ? (
        <ErrorState code={errorCode} deniedMessage={t('organizationalIntelligence.permissionDenied')} />
      ) : null}

      {noData ? (
        <EmptyState icon={Building2} title={t('organizationalIntelligence.noData')} />
      ) : null}

      {intelligenceQuery.data && intelligenceQuery.data.hasData ? (
        <>
          <section className="panel" aria-labelledby="organizational-context-title">
            <div className="panel__header">
              <div><h2 id="organizational-context-title">{t('organizationalIntelligence.context.title')}</h2><span>{t(`status.${intelligenceQuery.data.dataMode}`)}</span></div>
              {intelligenceQuery.data.dataMode === 'DEMO_SYNTHETIC' ? <span className="demo-label">{t('product.demo')}</span> : <ShieldCheck size={18} aria-label={t('organizationalIntelligence.context.productionData')} />}
            </div>
            {intelligenceQuery.data.organization ? (
              <dl className="details-list">
                <div><dt>{t('organizationalIntelligence.context.legalName')}</dt><dd>{intelligenceQuery.data.organization.legalName}</dd></div>
                <div><dt>{t('organizationalIntelligence.context.country')}</dt><dd>{intelligenceQuery.data.organization.countryCode}</dd></div>
                <div><dt>{t('organizationalIntelligence.context.industry')}</dt><dd>{intelligenceQuery.data.organization.industry}</dd></div>
              </dl>
            ) : <EmptyState icon={Building2} variant="inline" title={t('organizationalIntelligence.context.unavailable')} />}
          </section>

          <section className="metric-grid" aria-label={t('organizationalIntelligence.sourceSummaryLabel')}>
            <article className="metric-card"><span>{t('organizationalIntelligence.sources.total')}</span><strong>{intelligenceQuery.data.sources.total}</strong><small>{t('organizationalIntelligence.sources.totalDescription')}</small></article>
            <article className="metric-card"><span>{t('organizationalIntelligence.sources.humanVerified')}</span><strong>{intelligenceQuery.data.sources.humanVerified}</strong><small>{t('status.HUMAN_VERIFIED')}</small></article>
            <article className="metric-card metric-card--critical"><span>{t('organizationalIntelligence.sources.requiresReview')}</span><strong>{intelligenceQuery.data.sources.requiresReview}</strong><small>{t('status.REQUIRES_REVIEW')}</small></article>
          </section>

          <section aria-label={t('organizationalIntelligence.operationsLabel')}>
            <article className="panel">
              <div className="panel__header"><div><h2>{t('organizationalIntelligence.operations.obligations')}</h2><span>{intelligenceQuery.data.operations.activeObligations}</span></div><ClipboardCheck size={18} aria-hidden="true" /></div>
              <dl className="system-list">
                <div><dt>{t('organizationalIntelligence.operations.activeObligations')}</dt><dd>{intelligenceQuery.data.operations.activeObligations}</dd></div>
                <div><dt>{t('organizationalIntelligence.operations.atRiskObligations')}</dt><dd>{intelligenceQuery.data.operations.atRiskObligations}</dd></div>
                <div><dt>{t('organizationalIntelligence.operations.overdueObligations')}</dt><dd>{intelligenceQuery.data.operations.overdueObligations}</dd></div>
                <div><dt>{t('organizationalIntelligence.operations.evidenceGaps')}</dt><dd>{intelligenceQuery.data.operations.evidenceGaps}</dd></div>
              </dl>
            </article>
            <article className="panel">
              <div className="panel__header"><div><h2>{t('organizationalIntelligence.operations.workflow')}</h2><span>{intelligenceQuery.data.operations.activeTasks}</span></div><TriangleAlert size={18} aria-hidden="true" /></div>
              <dl className="system-list">
                <div><dt>{t('organizationalIntelligence.operations.activeTasks')}</dt><dd>{intelligenceQuery.data.operations.activeTasks}</dd></div>
                <div><dt>{t('organizationalIntelligence.operations.blockedTasks')}</dt><dd>{intelligenceQuery.data.operations.blockedTasks}</dd></div>
                <div><dt>{t('organizationalIntelligence.operations.activeRisks')}</dt><dd>{intelligenceQuery.data.operations.activeRisks}</dd></div>
                <div><dt>{t('organizationalIntelligence.operations.pendingApprovals')}</dt><dd>{intelligenceQuery.data.operations.pendingApprovals}</dd></div>
              </dl>
            </article>
          </section>

          <section className="panel panel--table" aria-labelledby="organizational-priorities-title">
            <div className="panel__header"><div><h2 id="organizational-priorities-title">{t('organizationalIntelligence.priorities.title')}</h2><span>{visiblePriorities.length}</span></div><AlertTriangle size={18} aria-hidden="true" /></div>
            <div className="attention-toolbar" aria-label={t('organizationalIntelligence.priorities.filtersLabel')}>
              <div className="attention-toolbar__heading"><Filter size={16} aria-hidden="true" /><h3>{t('organizationalIntelligence.priorities.filtersTitle')}</h3></div>
              <label className="attention-search"><span className="visually-hidden">{t('actions.search')}</span><Search size={16} aria-hidden="true" /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t('organizationalIntelligence.priorities.searchPlaceholder')} /></label>
              <label><span>{t('organizationalIntelligence.priorities.kind')}</span><select value={kind} onChange={(event) => setKind(event.target.value as PriorityKindFilter)}><option value="ALL">{t('organizationalIntelligence.priorities.all')}</option>{(['OBLIGATION', 'TASK', 'RISK'] as const).map((value) => <option value={value} key={value}>{t(`organizationalIntelligence.priorityKind.${value}`)}</option>)}</select></label>
              <button className="button button--quiet" type="button" onClick={resetFilters} disabled={!query && kind === 'ALL'}>{t('organizationalIntelligence.actions.clearFilters')}</button>
            </div>
            {visiblePriorities.length > 0 ? (
              <div className="table-scroll">
                <table>
                  <thead><tr><th>{t('organizationalIntelligence.priorities.priority')}</th><th>{t('organizationalIntelligence.priorities.kind')}</th><th>{t('table.owner')}</th><th>{t('table.deadline')}</th><th>{t('table.status')}</th></tr></thead>
                  <tbody>{visiblePriorities.map((priority) => <tr key={`${priority.kind}-${priority.id}`}>
                    <td><div className="table-primary"><span>{priority.title}</span><small>{t(`organizationalIntelligence.priorityReason.${priority.reason}`)}</small></div></td>
                    <td>{t(`organizationalIntelligence.priorityKind.${priority.kind}`)}</td>
                    <td>{priority.owner ?? t('organizationalIntelligence.notAvailable')}</td>
                    <td><time dateTime={priority.dueAt ?? undefined}>{formatDate(priority.dueAt, language, t('organizationalIntelligence.notAvailable'))}</time></td>
                    <td><PriorityBadge value={priority.state} /></td>
                  </tr>)}</tbody>
                </table>
              </div>
            ) : <EmptyState icon={CheckCircle2} variant="inline" title={hasFilteredOutPriorities ? t('organizationalIntelligence.priorities.noMatches') : t('organizationalIntelligence.priorities.empty')} />}
          </section>
        </>
      ) : null}
    </section>
  );
};
