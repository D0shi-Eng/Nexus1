import { useState, type FormEvent } from 'react';
import { CheckCircle2, ClipboardList, CircleAlert, LoaderCircle, Play, Plus, X, XCircle } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { DelegationWithVerification } from '@nexus/contracts';

import { EmptyState, LoadingState } from '../../components/States';
import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { useModalFocus } from '../../lib/use-modal-focus';
import { useSession } from '../../lib/use-session';
import { formatDate } from '../../lib/format';

// لوحة الخطط: إنشاء محدود بحد الهدف، موافقة مراجع مستقل، تنفيذ محكوم،
// وعرض يحفظ الاقتباسات والتعارضات والمجهولات كما سجلها الخلف (C-13).
const CreatePlanDialog = ({ onClose }: { readonly onClose: () => void }) => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const [goal, setGoal] = useState('');
  const [validationError, setValidationError] = useState(false);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const createMutation = useMutation({ mutationFn: nexusApi.createPlan });

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setValidationError(false);
    setErrorCode(null);
    if (!goal.trim() || goal.length > 500) {
      setValidationError(true);
      return;
    }
    try {
      await createMutation.mutateAsync({ goal: goal.trim() });
      await queryClient.invalidateQueries({ queryKey: ['plans'] });
      onClose();
    } catch (error) {
      setErrorCode(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section ref={dialogRef} className="task-dialog" role="dialog" aria-modal="true" aria-labelledby="plan-create-title" tabIndex={-1}>
        <header className="task-dialog__header">
          <div>
            <h2 id="plan-create-title">{t('plans.createTitle')}</h2>
            <p>{t('plans.createSubtitle')}</p>
          </div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}>
            <X size={18} aria-hidden="true" />
          </button>
        </header>
        <form className="task-form" onSubmit={(event) => { void submit(event); }} noValidate>
          <label>
            <span>{t('plans.createGoalField')}</span>
            <textarea value={goal} onChange={(event) => setGoal(event.currentTarget.value)} rows={3} maxLength={500} required aria-invalid={validationError && !goal.trim()} />
          </label>
          {validationError ? <p className="form-error" role="alert">{t('agents.dialog.validation')}</p> : null}
          {errorCode ? <p className="form-error" role="alert">{t(`errors.${errorCode}`, { defaultValue: t('errors.unavailable') })}</p> : null}
          <footer className="task-form__actions">
            <button className="button button--secondary" type="button" onClick={onClose} disabled={createMutation.isPending}>{t('actions.cancel')}</button>
            <button className="button button--primary" type="submit" disabled={createMutation.isPending}>
              {createMutation.isPending ? <LoaderCircle className="spin" size={16} aria-hidden="true" /> : <ClipboardList size={16} aria-hidden="true" />}
              {t('plans.createSubmit')}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
};

const DelegationRow = ({ entry }: { readonly entry: DelegationWithVerification }) => {
  const { t } = useTranslation();
  const { delegation, verification } = entry;
  return (
    <article className="knowledge-result">
      <div className="table-primary">
        <span><bdi>{delegation.targetAgentId}</bdi> · {t(`plans.statusLabel.${delegation.status}`, { defaultValue: delegation.status })}</span>
        <small>
          {t('plans.delegation.truth')}: {delegation.truthClass ? t(`truthClass.${delegation.truthClass}`, { defaultValue: delegation.truthClass }) : '—'} · {t('plans.verificationTitle')}: {t(`plans.verificationOutcome.${verification ? verification.outcome : 'NOT_VERIFIED'}`, { defaultValue: verification ? verification.outcome : 'NOT_VERIFIED' })}
          {delegation.failureCode ? ` · ${t('plans.delegation.failure')}: ${t(`errors.${delegation.failureCode}`, { defaultValue: delegation.failureCode })}` : ''}
        </small>
      </div>
      {delegation.output ? <p>{delegation.output}</p> : null}
      {delegation.citationsJson ? <p className="form-note">{t('plans.delegation.citations')}: {delegation.citationsJson}</p> : null}
    </article>
  );
};

export const AgentPlansPanel = () => {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const { can } = useSession();
  const language = i18n.resolvedLanguage ?? 'ar';
  const [createOpen, setCreateOpen] = useState(false);
  const [selectedPlanId, setSelectedPlanId] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [mutationError, setMutationError] = useState<string | null>(null);

  const plansQuery = useQuery({ queryKey: ['plans'], queryFn: nexusApi.getPlans, staleTime: 15_000 });
  const detailQuery = useQuery({
    queryKey: ['plan', selectedPlanId],
    queryFn: () => nexusApi.getPlan(selectedPlanId!),
    enabled: Boolean(selectedPlanId),
    staleTime: 5_000,
  });

  const executeMutation = useMutation({ mutationFn: nexusApi.startPlanExecution });
  const cancelMutation = useMutation({ mutationFn: nexusApi.cancelPlan });
  const decisionMutation = useMutation({ mutationFn: nexusApi.decideApproval });

  const refresh = async (): Promise<void> => {
    await queryClient.invalidateQueries({ queryKey: ['plans'] });
    if (selectedPlanId) {
      await queryClient.invalidateQueries({ queryKey: ['plan', selectedPlanId] });
    }
  };

  const handle = async (action: () => Promise<unknown>, successKey: string): Promise<void> => {
    setFeedback(null);
    setMutationError(null);
    try {
      await action();
      await refresh();
      setFeedback(t(successKey));
    } catch (error) {
      setMutationError(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  const plans = plansQuery.data?.items ?? [];
  const detail = selectedPlanId ? detailQuery.data : undefined;

  return (
    <section className="panel panel--table" aria-labelledby="plans-list-title">
      <div className="panel__header">
        <div><h2 id="plans-list-title">{t('plans.listTitle')}</h2><span>{plans.length}</span></div>
        {can('plan:create') ? (
          <button className="button button--primary" type="button" onClick={() => setCreateOpen(true)}>
            <Plus size={16} aria-hidden="true" /> {t('plans.create')}
          </button>
        ) : null}
      </div>

      {feedback ? <p className="operation-feedback" role="status">{feedback}</p> : null}
      {mutationError ? <div className="error-panel" role="alert"><CircleAlert size={20} aria-hidden="true" /><p>{t(`errors.${mutationError}`, { defaultValue: t('errors.unavailable') })}</p></div> : null}

      {plansQuery.isLoading ? <LoadingState label={t('plans.loading')} /> : null}
      {plans.length === 0 && !plansQuery.isLoading && !plansQuery.isError ? (
        <EmptyState icon={ClipboardList} title={t('plans.empty')} />
      ) : null}

      {plans.length > 0 ? (
        <div className="table-scroll">
          <table>
            <thead><tr><th>{t('plans.table.goal')}</th><th>{t('plans.table.status')}</th><th>{t('plans.table.created')}</th><th>{t('plans.table.conflict')}</th><th>{t('plans.table.actions')}</th></tr></thead>
            <tbody>{plans.map((plan) => <tr key={plan.id}>
              <td><div className="table-primary"><span>{plan.goal}</span></div></td>
              <td>{t(`plans.statusLabel.${plan.status}`, { defaultValue: plan.status })}</td>
              <td><time dateTime={plan.createdAt}>{formatDate(plan.createdAt, language)}</time></td>
              <td>{plan.conflictFlag ? t('plans.conflictFlag') : '—'}</td>
              <td>
                <div className="table-actions">
                  <button className="button button--quiet" type="button" onClick={() => { setSelectedPlanId(plan.id); void detailQuery.refetch(); }}>
                    {t('plans.detailsTitle')}
                  </button>
                  {plan.status === 'ACTIVE' && can('plan:execute') ? (
                    <button className="button button--quiet" type="button" onClick={() => { void handle(() => executeMutation.mutateAsync({ planId: plan.id }).then(() => setSelectedPlanId(plan.id)), 'plans.executedSuccess'); }} disabled={executeMutation.isPending}>
                      {executeMutation.isPending ? <LoaderCircle className="spin" size={16} aria-hidden="true" /> : <Play size={16} aria-hidden="true" />}
                      {executeMutation.isPending ? t('plans.executing') : t('plans.execute')}
                    </button>
                  ) : null}
                  {plan.status === 'PENDING_APPROVAL' || plan.status === 'ACTIVE' ? (
                    <button className="button button--quiet" type="button" onClick={() => { void handle(() => cancelMutation.mutateAsync({ planId: plan.id }), 'plans.cancelledSuccess'); }} disabled={cancelMutation.isPending}>
                      <XCircle size={16} aria-hidden="true" /> {t('plans.cancel')}
                    </button>
                  ) : null}
                </div>
              </td>
            </tr>)}</tbody>
          </table>
        </div>
      ) : null}

      {selectedPlanId && detail ? (
        <div className="plan-detail">
          <div className="panel__header">
            <div><h3>{t('plans.detailsTitle')}</h3><span>{detail.plan.goal}</span></div>
            <button className="icon-button" type="button" onClick={() => setSelectedPlanId(null)} aria-label={t('actions.close')}><X size={16} aria-hidden="true" /></button>
          </div>
          <p className="form-note">{t('plans.modelNote')}</p>

          <h4>{t('plans.stepsTitle')}</h4>
          <div className="table-scroll">
            <table>
              <thead><tr><th>{t('plans.stepTable.index')}</th><th>{t('plans.stepTable.objective')}</th><th>{t('plans.stepTable.capability')}</th><th>{t('plans.stepTable.knowledge')}</th><th>{t('plans.stepTable.status')}</th></tr></thead>
              <tbody>{detail.steps.map((step) => <tr key={step.id}>
                <td>{step.stepIndex + 1}</td>
                <td>{step.objective}</td>
                <td>{step.requiredCapability}</td>
                <td>{step.requiresKnowledge ? t('actions.search') : '—'}</td>
                <td>{t(`plans.statusLabel.${step.status}`, { defaultValue: step.status })}</td>
              </tr>)}</tbody>
            </table>
          </div>

          <h4>{t('plans.delegationsTitle')}</h4>
          {detail.delegations.length > 0 ? detail.delegations.map((entry) => (
            <DelegationRow key={entry.delegation.id} entry={entry} />
          )) : <p className="form-note">{t('plans.empty')}</p>}

          {detail.plan.synthesisOutput ? (
            <>
              <h4>{t('plans.synthesisTitle')}</h4>
              <pre className="plan-synthesis">{detail.plan.synthesisOutput}</pre>
              <p className="form-note">{detail.plan.conflictFlag ? t('plans.conflictFlag') : t('plans.noConflictFlag')}</p>
            </>
          ) : null}

          {detail.pendingApprovalId && can('approval:decide') ? (
            <div className="table-actions">
              <button className="button button--primary" type="button" onClick={() => { void handle(() => decisionMutation.mutateAsync({ approvalId: detail.pendingApprovalId ?? '', decision: 'APPROVED', reason: t('plans.approveReason') }), 'plans.decisionSuccess'); }} disabled={decisionMutation.isPending}>
                <CheckCircle2 size={16} aria-hidden="true" /> {t('plans.approve')}
              </button>
              <button className="button button--secondary" type="button" onClick={() => { void handle(() => decisionMutation.mutateAsync({ approvalId: detail.pendingApprovalId ?? '', decision: 'REJECTED', reason: t('plans.rejectReason') }), 'plans.decisionSuccess'); }} disabled={decisionMutation.isPending}>
                <XCircle size={16} aria-hidden="true" /> {t('plans.reject')}
              </button>
            </div>
          ) : null}
        </div>
      ) : null}

      {createOpen ? <CreatePlanDialog onClose={() => setCreateOpen(false)} /> : null}
    </section>
  );
};
