import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { BootstrapOptionsData } from '@nexus/contracts';
import type { ReactNode } from 'react';
import '../../lib/i18n';
import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { SetupScreen } from './SetupScreen';

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// كتالوج مصغّر يكفي لفحص العقد: الترتيب يثبت أن العرض يتبع الكتالوج القادم من الخلف.
const bootstrapOptions: BootstrapOptionsData = {
  countryCodes: ['SA', 'AE', 'QA'],
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

const flushOptions = async (container: HTMLDivElement): Promise<void> => {
  await act(async () => {
    await vi.waitFor(() => {
      expect(container.querySelectorAll('#setup-country-code option').length).toBeGreaterThan(1);
    });
  });
};

const requireElement = <T extends Element>(container: HTMLElement, selector: string): T => {
  const element = container.querySelector<T>(selector);
  if (element === null) throw new Error(`العنصر المطلوب غير موجود: ${selector}`);
  return element;
};

const setFieldValue = (container: HTMLElement, selector: string, value: string): void => {
  const element = requireElement<HTMLInputElement | HTMLSelectElement>(container, selector);
  const prototype = element instanceof HTMLSelectElement
    ? HTMLSelectElement.prototype
    : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(prototype, 'value')?.set?.call(element, value);
  element.dispatchEvent(new Event('input', { bubbles: true }));
  element.dispatchEvent(new Event('change', { bubbles: true }));
};

const fillValidForm = (container: HTMLElement): void => {
  setFieldValue(container, '#setup-organization-name', 'Nexus Test Group');
  setFieldValue(container, '#setup-country-code', 'QA');
  setFieldValue(container, '#setup-industry', 'Financial Services');
  setFieldValue(container, '#setup-admin-display-name', 'Test Administrator');
  setFieldValue(container, '#setup-admin-email', 'admin@example.test');
  setFieldValue(container, '#setup-password', 'Str0ng!Passw0rd#2026');
};

const submitForm = async (container: HTMLElement): Promise<void> => {
  const form = requireElement<HTMLFormElement>(container, 'form');
  await act(async () => {
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
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

describe('production setup declares organization data instead of assuming it', () => {
  it('renders the country and industry fields from the backend catalog', async () => {
    vi.spyOn(nexusApi, 'getBootstrapOptions').mockResolvedValue(bootstrapOptions);
    const container = renderWithClient(<SetupScreen />);
    await flushOptions(container);

    expect(container.textContent).toContain('Country');
    expect(container.textContent).toContain('Industry / sector');
    expect(container.querySelector('#setup-country-code')).not.toBeNull();
    expect(container.querySelector('#setup-industry')).not.toBeNull();

    // الخيار الأول هو العنصر الفارغ: لا دولة مختارة مسبقاً.
    const options = [...container.querySelectorAll<HTMLOptionElement>('#setup-country-code option')];
    expect(options[0].value).toBe('');
    // بقية الخيارات هي رموز الكتالوج حرفياً ومرتبة أبجدياً بالاسم المعروض.
    expect(options.slice(1).map((option) => option.value)).toEqual(['QA', 'SA', 'AE']);

    // مقترحات القطاع تلميح كتابة فقط، والحقل يبقى قابلاً للكتابة الحرة.
    const suggestions = [...container.querySelectorAll<HTMLOptionElement>('#setup-industry-suggestions option')];
    expect(suggestions.length).toBeGreaterThan(5);
    expect(container.querySelector<HTMLInputElement>('#setup-industry')?.getAttribute('list'))
      .toBe('setup-industry-suggestions');
  });

  it('keeps keyboard order and label association for the new fields', async () => {
    vi.spyOn(nexusApi, 'getBootstrapOptions').mockResolvedValue(bootstrapOptions);
    const container = renderWithClient(<SetupScreen />);
    await flushOptions(container);

    const order = [...container.querySelectorAll<HTMLElement>('input, select, button[type="submit"]')]
      .map((element) => element.id);
    expect(order).toEqual([
      'setup-organization-name',
      'setup-admin-display-name',
      'setup-country-code',
      'setup-industry',
      'setup-admin-email',
      'setup-password',
      '',
    ]);

    for (const id of ['setup-organization-name', 'setup-country-code', 'setup-industry', 'setup-admin-email', 'setup-password']) {
      expect(container.querySelector(`label[for="${id}"]`)).not.toBeNull();
    }
    // التركيز ينتقل فعلاً إلى حقل الدولة (لا حجب لوحة المفاتيح).
    const country = container.querySelector<HTMLSelectElement>('#setup-country-code');
    country?.focus();
    expect(document.activeElement).toBe(country);
  });

  it('shows Arabic labels and right-to-left direction in the Arabic interface', async () => {
    await act(async () => {
      await i18n.changeLanguage('ar');
    });
    vi.spyOn(nexusApi, 'getBootstrapOptions').mockResolvedValue(bootstrapOptions);
    const container = renderWithClient(<SetupScreen />);
    await flushOptions(container);

    expect(container.textContent).toContain('الدولة');
    expect(container.textContent).toContain('القطاع / النشاط');
    expect(document.documentElement.dir).toBe('rtl');
    // أسماء الدول تأتي من ICU بلغة الواجهة مع الرمز.
    const labels = [...container.querySelectorAll<HTMLOptionElement>('#setup-country-code option')]
      .map((option) => option.textContent ?? '');
    expect(labels.some((label) => label.includes('قطر') && label.includes('QA'))).toBe(true);
  });

  it('blocks submission until the country is chosen and the sector is valid', async () => {
    vi.spyOn(nexusApi, 'getBootstrapOptions').mockResolvedValue(bootstrapOptions);
    const bootstrap = vi.spyOn(nexusApi, 'bootstrapProduction');
    const container = renderWithClient(<SetupScreen />);
    await flushOptions(container);

    await submitForm(container);
    expect(container.textContent).toContain('Complete all required fields.');
    expect(bootstrap).not.toHaveBeenCalled();

    fillValidForm(container);
    // دولة غير معروفة في الكتالوج لا تُقبل من الواجهة (والخلف يرفضها أيضاً).
    setFieldValue(container, '#setup-country-code', '');
    await submitForm(container);
    expect(container.textContent).toContain('Select the country from the supported list.');
    expect(bootstrap).not.toHaveBeenCalled();

    // قطاع من وسوم HTML مرفوض قبل الإرسال، ومعه لا طلب خلفي.
    setFieldValue(container, '#setup-country-code', 'QA');
    setFieldValue(container, '#setup-industry', '<script>alert(1)</script>');
    await submitForm(container);
    expect(container.textContent).toContain('Enter a valid sector');
    expect(bootstrap).not.toHaveBeenCalled();
  });

  it('sends exactly the declared organization data and normalizes whitespace', async () => {
    vi.spyOn(nexusApi, 'getBootstrapOptions').mockResolvedValue(bootstrapOptions);
    const bootstrap = vi.spyOn(nexusApi, 'bootstrapProduction').mockResolvedValue({
      context: { actorId: 'user-1', displayName: 'Test Administrator', responsibility: 'ADMINISTRATOR', permissions: [] },
      sessionExpiresAt: '2030-01-01T00:00:00Z',
    });
    const container = renderWithClient(<SetupScreen />);
    await flushOptions(container);

    fillValidForm(container);
    setFieldValue(container, '#setup-industry', '  Financial   Services  ');

    await submitForm(container);

    expect(bootstrap).toHaveBeenCalledTimes(1);
    // الحمولة الفعلية هي الوسيط الأول (الثاني سياق الطفرة من TanStack).
    expect(bootstrap.mock.calls[0]?.[0]).toEqual({
      organizationName: 'Nexus Test Group',
      countryCode: 'QA',
      industry: 'Financial Services',
      adminDisplayName: 'Test Administrator',
      adminEmail: 'admin@example.test',
      password: 'Str0ng!Passw0rd#2026',
    });
  });

  it('prevents a duplicate submission while the first request is in flight', async () => {
    vi.spyOn(nexusApi, 'getBootstrapOptions').mockResolvedValue(bootstrapOptions);
    const bootstrap = vi.spyOn(nexusApi, 'bootstrapProduction').mockImplementation(
      () => new Promise(() => { /* يبقى الطلب معلقاً عمداً */ }),
    );
    const container = renderWithClient(<SetupScreen />);
    await flushOptions(container);
    fillValidForm(container);

    await submitForm(container);
    await submitForm(container);
    const submitButton = container.querySelector<HTMLButtonElement>('button[type="submit"]');
    await act(async () => {
      submitButton?.click();
    });

    expect(bootstrap).toHaveBeenCalledTimes(1);
    expect(submitButton?.disabled).toBe(true);
  });

  it('translates backend rejection of country or sector and marks the field', async () => {
    vi.spyOn(nexusApi, 'getBootstrapOptions').mockResolvedValue(bootstrapOptions);
    const bootstrap = vi.spyOn(nexusApi, 'bootstrapProduction').mockRejectedValue(
      new NexusApiError('BOOTSTRAP_COUNTRY_INVALID', 'rejected'),
    );
    const container = renderWithClient(<SetupScreen />);
    await flushOptions(container);
    fillValidForm(container);
    await submitForm(container);

    expect(container.textContent).toContain('Select a country from the supported ISO 3166-1 alpha-2 list.');
    expect(container.querySelector('#setup-country-code')?.getAttribute('aria-invalid')).toBe('true');
    expect(document.activeElement).toBe(container.querySelector('#setup-country-code'));

    bootstrap.mockRejectedValue(new NexusApiError('BOOTSTRAP_INDUSTRY_INVALID', 'rejected'));
    await submitForm(container);
    expect(container.textContent).toContain('The sector name is empty, too long, or contains unsupported symbols.');
    expect(container.querySelector('#setup-industry')?.getAttribute('aria-invalid')).toBe('true');
    expect(document.activeElement).toBe(container.querySelector('#setup-industry'));
  });

  it('fails closed and explains the failure when the catalog cannot be loaded', async () => {
    vi.spyOn(nexusApi, 'getBootstrapOptions').mockRejectedValue(new NexusApiError('STORAGE_ERROR', 'failed'));
    const bootstrap = vi.spyOn(nexusApi, 'bootstrapProduction');
    const container = renderWithClient(<SetupScreen />);

    await act(async () => {
      await vi.waitFor(() => {
        expect(container.textContent).toContain('The supported country list could not be loaded.');
      });
    });

    // بلا كتالوج لا يمكن إعلان دولة: الإرسال يبقى مرفوضاً في الواجهة والخلف.
    fillValidForm(container);
    await submitForm(container);
    expect(container.textContent).toContain('Select the country from the supported list.');
    expect(bootstrap).not.toHaveBeenCalled();
  });
});
