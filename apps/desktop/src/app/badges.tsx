import { useTranslation } from 'react-i18next';

import type { SeverityLevel } from '@nexus/contracts';

// القيمة غير المترجمة تُعرض كرمزها الخام لا كمفتاح منقّط: ظهور «status.X» للمستخدم
// عطل ظاهر، بينما الرمز الخام يبقى صادقاً وقابلاً للتشخيص حتى تُضاف الترجمة.
export const SeverityBadge = ({ severity }: { severity: SeverityLevel }) => {
  const { t } = useTranslation();
  return <span className={`badge badge--${severity.toLowerCase()}`}>{t(`status.${severity}`, { defaultValue: severity })}</span>;
};

export const StatusBadge = ({ value }: { value: string }) => {
  const { t } = useTranslation();
  return <span className="status-badge">{t(`status.${value}`, { defaultValue: value })}</span>;
};
