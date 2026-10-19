import { CircleAlert, Globe2, RefreshCw, ShieldCheck } from 'lucide-react';
import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { PageHeader } from '../../components/PageHeader';
import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { formatDateTime } from '../../lib/format';
import { useSession } from '../../lib/use-session';

// الوحدة 14 — حالة التكاملات الحقيقية: مزود Ollama المحلي فقط وسجل النماذج.
// «مدعوم» لا تعني «متصل» — الحالة من فحص فعلي، ولا موصلات جديدة تُضاف هنا.
export const IntegrationsPage = () => {
  const { t } = useTranslation();
  const { can } = useSession();
  const [configError, setConfigError] = useState<string | null>(null);
  const [configFeedback, setConfigFeedback] = useState<string | null>(null);
  const statusQuery = useQuery({
    queryKey: ['integration-status'],
    queryFn: nexusApi.getIntegrationStatus,
    staleTime: 30_000,
  });
  const errorCode = resolveErrorCode(statusQuery.error);
  const data = statusQuery.data;

  return (
    <section id="integrations" className="content-container" aria-labelledby="integrations-title" aria-live="polite">
      <PageHeader
        eyebrow={t('integrations.eyebrow')}
        eyebrowIcon={Globe2}
        title={t('integrations.title')}
        titleId="integrations-title"
        description={t('integrations.subtitle')}
        actions={<span className="status-badge">{t('moduleAvailability.status.READ_ONLY')}</span>}
      />

      {statusQuery.isLoading ? <LoadingState label={t('integrations.loading')} /> : null}
      {statusQuery.isError ? (
        <ErrorState code={errorCode} deniedMessage={t('integrations.permissionDenied')} />
      ) : null}

      {data ? (
        <>
          <section className="panel" aria-labelledby="integrations-providers-title">
            <div className="panel__header">
              <div><h2 id="integrations-providers-title">{t('integrations.providers.title')}</h2><span>{data.providers.length}</span></div>
              <button
                className="button button--quiet"
                type="button"
                onClick={() => { void statusQuery.refetch(); }}
                disabled={statusQuery.isFetching}
              >
                <RefreshCw size={16} className={statusQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
                {t('actions.refresh')}
              </button>
            </div>
            {data.providers.length > 0 ? (
              <ul className="details-list">
                {data.providers.map((provider) => (
                  <li key={provider.providerId}>
                    <div className="integrations-provider">
                      <div>
                        <strong dir="ltr">{provider.providerId}</strong>
                        <small>{t(`integrations.providerKind.${provider.providerKind}`)}</small>
                      </div>
                      <span className={`status-badge integration-state--${provider.state.toLowerCase()}`}>
                        {t(`integrations.state.${provider.state}`)}
                      </span>
                    </div>
                    {provider.state === 'CONNECTED' && provider.discoveredModelIds.length > 0 ? (
                      <div className="integration-models">
                        <span>{t('integrations.discoveredModels')}</span>
                        <ul>
                          {provider.discoveredModelIds.map((modelId) => <li key={modelId}><code dir="ltr">{modelId}</code></li>)}
                        </ul>
                      </div>
                    ) : null}
                  </li>
                ))}
              </ul>
            ) : <EmptyState icon={Globe2} variant="inline" title={t('integrations.providers.empty')} />}
            <p className="form-note">{t('integrations.providers.note')}</p>
          </section>

          <section className="panel" aria-labelledby="integrations-registry-title">
            <div className="panel__header">
              <div><h2 id="integrations-registry-title">{t('integrations.registry.title')}</h2><span>{data.enabledLocalModels.length}</span></div>
              {data.enabledLocalModels.length > 0 ? <ShieldCheck size={18} aria-hidden="true" /> : null}
            </div>
            {data.enabledLocalModels.length > 0 ? (
              <ul className="integration-models">
                {data.enabledLocalModels.map((modelId) => <li key={modelId}><code dir="ltr">{modelId}</code></li>)}
              </ul>
            ) : <EmptyState icon={ShieldCheck} variant="inline" title={t('integrations.registry.empty')} />}
            <ProviderCheckControls canCheck={can('integration:read')} />
            {!data.modelRegistryConfigurationAvailable ? (
              <p className="form-note">{t('integrations.registry.notConfigurable')}</p>
            ) : (
              <ModelConfigurationControls
                discoveredModelIds={data.providers.find((provider) => provider.state === 'CONNECTED')?.discoveredModelIds ?? []}
                configuredModelId={data.enabledLocalModels[0] ?? null}
                canConfigure={can('integration:update')}
                onError={setConfigError}
                onFeedback={setConfigFeedback}
              />
            )}
            {configFeedback ? <p className="operation-feedback" role="status">{configFeedback}</p> : null}
            {configError ? <div className="error-panel" role="alert"><CircleAlert size={20} aria-hidden="true" /><p>{t(`errors.${configError}`, { defaultValue: t('integrations.modelConfig.failure') })}</p></div> : null}
          </section>
        </>
      ) : null}
    </section>
  );
};

// فحص المزود الحي الصريح (C10): إجراء مستخدم بمهلة قصيرة — لا فحص دوري ولا ادعاء حالة دائم.
const ProviderCheckControls = ({ canCheck }: { readonly canCheck: boolean }) => {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage ?? 'ar';
  const checkMutation = useMutation({ mutationFn: nexusApi.checkAiProvider });
  // نتيجة فحص سابق لا يجوز أن تبقى معروضة بعد فشل فحص لاحق — الحالة المعروضة تتبع آخر محاولة.
  const result = checkMutation.isSuccess ? checkMutation.data : undefined;
  const errorCode = checkMutation.error instanceof NexusApiError ? checkMutation.error.code : null;
  const denial = checkMutation.isError && errorCode === 'AUTHORIZATION_DENIED';

  return (
    <div className="integration-model-config">
      <p className="form-note">{t('integrations.providerCheck.note')}</p>
      {canCheck ? (
        <div className="table-actions">
          <button
            className="button button--quiet"
            type="button"
            disabled={checkMutation.isPending}
            onClick={() => { checkMutation.mutate(); }}
          >
            <RefreshCw size={16} className={checkMutation.isPending ? 'spin' : undefined} aria-hidden="true" />
            {checkMutation.isPending ? t('integrations.providerCheck.checking') : t('integrations.providerCheck.action')}
          </button>
        </div>
      ) : null}
      {result ? (
        <p className="operation-feedback" role="status">
          {result.reachable
            ? t('integrations.providerCheck.reachable', { time: formatDateTime(result.checkedAt, language) })
            : t('integrations.providerCheck.unreachable', { time: formatDateTime(result.checkedAt, language) })}
        </p>
      ) : null}
      {denial ? (
        <p className="form-note" role="alert">{t('integrations.providerCheck.permissionDenied')}</p>
      ) : null}
      {checkMutation.isError && !denial ? (
        <div className="error-panel" role="alert">
          <CircleAlert size={20} aria-hidden="true" />
          <p>{t('integrations.providerCheck.failure')}</p>
        </div>
      ) : null}
    </div>
  );
};

const ModelConfigurationControls = ({
  discoveredModelIds,
  configuredModelId,
  canConfigure,
  onError,
  onFeedback,
}: {
  readonly discoveredModelIds: readonly string[];
  readonly configuredModelId: string | null;
  readonly canConfigure: boolean;
  readonly onError: (code: string | null) => void;
  readonly onFeedback: (message: string | null) => void;
}) => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const configureMutation = useMutation({ mutationFn: nexusApi.configureLocalModel });
  const clearMutation = useMutation({ mutationFn: nexusApi.clearLocalModelConfiguration });

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ['integration-status'] });
  };

  const select = async (modelId: string): Promise<void> => {
    onError(null);
    onFeedback(null);
    try {
      await configureMutation.mutateAsync({ modelId });
      onFeedback(t('integrations.modelConfig.selected', { model: modelId }));
      await refresh();
    } catch (error) {
      onError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  const clear = async (): Promise<void> => {
    onError(null);
    onFeedback(null);
    try {
      await clearMutation.mutateAsync();
      onFeedback(t('integrations.modelConfig.cleared'));
      await refresh();
    } catch (error) {
      onError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  const configurable = discoveredModelIds.filter((modelId) => modelId !== configuredModelId);

  return (
    <div className="integration-model-config">
      <p className="form-note">{t('integrations.modelConfig.note')}</p>
      <dl className="details-list">
        <div>
          <dt>{t('integrations.modelConfig.configured')}</dt>
          <dd>{configuredModelId ? <code dir="ltr">{configuredModelId}</code> : t('integrations.modelConfig.none')}</dd>
        </div>
      </dl>
      {canConfigure ? (
        <div className="table-actions">
          {configurable.map((modelId) => (
            <button
              key={modelId}
              className="button button--quiet"
              type="button"
              disabled={configureMutation.isPending}
              onClick={() => { void select(modelId); }}
            >
              {t('integrations.modelConfig.select', { model: modelId })}
            </button>
          ))}
          {configuredModelId ? (
            <button
              className="button button--secondary"
              type="button"
              disabled={clearMutation.isPending}
              onClick={() => { void clear(); }}
            >
              {t('integrations.modelConfig.clear')}
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
};
