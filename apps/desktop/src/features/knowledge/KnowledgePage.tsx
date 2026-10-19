import { useState, type FormEvent } from 'react';
import { BookOpen, CheckCircle2, CircleAlert, LoaderCircle, RefreshCw, Search, X } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { KnowledgeSourceView } from '@nexus/contracts';

import { PageHeader } from '../../components/PageHeader';
import { ConfirmDialog } from '../../components/ConfirmDialog';
import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { useModalFocus } from '../../lib/use-modal-focus';

const formatDate = (value: string | null, language: string): string => {
  if (!value) {
    return '—';
  }
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime())
    ? '—'
    : new Intl.DateTimeFormat(language, { day: 'numeric', month: 'short', year: 'numeric' }).format(parsed);
};

const IngestDialog = ({ onClose }: { readonly onClose: () => void }) => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const [title, setTitle] = useState('');
  const [origin, setOrigin] = useState('');
  const [text, setText] = useState('');
  const [filenameHint, setFilenameHint] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [validationError, setValidationError] = useState(false);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const ingestMutation = useMutation({ mutationFn: nexusApi.ingestKnowledgeSource });

  const onFile = (file: File | undefined): void => {
    if (!file) {
      return;
    }
    setFilenameHint(file.name);
    void file.text().then((content) => {
      setText(content);
    }).catch(() => {
      setFilenameHint(null);
      setErrorCode('KNOWLEDGE_EXTRACTION_FAILED');
    });
  };

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setValidationError(false);
    setErrorCode(null);
    const trimmed = text.trim();
    if (!title.trim() || title.length > 200 || !origin.trim() || origin.length > 200
      || trimmed.length < 20 || trimmed.length > 64_000) {
      setValidationError(true);
      return;
    }
    try {
      const result = await ingestMutation.mutateAsync({
        title,
        origin,
        text: trimmed,
        filenameHint,
      });
      await queryClient.invalidateQueries({ queryKey: ['knowledge-sources'] });
      await queryClient.invalidateQueries({ queryKey: ['memory'] });
      setFeedback(result.duplicate ? t('knowledge.ingestDuplicate') : t('knowledge.ingestSuccess'));
      setTitle('');
      setOrigin('');
      setText('');
      setFilenameHint(null);
    } catch (error) {
      setErrorCode(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="knowledge-ingest-title" tabIndex={-1}>
        <header className="task-dialog__header">
          <div>
            <h2 id="knowledge-ingest-title">{t('knowledge.ingestTitle')}</h2>
            <p>{t('knowledge.ingestSubtitle')}</p>
          </div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}>
            <X size={18} aria-hidden="true" />
          </button>
        </header>
        <form className="task-form" onSubmit={(event) => { void submit(event); }} noValidate>
          <label>
            <span>{t('knowledge.ingestTitleField')}</span>
            <input value={title} onChange={(event) => setTitle(event.currentTarget.value)} maxLength={200} required aria-invalid={validationError && !title.trim()} />
          </label>
          <label>
            <span>{t('knowledge.ingestOriginField')}</span>
            <input value={origin} onChange={(event) => setOrigin(event.currentTarget.value)} maxLength={200} required aria-invalid={validationError && !origin.trim()} />
          </label>
          <label>
            <span>{t('knowledge.ingestFileField')}</span>
            <input type="file" accept=".txt,.md,.markdown,text/plain,text/markdown" onChange={(event) => onFile(event.currentTarget.files?.[0])} />
          </label>
          {filenameHint ? <p className="form-note">{filenameHint}</p> : null}
          <label>
            <span>{t('knowledge.ingestTextField')}</span>
            <textarea value={text} onChange={(event) => setText(event.currentTarget.value)} rows={9} required aria-invalid={validationError && text.trim().length < 20} />
          </label>
          {validationError ? <p className="form-error" role="alert">{t('knowledge.ingestValidation')}</p> : null}
          {errorCode ? <p className="form-error" role="alert">{t(`errors.${errorCode}`, { defaultValue: t('errors.unavailable') })}</p> : null}
          {feedback ? <p className="operation-feedback" role="status">{feedback}</p> : null}
          <footer className="task-form__actions">
            <button className="button button--secondary" type="button" onClick={onClose} disabled={ingestMutation.isPending}>{t('actions.close')}</button>
            <button className="button button--primary" type="submit" disabled={ingestMutation.isPending}>
              {ingestMutation.isPending ? <LoaderCircle className="spin" size={16} aria-hidden="true" /> : <CheckCircle2 size={16} aria-hidden="true" />}
              {t('knowledge.ingestSubmit')}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
};

export const KnowledgePage = () => {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const language = i18n.resolvedLanguage ?? 'ar';
  const [ingestOpen, setIngestOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [searchExecuted, setSearchExecuted] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [mutationError, setMutationError] = useState<string | null>(null);
  const [pendingDisable, setPendingDisable] = useState<{ readonly sourceId: string; readonly title: string } | null>(null);

  const sourcesQuery = useQuery({ queryKey: ['knowledge-sources'], queryFn: nexusApi.getKnowledgeSources, staleTime: 30_000 });
  const memoryQuery = useQuery({ queryKey: ['memory'], queryFn: nexusApi.getMemory, staleTime: 30_000 });
  const searchMutation = useMutation({ mutationFn: nexusApi.searchKnowledge });
  const disableMutation = useMutation({ mutationFn: nexusApi.disableKnowledgeSource });

  // رمز الخطأ من المصدر المشترك للقراءة؛ أخطاء الطفرات تبقى محلية لأنها تُقرأ من `mutationError`.
  const errorCode = resolveErrorCode(sourcesQuery.error);
  const sources = sourcesQuery.data?.items ?? [];

  const disableSource = async (source: KnowledgeSourceView): Promise<void> => {
    setFeedback(null);
    setMutationError(null);
    // تأكيد داخلي بدل window.confirm (غير منفَّذ في WebView2/Tauri).
    setPendingDisable({
      sourceId: source.id,
      title: t('knowledge.disableTitle'),
    });
  };

  const confirmDisableSource = async (sourceId: string): Promise<void> => {
    setPendingDisable(null);
    try {
      await disableMutation.mutateAsync({ sourceId });
      await queryClient.invalidateQueries({ queryKey: ['knowledge-sources'] });
      setFeedback(t('knowledge.disabledSuccess'));
    } catch (error) {
      setMutationError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  const runSearch = async (): Promise<void> => {
    const trimmed = query.trim();
    if (!trimmed) {
      return;
    }
    setSearchExecuted(trimmed);
    await searchMutation.mutateAsync({ query: trimmed });
  };

  return (
    <section id="knowledge" className="tasks-page content-container" aria-labelledby="knowledge-title" aria-live="polite">
      <PageHeader
        eyebrow={t('knowledge.eyebrow')}
        eyebrowIcon={BookOpen}
        title={t('knowledge.title')}
        titleId="knowledge-title"
        description={t('knowledge.subtitle')}
        actions={
          <>
            <button className="button button--secondary" type="button" onClick={() => { void sourcesQuery.refetch(); void memoryQuery.refetch(); }} disabled={sourcesQuery.isFetching}>
              <RefreshCw size={16} className={sourcesQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
              {t('plans.refresh')}
            </button>
            <button className="button button--primary" type="button" onClick={() => setIngestOpen(true)}>
              <CheckCircle2 size={16} aria-hidden="true" />
              {t('knowledge.ingest')}
            </button>
          </>
        }
      />

      {feedback ? <p className="operation-feedback" role="status">{feedback}</p> : null}
      {mutationError ? <div className="error-panel" role="alert"><CircleAlert size={20} aria-hidden="true" /><p>{t(`errors.${mutationError}`, { defaultValue: t('errors.unavailable') })}</p></div> : null}
      {sourcesQuery.isLoading ? <LoadingState label={t('knowledge.loading')} /> : null}
      {sourcesQuery.isError ? <ErrorState code={errorCode} deniedMessage={t('knowledge.permissionDenied')} /> : null}

      <section className="panel panel--table" aria-labelledby="knowledge-list-title">
        <div className="panel__header"><div><h2 id="knowledge-list-title">{t('knowledge.listTitle')}</h2><span>{sources.length}</span></div><BookOpen size={18} aria-hidden="true" /></div>
        {sources.length > 0 ? (
          <div className="table-scroll">
            <table>
              <thead><tr><th>{t('knowledge.table.title')}</th><th>{t('knowledge.table.provenance')}</th><th>{t('knowledge.table.status')}</th><th>{t('knowledge.table.chunks')}</th><th>{t('knowledge.table.ingested')}</th><th>{t('knowledge.table.action')}</th></tr></thead>
              <tbody>{sources.map((source) => <tr key={source.id}>
                <td><div className="table-primary"><span>{source.title}</span><small>{source.origin}</small></div></td>
                <td>{t(`knowledge.provenanceLabel.${source.provenance}`, { defaultValue: source.provenance })}</td>
                {/* حالة المصدر رمز من الخلف: عرضها خاماً يجعل الصف العربي إنجليزياً جزئياً. */}
                <td><span className={`badge badge--${source.status.toLowerCase()}`}>{t(`knowledge.status.${source.status}`, { defaultValue: source.status })}</span></td>
                <td>{source.chunkCount}</td>
                <td><time dateTime={source.ingestedAt ?? undefined}>{formatDate(source.ingestedAt, language)}</time></td>
                <td>
                  <span className="table-actions">
                    {source.status === 'ACTIVE' ? (
                      <button className="button button--quiet" type="button" onClick={() => { void disableSource(source); }} disabled={disableMutation.isPending}>
                        {t('knowledge.disable')}
                      </button>
                    ) : null}
                  </span>
                </td>
              </tr>)}</tbody>
            </table>
          </div>
        ) : <EmptyState icon={BookOpen} title={t('knowledge.empty')} />}
      </section>

      <section className="panel" aria-labelledby="knowledge-search-title">
        <div className="panel__header"><div><h2 id="knowledge-search-title">{t('knowledge.searchResultsTitle')}</h2></div><Search size={18} aria-hidden="true" /></div>
        <div className="attention-toolbar" aria-label={t('knowledge.searchLabel')}>
          <label className="attention-search">
            <span className="visually-hidden">{t('knowledge.searchLabel')}</span>
            <Search size={16} aria-hidden="true" />
            <input type="search" value={query} onChange={(event) => setQuery(event.currentTarget.value)} placeholder={t('knowledge.searchPlaceholder')} />
          </label>
          <button className="button button--primary" type="button" onClick={() => { void runSearch(); }} disabled={searchMutation.isPending || !query.trim()}>
            {searchMutation.isPending ? <LoaderCircle className="spin" size={16} aria-hidden="true" /> : <Search size={16} aria-hidden="true" />}
            {t('knowledge.searchAction')}
          </button>
        </div>
        {searchMutation.isError ? (
          <div className="error-panel" role="alert"><CircleAlert size={20} aria-hidden="true" /><p>{t(`errors.${searchMutation.error instanceof NexusApiError ? searchMutation.error.code : 'LOCAL_SERVICE_UNAVAILABLE'}`, { defaultValue: t('errors.unavailable') })}</p></div>
        ) : null}
        {searchMutation.data && searchExecuted ? (
          searchMutation.data.items.length > 0 ? (
            <ul className="knowledge-results">
              {searchMutation.data.items.map((item) => (
                <li key={item.chunkId} className="knowledge-result">
                  <div className="table-primary">
                    <span>{t('knowledge.searchResultsTitle')} · CIT-{item.rank + 1}</span>
                    <small>{item.sourceTitle} — {t(`knowledge.provenanceLabel.${item.provenance}`, { defaultValue: item.provenance })}</small>
                  </div>
                  <p>{item.content}</p>
                </li>
              ))}
            </ul>
          ) : <EmptyState icon={Search} variant="inline" title={t('knowledge.searchEmpty')} />
        ) : null}
        <p className="form-note">{t('knowledge.truthNote')}</p>
      </section>

      <section className="panel" aria-labelledby="memory-list-title">
        <div className="panel__header"><div><h2 id="memory-list-title">{t('memory.title')}</h2></div><BookOpen size={18} aria-hidden="true" /></div>
        <p className="form-note">{t('memory.subtitle')}</p>
        {memoryQuery.data && memoryQuery.data.summary.length > 0 ? (
          <section className="metric-grid" aria-label={t('memory.summaryLabel')}>
            {memoryQuery.data.summary.map((entry) => (
              <article className="metric-card" key={`${entry.memoryType}-${entry.status}`}>
                <span>{t(`memory.type.${entry.memoryType}`, { defaultValue: entry.memoryType })} · {t(`memory.status.${entry.status}`, { defaultValue: entry.status })}</span>
                <strong>{entry.count}</strong>
              </article>
            ))}
          </section>
        ) : <EmptyState icon={BookOpen} variant="inline" title={t('memory.empty')} />}
        {memoryQuery.data && memoryQuery.data.items.length > 0 ? (
          <div className="table-scroll">
            <table>
              <thead><tr><th>{t('memory.table.type')}</th><th>{t('memory.scopeLabel')}</th><th>{t('memory.table.status')}</th><th>{t('memory.table.truth')}</th><th>{t('memory.table.expires')}</th></tr></thead>
              <tbody>{memoryQuery.data.items.slice(0, 20).map((item) => <tr key={item.id}>
                <td>{t(`memory.type.${item.memoryType}`, { defaultValue: item.memoryType })}</td>
                <td>{t(`memory.scope.${item.scopeResourceType}`, { defaultValue: item.scopeResourceType })}</td>
                <td>{t(`memory.status.${item.status}`, { defaultValue: item.status })}</td>
                <td>{t(`truthClass.${item.truthClass}`, { defaultValue: item.truthClass })}</td>
                <td><time dateTime={item.expiresAt ?? undefined}>{formatDate(item.expiresAt, language)}</time></td>
              </tr>)}</tbody>
            </table>
          </div>
        ) : null}
      </section>

      {ingestOpen ? <IngestDialog onClose={() => setIngestOpen(false)} /> : null}
      {pendingDisable ? (
        <ConfirmDialog
          title={pendingDisable.title}
          message={t('knowledge.disableConfirm')}
          confirmLabel={t('knowledge.disable')}
          cancelLabel={t('actions.cancel')}
          onCancel={() => setPendingDisable(null)}
          onConfirm={() => { void confirmDisableSource(pendingDisable.sourceId); }}
        />
      ) : null}
    </section>
  );
};
