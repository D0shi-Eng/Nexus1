import { useState, type FormEvent } from 'react';
import { CheckCircle2, LoaderCircle, UserPlus, X } from 'lucide-react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { CreateWorkspaceUserInput } from '@nexus/contracts';

import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { useModalFocus } from '../../lib/use-modal-focus';

// إنشاء عضو مؤسسي من داخل المنتج: الاسم والبريد وكلمة مرور أولية ودور من
// كتالوج المؤسسة. الخلف يرفض تكرار البريد، والدور الأجنبي، ورفع الصلاحيات،
// وكلمة المرور الضعيفة — والواجهة تعرض رمز الرفض كما هو.
export const CreateUserDialog = ({
  availableRoles,
  onClose,
  onCreated,
}: {
  readonly availableRoles: readonly string[];
  readonly onClose: () => void;
  readonly onCreated: () => void;
}) => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const [displayName, setDisplayName] = useState('');
  const [email, setEmail] = useState('');
  const [roleCode, setRoleCode] = useState(availableRoles[0] ?? '');
  const [initialPassword, setInitialPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const createMutation = useMutation({ mutationFn: nexusApi.createWorkspaceUser });

  const mismatch = confirmPassword.length > 0 && confirmPassword !== initialPassword;
  const canSubmit = displayName.trim().length >= 2
    && email.trim().length > 0
    && roleCode.length > 0
    && initialPassword.length > 0
    && initialPassword === confirmPassword
    && !isSubmitting;

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setErrorCode(null);
    if (!canSubmit) {
      return;
    }
    setIsSubmitting(true);
    const input: CreateWorkspaceUserInput = {
      displayName: displayName.trim(),
      email: email.trim(),
      initialPassword,
      roleCodes: [roleCode],
    };
    try {
      await createMutation.mutateAsync(input);
      await queryClient.invalidateQueries({ queryKey: ['workspace-users'] });
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
        aria-labelledby="create-user-title"
        tabIndex={-1}
      >
        <header className="task-dialog__header">
          <div>
            <h2 id="create-user-title">{t('users.create.title')}</h2>
            <p>{t('users.create.subtitle')}</p>
          </div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}>
            <X size={18} aria-hidden="true" />
          </button>
        </header>
        <form onSubmit={submit} className="task-form">
          <label>
            <span>{t('users.create.displayName')}</span>
            <input value={displayName} onChange={(event) => setDisplayName(event.target.value)} maxLength={80} required />
          </label>
          <label>
            <span>{t('users.create.email')}</span>
            <input type="email" value={email} onChange={(event) => setEmail(event.target.value)} maxLength={120} required />
          </label>
          <label>
            <span>{t('users.create.role')}</span>
            <select value={roleCode} onChange={(event) => setRoleCode(event.target.value)} required>
              {availableRoles.map((role) => (
                <option key={role} value={role}>{t(`users.role.${role}`, { defaultValue: role })}</option>
              ))}
            </select>
          </label>
          <div className="task-form__row">
            <label>
              <span>{t('users.create.initialPassword')}</span>
              <input
                type="password"
                value={initialPassword}
                onChange={(event) => setInitialPassword(event.target.value)}
                maxLength={128}
                autoComplete="new-password"
                required
              />
            </label>
            <label>
              <span>{t('users.create.confirmPassword')}</span>
              <input
                type="password"
                value={confirmPassword}
                onChange={(event) => setConfirmPassword(event.target.value)}
                maxLength={128}
                autoComplete="new-password"
                required
              />
            </label>
          </div>
          <p className="form-note">{t('users.create.passwordPolicy')}</p>
          {mismatch ? <p className="form-error" role="alert">{t('users.create.mismatch')}</p> : null}
          {errorCode ? (
            <p className="form-error" role="alert">
              {t(`errors.${errorCode}`, { defaultValue: t('users.create.failure') })}
            </p>
          ) : null}
          <footer className="task-form__actions">
            <button className="button button--secondary" type="button" onClick={onClose} disabled={isSubmitting}>
              {t('actions.cancel')}
            </button>
            <button className="button button--primary" type="submit" disabled={!canSubmit}>
              {isSubmitting
                ? <LoaderCircle size={16} className="spin" aria-hidden="true" />
                : <UserPlus size={16} aria-hidden="true" />}
              {t('users.create.submit')}
            </button>
          </footer>
          <p className="form-note">
            <CheckCircle2 size={14} aria-hidden="true" /> {t('users.create.auditNote')}
          </p>
        </form>
      </section>
    </div>
  );
};
