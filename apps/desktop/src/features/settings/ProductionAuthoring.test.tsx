import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { RegulatorySourceRecord } from '@nexus/contracts';

import { i18n } from '../../lib/i18n';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { SessionContext, type SessionState } from '../../lib/use-session';
import { CreateUserDialog } from './CreateUserDialog';
import { ChangePasswordPanel } from './ChangePasswordPanel';
import { CreateRegulatorySourceDialog } from '../regulatory/CreateRegulatorySourceDialog';
import { CreateObligationDialog } from '../obligations/CreateObligationDialog';

// تغطية واجهات 0026: إنشاء المستخدم وتغيير كلمة المرور وتأليف المصدر والالتزام.
// الاختبارات تقيس السلوك الظاهر (الحظر عند عدم التطابق، رمز الرفض الخلفي، حجب
// الإرسال) ولا تعيد اختبار ضوابط الخلف — تلك لها اختباراتها في Rust.

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const roots: Root[] = [];
const containers: HTMLDivElement[] = [];

const session: SessionState = {
  actorId: 'actor-1',
  displayName: 'Admin',
  responsibility: 'ADMINISTRATOR',
  permissions: ['user:create', 'user:update', 'obligation:create', 'regulatory_intelligence:create'],
  can: () => true,
};

const render = (node: React.ReactNode): HTMLDivElement => {
  const container = document.createElement('div');
  const root = createRoot(container);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  document.body.append(container);
  roots.push(root);
  containers.push(container);
  act(() => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <SessionContext.Provider value={session}>{node}</SessionContext.Provider>
      </QueryClientProvider>,
    );
  });
  return container;
};

const waitFor = (assertion: () => void): Promise<void> => vi.waitFor(assertion, { timeout: 5_000 });

const buttonByText = (container: HTMLElement, text: string): HTMLButtonElement | undefined =>
  [...container.querySelectorAll('button')].find((button) => button.textContent?.includes(text)) as
    | HTMLButtonElement
    | undefined;

const inputByLabel = (container: HTMLElement, label: string): HTMLInputElement => {
  const wrapper = [...container.querySelectorAll('label')].find((node) => node.textContent?.includes(label));
  const input = wrapper?.querySelector('input, select, textarea');
  if (!input) {
    throw new Error(`field not found: ${label}`);
  }
  return input as HTMLInputElement;
};

const setValue = (input: HTMLInputElement, value: string): void => {
  const setter = Object.getOwnPropertyDescriptor(
    Object.getPrototypeOf(input) as object,
    'value',
  )?.set;
  setter?.call(input, value);
  input.dispatchEvent(new Event('input', { bubbles: true }));
  input.dispatchEvent(new Event('change', { bubbles: true }));
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

describe('CreateUserDialog', () => {
  it('blocks submit until the two passwords match and the required fields are filled', async () => {
    const container = render(
      <CreateUserDialog availableRoles={['CONTRIBUTOR', 'TENANT_ADMIN']} onClose={() => undefined} onCreated={() => undefined} />,
    );

    await act(async () => {
      setValue(inputByLabel(container, 'Display name'), 'Field Contributor');
      setValue(inputByLabel(container, 'Email address'), 'contributor@example.test');
      setValue(inputByLabel(container, 'Initial password'), 'Memb3r!Passw0rd#2026');
      setValue(inputByLabel(container, 'Confirm password'), 'different-value');
    });

    expect(container.textContent).toContain('The two passwords do not match.');
    expect(buttonByText(container, 'Create account')?.disabled).toBe(true);
  });

  it('submits the role from the tenant catalogue and reports a backend rejection code', async () => {
    const create = vi
      .spyOn(nexusApi, 'createWorkspaceUser')
      .mockRejectedValue(new NexusApiError('USER_EMAIL_EXISTS', 'duplicate'));
    const container = render(
      <CreateUserDialog availableRoles={['CONTRIBUTOR']} onClose={() => undefined} onCreated={() => undefined} />,
    );

    await act(async () => {
      setValue(inputByLabel(container, 'Display name'), 'Field Contributor');
      setValue(inputByLabel(container, 'Email address'), 'duplicate@example.test');
      setValue(inputByLabel(container, 'Initial password'), 'Memb3r!Passw0rd#2026');
      setValue(inputByLabel(container, 'Confirm password'), 'Memb3r!Passw0rd#2026');
    });

    const submit = buttonByText(container, 'Create account');
    expect(submit?.disabled).toBe(false);
    await act(async () => {
      submit!.click();
      await waitFor(() => expect(container.textContent).toContain('already used by another account'));
    });
    expect(create.mock.calls[0]?.[0]).toEqual({
      displayName: 'Field Contributor',
      email: 'duplicate@example.test',
      initialPassword: 'Memb3r!Passw0rd#2026',
      roleCodes: ['CONTRIBUTOR'],
    });
  });
});

describe('ChangePasswordPanel', () => {
  it('sends only current and new passwords and shows the revoked session count', async () => {
    const change = vi
      .spyOn(nexusApi, 'changePassword')
      .mockResolvedValue({ changedAt: '2026-10-18T12:00:00Z', revokedSessions: 2 });
    const container = render(<ChangePasswordPanel />);

    await act(async () => {
      setValue(inputByLabel(container, 'Current password'), 'Str0ng!Passw0rd#2026');
      setValue(inputByLabel(container, 'New password'), 'R0tated!Passw0rd#2026');
      setValue(inputByLabel(container, 'Confirm new password'), 'R0tated!Passw0rd#2026');
    });
    await act(async () => {
      buttonByText(container, 'Change password')!.click();
      await waitFor(() => expect(container.textContent).toContain('Other sessions revoked: 2'));
    });
    expect(change.mock.calls[0]?.[0]).toEqual({
      currentPassword: 'Str0ng!Passw0rd#2026',
      newPassword: 'R0tated!Passw0rd#2026',
    });
  });

  it('surfaces a wrong current password rejection without claiming success', async () => {
    vi.spyOn(nexusApi, 'changePassword').mockRejectedValue(
      new NexusApiError('CURRENT_PASSWORD_INVALID', 'wrong'),
    );
    const container = render(<ChangePasswordPanel />);

    await act(async () => {
      setValue(inputByLabel(container, 'Current password'), 'Wr0ng!Passw0rd#2026');
      setValue(inputByLabel(container, 'New password'), 'R0tated!Passw0rd#2026');
      setValue(inputByLabel(container, 'Confirm new password'), 'R0tated!Passw0rd#2026');
    });
    await act(async () => {
      buttonByText(container, 'Change password')!.click();
      await waitFor(() => expect(container.textContent).toContain('The current password is not correct'));
    });
    expect(container.textContent).not.toContain('was changed');
  });
});

describe('CreateRegulatorySourceDialog', () => {
  it('submits a user-provided reference and keeps empty optional fields null', async () => {
    const create = vi.spyOn(nexusApi, 'createRegulatorySource').mockResolvedValue({
      source: {
        id: 'source-1',
        authority: 'Data Protection Authority',
        documentTitle: 'Personal Data Protection Regulation',
        official: false,
        verificationState: 'REQUIRES_REVIEW',
        provenance: 'USER_PROVIDED_UNVERIFIED',
        createdAt: '2026-10-18T12:00:00Z',
      },
    });
    const container = render(<CreateRegulatorySourceDialog onClose={() => undefined} onCreated={() => undefined} />);

    await act(async () => {
      setValue(inputByLabel(container, 'Attributed authority'), 'Data Protection Authority');
      setValue(inputByLabel(container, 'Document title'), 'Personal Data Protection Regulation');
    });
    await act(async () => {
      buttonByText(container, 'Register source')!.click();
      await waitFor(() => expect(create).toHaveBeenCalled());
    });
    expect(create.mock.calls[0]?.[0]).toEqual({
      authority: 'Data Protection Authority',
      documentTitle: 'Personal Data Protection Regulation',
      documentVersion: null,
      sourceUrl: null,
    });
  });

  it('reports an unsafe URL rejection from the backend', async () => {
    vi.spyOn(nexusApi, 'createRegulatorySource').mockRejectedValue(
      new NexusApiError('REGULATORY_SOURCE_INVALID', 'unsafe'),
    );
    const container = render(<CreateRegulatorySourceDialog onClose={() => undefined} onCreated={() => undefined} />);

    await act(async () => {
      setValue(inputByLabel(container, 'Attributed authority'), 'Authority');
      setValue(inputByLabel(container, 'Document title'), 'Document');
      setValue(inputByLabel(container, 'Reference URL (optional)'), 'javascript:alert(1)');
    });
    await act(async () => {
      buttonByText(container, 'Register source')!.click();
      await waitFor(() => expect(container.textContent).toContain('not a safe reference'));
    });
  });
});

describe('CreateObligationDialog', () => {
  const sources: readonly RegulatorySourceRecord[] = [
    {
      id: 'source-1',
      authority: 'Data Protection Authority',
      documentTitle: 'Personal Data Protection Regulation',
      documentVersion: '1.0',
      sourceUrl: null,
      retrievedAt: '2026-10-18T12:00:00Z',
      official: false,
      verificationState: 'REQUIRES_REVIEW',
      activeObligationCount: 0,
    },
  ];

  it('creates the obligation with the declared applicability and the chosen source', async () => {
    const create = vi.spyOn(nexusApi, 'createObligation').mockResolvedValue({
      id: 'obl-1',
      status: 'OPEN',
      sourceType: 'USER_PROVIDED',
      verificationState: 'UNVERIFIED',
      createdAt: '2026-10-18T12:00:00Z',
    });
    const onCreated = vi.fn();
    const container = render(
      <CreateObligationDialog sources={sources} onClose={() => undefined} onCreated={onCreated} />,
    );

    await act(async () => {
      setValue(inputByLabel(container, 'Obligation title'), 'Review the data inventory');
      setValue(inputByLabel(container, 'Accountable owner'), 'Privacy Officer');
    });
    await act(async () => {
      buttonByText(container, 'Create obligation')!.click();
      await waitFor(() => expect(onCreated).toHaveBeenCalledWith('obl-1'));
    });
    expect(create.mock.calls[0]?.[0]).toEqual(
      expect.objectContaining({
        sourceId: 'source-1',
        title: 'Review the data inventory',
        owner: 'Privacy Officer',
        applicability: 'APPLIES',
        priority: 'HIGH',
      }),
    );
  });

  it('cannot submit when the organization has no registered source to link', () => {
    const container = render(
      <CreateObligationDialog sources={[]} onClose={() => undefined} onCreated={() => undefined} />,
    );
    expect(buttonByText(container, 'Create obligation')?.disabled).toBe(true);
  });
});
