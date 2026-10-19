import { useMemo, useState, type FormEvent } from 'react';
import { CircleAlert, LoaderCircle, Play, X } from 'lucide-react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import type { AgentRecord } from '@nexus/contracts';

import { NexusApiError, nexusApi } from '../../lib/nexus-api';
import { useModalFocus } from '../../lib/use-modal-focus';
import { useSession } from '../../lib/use-session';

// حوار «تشغيل الوكيل» — حالة الاستخدام العامة الواحدة (§68):
// يعرض حالة البروفايل والنموذج والمهارة قبل السماح بالإدخال، ولا يخفي
// سبب عدم الجاهزية. الإدخال مهمة المستخدم فقط (محتوى غير موثوق أدنى سلطةً)
// واختيار مهارة اختياري — كل قرار أمني آخر مصدره الخلف.
export const RunAgentDialog = ({
  agent,
  onClose,
}: {
  readonly agent: AgentRecord;
  readonly onClose: () => void;
}) => {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { can } = useSession();
  const { dialogRef, initialFocusRef } = useModalFocus(onClose);
  const [taskContent, setTaskContent] = useState('');
  const [skillVersionId, setSkillVersionId] = useState('');
  const [knowledgeQuery, setKnowledgeQuery] = useState('');
  const [newInstruction, setNewInstruction] = useState('');
  const [validationError, setValidationError] = useState(false);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const [requestedRun, setRequestedRun] = useState<{ id: string; status: string } | null>(null);

  const profilesQuery = useQuery({
    queryKey: ['agent-execution-profiles', agent.id],
    queryFn: () => nexusApi.getAgentExecutionProfiles(agent.id),
    staleTime: 15_000,
  });
  const integrationQuery = useQuery({
    queryKey: ['integration-status'],
    queryFn: nexusApi.getIntegrationStatus,
    staleTime: 30_000,
  });
  const skillsQuery = useQuery({
    queryKey: ['skill-catalog'],
    queryFn: nexusApi.getSkillCatalog,
    staleTime: 30_000,
  });

  const latestProfileId = profilesQuery.data?.latestVersionId ?? null;
  const configuredModel = integrationQuery.data?.enabledLocalModels[0] ?? null;
  const agentSkills = useMemo(() => {
    const assignments = (skillsQuery.data?.items ?? []).flatMap((item) =>
      item.assignments
        .filter((assignment) => assignment.agentId === agent.id && assignment.unassignedAt === null)
        .map((assignment) => ({
          assignmentId: assignment.id,
          versionId: assignment.skillVersionId,
          skillName: item.skill.name,
          versionNumber: item.versions.find((version) => version.id === assignment.skillVersionId)?.versionNumber ?? 0,
          active: item.skill.status === 'ACTIVE',
        })),
    );
    // المهارة الصالحة للاختيار: نشطة ونسختها الدقيقة معينة لهذا الوكيل.
    return assignments.filter((assignment) => assignment.active && assignment.versionNumber > 0);
  }, [skillsQuery.data?.items, agent.id]);

  const createProfileMutation = useMutation({ mutationFn: nexusApi.createAgentExecutionProfile });
  const requestMutation = useMutation({
    mutationFn: nexusApi.requestAgentExecution,
    onSuccess: (result) => {
      setErrorCode(null);
      setRequestedRun({ id: result.run.id, status: result.run.status });
      void queryClient.invalidateQueries({ queryKey: ['agent-runs'] });
      void queryClient.invalidateQueries({ queryKey: ['attention'] });
    },
    onError: (error) => {
      setErrorCode(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    },
  });

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    setValidationError(false);
    setErrorCode(null);
    const trimmed = taskContent.trim();
    if (!trimmed || trimmed.length > 2_048) {
      setValidationError(true);
      return;
    }
    try {
      await requestMutation.mutateAsync({
        agentId: agent.id,
        taskContent: trimmed,
        skillVersionId: skillVersionId || null,
        knowledgeQuery: knowledgeQuery.trim() || null,
      });
    } catch {
      // الكود يُعرض عبر errorCode — لا تفاصيل خام تصل الواجهة.
    }
  };

  const createProfile = async (): Promise<void> => {
    setErrorCode(null);
    const trimmed = newInstruction.trim();
    if (!trimmed || trimmed.length > 2_048) {
      setErrorCode('VALIDATION_FAILED');
      return;
    }
    try {
      await createProfileMutation.mutateAsync({ agentId: agent.id, instruction: trimmed });
      await queryClient.invalidateQueries({ queryKey: ['agent-execution-profiles', agent.id] });
      setNewInstruction('');
    } catch (error) {
      setErrorCode(error instanceof NexusApiError ? error.code : 'LOCAL_SERVICE_UNAVAILABLE');
    }
  };

  const readyToRun = latestProfileId !== null && configuredModel !== null && agent.status === 'ACTIVE';

  return (
    <div className="dialog-layer" role="presentation">
      <button className="dialog-backdrop" type="button" onClick={onClose} aria-label={t('actions.close')} />
      <section
        ref={dialogRef}
        className="task-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="run-agent-dialog-title"
        tabIndex={-1}
      >
        <header className="task-dialog__header">
          <div>
            <h2 id="run-agent-dialog-title">{t('runDialog.title')}</h2>
            <p>{t('runDialog.subtitle', { agent: agent.name })}</p>
          </div>
          <button ref={initialFocusRef} className="icon-button" type="button" onClick={onClose} aria-label={t('actions.close')}>
            <X size={18} aria-hidden="true" />
          </button>
        </header>

        {requestedRun ? (
          <div className="run-request-state">
            <dl className="details-list">
              <div><dt>{t('runDialog.requestedRun')}</dt><dd><code>{requestedRun.id}</code></dd></div>
              <div><dt>{t('agentRuns.table.status')}</dt><dd>{t(`agentRuns.status.${requestedRun.status}`)}</dd></div>
            </dl>
            <p className="form-note">{t('runDialog.waitingApprovalNote')}</p>
            <footer className="task-form__actions">
              <button className="button button--primary" type="button" onClick={onClose}>{t('actions.close')}</button>
            </footer>
          </div>
        ) : (
          <>
            <dl className="details-list">
              <div>
                <dt>{t('runDialog.profileStatus')}</dt>
                <dd>
                  {profilesQuery.isLoading
                    ? t('runDialog.loading')
                    : latestProfileId
                      ? t('runDialog.profileConfigured')
                      : t('runDialog.profileMissing')}
                </dd>
              </div>
              <div>
                <dt>{t('runDialog.model')}</dt>
                <dd>
                  {integrationQuery.isLoading
                    ? t('runDialog.loading')
                    : configuredModel
                      ? configuredModel
                      : t('runDialog.modelNotConfigured')}
                </dd>
              </div>
              <div>
                <dt>{t('runDialog.skill')}</dt>
                <dd>
                  <label className="visually-hidden" htmlFor="run-agent-skill">{t('runDialog.skill')}</label>
                  <select
                    id="run-agent-skill"
                    value={skillVersionId}
                    onChange={(event) => setSkillVersionId(event.currentTarget.value)}
                  >
                    <option value="">{t('runDialog.skillNone')}</option>
                    {agentSkills.map((skill) => (
                      <option key={skill.versionId} value={skill.versionId}>
                        {`${skill.skillName} v${skill.versionNumber}`}
                      </option>
                    ))}
                  </select>
                </dd>
              </div>
            </dl>

            {!latestProfileId && can('agent:update') ? (
              <label>
                <span>{t('runDialog.profileInstruction')}</span>
                <textarea
                  value={newInstruction}
                  onChange={(event) => setNewInstruction(event.currentTarget.value)}
                  maxLength={2_048}
                  rows={3}
                />
                <button
                  className="button button--secondary"
                  type="button"
                  onClick={() => { void createProfile(); }}
                  disabled={createProfileMutation.isPending}
                >
                  {createProfileMutation.isPending ? <LoaderCircle className="spin" size={16} aria-hidden="true" /> : null}
                  {t('runDialog.profileCreate')}
                </button>
              </label>
            ) : null}

            <form className="task-form" onSubmit={(event) => { void submit(event); }} noValidate>
              <label>
                <span>{t('agentsDialog.knowledgeQuery')}</span>
                <input
                  value={knowledgeQuery}
                  onChange={(event) => setKnowledgeQuery(event.currentTarget.value)}
                  maxLength={500}
                  disabled={!readyToRun}
                />
                <small className="form-note">{t('agentsDialog.knowledgeQueryHint')}</small>
              </label>
              <label>
                <span>{t('runDialog.task')}</span>
                <textarea
                  value={taskContent}
                  onChange={(event) => setTaskContent(event.currentTarget.value)}
                  maxLength={2_048}
                  rows={4}
                  required
                  aria-invalid={validationError && !taskContent.trim()}
                  disabled={!readyToRun}
                />
              </label>
              {validationError ? <p className="form-error" role="alert">{t('runDialog.validationTask')}</p> : null}
              {!readyToRun ? <p className="form-note" role="status">{t('runDialog.notReady')}</p> : null}
              {errorCode ? <p className="form-error" role="alert">{t(`errors.${errorCode}`, { defaultValue: t('runDialog.failure') })}</p> : null}
              {errorCode === 'STALE_APPROVAL' ? <p className="form-note">{t('runDialog.staleNote')}</p> : null}
              <footer className="task-form__actions">
                <button className="button button--secondary" type="button" onClick={onClose}>{t('actions.cancel')}</button>
                <button className="button button--primary" type="submit" disabled={!readyToRun || requestMutation.isPending}>
                  {requestMutation.isPending ? <LoaderCircle className="spin" size={16} aria-hidden="true" /> : <Play size={16} aria-hidden="true" />}
                  {t('runDialog.submit')}
                </button>
              </footer>
            </form>
            <p className="form-note"><CircleAlert size={14} aria-hidden="true" /> {t('runDialog.approvalNote')}</p>
          </>
        )}
      </section>
    </div>
  );
};
