import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n } from '../../lib/i18n';
import { nexusApi } from '../../lib/nexus-api';
import { SessionProvider } from '../../lib/session-context';
import { useSession } from '../../lib/use-session';
import { THEME_STORAGE_KEY } from '../../lib/theme';
import { SettingsPage } from './SettingsPage';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const renderPage = (): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <SessionProvider>
          <SettingsPage />
        </SessionProvider>
      </QueryClientProvider>,
    );
  });
  return container;
};

// التنقّل بين أقسام الإعدادات انتقل من عرض متراكم إلى أقسام صريحة، لذلك تنتقل
// الاختبارات إلى القسم المقصود قبل التحقق — نفس النية السابقة ببنية جديدة.
const openSection = async (container: HTMLDivElement, label: string): Promise<void> => {
  const target = [...container.querySelectorAll('button')].find((button) => button.textContent?.includes(label));
  expect(target, `settings section "${label}" must exist`).toBeTruthy();
  await act(async () => {
    target!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  });
};

beforeEach(async () => {
  window.localStorage.clear();
  document.documentElement.removeAttribute('data-theme');
  await i18n.changeLanguage('en');
  vi.spyOn(nexusApi, 'getSessionContext').mockResolvedValue({
    actorId: 'user_demo_operator',
    displayName: 'NEXUS Demo Operator (Requester)',
    responsibility: 'REQUESTER',
    permissions: ['obligation:read', 'task:create', 'user:read', 'integration:read'],
  });
  vi.spyOn(nexusApi, 'getAuthStatus').mockResolvedValue({ mode: 'DEMO' } as never);
});

afterEach(async () => {
  act(() => {
    roots.splice(0).forEach((root) => root.unmount());
  });
  containers.splice(0).forEach((container) => container.remove());
  vi.restoreAllMocks();
  window.localStorage.clear();
  document.documentElement.removeAttribute('data-theme');
  await i18n.changeLanguage('ar');
});

describe('SettingsPage', () => {
  it('switches the real interface language through the existing i18n mechanism', async () => {
    const container = renderPage();
    await openSection(container, 'Language');

    const select = container.querySelector('select');
    expect(select).not.toBeNull();
    expect(select!.value).toBe('en');

    const valueSetter = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, 'value')?.set;
    await act(async () => {
      valueSetter?.call(select, 'ar');
      select!.dispatchEvent(new Event('change', { bubbles: true }));
    });

    expect(i18n.resolvedLanguage).toBe('ar');
    expect(document.documentElement.dir).toBe('rtl');
  });

  it('shows the active session actor from backend context without login claims', async () => {
    const container = renderPage();
    await openSection(container, 'Data & security');
    await act(async () => {
      await vi.waitFor(() => expect(container.textContent).toContain('NEXUS Demo Operator'));
    });

    expect(container.textContent).toContain('NEXUS Demo Operator (Requester)');
    expect(container.textContent).toContain('Requester');
    expect(container.textContent).toContain('Real user accounts are not implemented');
  });

  it('states unimplemented settings honestly instead of rendering fake controls', async () => {
    const container = renderPage();
    await openSection(container, 'About');

    expect(container.textContent).toContain('Not implemented in this phase');
    expect(container.textContent).toContain('Security policy, permissions, and approval rules');
    // الإعدادات الأمنية بلا تحكم واجهة — لا مفاتيح ولا سياسات قابلة للتعديل هنا.
    expect(container.querySelectorAll('select').length).toBe(0);
  });

  it('exposes only the sections that are genuinely implemented', () => {
    const container = renderPage();
    const labels = [...container.querySelectorAll('.settings-nav__item')].map((item) => item.textContent);
    expect(labels).toEqual([
      'General',
      'Appearance',
      'Language & region',
      'AI providers',
      'AI models',
      'Data & security',
      'About & diagnostics',
    ]);
  });

  it('hides the data section when the session holds no permission inside it', async () => {
    vi.spyOn(nexusApi, 'getSessionContext').mockResolvedValue({
      actorId: 'user_demo_reviewer',
      displayName: 'Reviewer',
      responsibility: 'REVIEWER',
      permissions: ['obligation:read'],
    });
    const container = renderPage();
    // أثناء تحميل الجلسة تبقى الأقسام ظاهرة (سياسة المشروع: لا إخفاء خاطئ قبل معرفة
    // الصلاحيات)، ثم يختفي قسم البيانات بعد وصول الصلاحيات الفعلية.
    await act(async () => {
      await vi.waitFor(() => {
        const labels = [...container.querySelectorAll('.settings-nav__item')].map((item) => item.textContent);
        expect(labels).toEqual(['General', 'Appearance', 'Language & region', 'About & diagnostics']);
      });
    });
  });

  it('applies and persists the theme choice immediately from the appearance section', async () => {
    const container = renderPage();
    await openSection(container, 'Appearance');

    const dark = container.querySelector<HTMLInputElement>('input[type="radio"][value="dark"]');
    expect(dark).not.toBeNull();

    await act(async () => {
      dark!.click();
    });

    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe('dark');

    const system = container.querySelector<HTMLInputElement>('input[type="radio"][value="system"]');
    await act(async () => {
      system!.click();
    });
    // اختيار «النظام» يزيل السمة ليحكم CSS وحده بلا JavaScript.
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
    expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe('system');
  });
});

describe('useSession permission gating', () => {
  const Probe = () => {
    const { can, permissions } = useSession();
    return (
      <div>
        <span data-testid="count">{permissions.length}</span>
        <span data-testid="can-task">{String(can('task:create'))}</span>
        <span data-testid="can-decide">{String(can('approval:decide'))}</span>
      </div>
    );
  };

  it('reflects backend-computed permissions exactly', () => {
    const container = document.createElement('div');
    const root = createRoot(container);
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    queryClient.setQueryData(['session-context'], {
      actorId: 'user_demo_operator',
      displayName: 'Operator',
      responsibility: 'REQUESTER',
      permissions: ['task:create'],
    });
    document.body.append(container);
    roots.push(root);
    containers.push(container);
    act(() => {
      root.render(
        <QueryClientProvider client={queryClient}>
          <SessionProvider>
            <Probe />
          </SessionProvider>
        </QueryClientProvider>,
      );
    });

    expect(container.querySelector('[data-testid="count"]')?.textContent).toBe('1');
    expect(container.querySelector('[data-testid="can-task"]')?.textContent).toBe('true');
    expect(container.querySelector('[data-testid="can-decide"]')?.textContent).toBe('false');
  });
});
