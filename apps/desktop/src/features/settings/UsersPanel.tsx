import { useState } from 'react';
import { CircleAlert, LoaderCircle, RefreshCw, UserPlus, UserRound, X } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { WorkspaceUser } from '@nexus/contracts';

import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { useSession } from '../../lib/use-session';
import { ConfirmDialog } from '../../components/ConfirmDialog';
import { CreateUserDialog } from './CreateUserDialog';

// إدارة مستخدمي مساحة العمل: كل إجراء يمر بالخلف حصراً من كتالوج أدوار المستأجر.
// إجراءات الذات معطلة في الواجهة أيضاً — الخلف يرفضها برمز SELF_MODIFICATION_FORBIDDEN.
// الإجراءات غير القابلة للتراجع تمر بحوار تأكيد داخلي (window.confirm غير منفَّذ
// في WebView2/Tauri تحت سياسة القدرات، فلا يجوز الاعتماد عليه لتأكيد فعلي).
export const UsersPanel = () => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { actorId, can } = useSession();
  const [feedback, setFeedback] = useState<string | null>(null);
  const [mutationError, setMutationError] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [pending, setPending] = useState<{
    readonly title: string;
    readonly message: string;
    readonly confirmLabel: string;
    readonly action: () => Promise<void>;
  } | null>(null);

  const usersQuery = useQuery({
    queryKey: ['workspace-users'],
    queryFn: nexusApi.listWorkspaceUsers,
    staleTime: 30_000,
  });

  const setStatusMutation = useMutation({ mutationFn: nexusApi.setUserStatus });
  const assignRoleMutation = useMutation({ mutationFn: nexusApi.assignUserRole });
  const revokeRoleMutation = useMutation({ mutationFn: nexusApi.revokeUserRole });
  const revokeSessionsMutation = useMutation({ mutationFn: nexusApi.revokeUserSessions });

  // تغيير المستخدم قد يغيّر صلاحيات الجلسة نفسها — نُبطل قائمة المستخدمين وسياق الجلسة معاً.
  const runUserMutation = async (action: () => Promise<unknown>, successKey = 'feedback.userUpdated'): Promise<void> => {
    setFeedback(null);
    setMutationError(null);
    try {
      await action();
      await queryClient.invalidateQueries({ queryKey: ['workspace-users'] });
      await queryClient.invalidateQueries({ queryKey: ['session-context'] });
      setFeedback(t(successKey));
    } catch (error) {
      setMutationError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  const changeStatus = async (user: WorkspaceUser): Promise<void> => {
    const nextStatus = user.status === 'ACTIVE' ? 'DISABLED' : 'ACTIVE';
    const apply = async (): Promise<void> => {
      await runUserMutation(() => setStatusMutation.mutateAsync({ userId: user.userId, status: nextStatus }));
    };
    if (nextStatus === 'DISABLED') {
      setPending({
        title: t('users.disable'),
        message: t('users.disableConfirm'),
        confirmLabel: t('users.disable'),
        action: apply,
      });
      return;
    }
    await apply();
  };

  const assignRole = async (user: WorkspaceUser, roleCode: string): Promise<void> => {
    await runUserMutation(() => assignRoleMutation.mutateAsync({ userId: user.userId, roleCode }));
  };

  const revokeRole = async (user: WorkspaceUser, roleCode: string): Promise<void> => {
    await runUserMutation(() => revokeRoleMutation.mutateAsync({ userId: user.userId, roleCode }));
  };

  // إلغاء جلسات مستخدم: يقطع وصوله الفعلي فوراً (فقد جهاز/اشتباه تسريب جلسة).
  const revokeSessions = async (user: WorkspaceUser): Promise<void> => {
    setPending({
      title: t('users.revokeSessions'),
      message: t('users.revokeSessionsConfirm', { name: user.displayName }),
      confirmLabel: t('users.revokeSessions'),
      action: () => runUserMutation(
        () => revokeSessionsMutation.mutateAsync({ userId: user.userId }),
        'feedback.userSessionsRevoked',
      ),
    });
  };

  const users = usersQuery.data?.users ?? [];
  const availableRoles = usersQuery.data?.availableRoles ?? [];
  const listErrorCode = usersQuery.error instanceof NexusApiError ? usersQuery.error.code : 'LOCAL_SERVICE_UNAVAILABLE';
  const anyMutationPending = setStatusMutation.isPending
    || assignRoleMutation.isPending
    || revokeRoleMutation.isPending
    || revokeSessionsMutation.isPending;
  const canCreate = can('user:create');

  return (
    <section className="panel" aria-labelledby="settings-users-title">
      <div className="panel__header">
        <div><h2 id="settings-users-title">{t('users.title')}</h2><span>{users.length}</span></div>
        <div className="page-heading__actions">
          {canCreate ? (
            <button className="button button--primary" type="button" onClick={() => setCreateOpen(true)}>
              <UserPlus size={16} aria-hidden="true" />
              {t('users.create.open')}
            </button>
          ) : null}
          <button
            className="icon-button"
            type="button"
            onClick={() => { void usersQuery.refetch(); }}
            aria-label={t('actions.refresh')}
            title={t('actions.refresh')}
            disabled={usersQuery.isFetching}
          >
            <RefreshCw size={16} className={usersQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
          </button>
        </div>
      </div>

      {feedback ? <p className="form-note" role="status">{feedback}</p> : null}
      {mutationError ? (
        <div className="error-panel" role="alert">
          <CircleAlert size={20} aria-hidden="true" />
          <p>{t(`errors.${mutationError}`, { defaultValue: t('errors.unavailable') })}</p>
        </div>
      ) : null}
      {usersQuery.isLoading ? (
        <div className="loading-panel">
          <LoaderCircle className="spin" size={18} aria-hidden="true" />
          <span>{t('users.loading')}</span>
        </div>
      ) : null}
      {usersQuery.isError ? (
        <div className="error-panel" role="alert">
          <CircleAlert size={20} aria-hidden="true" />
          <p>{t(`errors.${listErrorCode}`, { defaultValue: t('errors.unavailable') })}</p>
        </div>
      ) : null}

      {!usersQuery.isLoading && !usersQuery.isError && users.length > 0 ? (
        <div className="table-scroll">
          <table>
            <thead>
              <tr>
                <th>{t('users.table.user')}</th>
                <th>{t('users.table.roles')}</th>
                <th>{t('users.table.status')}</th>
                <th>{t('users.table.actions')}</th>
              </tr>
            </thead>
            <tbody>
              {users.map((user) => {
                const isSelf = user.userId === actorId;
                const selfHint = isSelf ? t('users.selfHint') : undefined;
                const unassignedRoles = availableRoles.filter((role) => !user.roles.includes(role));
                return (
                  <tr key={user.userId}>
                    <td>
                      <div className="table-primary">
                        <span>{user.displayName}</span>
                        <small>{user.email}</small>
                      </div>
                    </td>
                    <td>
                      <div className="user-roles">
                        {user.roles.map((role) => (
                          <span key={role} className="user-roles__chip">
                            {/* bdi يعزل رمز الدور اللاتيني غير المترجم عن اتجاه النص العربي فيمنع انقلاب ترتيبه. */}
                            <bdi>{t(`users.role.${role}`, { defaultValue: role })}</bdi>
                            <button
                              className="user-roles__remove"
                              type="button"
                              onClick={() => { void revokeRole(user, role); }}
                              disabled={isSelf || anyMutationPending}
                              title={selfHint}
                              aria-label={t('users.removeRole', { role: t(`users.role.${role}`, { defaultValue: role }) })}
                            >
                              <X size={14} aria-hidden="true" />
                            </button>
                          </span>
                        ))}
                        {unassignedRoles.length > 0 ? (
                          <label className="user-roles__add">
                            <span className="visually-hidden">{t('users.addRole')}</span>
                            <select
                              value=""
                              onChange={(event) => {
                                const roleCode = event.currentTarget.value;
                                if (roleCode) {
                                  void assignRole(user, roleCode);
                                }
                              }}
                              disabled={isSelf || anyMutationPending}
                              title={selfHint}
                              aria-label={t('users.addRole')}
                            >
                              <option value="">{t('users.addRole')}</option>
                              {unassignedRoles.map((role) => (
                                <option key={role} value={role}>{t(`users.role.${role}`, { defaultValue: role })}</option>
                              ))}
                            </select>
                          </label>
                        ) : (
                          <small className="user-roles__empty">{t('users.noUnassignedRoles')}</small>
                        )}
                      </div>
                    </td>
                    <td>
                      {user.status === 'ACTIVE'
                        ? <span className="badge badge--low">{t('status.ACTIVE')}</span>
                        : <span className="status-badge">{t('status.DISABLED')}</span>}
                    </td>
                    <td>
                      <div className="page-heading__actions">
                        <button
                          className="button button--quiet"
                          type="button"
                          onClick={() => { void changeStatus(user); }}
                          disabled={isSelf || anyMutationPending}
                          title={selfHint}
                        >
                          {user.status === 'ACTIVE' ? t('users.disable') : t('users.enable')}
                        </button>
                        {can('user:update') ? (
                          <button
                            className="button button--quiet"
                            type="button"
                            onClick={() => { void revokeSessions(user); }}
                            disabled={isSelf || anyMutationPending}
                            title={selfHint}
                          >
                            {t('users.revokeSessions')}
                          </button>
                        ) : null}
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      ) : null}
      {!usersQuery.isLoading && !usersQuery.isError && users.length === 0 ? (
        <div className="empty-state">
          <UserRound size={20} aria-hidden="true" />
          <span>{t('users.empty')}</span>
        </div>
      ) : null}
      <p className="form-note">{t('users.subtitle')}</p>
      {createOpen ? (
        <CreateUserDialog
          availableRoles={availableRoles}
          onClose={() => setCreateOpen(false)}
          onCreated={() => setFeedback(t('feedback.userCreated'))}
        />
      ) : null}
      {pending ? (
        <ConfirmDialog
          title={pending.title}
          message={pending.message}
          confirmLabel={pending.confirmLabel}
          cancelLabel={t('actions.cancel')}
          onCancel={() => setPending(null)}
          onConfirm={() => {
            const action = pending.action;
            setPending(null);
            void action();
          }}
        />
      ) : null}
    </section>
  );
};
