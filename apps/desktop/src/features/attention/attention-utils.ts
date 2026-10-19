import type { AttentionItem, AttentionItemKind, AttentionItemStatus, SeverityLevel } from '@nexus/contracts';

type AttentionTranslator = (key: string) => string;

export interface AttentionFilters {
  readonly query: string;
  readonly severity: 'ALL' | SeverityLevel;
  readonly kind: 'ALL' | AttentionItemKind;
  readonly status: 'ALL' | AttentionItemStatus;
  readonly language: string;
}

const severityRank: Record<SeverityLevel, number> = {
  CRITICAL: 0,
  HIGH: 1,
  MEDIUM: 2,
  LOW: 3,
};

export const isAttentionOverdue = (value: string | null, now = Date.now()): boolean => value !== null && new Date(value).getTime() < now;

export const filterAttentionItems = (
  items: readonly AttentionItem[],
  filters: AttentionFilters,
  translate: AttentionTranslator,
): AttentionItem[] => {
  const normalizedQuery = filters.query.trim().toLocaleLowerCase(filters.language);

  return [...items]
    .filter((item) => filters.severity === 'ALL' || item.severity === filters.severity)
    .filter((item) => filters.kind === 'ALL' || item.kind === filters.kind)
    .filter((item) => filters.status === 'ALL' || item.status === filters.status)
    .filter((item) => !normalizedQuery || [item.title, item.owner ?? '', item.relatedEntity?.title ?? '', translate(`attention.reason.${item.reasonCode}`)].join(' ').toLocaleLowerCase(filters.language).includes(normalizedQuery))
    .sort((left, right) => severityRank[left.severity] - severityRank[right.severity] || Number(isAttentionOverdue(right.dueAt)) - Number(isAttentionOverdue(left.dueAt)) || (left.dueAt ?? '').localeCompare(right.dueAt ?? ''));
};
