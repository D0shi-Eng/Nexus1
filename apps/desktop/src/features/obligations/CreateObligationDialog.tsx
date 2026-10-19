import { useEffect, useState, type FormEvent } from 'react';
import { ClipboardCheck, LoaderCircle, X } from 'lucide-react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type {
  CreateObligationInput, ObligationApplicability, ObligationPriority, RegulatorySourceRecord,
} from '@nexus/contracts';
import { obligationApplicabilities, obligationPriorities } from '@nexus/contracts';

import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { useModalFocus } from '../../lib/use-modal-focus';

// تأليف الالتزام المؤسسي: يُربط بمصدر تنظيمي مسجَّل في المؤسسة نفسها.
// المنشأ `USER_PROVIDED` وحالة التحقق `UNVERIFIED` يفرضهما الخلف، والانطباق
// يعلنه المستخدم صراحةً — لا يستنتجه النظام من نص المصدر.
export const CreateObligationDialog = ({
  sources,
  onClose,
  onCreated,
}: {
  readonly sources: readonly RegulatorySourceRecord[];
  readonly onClose: () => void;
  readonly onCreated: (obligationId: string) => void;
}) => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const [sourceId, setSourceId] = useState(sources[0]?.id ?? '');
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [owner, setOwner] = useState('');
  const [dueDate, setDueDate] = useState('');
  const [priority, setPriority] = useState<ObligationPriority>('HIGH');
  const [applicability, setApplicability] = useState<ObligationApplicability>('APPLIES');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const createMutation = useMutation({ mutationFn: nexusApi.createObligation });

  // المصادر تُطلب عند فتح الحوار، فقد تصل بعد التركيب الأول: نثبّت أول مصدر
  // متاح تلقائياً بدل ترك القائمة بلا اختيار وزر الإرسال معطلاً بلا سبب.
  useEffect(() => {
    if (sourceId.length === 0 && sources.length > 0) {
      setSourceId(sources[0].id);
    }
  }, [sourceId, sources]);

  const canSubmit = sourceId.length > 0
    && title.trim().length > 0
    && owner.trim().length > 0
    && !isSubmitting;

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setErrorCode(null);
    if (!canSubmit) {
      return;
    }
    setIsSubmitting(true);
    const input: CreateObligationInput = {
      sourceId,
      title: title.trim(),
      description: description.trim(),
      owner: owner.trim(),
      dueDate: dueDate ? `${dueDate}T00:00:00Z` : null,
      priority,
      applicability,
    };
    try {
      const created = await createMutation.mutateAsync(input);
      await queryClient.invalidateQueries({ queryKey: ['obligations'] });
      await queryClient.invalidateQueries({ queryKey: ['dashboard'] });
      await queryClient.invalidateQueries({ queryKey: ['attention'] });
      await queryClient.invalidateQueries({ queryKey: ['regulatory-sources'] });
      await queryClient.invalidateQueries({ queryKey: ['audit-trail'] });
      onCreated(created.id);
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
        aria-labelledby="create-obligation-title"
        tabIndex={-1}
      >
        <header className="task-dialog__header">
          <div>
            <h2 id="create-obligation-title">{t('obligations.create.title')}</h2>
            <p>{t('obligations.create.subtitle')}</p>
          </div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}>
            <X size={18} aria-hidden="true" />
          </button>
        </header>
        <form onSubmit={submit} className="task-form">
          <label>
            <span>{t('obligations.create.source')}</span>
            <select value={sourceId} onChange={(event) => setSourceId(event.target.value)} required>
              {sources.map((source) => (
                <option key={source.id} value={source.id}>
                  {`${source.authority} · ${source.documentTitle}`}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>{t('obligations.create.obligationTitle')}</span>
            <input value={title} onChange={(event) => setTitle(event.target.value)} maxLength={240} required />
          </label>
          <label>
            <span>{t('obligations.create.description')}</span>
            <textarea value={description} onChange={(event) => setDescription(event.target.value)} maxLength={4000} rows={4} />
          </label>
          <div className="task-form__row">
            <label>
              <span>{t('obligations.create.owner')}</span>
              <input value={owner} onChange={(event) => setOwner(event.target.value)} maxLength={160} required />
            </label>
            <label>
              <span>{t('obligations.create.dueDate')}</span>
              <input type="date" value={dueDate} onChange={(event) => setDueDate(event.target.value)} />
            </label>
          </div>
          <div className="obligations-edit-grid">
            <label>
              <span>{t('obligations.filters.priority')}</span>
              <select value={priority} onChange={(event) => setPriority(event.target.value as ObligationPriority)}>
                {obligationPriorities.map((value) => <option key={value} value={value}>{t(`status.${value}`)}</option>)}
              </select>
            </label>
            <label>
              <span>{t('obligations.create.applicability')}</span>
              <select
                value={applicability}
                onChange={(event) => setApplicability(event.target.value as ObligationApplicability)}
              >
                {obligationApplicabilities.map((value) => (
                  <option key={value} value={value}>{t(`status.${value}`)}</option>
                ))}
              </select>
            </label>
          </div>
          <p className="form-note">{t('obligations.create.provenanceNote')}</p>
          {errorCode ? (
            <p className="form-error" role="alert">
              {t(`errors.${errorCode}`, { defaultValue: t('obligations.create.failure') })}
            </p>
          ) : null}
          <footer className="task-form__actions">
            <button className="button button--secondary" type="button" onClick={onClose} disabled={isSubmitting}>
              {t('actions.cancel')}
            </button>
            <button className="button button--primary" type="submit" disabled={!canSubmit || sources.length === 0}>
              {isSubmitting
                ? <LoaderCircle size={16} className="spin" aria-hidden="true" />
                : <ClipboardCheck size={16} aria-hidden="true" />}
              {t('obligations.create.submit')}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
};
