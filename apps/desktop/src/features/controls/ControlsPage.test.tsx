import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ControlsData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { SessionContext } from '../../lib/use-session';
import { ControlsPage } from './ControlsPage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const controlsData: ControlsData = {
  items: [
    {
      id: 'control-access-review',
      title: 'Access review procedure',
      createdAt: '2026-08-18T00:00:00Z',
      updatedAt: '2026-08-20T00:00:00Z',
      status: 'ACTIVE',
      category: 'Access management',
      owner: 'Compliance',
      effectiveness: 'EFFECTIVE',
      lastAssessedAt: '2026-08-20T00:00:00Z',
      linkedObligationCount: 2,
      linkedRiskCount: 1,
      linkedEvidenceCount: 1,
    },
    {
      id: 'control-record-retention',
      title: 'Record retention procedure',
      createdAt: '2026-08-17T00:00:00Z',
      updatedAt: '2026-08-19T00:00:00Z',
      status: 'DISABLED',
      category: null,
      owner: null,
      effectiveness: 'NOT_ASSESSED',
      lastAssessedAt: null,
      linkedObligationCount: 0,
      linkedRiskCount: 0,
      linkedEvidenceCount: 0,
    },
  ],
  summary: { total: 2, active: 1, inactive: 0, disabled: 1 },
  dataMode: 'DEMO_SYNTHETIC',
  generatedAt: '2026-08-20T00:00:00Z',
  hasData: true,
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderWith = (initialData: ControlsData | undefined, permissions: readonly string[] | null): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (initialData) {
    queryClient.setQueryData(['controls'], initialData);
  }
  const page = <ControlsPage />;
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(
      <QueryClientProvider client={queryClient}>
        {permissions === null ? page : (
          <SessionContext.Provider
            value={{
              actorId: 'test',
              displayName: 'Test',
              responsibility: 'REQUESTER',
              permissions,
              can: (permission) => permissions.includes(permission),
            }}
          >
            {page}
          </SessionContext.Provider>
        )}
      </QueryClientProvider>,
    );
  });
  return container;
};

// الجلسة الافتراضية في هذه الملف تملك كل صلاحيات التأليف (كما يفعل سياق الجلسة الفارغ).
const renderPage = (initialData?: ControlsData): HTMLDivElement => renderWith(initialData, null);
// جلسة قراءة فقط: تثبّت أن السجل لا يعرض أي تحكم لا تملكه.
const renderReadOnly = (initialData: ControlsData): HTMLDivElement => renderWith(initialData, ['control:read']);

const setInputValue = async (input: HTMLInputElement, value: string): Promise<void> => {
  await act(async () => {
    const valueSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
    valueSetter?.call(input, value);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
};

const clickButton = async (container: HTMLDivElement, label: string, index = 0): Promise<void> => {
  const buttons = [...container.querySelectorAll('button')].filter((button) => button.textContent?.includes(label));
  expect(buttons.length, `button "${label}" must exist`).toBeGreaterThan(index);
  await act(async () => { buttons[index].click(); });
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

describe('ControlsPage', () => {
  it('renders the loading state while the local query is pending', () => {
    vi.spyOn(nexusApi, 'getControls').mockImplementation(() => new Promise(() => undefined));
    const container = renderPage();

    expect(container.textContent).toContain('Loading local controls');
  });

  it('renders stored control fields and supports local typed search with a no-results state', async () => {
    vi.spyOn(nexusApi, 'getControls').mockResolvedValue(controlsData);
    const container = renderPage(controlsData);

    expect(container.textContent).toContain('Access review procedure');
    expect(container.textContent).toContain('Record retention procedure');
    expect(container.textContent).toContain('Active');
    expect(container.textContent).toContain('Disabled');
    expect(container.textContent).toContain('Not assessed');
    expect(container.textContent).toContain('2 obligations · 1 risks · 1 evidence');
    // لا مؤشرات مخترعة غير مخزَّنة.
    expect(container.textContent).not.toContain('Compliance score');

    const searchInput = container.querySelector<HTMLInputElement>('input[type="search"]');
    expect(searchInput).not.toBeNull();
    if (searchInput) {
      await setInputValue(searchInput, 'RETENTION');
    }

    expect(container.textContent).not.toContain('Access review procedure');
    expect(container.textContent).toContain('Record retention procedure');

    if (searchInput) {
      await setInputValue(searchInput, 'no stored control');
    }
    expect(container.textContent).toContain('No controls match the current search.');
  });

  it('does not render authoring controls for a read-only session', () => {
    vi.spyOn(nexusApi, 'getControls').mockResolvedValue(controlsData);
    const container = renderReadOnly(controlsData);

    // السجل يبقى مقروءاً، لكن بلا أي تحكم كتابة أو تقييم.
    expect(container.textContent).toContain('Access review procedure');
    expect(container.textContent).not.toContain('New control');
    expect(container.textContent).not.toContain('Record assessment');
    expect(container.textContent).not.toContain('Edit');
    expect(container.querySelectorAll('select')).toHaveLength(0);
    expect(container.querySelectorAll('table thead th')).toHaveLength(5);
  });

  it('creates a control through the real audited IPC using only title, category, and owner', async () => {
    vi.spyOn(nexusApi, 'getControls').mockResolvedValue(controlsData);
    const createSpy = vi.spyOn(nexusApi, 'createControl').mockResolvedValue({
      controlId: 'control-new',
      status: 'ACTIVE',
      updatedAt: '2026-10-15T09:00:00Z',
    });
    const container = renderPage(controlsData);

    await clickButton(container, 'New control');
    const titleInput = container.querySelector<HTMLInputElement>('.task-dialog .task-form input');
    expect(titleInput).not.toBeNull();
    await setInputValue(titleInput!, 'Segregation of duties control');

    await clickButton(container, 'Save');

    expect(createSpy.mock.calls[0][0]).toEqual({
      title: 'Segregation of duties control',
      category: null,
      owner: null,
    });
    // العميل لا يرسل هوية ولا مستأجراً ولا حالة.
    expect(Object.keys(createSpy.mock.calls[0][0]).sort()).toEqual(['category', 'owner', 'title']);
  });

  it('records an effectiveness assessment with an explicit value', async () => {
    vi.spyOn(nexusApi, 'getControls').mockResolvedValue(controlsData);
    const assessSpy = vi.spyOn(nexusApi, 'recordControlAssessment').mockResolvedValue({
      controlId: 'control-access-review',
      effectiveness: 'PARTIALLY_EFFECTIVE',
      lastAssessedAt: '2026-10-15T09:00:00Z',
    });
    const container = renderPage(controlsData);

    await clickButton(container, 'Record assessment');
    const select = container.querySelector<HTMLSelectElement>('.task-dialog .task-form select');
    expect(select).not.toBeNull();
    const valueSetter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set;
    await act(async () => {
      valueSetter?.call(select, 'PARTIALLY_EFFECTIVE');
      select!.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await clickButton(container, 'Save');

    expect(assessSpy.mock.calls[0][0]).toMatchObject({
      controlId: 'control-access-review',
      effectiveness: 'PARTIALLY_EFFECTIVE',
    });
  });

  it('disables an active control and never offers to disable an already disabled one', async () => {
    vi.spyOn(nexusApi, 'getControls').mockResolvedValue(controlsData);
    const disableSpy = vi.spyOn(nexusApi, 'disableControl').mockResolvedValue({
      controlId: 'control-access-review',
      status: 'DISABLED',
      updatedAt: '2026-10-15T09:00:00Z',
    });
    const container = renderPage(controlsData);

    // صف واحد فقط نشط ⇒ زر تعطيل واحد فقط.
    const disableButtons = [...container.querySelectorAll('button')].filter((button) => button.textContent?.includes('Disable'));
    expect(disableButtons).toHaveLength(1);
    await act(async () => { disableButtons[0].click(); });

    expect(disableSpy.mock.calls[0][0]).toEqual({ controlId: 'control-access-review', status: 'DISABLED' });
  });

  it('renders an explicit empty state with a real create action and no manufactured metrics', async () => {
    const emptyData: ControlsData = {
      items: [],
      summary: { total: 0, active: 0, inactive: 0, disabled: 0 },
      dataMode: 'DEMO_SYNTHETIC',
      generatedAt: '2026-08-20T00:00:00Z',
      hasData: false,
    };
    vi.spyOn(nexusApi, 'getControls').mockResolvedValue(emptyData);
    const container = renderPage(emptyData);

    expect(container.textContent).toContain('There are no controls recorded in the current workspace.');
    expect(container.querySelector('table')).toBeNull();
    expect(container.querySelector('.metric-grid')).toBeNull();
    expect(container.textContent).toContain('New control');
  });

  it('links a control to an obligation and unlinks it through the real audited IPC', async () => {
    vi.spyOn(nexusApi, 'getControls').mockResolvedValue(controlsData);
    vi.spyOn(nexusApi, 'getControlDetail').mockResolvedValue({
      ...controlsData.items[0],
      assessmentNote: null,
      linkedObligations: [{ obligationId: 'obligation-a', title: 'Access review obligation', severity: 'HIGH', status: 'OPEN', createdAt: '2026-08-01T00:00:00Z' }],
      linkedRisks: [],
      linkedEvidence: [],
    });
    vi.spyOn(nexusApi, 'getObligations').mockResolvedValue({
      items: [
        { id: 'obligation-a', tenantId: 't', title: 'Access review obligation', authority: 'SDAIA', owner: 'Compliance', deadline: null, severity: 'HIGH', status: 'OPEN', applicability: 'APPLICABLE' },
        { id: 'obligation-b', tenantId: 't', title: 'Retention obligation', authority: 'SDAIA', owner: 'Compliance', deadline: null, severity: 'MEDIUM', status: 'OPEN', applicability: 'APPLICABLE' },
      ],
      summary: { total: 2 },
      dataMode: 'DEMO_SYNTHETIC',
      generatedAt: '2026-10-15T09:00:00Z',
      hasData: true,
    } as never);
    vi.spyOn(nexusApi, 'getRisks').mockResolvedValue({ items: [], summary: { total: 0 }, dataMode: 'DEMO_SYNTHETIC', generatedAt: '2026-10-15T09:00:00Z', hasData: false } as never);
    vi.spyOn(nexusApi, 'getEvidence').mockResolvedValue({ items: [], summary: { total: 0, requiresReview: 0, expired: 0 }, dataMode: 'DEMO_SYNTHETIC', generatedAt: '2026-10-15T09:00:00Z', hasData: false } as never);
    const linkSpy = vi.spyOn(nexusApi, 'linkControlObligation').mockResolvedValue({
      controlId: 'control-access-review',
      linkKind: 'obligation_id',
      targetId: 'obligation-b',
      linkCount: 2,
      changed: true,
    });
    const unlinkSpy = vi.spyOn(nexusApi, 'unlinkControlObligation').mockResolvedValue({
      controlId: 'control-access-review',
      linkKind: 'obligation_id',
      targetId: 'obligation-a',
      linkCount: 0,
      changed: true,
    });
    const container = renderPage(controlsData);

    await clickButton(container, 'Links');
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Linked obligations'));
    });
    // المرتبط حالياً يظهر، والمرشّح المرتبط لا يُعرض كخيار إضافة مكرَّر.
    expect(container.textContent).toContain('Access review obligation');

    const select = container.querySelector<HTMLSelectElement>('.link-section__add select');
    expect(select).not.toBeNull();
    expect([...select!.options].map((option) => option.value)).toEqual(['', 'obligation-b']);

    const valueSetter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set;
    await act(async () => {
      valueSetter?.call(select, 'obligation-b');
      select!.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await clickButton(container, 'Add link');
    expect(linkSpy.mock.calls[0][0]).toEqual({ controlId: 'control-access-review', obligationId: 'obligation-b' });

    await clickButton(container, 'Unlink');
    expect(unlinkSpy.mock.calls[0][0]).toEqual({ controlId: 'control-access-review', obligationId: 'obligation-a' });
  });

  it('hides the links action from a session without control:link', async () => {
    vi.spyOn(nexusApi, 'getControls').mockResolvedValue(controlsData);
    const container = renderWith(controlsData, ['control:read', 'control:update', 'control:assess']);
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Access review procedure'));
    });
    expect(container.textContent).not.toContain('Links');
    // وبقية الإجراءات المصرَّح بها تبقى ظاهرة.
    expect(container.textContent).toContain('Record assessment');
  });

  it('renders an authorization-denied safe error', async () => {
    vi.spyOn(nexusApi, 'getControls').mockRejectedValue(new NexusApiError('AUTHORIZATION_DENIED', 'safe'));
    const container = renderPage();

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('not authorized');
  });

  it('renders a generic safe error without leaking internal details', async () => {
    vi.spyOn(nexusApi, 'getControls').mockRejectedValue(new NexusApiError('STORAGE_ERROR', 'INTERNAL_SQLITE_DETAILS'));
    const container = renderPage();

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('storage operation');
    expect(container.textContent).not.toContain('INTERNAL_SQLITE_DETAILS');
  });

  it('uses semantic controls and switches between English LTR and Arabic RTL', async () => {
    vi.spyOn(nexusApi, 'getControls').mockResolvedValue(controlsData);
    const container = renderPage(controlsData);

    expect(document.documentElement.dir).toBe('ltr');
    expect(container.querySelector('section[aria-live="polite"]')).not.toBeNull();
    expect(container.querySelector('header')).not.toBeNull();
    expect(container.querySelector('label')).not.toBeNull();
    expect(container.querySelector('input[type="search"]')).not.toBeNull();
    expect(container.querySelector('button')).not.toBeNull();
    expect(container.querySelector('table thead')).not.toBeNull();
    expect(container.querySelector('table tbody')).not.toBeNull();

    await act(async () => {
      await i18n.changeLanguage('ar');
    });
    expect(document.documentElement.dir).toBe('rtl');
    expect(container.textContent).toContain('سجل الضوابط');
    expect(container.textContent).toContain('ضابط جديد');
  });
});
