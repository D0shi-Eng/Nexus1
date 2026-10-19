import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { RegulatorySourcesData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { RegulatoryIntelligencePage } from './RegulatoryIntelligencePage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const sourcesData: RegulatorySourcesData = {
  items: [
    {
      id: 'source_demo_pdpl',
      authority: 'DEMO / SYNTHETIC — Data Protection Authority',
      documentTitle: 'Synthetic PDPL readiness reference',
      documentVersion: 'DEMO-1.0',
      sourceUrl: 'https://example.invalid/demo/pdpl',
      retrievedAt: '2026-08-01T00:00:00Z',
      official: false,
      verificationState: 'REQUIRES_REVIEW',
      activeObligationCount: 2,
    },
  ],
  summary: { total: 1, official: 0, humanVerified: 0, requiresReview: 1, linkedActiveObligations: 2 },
  provenance: 'DEMO_SYNTHETIC',
  dataMode: 'DEMO_SYNTHETIC',
  generatedAt: '2026-10-02T00:00:00Z',
  hasData: true,
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderPage = (data?: RegulatorySourcesData): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (data) {
    queryClient.setQueryData(['regulatory-sources'], data);
  }
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(<QueryClientProvider client={queryClient}><RegulatoryIntelligencePage /></QueryClientProvider>);
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

describe('RegulatoryIntelligencePage', () => {
  it('renders the registered sources with truthful demo provenance and obligation counts', () => {
    const container = renderPage(sourcesData);

    expect(container.textContent).toContain('DEMO / SYNTHETIC — Data Protection Authority');
    expect(container.textContent).toContain('Synthetic PDPL readiness reference');
    // وسم المنشأ التركيبي ظاهر صراحة — لا يعرض شيء كأنه سجل تنظيمي رسمي.
    expect(container.textContent).toContain('DEMO / SYNTHETIC DATA');
    expect(container.textContent).toContain('All records in this workspace are demo / synthetic data');
    expect(container.textContent).toContain('Read only');
    expect(container.textContent).toContain('2');
    // لا ادعاء تغذية حية أو مراقبة تنظيمية.
    expect(container.textContent).toContain('No live feeds or external monitoring exist in this build');
  });

  it('renders an explicit empty state without inventing sources', () => {
    const container = renderPage({
      ...sourcesData,
      items: [],
      summary: { total: 0, official: 0, humanVerified: 0, requiresReview: 0, linkedActiveObligations: 0 },
      hasData: false,
    });

    expect(container.textContent).toContain('No regulatory sources are registered in this workspace');
    expect(container.querySelector('table')).toBeNull();
  });

  it('renders an authorization-denied safe error', async () => {
    vi.spyOn(nexusApi, 'getRegulatorySources').mockRejectedValue(
      new NexusApiError('AUTHORIZATION_DENIED', 'denied'),
    );
    const container = renderPage();
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('You are not authorized'));
    });

    expect(container.textContent).toContain('You are not authorized to read regulatory intelligence');
  });

  it('renders a generic safe error without leaking internals', async () => {
    vi.spyOn(nexusApi, 'getRegulatorySources').mockRejectedValue(
      new NexusApiError('STORAGE_ERROR', 'internal'),
    );
    const container = renderPage();
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('The local storage'));
    });

    expect(container.textContent).toContain('The local storage operation could not be completed safely');
  });

  it('never claims verified provenance for a user-provided unverified source', () => {
    // مصدر يقدّمه المستخدم داخل مستأجر إنتاجي: الشارة يجب ألا تدّعي تحققاً.
    const container = renderPage({
      ...sourcesData,
      provenance: 'USER_PROVIDED_UNVERIFIED',
      dataMode: 'PRODUCTION',
    });

    expect(container.querySelector('[aria-label="Verified provenance"]')).toBeNull();
    const badge = container.querySelector('svg[aria-label]');
    expect(badge?.getAttribute('aria-label')).toContain('their content was not verified');
    expect(container.textContent).toContain('They are not official sources');
  });
});
