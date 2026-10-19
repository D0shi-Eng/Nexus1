import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { AuthStatusData, DashboardData, DemoActorsData, HealthStatus } from '@nexus/contracts';
import type { ReactNode } from 'react';
import '../../lib/i18n';
import { i18n } from '../../lib/i18n';
import { nexusApi } from '../../lib/nexus-api';
import { SessionProvider } from '../../lib/session-context';
import { SettingsPage } from '../settings/SettingsPage';
import { AuthGate } from '../../app/AuthGate';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const unprovisioned: AuthStatusData = {
  mode: 'UNPROVISIONED',
  authenticated: false,
  needsBootstrap: true,
  actorDisplayName: null,
  sessionExpiresAt: null,
};

const productionUnauthenticated: AuthStatusData = {
  mode: 'PRODUCTION',
  authenticated: false,
  needsBootstrap: false,
  actorDisplayName: null,
  sessionExpiresAt: null,
};

const demoAuthenticated: AuthStatusData = {
  mode: 'DEMO',
  authenticated: true,
  needsBootstrap: false,
  actorDisplayName: null,
  sessionExpiresAt: null,
};

const dashboardData: DashboardData = {
  attentionItems: [],
  obligations: [],
  readiness: {
    tenantId: 'tenant-demo',
    readinessScore: null,
    verifiedCoverage: null,
    evidenceCompleteness: null,
    openCriticalItems: 0,
    eligibleObligations: 0,
    archivedObligations: 0,
    sufficientData: false,
    generatedAt: '2026-10-10T00:00:00Z',
    disclaimer: 'synthetic',
  },
  recentAuditEvents: [],
  auditChainIntegrity: 'VALID',
  dataMode: 'DEMO_SYNTHETIC',
};

const demoActors: DemoActorsData = {
  actors: [
    { actorId: 'user_demo_operator', displayName: 'NEXUS Demo Operator', responsibility: 'REQUESTER' },
    { actorId: 'user_demo_reviewer', displayName: 'NEXUS Demo Reviewer', responsibility: 'REVIEWER' },
  ],
  activeActorId: 'user_demo_operator',
};

const health: HealthStatus = {
  application: 'HEALTHY',
  database: 'HEALTHY',
  ai: 'NOT_CONFIGURED',
  checkedAt: '2026-10-10T00:00:00Z',
};

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderWithClient = (node: ReactNode): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(<QueryClientProvider client={queryClient}>{node}</QueryClientProvider>);
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

describe('AuthGate identity routing', () => {
  it('renders the setup screen when the workspace is unprovisioned', async () => {
    vi.spyOn(nexusApi, 'getAuthStatus').mockResolvedValue(unprovisioned);
    // GAP-05-OPEN: شاشة التأسيس تجلب كتالوج الدول من الخلف قبل عرض الحقول.
    vi.spyOn(nexusApi, 'getBootstrapOptions').mockResolvedValue({ countryCodes: ['SA', 'AE'] });
    const container = renderWithClient(<AuthGate />);

    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Workspace setup'));
    });
    expect(container.textContent).toContain('Start the demo experience');
    expect(container.textContent).toContain('Provision production');
    // حقلا الدولة والقطاع جزء من شاشة التأسيس نفسها.
    expect(container.querySelector('#setup-country-code')).not.toBeNull();
    expect(container.querySelector('#setup-industry')).not.toBeNull();
    expect(container.querySelector('.app-shell')).toBeNull();
  });

  it('renders the login screen when production is not authenticated', async () => {
    vi.spyOn(nexusApi, 'getAuthStatus').mockResolvedValue(productionUnauthenticated);
    const container = renderWithClient(<AuthGate />);

    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Sign in'));
    });
    expect(container.textContent).toContain('Business Obligation Operating System');
    expect(container.querySelector('#login-email')).not.toBeNull();
    expect(container.querySelector('#login-password')).not.toBeNull();
    expect(container.querySelector('.app-shell')).toBeNull();
  });

  it('renders the main app shell for an authenticated demo session', async () => {
    vi.spyOn(nexusApi, 'getAuthStatus').mockResolvedValue(demoAuthenticated);
    vi.spyOn(nexusApi, 'getDashboard').mockResolvedValue(dashboardData);
    vi.spyOn(nexusApi, 'getModuleAvailability').mockResolvedValue([]);
    vi.spyOn(nexusApi, 'getDemoActors').mockResolvedValue(demoActors);
    vi.spyOn(nexusApi, 'getHealth').mockResolvedValue(health);
    const container = renderWithClient(<AuthGate />);

    await act(async () => {
      await vi.waitFor(() => expect(container.querySelector('.sidebar')).not.toBeNull());
    });
    expect(container.querySelector('.app-shell')).not.toBeNull();
    expect(container.textContent).toContain('Local-first mode');
    expect(container.textContent).toContain('Demo tenant');
    expect(container.textContent).toContain('DEMO / SYNTHETIC DATA');
  });
});

describe('settings administration panels', () => {
  it('renders users and backup panels for permitted sessions only', async () => {
    vi.spyOn(nexusApi, 'getSessionContext').mockResolvedValue({
      actorId: 'user-admin-1',
      displayName: 'Amal Administrator',
      responsibility: 'ADMINISTRATOR',
      permissions: ['user:read', 'backup:create'],
    });
    vi.spyOn(nexusApi, 'listWorkspaceUsers').mockResolvedValue({
      users: [
        { userId: 'user-admin-1', displayName: 'Amal Administrator', email: 'amal@example.com', status: 'ACTIVE', roles: ['TENANT_ADMIN'] },
      ],
      availableRoles: ['TENANT_ADMIN', 'COMPLIANCE_OFFICER'],
    });

    const container = renderWithClient(
      <SessionProvider>
        <SettingsPage />
      </SessionProvider>,
    );

    // أُعيد تنظيم الإعدادات إلى أقسام: لوحات الإدارة تعيش داخل قسم «البيانات والأمان»،
    // فيجب الانتقال إليه قبل التحقق من ظهورها — النية السابقة محفوظة كما هي.
    const dataSection = [...container.querySelectorAll<HTMLButtonElement>('.settings-nav__item')]
      .find((button) => button.textContent?.includes('Data & security'));
    expect(dataSection).toBeTruthy();
    await act(async () => {
      dataSection!.click();
    });

    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('Amal Administrator'));
    });
    expect(container.textContent).toContain('Workspace users');
    expect(container.textContent).toContain('Backup and restore');
    // صف المستخدم نفسه: أفعال الذات معطلة مع تلميح المنع الصريح.
    const selfStatusButton = [...container.querySelectorAll<HTMLButtonElement>('tbody button')].find((button) => button.title);
    expect(selfStatusButton?.disabled).toBe(true);
    expect(selfStatusButton?.title).toContain('cannot modify your own account');

    vi.restoreAllMocks();
    vi.spyOn(nexusApi, 'getSessionContext').mockResolvedValue({
      actorId: 'user-member-1',
      displayName: 'Member Only',
      responsibility: 'MEMBER',
      permissions: [],
    });

    const restricted = renderWithClient(
      <SessionProvider>
        <SettingsPage />
      </SessionProvider>,
    );
    // ننتظر تحميل سياق الجلسة فعلياً: الدليل على اكتماله هو اختفاء قسم البيانات
    // الذي كان يظهر عمداً قبل معرفة الصلاحيات. (لوحة الجلسة نفسها داخل ذلك القسم.)
    await act(async () => {
      await vi.waitFor(() => {
        expect([...restricted.querySelectorAll('.settings-nav__item')].map((item) => item.textContent))
          .not.toContain('Data & security');
      });
    });
    expect(restricted.textContent).not.toContain('Workspace users');
    expect(restricted.textContent).not.toContain('Backup and restore');
    // قسم البيانات نفسه يختفي بلا صلاحية داخلية، فلا يبقى قسم فارغ بلا معنى.
    expect([...restricted.querySelectorAll('.settings-nav__item')].map((item) => item.textContent))
      .not.toContain('Data & security');
  });
});
