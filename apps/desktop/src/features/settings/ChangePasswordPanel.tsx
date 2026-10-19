import { useState, type FormEvent } from 'react';
import { CircleAlert, KeyRound, LoaderCircle, ShieldCheck } from 'lucide-react';
import { useMutation } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { ChangePasswordInput } from '@nexus/contracts';

import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { useSession } from '../../lib/use-session';

// تغيير كلمة المرور للمستخدم المسجّل دخوله في وضع الإنتاج.
// الضوابط الحقيقية في الخلف: تحقق Argon2id من الحالية، سياسة الكلمة الجديدة،
// وإبطال الجلسات الأخرى — الواجهة تجمع المدخلات وتعرض رمز الرفض فقط.
export const ChangePasswordPanel = () => {
  const { t } = useTranslation();
  const { can } = useSession();
  const [currentPassword, setCurrentPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [feedback, setFeedback] = useState<string | null>(null);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const changeMutation = useMutation({ mutationFn: nexusApi.changePassword });

  const mismatch = confirmPassword.length > 0 && confirmPassword !== newPassword;
  const canSubmit = currentPassword.length > 0
    && newPassword.length > 0
    && newPassword === confirmPassword
    && !changeMutation.isPending;
  // كلمة المرور ليست من صلاحيات الأدوار: كل مستخدم يغيّر كلمة مروره وحده.
  void can;

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setFeedback(null);
    setErrorCode(null);
    if (!canSubmit) {
      return;
    }
    const input: ChangePasswordInput = { currentPassword, newPassword };
    try {
      const result = await changeMutation.mutateAsync(input);
      setFeedback(t('security.password.success', { count: result.revokedSessions }));
      setCurrentPassword('');
      setNewPassword('');
      setConfirmPassword('');
    } catch (error) {
      setErrorCode(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  return (
    <section className="panel" aria-labelledby="settings-security-title">
      <div className="panel__header">
        <div><h2 id="settings-security-title">{t('security.password.title')}</h2></div>
        <KeyRound size={18} aria-hidden="true" />
      </div>
      <p className="form-note settings-note">{t('security.password.subtitle')}</p>
      <form className="task-form" onSubmit={submit}>
        <label>
          <span>{t('security.password.current')}</span>
          <input
            type="password"
            value={currentPassword}
            onChange={(event) => setCurrentPassword(event.target.value)}
            maxLength={128}
            autoComplete="current-password"
            required
          />
        </label>
        <div className="task-form__row">
          <label>
            <span>{t('security.password.new')}</span>
            <input
              type="password"
              value={newPassword}
              onChange={(event) => setNewPassword(event.target.value)}
              maxLength={128}
              autoComplete="new-password"
              required
            />
          </label>
          <label>
            <span>{t('security.password.confirm')}</span>
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
        <p className="form-note">{t('security.password.policy')}</p>
        {mismatch ? <p className="form-error" role="alert">{t('security.password.mismatch')}</p> : null}
        {errorCode ? (
          <div className="error-panel" role="alert">
            <CircleAlert size={18} aria-hidden="true" />
            <p>{t(`errors.${errorCode}`, { defaultValue: t('security.password.failure') })}</p>
          </div>
        ) : null}
        {feedback ? (
          <div className="system-note" role="status">
            <ShieldCheck size={16} aria-hidden="true" />
            <span>{feedback}</span>
          </div>
        ) : null}
        <footer className="task-form__actions">
          <button className="button button--primary" type="submit" disabled={!canSubmit}>
            {changeMutation.isPending
              ? <LoaderCircle size={16} className="spin" aria-hidden="true" />
              : <KeyRound size={16} aria-hidden="true" />}
            {t('security.password.submit')}
          </button>
        </footer>
      </form>
    </section>
  );
};
