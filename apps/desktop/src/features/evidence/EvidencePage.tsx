import { useMemo, useState } from 'react';
import { CheckCircle2, FileCheck2, FilePlus2, Filter, RefreshCw, Search } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { DataClassification, EvidenceRecord, EvidenceStatus } from '@nexus/contracts';
import { dataClassifications, evidenceStatuses } from '@nexus/contracts';

import { PageHeader } from '../../components/PageHeader';
import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { nexusApi } from '../../lib/nexus-api';
import { useSession } from '../../lib/use-session';

type EvidenceStatusFilter = 'ALL' | EvidenceStatus;
type EvidenceClassificationFilter = 'ALL' | DataClassification;

const formatDate = (value: string | null, language: string): string => {
  if (!value) {
    return '—';
  }

  return new Intl.DateTimeFormat(language, { day: 'numeric', month: 'short', year: 'numeric' }).format(new Date(value));
};

const toStatusFilter = (value: string): EvidenceStatusFilter =>
  value === 'ALL' ? 'ALL' : evidenceStatuses.find((status) => status === value) ?? 'ALL';

const toClassificationFilter = (value: string): EvidenceClassificationFilter =>
  value === 'ALL' ? 'ALL' : dataClassifications.find((classification) => classification === value) ?? 'ALL';

const matchesEvidence = (
  evidence: EvidenceRecord,
  query: string,
  status: EvidenceStatusFilter,
  classification: EvidenceClassificationFilter,
): boolean => {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const matchesQuery = !normalizedQuery || `${evidence.label} ${evidence.obligationTitle}`.toLocaleLowerCase().includes(normalizedQuery);
  // التصفية بالحالة الفعلية: الدليل المنتهي يظهر تحت EXPIRED حتى لو بقيت حالته
  // المخزَّنة VALID، لأن المستخدم يقرأ واقع الدليل لا عمود قاعدة البيانات.
  const matchesStatus = status === 'ALL' || evidence.effectiveStatus === status;
  const matchesClassification = classification === 'ALL' || evidence.classification === classification;
  return matchesQuery && matchesStatus && matchesClassification;
};

const EvidenceBadge = ({ value }: { value: EvidenceStatus | DataClassification }) => {
  const { t } = useTranslation();
  return <span className={`badge badge--${value.toLowerCase()}`}>{t(`status.${value}`)}</span>;
};

// صفحة الأدلة: سجل معزول بالمستأجر + مدخل حقيقي لتسجيل مرجع دليل.
// عيب مؤكَّد أُصلح (CQ-04): الصفحة كانت تتيح القراءة والتصفية فقط، ومسار الكتابة
// الموجود فعلاً (`register_evidence_reference`) كان غير قابل للوصول من الصفحة نفسها
// فبدت الوحدة معطلة. الآن يُفتح نفس المسار المدقَّق والمصرَّح به من هذه الصفحة.
// لا تُنشأ أدلة وهمية لملء الشاشة، والفراغ يُشرح بصدق مع إجراء واضح.
export const EvidencePage = ({ onRegisterEvidence }: { onRegisterEvidence?: (obligationId?: string) => void }) => {
  const { t, i18n } = useTranslation();
  const { can } = useSession();
  const queryClient = useQueryClient();
  const [query, setQuery] = useState('');
  const [status, setStatus] = useState<EvidenceStatusFilter>('ALL');
  const [classification, setClassification] = useState<EvidenceClassificationFilter>('ALL');
  const language = i18n.resolvedLanguage ?? 'ar';
  const evidenceQuery = useQuery({
    queryKey: ['evidence'],
    queryFn: nexusApi.getEvidence,
    staleTime: 30_000,
  });
  // نقل حالة الدليل: العقد يسمح بالحالة فقط — لا تصنيف ولا هوية ولا ملكية من الواجهة.
  const updateStatusMutation = useMutation({
    mutationFn: nexusApi.updateEvidenceStatus,
    onSuccess: () => {
      void evidenceQuery.refetch();
      void queryClient.invalidateQueries({ queryKey: ['dashboard'] });
      void queryClient.invalidateQueries({ queryKey: ['attention'] });
      void queryClient.invalidateQueries({ queryKey: ['audit-trail'] });
    },
  });
  const canUpdateStatus = can('evidence:update');
  const errorCode = resolveErrorCode(evidenceQuery.error);
  const visibleEvidence = useMemo(
    () => (evidenceQuery.data?.items ?? []).filter((item) => matchesEvidence(item, query, status, classification)),
    [classification, evidenceQuery.data?.items, query, status],
  );
  const noData = Boolean(evidenceQuery.data && !evidenceQuery.data.hasData);
  const hasFilteredOutEvidence = Boolean(evidenceQuery.data?.items.length) && visibleEvidence.length === 0;
  // الخلف هو الحكم: الزر يظهر فقط لمن يملك صلاحية الإنشاء ولديه مسار متصل فعلاً.
  const canRegister = can('evidence:create') && Boolean(onRegisterEvidence);

  const resetFilters = (): void => {
    setQuery('');
    setStatus('ALL');
    setClassification('ALL');
  };

  const refreshButton = (
    <button className="button button--secondary" type="button" onClick={() => { void evidenceQuery.refetch(); }} disabled={evidenceQuery.isFetching}>
      <RefreshCw size={16} className={evidenceQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
      {t('evidence.actions.refresh')}
    </button>
  );

  const registerButton = canRegister ? (
    <button className="button button--primary" type="button" onClick={() => onRegisterEvidence?.()}>
      <FilePlus2 size={16} aria-hidden="true" />
      {t('evidence.actions.register')}
    </button>
  ) : null;

  return (
    <section id="evidence" className="evidence-page content-container" aria-labelledby="evidence-title" aria-live="polite">
      <PageHeader
        eyebrow={t('evidence.eyebrow')}
        eyebrowIcon={FileCheck2}
        title={t('evidence.title')}
        titleId="evidence-title"
        description={t('evidence.subtitle')}
        actions={<>{registerButton}{refreshButton}</>}
      />

      {evidenceQuery.isLoading ? <LoadingState label={t('evidence.loading')} /> : null}
      {evidenceQuery.isError ? (
        <ErrorState code={errorCode} deniedMessage={t('evidence.permissionDenied')} />
      ) : null}

      {noData ? (
        <EmptyState
          icon={FileCheck2}
          title={t('evidence.empty')}
          description={t('evidence.emptyHint')}
          actions={registerButton}
        />
      ) : null}

      {evidenceQuery.data && evidenceQuery.data.hasData ? (
        <>
          {updateStatusMutation.isError ? (
            <p className="form-error" role="alert">{t(`errors.${resolveErrorCode(updateStatusMutation.error)}`, { defaultValue: t('evidence.updateFailed') })}</p>
          ) : null}
          <section className="metric-grid" aria-label={t('evidence.summaryLabel')}>
            <article className="metric-card"><span>{t('evidence.summary.total')}</span><strong>{evidenceQuery.data.summary.total}</strong><small>{t(`status.${evidenceQuery.data.dataMode}`)}</small></article>
            <article className="metric-card"><span>{t('evidence.summary.requiresReview')}</span><strong>{evidenceQuery.data.summary.requiresReview}</strong><small>{t('status.REQUIRES_REVIEW')}</small></article>
            <article className="metric-card"><span>{t('evidence.summary.expired')}</span><strong>{evidenceQuery.data.summary.expired}</strong><small>{t('status.EXPIRED')}</small></article>
          </section>

          <section className="panel panel--table" aria-labelledby="evidence-list-title">
            <div className="panel__header"><div><h2 id="evidence-list-title">{t('evidence.list.title')}</h2><span>{visibleEvidence.length}</span></div><FileCheck2 size={18} aria-hidden="true" /></div>
            <div className="attention-toolbar" aria-label={t('evidence.filters.title')}>
              <div className="attention-toolbar__heading"><Filter size={16} aria-hidden="true" /><h3>{t('evidence.filters.title')}</h3></div>
              <label className="attention-search"><span className="visually-hidden">{t('actions.search')}</span><Search size={16} aria-hidden="true" /><input value={query} onChange={(event) => setQuery(event.currentTarget.value)} placeholder={t('evidence.searchPlaceholder')} /></label>
              <label>
                <span>{t('evidence.filters.status')}</span>
                <select value={status} onChange={(event) => setStatus(toStatusFilter(event.currentTarget.value))}>
                  <option value="ALL">{t('evidence.filters.all')}</option>
                  {evidenceStatuses.map((value) => <option value={value} key={value}>{t(`status.${value}`)}</option>)}
                </select>
              </label>
              <label>
                <span>{t('evidence.filters.classification')}</span>
                <select value={classification} onChange={(event) => setClassification(toClassificationFilter(event.currentTarget.value))}>
                  <option value="ALL">{t('evidence.filters.all')}</option>
                  {dataClassifications.map((value) => <option value={value} key={value}>{t(`status.${value}`)}</option>)}
                </select>
              </label>
              <button className="button button--quiet" type="button" onClick={resetFilters} disabled={!query && status === 'ALL' && classification === 'ALL'}>{t('evidence.actions.clearFilters')}</button>
            </div>
            {visibleEvidence.length > 0 ? (
              <div className="table-scroll">
                <table>
                  <thead><tr><th>{t('evidence.table.label')}</th><th>{t('evidence.table.obligation')}</th><th>{t('evidence.table.classification')}</th><th>{t('table.status')}</th><th>{t('evidence.table.expiry')}</th><th>{t('table.updated')}</th></tr></thead>
                  <tbody>{visibleEvidence.map((item) => <tr key={item.id}>
                    <td><div className="table-primary"><span>{item.label}</span><small>{t('evidence.table.created')}: <time dateTime={item.createdAt}>{formatDate(item.createdAt, language)}</time></small><small>{item.registeredByState === 'UNKNOWN_LEGACY' ? t('evidence.legacyUnknown') : `${t('evidence.registeredBy')}: ${item.registeredBy ?? '—'}`}</small></div></td>
                    <td>{item.obligationTitle}</td>
                    <td><EvidenceBadge value={item.classification} /></td>
                    <td>
                      {canUpdateStatus ? (
                        <div className="table-primary">
                          <select
                            className="table-status-select"
                            aria-label={t('evidence.table.statusChange', { label: item.label })}
                            value={item.status}
                            disabled={updateStatusMutation.isPending}
                            onChange={(event) => updateStatusMutation.mutate({
                              evidenceId: item.id,
                              status: event.currentTarget.value as EvidenceStatus,
                            })}
                          >
                            {evidenceStatuses.map((value) => <option key={value} value={value}>{t(`status.${value}`)}</option>)}
                          </select>
                          {item.isExpired ? <small>{t('evidence.expiredNote')}</small> : null}
                        </div>
                      ) : <EvidenceBadge value={item.effectiveStatus} />}
                    </td>
                    <td><time dateTime={item.expiresAt ?? undefined}>{formatDate(item.expiresAt, language)}</time></td>
                    <td><time dateTime={item.updatedAt}>{formatDate(item.updatedAt, language)}</time></td>
                  </tr>)}</tbody>
                </table>
              </div>
            ) : (
              <EmptyState
                icon={CheckCircle2}
                variant="inline"
                title={hasFilteredOutEvidence ? t('evidence.noMatches') : t('evidence.list.empty')}
              />
            )}
          </section>
        </>
      ) : null}
    </section>
  );
};
