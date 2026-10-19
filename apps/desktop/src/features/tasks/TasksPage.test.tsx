import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { TaskRecord, TasksData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { TasksPage } from './TasksPage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const task = (overrides: Partial<TaskRecord> & Pick<TaskRecord, 'id' | 'status'>): TaskRecord => ({
  obligationId: 'obligation-a',
  obligationTitle: 'Synthetic obligation',
  title: 'Synthetic task',
  description: 'Synthetic task description.',
  dueAt: null,
  priority: 'MEDIUM',
  assigneeUserId: null,
  assigneeDisplayName: null,
  version: 1,
  completedAt: null,
  availableTransitions: [],
  canAssign: false,
  createdAt: '2026-08-16T00:00:00Z',
  updatedAt: '2026-08-18T00:00:00Z',
  ...overrides,
});

const tasksData: TasksData = {
  items: [
    task({ id: 'task-critical', title: 'Prepare access evidence', description: 'Prepare the documented access review evidence.', dueAt: '2026-08-20T00:00:00Z', priority: 'CRITICAL', status: 'OPEN', assigneeUserId: 'user-worker', assigneeDisplayName: 'NEXUS Demo Operator', availableTransitions: ['IN_PROGRESS', 'BLOCKED'], canAssign: true }),
    task({ id: 'task-progress', obligationId: 'obligation-b', title: 'Review retention record', description: 'Review the retention record before closure.', priority: 'HIGH', status: 'IN_PROGRESS', assigneeUserId: 'user-worker', assigneeDisplayName: 'NEXUS Demo Operator', availableTransitions: ['READY_FOR_REVIEW', 'BLOCKED'] }),
    task({ id: 'task-blocked', obligationId: 'obligation-c', title: 'Validate inventory register', description: 'Validation is waiting on a local dependency.', dueAt: '2026-08-21T00:00:00Z', status: 'BLOCKED' }),
  ],
  summary: { total: 3, assignedToMe: 2, open: 1, inProgress: 1, blocked: 1, readyForReview: 0, approved: 0, rejected: 0, completed: 0, overdue: 0 },
  dataMode: 'DEMO_SYNTHETIC',
  generatedAt: '2026-08-20T00:00:00Z',
  hasData: true,
};

const emptySummary = { total: 0, assignedToMe: 0, open: 0, inProgress: 0, blocked: 0, readyForReview: 0, approved: 0, rejected: 0, completed: 0, overdue: 0 };

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderPage = (initialData?: TasksData): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (initialData) {
    queryClient.setQueryData(['tasks'], initialData);
  }

  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(<QueryClientProvider client={queryClient}><TasksPage /></QueryClientProvider>);
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

const buttonByText = (container: HTMLElement, text: string): HTMLButtonElement | undefined =>
  Array.from(container.querySelectorAll<HTMLButtonElement>('button'))
    .find((button) => button.textContent?.trim() === text);

beforeEach(async () => {
  await i18n.changeLanguage('en');
  // لا نداء IPC حقيقي من الواجهة في الاختبارات.
  vi.spyOn(nexusApi, 'listWorkspaceUsers').mockResolvedValue({ users: [], availableRoles: [] });
});

afterEach(async () => {
  act(() => {
    roots.splice(0).forEach((root) => root.unmount());
  });
  containers.splice(0).forEach((container) => container.remove());
  vi.restoreAllMocks();
  await i18n.changeLanguage('ar');
});

describe('TasksPage', () => {
  it('renders the loading state while the local query is pending', () => {
    vi.spyOn(nexusApi, 'getTasks').mockImplementation(() => new Promise(() => undefined));
    const container = renderPage();

    expect(container.textContent).toContain('Loading local tasks');
  });

  it('renders stored fields and supports typed search, filters, and no-match state', async () => {
    vi.spyOn(nexusApi, 'getTasks').mockResolvedValue(tasksData);
    const container = renderPage(tasksData);

    expect(container.textContent).toContain('Prepare access evidence');
    expect(container.textContent).toContain('Review retention record');
    expect(container.textContent).toContain('Validate inventory register');
    expect(container.textContent).toContain('3');

    const searchInput = container.querySelector<HTMLInputElement>('input');
    expect(searchInput).not.toBeNull();
    if (searchInput) {
      await setInputValue(searchInput, 'retention');
    }
    expect(container.textContent).not.toContain('Prepare access evidence');
    expect(container.textContent).toContain('Review retention record');

    const selects = container.querySelectorAll<HTMLSelectElement>('select');
    expect(selects).toHaveLength(2);
    await setSelectValue(selects[0], 'BLOCKED');
    expect(container.textContent).toContain('No tasks match the current search or filters.');

    const clearButton = buttonByText(container, 'Clear filters');
    expect(clearButton).not.toBeUndefined();
    await act(async () => clearButton?.click());
    expect(container.textContent).toContain('Prepare access evidence');

    await setSelectValue(selects[1], 'MEDIUM');
    expect(container.textContent).toContain('Validate inventory register');
    expect(container.textContent).not.toContain('Review retention record');
  });

  it('renders an explicit empty state without manufacturing summary metrics', () => {
    vi.spyOn(nexusApi, 'getTasks').mockResolvedValue({
      items: [],
      summary: emptySummary,
      dataMode: 'DEMO_SYNTHETIC',
      generatedAt: '2026-08-20T00:00:00Z',
      hasData: false,
    });
    const container = renderPage({
      items: [],
      summary: emptySummary,
      dataMode: 'DEMO_SYNTHETIC',
      generatedAt: '2026-08-20T00:00:00Z',
      hasData: false,
    });

    expect(container.textContent).toContain('There are no tasks recorded in the current workspace.');
    // دليل البدء مطلوب في الحالة الفارغة بدل فراغ صامت.
    expect(container.textContent).toContain('Start in Obligations');
    expect(container.querySelector('table')).toBeNull();
    expect(container.querySelector('.metric-grid')).toBeNull();
  });

  // 0025: الأزرار تأتي من الحالات التي يعلنها الخلف، لا من تخمين في العميل.
  it('renders only the transitions the backend declares for the viewer', () => {
    vi.spyOn(nexusApi, 'getTasks').mockResolvedValue(tasksData);
    const container = renderPage(tasksData);

    expect(buttonByText(container, 'Start work')).not.toBeUndefined();
    expect(buttonByText(container, 'Block work')).not.toBeUndefined();
    expect(buttonByText(container, 'Submit for review')).not.toBeUndefined();
    // المهمة المتوقفة بلا إجراءات معلنة لا تُعرض لها أزرار انتقال إطلاقاً
    // (زر السجل ليس إجراءً على الحالة).
    const blockedRow = Array.from(container.querySelectorAll('tbody tr'))
      .find((row) => row.textContent?.includes('Validate inventory register'));
    const blockedLabels = Array.from(blockedRow?.querySelectorAll('button') ?? [])
      .map((button) => button.textContent?.trim());
    expect(blockedLabels).not.toContain('Start work');
    expect(blockedLabels).not.toContain('Block work');
    expect(blockedLabels).not.toContain('Submit for review');
    expect(blockedLabels).toContain('History');
    // القرارات لا تُتخذ من هذه الصفحة.
    expect(buttonByText(container, 'Approve')).toBeUndefined();
    expect(buttonByText(container, 'Reject')).toBeUndefined();
  });

  it('sends the exact version the viewer saw when applying a transition', async () => {
    vi.spyOn(nexusApi, 'getTasks').mockResolvedValue(tasksData);
    const transitionSpy = vi.spyOn(nexusApi, 'transitionTask').mockResolvedValue({
      taskId: 'task-critical',
      previousStatus: 'OPEN',
      status: 'IN_PROGRESS',
      version: 2,
      pendingApprovalId: null,
    });
    const container = renderPage(tasksData);

    await act(async () => buttonByText(container, 'Start work')?.click());
    const confirm = buttonByText(container, 'Apply action');
    expect(confirm).not.toBeUndefined();
    await act(async () => confirm?.click());

    expect(transitionSpy).toHaveBeenCalledWith({
      taskId: 'task-critical',
      targetStatus: 'IN_PROGRESS',
      reason: null,
      expectedVersion: 1,
    });
  });

  it('surfaces a safe error code when a transition is refused', async () => {
    vi.spyOn(nexusApi, 'getTasks').mockResolvedValue(tasksData);
    vi.spyOn(nexusApi, 'transitionTask').mockRejectedValue(new NexusApiError('TASK_EVIDENCE_REQUIRED', 'internal detail'));
    const container = renderPage(tasksData);

    await act(async () => buttonByText(container, 'Start work')?.click());
    await act(async () => buttonByText(container, 'Apply action')?.click());

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.textContent).not.toContain('internal detail');
  });

  it('offers self-assignment only for unassigned tasks the backend allows', async () => {
    const unassigned: TasksData = {
      ...tasksData,
      items: [task({ id: 'task-free', title: 'Free task', status: 'OPEN', canAssign: true })],
    };
    vi.spyOn(nexusApi, 'getTasks').mockResolvedValue(unassigned);
    const assignSpy = vi.spyOn(nexusApi, 'assignTask').mockResolvedValue({
      taskId: 'task-free',
      previousStatus: 'OPEN',
      status: 'OPEN',
      version: 2,
      pendingApprovalId: null,
    });
    const container = renderPage(unassigned);

    expect(container.textContent).toContain('Unassigned');
    await act(async () => buttonByText(container, 'Assign to me')?.click());
    expect(assignSpy).toHaveBeenCalledWith(expect.objectContaining({ taskId: 'task-free', expectedVersion: 1 }));
  });

  it('shows the real transition log instead of a synthetic history', async () => {
    vi.spyOn(nexusApi, 'getTasks').mockResolvedValue(tasksData);
    vi.spyOn(nexusApi, 'getTaskHistory').mockResolvedValue([
      { fromStatus: 'OPEN', toStatus: 'IN_PROGRESS', actorDisplayName: 'NEXUS Demo Operator', reason: 'Started', occurredAt: '2026-08-19T00:00:00Z' },
    ]);
    const container = renderPage(tasksData);

    await act(async () => buttonByText(container, 'History')?.click());
    // الانتظار على نص السجل نفسه: اسم الفاعل يظهر في الجدول أيضاً فلا يصلح دليلاً.
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Started'));
    });
    expect(container.textContent).toContain('Open → In progress');
  });

  it('renders an authorization-denied safe error', async () => {
    vi.spyOn(nexusApi, 'getTasks').mockRejectedValue(new NexusApiError('AUTHORIZATION_DENIED', 'safe'));
    const container = renderPage();

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('not authorized');
  });

  it('renders a generic safe error without exposing internal details', async () => {
    vi.spyOn(nexusApi, 'getTasks').mockRejectedValue(new NexusApiError('STORAGE_ERROR', 'INTERNAL_SQLITE_DETAILS'));
    const container = renderPage();

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('storage operation');
    expect(container.textContent).not.toContain('INTERNAL_SQLITE_DETAILS');
  });

  it('uses semantic controls and switches between English LTR and Arabic RTL', async () => {
    vi.spyOn(nexusApi, 'getTasks').mockResolvedValue(tasksData);
    const container = renderPage(tasksData);

    expect(document.documentElement.dir).toBe('ltr');
    expect(container.querySelector('section[aria-live="polite"]')).not.toBeNull();
    expect(container.querySelector('input')).not.toBeNull();
    expect(container.querySelectorAll('select')).toHaveLength(2);
    expect(container.querySelector('button')).not.toBeNull();
    expect(container.querySelector('table thead')).not.toBeNull();
    expect(container.querySelector('table tbody')).not.toBeNull();

    await act(async () => {
      await i18n.changeLanguage('ar');
    });
    expect(document.documentElement.dir).toBe('rtl');
    expect(container.textContent).toContain('سجل المهام');
  });
});
