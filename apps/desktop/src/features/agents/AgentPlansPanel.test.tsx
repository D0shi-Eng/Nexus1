import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PlanDetails, PlansData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { nexusApi } from '../../lib/nexus-api';
import { AgentPlansPanel } from './AgentPlansPanel';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const plansData: PlansData = {
  items: [
    {
      id: 'plan-1',
      tenantId: 'tenant-a',
      goal: 'Assess omega-77 compliance readiness',
      status: 'ACTIVE',
      stepsHash: 'hash',
      modelIdAtRequest: 'qwen3:4b',
      revision: 2,
      requestedByUserId: 'user-operator',
      synthesisOutput: null,
      conflictFlag: false,
      createdAt: '2026-10-05T00:00:00Z',
      updatedAt: '2026-10-05T00:00:00Z',
      completedAt: null,
    },
    {
      id: 'plan-2',
      tenantId: 'tenant-a',
      goal: 'Contradiction probe',
      status: 'COMPLETED',
      stepsHash: 'hash',
      modelIdAtRequest: 'qwen3:4b',
      revision: 5,
      requestedByUserId: 'user-operator',
      synthesisOutput: '[GOAL]\nContradiction probe\n\n[CONFLICTS]\nPreserved.',
      conflictFlag: true,
      createdAt: '2026-10-05T00:00:00Z',
      updatedAt: '2026-10-05T00:00:00Z',
      completedAt: '2026-10-05T01:00:00Z',
    },
  ],
};

const planDetails: PlanDetails = {
  plan: plansData.items[0],
  steps: [
    {
      id: 'step-1',
      tenantId: 'tenant-a',
      planId: 'plan-1',
      stepIndex: 0,
      objective: 'Research the omega-77 baseline',
      requiredCapability: 'RESEARCH',
      requiresKnowledge: true,
      evidenceRequirement: 'Cite retrieved sources',
      status: 'PENDING',
      createdAt: '2026-10-05T00:00:00Z',
      updatedAt: '2026-10-05T00:00:00Z',
    },
  ],
  delegations: [
    {
      delegation: {
        id: 'delegation-1',
        tenantId: 'tenant-a',
        planId: 'plan-1',
        planRevision: 2,
        stepId: 'step-1',
        targetAgentId: 'agent_demo_research',
        agentRunId: 'run-1',
        objective: 'Research the omega-77 baseline',
        allowedSkillVersionId: null,
        status: 'COMPLETED',
        revision: 2,
        output: 'The baseline requires quarterly review [CIT-1]',
        citationsJson: '[{"citationId":"CIT-1","sourceId":"source-1"}]',
        truthClass: 'AI_INTERPRETATION',
        failureCode: null,
        createdAt: '2026-10-05T00:00:00Z',
        updatedAt: '2026-10-05T00:00:00Z',
        finishedAt: '2026-10-05T00:30:00Z',
      },
      verification: {
        id: 'verification-1',
        tenantId: 'tenant-a',
        subjectKind: 'AGENT_RUN',
        agentRunId: 'run-1',
        planId: null,
        outcome: 'SUPPORTED',
        method: 'DETERMINISTIC_AND_AI',
        deterministicChecksJson: '{}',
        aiEvaluationJson: '{"outcome":"SUPPORTED"}',
        aiAnalysisRunId: 'ai-1',
        createdBy: 'user-operator',
        createdAt: '2026-10-05T00:31:00Z',
      },
    },
  ],
  pendingApprovalId: null,
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderPanel = (queryClient: QueryClient): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(<QueryClientProvider client={queryClient}><AgentPlansPanel /></QueryClientProvider>);
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

describe('AgentPlansPanel', () => {
  it('renders governed plans with truthful statuses and conflict flags', () => {
    vi.spyOn(nexusApi, 'getPlans').mockResolvedValue(plansData);
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    queryClient.setQueryData(['plans'], plansData);
    const container = renderPanel(queryClient);

    expect(container.textContent).toContain('Assess omega-77 compliance readiness');
    expect(container.textContent).toContain('Approved');
    expect(container.textContent).toContain('Conflicting verification outcomes are preserved.');
  });

  it('shows an explicit empty state when no plans exist', async () => {
    vi.spyOn(nexusApi, 'getPlans').mockResolvedValue({ items: [] });
    const container = renderPanel(new QueryClient({ defaultOptions: { queries: { retry: false } } }));
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('No plans exist in the current workspace yet.'));
    });
  });

  it('opens plan detail with steps, delegation outputs, citations, and verification outcomes', async () => {
    vi.spyOn(nexusApi, 'getPlans').mockResolvedValue(plansData);
    vi.spyOn(nexusApi, 'getPlan').mockResolvedValue(planDetails);
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    queryClient.setQueryData(['plans'], plansData);
    const container = renderPanel(queryClient);

    const detailButton = Array.from(container.querySelectorAll('button')).find((button) =>
      button.textContent?.includes('Plan detail'),
    );
    expect(detailButton).toBeDefined();
    await act(async () => {
      detailButton?.click();
    });
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Research the omega-77 baseline'));
    });
    expect(container.textContent).toContain('agent_demo_research');
    // تصنيف الحقيقة يُعرض مترجماً؛ نتحقق من التسمية المعروضة ومن غياب الرمز الخام معاً.
    expect(container.textContent).toContain(i18n.t('truthClass.AI_INTERPRETATION'));
    expect(container.textContent).not.toContain('AI_INTERPRETATION');
    // نتيجة التحقق تُعرض مترجمة أيضاً — لا نثبّت الرمز التقني في توقّع الاختبار.
    expect(container.textContent).toContain(i18n.t('plans.verificationOutcome.SUPPORTED'));
    expect(container.textContent).not.toContain('SUPPORTED');
    expect(container.textContent).toContain('[CIT-1]');
  });

  it('does not offer execution for a plan that is not approved yet', () => {
    const pending: PlansData = {
      items: [{ ...plansData.items[0], id: 'plan-3', status: 'PENDING_APPROVAL' }],
    };
    vi.spyOn(nexusApi, 'getPlans').mockResolvedValue(pending);
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    queryClient.setQueryData(['plans'], pending);
    const container = renderPanel(queryClient);

    expect(container.textContent).toContain('Waiting approval');
    const executeButton = Array.from(container.querySelectorAll('button')).find((button) =>
      button.textContent?.includes('Execute'),
    );
    expect(executeButton).toBeUndefined();
  });

  it('switches to Arabic RTL labels', async () => {
    vi.spyOn(nexusApi, 'getPlans').mockResolvedValue(plansData);
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    queryClient.setQueryData(['plans'], plansData);
    const container = renderPanel(queryClient);

    await act(async () => {
      await i18n.changeLanguage('ar');
    });
    expect(document.documentElement.dir).toBe('rtl');
    expect(container.textContent).toContain('خطة جديدة');
  });
});
