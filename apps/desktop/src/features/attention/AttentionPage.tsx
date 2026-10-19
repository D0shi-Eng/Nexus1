import { useMemo, useRef, useState } from 'react';
import { AlertTriangle, CheckCircle2, ClipboardCheck, FileCheck2, Filter, Search, X } from 'lucide-react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { AttentionAction, AttentionItem, AttentionItemKind, AttentionItemStatus, SeverityLevel } from '@nexus/contracts';
import { PageHeader } from '../../components/PageHeader';
import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { nexusApi } from '../../lib/nexus-api';
import { filterAttentionItems, isAttentionOverdue } from './attention-utils';
import { useModalFocus } from '../../lib/use-modal-focus';

interface AttentionPageProps {
  readonly obligationIds: readonly string[];
  readonly onCreateTask: (obligationId: string) => void;
  readonly onRegisterEvidence: (obligationId: string) => void;
  readonly onDecideApproval: (approvalId: string) => void;
  readonly onOpenObligation: (obligationId: string) => void;
}

type FilterValue<T extends string> = 'ALL' | T;

const formatDate = (value: string | null, language: string): string => {
  if (!value) {
    return '—';
  }

  return new Intl.DateTimeFormat(language, { day: 'numeric', month: 'short', year: 'numeric' }).format(new Date(value));
};

const AttentionBadge = ({ value, type }: { value: string; type: 'severity' | 'status' }) => {
  const { t } = useTranslation();
  return <span className={type === 'severity' ? `badge badge--${value.toLowerCase()}` : 'status-badge'}>{t(`status.${value}`)}</span>;
};

const actionIcon = (action: AttentionAction) => {
  if (action === 'CREATE_TASK') {
    return <ClipboardCheck size={16} aria-hidden="true" />;
  }
  if (action === 'REGISTER_EVIDENCE') {
    return <FileCheck2 size={16} aria-hidden="true" />;
  }
  if (action === 'DECIDE_APPROVAL') {
    return <CheckCircle2 size={16} aria-hidden="true" />;
  }
  return null;
};

export const AttentionPage = ({ obligationIds, onCreateTask, onRegisterEvidence, onDecideApproval, onOpenObligation }: AttentionPageProps) => {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage ?? 'ar';
  const [query, setQuery] = useState('');
  const [severity, setSeverity] = useState<FilterValue<SeverityLevel>>('ALL');
  const [kind, setKind] = useState<FilterValue<AttentionItemKind>>('ALL');
  const [status, setStatus] = useState<FilterValue<AttentionItemStatus>>('ALL');
  const [selectedItem, setSelectedItem] = useState<AttentionItem | null>(null);
  const lastTriggerRef = useRef<HTMLButtonElement | null>(null);
  const attentionQuery = useQuery({
    queryKey: ['attention'],
    queryFn: nexusApi.getAttention,
    staleTime: 30_000,
  });

  // إغلاق التفاصيل وإعادة التركيز إلى زر المشغّل — مصدر واحد يستخدمه الـ Escape والزر والخلفية.
  const closeDetails = (): void => {
    setSelectedItem(null);
    lastTriggerRef.current?.focus();
  };

  // حوار التفاصيل يستخدم نفس مدير التركيز المشترك: احتواء Tab كامل + Escape + استعادة التركيز.
  const { dialogRef, initialFocusRef } = useModalFocus(closeDetails);

  const visibleItems = useMemo(
    () => filterAttentionItems(attentionQuery.data?.items ?? [], { query, severity, kind, status, language }, (key) => t(key)),
    [attentionQuery.data?.items, kind, language, query, severity, status, t],
  );

  const openDetails = (item: AttentionItem, target: HTMLButtonElement): void => {
    lastTriggerRef.current = target;
    setSelectedItem(item);
  };

  const executeAction = (action: AttentionAction, item: AttentionItem): void => {
    if (action === 'CREATE_TASK' && item.relatedEntity?.kind === 'OBLIGATION') {
      onCreateTask(item.relatedEntity.id);
      closeDetails();
      return;
    }
    if (action === 'REGISTER_EVIDENCE' && item.relatedEntity?.kind === 'OBLIGATION') {
      onRegisterEvidence(item.relatedEntity.id);
      closeDetails();
      return;
    }
    if (action === 'DECIDE_APPROVAL') {
      onDecideApproval(item.resourceId);
      closeDetails();
    }
  };

  const canOpenRelatedObligation = (item: AttentionItem): boolean => item.relatedEntity?.kind === 'OBLIGATION' && obligationIds.includes(item.relatedEntity.id);
  // صلاحية بلا مسار تنفيذ في الواجهة (أو بلا كيان مرتبط تحدده) لا تُعرض كزر:
  // زر مفعّل لا يفعل شيئاً أسوأ من غياب الزر لأنه يوهم بإجراء متاح.
  const executableActions = (item: AttentionItem): readonly AttentionAction[] =>
    item.availableActions.filter((action) => {
      if (action === 'OPEN' || action === 'REVIEW') {
        return false;
      }
      if (action === 'CREATE_TASK' || action === 'REGISTER_EVIDENCE') {
        return item.relatedEntity?.kind === 'OBLIGATION';
      }
      return action === 'DECIDE_APPROVAL';
    });
  const errorCode = resolveErrorCode(attentionQuery.error);

  return (
    <section id="attention" className="attention-page content-container" aria-labelledby="attention-page-title" aria-live="polite">
      <PageHeader
        eyebrow={t('attention.eyebrow')}
        eyebrowIcon={AlertTriangle}
        title={t('attention.title')}
        titleId="attention-page-title"
        description={t('attention.subtitle')}
        actions={attentionQuery.data?.dataMode === 'DEMO_SYNTHETIC' ? <span className="demo-label attention-page__demo">{t('product.demo')}</span> : null}
      />

      {attentionQuery.isLoading ? <LoadingState label={t('attention.loading')} /> : null}
      {attentionQuery.isError ? (
        <ErrorState code={errorCode} deniedMessage={t('attention.permissionDenied')} />
      ) : null}

      {attentionQuery.data ? <>
        <section className="attention-summary" aria-label={t('attention.summaryLabel')}>
          {([
            ['total', 'attention.summary.total'],
            ['critical', 'attention.summary.critical'],
            ['high', 'attention.summary.high'],
            ['medium', 'attention.summary.medium'],
            ['low', 'attention.summary.low'],
            ['pending', 'attention.summary.pending'],
            ['overdue', 'attention.summary.overdue'],
            ['requiresReview', 'attention.summary.requiresReview'],
          ] as const).map(([key, label]) => <article className="attention-summary__card" key={key}><span>{t(label)}</span><strong>{attentionQuery.data.summary[key]}</strong></article>)}
        </section>

        <section className="attention-toolbar panel" aria-label={t('attention.filters.title')}>
          <div className="attention-toolbar__heading"><Filter size={16} aria-hidden="true" /><h2>{t('attention.filters.title')}</h2></div>
          <label className="attention-search"><span className="visually-hidden">{t('actions.search')}</span><Search size={16} aria-hidden="true" /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t('attention.searchPlaceholder')} /></label>
          <label><span>{t('attention.filters.priority')}</span><select value={severity} onChange={(event) => setSeverity(event.target.value as FilterValue<SeverityLevel>)}><option value="ALL">{t('attention.filters.all')}</option>{(['CRITICAL', 'HIGH', 'MEDIUM', 'LOW'] as const).map((value) => <option value={value} key={value}>{t(`status.${value}`)}</option>)}</select></label>
          <label><span>{t('attention.filters.type')}</span><select value={kind} onChange={(event) => setKind(event.target.value as FilterValue<AttentionItemKind>)}><option value="ALL">{t('attention.filters.all')}</option>{(['OBLIGATION', 'EVIDENCE', 'RISK', 'TASK', 'APPROVAL'] as const).map((value) => <option value={value} key={value}>{t(`attention.kind.${value}`)}</option>)}</select></label>
          <label><span>{t('attention.filters.status')}</span><select value={status} onChange={(event) => setStatus(event.target.value as FilterValue<AttentionItemStatus>)}><option value="ALL">{t('attention.filters.all')}</option>{(['OPEN', 'IN_PROGRESS', 'AT_RISK', 'REQUIRES_REVIEW', 'MISSING', 'EXPIRED', 'INVALID', 'BLOCKED', 'MITIGATING', 'PENDING'] as const).map((value) => <option value={value} key={value}>{t(`status.${value}`)}</option>)}</select></label>
        </section>

        <section className="attention-results" aria-label={t('attention.itemsLabel', { count: visibleItems.length })}>
          {visibleItems.length === 0 ? <EmptyState icon={CheckCircle2} variant="inline" title={attentionQuery.data.items.length === 0 ? t('attention.empty') : t('attention.noMatches')} /> : visibleItems.map((item) => (
            <article className="attention-card" key={item.id}>
              <header className="attention-card__header">
                <div className="attention-card__badges"><AttentionBadge value={item.severity} type="severity" /><AttentionBadge value={item.status} type="status" />{isAttentionOverdue(item.dueAt) ? <span className="status-badge attention-card__overdue">{t('attention.overdue')}</span> : null}</div>
                <span className="attention-card__kind">{t(`attention.kind.${item.kind}`)}</span>
              </header>
              <h2>{item.title}</h2>
              <p>{t(`attention.reason.${item.reasonCode}`)}</p>
              <dl className="attention-card__metadata">
                <div><dt>{t('attention.relatedEntity')}</dt><dd>{item.relatedEntity ? `${t(`attention.kind.${item.relatedEntity.kind}`)} · ${item.relatedEntity.title}` : '—'}</dd></div>
                <div><dt>{t('table.owner')}</dt><dd>{item.owner ?? '—'}</dd></div>
                <div><dt>{t('table.deadline')}</dt><dd>{formatDate(item.dueAt, language)}</dd></div>
                <div><dt>{t('table.updated')}</dt><dd>{formatDate(item.updatedAt, language)}</dd></div>
              </dl>
              <footer className="attention-card__actions">
                <button className="button button--secondary" type="button" onClick={(event) => openDetails(item, event.currentTarget)}>{t('actions.review')}</button>
                {executableActions(item).map((action) => <button className="button button--primary" type="button" key={action} onClick={() => executeAction(action, item)}>{actionIcon(action)} {t(`attention.action.${action}`)}</button>)}
              </footer>
            </article>
          ))}
        </section>
      </> : null}

      {selectedItem ? <div className="dialog-layer" role="presentation">
        <button className="dialog-backdrop" type="button" onClick={closeDetails} aria-label={t('actions.close')} />
        <section ref={dialogRef} className="task-dialog attention-details" role="dialog" aria-modal="true" aria-labelledby="attention-details-title">
          <header className="task-dialog__header"><div><h2 id="attention-details-title">{selectedItem.title}</h2><p>{t(`attention.reason.${selectedItem.reasonCode}`)}</p></div><button ref={initialFocusRef} className="icon-button" type="button" onClick={closeDetails} aria-label={t('actions.close')}><X size={18} aria-hidden="true" /></button></header>
          <dl className="details-list">
            <div><dt>{t('attention.type')}</dt><dd>{t(`attention.kind.${selectedItem.kind}`)}</dd></div>
            <div><dt>{t('table.status')}</dt><dd><AttentionBadge value={selectedItem.status} type="status" /></dd></div>
            <div><dt>{t('attention.reasonLabel')}</dt><dd>{t(`attention.reason.${selectedItem.reasonCode}`)}</dd></div>
            <div><dt>{t('attention.source')}</dt><dd>{t('attention.localSource')}</dd></div>
            <div><dt>{t('attention.relatedEntity')}</dt><dd>{selectedItem.relatedEntity?.title ?? '—'}</dd></div>
            <div><dt>{t('table.owner')}</dt><dd>{selectedItem.owner ?? '—'}</dd></div>
            <div><dt>{t('table.deadline')}</dt><dd>{formatDate(selectedItem.dueAt, language)}</dd></div>
            <div><dt>{t('table.updated')}</dt><dd>{formatDate(selectedItem.updatedAt, language)}</dd></div>
          </dl>
          <footer className="task-form__actions attention-details__actions">
            {canOpenRelatedObligation(selectedItem) ? <button className="button button--secondary" type="button" onClick={() => { onOpenObligation(selectedItem.relatedEntity!.id); closeDetails(); }}>{t('attention.action.OPEN')}</button> : null}
            {executableActions(selectedItem).map((action) => <button className="button button--primary" type="button" key={action} onClick={() => executeAction(action, selectedItem)}>{actionIcon(action)} {t(`attention.action.${action}`)}</button>)}
          </footer>
        </section>
      </div> : null}
    </section>
  );
};
