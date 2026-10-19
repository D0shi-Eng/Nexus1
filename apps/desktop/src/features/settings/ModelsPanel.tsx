import { useMemo, useState } from 'react';
import { Cpu, Filter, RefreshCw, Search, Sparkles } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { AiModelRecord } from '@nexus/contracts';

import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { ModelSelector } from '../../components/ModelSelector';
import { resolveErrorCode } from '../../lib/api-error';
import { nexusApi } from '../../lib/nexus-api';
import { useSession } from '../../lib/use-session';

// صفحة النماذج: سجل موحّد لكل النماذج المكتشفة عبر كل الموفرين.
// مبادئ معلنة في الواجهة نفسها:
// - لا تُخترع قدرات: غير المعروف يُعرض «غير معروف» لا «مدعوم» ولا «0».
// - النموذج نفسه يظهر تحت موفره الصحيح، فلا تكرار بلا تمييز.
// - لا fallback صامت: الاختيار صريح من المستخدم، ويُحفظ خلفياً بحالة واحدة مختارة.

type AvailabilityFilter = 'ALL' | 'AVAILABLE' | 'UNAVAILABLE';

const CapabilityChip = ({ label, value }: { label: string; value: boolean | null }) => {
  const { t } = useTranslation();
  const tone = value === true ? 'yes' : value === false ? 'no' : 'unknown';
  return (
    <span className={`capability-chip capability-chip--${tone}`}>
      {label}: {value === null ? t('ai.models.unknown') : value ? t('ai.models.yes') : t('ai.models.no')}
    </span>
  );
};

export const ModelsPanel = () => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { can } = useSession();
  const canRead = can('integration:read');
  const canSelect = can('integration:update');

  const [query, setQuery] = useState('');
  const [providerFilter, setProviderFilter] = useState('ALL');
  const [availability, setAvailability] = useState<AvailabilityFilter>('ALL');

  const modelsQuery = useQuery({
    queryKey: ['ai-models'],
    queryFn: nexusApi.getAiModels,
    staleTime: 10_000,
    enabled: canRead,
  });

  const selectMutation = useMutation({
    mutationFn: nexusApi.selectAiModel,
    onSuccess: (data) => {
      queryClient.setQueryData(['ai-models'], data);
      void queryClient.invalidateQueries({ queryKey: ['ai-providers'] });
      void queryClient.invalidateQueries({ queryKey: ['integration-status'] });
    },
  });
  const clearMutation = useMutation({
    mutationFn: nexusApi.clearAiModelSelection,
    onSuccess: (data) => {
      queryClient.setQueryData(['ai-models'], data);
      void queryClient.invalidateQueries({ queryKey: ['ai-providers'] });
      void queryClient.invalidateQueries({ queryKey: ['integration-status'] });
    },
  });

  // مرجع ثابت للنماذج: يمنع إعادة حساب التصفية في كل رسم ويُبقي اعتماديات useMemo مستقرة.
  const models = useMemo(() => modelsQuery.data?.models ?? [], [modelsQuery.data?.models]);
  const providerOptions = useMemo(
    () => [...new Set(models.map((model) => model.providerId))].sort(),
    [models],
  );

  const visibleModels = useMemo(() => models.filter((model) => {
    const normalized = query.trim().toLocaleLowerCase();
    const matchesQuery = !normalized
      || `${model.displayName} ${model.modelId} ${model.providerDisplayName}`.toLocaleLowerCase().includes(normalized);
    const matchesProvider = providerFilter === 'ALL' || model.providerId === providerFilter;
    const matchesAvailability = availability === 'ALL'
      || (availability === 'AVAILABLE' ? model.available : !model.available);
    return matchesQuery && matchesProvider && matchesAvailability;
  }), [availability, models, providerFilter, query]);

  if (!canRead) {
    return (
      <section className="panel" aria-labelledby="ai-models-title">
        <div className="panel__header"><div><h2 id="ai-models-title">{t('ai.models.title')}</h2></div><Cpu size={18} aria-hidden="true" /></div>
        <ErrorState code="AUTHORIZATION_DENIED" deniedMessage={t('ai.models.permissionDenied')} />
      </section>
    );
  }

  const selected = models.find((model) => model.selected) ?? null;

  return (
    <section className="panel" aria-labelledby="ai-models-title" aria-live="polite">
      <div className="panel__header">
        <div>
          <h2 id="ai-models-title">{t('ai.models.title')}</h2>
          <span>{models.length}</span>
        </div>
        <div className="page-heading__actions">
          <button className="button button--secondary" type="button" onClick={() => { void modelsQuery.refetch(); }} disabled={modelsQuery.isFetching}>
            <RefreshCw size={16} className={modelsQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
            {t('ai.models.actions.refresh')}
          </button>
        </div>
      </div>

      <p className="form-note settings-note">{t('ai.models.subtitle')}</p>

      {/* نفس المكوّن المستخدم في بقية أسطح الذكاء الاصطناعي — مسار اختيار واحد. */}
      <ModelSelector id="settings-model-selector" />

      {selected ? (
        <div className="system-note">
          <Sparkles size={16} aria-hidden="true" />
          <span>{t('ai.models.selectedModel', { model: selected.displayName, provider: selected.providerDisplayName })}</span>
        </div>
      ) : null}

      {modelsQuery.isLoading ? <LoadingState label={t('ai.models.loading')} /> : null}
      {modelsQuery.isError ? (
        <ErrorState code={resolveErrorCode(modelsQuery.error)} deniedMessage={t('ai.models.permissionDenied')} />
      ) : null}

      {modelsQuery.data && models.length === 0 ? (
        <EmptyState icon={Cpu} title={t('ai.models.empty')} description={t('ai.models.emptyHint')} />
      ) : null}

      {models.length > 0 ? (
        <>
          <div className="attention-toolbar" aria-label={t('ai.models.filters.title')}>
            <div className="attention-toolbar__heading"><Filter size={16} aria-hidden="true" /><h3>{t('ai.models.filters.title')}</h3></div>
            <label className="attention-search">
              <span className="visually-hidden">{t('actions.search')}</span>
              <Search size={16} aria-hidden="true" />
              <input value={query} onChange={(event) => setQuery(event.currentTarget.value)} placeholder={t('ai.models.searchPlaceholder')} />
            </label>
            <label>
              <span>{t('ai.models.filters.provider')}</span>
              <select value={providerFilter} onChange={(event) => setProviderFilter(event.currentTarget.value)}>
                <option value="ALL">{t('ai.models.filters.all')}</option>
                {providerOptions.map((providerId) => {
                  const model = models.find((item) => item.providerId === providerId);
                  return <option key={providerId} value={providerId}>{model?.providerDisplayName ?? providerId}</option>;
                })}
              </select>
            </label>
            <label>
              <span>{t('ai.models.filters.availability')}</span>
              <select value={availability} onChange={(event) => setAvailability(event.currentTarget.value as AvailabilityFilter)}>
                <option value="ALL">{t('ai.models.filters.all')}</option>
                <option value="AVAILABLE">{t('ai.models.filters.available')}</option>
                <option value="UNAVAILABLE">{t('ai.models.filters.unavailable')}</option>
              </select>
            </label>
            <button
              className="button button--quiet"
              type="button"
              onClick={() => { setQuery(''); setProviderFilter('ALL'); setAvailability('ALL'); }}
              disabled={!query && providerFilter === 'ALL' && availability === 'ALL'}
            >
              {t('ai.models.actions.clearFilters')}
            </button>
          </div>

          {visibleModels.length === 0 ? (
            <EmptyState icon={Cpu} variant="inline" title={t('ai.models.noMatches')} />
          ) : (
            <div className="model-list">
              {visibleModels.map((model: AiModelRecord) => (
                <article
                  key={`${model.providerId}::${model.modelId}`}
                  className={`model-row ${model.selected ? 'model-row--selected' : ''}`}
                >
                  <span className={`provider-status provider-status--${model.available ? 'connected' : 'unavailable'}`}>
                    {model.available ? t('ai.models.available') : t('ai.models.unavailable')}
                  </span>
                  <div className="model-row__main">
                    <span className="model-row__name">{model.displayName}</span>
                    <span className="model-row__meta">
                      {model.providerDisplayName} · {model.modelId} ·{' '}
                      {t('ai.models.contextLabel', {
                        value: model.contextLimit === null ? t('ai.models.unknown') : String(model.contextLimit),
                      })} · {t(`ai.models.source.${model.capabilitySource}`)}
                    </span>
                    <span className="model-row__caps">
                      <CapabilityChip label={t('ai.models.capabilities.tools')} value={model.supportsTools} />
                      <CapabilityChip label={t('ai.models.capabilities.structured')} value={model.supportsStructuredOutput} />
                      <CapabilityChip label={t('ai.models.capabilities.vision')} value={model.supportsVision} />
                    </span>
                  </div>
                  <div className="provider-row__actions">
                    {canSelect ? (
                      <button
                        className="button button--secondary"
                        type="button"
                        disabled={!model.available || model.selected || selectMutation.isPending}
                        onClick={() => selectMutation.mutate({ providerId: model.providerId, modelId: model.modelId })}
                      >
                        {model.selected ? t('ai.models.actions.selected') : t('ai.models.actions.select')}
                      </button>
                    ) : null}
                  </div>
                </article>
              ))}
            </div>
          )}

          {canSelect && selected ? (
            <div className="panel__footer">
              <button className="button button--quiet" type="button" onClick={() => clearMutation.mutate()} disabled={clearMutation.isPending}>
                {t('ai.models.actions.clearSelection')}
              </button>
            </div>
          ) : null}
        </>
      ) : null}
    </section>
  );
};
