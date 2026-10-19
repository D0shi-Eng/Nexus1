import { describe, expect, it } from 'vitest';
import type { ObligationListItem } from '@nexus/contracts';
import { filterAndSortObligations, getDueState, getObligationOwners } from './obligation-utils';

const item = (overrides: Partial<ObligationListItem>): ObligationListItem => ({
  id: 'obligation-default',
  tenantId: 'tenant-a',
  title: 'Synthetic obligation',
  description: 'Synthetic record for local verification',
  status: 'OPEN',
  priority: 'MEDIUM',
  sourceId: 'source-a',
  sourceReference: 'https://example.invalid/source-a',
  sourceType: 'SYNTHETIC',
  source: { authority: 'DEMO / SYNTHETIC Authority', documentTitle: 'Synthetic source', sourceUrl: 'https://example.invalid/source-a', version: '1.0', retrievedAt: '2026-08-20T00:00:00Z', official: false },
  owner: 'Owner A',
  dueDate: '2026-08-24T00:00:00Z',
  createdAt: '2026-08-18T00:00:00Z',
  updatedAt: '2026-08-20T00:00:00Z',
  reviewStatus: 'NOT_REVIEWED',
  complianceStatus: 'NOT_ASSESSED',
  dataMode: 'DEMO_SYNTHETIC',
  isArchived: false,
  ...overrides,
});

const filters = {
  query: '',
  status: 'ALL',
  priority: 'ALL',
  complianceStatus: 'ALL',
  sourceType: 'ALL',
  owner: 'ALL',
  dueState: 'ALL',
  sort: 'PRIORITY',
} as const;

describe('obligation-utils', () => {
  it('searches title, description, owner, source, and reference without regard to case', () => {
    const items = [
      item({ id: 'a', title: 'التزام الخصوصية', description: 'مراجعة سجل البيانات', owner: 'مسؤول الخصوصية' }),
      item({ id: 'b', title: 'Access review', sourceReference: 'Reference-ABC' }),
    ];
    expect(filterAndSortObligations(items, { ...filters, query: 'الخصوصية' }, new Date('2026-08-20T00:00:00Z')).map((value) => value.id)).toEqual(['a']);
    expect(filterAndSortObligations(items, { ...filters, query: 'reference-abc' }, new Date('2026-08-20T00:00:00Z')).map((value) => value.id)).toEqual(['b']);
    expect(filterAndSortObligations(items, { ...filters, query: 'does-not-exist' }, new Date('2026-08-20T00:00:00Z'))).toEqual([]);
  });

  it('filters by real status priority compliance source owner and due state', () => {
    const now = new Date('2026-08-20T00:00:00Z');
    const items = [
      item({ id: 'overdue', priority: 'CRITICAL', status: 'AT_RISK', complianceStatus: 'NON_COMPLIANT', owner: 'Owner A', dueDate: '2026-08-19T00:00:00Z' }),
      item({ id: 'soon', priority: 'HIGH', status: 'IN_PROGRESS', complianceStatus: 'PARTIALLY_COMPLIANT', owner: 'Owner B', dueDate: '2026-08-24T00:00:00Z', sourceType: 'INTERNAL' }),
      item({ id: 'none', priority: 'LOW', status: 'COMPLETED', complianceStatus: 'COMPLIANT', owner: 'Owner C', dueDate: null }),
    ];
    expect(filterAndSortObligations(items, { ...filters, status: 'AT_RISK' }, now).map((value) => value.id)).toEqual(['overdue']);
    expect(filterAndSortObligations(items, { ...filters, priority: 'HIGH' }, now).map((value) => value.id)).toEqual(['soon']);
    expect(filterAndSortObligations(items, { ...filters, complianceStatus: 'NON_COMPLIANT' }, now).map((value) => value.id)).toEqual(['overdue']);
    expect(filterAndSortObligations(items, { ...filters, sourceType: 'INTERNAL' }, now).map((value) => value.id)).toEqual(['soon']);
    expect(filterAndSortObligations(items, { ...filters, owner: 'Owner C' }, now).map((value) => value.id)).toEqual(['none']);
    expect(filterAndSortObligations(items, { ...filters, dueState: 'OVERDUE' }, now).map((value) => value.id)).toEqual(['overdue']);
    expect(filterAndSortObligations(items, { ...filters, dueState: 'DUE_SOON' }, now).map((value) => value.id)).toEqual(['soon']);
    expect(filterAndSortObligations(items, { ...filters, dueState: 'NO_DUE_DATE' }, now).map((value) => value.id)).toEqual(['none']);
  });

  it('sorts by priority due date update date creation date and status', () => {
    const items = [
      item({ id: 'low', priority: 'LOW', dueDate: '2026-09-01T00:00:00Z', updatedAt: '2026-08-19T00:00:00Z', createdAt: '2026-08-01T00:00:00Z', status: 'OPEN' }),
      item({ id: 'critical', priority: 'CRITICAL', dueDate: '2026-08-21T00:00:00Z', updatedAt: '2026-08-21T00:00:00Z', createdAt: '2026-08-21T00:00:00Z', status: 'REQUIRES_REVIEW' }),
      item({ id: 'high', priority: 'HIGH', dueDate: null, updatedAt: '2026-08-20T00:00:00Z', createdAt: '2026-08-10T00:00:00Z', status: 'AT_RISK' }),
    ];
    expect(filterAndSortObligations(items, { ...filters, sort: 'PRIORITY' }).map((value) => value.id)).toEqual(['critical', 'high', 'low']);
    expect(filterAndSortObligations(items, { ...filters, sort: 'DUE_DATE' }).map((value) => value.id)).toEqual(['critical', 'low', 'high']);
    expect(filterAndSortObligations(items, { ...filters, sort: 'UPDATED_AT' }).map((value) => value.id)).toEqual(['critical', 'high', 'low']);
    expect(filterAndSortObligations(items, { ...filters, sort: 'CREATED_AT' }).map((value) => value.id)).toEqual(['critical', 'high', 'low']);
    expect(filterAndSortObligations(items, { ...filters, sort: 'STATUS' }).map((value) => value.id)).toEqual(['high', 'low', 'critical']);
  });

  it('derives due state and owner options from the real dataset', () => {
    const now = new Date('2026-08-20T00:00:00Z');
    expect(getDueState('2026-08-19T00:00:00Z', now)).toBe('OVERDUE');
    expect(getDueState('2026-08-22T00:00:00Z', now)).toBe('DUE_SOON');
    expect(getDueState('2026-09-01T00:00:00Z', now)).toBe('FUTURE');
    expect(getDueState(null, now)).toBe('NO_DUE_DATE');
    expect(getObligationOwners([item({ owner: 'Owner B' }), item({ id: 'two', owner: 'Owner A' }), item({ id: 'three', owner: 'Owner B' })])).toEqual(['Owner A', 'Owner B']);
  });
});
