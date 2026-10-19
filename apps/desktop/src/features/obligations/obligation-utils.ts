import type { ComplianceStatus, ObligationListItem, ObligationPriority, ObligationStatus, SourceType } from '@nexus/contracts';

export type DueState = 'ALL' | 'OVERDUE' | 'DUE_SOON' | 'FUTURE' | 'NO_DUE_DATE';
export type ObligationSort = 'PRIORITY' | 'DUE_DATE' | 'UPDATED_AT' | 'CREATED_AT' | 'STATUS';

export interface ObligationFilters {
  readonly query: string;
  readonly status: 'ALL' | ObligationStatus;
  readonly priority: 'ALL' | ObligationPriority;
  readonly complianceStatus: 'ALL' | ComplianceStatus;
  readonly sourceType: 'ALL' | SourceType;
  readonly owner: 'ALL' | string;
  readonly dueState: DueState;
  readonly sort: ObligationSort;
}

const priorityRank: Record<ObligationPriority, number> = {
  CRITICAL: 0,
  HIGH: 1,
  MEDIUM: 2,
  LOW: 3,
};

const dueSoonDays = 7;

export const getDueState = (dueDate: string | null, now: Date): Exclude<DueState, 'ALL'> => {
  if (!dueDate) {
    return 'NO_DUE_DATE';
  }
  const due = new Date(dueDate);
  if (Number.isNaN(due.getTime())) {
    return 'NO_DUE_DATE';
  }
  if (due < now) {
    return 'OVERDUE';
  }
  const dueSoon = new Date(now);
  dueSoon.setDate(dueSoon.getDate() + dueSoonDays);
  return due <= dueSoon ? 'DUE_SOON' : 'FUTURE';
};

const normalized = (value: string): string => value.trim().toLocaleLowerCase();

const dateValue = (value: string | null): number => {
  if (!value) {
    return Number.POSITIVE_INFINITY;
  }
  const parsed = new Date(value).getTime();
  return Number.isNaN(parsed) ? Number.POSITIVE_INFINITY : parsed;
};

export const filterAndSortObligations = (
  items: readonly ObligationListItem[],
  filters: ObligationFilters,
  now: Date = new Date(),
): readonly ObligationListItem[] => {
  const query = normalized(filters.query);
  const filtered = items.filter((item) => {
    const searchable = normalized([
      item.title,
      item.description,
      item.owner,
      item.source.authority,
      item.source.documentTitle,
      item.sourceReference,
    ].join(' '));
    return (!query || searchable.includes(query))
      && (filters.status === 'ALL' || item.status === filters.status)
      && (filters.priority === 'ALL' || item.priority === filters.priority)
      && (filters.complianceStatus === 'ALL' || item.complianceStatus === filters.complianceStatus)
      && (filters.sourceType === 'ALL' || item.sourceType === filters.sourceType)
      && (filters.owner === 'ALL' || item.owner === filters.owner)
      && (filters.dueState === 'ALL' || getDueState(item.dueDate, now) === filters.dueState);
  });

  return [...filtered].sort((left, right) => {
    if (filters.sort === 'PRIORITY') {
      return priorityRank[left.priority] - priorityRank[right.priority] || dateValue(left.dueDate) - dateValue(right.dueDate);
    }
    if (filters.sort === 'DUE_DATE') {
      return dateValue(left.dueDate) - dateValue(right.dueDate);
    }
    if (filters.sort === 'UPDATED_AT') {
      return dateValue(right.updatedAt) - dateValue(left.updatedAt);
    }
    if (filters.sort === 'CREATED_AT') {
      return dateValue(right.createdAt) - dateValue(left.createdAt);
    }
    return left.status.localeCompare(right.status) || left.title.localeCompare(right.title);
  });
};

export const getObligationOwners = (items: readonly ObligationListItem[]): readonly string[] =>
  [...new Set(items.map((item) => item.owner).filter((owner) => owner.trim().length > 0))].sort((left, right) => left.localeCompare(right));
