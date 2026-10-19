import { useState, type FormEvent } from 'react';
import { Gavel, LoaderCircle, Plus, X } from 'lucide-react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { CreateRegulatorySourceInput } from '@nexus/contracts';

import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { useModalFocus } from '../../lib/use-modal-focus';

// تسجيل مصدر تنظيمي يقدّمه المستخدم: الجهة والوثيقة والإصدار والرابط.
// لا جلب ولا تحقق آلي: المصدر يبقى غير رسمي (`official = 0`) و«بانتظار المراجعة»،
// والرابط يُحفظ كمرجع أدخله المستخدم فقط — ويرفض الخلف المخططات غير الآمنة.
export const CreateRegulatorySourceDialog = ({
  onClose,
  onCreated,
}: {
  readonly onClose: () => void;
  readonly onCreated: () => void;
}) => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const [authority, setAuthority] = useState('');
  const [documentTitle, setDocumentTitle] = useState('');
  const [documentVersion, setDocumentVersion] = useState('');
  const [sourceUrl, setSourceUrl] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const createMutation = useMutation({ mutationFn: nexusApi.createRegulatorySource });

  const canSubmit = authority.trim().length >= 2
    && documentTitle.trim().length >= 2
    && !isSubmitting;

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setErrorCode(null);
    if (!canSubmit) {
      return;
    }
    setIsSubmitting(true);
    const input: CreateRegulatorySourceInput = {
      authority: authority.trim(),
      documentTitle: documentTitle.trim(),
      documentVersion: documentVersion.trim() ? documentVersion.trim() : null,
      sourceUrl: sourceUrl.trim() ? sourceUrl.trim() : null,
    };
    try {
      await createMutation.mutateAsync(input);
      await queryClient.invalidateQueries({ queryKey: ['regulatory-sources'] });
      await queryClient.invalidateQueries({ queryKey: ['audit-trail'] });
      onCreated();
      onClose();
    } catch (error) {
      setErrorCode(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section
        ref={dialogRef}
        className="task-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="create-regulatory-source-title"
        tabIndex={-1}
      >
        <header className="task-dialog__header">
          <div>
            <h2 id="create-regulatory-source-title">{t('regulatory.create.title')}</h2>
            <p>{t('regulatory.create.subtitle')}</p>
          </div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}>
            <X size={18} aria-hidden="true" />
          </button>
        </header>
        <form onSubmit={submit} className="task-form">
          <label>
            <span>{t('regulatory.create.authority')}</span>
            <input value={authority} onChange={(event) => setAuthority(event.target.value)} maxLength={160} required />
          </label>
          <label>
            <span>{t('regulatory.create.documentTitle')}</span>
            <input value={documentTitle} onChange={(event) => setDocumentTitle(event.target.value)} maxLength={240} required />
          </label>
          <div className="task-form__row">
            <label>
              <span>{t('regulatory.create.documentVersion')}</span>
              <input value={documentVersion} onChange={(event) => setDocumentVersion(event.target.value)} maxLength={64} />
            </label>
            <label>
              <span>{t('regulatory.create.sourceUrl')}</span>
              <input
                type="url"
                value={sourceUrl}
                onChange={(event) => setSourceUrl(event.target.value)}
                maxLength={500}
                placeholder="https://"
              />
            </label>
          </div>
          <p className="form-note">{t('regulatory.create.provenanceNote')}</p>
          {errorCode ? (
            <p className="form-error" role="alert">
              {t(`errors.${errorCode}`, { defaultValue: t('regulatory.create.failure') })}
            </p>
          ) : null}
          <footer className="task-form__actions">
            <button className="button button--secondary" type="button" onClick={onClose} disabled={isSubmitting}>
              {t('actions.cancel')}
            </button>
            <button className="button button--primary" type="submit" disabled={!canSubmit}>
              {isSubmitting
                ? <LoaderCircle size={16} className="spin" aria-hidden="true" />
                : <Gavel size={16} aria-hidden="true" />}
              {t('regulatory.create.submit')}
            </button>
          </footer>
          <p className="form-note"><Plus size={14} aria-hidden="true" /> {t('regulatory.create.auditNote')}</p>
        </form>
      </section>
    </div>
  );
};
