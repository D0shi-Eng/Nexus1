import { ClipboardCheck, FileCheck2, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { AttentionItem, ObligationSummary } from '@nexus/contracts';

import { useModalFocus } from '../../lib/use-modal-focus';
import { formatDate } from '../../lib/format';
import { SeverityBadge, StatusBadge } from '../badges';

// حوار تفاصيل الالتزام — عرض فقط، وأزرار الإجراءات تمرر إلى سير العمل العام.
export const ObligationDetailsDialog = ({
  obligation,
  onClose,
  onCreateTask,
  onRegisterEvidence,
}: {
  obligation: ObligationSummary;
  onClose: () => void;
  onCreateTask: (obligationId: string) => void;
  onRegisterEvidence: (obligationId: string) => void;
}) => {
  const { t, i18n } = useTranslation();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="obligation-details-title" tabIndex={-1}>
        <header className="task-dialog__header">
          <div><h2 id="obligation-details-title">{t('details.obligationTitle')}</h2><p>{obligation.title}</p></div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}><X size={18} aria-hidden="true" /></button>
        </header>
        <dl className="details-list">
          <div><dt>{t('details.authority')}</dt><dd>{obligation.authority}</dd></div>
          <div><dt>{t('table.owner')}</dt><dd>{obligation.owner}</dd></div>
          <div><dt>{t('table.deadline')}</dt><dd>{formatDate(obligation.deadline, i18n.language)}</dd></div>
          <div><dt>{t('table.status')}</dt><dd><StatusBadge value={obligation.status} /></dd></div>
          <div><dt>{t('table.evidence')}</dt><dd><StatusBadge value={obligation.evidenceStatus} /></dd></div>
          <div><dt>{t('details.verification')}</dt><dd><StatusBadge value={obligation.verificationState} /></dd></div>
          <div><dt>{t('details.applicability')}</dt><dd><StatusBadge value={obligation.applicability} /></dd></div>
          <div><dt>{t('details.source')}</dt><dd>{obligation.source.documentTitle} · {obligation.source.version ?? '—'}</dd></div>
        </dl>
        <footer className="task-form__actions">
          <button className="button button--secondary" type="button" onClick={() => onRegisterEvidence(obligation.id)}><FileCheck2 size={16} aria-hidden="true" /> {t('actions.registerEvidence')}</button>
          <button className="button button--primary" type="button" onClick={() => onCreateTask(obligation.id)}><ClipboardCheck size={16} aria-hidden="true" /> {t('actions.createTask')}</button>
        </footer>
      </section>
    </div>
  );
};

// حوار تفاصيل عنصر الانتباه — يربط العنصر بأصله عند توفره في بيانات اللوحة الحالية.
export const AttentionDetailsDialog = ({
  item,
  obligation,
  onClose,
  onOpenObligation,
}: {
  item: AttentionItem;
  obligation: ObligationSummary | undefined;
  onClose: () => void;
  onOpenObligation: (selected: ObligationSummary) => void;
}) => {
  const { t, i18n } = useTranslation();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="attention-details-title" tabIndex={-1}>
        <header className="task-dialog__header">
          <div><h2 id="attention-details-title">{t('details.attentionTitle')}</h2><p>{item.title}</p></div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}><X size={18} aria-hidden="true" /></button>
        </header>
        <dl className="details-list">
          <div><dt>{t('table.status')}</dt><dd><SeverityBadge severity={item.severity} /></dd></div>
          <div><dt>{t('table.deadline')}</dt><dd>{formatDate(item.dueAt, i18n.language)}</dd></div>
          <div><dt>{t('attention.reasonLabel')}</dt><dd>{t(`attention.reason.${item.reasonCode}`)}</dd></div>
        </dl>
        {obligation ? (
          <footer className="task-form__actions">
            <button className="button button--primary" type="button" onClick={() => onOpenObligation(obligation)}>{t('actions.viewDetails')}</button>
          </footer>
        ) : <p className="form-note">{t('details.noMatchingObligation')}</p>}
      </section>
    </div>
  );
};
