import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { EvidenceData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { SessionContext } from '../../lib/use-session';
import { EvidencePage } from './EvidencePage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const evidenceData: EvidenceData = {
  items: [
    { id: 'evidence-review', obligationId: 'obligation-a', obligationTitle: 'Synthetic access review obligation', label: 'Privileged access review record', classification: 'INTERNAL', status: 'REQUIRES_REVIEW', effectiveStatus: 'REQUIRES_REVIEW', isExpired: false, registeredBy: 'user-a', registeredByState: 'RECORDED', expiresAt: null, createdAt: '2026-08-18T00:00:00Z', updatedAt: '2026-08-20T00:00:00Z' },
    { id: 'evidence-expired', obligationId: 'obligation-b', obligationTitle: 'Synthetic retention obligation', label: 'Expired retention register', classification: 'CONFIDENTIAL', status: 'EXPIRED', effectiveStatus: 'EXPIRED', isExpired: true, registeredBy: null, registeredByState: 'UNKNOWN_LEGACY', expiresAt: '2026-08-01T00:00:00Z', createdAt: '2026-08-17T00:00:00Z', updatedAt: '2026-08-19T00:00:00Z' },
    { id: 'evidence-valid', obligationId: 'obligation-c', obligationTitle: 'Synthetic inventory obligation', label: 'Validated inventory register', classification: 'PUBLIC', status: 'VALID', effectiveStatus: 'VALID', isExpired: false, registeredBy: 'user-b', registeredByState: 'RECORDED', expiresAt: null, createdAt: '2026-08-16T00:00:00Z', updatedAt: '2026-08-18T00:00:00Z' },
  ],
  summary: { total: 3, requiresReview: 1, expired: 1 },
  dataMode: 'DEMO_SYNTHETIC',
  generatedAt: '2026-08-20T00:00:00Z',
  hasData: true,
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderPage = (initialData?: EvidenceData, onRegisterEvidence?: (obligationId?: string) => void): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (initialData) {
    queryClient.setQueryData(['evidence'], initialData);
  }

  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <EvidencePage onRegisterEvidence={onRegisterEvidence} />
      </QueryClientProvider>,
    );
  });

  return container;
};

// سياق جلسة مقيَّد: نتحقق أن الواجهة تُخفي مسار الكتابة عمّن لا يملك صلاحيته.
const renderPageWithoutCreatePermission = (initialData: EvidenceData): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  queryClient.setQueryData(['evidence'], initialData);
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <SessionContext.Provider value={{ actorId: 'reviewer', displayName: 'Reviewer', responsibility: 'REVIEWER', permissions: ['evidence:read'], can: (permission) => permission === 'evidence:read' }}>
          <EvidencePage onRegisterEvidence={() => undefined} />
        </SessionContext.Provider>
      </QueryClientProvider>,
    );
  });
  return container;
};

// جلسة تملك القراءة والإنشاء لكن لا تملك نقل الحالة — لاختبار إخفاء تحكم الدورة.
const renderPageWithoutStatusPermission = (initialData: EvidenceData): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  queryClient.setQueryData(['evidence'], initialData);
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <SessionContext.Provider value={{ actorId: 'requester', displayName: 'Requester', responsibility: 'REQUESTER', permissions: ['evidence:read', 'evidence:create'], can: (permission) => permission !== 'evidence:update' }}>
          <EvidencePage onRegisterEvidence={() => undefined} />
        </SessionContext.Provider>
      </QueryClientProvider>,
    );
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

describe('EvidencePage', () => {
  it('renders the loading state while the local query is pending', () => {
    vi.spyOn(nexusApi, 'getEvidence').mockImplementation(() => new Promise(() => undefined));
    const container = renderPage();

    expect(container.textContent).toContain('Loading local evidence');
  });

  it('renders stored fields and supports typed search, filters, and no-match state', async () => {
    vi.spyOn(nexusApi, 'getEvidence').mockResolvedValue(evidenceData);
    const container = renderPage(evidenceData);

    expect(container.textContent).toContain('Privileged access review record');
    expect(container.textContent).toContain('Expired retention register');
    expect(container.textContent).toContain('Validated inventory register');
    expect(container.textContent).toContain('3');

    const searchInput = container.querySelector<HTMLInputElement>('input');
    expect(searchInput).not.toBeNull();
    if (searchInput) {
      await setInputValue(searchInput, 'expired');
    }
    expect(container.textContent).not.toContain('Privileged access review record');
    expect(container.textContent).toContain('Expired retention register');

    const selects = container.querySelectorAll<HTMLSelectElement>('.attention-toolbar select');
    expect(selects).toHaveLength(2);
    await setSelectValue(selects[0], 'VALID');
    expect(container.textContent).toContain('No evidence records match the current search or filters.');

    const clearButton = Array.from(container.querySelectorAll<HTMLButtonElement>('button'))
      .find((button) => button.textContent?.includes('Clear filters'));
    expect(clearButton).not.toBeNull();
    await act(async () => clearButton?.click());
    expect(container.textContent).toContain('Privileged access review record');

    await setSelectValue(selects[1], 'PUBLIC');
    expect(container.textContent).toContain('Validated inventory register');
    expect(container.textContent).not.toContain('Expired retention register');
  });

  it('renders an explicit empty state without manufacturing summary metrics', () => {
    vi.spyOn(nexusApi, 'getEvidence').mockResolvedValue({
      items: [],
      summary: { total: 0, requiresReview: 0, expired: 0 },
      dataMode: 'DEMO_SYNTHETIC',
      generatedAt: '2026-08-20T00:00:00Z',
      hasData: false,
    });
    const container = renderPage({
      items: [],
      summary: { total: 0, requiresReview: 0, expired: 0 },
      dataMode: 'DEMO_SYNTHETIC',
      generatedAt: '2026-08-20T00:00:00Z',
      hasData: false,
    });

    expect(container.textContent).toContain('There are no evidence records in the current workspace.');
    expect(container.querySelector('table')).toBeNull();
    expect(container.querySelector('.metric-grid')).toBeNull();
  });

  // عيب مؤكَّد أُصلح (CQ-04): مسار الكتابة المدقَّق كان موجوداً في الخلف لكنه غير
  // قابل للوصول من صفحة الأدلة نفسها، فبدت الوحدة معطلة. هذه الاختبارات تثبّت
  // أن المدخل ظاهر لمن يملك الصلاحية ومخفي عمّن لا يملكها.
  it('opens the real audited register flow from the page when the actor may create evidence', async () => {
    const emptyEvidence: EvidenceData = {
      items: [],
      summary: { total: 0, requiresReview: 0, expired: 0 },
      dataMode: 'DEMO_SYNTHETIC',
      generatedAt: '2026-08-20T00:00:00Z',
      hasData: false,
    };
    vi.spyOn(nexusApi, 'getEvidence').mockResolvedValue(emptyEvidence);
    const onRegisterEvidence = vi.fn();
    const container = renderPage(emptyEvidence, onRegisterEvidence);

    const buttons = [...container.querySelectorAll('button')].filter((button) => button.textContent?.includes('Register evidence reference'));
    expect(buttons.length).toBeGreaterThan(0);

    await act(async () => {
      buttons[0].click();
    });
    expect(onRegisterEvidence).toHaveBeenCalledTimes(1);
  });

  it('hides the register action from an actor without evidence:create', () => {
    vi.spyOn(nexusApi, 'getEvidence').mockResolvedValue(evidenceData);
    const container = renderPageWithoutCreatePermission(evidenceData);

    expect(container.textContent).not.toContain('Register evidence reference');
    expect(container.textContent).toContain('Evidence register');
  });

  it('does not render a register action when no write path is wired', () => {
    vi.spyOn(nexusApi, 'getEvidence').mockResolvedValue(evidenceData);
    const container = renderPage(evidenceData);

    expect(container.textContent).not.toContain('Register evidence reference');
  });

  // دورة حياة الأدلة: كانت الحالة تُكتب مرة واحدة عند التسجيل ولا مسار لتغييرها.
  it('moves evidence through its real lifecycle through the audited IPC', async () => {
    vi.spyOn(nexusApi, 'getEvidence').mockResolvedValue(evidenceData);
    const updateSpy = vi.spyOn(nexusApi, 'updateEvidenceStatus').mockResolvedValue({
      evidenceId: 'evidence-review',
      obligationId: 'obligation-a',
      status: 'VALID',
      version: 2,
      obligationEvidenceStatus: 'VALID',
      updatedAt: '2026-10-15T09:00:00Z',
    });
    const container = renderPage(evidenceData);

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelectorAll('.table-status-select').length).toBe(3));
    });

    const first = container.querySelector<HTMLSelectElement>('.table-status-select')!;
    const valueSetter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set;
    await act(async () => {
      valueSetter?.call(first, 'VALID');
      first.dispatchEvent(new Event('change', { bubbles: true }));
    });

    expect(updateSpy.mock.calls[0][0]).toEqual({ evidenceId: 'evidence-review', status: 'VALID' });
    // العميل يرسل معرّف الدليل والحالة فقط — لا هوية ولا مستأجر ولا تصنيف.
    expect(Object.keys(updateSpy.mock.calls[0][0]).sort()).toEqual(['evidenceId', 'status']);
  });

  it('hides the lifecycle control from a session without evidence:update', async () => {
    vi.spyOn(nexusApi, 'getEvidence').mockResolvedValue(evidenceData);
    const container = renderPageWithoutStatusPermission(evidenceData);

    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Privileged access review record'));
    });
    expect(container.querySelectorAll('.table-status-select').length).toBe(0);
    // الحالة تبقى معروضة كوسم للقراءة فقط.
    expect(container.textContent).toContain('Requires review');
  });

  it('renders an authorization-denied safe error', async () => {    vi.spyOn(nexusApi, 'getEvidence').mockRejectedValue(new NexusApiError('AUTHORIZATION_DENIED', 'safe'));
    const container = renderPage();

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('not authorized');
  });

  it('renders a generic safe error without exposing internal details', async () => {
    vi.spyOn(nexusApi, 'getEvidence').mockRejectedValue(new NexusApiError('STORAGE_ERROR', 'INTERNAL_SQLITE_DETAILS'));
    const container = renderPage();

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('storage operation');
    expect(container.textContent).not.toContain('INTERNAL_SQLITE_DETAILS');
  });

  it('uses semantic controls and switches between English LTR and Arabic RTL', async () => {    vi.spyOn(nexusApi, 'getEvidence').mockResolvedValue(evidenceData);
    const container = renderPage(evidenceData);

    expect(document.documentElement.dir).toBe('ltr');
    expect(container.querySelector('section[aria-live="polite"]')).not.toBeNull();
    expect(container.querySelector('input')).not.toBeNull();
    expect(container.querySelectorAll('.attention-toolbar select')).toHaveLength(2);
    expect(container.querySelector('button')).not.toBeNull();

    await act(async () => {
      await i18n.changeLanguage('ar');
    });
    expect(document.documentElement.dir).toBe('rtl');
    expect(container.textContent).toContain('سجل الأدلة');
  });
});
