import { CircleHelp, LoaderCircle } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import type { ModuleAvailability, NexusModuleKey } from '@nexus/contracts';

import { PageHeader } from '../components/PageHeader';
import { EmptyState } from '../components/States';

interface ModuleViewProps {
  readonly moduleKey: NexusModuleKey;
  readonly availability: ModuleAvailability | undefined;
  readonly isLoading: boolean;
  readonly isUnavailable: boolean;
}

// شاشة احتياطية لأي وحدة غير متاحة: كانت تُبنى بترويسة ولافتة خاصتين بها، فبدت
// مختلفة عن بقية الصفحات (وهذا ما ظهر في صفحة التقارير). الآن تمرّ من نفس
// `PageHeader` و`EmptyState` اللذين تمرّ منهما الوحدات الـ15، فلا تفاوت.
export const ModuleView = ({ moduleKey, availability, isLoading, isUnavailable }: ModuleViewProps) => {
  const { t } = useTranslation();
  const status = availability?.status ?? 'UNAVAILABLE';
  const messageKey = isUnavailable ? 'moduleAvailability.unavailable' : (availability?.messageKey ?? 'moduleAvailability.unavailable');
  const titleId = `module-title-${moduleKey}`;

  return (
    <section id={moduleKey} className="content-container" aria-labelledby={titleId} aria-live="polite">
      <PageHeader
        eyebrow={t('product.name')}
        eyebrowIcon={CircleHelp}
        title={t(`navigation.${moduleKey}`)}
        titleId={titleId}
        actions={isLoading
          ? <LoaderCircle className="spin" size={16} aria-label={t('moduleAvailability.loading')} />
          : <span className="status-badge">{t(`moduleAvailability.status.${status}`)}</span>}
      />
      <div className="panel" role={isUnavailable ? 'alert' : 'status'}>
        <EmptyState
          icon={CircleHelp}
          title={t(`moduleAvailability.status.${status}`)}
          description={t(messageKey)}
        />
      </div>
    </section>
  );
};
