import { useMemo, useState } from 'react';
import { AlertTriangle, CheckCircle2, Filter, RefreshCw, Search } from 'lucide-react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { RiskRecord, RiskSeverity, RiskStatus } from '@nexus/contracts';
import { riskStatuses, severityLevels } from '@nexus/contracts';

import { PageHeader } from '../../components/PageHeader';
import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { nexusApi } from '../../lib/nexus-api';

type RiskStatusFilter = 'ALL' | RiskStatus;
type RiskSeverityFilter = 'ALL' | RiskSeverity;

const formatDate = (value: string, language: string): string =>
  new Intl.DateTimeFormat(language, { day: 'numeric', month: 'short', year: 'numeric' }).format(new Date(value));

const toStatusFilter = (value: string): RiskStatusFilter =>
  value === 'ALL' ? 'ALL' : riskStatuses.find((status) => status === value) ?? 'ALL';

const toSeverityFilter = (value: string): RiskSeverityFilter =>
  value === 'ALL' ? 'ALL' : severityLevels.find((severity) => severity === value) ?? 'ALL';

const matchesRisk = (
  risk: RiskRecord,
  query: string,
  status: RiskStatusFilter,
  severity: RiskSeverityFilter,
): boolean => {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const matchesQuery = !normalizedQuery || `${risk.title} ${risk.owner}`.toLocaleLowerCase().includes(normalizedQuery);
  const matchesStatus = status === 'ALL' || risk.status === status;
  const matchesSeverity = severity === 'ALL' || risk.severity === severity;
  return matchesQuery && matchesStatus && matchesSeverity;
};

const RiskBadge = ({ value }: { value: RiskStatus | RiskSeverity }) => {
  const { t } = useTranslation();
  return <span className={`badge badge--${value.toLowerCase()}`}>{t(`status.${value}`)}</span>;
};

export const RisksPage = () => {
  const { t, i18n } = useTranslation();
  const [query, setQuery] = useState('');
  const [status, setStatus] = useState<RiskStatusFilter>('ALL');
  const [severity, setSeverity] = useState<RiskSeverityFilter>('ALL');
  const language = i18n.resolvedLanguage ?? 'ar';
  const risksQuery = useQuery({
    queryKey: ['risks'],
    queryFn: nexusApi.getRisks,
    staleTime: 30_000,
  });
  // رمز الخطأ من المصدر المشترك — لا نسخة محلية من فحص النوع في كل صفحة.
  const errorCode = resolveErrorCode(risksQuery.error);
  const visibleRisks = useMemo(
    () => (risksQuery.data?.items ?? []).filter((risk) => matchesRisk(risk, query, status, severity)),
    [query, risksQuery.data?.items, severity, status],
  );
  const noData = Boolean(risksQuery.data && !risksQuery.data.hasData);
  const hasFilteredOutRisks = Boolean(risksQuery.data?.items.length) && visibleRisks.length === 0;

  const resetFilters = (): void => {
    setQuery('');
    setStatus('ALL');
    setSeverity('ALL');
  };

  return (
    <section id="risks" className="risks-page content-container" aria-labelledby="risks-title" aria-live="polite">
      <PageHeader
        eyebrow={t('risks.eyebrow')}
        eyebrowIcon={AlertTriangle}
        title={t('risks.title')}
        titleId="risks-title"
        description={t('risks.subtitle')}
        actions={
          <button className="button button--secondary" type="button" onClick={() => { void risksQuery.refetch(); }} disabled={risksQuery.isFetching}>
            <RefreshCw size={16} className={risksQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
            {t('risks.actions.refresh')}
          </button>
        }
      />

      {risksQuery.isLoading ? <LoadingState label={t('risks.loading')} /> : null}
      {risksQuery.isError ? (
        <ErrorState code={errorCode} deniedMessage={t('risks.permissionDenied')} />
      ) : null}

      {noData ? (
        <EmptyState icon={AlertTriangle} title={t('risks.empty')} />
      ) : null}

      {risksQuery.data && risksQuery.data.hasData ? (
        <>
          <section className="metric-grid" aria-label={t('risks.summaryLabel')}>
            <article className="metric-card"><span>{t('risks.summary.total')}</span><strong>{risksQuery.data.summary.total}</strong><small>{t(`status.${risksQuery.data.dataMode}`)}</small></article>
            <article className="metric-card metric-card--critical"><span>{t('risks.summary.open')}</span><strong>{risksQuery.data.summary.open}</strong><small>{t('status.OPEN')}</small></article>
            <article className="metric-card"><span>{t('risks.summary.mitigating')}</span><strong>{risksQuery.data.summary.mitigating}</strong><small>{t('status.MITIGATING')}</small></article>
            <article className="metric-card"><span>{t('risks.summary.accepted')}</span><strong>{risksQuery.data.summary.accepted}</strong><small>{t('status.ACCEPTED')}</small></article>
            <article className="metric-card"><span>{t('risks.summary.closed')}</span><strong>{risksQuery.data.summary.closed}</strong><small>{t('status.CLOSED')}</small></article>
          </section>

          <section className="panel panel--table" aria-labelledby="risks-list-title">
            <div className="panel__header"><div><h2 id="risks-list-title">{t('risks.list.title')}</h2><span>{visibleRisks.length}</span></div><AlertTriangle size={18} aria-hidden="true" /></div>
            <div className="attention-toolbar" aria-label={t('risks.filters.title')}>
              <div className="attention-toolbar__heading"><Filter size={16} aria-hidden="true" /><h3>{t('risks.filters.title')}</h3></div>
              <label className="attention-search"><span className="visually-hidden">{t('actions.search')}</span><Search size={16} aria-hidden="true" /><input value={query} onChange={(event) => setQuery(event.currentTarget.value)} placeholder={t('risks.searchPlaceholder')} /></label>
              <label>
                <span>{t('risks.filters.status')}</span>
                <select value={status} onChange={(event) => setStatus(toStatusFilter(event.currentTarget.value))}>
                  <option value="ALL">{t('risks.filters.all')}</option>
                  {riskStatuses.map((value) => <option value={value} key={value}>{t(`status.${value}`)}</option>)}
                </select>
              </label>
              <label>
                <span>{t('risks.filters.severity')}</span>
                <select value={severity} onChange={(event) => setSeverity(toSeverityFilter(event.currentTarget.value))}>
                  <option value="ALL">{t('risks.filters.all')}</option>
                  {severityLevels.map((value) => <option value={value} key={value}>{t(`status.${value}`)}</option>)}
                </select>
              </label>
              <button className="button button--quiet" type="button" onClick={resetFilters} disabled={!query && status === 'ALL' && severity === 'ALL'}>{t('risks.actions.clearFilters')}</button>
            </div>
            {visibleRisks.length > 0 ? (
              <div className="table-scroll">
                <table>
                  <thead><tr><th>{t('table.risk')}</th><th>{t('table.owner')}</th><th>{t('risks.table.likelihood')}</th><th>{t('risks.table.impact')}</th><th>{t('table.status')}</th><th>{t('table.updated')}</th></tr></thead>
                  <tbody>{visibleRisks.map((risk) => <tr key={risk.id}>
                    <td><div className="table-primary"><span>{risk.title}</span><small><RiskBadge value={risk.severity} /></small></div></td>
                    <td>{risk.owner}</td>
                    <td>{risk.likelihood}</td>
                    <td>{risk.impact}</td>
                    <td><RiskBadge value={risk.status} /></td>
                    <td><time dateTime={risk.updatedAt}>{formatDate(risk.updatedAt, language)}</time></td>
                  </tr>)}</tbody>
                </table>
              </div>
            ) : <EmptyState icon={CheckCircle2} variant="inline" title={hasFilteredOutRisks ? t('risks.noMatches') : t('risks.list.empty')} />}
          </section>
        </>
      ) : null}
    </section>
  );
};
