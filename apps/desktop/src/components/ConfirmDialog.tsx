import { CircleAlert, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { useModalFocus } from '../lib/use-modal-focus';

// حوار تأكيد موحّد للإجراءات غير القابلة للتراجع.
// السبب: `window.confirm` لا يُنفَّذ في WebView2/Tauri تحت سياسة القدرات،
// فيتحول إلى استدعاء مرفوض لا يعرض شيئاً — أي إجراء «مؤكد» بلا تأكيد فعلي.
// هذا المكوّن يحافظ على نفس الحماية ويستخدم نظام التصميم وحصر التركيز نفسه.
export const ConfirmDialog = ({
  title,
  message,
  confirmLabel,
  cancelLabel,
  tone = 'danger',
  onConfirm,
  onCancel,
}: {
  readonly title: string;
  readonly message: string;
  readonly confirmLabel: string;
  readonly cancelLabel: string;
  readonly tone?: 'danger' | 'primary';
  readonly onConfirm: () => void;
  readonly onCancel: () => void;
}) => {
  const { t } = useTranslation();
  const { dialogRef, initialFocusRef } = useModalFocus(onCancel);

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onCancel} aria-label={t('actions.close')} />
      <section
        ref={dialogRef}
        className="task-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="confirm-dialog-title"
        tabIndex={-1}
      >
        <header className="task-dialog__header">
          <div>
            <h2 id="confirm-dialog-title">{title}</h2>
            <p>{message}</p>
          </div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onCancel} aria-label={t('actions.close')}>
            <X size={18} aria-hidden="true" />
          </button>
        </header>
        <div className="task-form">
          <p className="form-note">
            <CircleAlert size={14} aria-hidden="true" /> {t('confirm.note')}
          </p>
          <footer className="task-form__actions">
            <button className="button button--secondary" type="button" onClick={onCancel}>
              {cancelLabel}
            </button>
            <button
              className={tone === 'danger' ? 'button button--danger' : 'button button--primary'}
              type="button"
              onClick={onConfirm}
            >
              {confirmLabel}
            </button>
          </footer>
        </div>
      </section>
    </div>
  );
};
