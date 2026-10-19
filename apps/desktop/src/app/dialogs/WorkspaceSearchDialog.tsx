import { Search, X } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import type { AttentionItem, ObligationSummary } from '@nexus/contracts';

import { useModalFocus } from '../../lib/use-modal-focus';
import { EmptyState } from '../../components/States';
import { navigationItems, type NavigationKey } from '../navigation';

// لوحة البحث العامة — تصفية محلية على بيانات اللوحة المحمّلة، لا بحث خلفي.
export const WorkspaceSearchDialog = ({
  obligations,
  attentionItems,
  onClose,
  onOpenObligation,
  onReviewAttention,
  onNavigate,
}: {
  obligations: readonly ObligationSummary[];
  attentionItems: readonly AttentionItem[];
  onClose: () => void;
  onOpenObligation: (obligation: ObligationSummary) => void;
  onReviewAttention: (item: AttentionItem) => void;
  onNavigate: (key: NavigationKey) => void;
}) => {
  const { t } = useTranslation();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const [query, setQuery] = useState('');
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const matchingModules = navigationItems.filter(({ key }) => !normalizedQuery || t(`navigation.${key}`).toLocaleLowerCase().includes(normalizedQuery));
  const matchingObligations = normalizedQuery ? obligations.filter((item) => `${item.title} ${item.authority} ${item.owner}`.toLocaleLowerCase().includes(normalizedQuery)) : obligations;
  const matchingAttention = normalizedQuery ? attentionItems.filter((item) => `${item.title} ${t(`attention.reason.${item.reasonCode}`)}`.toLocaleLowerCase().includes(normalizedQuery)) : attentionItems;
  const hasResults = matchingModules.length > 0 || matchingObligations.length > 0 || matchingAttention.length > 0;

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="search-dialog" role="dialog" aria-modal="true" aria-labelledby="workspace-search-title" tabIndex={-1}>
        <header><div><h2 id="workspace-search-title">{t('search.title')}</h2><p>{t('search.subtitle')}</p></div><button className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}><X size={16} aria-hidden="true" /></button></header>
        <label className="search-field"><span className="visually-hidden">{t('actions.search')}</span><Search size={16} aria-hidden="true" /><input ref={initialFocusRef} value={query} onChange={(event) => setQuery(event.target.value)} placeholder={t('search.placeholder')} /></label>
        {hasResults ? <div className="search-results">
          {matchingModules.map(({ key }) => <button key={`module-${key}`} type="button" onClick={() => onNavigate(key)}><strong>{t(`navigation.${key}`)}</strong><span>{t('actions.open')}</span></button>)}
          {matchingObligations.map((obligation) => <button key={obligation.id} type="button" onClick={() => onOpenObligation(obligation)}><strong>{obligation.title}</strong><span>{obligation.authority}</span></button>)}
          {matchingAttention.map((item) => <button key={item.id} type="button" onClick={() => onReviewAttention(item)}><strong>{item.title}</strong><span>{t(`attention.reason.${item.reasonCode}`)}</span></button>)}
        </div> : <EmptyState icon={Search} variant="inline" title={t('search.noResults')} />}
      </section>
    </div>
  );
};
