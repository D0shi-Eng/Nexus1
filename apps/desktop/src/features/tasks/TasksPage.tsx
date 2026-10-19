import { useMemo, useState } from 'react';
import { CheckCircle2, ClipboardCheck, Filter, History, RefreshCw, Search, UserPlus, X } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { TaskPriority, TaskRecord, TaskStatus } from '@nexus/contracts';
import { taskPriorities, taskStatuses } from '@nexus/contracts';

import { PageHeader } from '../../components/PageHeader';
import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { resolveErrorCode } from '../../lib/api-error';
import { useSession } from '../../lib/use-session';
import { useModalFocus } from '../../lib/use-modal-focus';

type TaskStatusFilter = 'ALL' | TaskStatus;
type TaskPriorityFilter = 'ALL' | TaskPriority;

const formatDate = (value: string | null, language: string): string => {
  if (!value) {
    return '—';
  }

  return new Intl.DateTimeFormat(language, { day: 'numeric', month: 'short', year: 'numeric' }).format(new Date(value));
};

const toStatusFilter = (value: string): TaskStatusFilter =>
  value === 'ALL' ? 'ALL' : taskStatuses.find((status) => status === value) ?? 'ALL';

const toPriorityFilter = (value: string): TaskPriorityFilter =>
  value === 'ALL' ? 'ALL' : taskPriorities.find((priority) => priority === value) ?? 'ALL';

const matchesTask = (
  task: TaskRecord,
  query: string,
  status: TaskStatusFilter,
  priority: TaskPriorityFilter,
  mineOnly: boolean,
  currentUserId: string | null,
): boolean => {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const matchesQuery = !normalizedQuery
    || `${task.title} ${task.description} ${task.obligationTitle}`.toLocaleLowerCase().includes(normalizedQuery);
  const matchesStatus = status === 'ALL' || task.status === status;
  const matchesPriority = priority === 'ALL' || task.priority === priority;
  const matchesMine = !mineOnly || (currentUserId !== null && task.assigneeUserId === currentUserId);
  return matchesQuery && matchesStatus && matchesPriority && matchesMine;
};

const TaskBadge = ({ value }: { value: TaskStatus | TaskPriority }) => {
  const { t } = useTranslation();
  return <span className={`badge badge--${value.toLowerCase()}`}>{t(`status.${value}`)}</span>;
};

// حوار إجراء واحد على المهمة: نقل الحالة أو الإسناد. السبب اختياري في الانتقال
// وإلزامي في قرار الاعتماد (وهو لا يمر من هنا بل من مسار الموافقات).
const TaskActionDialog = ({
  task,
  targetStatus,
  onClose,
  onDone,
}: {
  task: TaskRecord;
  targetStatus: TaskStatus;
  onClose: () => void;
  onDone: () => void;
}) => {
  const { t } = useTranslation();
  const { dialogRef } = useModalFocus(onClose);
  const [reason, setReason] = useState('');
  const [failure, setFailure] = useState<string | null>(null);
  const mutation = useMutation({
    mutationFn: () => nexusApi.transitionTask({
      taskId: task.id,
      targetStatus,
      reason: reason.trim() ? reason.trim() : null,
      expectedVersion: task.version,
    }),
    onSuccess: () => { onDone(); onClose(); },
    onError: (error: unknown) => { setFailure(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE'); },
  });

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="task-action-title" tabIndex={-1}>
        <header className="task-dialog__header">
          <div><h2 id="task-action-title">{t(`tasks.transition.${targetStatus}`)}</h2><p>{task.title}</p></div>
          <button className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}><X size={18} aria-hidden="true" /></button>
        </header>
        <div className="task-form">
          <label>
            <span>{t('tasks.action.reason')}</span>
            <textarea value={reason} onChange={(event) => setReason(event.currentTarget.value)} maxLength={1000} rows={3} />
          </label>
          {failure ? <p className="form-error" role="alert">{t(`errors.${failure}`, { defaultValue: t('errors.STORAGE_ERROR') })}</p> : null}
          <div className="task-form__actions">
            <button className="button button--primary" type="button" disabled={mutation.isPending} onClick={() => mutation.mutate()}>
              {t('tasks.action.confirm')}
            </button>
            <button className="button button--quiet" type="button" onClick={onClose}>{t('actions.cancel')}</button>
          </div>
        </div>
      </section>
    </div>
  );
};

const TaskHistoryDialog = ({ task, onClose }: { task: TaskRecord; onClose: () => void }) => {
  const { t, i18n } = useTranslation();
  const { dialogRef } = useModalFocus(onClose);
  const language = i18n.resolvedLanguage ?? 'ar';
  const historyQuery = useQuery({
    queryKey: ['task-history', task.id],
    queryFn: () => nexusApi.getTaskHistory(task.id),
    staleTime: 0,
  });

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="task-history-title" tabIndex={-1}>
        <header className="task-dialog__header">
          <div><h2 id="task-history-title">{t('tasks.history.title')}</h2><p>{task.title}</p></div>
          <button className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}><X size={18} aria-hidden="true" /></button>
        </header>
        {historyQuery.isLoading ? <LoadingState label={t('tasks.loading')} /> : null}
        {historyQuery.data && historyQuery.data.length === 0 ? (
          <EmptyState icon={History} variant="inline" title={t('tasks.history.empty')} />
        ) : null}
        {historyQuery.data && historyQuery.data.length > 0 ? (
          <ol className="audit-list">
            {historyQuery.data.map((record, index) => (
              <li key={`${record.occurredAt}-${index}`}>
                <span className="audit-list__icon"><History size={16} aria-hidden="true" /></span>
                <div>
                  <strong>{t(`status.${record.fromStatus}`)} → {t(`status.${record.toStatus}`)}</strong>
                  <span>{record.actorDisplayName}{record.reason ? ` · ${record.reason}` : ''}</span>
                </div>
                <time dateTime={record.occurredAt}>{formatDate(record.occurredAt, language)}</time>
              </li>
            ))}
          </ol>
        ) : null}
      </section>
    </div>
  );
};

export const TasksPage = () => {
  const { t, i18n } = useTranslation();
  const { can, actorId } = useSession();
  const queryClient = useQueryClient();
  const [query, setQuery] = useState('');
  const [status, setStatus] = useState<TaskStatusFilter>('ALL');
  const [priority, setPriority] = useState<TaskPriorityFilter>('ALL');
  const [mineOnly, setMineOnly] = useState(false);
  const [action, setAction] = useState<{ task: TaskRecord; target: TaskStatus } | null>(null);
  const [historyTask, setHistoryTask] = useState<TaskRecord | null>(null);
  const [assignTargets, setAssignTargets] = useState<Record<string, string>>({});
  const [failure, setFailure] = useState<string | null>(null);

  const canReadUsers = can('user:read');
  const language = i18n.resolvedLanguage ?? 'ar';
  const tasksQuery = useQuery({
    queryKey: ['tasks'],
    queryFn: nexusApi.getTasks,
    staleTime: 30_000,
  });
  // قائمة الأعضاء تُطلب فقط لمن يملك قراءتها — لا طلب يُرفض أصلاً.
  const usersQuery = useQuery({
    queryKey: ['workspace-users'],
    queryFn: nexusApi.listWorkspaceUsers,
    enabled: canReadUsers,
    staleTime: 60_000,
  });

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ['tasks'] });
    await queryClient.invalidateQueries({ queryKey: ['dashboard'] });
  };

  const assignMutation = useMutation({
    mutationFn: (input: { task: TaskRecord; assigneeUserId: string }) => nexusApi.assignTask({
      taskId: input.task.id,
      assigneeUserId: input.assigneeUserId,
      dueAt: null,
      expectedVersion: input.task.version,
    }),
    onSuccess: () => { setFailure(null); void refresh(); },
    onError: (error: unknown) => { setFailure(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE'); },
  });

  const errorCode = resolveErrorCode(tasksQuery.error);
  const visibleTasks = useMemo(
    () => (tasksQuery.data?.items ?? []).filter((task) => matchesTask(task, query, status, priority, mineOnly, actorId)),
    [mineOnly, priority, query, status, tasksQuery.data?.items, actorId],
  );
  const noData = Boolean(tasksQuery.data && !tasksQuery.data.hasData);
  const hasFilteredOutTasks = Boolean(tasksQuery.data?.items.length) && visibleTasks.length === 0;

  const resetFilters = (): void => {
    setQuery('');
    setStatus('ALL');
    setPriority('ALL');
    setMineOnly(false);
  };

  return (
    <section id="tasks" className="tasks-page content-container" aria-labelledby="tasks-title" aria-live="polite">
      <PageHeader
        eyebrow={t('tasks.eyebrow')}
        eyebrowIcon={ClipboardCheck}
        title={t('tasks.title')}
        titleId="tasks-title"
        description={t('tasks.subtitle')}
        actions={
          <button className="button button--secondary" type="button" onClick={() => { void tasksQuery.refetch(); }} disabled={tasksQuery.isFetching}>
            <RefreshCw size={16} className={tasksQuery.isFetching ? 'spin' : undefined} aria-hidden="true" />
            {t('tasks.actions.refresh')}
          </button>
        }
      />

      {tasksQuery.isLoading ? <LoadingState label={t('tasks.loading')} /> : null}
      {tasksQuery.isError ? (
        <ErrorState code={errorCode} deniedMessage={t('tasks.permissionDenied')} />
      ) : null}
      {noData ? (
        <EmptyState icon={ClipboardCheck} title={t('tasks.empty')} description={t('tasks.emptyHint')} />
      ) : null}
      {failure ? <p className="form-error" role="alert">{t(`errors.${failure}`, { defaultValue: t('errors.STORAGE_ERROR') })}</p> : null}

      {tasksQuery.data && tasksQuery.data.hasData ? (
        <>
          <section className="metric-grid" aria-label={t('tasks.summaryLabel')}>
            <article className="metric-card"><span>{t('tasks.summary.total')}</span><strong>{tasksQuery.data.summary.total}</strong><small>{t(`status.${tasksQuery.data.dataMode}`)}</small></article>
            <article className="metric-card"><span>{t('tasks.summary.assignedToMe')}</span><strong>{tasksQuery.data.summary.assignedToMe}</strong><small>{t('tasks.filters.mine')}</small></article>
            <article className="metric-card"><span>{t('tasks.summary.open')}</span><strong>{tasksQuery.data.summary.open}</strong><small>{t('status.OPEN')}</small></article>
            <article className="metric-card"><span>{t('tasks.summary.inProgress')}</span><strong>{tasksQuery.data.summary.inProgress}</strong><small>{t('status.IN_PROGRESS')}</small></article>
            <article className="metric-card"><span>{t('tasks.summary.readyForReview')}</span><strong>{tasksQuery.data.summary.readyForReview}</strong><small>{t('status.READY_FOR_REVIEW')}</small></article>
            <article className="metric-card metric-card--critical"><span>{t('tasks.summary.rejected')}</span><strong>{tasksQuery.data.summary.rejected}</strong><small>{t('status.REJECTED')}</small></article>
            <article className="metric-card metric-card--critical"><span>{t('tasks.summary.overdue')}</span><strong>{tasksQuery.data.summary.overdue}</strong><small>{t('tasks.summary.overdueHint')}</small></article>
            <article className="metric-card"><span>{t('tasks.summary.completed')}</span><strong>{tasksQuery.data.summary.completed}</strong><small>{t('status.COMPLETED')}</small></article>
          </section>

          <section className="panel panel--table" aria-labelledby="tasks-list-title">
            <div className="panel__header"><div><h2 id="tasks-list-title">{t('tasks.list.title')}</h2><span>{visibleTasks.length}</span></div><ClipboardCheck size={18} aria-hidden="true" /></div>
            <div className="attention-toolbar" aria-label={t('tasks.filters.title')}>
              <div className="attention-toolbar__heading"><Filter size={16} aria-hidden="true" /><h3>{t('tasks.filters.title')}</h3></div>
              <label className="attention-search"><span className="visually-hidden">{t('actions.search')}</span><Search size={16} aria-hidden="true" /><input value={query} onChange={(event) => setQuery(event.currentTarget.value)} placeholder={t('tasks.searchPlaceholder')} /></label>
              <label>
                <span>{t('tasks.filters.status')}</span>
                <select value={status} onChange={(event) => setStatus(toStatusFilter(event.currentTarget.value))}>
                  <option value="ALL">{t('tasks.filters.all')}</option>
                  {taskStatuses.map((value) => <option value={value} key={value}>{t(`status.${value}`)}</option>)}
                </select>
              </label>
              <label>
                <span>{t('tasks.filters.priority')}</span>
                <select value={priority} onChange={(event) => setPriority(toPriorityFilter(event.currentTarget.value))}>
                  <option value="ALL">{t('tasks.filters.all')}</option>
                  {taskPriorities.map((value) => <option value={value} key={value}>{t(`status.${value}`)}</option>)}
                </select>
              </label>
              <label className="checkbox-field">
                <input type="checkbox" checked={mineOnly} onChange={(event) => setMineOnly(event.currentTarget.checked)} />
                <span>{t('tasks.filters.mine')}</span>
              </label>
              <button className="button button--quiet" type="button" onClick={resetFilters} disabled={!query && status === 'ALL' && priority === 'ALL' && !mineOnly}>{t('tasks.actions.clearFilters')}</button>
            </div>
            {visibleTasks.length > 0 ? (
              <div className="table-scroll">
                <table>
                  <thead><tr>
                    <th>{t('tasks.table.task')}</th>
                    <th>{t('tasks.table.obligation')}</th>
                    <th>{t('tasks.table.assignee')}</th>
                    <th>{t('tasks.table.dueAt')}</th>
                    <th>{t('tasks.table.priority')}</th>
                    <th>{t('table.status')}</th>
                    <th>{t('tasks.table.actions')}</th>
                  </tr></thead>
                  <tbody>{visibleTasks.map((task) => (
                    <tr key={task.id}>
                      <td><div className="table-primary"><span>{task.title}</span><small>{task.description}</small></div></td>
                      <td>{task.obligationTitle}</td>
                      <td>{task.assigneeDisplayName ?? <span className="text-muted">{t('tasks.assign.unassigned')}</span>}</td>
                      <td><time dateTime={task.dueAt ?? undefined}>{formatDate(task.dueAt, language)}</time></td>
                      <td><TaskBadge value={task.priority} /></td>
                      <td><TaskBadge value={task.status} /></td>
                      <td>
                        <div className="row-actions">
                          {task.availableTransitions.map((target) => (
                            <button key={target} className="button button--quiet" type="button" onClick={() => setAction({ task, target })}>
                              {t(`tasks.transition.${target}`)}
                            </button>
                          ))}
                          {task.canAssign && task.assigneeUserId !== actorId ? (
                            <button
                              className="button button--quiet"
                              type="button"
                              disabled={assignMutation.isPending}
                              onClick={() => assignMutation.mutate({ task, assigneeUserId: actorId })}
                            >
                              <UserPlus size={14} aria-hidden="true" /> {t('tasks.assign.self')}
                            </button>
                          ) : null}
                          {task.canAssign && canReadUsers && usersQuery.data ? (
                            <span className="row-actions__assign">
                              <label>
                                <span className="visually-hidden">{t('tasks.assign.select')}</span>
                                <select
                                  value={assignTargets[task.id] ?? ''}
                                  onChange={(event) => setAssignTargets((current) => ({ ...current, [task.id]: event.currentTarget.value }))}
                                >
                                  <option value="">{t('tasks.assign.select')}</option>
                                  {usersQuery.data.users.filter((user) => user.status === 'ACTIVE').map((user) => (
                                    <option key={user.userId} value={user.userId}>{user.displayName}</option>
                                  ))}
                                </select>
                              </label>
                              <button
                                className="button button--secondary"
                                type="button"
                                disabled={!assignTargets[task.id] || assignMutation.isPending}
                                onClick={() => assignMutation.mutate({ task, assigneeUserId: assignTargets[task.id] })}
                              >
                                {t('tasks.assign.confirm')}
                              </button>
                            </span>
                          ) : null}
                          <button className="button button--quiet" type="button" onClick={() => setHistoryTask(task)}>
                            <History size={14} aria-hidden="true" /> {t('tasks.history.open')}
                          </button>
                        </div>
                      </td>
                    </tr>
                  ))}</tbody>
                </table>
              </div>
            ) : <EmptyState icon={CheckCircle2} variant="inline" title={hasFilteredOutTasks ? t('tasks.noMatches') : t('tasks.list.empty')} />}
          </section>
        </>
      ) : null}

      {action ? (
        <TaskActionDialog
          task={action.task}
          targetStatus={action.target}
          onClose={() => setAction(null)}
          onDone={() => { void refresh(); }}
        />
      ) : null}
      {historyTask ? <TaskHistoryDialog task={historyTask} onClose={() => setHistoryTask(null)} /> : null}
    </section>
  );
};
