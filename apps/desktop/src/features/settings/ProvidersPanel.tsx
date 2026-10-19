import { useState } from 'react';
import { CircleAlert, LoaderCircle, PlugZap, Plus, RefreshCw, ShieldAlert, Trash2, Zap } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { AiProviderKind, AiProviderRecord } from '@nexus/contracts';
import { aiProviderKinds } from '@nexus/contracts';

import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { nexusApi } from '../../lib/nexus-api';
import { useSession } from '../../lib/use-session';

// إدارة الموفرين: القائمة الموحّدة لكل الموفرين — المحليون والبعيدون في مسار واحد.
// قرار تجربة الاستخدام: لا مفتاح «محلي/سحابي» كما طُلب؛ المسار هو
// اختيار موفر ← تهيئة/اعتماد ← اختبار اتصال ← اكتشاف نماذج ← اختيار نموذج.
// الحالة `CONNECTED` لا تُمنح بحفظ مفتاح؛ تُمنح فقط بنتيجة اختبار اتصال ناجح فعلي.

const connectionStateModifier = (state: AiProviderRecord['connectionState']): string =>
  state.toLowerCase();

/** بطاقة موفر واحدة: الحالة + الإجراءات المصرَّح بها فقط. */
const ProviderRow = ({
  provider,
  canUpdate,
  canManageCredential,
  onConfigure,
  onCredential,
  onTest,
  onDiscover,
  onRemove,
  busy,
}: {
  provider: AiProviderRecord;
  canUpdate: boolean;
  canManageCredential: boolean;
  onConfigure: () => void;
  onCredential: () => void;
  onTest: () => void;
  onDiscover: () => void;
  onRemove: () => void;
  busy: boolean;
}) => {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage ?? 'ar';
  const checkedAt = provider.connectionCheckedAt
    ? new Intl.DateTimeFormat(language, { dateStyle: 'short', timeStyle: 'short' }).format(new Date(provider.connectionCheckedAt))
    : t('ai.providers.neverChecked');

  return (
    <article className="provider-row" aria-label={provider.displayName}>
      <div className="provider-row__main">
        <div className="provider-row__title">
          <strong>{provider.displayName}</strong>
          <span className={`provider-status provider-status--${connectionStateModifier(provider.connectionState)}`}>
            {t(`ai.providers.state.${provider.connectionState}`)}
          </span>
          {provider.builtIn ? <span className="badge">{t('ai.providers.builtIn')}</span> : null}
          {provider.credentialConfigured ? <span className="badge">{t('ai.providers.credentialStored')}</span> : null}
        </div>
        <span className="provider-row__meta">
          {t(`ai.providers.kind.${provider.kind}`)} · {provider.baseUrl || t('ai.providers.noBaseUrl')} ·{' '}
          {t('ai.providers.modelCount', { count: provider.modelCount })} · {t('ai.providers.lastChecked', { value: checkedAt })}
        </span>
      </div>
      <div className="provider-row__actions">
        {canUpdate ? (
          <>
            <button className="button button--quiet" type="button" onClick={onTest} disabled={busy}>
              <Zap size={16} aria-hidden="true" /> {t('ai.providers.actions.test')}
            </button>
            <button className="button button--quiet" type="button" onClick={onDiscover} disabled={busy}>
              <RefreshCw size={16} aria-hidden="true" /> {t('ai.providers.actions.discover')}
            </button>
            <button className="button button--secondary" type="button" onClick={onConfigure} disabled={busy}>
              {t('ai.providers.actions.configure')}
            </button>
          </>
        ) : null}
        {canManageCredential ? (
          <button className="button button--secondary" type="button" onClick={onCredential} disabled={busy}>
            {provider.credentialConfigured ? t('ai.providers.actions.replaceCredential') : t('ai.providers.actions.setCredential')}
          </button>
        ) : null}
        {canUpdate && !provider.builtIn ? (
          <button className="button button--danger" type="button" onClick={onRemove} disabled={busy}>
            <Trash2 size={16} aria-hidden="true" /> {t('ai.providers.actions.remove')}
          </button>
        ) : null}
      </div>
    </article>
  );
};

type ProviderDraft = {
  providerId: string;
  kind: AiProviderKind;
  displayName: string;
  baseUrl: string;
  enabled: boolean;
};

const emptyDraft: ProviderDraft = {
  providerId: '',
  kind: 'OPENAI_COMPATIBLE',
  displayName: '',
  baseUrl: '',
  enabled: true,
};

export const ProvidersPanel = () => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { can } = useSession();
  const canRead = can('integration:read');
  const canUpdate = can('integration:update');
  // صلاحية المادة السرية منفصلة عمداً: قراءة الموفرين لا تمنح حق كتابة مفتاح.
  const canManageCredential = can('integration:credential');

  const [draft, setDraft] = useState<ProviderDraft | null>(null);
  const [credentialTarget, setCredentialTarget] = useState<AiProviderRecord | null>(null);
  const [credentialValue, setCredentialValue] = useState('');
  const [notice, setNotice] = useState<{ tone: 'ok' | 'error'; text: string } | null>(null);

  const providersQuery = useQuery({
    queryKey: ['ai-providers'],
    queryFn: nexusApi.getAiProviders,
    staleTime: 10_000,
    enabled: canRead,
  });

  const invalidate = (): void => {
    void queryClient.invalidateQueries({ queryKey: ['ai-providers'] });
    void queryClient.invalidateQueries({ queryKey: ['ai-models'] });
    void queryClient.invalidateQueries({ queryKey: ['integration-status'] });
  };

  const reportError = (error: unknown): void => {
    setNotice({ tone: 'error', text: t(`errors.${resolveErrorCode(error)}`, { defaultValue: t('errors.unavailable') }) });
  };

  const configureMutation = useMutation({
    mutationFn: nexusApi.configureAiProvider,
    onSuccess: () => { setDraft(null); setNotice({ tone: 'ok', text: t('ai.providers.notice.configurationSaved') }); invalidate(); },
    onError: reportError,
  });
  const removeMutation = useMutation({
    mutationFn: nexusApi.removeAiProvider,
    onSuccess: () => { setNotice({ tone: 'ok', text: t('ai.providers.notice.removed') }); invalidate(); },
    onError: reportError,
  });
  const testMutation = useMutation({
    mutationFn: nexusApi.testAiProvider,
    // فشل الاختبار لا يحذف تهيئة صالحة ولا مفتاحاً محفوظاً — الحالة تُحدَّث فقط.
    onSuccess: (result) => {
      setNotice(result.connectionState === 'CONNECTED'
        ? { tone: 'ok', text: t('ai.providers.notice.testSucceeded', { count: result.discoveredModelCount, ms: result.latencyMs }) }
        : { tone: 'error', text: t('ai.providers.notice.testFailed', { code: result.safeErrorCode ?? 'UNAVAILABLE' }) });
      invalidate();
    },
    onError: reportError,
  });
  const discoverMutation = useMutation({
    mutationFn: nexusApi.discoverAiModels,
    onSuccess: (result) => {
      setNotice(result.safeErrorCode
        ? { tone: 'error', text: t('ai.providers.notice.discoveryFailed', { code: result.safeErrorCode }) }
        : { tone: 'ok', text: t('ai.providers.notice.discovered', { count: result.models.length }) });
      invalidate();
    },
    onError: reportError,
  });
  const credentialMutation = useMutation({
    mutationFn: nexusApi.setAiProviderCredential,
    onSuccess: () => {
      setCredentialTarget(null);
      setCredentialValue('');
      setNotice({ tone: 'ok', text: t('ai.providers.notice.credentialSaved') });
      invalidate();
    },
    onError: reportError,
  });
  const clearCredentialMutation = useMutation({
    mutationFn: nexusApi.clearAiProviderCredential,
    onSuccess: () => {
      setCredentialTarget(null);
      setCredentialValue('');
      setNotice({ tone: 'ok', text: t('ai.providers.notice.credentialCleared') });
      invalidate();
    },
    onError: reportError,
  });

  const busy = configureMutation.isPending || removeMutation.isPending || testMutation.isPending
    || discoverMutation.isPending || credentialMutation.isPending || clearCredentialMutation.isPending;

  if (!canRead) {
    return (
      <section className="panel" aria-labelledby="ai-providers-title">
        <div className="panel__header"><div><h2 id="ai-providers-title">{t('ai.providers.title')}</h2></div><PlugZap size={18} aria-hidden="true" /></div>
        <ErrorState code="AUTHORIZATION_DENIED" deniedMessage={t('ai.providers.permissionDenied')} />
      </section>
    );
  }

  const providers = providersQuery.data?.providers ?? [];

  return (
    <section className="panel" aria-labelledby="ai-providers-title" aria-live="polite">
      <div className="panel__header">
        <div><h2 id="ai-providers-title">{t('ai.providers.title')}</h2></div>
        <div className="page-heading__actions">
          {canUpdate ? (
            <button className="button button--primary" type="button" onClick={() => setDraft(emptyDraft)}>
              <Plus size={16} aria-hidden="true" /> {t('ai.providers.actions.add')}
            </button>
          ) : null}
          <button className="button button--secondary" type="button" onClick={() => { void providersQuery.refetch(); }} disabled={providersQuery.isFetching}>
            <RefreshCw size={16} className={providersQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
            {t('ai.providers.actions.refresh')}
          </button>
        </div>
      </div>

      <p className="form-note settings-note">{t('ai.providers.subtitle')}</p>

      {notice ? (
        <div className={notice.tone === 'ok' ? 'system-note' : 'error-panel'} role={notice.tone === 'ok' ? 'status' : 'alert'}>
          {notice.tone === 'ok' ? <PlugZap size={16} aria-hidden="true" /> : <CircleAlert size={18} aria-hidden="true" />}
          <span>{notice.text}</span>
        </div>
      ) : null}

      {providersQuery.isLoading ? <LoadingState label={t('ai.providers.loading')} /> : null}
      {providersQuery.isError ? (
        <ErrorState code={resolveErrorCode(providersQuery.error)} deniedMessage={t('ai.providers.permissionDenied')} />
      ) : null}

      {providersQuery.data && providers.length === 0 ? (
        <EmptyState icon={PlugZap} title={t('ai.providers.empty')} description={t('ai.providers.emptyHint')} />
      ) : null}

      {providers.length > 0 ? (
        <div className="provider-list">
          {providers.map((provider) => (
            <ProviderRow
              key={provider.providerId}
              provider={provider}
              canUpdate={canUpdate}
              canManageCredential={canManageCredential}
              busy={busy}
              onTest={() => { setNotice(null); testMutation.mutate({ providerId: provider.providerId }); }}
              onDiscover={() => { setNotice(null); discoverMutation.mutate({ providerId: provider.providerId }); }}
              onConfigure={() => setDraft({
                providerId: provider.providerId,
                kind: provider.kind,
                displayName: provider.displayName,
                baseUrl: provider.baseUrl,
                enabled: provider.enabled,
              })}
              onCredential={() => { setNotice(null); setCredentialValue(''); setCredentialTarget(provider); }}
              onRemove={() => { setNotice(null); removeMutation.mutate({ providerId: provider.providerId }); }}
            />
          ))}
        </div>
      ) : null}

      {draft ? (
        <div className="dialog-layer" role="presentation">
          <button className="dialog-backdrop" type="button" aria-label={t('actions.close')} onClick={() => setDraft(null)} />
          <section className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="provider-dialog-title">
            <header className="task-dialog__header">
              <div>
                <h2 id="provider-dialog-title">{t('ai.providers.dialog.title')}</h2>
                <p>{t('ai.providers.dialog.subtitle')}</p>
              </div>
            </header>
            <div className="task-form">
              <label>
                <span>{t('ai.providers.fields.providerId')}</span>
                <input
                  value={draft.providerId}
                  disabled={providers.some((provider) => provider.providerId === draft.providerId)}
                  onChange={(event) => setDraft({ ...draft, providerId: event.currentTarget.value })}
                />
              </label>
              <label>
                <span>{t('ai.providers.fields.kind')}</span>
                <select value={draft.kind} onChange={(event) => setDraft({ ...draft, kind: event.currentTarget.value as AiProviderKind })}>
                  {aiProviderKinds.map((kind) => <option key={kind} value={kind}>{t(`ai.providers.kind.${kind}`)}</option>)}
                </select>
              </label>
              <label>
                <span>{t('ai.providers.fields.displayName')}</span>
                <input value={draft.displayName} onChange={(event) => setDraft({ ...draft, displayName: event.currentTarget.value })} />
              </label>
              <label>
                <span>{t('ai.providers.fields.baseUrl')}</span>
                <input
                  value={draft.baseUrl}
                  placeholder={t('ai.providers.fields.baseUrlPlaceholder')}
                  onChange={(event) => setDraft({ ...draft, baseUrl: event.currentTarget.value })}
                />
                <small className="form-note">{t('ai.providers.fields.baseUrlHint')}</small>
              </label>
              <label>
                <span>{t('ai.providers.fields.enabled')}</span>
                <input type="checkbox" checked={draft.enabled} onChange={(event) => setDraft({ ...draft, enabled: event.currentTarget.checked })} />
              </label>
              <div className="task-form__actions">
                <button className="button button--secondary" type="button" onClick={() => setDraft(null)}>{t('ai.providers.actions.cancel')}</button>
                <button
                  className="button button--primary"
                  type="button"
                  disabled={!draft.providerId.trim() || !draft.displayName.trim() || configureMutation.isPending}
                  onClick={() => configureMutation.mutate({
                    providerId: draft.providerId.trim(),
                    kind: draft.kind,
                    displayName: draft.displayName.trim(),
                    baseUrl: draft.baseUrl.trim(),
                    enabled: draft.enabled,
                  })}
                >
                  {t('ai.providers.actions.save')}
                </button>
              </div>
            </div>
          </section>
        </div>
      ) : null}

      {credentialTarget ? (
        <div className="dialog-layer" role="presentation">
          <button className="dialog-backdrop" type="button" aria-label={t('actions.close')} onClick={() => { setCredentialTarget(null); setCredentialValue(''); }} />
          <section className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="credential-dialog-title">
            <header className="task-dialog__header">
              <div>
                <h2 id="credential-dialog-title">{t('ai.providers.credential.title', { provider: credentialTarget.displayName })}</h2>
                <p>{t('ai.providers.credential.subtitle')}</p>
              </div>
              <ShieldAlert size={18} aria-hidden="true" />
            </header>
            <div className="task-form">
              <label>
                <span>{t('ai.providers.credential.label')}</span>
                <input
                  type="password"
                  autoComplete="off"
                  value={credentialValue}
                  onChange={(event) => setCredentialValue(event.currentTarget.value)}
                />
                <small className="form-note">{t('ai.providers.credential.hint')}</small>
              </label>
              <div className="task-form__actions">
                {credentialTarget.credentialConfigured ? (
                  <button
                    className="button button--danger"
                    type="button"
                    disabled={clearCredentialMutation.isPending}
                    onClick={() => clearCredentialMutation.mutate({ providerId: credentialTarget.providerId })}
                  >
                    {t('ai.providers.credential.clear')}
                  </button>
                ) : null}
                <button className="button button--secondary" type="button" onClick={() => { setCredentialTarget(null); setCredentialValue(''); }}>
                  {t('ai.providers.actions.cancel')}
                </button>
                <button
                  className="button button--primary"
                  type="button"
                  disabled={!credentialValue || credentialMutation.isPending}
                  onClick={() => credentialMutation.mutate({ providerId: credentialTarget.providerId, credential: credentialValue })}
                >
                  {credentialMutation.isPending
                    ? <LoaderCircle className="spin" size={16} aria-hidden="true" />
                    : t('ai.providers.credential.save')}
                </button>
              </div>
            </div>
          </section>
        </div>
      ) : null}
    </section>
  );
};
