import { useMemo, useState } from 'react';
import { ClipboardCheck, Link2, Plus, RefreshCw, Search } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { ControlEffectiveness, ControlRecord, ControlStatus } from '@nexus/contracts';
import { controlEffectivenessValues } from '@nexus/contracts';

import { PageHeader } from '../../components/PageHeader';
import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { nexusApi } from '../../lib/nexus-api';
import { useSession } from '../../lib/use-session';

// قسم ارتباط واحد: قائمة المرتبط حالياً + اختيار جديد. لا يوحي بوجود ارتباط
// لم يُحفظ، ولا يعرض مرشحاً غير موجود فعلاً.
const LinkSection = ({
  title,
  linked,
  candidates,
  selected,
  placeholder,
  emptyLabel,
  onSelect,
  onLink,
  onUnlink,
  busy,
}: {
  title: string;
  linked: readonly { id: string; label: string }[];
  candidates: readonly { id: string; label: string }[];
  selected: string;
  placeholder: string;
  emptyLabel: string;
  onSelect: (value: string) => void;
  onLink: () => void;
  onUnlink: (id: string) => void;
  busy: boolean;
}) => {
  const { t } = useTranslation();
  const linkedIds = new Set(linked.map((item) => item.id));
  const selectable = candidates.filter((candidate) => !linkedIds.has(candidate.id));

  return (
    <section className="link-section">
      <h3>{title}</h3>
      {linked.length > 0 ? (
        <ul className="link-section__list">
          {linked.map((item) => (
            <li key={item.id}>
              <span>{item.label}</span>
              <button className="button button--quiet" type="button" disabled={busy} onClick={() => onUnlink(item.id)}>
                {t('controls.links.removeLink')}
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="form-note">{emptyLabel}</p>
      )}
      {selectable.length > 0 ? (
        <div className="link-section__add">
          <select value={selected} onChange={(event) => onSelect(event.currentTarget.value)} aria-label={placeholder}>
            <option value="">{placeholder}</option>
            {selectable.map((candidate) => (
              <option key={candidate.id} value={candidate.id}>{candidate.label}</option>
            ))}
          </select>
          <button className="button button--secondary" type="button" disabled={!selected || busy} onClick={onLink}>
            {t('controls.links.addLink')}
          </button>
        </div>
      ) : <p className="form-note">{t('controls.links.noCandidates')}</p>}
    </section>
  );
};

// وحدة الضوابط: سجل قابل للتأليف مع دورة تقييم مسجَّلة.
//
// عيب مؤكَّد أُصلح (CQ-03): الوحدة كانت للقراءة فقط في كل الطبقات — لا صلاحية ولا
// دالة خدمة ولا أمر IPC ولا تحكم واجهة. الآن تُنفَّذ أصغر دورة عمل متماسكة وآمنة:
// إنشاء · تعديل · تعطيل · تقييم فعالية — وكل تغيير يمر بتفويض وتدقيق وعزل مستأجرين،
// والواجهة لا ترسل هوية ولا مستأجراً ولا تصنيفاً، ولا تعرض قدرة لا يملكها الفاعل.

const formatDate = (value: string | null, language: string): string => {
  if (!value) return '—';
  return new Intl.DateTimeFormat(language, { day: 'numeric', month: 'short', year: 'numeric' }).format(new Date(value));
};

const matchesControl = (control: ControlRecord, query: string, language: string): boolean => {
  const normalizedQuery = query.trim().toLocaleLowerCase(language);
  return !normalizedQuery
    || control.title.toLocaleLowerCase(language).includes(normalizedQuery)
    || (control.owner ?? '').toLocaleLowerCase(language).includes(normalizedQuery)
    || (control.category ?? '').toLocaleLowerCase(language).includes(normalizedQuery);
};

type ControlDraft = { controlId: string | null; title: string; category: string; owner: string };

export const ControlsPage = () => {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const { can } = useSession();
  const [query, setQuery] = useState('');
  const [draft, setDraft] = useState<ControlDraft | null>(null);
  const [assessmentTarget, setAssessmentTarget] = useState<ControlRecord | null>(null);
  const [linksTarget, setLinksTarget] = useState<ControlRecord | null>(null);
  const [linkSelection, setLinkSelection] = useState({ obligation: '', risk: '', evidence: '' });
  const [effectiveness, setEffectiveness] = useState<ControlEffectiveness>('NOT_ASSESSED');
  const [assessmentNote, setAssessmentNote] = useState('');
  const [notice, setNotice] = useState<string | null>(null);
  const language = i18n.resolvedLanguage ?? 'ar';

  const canCreate = can('control:create');
  const canUpdate = can('control:update');
  const canDisable = can('control:disable');
  const canAssess = can('control:assess');
  const canLink = can('control:link');

  const controlsQuery = useQuery({
    queryKey: ['controls'],
    queryFn: nexusApi.getControls,
    staleTime: 30_000,
  });
  const errorCode = resolveErrorCode(controlsQuery.error);

  const invalidate = (): void => {
    void queryClient.invalidateQueries({ queryKey: ['controls'] });
    void queryClient.invalidateQueries({ queryKey: ['audit-trail'] });
  };

  const createMutation = useMutation({
    mutationFn: nexusApi.createControl,
    onSuccess: () => { setDraft(null); setNotice(t('controls.notice.created')); invalidate(); },
  });
  const updateMutation = useMutation({
    mutationFn: nexusApi.updateControl,
    onSuccess: () => { setDraft(null); setNotice(t('controls.notice.updated')); invalidate(); },
  });
  const disableMutation = useMutation({
    mutationFn: nexusApi.disableControl,
    onSuccess: () => { setNotice(t('controls.notice.disabled')); invalidate(); },
  });
  const assessmentMutation = useMutation({
    mutationFn: nexusApi.recordControlAssessment,
    onSuccess: () => { setAssessmentTarget(null); setAssessmentNote(''); setNotice(t('controls.notice.assessed')); invalidate(); },
  });

  // بيانات الارتباطات تُجلب فقط عند فتح حوارها — لا طلبات بلا سبب.
  const linkDetailQuery = useQuery({
    queryKey: ['control-detail', linksTarget?.id],
    queryFn: () => nexusApi.getControlDetail(linksTarget!.id),
    enabled: Boolean(linksTarget) && canLink,
  });
  const obligationsQuery = useQuery({ queryKey: ['obligations'], queryFn: nexusApi.getObligations, enabled: Boolean(linksTarget) });
  const risksQuery = useQuery({ queryKey: ['risks'], queryFn: nexusApi.getRisks, enabled: Boolean(linksTarget) });
  const evidenceQuery = useQuery({ queryKey: ['evidence'], queryFn: nexusApi.getEvidence, enabled: Boolean(linksTarget) });

  const linkMutation = useMutation({
    mutationFn: (input: { kind: 'obligation' | 'risk' | 'evidence'; controlId: string; targetId: string; link: boolean }) => {
      const base = { controlId: input.controlId };
      if (input.kind === 'obligation') {
        return input.link
          ? nexusApi.linkControlObligation({ ...base, obligationId: input.targetId })
          : nexusApi.unlinkControlObligation({ ...base, obligationId: input.targetId });
      }
      if (input.kind === 'risk') {
        return input.link
          ? nexusApi.linkControlRisk({ ...base, riskId: input.targetId })
          : nexusApi.unlinkControlRisk({ ...base, riskId: input.targetId });
      }
      return input.link
        ? nexusApi.linkControlEvidence({ ...base, evidenceId: input.targetId })
        : nexusApi.unlinkControlEvidence({ ...base, evidenceId: input.targetId });
    },
    onSuccess: () => {
      setLinkSelection({ obligation: '', risk: '', evidence: '' });
      void queryClient.invalidateQueries({ queryKey: ['control-detail'] });
      invalidate();
    },
  });

  const saveDraft = (): void => {
    if (!draft || !draft.title.trim()) return;
    const category = draft.category.trim() || null;
    const owner = draft.owner.trim() || null;
    if (draft.controlId) {
      updateMutation.mutate({ controlId: draft.controlId, title: draft.title.trim(), category, owner });
    } else {
      createMutation.mutate({ title: draft.title.trim(), category, owner });
    }
  };

  const visibleControls = useMemo(
    () => (controlsQuery.data?.items ?? []).filter((control) => matchesControl(control, query, language)),
    [controlsQuery.data?.items, language, query],
  );
  const noData = Boolean(controlsQuery.data && !controlsQuery.data.hasData);
  const hasFilteredOutControls = Boolean(controlsQuery.data?.items.length) && visibleControls.length === 0;
  const busy = createMutation.isPending || updateMutation.isPending || disableMutation.isPending || assessmentMutation.isPending || linkMutation.isPending;

  return (
    <section id="controls" className="controls-page content-container" aria-labelledby="controls-title" aria-live="polite">
      <PageHeader
        eyebrow={t('controls.eyebrow')}
        eyebrowIcon={ClipboardCheck}
        title={t('controls.title')}
        titleId="controls-title"
        description={t('controls.subtitle')}
        actions={
          <>
            {canCreate ? (
              <button className="button button--primary" type="button" onClick={() => setDraft({ controlId: null, title: '', category: '', owner: '' })}>
                <Plus size={16} aria-hidden="true" />
                {t('controls.actions.create')}
              </button>
            ) : null}
            <button className="button button--secondary" type="button" onClick={() => { void controlsQuery.refetch(); }} disabled={controlsQuery.isFetching}>
              <RefreshCw size={16} className={controlsQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
              {t('controls.actions.refresh')}
            </button>
          </>
        }
      />

      {notice ? <div className="system-note" role="status"><ClipboardCheck size={16} aria-hidden="true" /><span>{notice}</span></div> : null}
      {createMutation.isError || updateMutation.isError || disableMutation.isError || assessmentMutation.isError ? (
        <ErrorState code={resolveErrorCode(createMutation.error ?? updateMutation.error ?? disableMutation.error ?? assessmentMutation.error)} deniedMessage={t('controls.permissionDenied')} />
      ) : null}

      {controlsQuery.isLoading ? <LoadingState label={t('controls.loading')} /> : null}
      {controlsQuery.isError ? (
        <ErrorState code={errorCode} deniedMessage={t('controls.permissionDenied')} />
      ) : null}

      {noData ? (
        <EmptyState
          icon={ClipboardCheck}
          title={t('controls.empty')}
          description={canCreate ? t('controls.emptyHint') : undefined}
          actions={canCreate ? (
            <button className="button button--primary" type="button" onClick={() => setDraft({ controlId: null, title: '', category: '', owner: '' })}>
              <Plus size={16} aria-hidden="true" />
              {t('controls.actions.create')}
            </button>
          ) : null}
        />
      ) : null}

      {controlsQuery.data && controlsQuery.data.hasData ? (
        <>
          <section className="metric-grid" aria-label={t('controls.summaryLabel')}>
            <article className="metric-card"><span>{t('controls.summary.total')}</span><strong>{controlsQuery.data.summary.total}</strong><small>{t(`status.${controlsQuery.data.dataMode}`)}</small></article>
            <article className="metric-card"><span>{t('controls.summary.active')}</span><strong>{controlsQuery.data.summary.active ?? 0}</strong><small>{t('controls.status.ACTIVE')}</small></article>
            <article className="metric-card"><span>{t('controls.summary.disabled')}</span><strong>{controlsQuery.data.summary.disabled ?? 0}</strong><small>{t('controls.status.DISABLED')}</small></article>
          </section>

          <section className="panel panel--table" aria-labelledby="controls-list-title">
            <div className="panel__header"><div><h2 id="controls-list-title">{t('controls.list.title')}</h2><span>{visibleControls.length}</span></div><ClipboardCheck size={18} aria-hidden="true" /></div>
            <div className="attention-toolbar" aria-label={t('controls.searchLabel')}>
              <label className="attention-search">
                <span className="visually-hidden">{t('controls.searchLabel')}</span>
                <Search size={16} aria-hidden="true" />
                <input type="search" value={query} onChange={(event) => setQuery(event.currentTarget.value)} placeholder={t('controls.searchPlaceholder')} />
              </label>
              <button className="button button--quiet" type="button" onClick={() => setQuery('')} disabled={!query}>{t('controls.actions.clearSearch')}</button>
            </div>
            {visibleControls.length > 0 ? (
              <div className="table-scroll">
                <table>
                  <thead>
                    <tr>
                      <th>{t('controls.table.control')}</th>
                      <th>{t('controls.table.status')}</th>
                      <th>{t('controls.table.effectiveness')}</th>
                      <th>{t('controls.table.links')}</th>
                      <th>{t('controls.table.updated')}</th>
                      {(canUpdate || canDisable || canAssess || canLink) ? <th>{t('controls.table.actions')}</th> : null}
                    </tr>
                  </thead>
                  <tbody>{visibleControls.map((control) => (
                    <tr key={control.id}>
                      <td>
                        <div className="table-primary">
                          <span>{control.title}</span>
                          <small>
                            {[control.category, control.owner].filter(Boolean).join(' · ') || t('controls.table.unassigned')}
                          </small>
                        </div>
                      </td>
                      <td><span className={`badge badge--${(control.status ?? 'ACTIVE') === 'ACTIVE' ? 'low' : 'medium'}`}>{t(`controls.status.${control.status ?? 'ACTIVE'}`)}</span></td>
                      <td>
                        <span className={control.effectiveness && control.effectiveness !== 'NOT_ASSESSED' ? 'badge badge--low' : 'status-badge'}>
                          {t(`controls.effectiveness.${control.effectiveness ?? 'NOT_ASSESSED'}`)}
                        </span>
                      </td>
                      <td>
                        <span className="control-links">
                          {t('controls.table.linkSummary', {
                            obligations: control.linkedObligationCount ?? 0,
                            risks: control.linkedRiskCount ?? 0,
                            evidence: control.linkedEvidenceCount ?? 0,
                          })}
                        </span>
                      </td>
                      <td><time dateTime={control.updatedAt}>{formatDate(control.updatedAt, language)}</time></td>
                      {(canUpdate || canDisable || canAssess || canLink) ? (
                        <td>
                          <span className="table-actions">
                            {canUpdate ? (
                              <button className="button button--quiet" type="button" disabled={busy} onClick={() => setDraft({
                                controlId: control.id,
                                title: control.title,
                                category: control.category ?? '',
                                owner: control.owner ?? '',
                              })}>
                                {t('controls.actions.edit')}
                              </button>
                            ) : null}
                            {canAssess ? (
                              <button className="button button--quiet" type="button" disabled={busy} onClick={() => {
                                setAssessmentNote('');
                                setEffectiveness(control.effectiveness ?? 'NOT_ASSESSED');
                                setAssessmentTarget(control);
                              }}>
                                {t('controls.actions.assess')}
                              </button>
                            ) : null}
                            {canLink ? (
                              <button className="button button--quiet" type="button" disabled={busy} onClick={() => {
                                setLinkSelection({ obligation: '', risk: '', evidence: '' });
                                setLinksTarget(control);
                              }}>
                                <Link2 size={16} aria-hidden="true" />
                                {t('controls.actions.links')}
                              </button>
                            ) : null}
                            {canDisable && (control.status ?? 'ACTIVE') !== 'DISABLED' ? (
                              <button className="button button--danger" type="button" disabled={busy} onClick={() => disableMutation.mutate({ controlId: control.id, status: 'DISABLED' as ControlStatus })}>
                                {t('controls.actions.disable')}
                              </button>
                            ) : null}
                          </span>
                        </td>
                      ) : null}
                    </tr>
                  ))}</tbody>
                </table>
              </div>
            ) : <EmptyState icon={ClipboardCheck} variant="inline" title={hasFilteredOutControls ? t('controls.noMatches') : t('controls.list.empty')} />}
          </section>
        </>
      ) : null}

      {draft ? (
        <div className="dialog-layer" role="presentation">
          <button className="dialog-backdrop" type="button" aria-label={t('actions.close')} onClick={() => setDraft(null)} />
          <section className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="control-dialog-title">
            <header className="task-dialog__header">
              <div>
                <h2 id="control-dialog-title">{draft.controlId ? t('controls.dialog.editTitle') : t('controls.dialog.createTitle')}</h2>
                <p>{t('controls.dialog.subtitle')}</p>
              </div>
            </header>
            <div className="task-form">
              <label>
                <span>{t('controls.dialog.title')}</span>
                <input value={draft.title} onChange={(event) => setDraft({ ...draft, title: event.currentTarget.value })} />
              </label>
              <div className="task-form__row">
                <label>
                  <span>{t('controls.dialog.category')}</span>
                  <input value={draft.category} onChange={(event) => setDraft({ ...draft, category: event.currentTarget.value })} />
                </label>
                <label>
                  <span>{t('controls.dialog.owner')}</span>
                  <input value={draft.owner} onChange={(event) => setDraft({ ...draft, owner: event.currentTarget.value })} />
                </label>
              </div>
              <div className="task-form__actions">
                <button className="button button--secondary" type="button" onClick={() => setDraft(null)}>{t('controls.actions.cancel')}</button>
                <button className="button button--primary" type="button" disabled={!draft.title.trim() || busy} onClick={saveDraft}>
                  {t('controls.actions.save')}
                </button>
              </div>
            </div>
          </section>
        </div>
      ) : null}

      {assessmentTarget ? (
        <div className="dialog-layer" role="presentation">
          <button className="dialog-backdrop" type="button" aria-label={t('actions.close')} onClick={() => setAssessmentTarget(null)} />
          <section className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="control-assessment-title">
            <header className="task-dialog__header">
              <div>
                <h2 id="control-assessment-title">{t('controls.assessment.title')}</h2>
                <p>{t('controls.assessment.subtitle', { control: assessmentTarget.title })}</p>
              </div>
            </header>
            <div className="task-form">
              <label>
                <span>{t('controls.assessment.effectiveness')}</span>
                <select value={effectiveness} onChange={(event) => setEffectiveness(event.currentTarget.value as ControlEffectiveness)}>
                  {controlEffectivenessValues.map((value) => <option key={value} value={value}>{t(`controls.effectiveness.${value}`)}</option>)}
                </select>
              </label>
              <label>
                <span>{t('controls.assessment.note')}</span>
                <textarea value={assessmentNote} onChange={(event) => setAssessmentNote(event.currentTarget.value)} />
              </label>
              <div className="task-form__actions">
                <button className="button button--secondary" type="button" onClick={() => setAssessmentTarget(null)}>{t('controls.actions.cancel')}</button>
                <button
                  className="button button--primary"
                  type="button"
                  disabled={busy}
                  onClick={() => assessmentMutation.mutate({
                    controlId: assessmentTarget.id,
                    effectiveness,
                    assessmentNote: assessmentNote.trim() || null,
                  })}
                >
                  {t('controls.actions.save')}
                </button>
              </div>
            </div>
          </section>
        </div>
      ) : null}

      {linksTarget ? (
        <div className="dialog-layer" role="presentation">
          <button className="dialog-backdrop" type="button" aria-label={t('actions.close')} onClick={() => setLinksTarget(null)} />
          <section className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="control-links-title">
            <header className="task-dialog__header">
              <div>
                <h2 id="control-links-title">{t('controls.links.title')}</h2>
                <p>{t('controls.links.subtitle', { control: linksTarget.title })}</p>
              </div>
              <Link2 size={18} aria-hidden="true" />
            </header>
            <div className="task-form">
              {linkDetailQuery.isLoading ? <LoadingState label={t('controls.loading')} /> : null}
              {linkDetailQuery.isError ? (
                <ErrorState
                  code={resolveErrorCode(linkDetailQuery.error)}
                  deniedMessage={t('controls.permissionDenied')}
                  fallbackMessage={t('controls.links.loadFailed')}
                />
              ) : null}
              {linkMutation.isError ? (
                <ErrorState code={resolveErrorCode(linkMutation.error)} deniedMessage={t('controls.permissionDenied')} />
              ) : null}

              {linkDetailQuery.data ? (
                <>
                  <LinkSection
                    title={t('controls.links.obligations')}
                    linked={(linkDetailQuery.data.linkedObligations ?? []).map((link) => ({ id: link.obligationId, label: link.title }))}
                    candidates={(obligationsQuery.data?.items ?? []).map((item) => ({ id: item.id, label: item.title }))}
                    selected={linkSelection.obligation}
                    placeholder={t('controls.links.selectPlaceholder')}
                    emptyLabel={t('controls.links.none')}
                    busy={busy}
                    onSelect={(value) => setLinkSelection((current) => ({ ...current, obligation: value }))}
                    onLink={() => linkMutation.mutate({ kind: 'obligation', controlId: linksTarget.id, targetId: linkSelection.obligation, link: true })}
                    onUnlink={(targetId) => linkMutation.mutate({ kind: 'obligation', controlId: linksTarget.id, targetId, link: false })}
                  />
                  <LinkSection
                    title={t('controls.links.risks')}
                    linked={(linkDetailQuery.data.linkedRisks ?? []).map((link) => ({ id: link.riskId, label: link.title }))}
                    candidates={(risksQuery.data?.items ?? []).map((item) => ({ id: item.id, label: item.title }))}
                    selected={linkSelection.risk}
                    placeholder={t('controls.links.selectPlaceholder')}
                    emptyLabel={t('controls.links.none')}
                    busy={busy}
                    onSelect={(value) => setLinkSelection((current) => ({ ...current, risk: value }))}
                    onLink={() => linkMutation.mutate({ kind: 'risk', controlId: linksTarget.id, targetId: linkSelection.risk, link: true })}
                    onUnlink={(targetId) => linkMutation.mutate({ kind: 'risk', controlId: linksTarget.id, targetId, link: false })}
                  />
                  <LinkSection
                    title={t('controls.links.evidence')}
                    linked={(linkDetailQuery.data.linkedEvidence ?? []).map((link) => ({ id: link.evidenceId, label: link.label }))}
                    candidates={(evidenceQuery.data?.items ?? []).map((item) => ({ id: item.id, label: item.label }))}
                    selected={linkSelection.evidence}
                    placeholder={t('controls.links.selectPlaceholder')}
                    emptyLabel={t('controls.links.none')}
                    busy={busy}
                    onSelect={(value) => setLinkSelection((current) => ({ ...current, evidence: value }))}
                    onLink={() => linkMutation.mutate({ kind: 'evidence', controlId: linksTarget.id, targetId: linkSelection.evidence, link: true })}
                    onUnlink={(targetId) => linkMutation.mutate({ kind: 'evidence', controlId: linksTarget.id, targetId, link: false })}
                  />
                </>
              ) : null}

              <div className="task-form__actions">
                <button className="button button--secondary" type="button" onClick={() => setLinksTarget(null)}>{t('controls.actions.cancel')}</button>
              </div>
            </div>
          </section>
        </div>
      ) : null}
    </section>
  );
};
