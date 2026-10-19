use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    application::{
        authorization_service, delegation_execution_service, memory_service,
        planner_service::DelegationWithVerification,
        provider_runtime_service::ProviderRuntimeService, verifier_service,
    },
    domain::{
        error::AppError,
        plan::{
            DelegationStatus, PlanRecord, PlanStatus, PlanStepRecord, PlanStepStatus,
            MAX_DELEGATIONS_PER_PLAN, MAX_PROVIDER_CALLS_PER_PLAN,
        },
    },
    infrastructure::{
        agent_repository::AgentRepository,
        database::{append_audit_event, latest_audit_event_hash, AppState},
        plan_repository::{DelegationInsert, PlanRepository},
        verification_repository::VerificationRunRecord,
    },
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPlanExecutionInput {
    pub plan_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanExecutionResult {
    pub plan: PlanRecord,
    pub delegations: Vec<DelegationWithVerification>,
}

// المشرف (§63): يقبل خطة مُعتمدة، يفوّض الخطوات لوكلاء مؤهلين حتمياً، يجمع،
// يحقق، ثم يولّف نتيجة حتمية تحفظ الاقتباسات والتعارضات والمجهولات —
// لا يمنح صلاحيات ولا يتجاوز موافقة ولا يبدّل مستأجراً (C-11).
pub fn start_plan_execution(
    state: &AppState,
    input: StartPlanExecutionInput,
) -> Result<PlanExecutionResult, AppError> {
    let provider = ProviderRuntimeService::local_ollama();
    start_plan_execution_with_provider(state, input, &provider)
}

pub(crate) fn start_plan_execution_with_provider(
    state: &AppState,
    input: StartPlanExecutionInput,
    provider: &dyn crate::domain::provider::ModelProvider,
) -> Result<PlanExecutionResult, AppError> {
    authorization_service::require_permission(state, "PLAN", "EXECUTE")?;
    let plan_id = input.plan_id.trim();
    if plan_id.is_empty() || plan_id.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }

    // 0) استرداد الانهيار الحتمي: كل خطة عالقة RUNNING تُنهى فشلاً قبل أي
    //    عمل جديد — لا إعادة تشغيل صامتة لإجراء حساس (T11/C-11).
    recover_stale_running_plans(state)?;

    // 1) حجز الخطة: ACTIVE → RUNNING بشرط CAS كامل
    let plan = {
        let now = Utc::now().to_rfc3339();
        let connection = state
            .connection
            .lock()
            .map_err(|_| AppError::StorageOperation)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        let plan =
            PlanRepository::find_plan_in_transaction(&transaction, &state.tenant_id(), plan_id)?
                .ok_or(AppError::ResourceNotFound)?;
        if plan.status != PlanStatus::Active {
            return Err(AppError::InvalidStateTransition);
        }
        let changed = PlanRepository::transition_plan(
            &transaction,
            &state.tenant_id(),
            plan_id,
            PlanStatus::Active,
            plan.revision,
            PlanStatus::Running,
            plan.revision + 1,
            None,
            &now,
        )?;
        if !changed {
            return Err(AppError::Conflict);
        }
        append_plan_audit(&transaction, state, "PLAN_EXECUTION_STARTED", plan_id, &now)?;
        let reserved =
            PlanRepository::find_plan_in_transaction(&transaction, &state.tenant_id(), plan_id)?
                .ok_or(AppError::ResourceNotFound)?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
        reserved
    };

    let steps = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        PlanRepository::list_steps(&connection, &state.tenant_id(), plan_id)?
    };

    // 2) تنفيذ الخطوات تسلسلياً بتفويض واحد لكل خطوة
    let mut delegation_results: Vec<DelegationWithVerification> = Vec::new();
    let mut any_failed = false;
    let mut plan_failed = false;
    for step in &steps {
        if any_failed {
            transition_step(
                state,
                step,
                PlanStepStatus::Pending,
                PlanStepStatus::Cancelled,
            )?;
            continue;
        }
        let delegation_count = {
            let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
            PlanRepository::count_delegations_for_plan_on_connection(
                &connection,
                &state.tenant_id(),
                plan_id,
            )?
        };
        // الحد المزدوج: تفويضات صلبة + ميزانية نداءات مزود مشتقة (2 نداء/تفويض §69/C-11)
        let provider_budget_delegations =
            (MAX_PROVIDER_CALLS_PER_PLAN / 2).min(MAX_DELEGATIONS_PER_PLAN);
        if delegation_count >= provider_budget_delegations as i64 {
            return Err(AppError::DelegationLimitExceeded);
        }

        // اختيار حتمي: أقدم وكيل نشط بالقدرة داخل المستأجر — لا اختيار نموذج
        let agent = {
            let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
            let transaction = connection
                .unchecked_transaction()
                .map_err(|_| AppError::StorageOperation)?;
            let agent = AgentRepository::find_oldest_active_by_capability(
                &transaction,
                &state.tenant_id(),
                &step.required_capability,
            )?;
            transaction
                .commit()
                .map_err(|_| AppError::StorageOperation)?;
            agent
        };
        let Some(agent) = agent else {
            // قدرة بلا وكيل مؤهل — فشل مغلق للخطة كلها (لا تخطي صامت)
            transition_step(state, step, PlanStepStatus::Pending, PlanStepStatus::Failed)?;
            any_failed = true;
            plan_failed = true;
            continue;
        };

        // عقد التفويض المطبوع ثم CAS إلى RUNNING
        let delegation_id = format!("delegation_{}", Uuid::new_v4());
        let now = Utc::now().to_rfc3339();
        {
            let connection = state
                .connection
                .lock()
                .map_err(|_| AppError::StorageOperation)?;
            let transaction = connection
                .unchecked_transaction()
                .map_err(|_| AppError::StorageOperation)?;
            PlanRepository::insert_delegation(
                &transaction,
                &state.tenant_id(),
                &DelegationInsert {
                    id: &delegation_id,
                    plan_id,
                    plan_revision: plan.revision,
                    step_id: &step.id,
                    target_agent_id: &agent.id,
                    agent_run_id: None,
                    objective: &step.objective,
                    allowed_context_refs_json: &serde_json::to_string(&serde_json::json!({
                        "planId": plan.id,
                        "stepId": step.id,
                        "requiresKnowledge": step.requires_knowledge,
                    }))
                    .map_err(|_| AppError::StorageOperation)?,
                    allowed_skill_version_id: None,
                },
                &now,
            )?;
            append_plan_audit(
                &transaction,
                state,
                "DELEGATION_CREATED",
                &delegation_id,
                &now,
            )?;
            transaction
                .commit()
                .map_err(|_| AppError::StorageOperation)?;
        }
        transition_step(
            state,
            step,
            PlanStepStatus::Pending,
            PlanStepStatus::Running,
        )?;
        transition_delegation(
            state,
            &delegation_id,
            DelegationStatus::Pending,
            DelegationStatus::Running,
        )?;

        // التنفيذ المحكوم للتفويض (مسار MP03 كاملاً تحت هوية الخطة المعتمدة)
        let execution = match delegation_execution_service::execute_delegation(
            state, &plan, step, &agent, provider,
        ) {
            Ok(execution) => execution,
            Err(error) => {
                finish_delegation(
                    state,
                    &delegation_id,
                    DelegationStatus::Running,
                    false,
                    None,
                    None,
                    Some(match error {
                        AppError::KnowledgeRetrievalFailed => "KNOWLEDGE_RETRIEVAL_FAILED",
                        AppError::ModelUnavailable => "MODEL_UNAVAILABLE",
                        AppError::ExecutionNotConfigured => "EXECUTION_NOT_CONFIGURED",
                        _ => "DELEGATION_EXECUTION_ERROR",
                    }),
                    None,
                )?;
                transition_step(state, step, PlanStepStatus::Running, PlanStepStatus::Failed)?;
                any_failed = true;
                plan_failed = true;
                continue;
            }
        };

        let verification: Option<VerificationRunRecord> = if execution.completed {
            verifier_service::verify_agent_run_internal(state, &execution.agent_run_id, provider)
                .ok()
        } else {
            None
        };

        let citations_snapshot = execution.citations_json.clone();
        finish_delegation(
            state,
            &delegation_id,
            DelegationStatus::Running,
            execution.completed,
            execution.output.as_deref(),
            citations_snapshot.as_deref(),
            execution.failure_code.as_deref(),
            Some(execution.agent_run_id.as_str()),
        )?;
        transition_step(
            state,
            step,
            PlanStepStatus::Running,
            if execution.completed {
                PlanStepStatus::Completed
            } else {
                PlanStepStatus::Failed
            },
        )?;
        append_delegation_terminal_audit(state, &delegation_id, execution.completed)?;

        let delegation_record = {
            let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
            PlanRepository::find_delegation(&connection, &state.tenant_id(), &delegation_id)?
                .ok_or(AppError::ResourceNotFound)?
        };
        delegation_results.push(DelegationWithVerification {
            delegation: delegation_record,
            verification,
        });
        if !execution.completed {
            any_failed = true;
            plan_failed = true;
        }
    }

    // 3) التوليف الحتمي: يحفظ الاقتباسات والتعارضات والمجهولات كما هي —
    //    لا تسطيح عدم اليقين في نثر واثق (§74/C-11).
    let (synthesis, conflict_flag) = build_deterministic_synthesis(&plan, &delegation_results);
    let final_status = if plan_failed {
        PlanStatus::Failed
    } else {
        PlanStatus::Completed
    };
    {
        let now = Utc::now().to_rfc3339();
        let connection = state
            .connection
            .lock()
            .map_err(|_| AppError::StorageOperation)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        let current =
            PlanRepository::find_plan_in_transaction(&transaction, &state.tenant_id(), plan_id)?
                .ok_or(AppError::ResourceNotFound)?;
        if current.status != PlanStatus::Running {
            return Err(AppError::Conflict);
        }
        let changed = PlanRepository::record_synthesis(
            &transaction,
            &state.tenant_id(),
            plan_id,
            PlanStatus::Running,
            current.revision,
            final_status,
            current.revision + 1,
            &synthesis,
            conflict_flag,
            &now,
        )?;
        if !changed {
            return Err(AppError::Conflict);
        }
        append_plan_audit(
            &transaction,
            state,
            if plan_failed {
                "PLAN_FAILED"
            } else {
                "PLAN_COMPLETED"
            },
            plan_id,
            &now,
        )?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
    }

    // 4) ذاكرة حلقية محدودة لإنهاء الخطة (معرفات وأحداث لا نصوص)
    let _ = memory_service::record_episodic_plan_event(state, plan_id, final_status);

    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let updated_plan = PlanRepository::find_plan(&connection, &state.tenant_id(), plan_id)?
        .ok_or(AppError::ResourceNotFound)?;
    Ok(PlanExecutionResult {
        plan: updated_plan,
        delegations: delegation_results,
    })
}

// الاسترداد الحتمي: خطة RUNNING عالقة (انهيار تطبيق) تُنهى فشلاً مع
// تفويضاتها النشطة — لا استئناف صامت (T11/§111).
pub fn recover_stale_running_plans(state: &AppState) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let stale = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        PlanRepository::list_stale_running_plans(&connection, &state.tenant_id())?
    };
    for plan in stale {
        if plan.status != PlanStatus::Running {
            continue;
        }
        let connection = state
            .connection
            .lock()
            .map_err(|_| AppError::StorageOperation)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        let delegations =
            PlanRepository::list_delegations_for_plan(&transaction, &state.tenant_id(), &plan.id)?;
        for delegation in delegations {
            if delegation.status == DelegationStatus::Running
                || delegation.status == DelegationStatus::Pending
            {
                PlanRepository::transition_delegation(
                    &transaction,
                    &state.tenant_id(),
                    &delegation.id,
                    delegation.status,
                    delegation.revision,
                    DelegationStatus::Failed,
                    delegation.revision + 1,
                    None,
                    None,
                    None,
                    Some("RECOVERY"),
                    None,
                    &now,
                )?;
            }
        }
        let steps = PlanRepository::list_steps(&transaction, &state.tenant_id(), &plan.id)?;
        for step in steps {
            if step.status == PlanStepStatus::Running || step.status == PlanStepStatus::Pending {
                PlanRepository::transition_step(
                    &transaction,
                    &state.tenant_id(),
                    &step.id,
                    step.status,
                    PlanStepStatus::Failed,
                    &now,
                )?;
            }
        }
        let changed = PlanRepository::transition_plan(
            &transaction,
            &state.tenant_id(),
            &plan.id,
            PlanStatus::Running,
            plan.revision,
            PlanStatus::Failed,
            plan.revision + 1,
            Some(&now),
            &now,
        )?;
        if changed {
            append_plan_audit(&transaction, state, "PLAN_FAILED", &plan.id, &now)?;
        }
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
    }
    Ok(())
}

fn transition_step(
    state: &AppState,
    step: &PlanStepRecord,
    from: PlanStepStatus,
    to: PlanStepStatus,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let changed = PlanRepository::transition_step(
        &transaction,
        &state.tenant_id(),
        &step.id,
        from,
        to,
        &now,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

fn transition_delegation(
    state: &AppState,
    delegation_id: &str,
    from: DelegationStatus,
    to: DelegationStatus,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let delegation = PlanRepository::find_delegation_in_transaction(
        &transaction,
        &state.tenant_id(),
        delegation_id,
    )?
    .ok_or(AppError::ResourceNotFound)?;
    let changed = PlanRepository::transition_delegation(
        &transaction,
        &state.tenant_id(),
        delegation_id,
        from,
        delegation.revision,
        to,
        delegation.revision + 1,
        None,
        None,
        None,
        None,
        None,
        &now,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

fn finish_delegation(
    state: &AppState,
    delegation_id: &str,
    from: DelegationStatus,
    completed: bool,
    output: Option<&str>,
    citations_json: Option<&str>,
    failure_code: Option<&str>,
    agent_run_id: Option<&str>,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let delegation = PlanRepository::find_delegation_in_transaction(
        &transaction,
        &state.tenant_id(),
        delegation_id,
    )?
    .ok_or(AppError::ResourceNotFound)?;
    let truth_class: Option<&str> = if completed {
        Some("AI_INTERPRETATION")
    } else {
        None
    };
    let changed = PlanRepository::transition_delegation(
        &transaction,
        &state.tenant_id(),
        delegation_id,
        from,
        delegation.revision,
        if completed {
            DelegationStatus::Completed
        } else {
            DelegationStatus::Failed
        },
        delegation.revision + 1,
        output,
        citations_json,
        truth_class,
        failure_code,
        agent_run_id,
        &now,
    )?;
    if !changed {
        return Err(AppError::Conflict);
    }
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

fn append_delegation_terminal_audit(
    state: &AppState,
    delegation_id: &str,
    completed: bool,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    append_plan_audit(
        &transaction,
        state,
        if completed {
            "DELEGATION_COMPLETED"
        } else {
            "DELEGATION_FAILED"
        },
        delegation_id,
        &now,
    )?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

// التوليف الحتمي — يجمع مخرجات محدودة مع اقتباساتها وتحققها ويصرّح
// بالتعارضات والمجهولات بدل دفنها في نثر واثق.
fn build_deterministic_synthesis(
    plan: &PlanRecord,
    delegations: &[DelegationWithVerification],
) -> (String, bool) {
    let mut sections = vec![format!("[GOAL]\n{}", plan.goal)];
    let mut conflict = false;
    let mut unknowns = Vec::new();
    let outcomes: Vec<&str> = delegations
        .iter()
        .map(|entry| {
            entry
                .verification
                .as_ref()
                .map(|verification| verification.outcome.as_str())
                .unwrap_or("NOT_VERIFIED")
        })
        .collect();
    if outcomes.iter().any(|outcome| *outcome == "SUPPORTED")
        && outcomes.iter().any(|outcome| *outcome == "UNSUPPORTED")
    {
        conflict = true;
    }
    if outcomes.iter().any(|outcome| *outcome == "CONFLICTING") {
        conflict = true;
    }
    for (index, entry) in delegations.iter().enumerate() {
        let outcome = outcomes[index];
        if matches!(
            outcome,
            "UNKNOWN" | "INSUFFICIENT_EVIDENCE" | "NOT_VERIFIED"
        ) {
            unknowns.push(format!("step result {index} verification: {outcome}"));
        }
        let bounded_output: String = entry
            .delegation
            .output
            .as_deref()
            .unwrap_or("(no output)")
            .chars()
            .take(1200)
            .collect();
        let citations: Vec<String> = entry
            .delegation
            .citations_json
            .as_deref()
            .and_then(|json| {
                serde_json::from_str::<Vec<crate::domain::knowledge::KnowledgeCitation>>(json).ok()
            })
            .map(|citations| {
                citations
                    .iter()
                    .map(|citation| {
                        format!("{} ({})", citation.citation_id, citation.display_label)
                    })
                    .collect()
            })
            .unwrap_or_default();
        sections.push(format!(
            "[STEP {}]\nstatus: {}\nverification: {}\ncitations: {}\noutput:\n{}",
            index + 1,
            entry.delegation.status.as_code(),
            outcome,
            if citations.is_empty() {
                "(none)".to_owned()
            } else {
                citations.join(", ")
            },
            bounded_output
        ));
    }
    if conflict {
        sections.push(
            "[CONFLICTS]\nDelegated results contain conflicting verification outcomes; both source references are preserved above."
                .to_owned(),
        );
    }
    if !unknowns.is_empty() {
        sections.push(format!("[UNKNOWNS]\n{}", unknowns.join("\n")));
    }
    sections.push(
        "[TRUTH]\nThis synthesis remains AI_INTERPRETATION; cited content provenance is recorded per citation."
            .to_owned(),
    );
    (sections.join("\n\n"), conflict)
}

fn append_plan_audit(
    transaction: &rusqlite::Transaction<'_>,
    state: &AppState,
    event_type: &str,
    resource_id: &str,
    occurred_at: &str,
) -> Result<(), AppError> {
    let audit_id = format!("audit_{}", Uuid::new_v4());
    let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
    append_audit_event(
        transaction,
        &state.tenant_id(),
        &audit_id,
        event_type,
        &state.current_user_id()?,
        "Plan",
        resource_id,
        occurred_at,
        &previous_hash,
    )
    .map(|_| ())
}

#[cfg(test)]
#[path = "supervisor_service_tests.rs"]
mod tests;
