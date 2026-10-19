import { FileSearch, RefreshCw } from 'lucide-react';
import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { AuditTrailCursor } from '@nexus/contracts';

import { PageHeader } from '../../components/PageHeader';
import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { nexusApi } from '../../lib/nexus-api';
import { formatDate } from '../../lib/format';

// الوحدة 12 — عارض سجل التدقيق: قراءة فقط، صفحات محدودة بترقيم مفتاحي،
// وسلامة السلسلة فعلية من الخلف. لا تحرير ولا حذف — السجل غير قابل للتعديل بنيوياً.
export const AuditTrailPage = () => {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage ?? 'ar';
  const [cursor, setCursor] = useState<AuditTrailCursor | null>(null);
  const trailQuery = useQuery({
    queryKey: ['audit-trail', cursor],
    queryFn: () => nexusApi.getAuditTrail({ before: cursor }),
    staleTime: 30_000,
  });
  const errorCode = resolveErrorCode(trailQuery.error);
  const data = trailQuery.data;
  const integrityLabel = data ? t(`audit.integrity.${data.chainIntegrity}`) : null;
  // حالة المرساة تُعرض منفصلة عن سلامة السلسلة: اقتطاع الذيل قد يحدث مع سلسلة
  // داخلية متماسكة، ولا يجوز أن يبقى العرض «سليم» حينها.
  const anchorConsistent = !data || (data.chainAnchorState !== 'TRUNCATED' && data.chainAnchorState !== 'PREFIX_MISMATCH');

  return (
    <section id="audit" className="content-container" aria-labelledby="audit-title" aria-live="polite">
      <PageHeader
        eyebrow={t('audit.eyebrow')}
        eyebrowIcon={FileSearch}
        title={t('audit.title')}
        titleId="audit-title"
        description={t('audit.subtitle')}
        actions={
          <>
            {integrityLabel ? (
              <span
                className={`status-badge ${data?.chainIntegrity === 'INVALID' ? 'badge--critical' : ''}`}
                aria-label={t('audit.integrity.label')}
              >
                {integrityLabel}
              </span>
            ) : null}
            {data && !anchorConsistent ? (
              <span className="status-badge badge--critical" role="alert">
                {t(`audit.anchor.${data.chainAnchorState}`)}
              </span>
            ) : null}
            <span className="status-badge">{t('moduleAvailability.status.READ_ONLY')}</span>
          </>
        }
      />

      {trailQuery.isLoading ? <LoadingState label={t('audit.loading')} /> : null}
      {trailQuery.isError ? (
        <ErrorState code={errorCode} deniedMessage={t('audit.permissionDenied')} />
      ) : null}

      {data && data.items.length === 0 ? (
        <EmptyState icon={FileSearch} title={t('audit.empty')} />
      ) : null}

      {data && data.items.length > 0 ? (
        <section className="panel panel--table" aria-labelledby="audit-events-title">
          <div className="panel__header">
            <div><h2 id="audit-events-title">{t('audit.events.title')}</h2><span>{data.items.length}</span></div>
            <button
              className="button button--quiet"
              type="button"
              onClick={() => {
                setCursor(null);
                void trailQuery.refetch();
              }}
              disabled={trailQuery.isFetching}
            >
              <RefreshCw size={16} className={trailQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
              {t('actions.refresh')}
            </button>
          </div>
          <div className="table-scroll">
            <table>
              <thead>
                <tr>
                  <th>{t('audit.table.time')}</th>
                  <th>{t('audit.table.event')}</th>
                  <th>{t('audit.table.actor')}</th>
                  <th>{t('audit.table.resource')}</th>
                </tr>
              </thead>
              <tbody>
                {data.items.map((event) => (
                  <tr key={event.id}>
                    <td><time dateTime={event.occurredAt}>{formatDate(event.occurredAt, language)}</time></td>
                    <td>{t(`audit.event.${event.eventType}`, { defaultValue: event.eventType })}</td>
                    <td>{event.actorName}</td>
                    <td><div className="table-primary"><span>{event.resourceType}</span><small>{event.resourceId}</small></div></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {data.hasMore && data.nextCursor ? (
            <div className="panel__footer">
              <button
                className="button button--secondary"
                type="button"
                onClick={() => setCursor(data.nextCursor ?? null)}
                disabled={trailQuery.isFetching}
              >
                {t('audit.loadMore')}
              </button>
            </div>
          ) : <p className="form-note">{t('audit.endOfTrail')}</p>}
          <p className="form-note">{t('audit.readOnlyNote')}</p>
        </section>
      ) : null}
    </section>
  );
};
