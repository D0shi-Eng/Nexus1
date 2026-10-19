import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReportCatalogData, ReportData } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { SessionContext, type SessionState } from '../../lib/use-session';
import { ReportsPage } from './ReportsPage';

// منفذ اختيار مسار الحفظ يُحقن صراحةً في المكوّن، فلا يُفتح أي حوار نظام أثناء
// الاختبار ولا نحتاج محاكاة وحدات (وهي هشّة مقابل ترتيب تحميل الملفات).
const chooseSavePathMock = vi.fn();

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const catalog: ReportCatalogData = {
  reports: [
    { reportId: 'OBLIGATION_STATUS', title: 'Obligation status', description: 'Backend description' },
    { reportId: 'RISK_REGISTER', title: 'Risk register', description: 'Backend description' },
  ],
  maxRowsPerSection: 500,
  generatedAt: '2026-10-15T09:00:00Z',
};

const report: ReportData = {
  reportId: 'OBLIGATION_STATUS',
  title: 'Obligation status',
  sections: [
    {
      title: 'Obligations by status',
      columns: [{ key: 'title', label: 'Obligation' }, { key: 'status', label: 'Status' }],
      rows: [['Access review obligation', 'OPEN'], ['Retention obligation', 'OPEN']],
      totalRows: 2,
      truncated: false,
    },
    {
      title: 'Evidence coverage',
      columns: [{ key: 'title', label: 'Obligation' }, { key: 'evidence', label: 'Evidence' }],
      rows: [['Access review obligation', 'MISSING']],
      totalRows: 830,
      truncated: true,
    },
  ],
  provenance: {
    dataMode: 'DEMO_SYNTHETIC',
    sourceTables: ['obligations', 'evidence_items'],
    generatedAt: '2026-10-15T09:30:00Z',
    disclaimer: 'Backend disclaimer text',
  },
};

const render = (permissions: readonly string[]): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const session: SessionState = {
    actorId: 'test',
    displayName: 'Test',
    responsibility: 'ADMINISTRATOR',
    permissions,
    can: (permission) => permissions.includes(permission),
  };
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <SessionContext.Provider value={session}>
          <ReportsPage chooseSavePath={chooseSavePathMock} />
        </SessionContext.Provider>
      </QueryClientProvider>,
    );
  });
  return container;
};

// مهلة أوسع من الافتراضية: التشغيل الكامل يشغّل ملفات كثيرة بالتوازي، ومهلة
// ثانية واحدة لا تكفي دائماً لسلسلة (حوار الحفظ ← أمر التصدير ← إظهار النتيجة).
const waitFor = (assertion: () => void): Promise<void> => vi.waitFor(assertion, { timeout: 5_000 });

beforeEach(async () => {
  chooseSavePathMock.mockReset();
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

describe('ReportsPage', () => {
  it('shows a loading state while the catalogue is pending', () => {
    vi.spyOn(nexusApi, 'getReportCatalog').mockImplementation(() => new Promise(() => undefined));
    const container = render(['report:read']);
    expect(container.textContent).toContain('Loading the report catalogue');
  });

  it('builds a report from real backend data with its provenance and honest truncation', async () => {
    vi.spyOn(nexusApi, 'getReportCatalog').mockResolvedValue(catalog);
    vi.spyOn(nexusApi, 'generateReport').mockResolvedValue(report);
    const container = render(['report:read']);

    await act(async () => {
      await waitFor(() => expect(container.textContent).toContain('Obligation status'));
    });

    const item = [...container.querySelectorAll('button')].find((button) => button.textContent?.includes('Obligation status'));
    expect(item).toBeTruthy();
    await act(async () => { item!.click(); });

    await act(async () => {
      await waitFor(() => expect(container.textContent).toContain('Obligations by status'));
    });

    // المحتوى الحقيقي يظهر كما هو.
    expect(container.textContent).toContain('Access review obligation');
    // المنشأ معلن: الجداول المصدر ووضع البيانات كتسمية بشرية مترجمة.
    expect(container.textContent).toContain('obligations · evidence_items');
    expect(container.textContent).toContain('Demo / synthetic');
    // ولا يُعرض الرمز التقني الخام كنص مستخدم — قاعدة المشروع نفسها.
    expect(container.textContent).not.toContain('DEMO_SYNTHETIC');
    // الاقتطاع يُعلن بصدق: يُعرض 1 من 830 لا «1».
    expect(container.textContent).toContain('Showing 1 of 830 rows');
    expect(container.textContent).toContain('2 rows');
  });

  it('hides export controls from a session without report:export', async () => {
    vi.spyOn(nexusApi, 'getReportCatalog').mockResolvedValue(catalog);
    vi.spyOn(nexusApi, 'generateReport').mockResolvedValue(report);
    const container = render(['report:read']);

    await act(async () => {
      await waitFor(() => expect(container.textContent).toContain('Obligation status'));
    });
    const item = [...container.querySelectorAll('button')].find((button) => button.textContent?.includes('Obligation status'));
    await act(async () => { item!.click(); });
    await act(async () => {
      await waitFor(() => expect(container.textContent).toContain('Obligations by status'));
    });

    expect(container.textContent).not.toContain('Export CSV');
    expect(container.textContent).not.toContain('Export Markdown');
  });

  it('exports through the native save dialog and reports the real file result', async () => {
    vi.spyOn(nexusApi, 'getReportCatalog').mockResolvedValue(catalog);
    vi.spyOn(nexusApi, 'generateReport').mockResolvedValue(report);
    chooseSavePathMock.mockResolvedValue('C:\\Users\\test\\obligation_status.csv');
    const exportSpy = vi.spyOn(nexusApi, 'exportReport').mockResolvedValue({
      fileName: 'obligation_status.csv',
      bytes: 512,
      sha256: 'a'.repeat(64),
      format: 'CSV',
    });
    const container = render(['report:read', 'report:export']);

    await act(async () => {
      await waitFor(() => expect(container.textContent).toContain('Obligation status'));
    });
    const item = [...container.querySelectorAll('button')].find((button) => button.textContent?.includes('Obligation status'));
    await act(async () => { item!.click(); });
    // الانتظار على **محتوى التقرير** لا على نص الزر: الزر يظهر مع اللوحة لكنه
    // يبقى معطَّلاً حتى تصل البيانات، والنقر عليه قبل ذلك لا يفعل شيئاً.
    await act(async () => {
      await waitFor(() => expect(container.textContent).toContain('Obligations by status'));
    });

    const exportButton = [...container.querySelectorAll('button')].find((button) => button.textContent?.includes('Export CSV'));
    await act(async () => { exportButton!.click(); });

    await act(async () => {
      await waitFor(() => expect(container.textContent).toContain('Exported obligation_status.csv'));
    });
    expect(exportSpy.mock.calls[0][0]).toEqual({
      reportId: 'OBLIGATION_STATUS',
      format: 'CSV',
      targetPath: 'C:\\Users\\test\\obligation_status.csv',
    });
  });

  it('writes nothing when the user cancels the save dialog', async () => {
    vi.spyOn(nexusApi, 'getReportCatalog').mockResolvedValue(catalog);
    vi.spyOn(nexusApi, 'generateReport').mockResolvedValue(report);
    chooseSavePathMock.mockResolvedValue(null);
    const exportSpy = vi.spyOn(nexusApi, 'exportReport');
    const container = render(['report:read', 'report:export']);

    await act(async () => {
      await waitFor(() => expect(container.textContent).toContain('Obligation status'));
    });
    const item = [...container.querySelectorAll('button')].find((button) => button.textContent?.includes('Obligation status'));
    await act(async () => { item!.click(); });
    // الانتظار على **محتوى التقرير** لا على نص الزر: الزر يظهر مع اللوحة لكنه
    // يبقى معطَّلاً حتى تصل البيانات، والنقر عليه قبل ذلك لا يفعل شيئاً.
    await act(async () => {
      await waitFor(() => expect(container.textContent).toContain('Obligations by status'));
    });
    const exportButton = [...container.querySelectorAll('button')].find((button) => button.textContent?.includes('Export CSV'));
    await act(async () => { exportButton!.click(); });

    expect(exportSpy).not.toHaveBeenCalled();
  });

  it('renders an authorization-denied state without asking the backend', () => {
    const catalogSpy = vi.spyOn(nexusApi, 'getReportCatalog');
    const container = render([]);
    expect(container.querySelector('[role="alert"]')?.textContent).toContain('not authorized');
    expect(catalogSpy).not.toHaveBeenCalled();
  });

  it('renders a safe error without leaking internal details, and switches to Arabic RTL', async () => {
    vi.spyOn(nexusApi, 'getReportCatalog').mockRejectedValue(new NexusApiError('STORAGE_ERROR', 'INTERNAL_SQLITE_DETAILS'));
    const container = render(['report:read']);

    await act(async () => {
      await waitFor(() => expect(container.querySelector('[role="alert"]')).not.toBeNull());
    });
    expect(container.textContent).not.toContain('INTERNAL_SQLITE_DETAILS');

    await act(async () => {
      await i18n.changeLanguage('ar');
    });
    expect(document.documentElement.dir).toBe('rtl');
    expect(container.textContent).toContain('التقارير');
  });
});
