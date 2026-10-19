import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { AttentionData } from '@nexus/contracts';
import '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { AttentionPage } from './AttentionPage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const attentionData: AttentionData = {
  dataMode: 'DEMO_SYNTHETIC',
  generatedAt: '2026-08-20T00:00:00Z',
  summary: { total: 1, critical: 1, high: 0, medium: 0, low: 0, pending: 0, overdue: 1, requiresReview: 0 },
  items: [{
    id: 'attention-obligation-1',
    kind: 'OBLIGATION',
    severity: 'CRITICAL',
    title: 'Synthetic obligation',
    status: 'AT_RISK',
    reasonCode: 'OBLIGATION_AT_RISK',
    source: 'LOCAL_WORKSPACE',
    dueAt: '2020-01-01T00:00:00Z',
    resourceId: 'obligation-1',
    relatedEntity: { kind: 'OBLIGATION', id: 'obligation-1', title: 'Synthetic obligation' },
    owner: 'Owner',
    updatedAt: '2026-08-20T00:00:00Z',
    availableActions: ['OPEN', 'REVIEW', 'CREATE_TASK', 'REGISTER_EVIDENCE'],
  }],
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderAttentionPage = (
  overrides?: Partial<Parameters<typeof AttentionPage>[0]>,
  initialData?: AttentionData,
) => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (initialData) {
    queryClient.setQueryData(['attention'], initialData);
  }
  const props = {
    obligationIds: ['obligation-1'],
    onCreateTask: vi.fn(),
    onRegisterEvidence: vi.fn(),
    onDecideApproval: vi.fn(),
    onOpenObligation: vi.fn(),
    ...overrides,
  };

  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(<QueryClientProvider client={queryClient}><AttentionPage {...props} /></QueryClientProvider>);
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

describe('AttentionPage', () => {
  it('renders fetched attention data, opens details, and invokes the real task action callback', async () => {
    vi.spyOn(nexusApi, 'getAttention').mockResolvedValue(attentionData);
    const { container, props } = renderAttentionPage(undefined, attentionData);

    expect(container.textContent).toContain('Synthetic obligation');
    expect(container.textContent).toContain('1');

    const reviewButton = container.querySelector<HTMLButtonElement>('.attention-card__actions .button--secondary');
    expect(reviewButton).not.toBeNull();
    await act(async () => reviewButton?.click());
    expect(container.querySelector('[role="dialog"]')).not.toBeNull();

    const createTaskButton = container.querySelector<HTMLButtonElement>('.attention-details__actions .button--primary');
    expect(createTaskButton).not.toBeNull();
    await act(async () => createTaskButton?.click());
    expect(props.onCreateTask).toHaveBeenCalledWith('obligation-1');
  });

  it('renders the authorization-denied state from the safe IPC error code', async () => {
    vi.spyOn(nexusApi, 'getAttention').mockRejectedValue(new NexusApiError('AUTHORIZATION_DENIED', 'safe'));
    const { container } = renderAttentionPage();

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('authorized');
  });
});
