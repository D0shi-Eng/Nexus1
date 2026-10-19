import { useState, type FormEvent } from 'react';
import { Eye, EyeOff, LoaderCircle, LogIn } from 'lucide-react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { BrandMark } from '../../components/BrandMark';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';

// شاشة الدخول الإنتاجي: البريد وكلمة المرور فقط — لا «تذكرني» ولا هوية من الواجهة.
// نجاح الدخول يُبطل كل الاستعلامات فتعيد البوابة بناء القشرة بجلسة الخلف الصادقة.
export const LoginScreen = () => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [showPassword, setShowPassword] = useState(false);
  const [validationError, setValidationError] = useState(false);
  const [errorCode, setErrorCode] = useState<string | null>(null);

  const loginMutation = useMutation({
    mutationFn: nexusApi.login,
    onSuccess: async () => {
      await queryClient.invalidateQueries();
    },
  });

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setValidationError(false);
    setErrorCode(null);
    if (!email.trim() || !password) {
      setValidationError(true);
      return;
    }
    try {
      await loginMutation.mutateAsync({ email: email.trim(), password });
    } catch (error) {
      setErrorCode(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  return (
    <div className="auth-screen">
      <section className="auth-card panel" aria-labelledby="login-title">
        <header className="auth-card__header">
          <div className="auth-brand">
            <div className="brand__mark"><BrandMark /></div>
            <div>
              <strong>{t('product.name')}</strong>
              <span>{t('product.subtitle')}</span>
            </div>
          </div>
          <h1 id="login-title">{t('login.title')}</h1>
          <p>{t('login.subtitle')}</p>
        </header>

        <form className="task-form" onSubmit={(event) => { void submit(event); }} noValidate>
          <label htmlFor="login-email">
            <span>{t('login.email')}</span>
            <input
              id="login-email"
              type="email"
              value={email}
              onChange={(event) => setEmail(event.currentTarget.value)}
              maxLength={320}
              autoComplete="email"
              required
            />
          </label>
          <label htmlFor="login-password">
            <span>{t('login.password')}</span>
            <span className="password-field">
              <input
                id="login-password"
                type={showPassword ? 'text' : 'password'}
                value={password}
                onChange={(event) => setPassword(event.currentTarget.value)}
                autoComplete="current-password"
                required
              />
              <button
                className="icon-button"
                type="button"
                onClick={() => setShowPassword((visible) => !visible)}
                aria-label={showPassword ? t('login.hidePassword') : t('login.showPassword')}
                aria-pressed={showPassword}
              >
                {showPassword ? <EyeOff size={16} aria-hidden="true" /> : <Eye size={16} aria-hidden="true" />}
              </button>
            </span>
          </label>
          {validationError ? <p className="form-error" role="alert">{t('setup.validationRequired')}</p> : null}
          {errorCode ? <p className="form-error" role="alert">{t(`errors.${errorCode}`, { defaultValue: t('errors.unavailable') })}</p> : null}
          <footer className="task-form__actions">
            <button className="button button--primary" type="submit" disabled={loginMutation.isPending}>
              {loginMutation.isPending
                ? <LoaderCircle className="spin" size={16} aria-hidden="true" />
                : <LogIn size={16} aria-hidden="true" />}
              {t('login.submit')}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
};
