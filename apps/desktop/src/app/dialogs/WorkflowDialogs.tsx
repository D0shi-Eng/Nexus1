import { CheckCircle2, ClipboardCheck, FileCheck2, LoaderCircle, X } from 'lucide-react';
import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';
import type {
  CreateRemediationTaskInput, DecideApprovalInput, ObligationSummary,
  RegisterEvidenceReferenceInput,
} from '@nexus/contracts';

import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { useModalFocus } from '../../lib/use-modal-focus';

// حوار إنشاء مهمة معالجة — الإرسال الحقيقي عبر IPC مع حماية إرسال مزدوج.
export const TaskDialog = ({
  obligations,
  initialObligationId,
  onClose,
  onCreated,
}: {
  obligations: readonly ObligationSummary[];
  initialObligationId?: string;
  onClose: () => void;
  onCreated: () => void;
}) => {
  const { t } = useTranslation();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const [obligationId, setObligationId] = useState(initialObligationId ?? obligations[0]?.id ?? '');
  const [title, setTitle] = useState('');
  const [description, setDescription] = useState('');
  const [priority, setPriority] = useState<CreateRemediationTaskInput['priority']>('HIGH');
  const [dueDate, setDueDate] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submissionError, setSubmissionError] = useState<string | null>(null);

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setSubmissionError(null);
    setIsSubmitting(true);
    try {
      await nexusApi.createRemediationTask({
        obligationId,
        title,
        description,
        priority,
        dueAt: dueDate ? `${dueDate}T00:00:00Z` : null,
      });
      onCreated();
      onClose();
    } catch (error) {
      setSubmissionError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="create-task-title" tabIndex={-1}>
        <header className="task-dialog__header">
          <div><h2 id="create-task-title">{t('taskDialog.title')}</h2><p>{t('taskDialog.subtitle')}</p></div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}><X size={18} aria-hidden="true" /></button>
        </header>
        <form onSubmit={submit} className="task-form">
          {obligations.length === 0 ? (
            // R7 / UAT-08: لا حوار مسدود. غياب الالتزام القابل للربط يُعلن صراحةً
            // مع مخرج آمن، بدل قائمة فارغة وزر إنشاء معطّل بلا تفسير.
            <div className="empty-state empty-state--block" role="status">
              <p>{t('taskDialog.noObligations')}</p>
            </div>
          ) : null}
          <label>
            <span>{t('taskDialog.obligation')}</span>
            <select value={obligationId} onChange={(event) => setObligationId(event.target.value)} required>
              {obligations.map((obligation) => <option key={obligation.id} value={obligation.id}>{obligation.title}</option>)}
            </select>
          </label>
          <label>
            <span>{t('taskDialog.taskTitle')}</span>
            <input value={title} onChange={(event) => setTitle(event.target.value)} maxLength={160} required />
          </label>
          <label>
            <span>{t('taskDialog.description')}</span>
            <textarea value={description} onChange={(event) => setDescription(event.target.value)} maxLength={2000} rows={4} required />
          </label>
          <div className="task-form__row">
            <label>
              <span>{t('taskDialog.priority')}</span>
              <select value={priority} onChange={(event) => setPriority(event.target.value as CreateRemediationTaskInput['priority'])}>
                {(['CRITICAL', 'HIGH', 'MEDIUM', 'LOW'] as const).map((level) => <option key={level} value={level}>{t(`status.${level}`)}</option>)}
              </select>
            </label>
            <label>
              <span>{t('taskDialog.dueDate')}</span>
              <input type="date" value={dueDate} onChange={(event) => setDueDate(event.target.value)} />
            </label>
          </div>
          {submissionError ? <p className="form-error" role="alert">{t(`errors.${submissionError}`, { defaultValue: t('taskDialog.failure') })}</p> : null}
          <footer className="task-form__actions">
            <button className="button button--secondary" type="button" onClick={onClose}>{t('actions.cancel')}</button>
            <button className="button button--primary" type="submit" disabled={isSubmitting || obligations.length === 0}>
              {isSubmitting ? <LoaderCircle size={16} className="spin" aria-hidden="true" /> : <ClipboardCheck size={16} aria-hidden="true" />}
              {t('actions.createTask')}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
};

// حوار تسجيل مرجع دليل — خيارات التصنيف مترجمة مثل صفحة الأدلة تماماً.
export const EvidenceDialog = ({
  obligations,
  initialObligationId,
  onClose,
  onCreated,
}: {
  obligations: readonly ObligationSummary[];
  initialObligationId?: string;
  onClose: () => void;
  onCreated: () => void;
}) => {
  const { t } = useTranslation();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const [obligationId, setObligationId] = useState(initialObligationId ?? obligations[0]?.id ?? '');
  const [label, setLabel] = useState('');
  const [classification, setClassification] = useState<RegisterEvidenceReferenceInput['classification']>('INTERNAL');
  const [expiresAt, setExpiresAt] = useState('');
  const [contentFingerprint, setContentFingerprint] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submissionError, setSubmissionError] = useState<string | null>(null);

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setSubmissionError(null);
    setIsSubmitting(true);
    try {
      await nexusApi.registerEvidenceReference({
        obligationId,
        label,
        classification,
        expiresAt: expiresAt ? `${expiresAt}T00:00:00Z` : null,
        contentFingerprint,
      });
      onCreated();
      onClose();
    } catch (error) {
      setSubmissionError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="register-evidence-title" tabIndex={-1}>
        <header className="task-dialog__header">
          <div><h2 id="register-evidence-title">{t('evidenceDialog.title')}</h2><p>{t('evidenceDialog.subtitle')}</p></div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}><X size={18} aria-hidden="true" /></button>
        </header>
        <form onSubmit={submit} className="task-form">
          {obligations.length === 0 ? (
            <div className="empty-state empty-state--block" role="status">
              <p>{t('evidenceDialog.noObligations')}</p>
            </div>
          ) : null}
          <label>
            <span>{t('evidenceDialog.obligation')}</span>
            <select value={obligationId} onChange={(event) => setObligationId(event.target.value)} required>
              {obligations.map((obligation) => <option key={obligation.id} value={obligation.id}>{obligation.title}</option>)}
            </select>
          </label>
          <label><span>{t('evidenceDialog.label')}</span><input value={label} onChange={(event) => setLabel(event.target.value)} maxLength={180} required /></label>
          <label>
            <span>{t('evidenceDialog.classification')}</span>
            <select value={classification} onChange={(event) => setClassification(event.target.value as RegisterEvidenceReferenceInput['classification'])}>
              {(['PUBLIC', 'INTERNAL', 'CONFIDENTIAL', 'RESTRICTED', 'HIGHLY_RESTRICTED'] as const).map((value) => <option key={value} value={value}>{t(`status.${value}`)}</option>)}
            </select>
          </label>
          <div className="task-form__row">
            <label><span>{t('evidenceDialog.expiry')}</span><input type="date" value={expiresAt} onChange={(event) => setExpiresAt(event.target.value)} /></label>
            <label><span>{t('evidenceDialog.fingerprint')}</span><input value={contentFingerprint} onChange={(event) => setContentFingerprint(event.target.value)} maxLength={1000} required /></label>
          </div>
          {submissionError ? <p className="form-error" role="alert">{t(`errors.${submissionError}`, { defaultValue: t('evidenceDialog.failure') })}</p> : null}
          <footer className="task-form__actions">
            <button className="button button--secondary" type="button" onClick={onClose}>{t('actions.cancel')}</button>
            <button className="button button--primary" type="submit" disabled={isSubmitting || obligations.length === 0}>
              {isSubmitting ? <LoaderCircle size={16} className="spin" aria-hidden="true" /> : <FileCheck2 size={16} aria-hidden="true" />}
              {t('actions.registerEvidence')}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
};

// حوار قرار الموافقة — سبب إلزامي، والإرسال المزدوج محجوز.
export const ApprovalDecisionDialog = ({
  approvalId,
  onClose,
  onDecided,
}: {
  approvalId: string;
  onClose: () => void;
  onDecided: () => void;
}) => {
  const { t } = useTranslation();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const [reason, setReason] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submissionError, setSubmissionError] = useState<string | null>(null);

  const decide = async (decision: DecideApprovalInput['decision']): Promise<void> => {
    setSubmissionError(null);
    setIsSubmitting(true);
    try {
      await nexusApi.decideApproval({ approvalId, decision, reason });
      onDecided();
    } catch (error) {
      setSubmissionError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="approval-decision-title" tabIndex={-1}>
        <header className="task-dialog__header">
          <div><h2 id="approval-decision-title">{t('approvalDialog.title')}</h2><p>{t('approvalDialog.subtitle')}</p></div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}><X size={18} aria-hidden="true" /></button>
        </header>
        <div className="task-form">
          <label>
            <span>{t('approvalDialog.reason')}</span>
            <textarea value={reason} onChange={(event) => setReason(event.target.value)} maxLength={1000} rows={4} required />
          </label>
          {submissionError ? <p className="form-error" role="alert">{t(`errors.${submissionError}`, { defaultValue: t('errors.unavailable') })}</p> : null}
          <footer className="task-form__actions">
            <button className="button button--secondary" type="button" disabled={isSubmitting || reason.trim().length === 0} onClick={() => { void decide('REJECTED'); }}>{t('approvalDialog.reject')}</button>
            <button className="button button--primary" type="button" disabled={isSubmitting || reason.trim().length === 0} onClick={() => { void decide('APPROVED'); }}>{isSubmitting ? <LoaderCircle size={16} className="spin" aria-hidden="true" /> : <CheckCircle2 size={16} aria-hidden="true" />}{t('approvalDialog.approve')}</button>
          </footer>
        </div>
      </section>
    </div>
  );
};
