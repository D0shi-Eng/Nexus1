import type { SeverityLevel } from '@nexus/contracts';

// ترتيب الشدات للأغراض العرضية فقط — الترتيب الحتمي للمخارج يبقى مسؤولية الخلف.
export const severityRank: Record<SeverityLevel, number> = {
  CRITICAL: 0,
  HIGH: 1,
  MEDIUM: 2,
  LOW: 3,
};

// عرض طوابع التاريخ بصيغة محلية مختصرة — القيم المحفوظة تبقى RFC3339 خام.
export const formatDate = (value: string | null, language: string): string => {
  if (!value) {
    return '—';
  }

  return new Intl.DateTimeFormat(language, {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
  }).format(new Date(value));
};

// نصوص الواجهة التي تذكر «زمن» الفحص تحتاج ساعة ودقيقة؛ تمرير RFC3339 خاماً
// يعرض قيمة تقنية غير مقروءة وتنتهك اتجاه النص في الواجهة العربية.
export const formatDateTime = (value: string | null, language: string): string => {
  if (!value) {
    return '—';
  }
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) {
    return '—';
  }

  return new Intl.DateTimeFormat(language, {
    day: 'numeric',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
  }).format(parsed);
};
