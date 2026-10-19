import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { DashboardData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { nexusApi } from '../../lib/nexus-api';
import { SessionProvider } from '../../lib/session-context';
import { OverviewPage } from './OverviewPage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const emptyHandlers = {
  onOpenAttention: vi.fn(),
  onOpenObligation: vi.fn(),
  onOpenTaskFor: vi.fn(),
  onOpenEvidenceFor: vi.fn(),
};

const dashboardData: DashboardData = {
  attentionItems: [
    { id: 'att-1', kind: 'OBLIGATION', severity: 'CRITICAL', title: 'At-risk obligation', status: 'AT_RISK', reasonCode: 'OBLIGATION_AT_RISK', source: 'LOCAL_WORKSPACE', dueAt: '2026-09-01T00:00:00Z', resourceId: 'obl_demo_001', relatedEntity: null, owner: 'Privacy Officer', updatedAt: '2026-08-20T00:00:00Z', availableActions: ['OPEN'] },
  ],
  obligations: [
    { id: 'obl_demo_001', tenantId: 'tenant_demo_sa_001', title: 'Review the synthetic data inventory evidence', authority: 'DEMO / SYNTHETIC — Data Protection Authority', owner: 'Privacy Officer', deadline: '2026-08-20T00:00:00Z', severity: 'CRITICAL', status: 'AT_RISK', applicability: 'APPLIES', evidenceStatus: 'MISSING', verificationState: 'REQUIRES_REVIEW', source: { authority: 'DEMO / SYNTHETIC — Data Protection Authority', documentTitle: 'Synthetic PDPL readiness reference', version: 'DEMO-1.0', section: undefined, retrievedAt: '2026-08-01T00:00:00Z', official: false, sourceUrl: undefined }, updatedAt: '2026-08-20T00:00:00Z' },
  ],
  readiness: { tenantId: 'tenant_demo_sa_001', readinessScore: 40, verifiedCoverage: 25, evidenceCompleteness: 62, openCriticalItems: 1, eligibleObligations: 4, archivedObligations: 0, sufficientData: true, generatedAt: '2026-10-02T00:00:00Z', disclaimer: 'DEMO / SYNTHETIC readiness estimate based on locally available evidence. Human verification is required.' },
  recentAuditEvents: [
    { id: 'audit_demo_001', eventType: 'DEMO_DATA_INITIALIZED', actorName: 'NEXUS System', resourceType: 'Tenant', resourceId: 'tenant_demo_sa_001', occurredAt: '2026-08-01T00:00:00Z', integrityStatus: 'VALID' },
  ],
  auditChainIntegrity: 'VALID',
  dataMode: 'DEMO_SYNTHETIC',
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderPage = (options?: { readonly data?: DashboardData; readonly permissions?: readonly string[] }): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (options?.data) {
    queryClient.setQueryData(['dashboard'], options.data);
  }
  if (options?.permissions) {
    vi.spyOn(nexusApi, 'getSessionContext').mockResolvedValue({
      actorId: 'user_demo_reviewer',
      displayName: 'NEXUS Demo Reviewer',
      responsibility: 'REVIEWER',
      permissions: options.permissions,
    });
  }
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  const page = <OverviewPage {...emptyHandlers} />;
  act(() => {
    root.render(
      options?.permissions
        ? <QueryClientProvider client={queryClient}><SessionProvider>{page}</SessionProvider></QueryClientProvider>
        : <QueryClientProvider client={queryClient}>{page}</QueryClientProvider>,
    );
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

describe('OverviewPage', () => {
  it('renders backend-derived metrics, attention, obligations, and audit activity', () => {
    const container = renderPage({ data: dashboardData });

    expect(container.textContent).toContain('40%');
    expect(container.textContent).toContain('Review the synthetic data inventory evidence');
    expect(container.textContent).toContain('At-risk obligation');
    // حدث التدقيق يُعرض بتسميته المترجمة لا برمزه التقني — الرمز الخام لا يجوز أن يظهر للمستخدم.
    expect(container.textContent).toContain(i18n.t('audit.event.DEMO_DATA_INITIALIZED'));
    expect(container.textContent).not.toContain('DEMO_DATA_INITIALIZED');
  });

  it('shows truthful empty states for obligations and audit when datasets are empty', () => {
    const container = renderPage({
      data: { ...dashboardData, obligations: [], recentAuditEvents: [] },
    });

    expect(container.textContent).toContain('There are no active obligations to show');
    expect(container.textContent).toContain('No audit activity has been recorded yet');
    expect(container.querySelector('table')).toBeNull();
  });

  it('hides workflow buttons from an actor without create permissions', async () => {
    const container = renderPage({
      data: dashboardData,
      permissions: ['obligation:read'],
    });
    await act(async () => {
      await vi.waitFor(() => {
        expect(container.textContent).not.toContain('Create task');
      });
    });

    expect(container.textContent).not.toContain('Register evidence reference');
  });

  it('keeps workflow buttons for an actor holding the create permissions', () => {
    const container = renderPage({
      data: dashboardData,
      permissions: ['obligation:read', 'task:create', 'evidence:create'],
    });

    expect(container.textContent).toContain('Create task');
    expect(container.textContent).toContain('Register evidence reference');
  });
});
