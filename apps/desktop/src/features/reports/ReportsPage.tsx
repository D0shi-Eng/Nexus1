import { useState } from 'react';
import { BarChart3, CircleAlert, Download, FileSpreadsheet, RefreshCw } from 'lucide-react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { ReportExportFormat, ReportId } from '@nexus/contracts';

import { PageHeader } from '../../components/PageHeader';
import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { chooseSavePath as defaultChooseSavePath } from '../../lib/file-dialog';
import { nexusApi } from '../../lib/nexus-api';
import { useSession } from '../../lib/use-session';

// وحدة التقارير: كانت معلنة صراحةً كغير منفذة، وكانت تُعرض بلافتة خاصة تختلف عن
// بقية الصفحات. الآن هي صفحة حقيقية بنفس ترويسة الوحدات الـ15 وحالاتها، وتبني كل
// تقرير من سجلات محلية فعلية داخل نطاق المستأجر — بلا رقم مُختلق وبلا إقرار امتثال.
//
// `chooseSavePath` يُمرَّر كخاصية اختيارية: منفذ حوار نظام التشغيل يُحقن من الخارج،
// فيبقى التعامل مع النظام في نقطة واحدة قابلة للاستبدال في الاختبار.
export const ReportsPage = ({
  chooseSavePath = defaultChooseSavePath,
}: {
  chooseSavePath?: typeof defaultChooseSavePath;
} = {}) => {
  const { t, i18n } = useTranslation();
  const { can } = useSession();
  const canRead = can('report:read');
  const canExport = can('report:export');
  const [selected, setSelected] = useState<ReportId | null>(null);
  const [notice, setNotice] = useState<{ tone: 'ok' | 'error'; text: string } | null>(null);
  const language = i18n.resolvedLanguage ?? 'ar';

  const catalogQuery = useQuery({
    queryKey: ['report-catalog'],
    queryFn: nexusApi.getReportCatalog,
    staleTime: 60_000,
    enabled: canRead,
  });

  const reportQuery = useQuery({
    queryKey: ['report', selected],
    queryFn: () => nexusApi.generateReport({ reportId: selected! }),
    enabled: canRead && Boolean(selected),
    staleTime: 0,
  });

  const exportMutation = useMutation({
    mutationFn: async (format: ReportExportFormat) => {
      // المسار يختاره المستخدم من حوار الحفظ الأصلي؛ ويتحقق الخلف منه قبل الكتابة.
      const extension = format === 'CSV' ? 'csv' : 'md';
      const targetPath = await chooseSavePath({
        defaultPath: `${selected}.${extension}`,
        filterName: format === 'CSV' ? 'CSV' : 'Markdown',
        extension,
      });
      if (!targetPath) return null;
      return nexusApi.exportReport({ reportId: selected!, format, targetPath });
    },
    onSuccess: (result) => {
      if (!result) return;
      setNotice({ tone: 'ok', text: t('reports.export.success', { file: result.fileName, bytes: result.bytes }) });
    },
    onError: () => setNotice({ tone: 'error', text: t('reports.export.failed') }),
  });
  const formatDateTime = (value: string): string => {
    try {
      return new Intl.DateTimeFormat(language, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value));
    } catch {
      return value;
    }
  };

  if (!canRead) {
    return (
      <section id="reports" className="content-container" aria-labelledby="reports-title">
        <PageHeader
          eyebrow={t('reports.eyebrow')}
          eyebrowIcon={BarChart3}
          title={t('reports.title')}
          titleId="reports-title"
          description={t('reports.subtitle')}
        />
        <ErrorState code="AUTHORIZATION_DENIED" deniedMessage={t('reports.permissionDenied')} />
      </section>
    );
  }

  const catalog = catalogQuery.data?.reports ?? [];

  return (
    <section id="reports" className="content-container" aria-labelledby="reports-title" aria-live="polite">
      <PageHeader
        eyebrow={t('reports.eyebrow')}
        eyebrowIcon={BarChart3}
        title={t('reports.title')}
        titleId="reports-title"
        description={t('reports.subtitle')}
        actions={
          <button className="button button--secondary" type="button" onClick={() => { void catalogQuery.refetch(); }} disabled={catalogQuery.isFetching}>
            <RefreshCw size={16} className={catalogQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
            {t('reports.actions.refresh')}
          </button>
        }
      />

      {notice ? (
        <div className={notice.tone === 'ok' ? 'system-note' : 'error-panel'} role={notice.tone === 'ok' ? 'status' : 'alert'}>
          {notice.tone === 'ok' ? <FileSpreadsheet size={16} aria-hidden="true" /> : <CircleAlert size={18} aria-hidden="true" />}
          <span>{notice.text}</span>
        </div>
      ) : null}

      {catalogQuery.isLoading ? <LoadingState label={t('reports.loading')} /> : null}
      {catalogQuery.isError ? (
        <ErrorState code={resolveErrorCode(catalogQuery.error)} deniedMessage={t('reports.permissionDenied')} />
      ) : null}

      {catalogQuery.data && catalog.length === 0 ? (
        <EmptyState icon={BarChart3} title={t('reports.empty')} />
      ) : null}

      {catalog.length > 0 ? (
        <section className="panel" aria-labelledby="reports-catalog-title">
          <div className="panel__header">
            <div><h2 id="reports-catalog-title">{t('reports.catalog.title')}</h2><span>{catalog.length}</span></div>
            <BarChart3 size={18} aria-hidden="true" />
          </div>
          <p className="form-note settings-note">{t('reports.catalog.hint')}</p>
          <div className="report-catalog">
            {catalog.map((entry) => (
              <button
                key={entry.reportId}
                type="button"
                className={`report-catalog__item ${selected === entry.reportId ? 'report-catalog__item--active' : ''}`}
                aria-pressed={selected === entry.reportId}
                onClick={() => { setNotice(null); setSelected(entry.reportId); }}
              >
                <strong>{t(`reports.definition.${entry.reportId}.title`, { defaultValue: entry.title })}</strong>
                <small>{t(`reports.definition.${entry.reportId}.description`, { defaultValue: entry.description })}</small>
              </button>
            ))}
          </div>
        </section>
      ) : null}

      {selected ? (
        <section className="panel panel--table" aria-labelledby="reports-result-title">
          <div className="panel__header">
            <div>
              <h2 id="reports-result-title">{t(`reports.definition.${selected}.title`, { defaultValue: selected })}</h2>
            </div>
            <div className="page-heading__actions">
              {canExport ? (
                <>
                  <button className="button button--secondary" type="button" disabled={!reportQuery.data || exportMutation.isPending} onClick={() => exportMutation.mutate('CSV')}>
                    <Download size={16} aria-hidden="true" />
                    {t('reports.actions.exportCsv')}
                  </button>
                  <button className="button button--secondary" type="button" disabled={!reportQuery.data || exportMutation.isPending} onClick={() => exportMutation.mutate('MARKDOWN')}>
                    <Download size={16} aria-hidden="true" />
                    {t('reports.actions.exportMarkdown')}
                  </button>
                </>
              ) : null}
            </div>
          </div>

          {reportQuery.isLoading ? <LoadingState label={t('reports.generating')} /> : null}
          {reportQuery.isError ? (
            <ErrorState code={resolveErrorCode(reportQuery.error)} deniedMessage={t('reports.permissionDenied')} />
          ) : null}

          {reportQuery.data ? (
            <>
              {/* المنشأ: وضع البيانات والجداول المصدر ولحظة التوليد والوسم الصريح. */}
              <dl className="details-list">
                <div><dt>{t('reports.provenance.generatedAt')}</dt><dd>{formatDateTime(reportQuery.data.provenance.generatedAt)}</dd></div>
                <div><dt>{t('reports.provenance.dataMode')}</dt><dd>{t(`status.${reportQuery.data.provenance.dataMode}`, { defaultValue: reportQuery.data.provenance.dataMode })}</dd></div>
                <div><dt>{t('reports.provenance.tables')}</dt><dd>{reportQuery.data.provenance.sourceTables.join(' · ')}</dd></div>
                <div><dt>{t('reports.provenance.disclaimerLabel')}</dt><dd>{t('reports.provenance.disclaimer', { defaultValue: reportQuery.data.provenance.disclaimer })}</dd></div>
              </dl>

              {reportQuery.data.sections.map((section) => (
                <div className="report-section" key={section.title}>
                  <div className="report-section__header">
                    <h3>{section.title}</h3>
                    <span className="status-badge">
                      {section.truncated
                        ? t('reports.section.truncated', { shown: section.rows.length, total: section.totalRows })
                        : t('reports.section.rows', { count: section.totalRows })}
                    </span>
                  </div>
                  {section.rows.length > 0 ? (
                    <div className="table-scroll">
                      <table>
                        <thead><tr>{section.columns.map((column) => <th key={column.key}>{column.label}</th>)}</tr></thead>
                        <tbody>
                          {section.rows.map((row, rowIndex) => (
                            <tr key={`${section.title}-${rowIndex}`}>
                              {row.map((cell, cellIndex) => <td key={`${section.title}-${rowIndex}-${cellIndex}`}>{cell}</td>)}
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  ) : (
                    <EmptyState icon={BarChart3} variant="inline" title={t('reports.section.empty')} />
                  )}
                </div>
              ))}
            </>
          ) : null}
        </section>
      ) : null}
    </section>
  );
};
