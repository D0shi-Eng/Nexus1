import { CircleAlert, ListChecks, LoaderCircle, Play, X } from 'lucide-react';
import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { AgentRunRecord, AgentRunResultData, AgentRunStatus } from '@nexus/contracts';

import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { useModalFocus } from '../../lib/use-modal-focus';
import { useSession } from '../../lib/use-session';
import { formatDate } from '../../lib/format';

const formatRunDate = (value: string, language: string): string => formatDate(value, language);

// الحالات التي يسمح الدومين بإلغائها — الإلغاء الفعلي يبقى محكوماً بالخلف.
const cancellableStatuses: readonly AgentRunStatus[] = ['QUEUED', 'RUNNING', 'WAITING_APPROVAL'];
const resultStatuses: readonly AgentRunStatus[] = ['COMPLETED', 'FAILED'];

const AgentRunStatusBadge = ({ status }: { readonly status: AgentRunStatus }) => {
  const { t } = useTranslation();
  return <span className={`badge badge--${status.toLowerCase()}`}>{t(`agentRuns.status.${status}`)}</span>;
};

// حوار نتيجة التنفيذ: منشأ كامل (مزود/نموذج/زمن/رمز فشل) والمخرج يوسم صريحاً
// كنص مولد من نموذج ذكاء اصطناعي — لا يرقى إلى حقيقة بأي حال.
const RunResultDialog = ({
  result,
  onClose,
}: {
  result: AgentRunResultData;
  onClose: () => void;
}) => {
  const { t, i18n } = useTranslation();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const language = i18n.resolvedLanguage ?? 'ar';
  const invocationsQuery = useQuery({
    queryKey: ['tool-invocations', result.runId],
    queryFn: () => nexusApi.getToolInvocationsForRun(result.runId),
    staleTime: 30_000,
  });

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="run-result-title" tabIndex={-1}>
        <header className="task-dialog__header">
          <div><h2 id="run-result-title">{t('agentRuns.result.title')}</h2><p><code dir="ltr">{result.runId}</code></p></div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}><X size={18} aria-hidden="true" /></button>
        </header>
        <dl className="details-list">
          <div><dt>{t('agentRuns.table.status')}</dt><dd><AgentRunStatusBadge status={result.runStatus} /></dd></div>
          <div><dt>{t('agentRuns.result.outcome')}</dt><dd>{t(`agentRuns.outcome.${result.outcome}`)}</dd></div>
          <div><dt>{t('agentRuns.result.provider')}</dt><dd>{result.providerId}</dd></div>
          {result.modelId ? <div><dt>{t('agentRuns.result.model')}</dt><dd><code dir="ltr">{result.modelId}</code></dd></div> : null}
          {result.failureCode ? (
            <div><dt>{t('agentRuns.result.failureCode')}</dt><dd>{t(`errors.${result.failureCode}`, { defaultValue: result.failureCode })}</dd></div>
          ) : null}
          {result.providerGeneratedAt ? (
            <div><dt>{t('agentRuns.result.generatedAt')}</dt><dd>{formatRunDate(result.providerGeneratedAt, language)}</dd></div>
          ) : null}
          <div><dt>{t('agentRuns.result.recordedAt')}</dt><dd>{formatRunDate(result.createdAt, language)}</dd></div>
        </dl>
        {result.output ? (
          <div className="run-result-output">
            <span className="demo-label">{t('agentRuns.result.aiOutputLabel')}</span>
            <pre>{result.output}</pre>
          </div>
        ) : null}
        {invocationsQuery.data?.hasData ? (
          <div>
            <h3>{t('agentRuns.invocations.title')}</h3>
            <ul className="assignment-list">
              {invocationsQuery.data.items.map((invocation) => (
                <li key={invocation.id}>
                  <code dir="ltr">{invocation.toolId}</code>
                  {' - '}
                  {t(`agentRuns.invocations.status.${invocation.status}`)}
                  {' - '}
                  {t(`agentRuns.invocations.decision.${invocation.policyDecision}`)}
                  {invocation.failureCode ? ` - ${t(`errors.${invocation.failureCode}`, { defaultValue: invocation.failureCode })}` : ''}
                </li>
              ))}
            </ul>
          </div>
        ) : null}
        <p className="form-note">{t('agentRuns.result.note')}</p>
      </section>
    </div>
  );
};

export const AgentRunsPanel = () => {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const { can } = useSession();
  const language = i18n.resolvedLanguage ?? 'ar';
  const [cancelError, setCancelError] = useState<string | null>(null);
  const [resultForRun, setResultForRun] = useState<AgentRunRecord | null>(null);
  const runsQuery = useQuery({ queryKey: ['agent-runs'], queryFn: nexusApi.getAgentRuns, staleTime: 30_000 });
  const resultQuery = useQuery({
    queryKey: ['agent-run-result', resultForRun?.id],
    queryFn: () => nexusApi.getAgentRunResult({ runId: resultForRun!.id }),
    enabled: resultForRun !== null,
    staleTime: 30_000,
  });
  const cancelRunMutation = useMutation({
    mutationFn: nexusApi.cancelAgentRun,
    onSuccess: () => {
      setCancelError(null);
      void queryClient.invalidateQueries({ queryKey: ['agent-runs'] });
      void queryClient.invalidateQueries({ queryKey: ['dashboard'] });
      void queryClient.invalidateQueries({ queryKey: ['attention'] });
    },
    onError: (error) => {
      setCancelError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    },
  });
  // بدء التنفيذ الفعلي بعد اعتماد المراجع (طلب QUEUED) - نداء واحد يحكمه
  // المنسق الخلفي: نداء مزود ثم أداة واحدة كحد أقصى ثم نداء أخير.
  const startRunMutation = useMutation({
    mutationFn: nexusApi.startQueuedExecution,
    onSuccess: () => {
      setStartError(null);
      void queryClient.invalidateQueries({ queryKey: ['agent-runs'] });
      void queryClient.invalidateQueries({ queryKey: ['agent-run-result'] });
      void queryClient.invalidateQueries({ queryKey: ['tool-invocations'] });
    },
    onError: (error) => {
      setStartError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    },
  });
  const [startError, setStartError] = useState<string | null>(null);
  const [startingRunId, setStartingRunId] = useState<string | null>(null);
  const errorCode = resolveErrorCode(runsQuery.error);
  const canCancel = can('agent:execute');

  return (
    <section className="panel panel--table" aria-labelledby="agent-runs-title">
      <div className="panel__header"><div><h2 id="agent-runs-title">{t('agentRuns.title')}</h2><span>{runsQuery.data?.items.length ?? 0}</span></div><ListChecks size={18} aria-hidden="true" /></div>
      <p className="form-note">{t('agentRuns.boundary')}</p>
      {runsQuery.isLoading ? <LoadingState label={t('agentRuns.loading')} /> : null}
      {runsQuery.isError ? <ErrorState code={errorCode} deniedMessage={t('agentRuns.permissionDenied')} /> : null}
      {cancelError ? <div className="error-panel" role="alert"><CircleAlert size={20} aria-hidden="true" /><p>{t(`errors.${cancelError}`, { defaultValue: t('agentRuns.cancelFailure') })}</p></div> : null}
      {startError ? <div className="error-panel" role="alert"><CircleAlert size={20} aria-hidden="true" /><p>{t(`errors.${startError}`, { defaultValue: t('agentRuns.startFailure') })}</p></div> : null}
      {runsQuery.data && !runsQuery.data.hasData ? <EmptyState icon={ListChecks} title={t('agentRuns.empty')} /> : null}
      {runsQuery.data?.hasData ? (
        <div className="table-scroll">
          <table>
            <thead><tr><th>{t('agentRuns.table.agent')}</th><th>{t('agentRuns.table.status')}</th><th>{t('agentRuns.table.requestedBy')}</th><th>{t('agentRuns.table.created')}</th><th>{t('agentRuns.table.updated')}</th><th><span className="visually-hidden">{t('table.action')}</span></th></tr></thead>
            <tbody>{runsQuery.data.items.map((run) => (
              <tr key={run.id}>
                <td><code dir="ltr">{run.agentId}</code></td>
                <td><AgentRunStatusBadge status={run.status} /></td>
                <td><code dir="ltr">{run.requestedByUserId}</code></td>
                <td><time dateTime={run.createdAt}>{formatRunDate(run.createdAt, language)}</time></td>
                <td><time dateTime={run.updatedAt}>{formatRunDate(run.updatedAt, language)}</time></td>
                <td>
                  <div className="table-actions">
                    {run.status === 'QUEUED' && canCancel ? (
                      <button
                        className="button button--quiet"
                        type="button"
                        disabled={startRunMutation.isPending}
                        onClick={() => {
                          setStartError(null);
                          setStartingRunId(run.id);
                          startRunMutation.mutate(run.id, { onSettled: () => setStartingRunId(null) });
                        }}
                      >
                        {startingRunId === run.id ? <LoaderCircle className="spin" size={16} aria-hidden="true" /> : <Play size={16} aria-hidden="true" />}
                        {t('agentRuns.start')}
                      </button>
                    ) : null}
                    {cancellableStatuses.includes(run.status) && canCancel ? (
                      <button
                        className="button button--quiet"
                        type="button"
                        disabled={cancelRunMutation.isPending}
                        onClick={() => {
                          setCancelError(null);
                          cancelRunMutation.mutate({ runId: run.id });
                        }}
                      >
                        {t('agentRuns.cancel')}
                      </button>
                    ) : null}
                    {resultStatuses.includes(run.status) ? (
                      <button className="button button--quiet" type="button" onClick={() => setResultForRun(run)}>
                        {t('agentRuns.viewResult')}
                      </button>
                    ) : null}
                  </div>
                </td>
              </tr>
            ))}</tbody>
          </table>
        </div>
      ) : null}
      {resultForRun && resultQuery.data ? (
        <RunResultDialog result={resultQuery.data} onClose={() => setResultForRun(null)} />
      ) : null}
      {resultForRun && resultQuery.isError ? (
        <div className="error-panel" role="alert">
          <CircleAlert size={20} aria-hidden="true" />
          <p>{t('agentRuns.result.unavailable')}</p>
          <button className="button button--quiet" type="button" onClick={() => setResultForRun(null)}>{t('actions.close')}</button>
        </div>
      ) : null}
    </section>
  );
};
