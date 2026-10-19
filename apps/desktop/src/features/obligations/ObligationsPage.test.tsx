import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ObligationDetails, ObligationsData } from '@nexus/contracts';
import '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { ObligationsPage } from './ObligationsPage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const obligation = {
  id: 'obligation-1',
  tenantId: 'tenant-a',
  title: 'Synthetic privacy obligation',
  description: 'Synthetic description for local testing.',
  status: 'OPEN' as const,
  priority: 'CRITICAL' as const,
  sourceId: 'source-a',
  sourceReference: 'https://example.invalid/synthetic-source',
  sourceType: 'SYNTHETIC' as const,
  source: { authority: 'DEMO / SYNTHETIC Authority', documentTitle: 'Synthetic source', sourceUrl: 'https://example.invalid/synthetic-source', version: '1.0', retrievedAt: '2026-08-20T00:00:00Z', official: false },
  owner: 'Privacy Owner',
  dueDate: '2030-01-01T00:00:00Z',
  createdAt: '2026-08-18T00:00:00Z',
  updatedAt: '2026-08-20T00:00:00Z',
  reviewStatus: 'IN_REVIEW' as const,
  complianceStatus: 'NON_COMPLIANT' as const,
  dataMode: 'DEMO_SYNTHETIC' as const,
  isArchived: false,
};

const obligationsData: ObligationsData = {
  items: [obligation],
  summary: { total: 1, critical: 1, dueSoon: 0, overdue: 0, nonCompliant: 1, underReview: 1 },
  dataMode: 'DEMO_SYNTHETIC',
  generatedAt: '2026-08-20T00:00:00Z',
};

const details: ObligationDetails = {
  ...obligation,
  version: 1,
  relatedResources: {
    evidence: [{ id: 'evidence-1', label: 'Synthetic evidence', classification: 'INTERNAL', status: 'REQUIRES_REVIEW', expiresAt: null, createdAt: '2026-08-20T00:00:00Z' }],
    tasks: [{ id: 'task-1', title: 'Synthetic task', status: 'OPEN', priority: 'HIGH', dueAt: null, createdAt: '2026-08-20T00:00:00Z' }],
    risks: [{ id: 'risk-1', title: 'Synthetic risk', status: 'OPEN', severity: 'HIGH', owner: 'Risk Owner', createdAt: '2026-08-20T00:00:00Z' }],
    approvals: [{ id: 'approval-1', resourceType: 'TASK', decision: 'PENDING', requiredRoleCode: 'TENANT_ADMIN', createdAt: '2026-08-20T00:00:00Z', decidedAt: null }],
  },
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderObligationsPage = (initialData?: ObligationsData) => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (initialData) {
    queryClient.setQueryData(['obligations'], initialData);
  }
  const props = { onCreateTask: vi.fn(), onRegisterEvidence: vi.fn() };
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(<QueryClientProvider client={queryClient}><ObligationsPage {...props} /></QueryClientProvider>);
  });
  return { container, props };
};

afterEach(() => {
  act(() => {
    roots.splice(0).forEach((root) => root.unmount());
  });
  containers.splice(0).forEach((container) => container.remove());
  vi.restoreAllMocks();
});

describe('ObligationsPage', () => {
  it('renders real summary data, details, related resources, and supported action callbacks', async () => {
    vi.spyOn(nexusApi, 'getObligations').mockResolvedValue(obligationsData);
    vi.spyOn(nexusApi, 'getObligation').mockResolvedValue(details);
    const { container, props } = renderObligationsPage(obligationsData);

    expect(container.textContent).toContain('Synthetic privacy obligation');
    expect(container.textContent).toContain('1');
    const openButton = container.querySelector<HTMLButtonElement>('.obligations-table .button--quiet');
    expect(openButton).not.toBeNull();
    await act(async () => openButton?.click());
    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="dialog"]')).not.toBeNull());
    });
    expect(container.textContent).toContain('Synthetic evidence');
    expect(container.textContent).toContain('Synthetic task');
    expect(container.textContent).toContain('Synthetic risk');

    const actionButtons = [...container.querySelectorAll<HTMLButtonElement>('.obligation-details-actions button')];
    const evidenceButton = actionButtons.find((button) => button.textContent?.includes('Register evidence reference'));
    expect(evidenceButton).not.toBeUndefined();
    await act(async () => evidenceButton?.click());
    expect(props.onRegisterEvidence).toHaveBeenCalledWith('obligation-1');

    await act(async () => openButton?.click());
    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="dialog"]')).not.toBeNull());
    });
    const reopenedTaskButton = [...container.querySelectorAll<HTMLButtonElement>('.obligation-details-actions button')].find((button) => button.textContent?.includes('Create task'));
    expect(reopenedTaskButton).not.toBeUndefined();
    await act(async () => reopenedTaskButton?.click());
    expect(props.onCreateTask).toHaveBeenCalledWith('obligation-1');
  });

  it('renders loading and authorization-denied states from the safe IPC contract', async () => {
    let resolveRequest: ((value: ObligationsData) => void) | undefined;
    vi.spyOn(nexusApi, 'getObligations').mockImplementation(() => new Promise<ObligationsData>((resolve) => { resolveRequest = resolve; }));
    const { container } = renderObligationsPage();
    expect(container.textContent).toContain('Loading obligations');
    await act(async () => resolveRequest?.(obligationsData));

    vi.restoreAllMocks();
    vi.spyOn(nexusApi, 'getObligations').mockRejectedValue(new NexusApiError('AUTHORIZATION_DENIED', 'safe'));
    const denied = renderObligationsPage();
    await act(async () => {
      await vi.waitFor(() => expect(denied.container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(denied.container.querySelector('[role="alert"]')?.textContent).toContain('authorized');
  });

  it('surfaces safe conflicts for update and archive operations', async () => {
    vi.spyOn(nexusApi, 'getObligations').mockResolvedValue(obligationsData);
    vi.spyOn(nexusApi, 'getObligation').mockResolvedValue(details);
    const update = vi.spyOn(nexusApi, 'updateObligation').mockRejectedValue(new NexusApiError('CONFLICT', 'safe'));
    vi.spyOn(nexusApi, 'archiveObligation').mockRejectedValue(new NexusApiError('CONFLICT', 'safe'));
    const { container } = renderObligationsPage(obligationsData);
    const openButton = container.querySelector<HTMLButtonElement>('.obligations-table .button--quiet');
    await act(async () => openButton?.click());
    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('.obligation-details-actions')).not.toBeNull());
    });

    const editButton = [...container.querySelectorAll<HTMLButtonElement>('.obligation-details-actions button')].find((button) => button.textContent?.includes('Edit'));
    await act(async () => editButton?.click());
    const form = container.querySelector<HTMLFormElement>('.obligation-edit-form');
    expect(form).not.toBeNull();
    await act(async () => form?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('record changed'));
    });
    expect(update).toHaveBeenCalledWith(expect.objectContaining({ obligationId: 'obligation-1', expectedVersion: 1 }), expect.anything());

    const cancelButton = [...container.querySelectorAll<HTMLButtonElement>('.obligation-edit-form button')].find((button) => button.textContent?.includes('Cancel'));
    await act(async () => cancelButton?.click());
    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('.obligation-details-actions .button--danger')).not.toBeNull());
    });
    const archiveButton = container.querySelector<HTMLButtonElement>('.obligation-details-actions .button--danger');
    await act(async () => archiveButton?.click());
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('record changed'));
    });
  });

  it('keeps dialog keyboard focus contained and returns it after Escape', async () => {
    vi.spyOn(nexusApi, 'getObligations').mockResolvedValue(obligationsData);
    vi.spyOn(nexusApi, 'getObligation').mockResolvedValue(details);
    const { container } = renderObligationsPage(obligationsData);
    const openButton = container.querySelector<HTMLButtonElement>('.obligations-table .button--quiet');
    await act(async () => openButton?.click());
    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('.obligation-details-actions')).not.toBeNull());
    });
    const dialog = container.querySelector<HTMLElement>('[role="dialog"]');
    const focusable = dialog?.querySelectorAll<HTMLElement>('button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])');
    expect(focusable?.length).toBeGreaterThan(1);
    const first = focusable?.[0];
    const last = focusable?.[focusable.length - 1];
    expect(document.activeElement).toBe(first);

    last?.focus();
    await act(async () => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true })));
    expect(document.activeElement).toBe(first);

    first?.focus();
    await act(async () => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', shiftKey: true, bubbles: true })));
    expect(document.activeElement).toBe(last);

    await act(async () => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
    expect(container.querySelector('[role="dialog"]')).toBeNull();
    expect(document.activeElement).toBe(openButton);
  });
});
