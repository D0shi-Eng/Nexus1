import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { KnowledgeSearchResultData, KnowledgeSourcesData, MemoryData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { KnowledgePage } from './KnowledgePage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const sourcesData: KnowledgeSourcesData = {
  items: [
    {
      id: 'source-1',
      sourceType: 'USER_PROVIDED_DOCUMENT',
      title: 'Compliance Baseline',
      origin: 'workspace upload',
      provenance: 'USER_PROVIDED',
      status: 'ACTIVE',
      chunkCount: 4,
      documentCount: 1,
      createdAt: '2026-10-05T00:00:00Z',
      ingestedAt: '2026-10-05T00:00:00Z',
      updatedAt: '2026-10-05T00:00:00Z',
    },
  ],
  generatedAt: '2026-10-05T00:00:00Z',
};

const searchData: KnowledgeSearchResultData = {
  items: [
    {
      sourceId: 'source-1',
      documentId: 'doc-1',
      chunkId: 'chunk-1',
      sourceTitle: 'Compliance Baseline',
      provenance: 'USER_PROVIDED',
      rank: 0,
      score: -1.2,
      content: 'The baseline requires quarterly evidence review.',
    },
  ],
  generatedAt: '2026-10-05T00:00:00Z',
};

const memoryData: MemoryData = {
  summary: [{ memoryType: 'EPISODIC', status: 'ACTIVE', count: 2 }],
  items: [
    {
      id: 'memory-1',
      tenantId: 'tenant-a',
      memoryType: 'EPISODIC',
      scopeResourceType: 'AGENT_RUN',
      scopeResourceId: 'run-1',
      agentId: null,
      content: 'Agent run completed: run-1',
      truthClass: 'AI_INTERPRETATION',
      provenanceKind: 'AGENT_RUN',
      provenanceRef: 'run-1',
      status: 'ACTIVE',
      createdBy: 'user-a',
      expiresAt: '2026-11-05T00:00:00Z',
      createdAt: '2026-10-05T00:00:00Z',
      updatedAt: '2026-10-05T00:00:00Z',
    },
  ],
  generatedAt: '2026-10-05T00:00:00Z',
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderPage = (queryClient: QueryClient): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(<QueryClientProvider client={queryClient}><KnowledgePage /></QueryClientProvider>);
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

describe('KnowledgePage', () => {
  it('renders ingested sources with truthful provenance and chunk counts', () => {
    vi.spyOn(nexusApi, 'getKnowledgeSources').mockResolvedValue(sourcesData);
    vi.spyOn(nexusApi, 'getMemory').mockResolvedValue(memoryData);
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    queryClient.setQueryData(['knowledge-sources'], sourcesData);
    queryClient.setQueryData(['memory'], memoryData);
    const container = renderPage(queryClient);

    expect(container.textContent).toContain('Compliance Baseline');
    expect(container.textContent).toContain('User provided');
    expect(container.textContent).toContain('4');
    expect(container.textContent).not.toContain('AI Knowledge Score');
  });

  it('shows an explicit empty state and no fake retrieval before a search', () => {
    const empty: KnowledgeSourcesData = { items: [], generatedAt: '2026-10-05T00:00:00Z' };
    vi.spyOn(nexusApi, 'getKnowledgeSources').mockResolvedValue(empty);
    vi.spyOn(nexusApi, 'getMemory').mockResolvedValue({ summary: [], items: [], generatedAt: '2026-10-05T00:00:00Z' });
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    queryClient.setQueryData(['knowledge-sources'], empty);
    const container = renderPage(queryClient);

    expect(container.textContent).toContain('No knowledge sources are ingested');
    expect(container.querySelector('.knowledge-results')).toBeNull();
  });

  it('runs a real retrieval preview and renders bounded results with citation ranks', async () => {
    vi.spyOn(nexusApi, 'getKnowledgeSources').mockResolvedValue(sourcesData);
    vi.spyOn(nexusApi, 'getMemory').mockResolvedValue(memoryData);
    const searchSpy = vi.spyOn(nexusApi, 'searchKnowledge').mockResolvedValue(searchData);
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    queryClient.setQueryData(['knowledge-sources'], sourcesData);
    const container = renderPage(queryClient);

    const searchInput = container.querySelector<HTMLInputElement>('input[type="search"]');
    expect(searchInput).not.toBeNull();
    if (searchInput) {
      await setInputValue(searchInput, 'quarterly evidence');
    }
    const searchButton = Array.from(container.querySelectorAll('button')).find((button) =>
      button.textContent?.includes('Search'),
    );
    expect(searchButton).toBeDefined();
    await act(async () => {
      searchButton?.click();
    });

    expect(searchSpy).toHaveBeenCalledWith({ query: 'quarterly evidence' }, expect.anything());
    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('.knowledge-results')).not.toBeNull());
    });
    expect(container.textContent).toContain('The baseline requires quarterly evidence review.');
    expect(container.textContent).toContain('CIT-1');
  });

  it('surfaces a safe authorization error without internal details', async () => {
    vi.spyOn(nexusApi, 'getKnowledgeSources').mockRejectedValue(new NexusApiError('AUTHORIZATION_DENIED', 'safe'));
    vi.spyOn(nexusApi, 'getMemory').mockRejectedValue(new NexusApiError('AUTHORIZATION_DENIED', 'safe'));
    const container = renderPage(new QueryClient({ defaultOptions: { queries: { retry: false } } }));

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('not authorized');
  });

  it('exposes the memory management summary with truth classes and switches to Arabic RTL', async () => {
    vi.spyOn(nexusApi, 'getKnowledgeSources').mockResolvedValue(sourcesData);
    vi.spyOn(nexusApi, 'getMemory').mockResolvedValue(memoryData);
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    queryClient.setQueryData(['knowledge-sources'], sourcesData);
    queryClient.setQueryData(['memory'], memoryData);
    const container = renderPage(queryClient);

    expect(container.textContent).toContain('Memory');
    // الواجهة تعرض التسمية المترجمة لا الرمز التقني الخام — نستخرج التسمية من مورد الترجمة نفسه.
    expect(container.textContent).toContain(i18n.t('truthClass.AI_INTERPRETATION'));
    expect(container.textContent).not.toContain('AI_INTERPRETATION');

    await act(async () => {
      await i18n.changeLanguage('ar');
    });
    expect(document.documentElement.dir).toBe('rtl');
    expect(container.textContent).toContain('قاعدة المعرفة');
  });
});
