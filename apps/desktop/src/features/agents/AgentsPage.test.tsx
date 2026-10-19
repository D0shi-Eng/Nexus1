import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AgentMutationResult, AgentRunsData, AgentsData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { SessionProvider } from '../../lib/session-context';
import { AgentsPage } from './AgentsPage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const agentsData: AgentsData = {
  hasData: true,
  items: [
    { id: 'agent-a', tenantId: 'tenant-a', name: 'Governance definition', description: 'A governed local registry definition.', status: 'DISABLED', capability: 'GENERAL', createdAt: '2026-08-21T00:00:00Z', updatedAt: '2026-08-21T00:00:00Z' },
    { id: 'agent-b', tenantId: 'tenant-a', name: 'Evidence definition', description: 'An evidence-oriented local registry definition.', status: 'ACTIVE', capability: 'RESEARCH', createdAt: '2026-08-20T00:00:00Z', updatedAt: '2026-08-20T00:00:00Z' },
  ],
};

const savedAgent: AgentMutationResult = { agent: agentsData.items[0] };
const emptyAgentRunsData: AgentRunsData = { hasData: false, items: [] };
const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderPage = (initialData?: AgentsData): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (initialData) {
    queryClient.setQueryData(['agents'], initialData);
  }
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(<QueryClientProvider client={queryClient}><AgentsPage /></QueryClientProvider>);
  });
  return container;
};

const setInputValue = async (input: HTMLInputElement | HTMLTextAreaElement, value: string): Promise<void> => {
  await act(async () => {
    const descriptor = Object.getOwnPropertyDescriptor(
      input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype,
      'value',
    );
    descriptor?.set?.call(input, value);
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
  vi.spyOn(nexusApi, 'getAgentRuns').mockResolvedValue(emptyAgentRunsData);
});

afterEach(async () => {
  act(() => {
    roots.splice(0).forEach((root) => root.unmount());
  });
  containers.splice(0).forEach((container) => container.remove());
  vi.restoreAllMocks();
  await i18n.changeLanguage('ar');
});

describe('AgentsPage', () => {
  it('renders a loading state while the local agents query is pending', () => {
    vi.spyOn(nexusApi, 'getAgents').mockImplementation(() => new Promise(() => undefined));
    const container = renderPage();
    expect(container.textContent).toContain('Loading agent definitions');
  });

  it('renders persisted fields and supports search, status filtering, and no-results state', async () => {
    vi.spyOn(nexusApi, 'getAgents').mockResolvedValue(agentsData);
    const container = renderPage(agentsData);
    expect(container.textContent).toContain('Governance definition');
    expect(container.textContent).toContain('Evidence definition');

    const searchInput = container.querySelector<HTMLInputElement>('input');
    expect(searchInput).not.toBeNull();
    if (searchInput) {
      await setInputValue(searchInput, 'evidence');
    }
    expect(container.textContent).not.toContain('Governance definition');
    expect(container.textContent).toContain('Evidence definition');

    const filter = container.querySelector<HTMLSelectElement>('select');
    expect(filter).not.toBeNull();
    if (filter) {
      await setSelectValue(filter, 'DISABLED');
    }
    expect(container.textContent).toContain('No agent definitions match the current search or status filter.');

    const clearButton = Array.from(container.querySelectorAll<HTMLButtonElement>('button'))
      .find((button) => button.textContent?.includes('Clear filters'));
    await act(async () => clearButton?.click());
    expect(container.textContent).toContain('Governance definition');
  });

  it('renders a real empty state without fabricated agent metrics', () => {
    const emptyData: AgentsData = { hasData: false, items: [] };
    vi.spyOn(nexusApi, 'getAgents').mockResolvedValue(emptyData);
    const container = renderPage(emptyData);
    expect(container.textContent).toContain('No agent definitions have been created in this workspace.');
    expect(container.querySelector('table')).toBeNull();
    expect(container.querySelector('.metric-grid')).toBeNull();
  });

  it('renders a separate persisted-runs area with an honest empty state and no execution controls', async () => {
    vi.spyOn(nexusApi, 'getAgents').mockResolvedValue(agentsData);
    const container = renderPage(agentsData);
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('No run request records exist in this workspace.'));
    });
    expect(container.textContent).not.toContain('Execute');
    expect(container.textContent).not.toContain('Start run');
    expect(container.textContent).not.toContain('Token');
    expect(container.textContent).not.toContain('Provider');
  });

  it('renders authorization-denied and generic safe errors without internal details', async () => {
    vi.spyOn(nexusApi, 'getAgents').mockRejectedValueOnce(new NexusApiError('AUTHORIZATION_DENIED', 'safe'));
    const deniedContainer = renderPage();
    await act(async () => {
      await vi.waitFor(() => expect(deniedContainer.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(deniedContainer.querySelector('[role="alert"]')?.textContent).toContain('not authorized');

    vi.spyOn(nexusApi, 'getAgents').mockRejectedValueOnce(new NexusApiError('STORAGE_ERROR', 'INTERNAL_SQLITE_DETAILS'));
    const errorContainer = renderPage();
    await act(async () => {
      await vi.waitFor(() => expect(errorContainer.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(errorContainer.querySelector('[role="alert"]')?.textContent).not.toContain('INTERNAL_SQLITE_DETAILS');
  });

  it('validates creation and performs a typed create mutation through the dialog', async () => {
    vi.spyOn(nexusApi, 'getAgents').mockResolvedValue(agentsData);
    const createSpy = vi.spyOn(nexusApi, 'createAgent').mockResolvedValue(savedAgent);
    const container = renderPage(agentsData);
    const createButton = Array.from(container.querySelectorAll<HTMLButtonElement>('button'))
      .find((button) => button.textContent?.includes('Create agent'));
    await act(async () => createButton?.click());
    const form = container.querySelector<HTMLFormElement>('form');
    expect(form).not.toBeNull();
    if (form) {
      await act(async () => form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
    }
    expect(container.textContent).toContain('Provide a name of up to 160 characters');

    const fields = container.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>('form input, form textarea');
    await setInputValue(fields[0], 'Registry definition');
    await setInputValue(fields[1], 'A controlled definition that does not execute an agent.');
    if (form) {
      await act(async () => form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
    }
    await act(async () => {
      await vi.waitFor(() => expect(createSpy).toHaveBeenCalled());
    });
    expect(createSpy.mock.calls[0]?.[0]).toEqual({ name: 'Registry definition', description: 'A controlled definition that does not execute an agent.' });
    expect(container.textContent).toContain('was saved and recorded in the local audit trail');
  });

  it('supports edit and enable-disable mutations with accessible semantic controls', async () => {
    vi.spyOn(nexusApi, 'getAgents').mockResolvedValue(agentsData);
    const updateSpy = vi.spyOn(nexusApi, 'updateAgent').mockResolvedValue(savedAgent);
    const statusSpy = vi.spyOn(nexusApi, 'setAgentStatus').mockResolvedValue(savedAgent);
    const container = renderPage(agentsData);
    const editButton = Array.from(container.querySelectorAll<HTMLButtonElement>('button'))
      .find((button) => button.textContent?.includes('Edit'));
    await act(async () => editButton?.click());
    const fields = container.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>('form input, form textarea');
    await setInputValue(fields[0], 'Updated registry definition');
    const form = container.querySelector<HTMLFormElement>('form');
    if (form) {
      await act(async () => form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
    }
    await act(async () => {
      await vi.waitFor(() => expect(updateSpy).toHaveBeenCalled());
    });
    expect(updateSpy.mock.calls[0]?.[0]).toEqual(expect.objectContaining({ agentId: 'agent-a', name: 'Updated registry definition' }));

    const enableButton = Array.from(container.querySelectorAll<HTMLButtonElement>('button'))
      .find((button) => button.textContent?.includes('Enable'));
    await act(async () => enableButton?.click());
    await act(async () => {
      await vi.waitFor(() => expect(statusSpy).toHaveBeenCalled());
    });
    expect(statusSpy.mock.calls[0]?.[0]).toEqual({ agentId: 'agent-a', status: 'ACTIVE' });
    expect(container.querySelector('section[aria-live="polite"]')).not.toBeNull();
    expect(container.querySelector('table thead')).not.toBeNull();
  });

  it('switches between English LTR and Arabic RTL and provides an accessible modal escape path', async () => {
    vi.spyOn(nexusApi, 'getAgents').mockResolvedValue(agentsData);
    const container = renderPage(agentsData);
    expect(document.documentElement.dir).toBe('ltr');
    const createButton = Array.from(container.querySelectorAll<HTMLButtonElement>('button'))
      .find((button) => button.textContent?.includes('Create agent'));
    await act(async () => createButton?.click());
    expect(container.querySelector('[role="dialog"]')).not.toBeNull();
    await act(async () => window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
    expect(container.querySelector('[role="dialog"]')).toBeNull();
    await act(async () => {
      await i18n.changeLanguage('ar');
    });
    expect(document.documentElement.dir).toBe('rtl');
    expect(container.textContent).toContain('سجل وكلاء محكوم');
  });

  it('hides mutation controls from an actor lacking agent write permissions', async () => {
    vi.spyOn(nexusApi, 'getAgents').mockResolvedValue(agentsData);
    vi.spyOn(nexusApi, 'getSessionContext').mockResolvedValue({
      actorId: 'user_demo_reviewer',
      displayName: 'NEXUS Demo Reviewer',
      responsibility: 'REVIEWER',
      permissions: ['agent:read'],
    });
    const container = document.createElement('div');
    const root = createRoot(container);
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    document.body.append(container);
    roots.push(root);
    containers.push(container);
    await act(async () => {
      root.render(
        <QueryClientProvider client={queryClient}>
          <SessionProvider>
            <AgentsPage />
          </SessionProvider>
        </QueryClientProvider>,
      );
    });

    // ننتظر وصول سياق الجلسة من الخلف ثم نتحقق من الإخفاء.
    await act(async () => {
      await vi.waitFor(() => {
        expect(container.textContent).not.toContain('Create agent');
      });
    });

    // لا زر إنشاء ولا إجراءات تعديل/تفعيل — الواجهة لا تعرض ما لا يملكه الفاعل.
    expect(container.textContent).not.toContain('Create agent');
    expect(container.textContent).not.toContain('Edit');
    expect(container.textContent).not.toContain('Enable');
    // القراءة نفسها سليمة.
    await act(async () => {
      await vi.waitFor(() => {
        expect(container.querySelector('table')).not.toBeNull();
      });
    });
  });
});
