import type { ComponentType, ReactNode } from 'react';
import { CircleAlert, LoaderCircle, ShieldAlert } from 'lucide-react';
import { useTranslation } from 'react-i18next';

// حالات العرض المشتركة: التحميل والخطأ والفراغ.
// الغاية: لا تعيد كل صفحة تركيبها الخاص فتتفاوت المسافات والأيقونات والنصوص.
// كل حالة هنا تحمل دلالة وصولية واحدة (role/aria) فلا تختلف الصفحات في السلوك.
// هذا الملف يُصدّر مكوّنات فقط؛ رمز الخطأ يُقرأ من `lib/api-error`.

export const LoadingState = ({ label }: { label: string }) => (
  <div className="loading-panel" role="status" aria-live="polite">
    <LoaderCircle className="spin" size={18} aria-hidden="true" />
    <span>{label}</span>
  </div>
);

type ErrorStateProps = {
  readonly code: string;
  /** رسالة مخصّصة لرفض التفويض؛ تُترجم في الصفحة لأن صياغتها سياقية. */
  readonly deniedMessage: string;
  readonly fallbackMessage?: string;
};

export const ErrorState = ({ code, deniedMessage, fallbackMessage }: ErrorStateProps) => {
  const { t } = useTranslation();
  if (code === 'AUTHORIZATION_DENIED') {
    return (
      <div className="error-panel" role="alert">
        <ShieldAlert size={20} aria-hidden="true" />
        <p>{deniedMessage}</p>
      </div>
    );
  }
  // الرمز يترجم إلى نص بشري؛ وإن لم توجد ترجمة يظهر نص عام بدل رمز تقني خام.
  return (
    <div className="error-panel" role="alert">
      <CircleAlert size={20} aria-hidden="true" />
      <p>{t(`errors.${code}`, { defaultValue: fallbackMessage ?? t('errors.unavailable') })}</p>
    </div>
  );
};

type EmptyStateProps = {
  readonly icon: ComponentType<{ size?: number; 'aria-hidden'?: boolean | 'true' | 'false' }>;
  readonly title: string;
  readonly description?: string;
  readonly actions?: ReactNode;
  /** كتلة مركزية بارزة للفراغ الكامل؛ السطر المضمّن للنتائج المفلترة. */
  readonly variant?: 'block' | 'inline';
};

export const EmptyState = ({ icon: Icon, title, description, actions, variant = 'block' }: EmptyStateProps) => {
  if (variant === 'inline') {
    return (
      <div className="empty-state">
        <Icon size={20} aria-hidden="true" />
        <span>{title}</span>
      </div>
    );
  }
  return (
    <div className="empty-state empty-state--block">
      <span className="empty-state__icon"><Icon size={20} aria-hidden="true" /></span>
      <h3>{title}</h3>
      {description ? <p>{description}</p> : null}
      {actions ? <div className="empty-state__actions">{actions}</div> : null}
    </div>
  );
};
