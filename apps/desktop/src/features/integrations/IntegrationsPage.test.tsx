import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { IntegrationStatusData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { IntegrationsPage } from './IntegrationsPage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const statusData: IntegrationStatusData = {
  providers: [
    {
      providerId: 'OLLAMA_LOCAL',
      providerKind: 'LOCAL',
      state: 'UNAVAILABLE',
      discoveredModelIds: [],
      checkedAt: '2026-10-02T00:00:00Z',
    },
  ],
  enabledLocalModels: [],
  modelRegistryConfigurationAvailable: false,
  checkedAt: '2026-10-02T00:00:00Z',
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderPage = (data?: IntegrationStatusData): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (data) {
    queryClient.setQueryData(['integration-status'], data);
  }
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(<QueryClientProvider client={queryClient}><IntegrationsPage /></QueryClientProvider>);
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

describe('IntegrationsPage', () => {
  it('shows the supported local provider with its actual probed state', () => {
    const container = renderPage(statusData);

    expect(container.textContent).toContain('OLLAMA_LOCAL');
    expect(container.textContent).toContain('Local provider (loopback only)');
    expect(container.textContent).toContain('Unavailable');
    // «مدعوم» لا تعني «متصل» — النص الصريح موجود.
    expect(container.textContent).toContain('Supported does not mean connected');
    expect(container.textContent).toContain('No cloud providers');
  });

  it('lists discovered models only when the provider is actually connected', () => {
    const container = renderPage({
      ...statusData,
      providers: [{ ...statusData.providers[0], state: 'CONNECTED', discoveredModelIds: ['qwen3:4b'] }],
    });

    expect(container.textContent).toContain('Connected');
    expect(container.textContent).toContain('qwen3:4b');
  });

  it('reports the empty model registry truthfully and marks configuration unavailable', () => {
    const container = renderPage(statusData);

    expect(container.textContent).toContain('No local model is enabled in the registry');
    expect(container.textContent).toContain('Model registry configuration is not available in this phase');
  });

  it('renders an authorization-denied safe error', async () => {
    vi.spyOn(nexusApi, 'getIntegrationStatus').mockRejectedValue(
      new NexusApiError('AUTHORIZATION_DENIED', 'denied'),
    );
    const container = renderPage();
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('You are not authorized'));
    });

    expect(container.textContent).toContain('You are not authorized to read integration status');
  });

  // MP05 / C10: الفحص الحي إجراء صريح بطلب المستخدم — لا فحص دوري ولا ادعاء اتصال دائم.
  it('runs the live provider check only on explicit user request and reports the real outcome', async () => {
    const checkSpy = vi.spyOn(nexusApi, 'checkAiProvider').mockResolvedValue({
      reachable: true,
      checkedAt: '2026-10-15T05:00:00Z',
    });
    const container = renderPage(statusData);

    // لا نتيجة ولا اتصال مزعوم قبل أن يطلبه المستخدم.
    expect(container.textContent).toContain('It runs only when you ask for it');
    expect(checkSpy).not.toHaveBeenCalled();

    const button = [...container.querySelectorAll('button')]
      .find((candidate) => candidate.textContent?.includes('Check provider now'));
    expect(button).toBeDefined();

    await act(async () => {
      button?.click();
      await vi.waitFor(() => expect(checkSpy).toHaveBeenCalledTimes(1));
    });
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('The local provider responded'));
    });
  });

  it('keeps the provider check fail-closed when the backend refuses it', async () => {
    vi.spyOn(nexusApi, 'checkAiProvider').mockRejectedValue(
      new NexusApiError('AUTHORIZATION_DENIED', 'denied'),
    );
    const container = renderPage(statusData);

    const button = [...container.querySelectorAll('button')]
      .find((candidate) => candidate.textContent?.includes('Check provider now'));
    await act(async () => {
      button?.click();
      await vi.waitFor(() =>
        expect(container.textContent).toContain('not authorized to check the AI provider'),
      );
    });

    // لا تُعرض أي نتيجة «متصل» عند الرفض.
    expect(container.textContent).not.toContain('The local provider responded');
  });
});
