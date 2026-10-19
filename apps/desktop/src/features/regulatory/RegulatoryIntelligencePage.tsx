import { useState } from 'react';
import { CheckCircle2, Gavel, Plus, RefreshCw, ShieldAlert, ShieldCheck } from 'lucide-react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { PageHeader } from '../../components/PageHeader';
import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { nexusApi } from '../../lib/nexus-api';
import { formatDate } from '../../lib/format';
import { useSession } from '../../lib/use-session';
import { StatusBadge } from '../../app/badges';
import { CreateRegulatorySourceDialog } from './CreateRegulatorySourceDialog';

// الوحدة 04 — سجل المصادر التنظيمية المحلية.
// لا تغذية تنظيمية حية ولا مراقبة خارجية؛ ما يظهر هو المسجل محلياً بمنشأه الصادق.
// 0026: صار بإمكان المستخدم المخوّل تسجيل مرجع تنظيمي يقدّمه بنفسه، ويبقى
// غير رسمي وغير محقَّق حتى يقرر مسار التحقق البشري غير ذلك.
export const RegulatoryIntelligencePage = () => {
  const { t, i18n } = useTranslation();
  const { can } = useSession();
  const queryClient = useQueryClient();
  const language = i18n.resolvedLanguage ?? 'ar';
  const [createOpen, setCreateOpen] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const sourcesQuery = useQuery({
    queryKey: ['regulatory-sources'],
    queryFn: nexusApi.getRegulatorySources,
    staleTime: 30_000,
  });
  const errorCode = resolveErrorCode(sourcesQuery.error);
  const data = sourcesQuery.data;
  // التسجيل مسار إنتاجي: في الديمو تبقى البيانات تركيبية معلنة ولا تُضاف إليها.
  const canRegister = can('regulatory_intelligence:create') && data?.dataMode === 'PRODUCTION';

  const registerButton = canRegister ? (
    <button className="button button--primary" type="button" onClick={() => setCreateOpen(true)}>
      <Plus size={16} aria-hidden="true" />
      {t('regulatory.create.open')}
    </button>
  ) : null;

  return (
    <section id="regulatory" className="content-container" aria-labelledby="regulatory-title" aria-live="polite">
      <PageHeader
        eyebrow={t('regulatory.eyebrow')}
        eyebrowIcon={Gavel}
        title={t('regulatory.title')}
        titleId="regulatory-title"
        description={t('regulatory.subtitle')}
        actions={(
          <>
            {registerButton}
            <span className="status-badge">{t('moduleAvailability.status.READ_ONLY')}</span>
          </>
        )}
      />

      {feedback ? <p className="form-note" role="status">{feedback}</p> : null}
      {sourcesQuery.isLoading ? <LoadingState label={t('regulatory.loading')} /> : null}
      {sourcesQuery.isError ? (
        <ErrorState code={errorCode} deniedMessage={t('regulatory.permissionDenied')} />
      ) : null}

      {data && !data.hasData ? (
        <EmptyState icon={Gavel} title={t('regulatory.empty')} description={t('regulatory.emptyHint')} actions={registerButton} />
      ) : null}

      {data && data.hasData ? (
        <>
          <section className="panel" aria-labelledby="regulatory-provenance-title">
            <div className="panel__header">
              <div><h2 id="regulatory-provenance-title">{t('regulatory.provenance.title')}</h2><span>{t(`status.${data.dataMode}`)}</span></div>
              {/* درع «منشأ متحقق» لا يظهر إلا لمنشأ رسمي فعلاً: عرضه على مصدر
                  يقدّمه المستخدم (USER_PROVIDED_UNVERIFIED) ادعاء منشأ كاذب. */}
              {data.provenance === 'DEMO_SYNTHETIC' ? (
                <span className="demo-label">{t('product.demo')}</span>
              ) : data.provenance === 'OFFICIAL' ? (
                <ShieldCheck size={18} aria-label={t('regulatory.provenance.verifiedLabel')} />
              ) : (
                <ShieldAlert size={18} aria-label={t(`regulatory.provenance.${data.provenance}`)} />
              )}
            </div>
            <p className="form-note">{t(`regulatory.provenance.${data.provenance}`)}</p>
            <dl className="details-list">
              <div><dt>{t('regulatory.summary.total')}</dt><dd>{data.summary.total}</dd></div>
              <div><dt>{t('regulatory.summary.official')}</dt><dd>{data.summary.official}</dd></div>
              <div><dt>{t('regulatory.summary.humanVerified')}</dt><dd>{data.summary.humanVerified}</dd></div>
              <div><dt>{t('regulatory.summary.requiresReview')}</dt><dd>{data.summary.requiresReview}</dd></div>
              <div><dt>{t('regulatory.summary.linkedObligations')}</dt><dd>{data.summary.linkedActiveObligations}</dd></div>
            </dl>
          </section>

          <section className="panel panel--table" aria-labelledby="regulatory-sources-title">
            <div className="panel__header">
              <div><h2 id="regulatory-sources-title">{t('regulatory.sources.title')}</h2><span>{data.items.length}</span></div>
              <button
                className="button button--quiet"
                type="button"
                onClick={() => { void sourcesQuery.refetch(); }}
                disabled={sourcesQuery.isFetching}
              >
                <RefreshCw size={16} className={sourcesQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
                {t('actions.refresh')}
              </button>
            </div>
            <div className="table-scroll">
              <table>
                <thead>
                  <tr>
                    <th>{t('regulatory.table.authority')}</th>
                    <th>{t('regulatory.table.document')}</th>
                    <th>{t('regulatory.table.version')}</th>
                    <th>{t('regulatory.table.retrieved')}</th>
                    <th>{t('regulatory.table.official')}</th>
                    <th>{t('details.verification')}</th>
                    <th>{t('regulatory.table.obligations')}</th>
                  </tr>
                </thead>
                <tbody>
                  {data.items.map((source) => (
                    <tr key={source.id}>
                      <td><div className="table-primary"><span>{source.authority}</span></div></td>
                      <td><div className="table-primary"><span>{source.documentTitle}</span><small>{source.sourceUrl ?? '—'}</small></div></td>
                      <td>{source.documentVersion ?? '—'}</td>
                      <td><time dateTime={source.retrievedAt}>{formatDate(source.retrievedAt, language)}</time></td>
                      <td>{source.official ? <CheckCircle2 size={16} aria-label={t('regulatory.officialSource')} /> : <span role="img" aria-label={t('regulatory.unofficialSource')}>—</span>}</td>
                      <td><StatusBadge value={source.verificationState} /></td>
                      <td>{source.activeObligationCount}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <p className="form-note">{t('regulatory.sources.note')}</p>
          </section>
        </>
      ) : null}

      {createOpen ? (
        <CreateRegulatorySourceDialog
          onClose={() => setCreateOpen(false)}
          onCreated={() => {
            setFeedback(t('feedback.regulatorySourceCreated'));
            void queryClient.invalidateQueries({ queryKey: ['regulatory-sources'] });
          }}
        />
      ) : null}
    </section>
  );
};
