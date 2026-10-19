import { useQuery } from '@tanstack/react-query';
import { CircleAlert, LoaderCircle } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { nexusApi } from '../lib/nexus-api';
import { App } from './App';
import { LoginScreen } from '../features/auth/LoginScreen';
import { SetupScreen } from '../features/auth/SetupScreen';

// بوابة الهوية قبل القشرة كلها: وضع التشغيل والجلوس مصدرهما الخلف حصراً.
// لا يُرسم أي سطح عمل قبل معرفة وضع المصادقة — والفشل يظهر شاشة غير متاحة آمنة.
export const AuthGate = () => {
  const { t } = useTranslation();
  const authStatusQuery = useQuery({
    queryKey: ['auth-status'],
    queryFn: nexusApi.getAuthStatus,
    staleTime: 5_000,
    retry: 1,
  });

  const authStatus = authStatusQuery.data;

  if (authStatusQuery.isLoading) {
    return (
      <div className="auth-screen auth-screen--center" role="status">
        <span className="loading-spinner" aria-hidden="true" />
        <p>{t('dashboard.loading')}</p>
      </div>
    );
  }

  if (authStatus === undefined) {
    return (
      <div className="auth-screen auth-screen--center">
        <div className="error-panel" role="alert">
          <CircleAlert size={20} aria-hidden="true" />
          <p>{t('errors.LOCAL_SERVICE_UNAVAILABLE')}</p>
          <button className="button button--secondary" type="button" onClick={() => { void authStatusQuery.refetch(); }}>
            <LoaderCircle size={16} aria-hidden="true" />
            {t('actions.refresh')}
          </button>
        </div>
      </div>
    );
  }
  if (authStatus.mode === 'UNPROVISIONED') {
    return <SetupScreen />;
  }
  if (authStatus.mode === 'PRODUCTION' && !authStatus.authenticated) {
    return <LoginScreen />;
  }
  return <App />;
};
