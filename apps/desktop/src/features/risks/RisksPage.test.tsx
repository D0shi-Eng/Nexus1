import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { RisksData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { RisksPage } from './RisksPage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const risksData: RisksData = {
  items: [
    { id: 'risk-critical', title: 'Critical data exposure', severity: 'CRITICAL', likelihood: 4, impact: 5, status: 'OPEN', owner: 'Risk Manager', createdAt: '2026-08-18T00:00:00Z', updatedAt: '2026-08-20T00:00:00Z' },
    { id: 'risk-high', title: 'Supplier dependency', severity: 'HIGH', likelihood: 3, impact: 4, status: 'MITIGATING', owner: 'Security Officer', createdAt: '2026-08-17T00:00:00Z', updatedAt: '2026-08-19T00:00:00Z' },
    { id: 'risk-closed', title: 'Resolved access concern', severity: 'LOW', likelihood: 1, impact: 2, status: 'CLOSED', owner: 'Privacy Officer', createdAt: '2026-08-16T00:00:00Z', updatedAt: '2026-08-18T00:00:00Z' },
  ],
  summary: { total: 3, open: 1, mitigating: 1, accepted: 0, closed: 1 },
  dataMode: 'DEMO_SYNTHETIC',
  generatedAt: '2026-08-20T00:00:00Z',
  hasData: true,
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderPage = (initialData?: RisksData): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (initialData) {
    queryClient.setQueryData(['risks'], initialData);
  }

  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(<QueryClientProvider client={queryClient}><RisksPage /></QueryClientProvider>);
  });

  return container;
};

const setInputValue = async (input: HTMLInputElement, value: string): Promise<void> => {
  await act(async () => {
    const valueSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
    valueSetter?.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
};

const setSelectValue = async (select: HTMLSelectElement, value: string): Promise<void> => {
  await act(async () => {
    const valueSetter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set;
    valueSetter?.call(select, value);
    select.dispatchEvent(new Event('change', { bubbles: true }));
  });
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

describe('RisksPage', () => {
  it('renders the loading state while the local query is pending', () => {
    vi.spyOn(nexusApi, 'getRisks').mockImplementation(() => new Promise(() => undefined));
    const container = renderPage();

    expect(container.textContent).toContain('Loading local risks');
  });

  it('renders stored fields and supports typed search, filters, and no-match state', async () => {
    vi.spyOn(nexusApi, 'getRisks').mockResolvedValue(risksData);
    const container = renderPage(risksData);

    expect(container.textContent).toContain('Critical data exposure');
    expect(container.textContent).toContain('Supplier dependency');
    expect(container.textContent).toContain('Resolved access concern');
    expect(container.textContent).toContain('3');

    const searchInput = container.querySelector<HTMLInputElement>('input');
    expect(searchInput).not.toBeNull();
    if (searchInput) {
      await setInputValue(searchInput, 'supplier');
    }
    expect(container.textContent).not.toContain('Critical data exposure');
    expect(container.textContent).toContain('Supplier dependency');

    const selects = container.querySelectorAll<HTMLSelectElement>('select');
    expect(selects).toHaveLength(2);
    await setSelectValue(selects[0], 'OPEN');
    expect(container.textContent).toContain('No risks match the current search or filters.');

    const clearButton = Array.from(container.querySelectorAll<HTMLButtonElement>('button'))
      .find((button) => button.textContent?.includes('Clear filters'));
    expect(clearButton).not.toBeNull();
    await act(async () => clearButton?.click());
    expect(container.textContent).toContain('Critical data exposure');

    await setSelectValue(selects[1], 'LOW');
    expect(container.textContent).toContain('Resolved access concern');
    expect(container.textContent).not.toContain('Supplier dependency');
  });

  it('renders an explicit empty state without manufacturing summary metrics', () => {
    vi.spyOn(nexusApi, 'getRisks').mockResolvedValue({
      items: [],
      summary: { total: 0, open: 0, mitigating: 0, accepted: 0, closed: 0 },
      dataMode: 'DEMO_SYNTHETIC',
      generatedAt: '2026-08-20T00:00:00Z',
      hasData: false,
    });
    const container = renderPage({
      items: [],
      summary: { total: 0, open: 0, mitigating: 0, accepted: 0, closed: 0 },
      dataMode: 'DEMO_SYNTHETIC',
      generatedAt: '2026-08-20T00:00:00Z',
      hasData: false,
    });

    expect(container.textContent).toContain('There are no risks recorded in the current workspace.');
    expect(container.querySelector('table')).toBeNull();
    expect(container.querySelector('.metric-grid')).toBeNull();
  });

  it('renders an authorization-denied safe error', async () => {
    vi.spyOn(nexusApi, 'getRisks').mockRejectedValue(new NexusApiError('AUTHORIZATION_DENIED', 'safe'));
    const container = renderPage();

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('not authorized');
  });

  it('renders a generic safe error without exposing internal details', async () => {
    vi.spyOn(nexusApi, 'getRisks').mockRejectedValue(new NexusApiError('STORAGE_ERROR', 'INTERNAL_SQLITE_DETAILS'));
    const container = renderPage();

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('storage operation');
    expect(container.textContent).not.toContain('INTERNAL_SQLITE_DETAILS');
  });

  it('uses semantic controls and switches between English LTR and Arabic RTL', async () => {
    vi.spyOn(nexusApi, 'getRisks').mockResolvedValue(risksData);
    const container = renderPage(risksData);

    expect(document.documentElement.dir).toBe('ltr');
    expect(container.querySelector('section[aria-live="polite"]')).not.toBeNull();
    expect(container.querySelector('input')).not.toBeNull();
    expect(container.querySelectorAll('select')).toHaveLength(2);
    expect(container.querySelector('button')).not.toBeNull();

    await act(async () => {
      await i18n.changeLanguage('ar');
    });
    expect(document.documentElement.dir).toBe('rtl');
    expect(container.textContent).toContain('المخاطر');
  });
});
