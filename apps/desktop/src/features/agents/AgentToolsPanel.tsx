import { useState, type FormEvent } from 'react';
import { CircleAlert, LoaderCircle, Plus, Wrench, X } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { ToolEffectProfile } from '@nexus/contracts';

import { EmptyState, ErrorState, LoadingState } from '../../components/States';
import { resolveErrorCode } from '../../lib/api-error';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { useModalFocus } from '../../lib/use-modal-focus';
import { useSession } from '../../lib/use-session';

// لوحة الأدوات: تمييز صادق بين REGISTERED_ONLY وEXECUTABLE_TRUSTED_ADAPTER (§70) —
// تسجيل أداة من المستأجر لا يعني قابلية التنفيذ؛ التنفيذ يتطلب ربط محوّل موثوق
// خلفياً، ولا يُعرض أي مسار تنفيذ داخلي.
const emptyContract = { fields: [], allowsAdditionalFields: false };
const readOnlyEffect: ToolEffectProfile = { mutation: 'NONE', externality: 'INTERNAL', privilege: 'STANDARD', reversibility: 'NOT_APPLICABLE' };

const ToolDialog = ({
  onClose,
  onSaved,
}: {
  readonly onClose: () => void;
  readonly onSaved: () => void;
}) => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [validationError, setValidationError] = useState(false);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const createMutation = useMutation({ mutationFn: nexusApi.createTool });

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setValidationError(false);
    setErrorCode(null);
    if (!name.trim() || !description.trim()) {
      setValidationError(true);
      return;
    }
    try {
      // أدوات المستأجر تُسجَّل بوصف غير تنفيذي وبأثر القراءة الصامت الافتراضي —
      // بلا ربطية محوّل تبقى REGISTERED_ONLY ولا تُنفَّذ أبداً.
      await createMutation.mutateAsync({
        name: name.trim(),
        description: description.trim(),
        inputContract: emptyContract,
        outputContract: emptyContract,
        effect: readOnlyEffect,
      });
      await queryClient.invalidateQueries({ queryKey: ['tool-catalog'] });
      onSaved();
    } catch (error) {
      setErrorCode(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="tool-dialog-title" tabIndex={-1}>
        <header className="task-dialog__header">
          <div><h2 id="tool-dialog-title">{t('agentsTools.dialog.title')}</h2><p>{t('agentsTools.dialog.subtitle')}</p></div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}><X size={18} aria-hidden="true" /></button>
        </header>
        <form className="task-form" onSubmit={(event) => { void submit(event); }} noValidate>
          <label><span>{t('agentsTools.dialog.name')}</span>
            <input value={name} onChange={(event) => setName(event.currentTarget.value)} maxLength={160} required /></label>
          <label><span>{t('agentsTools.dialog.description')}</span>
            <textarea value={description} onChange={(event) => setDescription(event.currentTarget.value)} maxLength={2_000} rows={3} required /></label>
          {validationError ? <p className="form-error" role="alert">{t('agentsTools.dialog.validation')}</p> : null}
          {errorCode ? <p className="form-error" role="alert">{t(`errors.${errorCode}`, { defaultValue: t('agentsTools.dialog.failure') })}</p> : null}
          <footer className="task-form__actions">
            <button className="button button--secondary" type="button" onClick={onClose} disabled={createMutation.isPending}>{t('actions.cancel')}</button>
            <button className="button button--primary" type="submit" disabled={createMutation.isPending}>
              {createMutation.isPending ? <LoaderCircle className="spin" size={16} aria-hidden="true" /> : <Plus size={16} aria-hidden="true" />}
              {t('agentsTools.dialog.create')}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
};

export const AgentToolsPanel = () => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { can } = useSession();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [assigningTool, setAssigningTool] = useState<{ toolId: string; versionId: string } | null>(null);
  const [assignAgentId, setAssignAgentId] = useState('');

  const catalogQuery = useQuery({ queryKey: ['tool-catalog'], queryFn: nexusApi.getToolCatalog, staleTime: 30_000 });
  const agentsQuery = useQuery({ queryKey: ['agents'], queryFn: nexusApi.getAgents, staleTime: 30_000 });
  const invalidate = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ['tool-catalog'] });
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
  const disableMutation = useMutation({ mutationFn: nexusApi.disableTool });
  const assignMutation = useMutation({ mutationFn: nexusApi.assignTool });

  const errorCode = resolveErrorCode(catalogQuery.error);
  const canManage = can('tool:create') || can('tool:update') || can('tool:assign');

  const submitAssign = async (): Promise<void> => {
    if (!assigningTool || !assignAgentId) {
      return;
    }
    await mutate(async () => {
      await assignMutation.mutateAsync({ agentId: assignAgentId, toolVersionId: assigningTool.versionId });
      setAssigningTool(null);
      setAssignAgentId('');
    });
  };

  return (
    <section className="panel panel--table" aria-labelledby="tools-panel-title">
      <div className="panel__header">
        <div><h2 id="tools-panel-title">{t('agentsTools.title')}</h2><span>{catalogQuery.data?.items.length ?? 0}</span></div>
        {can('tool:create') ? (
          <button className="button button--primary" type="button" onClick={() => setDialogOpen(true)}>
            <Plus size={16} aria-hidden="true" /> {t('agentsTools.actions.create')}
          </button>
        ) : null}
      </div>
      <p className="form-note">{t('agentsTools.boundary')}</p>
      {catalogQuery.isLoading ? <LoadingState label={t('agentsTools.loading')} /> : null}
      {catalogQuery.isError ? <ErrorState code={errorCode} deniedMessage={t('agentsTools.permissionDenied')} /> : null}
      {actionError ? <div className="error-panel" role="alert"><CircleAlert size={20} aria-hidden="true" /><p>{t(`errors.${actionError}`, { defaultValue: t('agentsTools.dialog.failure') })}</p></div> : null}
      {catalogQuery.data && !catalogQuery.data.hasData ? <EmptyState icon={Wrench} title={t('agentsTools.empty')} /> : null}

      {catalogQuery.data?.hasData ? (
        <div className="table-scroll">
          <table>
            <thead><tr>
              <th>{t('agentsTools.table.name')}</th>
              <th>{t('agentsTools.table.status')}</th>
              <th>{t('agentsTools.table.versions')}</th>
              {canManage ? <th><span className="visually-hidden">{t('table.action')}</span></th> : null}
            </tr></thead>
            <tbody>
              {catalogQuery.data.items.map((item) => (
                <tr key={item.tool.id}>
                  <td><strong>{item.tool.name}</strong><br /><span>{item.tool.description}</span></td>
                  <td><span className={`badge badge--${item.tool.status.toLowerCase()}`}>{t(`status.${item.tool.status}`)}</span></td>
                  <td>
                    <ul className="assignment-list">
                      {item.versions.map((version) => (
                        <li key={version.id}>
                          <code>v{version.versionNumber}</code>
                          {' · '}
                          {t(`agentsTools.effect.${version.effect.mutation}`)}
                          {'/'}
                          {t(`agentsTools.effect.${version.effect.externality}`)}
                          {' · '}
                          {version.executable
                            ? <span className="badge badge--active">{t('agentsTools.executable')}</span>
                            : <span className="badge badge--disabled">{t('agentsTools.registeredOnly')}</span>}
                        </li>
                      ))}
                    </ul>
                  </td>
                  {canManage ? (
                    <td>
                      <div className="table-actions">
                        {can('tool:assign') && item.versions.length > 0 ? (
                          <button
                            className="button button--quiet"
                            type="button"
                            onClick={() => setAssigningTool({ toolId: item.tool.id, versionId: item.versions[0]!.id })}
                          >
                            {t('agentsTools.actions.assign')}
                          </button>
                        ) : null}
                        {can('tool:update') && item.tool.status === 'ACTIVE' ? (
                          <button
                            className="button button--quiet"
                            type="button"
                            disabled={disableMutation.isPending}
                            onClick={() => { void mutate(async () => { await disableMutation.mutateAsync({ toolId: item.tool.id }); }); }}
                          >
                            {t('agentsTools.actions.disable')}
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

      {assigningTool ? (
        <form
          className="task-form"
          onSubmit={(event) => { event.preventDefault(); void submitAssign(); }}
          aria-label={t('agentsTools.assignTitle')}
        >
          <h3>{t('agentsTools.assignTitle')}</h3>
          <p className="form-note">{t('agentsTools.assignNote')}</p>
          <label>
            <span>{t('agentsSkills.assignAgent')}</span>
            <select value={assignAgentId} onChange={(event) => setAssignAgentId(event.currentTarget.value)} required>
              <option value="">—</option>
              {(agentsQuery.data?.items ?? []).filter((agent) => agent.status === 'ACTIVE').map((agent) => (
                <option key={agent.id} value={agent.id}>{agent.name}</option>
              ))}
            </select>
          </label>
          <footer className="task-form__actions">
            <button className="button button--secondary" type="button" onClick={() => setAssigningTool(null)}>{t('actions.cancel')}</button>
            <button className="button button--primary" type="submit" disabled={assignMutation.isPending}>{t('agentsTools.actions.assign')}</button>
          </footer>
        </form>
      ) : null}

      {dialogOpen ? <ToolDialog onClose={() => setDialogOpen(false)} onSaved={() => setDialogOpen(false)} /> : null}
    </section>
  );
};
