import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AiModelsData, AiProvidersData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { SessionContext, type SessionState } from '../../lib/use-session';
import { ModelsPanel } from './ModelsPanel';
import { ProvidersPanel } from './ProvidersPanel';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const sessionWith = (permissions: readonly string[]): SessionState => ({
  actorId: 'test-actor',
  displayName: 'Test Actor',
  responsibility: 'ADMINISTRATOR',
  permissions,
  can: (permission) => permissions.includes(permission),
});

const render = (node: React.ReactNode, permissions: readonly string[]): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <SessionContext.Provider value={sessionWith(permissions)}>{node}</SessionContext.Provider>
      </QueryClientProvider>,
    );
  });
  return container;
};

// لا يحتوي أي حقل على مادة سرية؛ الحقل الوحيد المتاح علامة وجود.
const providersData: AiProvidersData = {
  providers: [
    {
      providerId: 'lm_studio_local',
      kind: 'LM_STUDIO',
      displayName: 'LM Studio (local)',
      baseUrl: 'http://127.0.0.1:1234/v1',
      enabled: true,
      connectionState: 'UNVERIFIED',
      connectionCheckedAt: null,
      credentialConfigured: false,
      requiresCredential: false,
      supportsDiscovery: true,
      modelCount: 0,
      builtIn: false,
    },
    {
      providerId: 'openai_hosted',
      kind: 'OPENAI',
      displayName: 'OpenAI (hosted)',
      baseUrl: 'https://api.openai.com/v1',
      enabled: true,
      connectionState: 'UNAVAILABLE',
      connectionCheckedAt: '2026-10-15T09:00:00Z',
      credentialConfigured: true,
      requiresCredential: true,
      supportsDiscovery: true,
      modelCount: 3,
      builtIn: false,
    },
  ],
  selectedProviderId: null,
  selectedModelId: null,
  dataMode: 'DEMO_SYNTHETIC',
  generatedAt: '2026-10-15T09:00:00Z',
};

const modelsData: AiModelsData = {
  models: [
    {
      modelId: 'qwen3:4b',
      providerId: 'ollama_local',
      providerKind: 'OLLAMA',
      providerDisplayName: 'Ollama (local)',
      displayName: 'qwen3:4b',
      available: true,
      selected: false,
      contextLimit: null,
      supportsTools: null,
      supportsStructuredOutput: null,
      supportsVision: null,
      capabilitySource: 'UNKNOWN',
      lastDiscoveredAt: '2026-10-15T09:00:00Z',
    },
    {
      modelId: 'gpt-4o-mini',
      providerId: 'openai_hosted',
      providerKind: 'OPENAI',
      providerDisplayName: 'OpenAI (hosted)',
      displayName: 'gpt-4o-mini',
      available: true,
      selected: true,
      contextLimit: 128000,
      supportsTools: true,
      supportsStructuredOutput: true,
      supportsVision: false,
      capabilitySource: 'DECLARED',
      lastDiscoveredAt: '2026-10-15T09:00:00Z',
    },
  ],
  selectedProviderId: 'openai_hosted',
  selectedModelId: 'gpt-4o-mini',
  dataMode: 'DEMO_SYNTHETIC',
  generatedAt: '2026-10-15T09:00:00Z',
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

describe('ProvidersPanel', () => {
  it('shows the truthful connection state and never upgrades an untested provider to connected', async () => {
    vi.spyOn(nexusApi, 'getAiProviders').mockResolvedValue(providersData);
    const container = render(<ProvidersPanel />, ['integration:read', 'integration:update', 'integration:credential']);

    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('LM Studio (local)'));
    });
    expect(container.textContent).toContain('Not verified yet');
    expect(container.textContent).toContain('Unavailable');
    // لا يظهر وصف «متصل» لأي موفر لم ينجح فحصه.
    expect(container.textContent).not.toContain('Connected');
  });

  it('never renders a credential value, only the presence marker', async () => {
    const secret = 'sk-live-do-not-leak-0123456789';
    vi.spyOn(nexusApi, 'getAiProviders').mockResolvedValue(providersData);
    vi.spyOn(nexusApi, 'setAiProviderCredential').mockImplementation(async (input) => {
      // حتى عند الإرسال، ما تُرجعه الواجهة الخلفية هو علامة لا قيمة.
      expect(input.credential).toBe(secret);
      return { providerId: input.providerId, credentialConfigured: true };
    });
    const container = render(<ProvidersPanel />, ['integration:read', 'integration:credential']);
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Credential stored'));
    });

    const openButton = [...container.querySelectorAll('button')].find((button) => button.textContent?.includes('Replace credential'));
    expect(openButton).toBeTruthy();
    await act(async () => { openButton!.click(); });

    const input = container.querySelector<HTMLInputElement>('input[type="password"]');
    expect(input).not.toBeNull();
    // الحقل يقبل قيمة جديدة ولا يعرض أي قيمة محفوظة.
    expect(input!.value).toBe('');

    const valueSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
    await act(async () => {
      valueSetter?.call(input, secret);
      input!.dispatchEvent(new Event('input', { bubbles: true }));
    });
    const save = [...container.querySelectorAll('button')].find((button) => button.textContent?.includes('Store credential'));
    await act(async () => { save!.click(); });

    // بعد الحفظ لا يبقى السر في الشاشة: لا في نص ولا في قيمة حقل.
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Credential stored in protected storage'));
    });
    expect(container.textContent).not.toContain(secret);
    expect(container.querySelectorAll('input[type="password"]').length).toBe(0);
  });

  it('hides credential controls from a session without the credential permission', async () => {
    vi.spyOn(nexusApi, 'getAiProviders').mockResolvedValue(providersData);
    const container = render(<ProvidersPanel />, ['integration:read', 'integration:update']);
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('LM Studio (local)'));
    });
    expect(container.textContent).not.toContain('Replace credential');
    expect(container.textContent).not.toContain('Set credential');
    // لكن إجراءات التهيئة تبقى متاحة لمن يملك صلاحية التحديث.
    expect(container.textContent).toContain('Test connection');
  });

  it('hides all mutating actions from a read-only session', async () => {
    vi.spyOn(nexusApi, 'getAiProviders').mockResolvedValue(providersData);
    const container = render(<ProvidersPanel />, ['integration:read']);
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('LM Studio (local)'));
    });
    expect(container.textContent).not.toContain('Test connection');
    expect(container.textContent).not.toContain('Add provider');
    expect(container.textContent).toContain('Refresh');
  });

  it('keeps the saved configuration after a failed connection test and surfaces only a safe code', async () => {
    vi.spyOn(nexusApi, 'getAiProviders').mockResolvedValue(providersData);
    vi.spyOn(nexusApi, 'testAiProvider').mockResolvedValue({
      providerId: 'openai_hosted',
      connectionState: 'UNAVAILABLE',
      checkedAt: '2026-10-15T09:05:00Z',
      latencyMs: 12,
      discoveredModelCount: 0,
      safeErrorCode: 'NETWORK_DESTINATION_DENIED',
    });
    const container = render(<ProvidersPanel />, ['integration:read', 'integration:update']);

    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('OpenAI (hosted)'));
    });
    const testButtons = [...container.querySelectorAll('button')].filter((button) => button.textContent?.includes('Test connection'));
    await act(async () => { testButtons[1].click(); });

    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('NETWORK_DESTINATION_DENIED'));
    });
    // الفشل لا يحذف صف الموفر ولا علامة المفتاح المحفوظ.
    expect(container.textContent).toContain('OpenAI (hosted)');
    expect(container.textContent).toContain('Credential stored');
  });

  it('renders an authorization-denied state when integration:read is missing', () => {
    vi.spyOn(nexusApi, 'getAiProviders').mockResolvedValue(providersData);
    const container = render(<ProvidersPanel />, []);
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('not authorized');
  });
});

describe('ModelsPanel', () => {
  it('shows unknown capabilities as unknown instead of inventing values', async () => {
    vi.spyOn(nexusApi, 'getAiModels').mockResolvedValue(modelsData);
    const container = render(<ModelsPanel />, ['integration:read', 'integration:update']);

    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('qwen3:4b'));
    });
    // النموذج المجهول القدرات: لا «0» ولا «مدعوم» ولا نافذة سياق مخترعة.
    expect(container.textContent).toContain('Tools: Unknown');
    expect(container.textContent).toContain('Context: Unknown');
    expect(container.textContent).toContain('source unknown');
    // والنموذج المعلن يظهر بقيمه الحقيقية.
    expect(container.textContent).toContain('Tools: yes');
    expect(container.textContent).toContain('Vision: no');
    expect(container.textContent).toContain('128000');
  });

  it('selects a model through the real IPC with its provider identity', async () => {
    vi.spyOn(nexusApi, 'getAiModels').mockResolvedValue(modelsData);
    const selectSpy = vi.spyOn(nexusApi, 'selectAiModel').mockResolvedValue({
      ...modelsData,
      selectedProviderId: 'ollama_local',
      selectedModelId: 'qwen3:4b',
    });
    const container = render(<ModelsPanel />, ['integration:read', 'integration:update']);
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('qwen3:4b'));
    });

    const selectButton = [...container.querySelectorAll('button')].find((button) => button.textContent?.trim() === 'Select');
    expect(selectButton).toBeTruthy();
    await act(async () => { selectButton!.click(); });

    // useMutation يمرّر سياقاً ثانياً؛ نتحقق من متغيّرات النداء وحدها.
    expect(selectSpy.mock.calls[0][0]).toEqual({ providerId: 'ollama_local', modelId: 'qwen3:4b' });
  });

  it('hides selection controls from a read-only session and renders Arabic RTL labels', async () => {
    vi.spyOn(nexusApi, 'getAiModels').mockResolvedValue(modelsData);
    const container = render(<ModelsPanel />, ['integration:read']);
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('qwen3:4b'));
    });
    expect(container.textContent).not.toContain('Clear model selection');

    await act(async () => {
      await i18n.changeLanguage('ar');
    });
    expect(document.documentElement.dir).toBe('rtl');
    expect(container.textContent).toContain('نماذج الذكاء الاصطناعي');
    expect(container.textContent).toContain('غير معروف');
  });

  it('renders a truthful empty state and a safe error without internals', async () => {
    vi.spyOn(nexusApi, 'getAiModels').mockResolvedValue({
      models: [],
      selectedProviderId: null,
      selectedModelId: null,
      dataMode: 'DEMO_SYNTHETIC',
      generatedAt: '2026-10-15T09:00:00Z',
    });
    const container = render(<ModelsPanel />, ['integration:read']);
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('No models are registered yet'));
    });
    expect(container.querySelector('.model-list')).toBeNull();

    vi.restoreAllMocks();
    vi.spyOn(nexusApi, 'getAiModels').mockRejectedValue(new NexusApiError('STORAGE_ERROR', 'INTERNAL_SQLITE_DETAILS'));
    const failing = render(<ModelsPanel />, ['integration:read']);
    await act(async () => {
      await vi.waitFor(() => expect(failing.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(failing.textContent).not.toContain('INTERNAL_SQLITE_DETAILS');
  });
});
