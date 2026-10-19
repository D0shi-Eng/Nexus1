import { useState, type FormEvent } from 'react';
import { CheckCircle2, CircleAlert, ClipboardList, LoaderCircle, Plus, X } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';

import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { useModalFocus } from '../../lib/use-modal-focus';
import { useSession } from '../../lib/use-session';

// لوحة المهارات: تعرض السجل القائم كما هو (هوية/نسخ/تعيينات) — المهارة
// تعليمة إصدارية غير تنفيذية وليست صلاحية ولا أداة (§27).
const SkillDialog = ({
  onClose,
  onSaved,
}: {
  readonly onClose: () => void;
  readonly onSaved: () => void;
}) => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const [form, setForm] = useState({ name: '', description: '', instruction: '', inputContract: '', outputContract: '', constraints: '' });
  const [validationError, setValidationError] = useState(false);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const createMutation = useMutation({ mutationFn: nexusApi.createSkill });

  const update = (field: keyof typeof form, value: string): void => {
    setForm((previous) => ({ ...previous, [field]: value }));
  };

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setValidationError(false);
    setErrorCode(null);
    if (!form.name.trim() || !form.description.trim() || !form.instruction.trim()) {
      setValidationError(true);
      return;
    }
    try {
      await createMutation.mutateAsync({
        name: form.name.trim(),
        description: form.description.trim(),
        instruction: form.instruction.trim(),
        inputContract: form.inputContract.trim() || 'free text',
        outputContract: form.outputContract.trim() || 'bounded analysis',
        constraints: form.constraints.trim() || 'no side effects',
      });
      await queryClient.invalidateQueries({ queryKey: ['skill-catalog'] });
      onSaved();
    } catch (error) {
      setErrorCode(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="skill-dialog-title" tabIndex={-1}>
        <header className="task-dialog__header">
          <div><h2 id="skill-dialog-title">{t('agentsSkills.dialog.title')}</h2><p>{t('agentsSkills.dialog.subtitle')}</p></div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}><X size={18} aria-hidden="true" /></button>
        </header>
        <form className="task-form" onSubmit={(event) => { void submit(event); }} noValidate>
          <label><span>{t('agentsSkills.dialog.name')}</span>
            <input value={form.name} onChange={(event) => update('name', event.currentTarget.value)} maxLength={160} required /></label>
          <label><span>{t('agentsSkills.dialog.description')}</span>
            <textarea value={form.description} onChange={(event) => update('description', event.currentTarget.value)} maxLength={2_000} rows={2} required /></label>
          <label><span>{t('agentsSkills.dialog.instruction')}</span>
            <textarea value={form.instruction} onChange={(event) => update('instruction', event.currentTarget.value)} maxLength={2_048} rows={3} required /></label>
          {validationError ? <p className="form-error" role="alert">{t('agentsSkills.dialog.validation')}</p> : null}
          {errorCode ? <p className="form-error" role="alert">{t(`errors.${errorCode}`, { defaultValue: t('agentsSkills.dialog.failure') })}</p> : null}
          <footer className="task-form__actions">
            <button className="button button--secondary" type="button" onClick={onClose} disabled={createMutation.isPending}>{t('actions.cancel')}</button>
            <button className="button button--primary" type="submit" disabled={createMutation.isPending}>
              {createMutation.isPending ? <LoaderCircle className="spin" size={16} aria-hidden="true" /> : <CheckCircle2 size={16} aria-hidden="true" />}
              {t('agentsSkills.dialog.create')}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
};

export const AgentSkillsPanel = () => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { can } = useSession();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [assigningSkill, setAssigningSkill] = useState<string | null>(null);
  const [assignAgentId, setAssignAgentId] = useState('');
  const [assignVersionId, setAssignVersionId] = useState('');

  const catalogQuery = useQuery({ queryKey: ['skill-catalog'], queryFn: nexusApi.getSkillCatalog, staleTime: 30_000 });
  const agentsQuery = useQuery({ queryKey: ['agents'], queryFn: nexusApi.getAgents, staleTime: 30_000 });
  const invalidate = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ['skill-catalog'] });
    await queryClient.invalidateQueries({ queryKey: ['agents'] });
  };
  const mutate = async (action: () => Promise<void>): Promise<void> => {
    setActionError(null);
    try {
      await action();
      await invalidate();
    } catch (error) {
      setActionError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };
  const disableMutation = useMutation({ mutationFn: nexusApi.disableSkill });
  const assignMutation = useMutation({ mutationFn: nexusApi.assignSkill });
  const unassignMutation = useMutation({ mutationFn: nexusApi.unassignSkill });

  const errorCode = resolveErrorCode(catalogQuery.error);
  const canManage = can('skill:create') || can('skill:update') || can('skill:assign');

  const submitAssign = async (): Promise<void> => {
    if (!assigningSkill || !assignAgentId || !assignVersionId) {
      return;
    }
    await mutate(async () => {
      await assignMutation.mutateAsync({ agentId: assignAgentId, skillVersionId: assignVersionId });
      setAssigningSkill(null);
      setAssignAgentId('');
      setAssignVersionId('');
    });
  };

  return (
    <section className="panel panel--table" aria-labelledby="skills-panel-title">
      <div className="panel__header">
        <div><h2 id="skills-panel-title">{t('agentsSkills.title')}</h2><span>{catalogQuery.data?.items.length ?? 0}</span></div>
        {can('skill:create') ? (
          <button className="button button--primary" type="button" onClick={() => setDialogOpen(true)}>
            <Plus size={16} aria-hidden="true" /> {t('agentsSkills.actions.create')}
          </button>
        ) : null}
      </div>
      <p className="form-note">{t('agentsSkills.boundary')}</p>
      {catalogQuery.isLoading ? <LoadingState label={t('agentsSkills.loading')} /> : null}
      {catalogQuery.isError ? <ErrorState code={errorCode} deniedMessage={t('agentsSkills.permissionDenied')} /> : null}
      {catalogQuery.isError && errorCode !== 'AUTHORIZATION_DENIED' ? <div className="error-panel" role="alert"><CircleAlert size={20} aria-hidden="true" /><p>{t(`errors.${errorCode}`, { defaultValue: t('errors.unavailable') })}</p></div> : null}
      {actionError ? <div className="error-panel" role="alert"><CircleAlert size={20} aria-hidden="true" /><p>{t(`errors.${actionError}`, { defaultValue: t('agentsSkills.dialog.failure') })}</p></div> : null}
      {catalogQuery.data && !catalogQuery.data.hasData ? <EmptyState icon={ClipboardList} title={t('agentsSkills.empty')} /> : null}

      {catalogQuery.data?.hasData ? (
        <div className="table-scroll">
          <table>
            <thead><tr>
              <th>{t('agentsSkills.table.name')}</th>
              <th>{t('agentsSkills.table.status')}</th>
              <th>{t('agentsSkills.table.versions')}</th>
              <th>{t('agentsSkills.table.assignments')}</th>
              {canManage ? <th><span className="visually-hidden">{t('table.action')}</span></th> : null}
            </tr></thead>
            <tbody>
              {catalogQuery.data.items.map((item) => (
                <tr key={item.skill.id}>
                  <td><strong>{item.skill.name}</strong><br /><span>{item.skill.description}</span></td>
                  <td><span className={`badge badge--${item.skill.status.toLowerCase()}`}>{t(`status.${item.skill.status}`)}</span></td>
                  <td>
                    <ul className="assignment-list">
                      {item.versions.map((version) => (
                        <li key={version.id}><code>v{version.versionNumber}</code></li>
                      ))}
                    </ul>
                  </td>
                  <td>
                    <ul className="assignment-list">
                      {item.assignments.length === 0 ? <li>—</li> : null}
                      {item.assignments.map((assignment) => (
                        <li key={assignment.id}>
                          <code dir="ltr">{assignment.agentId}</code>
                          {' · '}
                          {item.versions.find((version) => version.id === assignment.skillVersionId)?.versionNumber}
                          {' · '}
                          {assignment.unassignedAt ? t('agentsSkills.unassigned') : t('agentsSkills.assigned')}
                          {assignment.unassignedAt === null && can('skill:assign') ? (
                            <button
                              className="button button--quiet"
                              type="button"
                              disabled={unassignMutation.isPending}
                              onClick={() => { void mutate(async () => { await unassignMutation.mutateAsync({ assignmentId: assignment.id }); }); }}
                            >
                              {t('agentsSkills.actions.unassign')}
                            </button>
                          ) : null}
                        </li>
                      ))}
                    </ul>
                  </td>
                  {canManage ? (
                    <td>
                      <div className="table-actions">
                        {can('skill:assign') ? (
                          <button className="button button--quiet" type="button" onClick={() => { setAssigningSkill(item.skill.id); setAssignVersionId(item.versions[0]?.id ?? ''); }}>
                            {t('agentsSkills.actions.assign')}
                          </button>
                        ) : null}
                        {can('skill:update') && item.skill.status === 'ACTIVE' ? (
                          <button
                            className="button button--quiet"
                            type="button"
                            disabled={disableMutation.isPending}
                            onClick={() => { void mutate(async () => { await disableMutation.mutateAsync({ skillId: item.skill.id }); }); }}
                          >
                            {t('agentsSkills.actions.disable')}
                          </button>
                        ) : null}
                      </div>
                    </td>
                  ) : null}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}

      {assigningSkill ? (
        <form
          className="task-form"
          onSubmit={(event) => { event.preventDefault(); void submitAssign(); }}
          aria-label={t('agentsSkills.assignTitle')}
        >
          <h3>{t('agentsSkills.assignTitle')}</h3>
          <label>
            <span>{t('agentsSkills.assignAgent')}</span>
            <select value={assignAgentId} onChange={(event) => setAssignAgentId(event.currentTarget.value)} required>
              <option value="">—</option>
              {(agentsQuery.data?.items ?? []).filter((agent) => agent.status === 'ACTIVE').map((agent) => (
                <option key={agent.id} value={agent.id}>{agent.name}</option>
              ))}
            </select>
          </label>
          <label>
            <span>{t('agentsSkills.assignVersion')}</span>
            <select value={assignVersionId} onChange={(event) => setAssignVersionId(event.currentTarget.value)} required>
              {(catalogQuery.data?.items ?? []).find((item) => item.skill.id === assigningSkill)?.versions.map((version) => (
                <option key={version.id} value={version.id}>{`v${version.versionNumber}`}</option>
              ))}
            </select>
          </label>
          <footer className="task-form__actions">
            <button className="button button--secondary" type="button" onClick={() => setAssigningSkill(null)}>{t('actions.cancel')}</button>
            <button className="button button--primary" type="submit" disabled={assignMutation.isPending}>{t('agentsSkills.actions.assign')}</button>
          </footer>
        </form>
      ) : null}

      {dialogOpen ? <SkillDialog onClose={() => setDialogOpen(false)} onSaved={() => setDialogOpen(false)} /> : null}
    </section>
  );
};
