import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { OrganizationalIntelligenceData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { OrganizationalIntelligencePage } from './OrganizationalIntelligencePage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const intelligenceData: OrganizationalIntelligenceData = {
  organization: { legalName: 'NEXUS DEMO / SYNTHETIC Organization', countryCode: 'SA', industry: 'Technology Services' },
  sources: { total: 3, humanVerified: 1, requiresReview: 2 },
  operations: {
    activeObligations: 4,
    atRiskObligations: 1,
    overdueObligations: 1,
    evidenceGaps: 2,
    activeTasks: 1,
    blockedTasks: 1,
    activeRisks: 1,
    pendingApprovals: 1,
  },
  priorities: [
    {
      id: 'obligation-1',
      kind: 'OBLIGATION',
      title: 'Synthetic evidence gap',
      severity: 'CRITICAL',
      state: 'AT_RISK',
      reason: 'OBLIGATION_AT_RISK',
      owner: 'Privacy Officer',
      dueAt: '2020-01-01T00:00:00Z',
      updatedAt: '2026-08-20T00:00:00Z',
    },
    {
      id: 'risk-1',
      kind: 'RISK',
      title: 'Synthetic risk exposure',
      severity: 'HIGH',
      state: 'OPEN',
      reason: 'RISK_ACTIVE',
      owner: 'Risk Manager',
      dueAt: null,
      updatedAt: '2026-08-20T00:00:00Z',
    },
  ],
  dataMode: 'DEMO_SYNTHETIC',
  generatedAt: '2026-08-20T00:00:00Z',
  hasData: true,
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderPage = (initialData?: OrganizationalIntelligenceData) => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (initialData) {
    queryClient.setQueryData(['organizational-intelligence'], initialData);
  }

  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(<QueryClientProvider client={queryClient}><OrganizationalIntelligencePage /></QueryClientProvider>);
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

describe('OrganizationalIntelligencePage', () => {
  it('renders the loading state while the local query is pending', () => {
    vi.spyOn(nexusApi, 'getOrganizationalIntelligence').mockImplementation(() => new Promise(() => undefined));
    const container = renderPage();

    expect(container.textContent).toContain('Loading the local organizational summary');
  });

  it('shows the read-only status badge like the other read-only modules', async () => {
    vi.spyOn(nexusApi, 'getOrganizationalIntelligence').mockResolvedValue(intelligenceData);
    const container = renderPage(intelligenceData);

    // انحدار A1: شارة «قراءة فقط» ظاهرة في رأس الصفحة (moduleAvailability.status.READ_ONLY).
    expect(container.textContent).toContain('Read only');
  });

  it('renders only derived data and filters the deterministic priority list', async () => {
    vi.spyOn(nexusApi, 'getOrganizationalIntelligence').mockResolvedValue(intelligenceData);
    const container = renderPage(intelligenceData);

    expect(container.textContent).toContain('NEXUS DEMO / SYNTHETIC Organization');
    expect(container.textContent).toContain('Synthetic evidence gap');
    expect(container.textContent).toContain('Synthetic risk exposure');
    expect(container.textContent).toContain('4');

    const searchInput = container.querySelector<HTMLInputElement>('input');
    expect(searchInput).not.toBeNull();
    await act(async () => {
      if (searchInput) {
        const valueSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
        valueSetter?.call(searchInput, 'risk');
        searchInput.dispatchEvent(new Event('input', { bubbles: true }));
      }
    });
    expect(container.textContent).not.toContain('Synthetic evidence gap');
    expect(container.textContent).toContain('Synthetic risk exposure');

    const clearButton = Array.from(container.querySelectorAll<HTMLButtonElement>('button'))
      .find((button) => button.textContent?.includes('Clear filters'));
    expect(clearButton).not.toBeNull();
    await act(async () => clearButton?.click());
    expect(container.textContent).toContain('Synthetic evidence gap');
  });

  it('renders an explicit no-data state without manufacturing metrics', () => {
    vi.spyOn(nexusApi, 'getOrganizationalIntelligence').mockResolvedValue({
      ...intelligenceData,
      organization: null,
      sources: { total: 0, humanVerified: 0, requiresReview: 0 },
      operations: { activeObligations: 0, atRiskObligations: 0, overdueObligations: 0, evidenceGaps: 0, activeTasks: 0, blockedTasks: 0, activeRisks: 0, pendingApprovals: 0 },
      priorities: [],
      hasData: false,
    });
    const container = renderPage({
      ...intelligenceData,
      organization: null,
      sources: { total: 0, humanVerified: 0, requiresReview: 0 },
      operations: { activeObligations: 0, atRiskObligations: 0, overdueObligations: 0, evidenceGaps: 0, activeTasks: 0, blockedTasks: 0, activeRisks: 0, pendingApprovals: 0 },
      priorities: [],
      hasData: false,
    });

    expect(container.textContent).toContain('There is not enough local data');
    expect(container.querySelector('table')).toBeNull();
  });

  it('renders an authorization-denied safe error', async () => {
    vi.spyOn(nexusApi, 'getOrganizationalIntelligence').mockRejectedValue(new NexusApiError('AUTHORIZATION_DENIED', 'safe'));
    const container = renderPage();

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('not authorized');
  });

  it('renders a generic safe error without exposing internal details', async () => {
    vi.spyOn(nexusApi, 'getOrganizationalIntelligence').mockRejectedValue(new NexusApiError('STORAGE_ERROR', 'safe'));
    const container = renderPage();

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('storage operation');
  });

  it('uses semantic controls and switches to Arabic RTL translations', async () => {
    vi.spyOn(nexusApi, 'getOrganizationalIntelligence').mockResolvedValue(intelligenceData);
    const container = renderPage(intelligenceData);

    expect(document.documentElement.dir).toBe('ltr');
    expect(container.querySelector('section[aria-live="polite"]')).not.toBeNull();
    expect(container.querySelector('input')).not.toBeNull();
    expect(container.querySelector('select')).not.toBeNull();
    expect(container.querySelector('button')).not.toBeNull();

    await act(async () => {
      await i18n.changeLanguage('ar');
    });
    expect(document.documentElement.dir).toBe('rtl');
    expect(container.textContent).toContain('المؤسسة');
    // انحدار A1: شارة «قراءة فقط» ظاهرة بالعربية أيضاً — عيب الفحص اليدوي مغلق.
    expect(container.textContent).toContain('قراءة فقط');
  });
});
