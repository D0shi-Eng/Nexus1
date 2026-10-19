import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AuditTrailData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { AuditTrailPage } from './AuditTrailPage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const pageOne: AuditTrailData = {
  items: [
    { id: 'event-3', eventType: 'OBLIGATION_UPDATED', actorName: 'NEXUS Demo Operator (Requester)', resourceType: 'Obligation', resourceId: 'obl_demo_001', occurredAt: '2026-10-01T00:00:00Z' },
    { id: 'event-2', eventType: 'TASK_CREATED', actorName: 'NEXUS Demo Operator (Requester)', resourceType: 'Task', resourceId: 'task_demo_001', occurredAt: '2026-09-30T00:00:00Z' },
  ],
  chainIntegrity: 'VALID',
  chainAnchorState: 'MATCH',
  hasMore: true,
  nextCursor: { occurredAt: '2026-09-30T00:00:00Z', id: 'event-2' },
  generatedAt: '2026-10-02T00:00:00Z',
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderPage = (data?: AuditTrailData): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (data) {
    queryClient.setQueryData(['audit-trail', null], data);
  }
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(<QueryClientProvider client={queryClient}><AuditTrailPage /></QueryClientProvider>);
  });
  return container;
};

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

afterEach(async () => {
  act(() => {
    roots.splice(0).forEach((root) => root.unmount());
  });
  containers.splice(0).forEach((container) => container.remove());
  vi.restoreAllMocks();
  await i18n.changeLanguage('ar');
});

describe('AuditTrailPage', () => {
  it('renders tenant events with real chain integrity and read-only framing', () => {
    const container = renderPage(pageOne);

    expect(container.textContent).toContain('Obligation updated');
    expect(container.textContent).toContain('Task created');
    expect(container.textContent).toContain('NEXUS Demo Operator (Requester)');
    expect(container.textContent).toContain('Chain integrity verified');
    expect(container.textContent).toContain('Read only');
    expect(container.textContent).toContain('cannot be edited or deleted');
    // لا مفاتيح تجزئة خام ولا تفاصيل داخلية تُعرض.
    expect(container.textContent).not.toContain('event_hash');
    expect(container.textContent).not.toContain('previous_hash');
  });

  it('loads the next bounded page through the keyset cursor without overlap', async () => {
    const spy = vi.spyOn(nexusApi, 'getAuditTrail').mockResolvedValue(pageOne);
    const container = renderPage(pageOne);
    const loadMore = Array.from(container.querySelectorAll<HTMLButtonElement>('button'))
      .find((button) => button.textContent?.includes('Load older events'));
    expect(loadMore).toBeDefined();

    // النقر نفسه يحدّث حالة مؤشر الصفحات، فيجب أن يقع داخل act مع انتظار الجلب التالي —
    // الفصل بينهما هو ما أنتج تحذير React من تحديث حالة خارج دورة الالتزام.
    await act(async () => {
      loadMore!.click();
      await vi.waitFor(() => expect(spy).toHaveBeenCalled());
    });

    expect(spy.mock.calls[0]?.[0]).toEqual({ before: pageOne.nextCursor });
  });

  it('shows the end-of-trail note when the bounded trail is fully loaded', () => {
    const container = renderPage({ ...pageOne, hasMore: false, nextCursor: null });

    expect(container.textContent).toContain('End of the audit trail');
    expect(container.textContent).not.toContain('Load older events');
  });

  it('renders an authorization-denied safe error', async () => {
    vi.spyOn(nexusApi, 'getAuditTrail').mockRejectedValue(
      new NexusApiError('AUTHORIZATION_DENIED', 'denied'),
    );
    const container = renderPage();
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('You are not authorized'));
    });

    expect(container.textContent).toContain('You are not authorized to read the audit trail');
  });

  it('renders an empty trail truthfully', () => {
    const container = renderPage({ ...pageOne, items: [], hasMore: false, nextCursor: null });

    expect(container.textContent).toContain('No audit events have been recorded');
    expect(container.querySelector('table')).toBeNull();
  });
});
