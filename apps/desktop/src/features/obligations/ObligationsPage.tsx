import { useMemo, useRef, useState, type FormEvent } from 'react';
import { Archive, BadgeCheck, CheckCircle2, ChevronRight, ClipboardCheck, FileCheck2, Filter, LoaderCircle, Pencil, Plus, Search, ShieldAlert, X } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { ComplianceStatus, ObligationDetails, ObligationListItem, ObligationPriority, ObligationStatus, ReviewStatus, SourceType } from '@nexus/contracts';
import { complianceStatuses, obligationPriorities, obligationStatuses, reviewStatuses, sourceTypes } from '@nexus/contracts';
import { PageHeader } from '../../components/PageHeader';
import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { NexusApiError, nexusApi, type UpdateObligationInput } from '../../lib/nexus-api';
import { useModalFocus } from '../../lib/use-modal-focus';
import { useSession } from '../../lib/use-session';
import { filterAndSortObligations, getObligationOwners, type DueState, type ObligationFilters, type ObligationSort } from './obligation-utils';
import { CreateObligationDialog } from './CreateObligationDialog';

interface ObligationsPageProps {
  readonly onCreateTask: (obligationId: string) => void;
  readonly onRegisterEvidence: (obligationId: string) => void;
}

type FilterValue<T extends string> = 'ALL' | T;

const formatDate = (value: string | null, language: string): string => {
  if (!value) {
    return '—';
  }
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return '—';
  }
  return new Intl.DateTimeFormat(language, { day: 'numeric', month: 'short', year: 'numeric' }).format(parsed);
};

const toDateInput = (value: string | null): string => value?.slice(0, 10) ?? '';

const ObligationBadge = ({ value, priority = false }: { value: string; priority?: boolean }) => {
  const { t } = useTranslation();
  return <span className={priority ? `badge badge--${value.toLowerCase()}` : 'status-badge'}>{t(`status.${value}`)}</span>;
};

const RelatedList = ({
  label,
  items,
  render,
}: {
  readonly label: string;
  readonly items: readonly { readonly id: string }[];
  readonly render: (item: { readonly id: string }) => React.ReactNode;
}) => (
  <section className="obligation-related" aria-label={label}>
    <h3>{label}</h3>
    {items.length === 0 ? <p className="form-note">—</p> : <ul>{items.map((item) => <li key={item.id}>{render(item)}</li>)}</ul>}
  </section>
);

const EditObligationForm = ({
  obligation,
  onCancel,
  onSaved,
}: {
  readonly obligation: ObligationDetails;
  readonly onCancel: () => void;
  readonly onSaved: () => void;
}) => {
  const { t } = useTranslation();
  const [title, setTitle] = useState(obligation.title);
  const [description, setDescription] = useState(obligation.description);
  const [owner, setOwner] = useState(obligation.owner);
  const [dueDate, setDueDate] = useState(toDateInput(obligation.dueDate));
  const [priority, setPriority] = useState<ObligationPriority>(obligation.priority);
  const [status, setStatus] = useState<ObligationStatus>(obligation.status);
  const [complianceStatus, setComplianceStatus] = useState<ComplianceStatus>(obligation.complianceStatus);
  const [reviewStatus, setReviewStatus] = useState<ReviewStatus>(obligation.reviewStatus);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const updateMutation = useMutation({ mutationFn: nexusApi.updateObligation });

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setErrorCode(null);
    const input: UpdateObligationInput = {
      obligationId: obligation.id,
      title,
      description,
      owner,
      dueDate: dueDate ? `${dueDate}T00:00:00Z` : null,
      priority,
      status,
      complianceStatus,
      reviewStatus,
      expectedVersion: obligation.version,
    };
    try {
      await updateMutation.mutateAsync(input);
      onSaved();
    } catch (error) {
      setErrorCode(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  return (
    <form className="task-form obligation-edit-form" onSubmit={submit}>
      <label><span>{t('obligations.edit.titleLabel')}</span><input value={title} onChange={(event) => setTitle(event.target.value)} maxLength={240} required /></label>
      <label><span>{t('obligations.edit.descriptionLabel')}</span><textarea value={description} onChange={(event) => setDescription(event.target.value)} maxLength={4000} rows={5} /></label>
      <div className="task-form__row">
        <label><span>{t('obligations.edit.ownerLabel')}</span><input value={owner} onChange={(event) => setOwner(event.target.value)} maxLength={160} required /></label>
        <label><span>{t('obligations.edit.dueDateLabel')}</span><input type="date" value={dueDate} onChange={(event) => setDueDate(event.target.value)} /></label>
      </div>
      <div className="obligations-edit-grid">
        <label><span>{t('obligations.filters.priority')}</span><select value={priority} onChange={(event) => setPriority(event.target.value as ObligationPriority)}>{obligationPriorities.map((value) => <option value={value} key={value}>{t(`status.${value}`)}</option>)}</select></label>
        <label><span>{t('obligations.filters.status')}</span><select value={status} onChange={(event) => setStatus(event.target.value as ObligationStatus)}>{obligationStatuses.map((value) => <option value={value} key={value}>{t(`status.${value}`)}</option>)}</select></label>
        <label><span>{t('obligations.filters.compliance')}</span><select value={complianceStatus} onChange={(event) => setComplianceStatus(event.target.value as ComplianceStatus)}>{complianceStatuses.map((value) => <option value={value} key={value}>{t(`status.${value}`)}</option>)}</select></label>
        <label><span>{t('obligations.details.review')}</span><select value={reviewStatus} onChange={(event) => setReviewStatus(event.target.value as ReviewStatus)}>{reviewStatuses.map((value) => <option value={value} key={value}>{t(`status.${value}`)}</option>)}</select></label>
      </div>
      {errorCode ? <p className="form-error" role="alert">{t(`errors.${errorCode}`, { defaultValue: t('obligations.edit.failure') })}</p> : null}
      <footer className="task-form__actions">
        <button className="button button--secondary" type="button" onClick={onCancel} disabled={updateMutation.isPending}>{t('actions.cancel')}</button>
        <button className="button button--primary" type="submit" disabled={updateMutation.isPending}>{updateMutation.isPending ? <LoaderCircle className="spin" size={16} /> : <CheckCircle2 size={16} />}{t('obligations.edit.save')}</button>
      </footer>
    </form>
  );
};

const ObligationDetailsDialog = ({
  obligationId,
  onClose,
  onCreateTask,
  onRegisterEvidence,
}: {
  readonly obligationId: string;
  readonly onClose: () => void;
  readonly onCreateTask: (obligationId: string) => void;
  readonly onRegisterEvidence: (obligationId: string) => void;
}) => {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const { can } = useSession();
  // مدير التركيز المشترك يحصر Tab وShift+Tab فعلياً ويعيد التركيز عند الإغلاق؛
  // النسخة المحلية السابقة كانت تحرس الطرفين فقط فيتسرب التركيز إلى خلفية الصفحة.
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const [isEditing, setIsEditing] = useState(false);
  const [archiveError, setArchiveError] = useState<string | null>(null);
  const detailQuery = useQuery({ queryKey: ['obligation', obligationId], queryFn: () => nexusApi.getObligation(obligationId), staleTime: 30_000 });
  const archiveMutation = useMutation({ mutationFn: nexusApi.archiveObligation });
  const verifyMutation = useMutation({ mutationFn: nexusApi.verifyObligation });
  const language = i18n.resolvedLanguage ?? 'ar';

  const refreshQueries = async (): Promise<void> => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ['obligations'] }),
      queryClient.invalidateQueries({ queryKey: ['obligation', obligationId] }),
      queryClient.invalidateQueries({ queryKey: ['dashboard'] }),
      queryClient.invalidateQueries({ queryKey: ['attention'] }),
    ]);
  };

  const archive = async (): Promise<void> => {
    if (!detailQuery.data) {
      return;
    }
    setArchiveError(null);
    try {
      await archiveMutation.mutateAsync({ obligationId, expectedVersion: detailQuery.data.version });
      await refreshQueries();
      onClose();
    } catch (error) {
      setArchiveError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  // 0025: التحقق البشري — خطوة مستقلة تملكها جهة المراجعة وحدها.
  const verify = async (): Promise<void> => {
    if (!detailQuery.data) {
      return;
    }
    setArchiveError(null);
    try {
      await verifyMutation.mutateAsync({ obligationId, expectedVersion: detailQuery.data.version });
      await refreshQueries();
    } catch (error) {
      setArchiveError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  const details = detailQuery.data;
  const errorCode = detailQuery.error instanceof NexusApiError ? detailQuery.error.code : 'LOCAL_SERVICE_UNAVAILABLE';

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="task-dialog obligation-details-dialog" role="dialog" aria-modal="true" aria-labelledby="obligation-details-title" tabIndex={-1}>
        <header className="task-dialog__header">
          <div><h2 id="obligation-details-title">{isEditing ? t('obligations.edit.title') : t('details.obligationTitle')}</h2><p>{isEditing ? t('obligations.edit.subtitle') : details?.title ?? t('obligations.loading')}</p></div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}><X size={18} /></button>
        </header>
        {detailQuery.isLoading ? <div className="loading-panel" role="status"><LoaderCircle className="spin" size={18} aria-hidden="true" /><span>{t('obligations.loading')}</span></div> : null}
        {detailQuery.isError ? <div className="error-panel" role="alert"><ShieldAlert size={20} aria-hidden="true" /><p>{t(`errors.${errorCode}`, { defaultValue: t('errors.unavailable') })}</p></div> : null}
        {details && isEditing ? <EditObligationForm obligation={details} onCancel={() => setIsEditing(false)} onSaved={() => { setIsEditing(false); void refreshQueries(); }} /> : null}
        {details && !isEditing ? <>
          <dl className="details-list obligation-details-list">
            <div><dt>{t('obligations.details.description')}</dt><dd>{details.description || '—'}</dd></div>
            <div><dt>{t('table.status')}</dt><dd><ObligationBadge value={details.status} /></dd></div>
            <div><dt>{t('obligations.details.priority')}</dt><dd><ObligationBadge value={details.priority} priority /></dd></div>
            <div><dt>{t('obligations.details.compliance')}</dt><dd><ObligationBadge value={details.complianceStatus} /></dd></div>
            <div><dt>{t('obligations.details.review')}</dt><dd><ObligationBadge value={details.reviewStatus} /></dd></div>
            <div><dt>{t('table.owner')}</dt><dd>{details.owner}</dd></div>
            <div><dt>{t('table.deadline')}</dt><dd>{formatDate(details.dueDate, language)}</dd></div>
            <div><dt>{t('details.source')}</dt><dd>{details.source.authority} · {details.source.documentTitle}</dd></div>
            <div><dt>{t('obligations.details.sourceType')}</dt><dd><ObligationBadge value={details.sourceType} /></dd></div>
            <div><dt>{t('obligations.details.sourceReference')}</dt><dd>{details.sourceReference}</dd></div>
            <div><dt>{t('obligations.details.dataMode')}</dt><dd><ObligationBadge value={details.dataMode} /></dd></div>
            <div><dt>{t('obligations.details.created')}</dt><dd>{formatDate(details.createdAt, language)}</dd></div>
            <div><dt>{t('table.updated')}</dt><dd>{formatDate(details.updatedAt, language)}</dd></div>
            <div><dt>{t('obligations.details.version')}</dt><dd>{details.version}</dd></div>
          </dl>
          <section className="obligation-related-grid" aria-labelledby="obligation-resources-title">
            <h3 id="obligation-resources-title">{t('obligations.details.relatedResources')}</h3>
            <RelatedList label={t('obligations.details.evidence')} items={details.relatedResources.evidence} render={(item) => { const reference = details.relatedResources.evidence.find((value) => value.id === item.id)!; return <><strong>{reference.label}</strong><span>{t(`status.${reference.status}`)}</span></>; }} />
            <RelatedList label={t('obligations.details.tasks')} items={details.relatedResources.tasks} render={(item) => { const reference = details.relatedResources.tasks.find((value) => value.id === item.id)!; return <><strong>{reference.title}</strong><span>{t(`status.${reference.status}`)}</span></>; }} />
            <RelatedList label={t('obligations.details.risks')} items={details.relatedResources.risks} render={(item) => { const reference = details.relatedResources.risks.find((value) => value.id === item.id)!; return <><strong>{reference.title}</strong><span>{t(`status.${reference.status}`)}</span></>; }} />
            <RelatedList label={t('obligations.details.approvals')} items={details.relatedResources.approvals} render={(item) => { const reference = details.relatedResources.approvals.find((value) => value.id === item.id)!; return <><strong>{reference.requiredRoleCode}</strong><span>{t(`status.${reference.decision}`)}</span></>; }} />
          </section>
          {archiveError ? <p className="form-error" role="alert">{t(`errors.${archiveError}`, { defaultValue: t('obligations.archive.failure') })}</p> : null}
          <footer className="task-form__actions obligation-details-actions">
            {can('obligation:update') ? (
              <button className="button button--secondary" type="button" onClick={() => setIsEditing(true)}><Pencil size={16} />{t('actions.edit')}</button>
            ) : null}
            {can('evidence:create') ? (
              <button className="button button--secondary" type="button" onClick={() => onRegisterEvidence(details.id)}><FileCheck2 size={16} />{t('actions.registerEvidence')}</button>
            ) : null}
            {can('task:create') ? (
              <button className="button button--primary" type="button" onClick={() => onCreateTask(details.id)}><ClipboardCheck size={16} />{t('actions.createTask')}</button>
            ) : null}
            {/* التحقق البشري: صلاحية مستقلة عن التحديث — منفّذ العمل لا يتحقق من عمله. */}
            {can('obligation:verify') ? (
              <button className="button button--secondary" type="button" onClick={() => { void verify(); }} disabled={verifyMutation.isPending}><BadgeCheck size={16} />{t('actions.verifyObligation')}</button>
            ) : null}
            {can('obligation:archive') ? (
              <button className="button button--danger" type="button" onClick={() => { void archive(); }} disabled={archiveMutation.isPending}>{archiveMutation.isPending ? <LoaderCircle className="spin" size={16} /> : <Archive size={16} />}{t('actions.archive')}</button>
            ) : null}
          </footer>
        </> : null}
      </section>
    </div>
  );
};

export const ObligationsPage = ({ onCreateTask, onRegisterEvidence }: ObligationsPageProps) => {
  const { t, i18n } = useTranslation();
  const { can } = useSession();
  const queryClient = useQueryClient();
  const language = i18n.resolvedLanguage ?? 'ar';
  const [filters, setFilters] = useState<ObligationFilters>({ query: '', status: 'ALL', priority: 'ALL', complianceStatus: 'ALL', sourceType: 'ALL', owner: 'ALL', dueState: 'ALL', sort: 'PRIORITY' });
  const [selectedObligationId, setSelectedObligationId] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const openTriggerRef = useRef<HTMLButtonElement | null>(null);
  const obligationsQuery = useQuery({ queryKey: ['obligations'], queryFn: nexusApi.getObligations, staleTime: 30_000 });
  // المصادر التنظيمية تُقرأ فقط عند فتح حوار التأليف — لا طلب زائد في كل تحميل.
  const canCreate = can('obligation:create');
  const sourcesQuery = useQuery({
    queryKey: ['regulatory-sources'],
    queryFn: nexusApi.getRegulatorySources,
    staleTime: 30_000,
    enabled: createOpen && canCreate,
  });
  const owners = useMemo(() => getObligationOwners(obligationsQuery.data?.items ?? []), [obligationsQuery.data?.items]);
  const visibleItems = useMemo(() => filterAndSortObligations(obligationsQuery.data?.items ?? [], filters), [filters, obligationsQuery.data?.items]);
  // رمز الخطأ من المصدر المشترك — لا فرع `instanceof` محلي.
  const errorCode = resolveErrorCode(obligationsQuery.error);
  const creatableSources = sourcesQuery.data?.items ?? [];
  // التأليف مسار إنتاجي: بيانات الديمو تركيبية معلنة ولا تُضاف إليها سجلات.
  const showCreate = canCreate && obligationsQuery.data?.dataMode === 'PRODUCTION';

  const openCreate = (): void => {
    setFeedback(null);
    setCreateOpen(true);
  };

  const handleCreated = (obligationId: string): void => {
    setFeedback(t('feedback.obligationCreated'));
    void queryClient.invalidateQueries({ queryKey: ['obligations'] });
    void queryClient.invalidateQueries({ queryKey: ['regulatory-sources'] });
    setSelectedObligationId(obligationId);
  };

  const createButton = showCreate ? (
    <button className="button button--primary" type="button" onClick={openCreate}>
      <Plus size={16} aria-hidden="true" />
      {t('obligations.create.open')}
    </button>
  ) : null;

  const updateFilter = <T extends keyof ObligationFilters>(key: T, value: ObligationFilters[T]): void => {
    setFilters((current) => ({ ...current, [key]: value }));
  };

  const openDetails = (item: ObligationListItem, target: HTMLButtonElement): void => {
    openTriggerRef.current = target;
    setSelectedObligationId(item.id);
  };

  const closeDetails = (): void => {
    setSelectedObligationId(null);
    openTriggerRef.current?.focus();
  };

  return (
    <section id="obligations" className="obligations-page content-container" aria-labelledby="obligations-page-title" aria-live="polite">
      <PageHeader
        eyebrow={t('obligations.eyebrow')}
        eyebrowIcon={ClipboardCheck}
        title={t('obligations.title')}
        titleId="obligations-page-title"
        description={t('obligations.subtitle')}
        actions={obligationsQuery.data?.dataMode === 'DEMO_SYNTHETIC' ? <span className="demo-label attention-page__demo">{t('product.demo')}</span> : createButton}
      />
      {feedback ? <p className="form-note" role="status">{feedback}</p> : null}
      {obligationsQuery.isLoading ? <LoadingState label={t('obligations.loading')} /> : null}
      {obligationsQuery.isError ? <ErrorState code={errorCode} deniedMessage={t('obligations.permissionDenied')} /> : null}
      {obligationsQuery.data ? <>
        <section className="attention-summary obligations-summary" aria-label={t('obligations.summaryLabel')}>
          {(['total', 'critical', 'dueSoon', 'overdue', 'nonCompliant', 'underReview'] as const).map((key) => <article className="attention-summary__card" key={key}><span>{t(`obligations.summary.${key}`)}</span><strong>{obligationsQuery.data!.summary[key]}</strong></article>)}
        </section>
        <section className="attention-toolbar obligations-toolbar panel" aria-label={t('obligations.filters.title')}>
          <div className="attention-toolbar__heading"><Filter size={16} aria-hidden="true" /><h2>{t('obligations.filters.title')}</h2></div>
          <label className="attention-search"><span className="visually-hidden">{t('actions.search')}</span><Search size={16} aria-hidden="true" /><input value={filters.query} onChange={(event) => updateFilter('query', event.target.value)} placeholder={t('obligations.searchPlaceholder')} /></label>
          <label><span>{t('obligations.filters.status')}</span><select value={filters.status} onChange={(event) => updateFilter('status', event.target.value as FilterValue<ObligationStatus>)}><option value="ALL">{t('obligations.filters.all')}</option>{obligationStatuses.map((value) => <option key={value} value={value}>{t(`status.${value}`)}</option>)}</select></label>
          <label><span>{t('obligations.filters.priority')}</span><select value={filters.priority} onChange={(event) => updateFilter('priority', event.target.value as FilterValue<ObligationPriority>)}><option value="ALL">{t('obligations.filters.all')}</option>{obligationPriorities.map((value) => <option key={value} value={value}>{t(`status.${value}`)}</option>)}</select></label>
          <label><span>{t('obligations.filters.compliance')}</span><select value={filters.complianceStatus} onChange={(event) => updateFilter('complianceStatus', event.target.value as FilterValue<ComplianceStatus>)}><option value="ALL">{t('obligations.filters.all')}</option>{complianceStatuses.map((value) => <option key={value} value={value}>{t(`status.${value}`)}</option>)}</select></label>
          <label><span>{t('obligations.filters.sourceType')}</span><select value={filters.sourceType} onChange={(event) => updateFilter('sourceType', event.target.value as FilterValue<SourceType>)}><option value="ALL">{t('obligations.filters.all')}</option>{sourceTypes.map((value) => <option key={value} value={value}>{t(`status.${value}`)}</option>)}</select></label>
          <label><span>{t('obligations.filters.owner')}</span><select value={filters.owner} onChange={(event) => updateFilter('owner', event.target.value)}><option value="ALL">{t('obligations.filters.all')}</option>{owners.map((value) => <option key={value} value={value}>{value}</option>)}</select></label>
          <label><span>{t('obligations.filters.dueState')}</span><select value={filters.dueState} onChange={(event) => updateFilter('dueState', event.target.value as DueState)}>{(['ALL', 'OVERDUE', 'DUE_SOON', 'FUTURE', 'NO_DUE_DATE'] as const).map((value) => <option key={value} value={value}>{t(`obligations.dueState.${value}`)}</option>)}</select></label>
          <label><span>{t('obligations.filters.sort')}</span><select value={filters.sort} onChange={(event) => updateFilter('sort', event.target.value as ObligationSort)}>{(['PRIORITY', 'DUE_DATE', 'UPDATED_AT', 'CREATED_AT', 'STATUS'] as const).map((value) => <option key={value} value={value}>{t(`obligations.sort.${value}`)}</option>)}</select></label>
        </section>
        <section className="obligation-results" aria-label={t('obligations.itemsLabel', { count: visibleItems.length })}>
          {visibleItems.length === 0 ? <EmptyState icon={CheckCircle2} variant="inline" title={obligationsQuery.data.items.length === 0 ? t('obligations.empty') : t('obligations.noMatches')} description={obligationsQuery.data.items.length === 0 ? t('obligations.emptyHint') : undefined} actions={obligationsQuery.data.items.length === 0 ? createButton : undefined} /> : <div className="table-scroll obligations-table"><table><thead><tr><th>{t('table.obligation')}</th><th>{t('table.owner')}</th><th>{t('obligations.details.priority')}</th><th>{t('obligations.details.compliance')}</th><th>{t('table.deadline')}</th><th>{t('table.updated')}</th><th><span className="visually-hidden">{t('table.action')}</span></th></tr></thead><tbody>{visibleItems.map((item) => <tr key={item.id}><td><div className="table-primary"><span>{item.title}</span><small>{item.source.authority}</small></div></td><td>{item.owner}</td><td><ObligationBadge value={item.priority} priority /></td><td><ObligationBadge value={item.complianceStatus} /></td><td><time dateTime={item.dueDate ?? undefined}>{formatDate(item.dueDate, language)}</time></td><td><time dateTime={item.updatedAt}>{formatDate(item.updatedAt, language)}</time></td><td><span className="table-actions"><button className="button button--quiet" type="button" onClick={(event) => openDetails(item, event.currentTarget)}>{t('actions.open')}<ChevronRight size={16} aria-hidden="true" /></button></span></td></tr>)}</tbody></table></div>}
        </section>
      </> : null}
      {selectedObligationId ? <ObligationDetailsDialog obligationId={selectedObligationId} onClose={closeDetails} onCreateTask={(id) => { closeDetails(); onCreateTask(id); }} onRegisterEvidence={(id) => { closeDetails(); onRegisterEvidence(id); }} /> : null}
      {createOpen ? (
        <CreateObligationDialog
          sources={creatableSources}
          onClose={() => setCreateOpen(false)}
          onCreated={handleCreated}
        />
      ) : null}
    </section>
  );
};
