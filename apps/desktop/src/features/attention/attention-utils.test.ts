import { describe, expect, it } from 'vitest';
import type { AttentionItem } from '@nexus/contracts';
import { filterAttentionItems, isAttentionOverdue } from './attention-utils';

const item = (overrides: Partial<AttentionItem>): AttentionItem => ({
  id: 'item-1',
  kind: 'TASK',
  severity: 'HIGH',
  title: 'Prepare evidence register',
  status: 'OPEN',
  reasonCode: 'TASK_OPEN',
  source: 'LOCAL_WORKSPACE',
  dueAt: '2026-08-25T00:00:00Z',
  resourceId: 'task-1',
  relatedEntity: { kind: 'OBLIGATION', id: 'obligation-1', title: 'Data inventory' },
  owner: 'Privacy Officer',
  updatedAt: '2026-08-20T00:00:00Z',
  availableActions: ['OPEN', 'REVIEW'],
  ...overrides,
});

const noFilter = {
  query: '',
  severity: 'ALL' as const,
  kind: 'ALL' as const,
  status: 'ALL' as const,
  language: 'en',
};

const translate = (key: string): string => ({
  'attention.reason.TASK_OPEN': 'The task is open and needs follow-up.',
  'attention.reason.EVIDENCE_MISSING': 'Required evidence has not been registered.',
  'attention.reason.APPROVAL_PENDING': 'A human approval is pending.',
}[key] ?? key);

describe('attention item filtering', () => {
  it('filters by priority, type, and status using the real item fields', () => {
    const items = [
      item({ id: 'task-high', severity: 'HIGH', kind: 'TASK', status: 'OPEN' }),
      item({ id: 'evidence-critical', severity: 'CRITICAL', kind: 'EVIDENCE', status: 'MISSING', reasonCode: 'EVIDENCE_MISSING' }),
    ];

    expect(filterAttentionItems(items, { ...noFilter, severity: 'CRITICAL' }, translate).map((value) => value.id)).toEqual(['evidence-critical']);
    expect(filterAttentionItems(items, { ...noFilter, kind: 'TASK', status: 'OPEN' }, translate).map((value) => value.id)).toEqual(['task-high']);
  });

  it('searches the title, owner, related entity, and localized reason', () => {
    const items = [
      item({ id: 'task', owner: 'Privacy Officer' }),
      item({ id: 'evidence', kind: 'EVIDENCE', reasonCode: 'EVIDENCE_MISSING', title: 'Evidence gap', owner: 'Security Officer' }),
    ];

    expect(filterAttentionItems(items, { ...noFilter, query: 'privacy' }, translate).map((value) => value.id)).toEqual(['task']);
    expect(filterAttentionItems(items, { ...noFilter, query: 'not been registered' }, translate).map((value) => value.id)).toEqual(['evidence']);
  });

  it('orders critical items before lower priorities and flags overdue dates from actual timestamps', () => {
    const items = [
      item({ id: 'low-overdue', severity: 'LOW', dueAt: '2020-01-01T00:00:00Z' }),
      item({ id: 'critical-future', severity: 'CRITICAL', dueAt: '2030-01-01T00:00:00Z' }),
    ];

    expect(filterAttentionItems(items, noFilter, translate).map((value) => value.id)).toEqual(['critical-future', 'low-overdue']);
    expect(isAttentionOverdue('2020-01-01T00:00:00Z', Date.parse('2026-08-20T00:00:00Z'))).toBe(true);
    expect(isAttentionOverdue('2030-01-01T00:00:00Z', Date.parse('2026-08-20T00:00:00Z'))).toBe(false);
  });
});
