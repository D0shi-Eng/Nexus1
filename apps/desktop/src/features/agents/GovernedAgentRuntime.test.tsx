import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type {
  AgentExecutionProfilesData, AgentRunResultData, AgentRunsData, AgentsData,
  IntegrationStatusData, SkillCatalogData, StartExecutionOutcome, ToolCatalogData,
  ToolInvocationsData,
} from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { nexusApi } from '../../lib/nexus-api';
import { SessionProvider } from '../../lib/session-context';
import { AgentsPage } from './AgentsPage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const activeAgent: AgentsData['items'][number] = {
  id: 'agent-live', tenantId: 'tenant-a', name: 'Live governed agent',
  description: 'Configured for governed execution.', status: 'ACTIVE', capability: 'GENERAL',
  createdAt: '2026-10-01T00:00:00Z', updatedAt: '2026-10-01T00:00:00Z',
};

const profilesData: AgentExecutionProfilesData = {
  hasData: true,
  items: [{ id: 'profile-1', tenantId: 'tenant-a', agentId: 'agent-live', versionNumber: 1, instruction: 'Summarize bounded obligations.', createdAt: '2026-10-01T00:00:00Z' }],
  latestVersionId: 'profile-1',
};

const integrationData: IntegrationStatusData = {
  providers: [{ providerId: 'OLLAMA_LOCAL', providerKind: 'LOCAL', state: 'CONNECTED', discoveredModelIds: ['qwen3:4b'], checkedAt: '2026-10-04T00:00:00Z' }],
  enabledLocalModels: ['qwen3:4b'],
  modelRegistryConfigurationAvailable: true,
  checkedAt: '2026-10-04T00:00:00Z',
};

const skillCatalog: SkillCatalogData = {
  hasData: true,
  items: [{
    skill: { id: 'skill-1', tenantId: 'tenant-a', name: 'Bounded summarizer', description: 'Summarizes within bounds.', status: 'ACTIVE', createdAt: '2026-10-01T00:00:00Z', updatedAt: '2026-10-01T00:00:00Z' },
    versions: [{ id: 'skill-version-1', tenantId: 'tenant-a', skillId: 'skill-1', skillStatus: 'ACTIVE', versionNumber: 1, instruction: 'Summarize.', inputContract: 'text', outputContract: 'text', constraints: 'bounded', createdAt: '2026-10-01T00:00:00Z' }],
    assignments: [{ id: 'assignment-1', agentId: 'agent-live', skillVersionId: 'skill-version-1', assignedAt: '2026-10-01T00:00:00Z', unassignedAt: null }],
  }],
};

const toolCatalog: ToolCatalogData = {
  hasData: true,
  items: [{
    tool: { id: 'tool-1', tenantId: 'tenant-a', name: 'Obligation Bounded List (Built-in)', description: 'Read-only built-in.', status: 'ACTIVE', createdAt: '2026-10-01T00:00:00Z', updatedAt: '2026-10-01T00:00:00Z' },
    versions: [
      { id: 'tool-version-1', versionNumber: 1, effect: { mutation: 'NONE', externality: 'INTERNAL', privilege: 'STANDARD', reversibility: 'NOT_APPLICABLE' }, executable: true },
    ],
  }, {
    tool: { id: 'tool-2', tenantId: 'tenant-a', name: 'Tenant authored tool', description: 'Registered only.', status: 'ACTIVE', createdAt: '2026-10-01T00:00:00Z', updatedAt: '2026-10-01T00:00:00Z' },
    versions: [
      { id: 'tool-version-2', versionNumber: 1, effect: { mutation: 'NONE', externality: 'INTERNAL', privilege: 'STANDARD', reversibility: 'NOT_APPLICABLE' }, executable: false },
    ],
  }],
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderPage = (options?: { readonly permissions?: readonly string[] }): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  const page = (
    <QueryClientProvider client={queryClient}>
      {options?.permissions ? <SessionProvider><AgentsPage /></SessionProvider> : <AgentsPage />}
    </QueryClientProvider>
  );
  act(() => {
    root.render(page);
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

const clickButton = async (container: HTMLElement, label: string): Promise<void> => {
  const button = Array.from(container.querySelectorAll<HTMLButtonElement>('button'))
    .find((candidate) => candidate.textContent?.includes(label));
  await act(async () => button?.click());
};

beforeEach(async () => {
  await i18n.changeLanguage('en');
  vi.spyOn(nexusApi, 'getAgents').mockResolvedValue({ hasData: true, items: [activeAgent] });
  vi.spyOn(nexusApi, 'getAgentRuns').mockResolvedValue({ hasData: false, items: [] });
  vi.spyOn(nexusApi, 'getAgentExecutionProfiles').mockResolvedValue(profilesData);
  vi.spyOn(nexusApi, 'getIntegrationStatus').mockResolvedValue(integrationData);
  vi.spyOn(nexusApi, 'getSkillCatalog').mockResolvedValue(skillCatalog);
  vi.spyOn(nexusApi, 'getToolCatalog').mockResolvedValue(toolCatalog);
});

afterEach(async () => {
  act(() => {
    roots.splice(0).forEach((root) => root.unmount());
  });
  containers.splice(0).forEach((container) => container.remove());
  vi.restoreAllMocks();
  await i18n.changeLanguage('ar');
});

describe('Governed agent runtime surfaces (MASTER PHASE 03)', () => {
  it('opens the run dialog with truthful readiness state and submits a governed request', async () => {
    const requestSpy = vi.spyOn(nexusApi, 'requestAgentExecution').mockResolvedValue({
      run: { id: 'agent_run_1', tenantId: 'tenant-a', agentId: 'agent-live', requestedByUserId: 'op', status: 'WAITING_APPROVAL', requestSummary: 'GOVERNED_EXECUTION_REQUEST', revision: 1, startedAt: null, completedAt: null, createdAt: '', updatedAt: '' },
      approvalId: 'approval-1',
    });
    const container = renderPage();
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Live governed agent'));
    });
    await clickButton(container, 'Run');

    const dialog = container.querySelector('[role="dialog"]');
    expect(dialog).not.toBeNull();
    // حالة الجاهزية الصادقة: البروفايل والنموذج مكوّنان فتُقبل المهمة.
    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="dialog"]')?.textContent).toContain('qwen3:4b'));
    });
    expect(container.querySelector('[role="dialog"]')?.textContent).toContain('Configured');

    const taskArea = container.querySelector<HTMLTextAreaElement>('[role="dialog"] textarea');
    await setInputValue(taskArea!, 'Summarize the obligations at risk.');
    const form = container.querySelector<HTMLFormElement>('[role="dialog"] form');
    await act(async () => form?.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true })));
    await act(async () => {
      await vi.waitFor(() => expect(requestSpy).toHaveBeenCalled());
    });
    expect(requestSpy.mock.calls[0]?.[0]).toEqual({ agentId: 'agent-live', taskContent: 'Summarize the obligations at risk.', skillVersionId: null, knowledgeQuery: null });
    expect(container.textContent).toContain('waiting for independent reviewer approval');
  });

  it('refuses task input when configuration cannot execute and says why', async () => {
    vi.spyOn(nexusApi, 'getAgentExecutionProfiles').mockResolvedValue({ hasData: false, items: [], latestVersionId: null });
    vi.spyOn(nexusApi, 'getIntegrationStatus').mockResolvedValue({ ...integrationData, enabledLocalModels: [] });
    const container = renderPage();
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Live governed agent'));
    });
    await clickButton(container, 'Run');
    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="dialog"]')?.textContent).toContain('Running is unavailable'));
    });
    const submit = Array.from(container.querySelectorAll<HTMLButtonElement>('[role="dialog"] button'))
      .find((button) => button.textContent?.includes('Request governed execution'));
    expect(submit?.disabled).toBe(true);
  });

  it('lists the skills registry with assignments and keeps management behind permissions', async () => {
    vi.spyOn(nexusApi, 'getSessionContext').mockResolvedValue({
      actorId: 'user_demo_reviewer', displayName: 'Reviewer', responsibility: 'REVIEWER', permissions: ['agent:read', 'skill:read'],
    });
    const container = renderPage({ permissions: ['agent:read', 'skill:read'] });
    await clickButton(container, 'Skills');
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Bounded summarizer'));
    });
    expect(container.textContent).toContain('v1');
    expect(container.textContent).toContain('agent-live');
    // قارئ بلا skill:create لا يرى زر الإنشاء إطلاقاً (بعد وصول سياق الجلسة).
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).not.toContain('New skill'));
    });
  });

  it('distinguishes executable tools from registered-only ones truthfully', async () => {
    const container = renderPage();
    await clickButton(container, 'Tools');
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Obligation Bounded List'));
    });
    expect(container.textContent).toContain('EXECUTABLE (trusted adapter)');
    expect(container.textContent).toContain('REGISTERED_ONLY');
  });

  it('starts an approved queued run and surfaces tool invocation status in the result', async () => {
    const runsData: AgentRunsData = {
      hasData: true,
      items: [{ id: 'agent_run_9', tenantId: 'tenant-a', agentId: 'agent-live', requestedByUserId: 'op', status: 'QUEUED', requestSummary: 'GOVERNED_EXECUTION_REQUEST', revision: 2, startedAt: null, completedAt: null, createdAt: '2026-10-04T00:00:00Z', updatedAt: '2026-10-04T00:00:00Z' }],
    };
    const runsQueryMock = vi.spyOn(nexusApi, 'getAgentRuns');
    runsQueryMock.mockResolvedValueOnce(runsData);
    runsQueryMock.mockResolvedValue({
      hasData: true,
      items: [{ ...runsData.items[0], status: 'COMPLETED', revision: 4, completedAt: '2026-10-04T01:00:00Z' }],
    });
    const startSpy = vi.spyOn(nexusApi, 'startQueuedExecution').mockResolvedValue({
      run: { ...runsData.items[0], status: 'COMPLETED', revision: 4 },
      result: {
        runId: 'agent_run_9', runStatus: 'COMPLETED', outcome: 'COMPLETED', providerId: 'OLLAMA_LOCAL',
        modelId: 'qwen3:4b', failureCode: null, output: 'Governed answer.', providerGeneratedAt: '2026-10-04T01:00:00Z', createdAt: '2026-10-04T01:00:00Z',
        citationsJson: null, truthClass: 'AI_INTERPRETATION',
      } satisfies AgentRunResultData,
    } satisfies StartExecutionOutcome);
    const invocations: ToolInvocationsData = {
      hasData: true,
      items: [{ id: 'tool_invocation_1', tenantId: 'tenant-a', agentRunId: 'agent_run_9', agentRunRevision: 3, toolId: 'tool-1', toolVersionId: 'tool-version-1', adapterKey: 'nexus.obligations.bounded_list.v1', inputJson: '{"limit":5}', inputHash: 'abc', policyDecision: 'ALLOW', policyReason: 'SAFE_READ_ONLY_INTERNAL_TOOL', status: 'COMPLETED', revision: 2, failureCode: null, createdAt: '', startedAt: '', finishedAt: '', updatedAt: '' }],
    };
    vi.spyOn(nexusApi, 'getToolInvocationsForRun').mockResolvedValue(invocations);
    vi.spyOn(nexusApi, 'getAgentRunResult').mockResolvedValue({
      runId: 'agent_run_9', runStatus: 'COMPLETED', outcome: 'COMPLETED', providerId: 'OLLAMA_LOCAL',
      modelId: 'qwen3:4b', failureCode: null, output: 'Governed answer.', providerGeneratedAt: '2026-10-04T01:00:00Z', createdAt: '2026-10-04T01:00:00Z',
      citationsJson: null, truthClass: 'AI_INTERPRETATION',
    });
    const container = renderPage();
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Start execution'));
    });
    await clickButton(container, 'Start execution');
    await act(async () => {
      await vi.waitFor(() => expect(startSpy.mock.calls[0]?.[0]).toBe('agent_run_9'));
    });
    // عرض النتيجة: منشأ كامل + حالة استدعاء الأداة.
    await clickButton(container, 'View result');
    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="dialog"]')?.textContent).toContain('AI-GENERATED OUTPUT'));
    });
    expect(container.querySelector('[role="dialog"]')?.textContent).toContain('Tool invocations in this run');
    expect(container.querySelector('[role="dialog"]')?.textContent).toContain('Policy allowed');
  });

  it('renders the new sections in Arabic RTL with translated labels', async () => {
    const container = renderPage();
    await act(async () => {
      await i18n.changeLanguage('ar');
    });
    expect(document.documentElement.dir).toBe('rtl');
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('الوكلاء'));
    });
    expect(container.textContent).toContain('المهارات');
    expect(container.textContent).toContain('الأدوات');
    expect(container.textContent).toContain('سجلات طلبات التشغيل');
  });
});
