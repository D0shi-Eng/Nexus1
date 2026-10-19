import { AlertTriangle, CheckCircle2, ClipboardCheck, FileCheck2, LockKeyhole, Sparkles } from 'lucide-react';
import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { AttentionItem, DashboardData, ObligationSummary } from '@nexus/contracts';

import { PageHeader } from '../../components/PageHeader';
import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { nexusApi } from '../../lib/nexus-api';
import { formatDate, severityRank } from '../../lib/format';
import { useSession } from '../../lib/use-session';
import { SeverityBadge, StatusBadge } from '../../app/badges';

const AttentionRow = ({ item, language, onReview }: { item: AttentionItem; language: string; onReview: (item: AttentionItem) => void }) => {
  const { t } = useTranslation();
  return (
    <li className="attention-row">
      <div className="attention-row__marker" aria-hidden="true" />
      <div className="attention-row__content">
        <div className="attention-row__topline">
          <SeverityBadge severity={item.severity} />
          {item.dueAt ? <time dateTime={item.dueAt}>{formatDate(item.dueAt, language)}</time> : null}
        </div>
        <strong>{item.title}</strong>
        <p>{t(`attention.reason.${item.reasonCode}`)}</p>
      </div>
      <button className="button button--quiet" type="button" onClick={() => onReview(item)}>
        {t('actions.review')}
      </button>
    </li>
  );
};

const ObligationTable = ({ obligations, language, onOpen }: { obligations: readonly ObligationSummary[]; language: string; onOpen: (obligation: ObligationSummary) => void }) => {
  const { t } = useTranslation();
  return (
    <div className="table-scroll">
      <table>
        <thead>
          <tr>
            <th>{t('table.obligation')}</th>
            <th>{t('table.owner')}</th>
            <th>{t('table.deadline')}</th>
            <th>{t('table.evidence')}</th>
            <th>{t('table.status')}</th>
            <th><span className="visually-hidden">{t('table.action')}</span></th>
          </tr>
        </thead>
        <tbody>
          {obligations.map((obligation) => (
            <tr key={obligation.id}>
              <td>
                <div className="table-primary">
                  <span>{obligation.title}</span>
                  <small>{obligation.authority}</small>
                </div>
              </td>
              <td>{obligation.owner}</td>
              <td><time dateTime={obligation.deadline ?? undefined}>{formatDate(obligation.deadline, language)}</time></td>
              <td><StatusBadge value={obligation.evidenceStatus} /></td>
              <td><StatusBadge value={obligation.status} /></td>
              <td><button className="button button--quiet" type="button" onClick={() => onOpen(obligation)}>{t('actions.open')}</button></td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
};

export const OverviewPage = ({
  onOpenAttention,
  onOpenObligation,
  onOpenTaskFor,
  onOpenEvidenceFor,
}: {
  onOpenAttention: (item: AttentionItem) => void;
  onOpenObligation: (obligation: ObligationSummary) => void;
  onOpenTaskFor: () => void;
  onOpenEvidenceFor: () => void;
}) => {
  const { t, i18n } = useTranslation();
  const { can } = useSession();
  const language = i18n.resolvedLanguage ?? 'ar';
  const dashboardQuery = useQuery({
    queryKey: ['dashboard'],
    queryFn: nexusApi.getDashboard,
    staleTime: 30_000,
  });
  const healthQuery = useQuery({
    queryKey: ['system-health'],
    queryFn: nexusApi.getHealth,
    staleTime: 30_000,
  });
  const dashboard: DashboardData | undefined = dashboardQuery.data;
  // دالة عرض واحدة لحالات الصحة: «غير متاح» تُحصر في الحالة السلبية الفعلية،
  // وعرض حالة سلبية أثناء التحميل أو لحالة سليمة ادعاء خاطئ.
  const describeHealth = (state: string | undefined, isLoading: boolean, notConfiguredKey?: string): string => {
    if (isLoading) {
      return t('system.checking');
    }
    if (state === 'HEALTHY') {
      return t('system.healthy');
    }
    if (state === 'DEGRADED') {
      return t('system.degraded');
    }
    if (state === 'NOT_CONFIGURED' && notConfiguredKey) {
      return t(notConfiguredKey);
    }
    return t('system.unavailable');
  };
  const databaseHealthLabel = describeHealth(healthQuery.data?.database, healthQuery.isLoading);
  const aiHealthLabel = describeHealth(healthQuery.data?.ai, healthQuery.isLoading, 'system.aiNotConfigured');
  const attentionItems = useMemo(
    () => [...(dashboard?.attentionItems ?? [])].sort((left, right) => severityRank[left.severity] - severityRank[right.severity]),
    [dashboard?.attentionItems],
  );
  const canCreateTask = can('task:create');
  const canRegisterEvidence = can('evidence:create');

  return (
    <section id="overview" className="content-container" aria-labelledby="overview-title" aria-live="polite">
      <PageHeader
        eyebrow={t('dashboard.eyebrow')}
        eyebrowIcon={Sparkles}
        title={t('dashboard.title')}
        titleId="overview-title"
        description={t('dashboard.subtitle')}
        actions={
          <>
            {canRegisterEvidence ? (
              <button className="button button--secondary" type="button" onClick={onOpenEvidenceFor}><FileCheck2 size={16} aria-hidden="true" /> {t('actions.registerEvidence')}</button>
            ) : null}
            {canCreateTask ? (
              <button className="button button--primary" type="button" onClick={onOpenTaskFor}><ClipboardCheck size={16} aria-hidden="true" /> {t('actions.createTask')}</button>
            ) : null}
          </>
        }
      />

      {dashboardQuery.isError ? (
        // الرسالة الحالية محفوظة عبر `fallbackMessage` حتى لا يتغير نص الخطأ المعروض للمستخدم.
        <ErrorState
          code={resolveErrorCode(dashboardQuery.error)}
          deniedMessage={t('errors.unavailable')}
          fallbackMessage={t('errors.loadingDashboard')}
        />
      ) : null}

      {dashboard ? (
        <>
          {/* لا نسبة عند غياب بيانات كافية: عرض «0%» كان ادعاءً كاذباً بأن
              القياس جرى وفشل، والحقيقة أن القياس غير ممكن. */}
          <section className="metric-grid" aria-label={t('dashboard.readiness')}>
            <article className="readiness-card metric-card metric-card--featured">
              <div className="metric-card__heading">
                <span>{t('dashboard.readiness')}</span>
                <span className="status-badge">
                  {dashboard.readiness.verifiedCoverage === null
                    ? t('dashboard.notMeasured')
                    : `${dashboard.readiness.verifiedCoverage}%`}
                </span>
              </div>
              <div className="readiness-card__body">
                {dashboard.readiness.readinessScore === null ? (
                  <>
                    <div className="readiness-ring readiness-ring--unknown" aria-hidden="true"><span>—</span></div>
                    <p>{t('dashboard.insufficientData')}</p>
                  </>
                ) : (
                  <>
                    <div className="readiness-ring" style={{ '--readiness': `${dashboard.readiness.readinessScore}%` } as React.CSSProperties}>
                      <span>{dashboard.readiness.readinessScore}%</span>
                    </div>
                    <p>{t('dashboard.readinessDescription')}</p>
                  </>
                )}
              </div>
              {/* المنهجية معلنة بجانب الرقم: المقام والمصدر وحدود الدلالة. */}
              <p className="metric-card__note">
                {t('dashboard.readinessMethod', { count: dashboard.readiness.eligibleObligations })}
              </p>
              {dashboard.readiness.archivedObligations > 0 ? (
                <p className="metric-card__note">
                  {t('dashboard.archivedExcluded', { count: dashboard.readiness.archivedObligations })}
                </p>
              ) : null}
            </article>
            <article className="metric-card">
              <span>{t('dashboard.verifiedCoverage')}</span>
              <strong>{dashboard.readiness.verifiedCoverage === null ? '—' : `${dashboard.readiness.verifiedCoverage}%`}</strong>
              <small><CheckCircle2 size={14} aria-hidden="true" /> {t('status.HUMAN_VERIFIED')}</small>
            </article>
            <article className="metric-card">
              <span>{t('dashboard.evidenceCompleteness')}</span>
              <strong>{dashboard.readiness.evidenceCompleteness === null ? '—' : `${dashboard.readiness.evidenceCompleteness}%`}</strong>
              <small><FileCheck2 size={14} aria-hidden="true" /> {t('status.VALID')}</small>
            </article>
            <article className="metric-card metric-card--critical">
              <span>{t('dashboard.criticalOpen')}</span>
              <strong>{dashboard.readiness.openCriticalItems}</strong>
              <small><AlertTriangle size={14} aria-hidden="true" /> {t('status.CRITICAL')}</small>
            </article>
          </section>

          <section className="content-grid">
            <article id="attention" className="panel panel--attention">
              <div className="panel__header">
                <div><h2>{t('dashboard.attentionQueue')}</h2><span>{attentionItems.length}</span></div>
              </div>
              {attentionItems.length > 0 ? (
                <ol className="attention-list">
                  {attentionItems.map((item) => <AttentionRow item={item} language={language} onReview={onOpenAttention} key={item.id} />)}
                </ol>
              ) : <EmptyState icon={ClipboardCheck} variant="inline" title={t('empty.noItems')} />}
            </article>
            <article className="panel panel--system">
              <div className="panel__header"><div><h2>{t('system.health')}</h2><span>{t('system.localMode')}</span></div><LockKeyhole size={18} aria-hidden="true" /></div>
              <dl className="system-list">
                <div><dt>{t('system.database')}</dt><dd><span className={`health-dot health-dot--${healthQuery.data?.database === 'HEALTHY' ? 'healthy' : 'unknown'}`} aria-hidden="true" />{databaseHealthLabel}</dd></div>
                <div><dt>{t('system.ai')}</dt><dd><span className={`health-dot health-dot--${healthQuery.data?.ai === 'HEALTHY' ? 'healthy' : 'unknown'}`} aria-hidden="true" />{aiHealthLabel}</dd></div>
              </dl>
              <div className="system-note"><LockKeyhole size={16} aria-hidden="true" /><span>{dashboard.dataMode === 'DEMO_SYNTHETIC' ? t('dashboard.sourceNoteDemo') : t('dashboard.sourceNote')}</span></div>
            </article>
          </section>

          <section id="obligations" className="panel panel--table">
            <div className="panel__header">
              <div><h2>{t('dashboard.obligations')}</h2><span>{dashboard.obligations.length}</span></div>
            </div>
            {dashboard.obligations.length > 0 ? (
              <ObligationTable obligations={dashboard.obligations} language={language} onOpen={onOpenObligation} />
            ) : <EmptyState icon={ClipboardCheck} variant="inline" title={t('empty.noObligations')} />}
          </section>

          <section className="panel panel--audit">
            <div className="panel__header">
              <div><h2>{t('dashboard.recentAudit')}</h2><span>{dashboard.recentAuditEvents.length}</span></div>
              {/* سلامة السلسلة معروضة من نتيجة التحقق الفعلي، لا من قيمة ثابتة. */}
              <span className={`status-badge ${dashboard.auditChainIntegrity === 'VALID' ? '' : 'status-badge--critical'}`}>
                {dashboard.auditChainIntegrity === 'VALID' ? t('dashboard.chainValid') : t('dashboard.chainInvalid')}
              </span>
            </div>
            {dashboard.recentAuditEvents.length > 0 ? (
              <ol className="audit-list">
                {dashboard.recentAuditEvents.map((event) => (
                  <li key={event.id}>
                    <span className="audit-list__icon"><FileCheck2 size={16} aria-hidden="true" /></span>
                    <div><strong>{t(`audit.event.${event.eventType}`, { defaultValue: event.eventType })}</strong><span>{event.actorName} · {event.resourceType}</span></div>
                    <time dateTime={event.occurredAt}>{formatDate(event.occurredAt, language)}</time>
                  </li>
                ))}
              </ol>
            ) : <EmptyState icon={FileCheck2} variant="inline" title={t('empty.noAuditEvents')} />}
          </section>
        </>
      ) : !dashboardQuery.isError ? <LoadingState label={t('dashboard.loading')} /> : null}
    </section>
  );
};
