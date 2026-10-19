import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { IntegrationStatusData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { SessionProvider } from '../../lib/session-context';
import { IntegrationsPage } from './IntegrationsPage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const connectedStatus: IntegrationStatusData = {
  providers: [{ providerId: 'OLLAMA_LOCAL', providerKind: 'LOCAL', state: 'CONNECTED', discoveredModelIds: ['llama3.2:3b', 'qwen3:4b'], checkedAt: '2026-10-04T00:00:00Z' }],
  enabledLocalModels: [],
  modelRegistryConfigurationAvailable: true,
  checkedAt: '2026-10-04T00:00:00Z',
};

const configuredStatus: IntegrationStatusData = {
  ...connectedStatus,
  enabledLocalModels: ['qwen3:4b'],
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderPage = (permissions?: readonly string[]): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(
      <QueryClientProvider client={queryClient}>
        {permissions ? <SessionProvider><IntegrationsPage /></SessionProvider> : <IntegrationsPage />}
      </QueryClientProvider>,
    );
  });
  return container;
};

beforeEach(async () => {
  await i18n.changeLanguage('en');
  vi.spyOn(nexusApi, 'getIntegrationStatus').mockResolvedValue(connectedStatus);
});

afterEach(async () => {
  act(() => {
    roots.splice(0).forEach((root) => root.unmount());
  });
  containers.splice(0).forEach((container) => container.remove());
  vi.restoreAllMocks();
  await i18n.changeLanguage('ar');
});

describe('Local model configuration (MASTER PHASE 03)', () => {
  it('offers selecting only live-discovered models and reports the configured one', async () => {
    const configureSpy = vi.spyOn(nexusApi, 'configureLocalModel').mockResolvedValue({ configuredModelId: 'qwen3:4b' });
    const container = renderPage();
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Current executable model'));
    });
    // النموذجان مكتشفان حياً؛ زرا الاختيار متاحان والتكوين فارغ بصدق.
    expect(container.textContent).toContain('None — execution fails closed');
    const selectButtons = Array.from(container.querySelectorAll<HTMLButtonElement>('button'))
      .filter((button) => button.textContent?.includes('Select'));
    expect(selectButtons.length).toBe(2);

    await act(async () => selectButtons.find((button) => button.textContent?.includes('qwen3:4b'))?.click());
    await act(async () => {
      await vi.waitFor(() => expect(configureSpy.mock.calls[0]?.[0]).toEqual({ modelId: 'qwen3:4b' }));
    });
    expect(container.textContent).toContain('qwen3:4b is now the single executable model.');
  });

  it('clears the configuration so execution fails closed again', async () => {
    vi.spyOn(nexusApi, 'getIntegrationStatus').mockResolvedValue(configuredStatus);
    const clearSpy = vi.spyOn(nexusApi, 'clearLocalModelConfiguration').mockResolvedValue({ configuredModelId: null });
    const container = renderPage();
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('qwen3:4b'));
    });
    const clearButton = Array.from(container.querySelectorAll<HTMLButtonElement>('button'))
      .find((button) => button.textContent?.includes('Clear configuration'));
    await act(async () => clearButton?.click());
    await act(async () => {
      await vi.waitFor(() => expect(clearSpy).toHaveBeenCalled());
    });
    expect(container.textContent).toContain('Executable model configuration cleared.');
  });

  it('hides configuration controls from an actor without integration:update', async () => {
    vi.spyOn(nexusApi, 'getSessionContext').mockResolvedValue({
      actorId: 'user_demo_reviewer', displayName: 'Reviewer', responsibility: 'REVIEWER', permissions: ['integration:read'],
    });
    const container = renderPage(['integration:read']);
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Current executable model'));
    });
    expect(Array.from(container.querySelectorAll('button')).find((button) => button.textContent?.includes('Select'))).toBeUndefined();
    // القائمة الاعتراضية تبقى مقروءة لصاحب integration:read.
    expect(container.textContent).toContain('discovered');
  });

  it('surfaces a safe backend rejection without internals', async () => {
    vi.spyOn(nexusApi, 'configureLocalModel').mockRejectedValue(new NexusApiError('MODEL_UNAVAILABLE', 'safe'));
    const container = renderPage();
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Select'));
    });
    const selectButton = Array.from(container.querySelectorAll<HTMLButtonElement>('button'))
      .find((button) => button.textContent?.includes('Select qwen3:4b'));
    await act(async () => selectButton?.click());
    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('No approved local model is available');
  });

  it('renders Arabic RTL configuration labels', async () => {
    const container = renderPage();
    await act(async () => {
      await i18n.changeLanguage('ar');
    });
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('النموذج التنفيذي الحالي'));
    });
    expect(document.documentElement.dir).toBe('rtl');
    expect(container.textContent).toContain('تعيين qwen3:4b');
  });
});
