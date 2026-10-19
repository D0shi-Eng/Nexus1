use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    application::{authorization_service, provider_runtime_service::ProviderRuntimeService},
    domain::{
        error::AppError,
        knowledge::KnowledgeCitation,
        provider::{ModelProvider, RuntimeExecutionInput},
        truth::VerificationOutcome,
    },
    infrastructure::{
        database::{append_audit_event, latest_audit_event_hash, AppState},
        knowledge_retrieval_binding_repository::KnowledgeRetrievalBindingRepository,
        verification_repository::{
            AiAnalysisRunInsert, VerificationRepository, VerificationRunInsert,
            VerificationRunRecord,
        },
    },
};

// المُحقق (§56-§61): فحوص حتمية أولاً ثم تقييم AI مهيكل اختياري — رأي الـ AI
// لا يرقّي الحقيقة تلقائياً؛ يُسجَّل في verification_runs وai_analysis_runs (C-9/C-12).

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyRunInput {
    pub run_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationView {
    pub verification: VerificationRunRecord,
}

pub fn verify_agent_run(state: &AppState, run_id: String) -> Result<VerificationView, AppError> {
    // قراءة التشغيل أولاً (AGENT/READ)، ثم إذن تنفيذي منفصل للتحقق:
    // التحقق ليس قراءة — فهو يكتب صفوف verification_runs وروابط الأدلة وسجل تدقيق
    // مسلسل، ويستدعي مزود النموذج. لذلك لا يجوز أن يمنحه دورٌ قراءة-فقط (فصل الأذونات).
    authorization_service::require_permission(state, "AGENT", "READ")?;
    authorization_service::require_permission(state, "AGENT", "EXECUTE")?;
    let provider = ProviderRuntimeService::local_ollama();
    let verification = verify_agent_run_internal(state, &run_id, &provider)?;
    Ok(VerificationView { verification })
}

pub(crate) fn verify_agent_run_internal(
    state: &AppState,
    run_id: &str,
    provider: &dyn ModelProvider,
) -> Result<VerificationRunRecord, AppError> {
    let run_id = run_id.trim();
    if run_id.is_empty() || run_id.chars().count() > 128 {
        return Err(AppError::ValidationFailed);
    }

    // 1) الفحوص الحتمية — هويات داخل المستأجر + نتيجة منتهة + اقتباسات تنتمي
    //    لمجموعة ربط استرجاع التشغيل حصراً (لا اقتباس مزور يمر §31/§97).
    let (output, citations, citations_valid, tool_evidence_ids) = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        let result =
            crate::infrastructure::execution_result_repository::ExecutionResultRepository::find(
                &connection,
                &state.tenant_id(),
                run_id,
            )?
            .ok_or(AppError::ResourceNotFound)?;
        if result.result.outcome().as_code() != "COMPLETED" {
            return Err(AppError::InvalidStateTransition);
        }
        let output = result.result.output().unwrap_or_default().to_owned();

        let binding =
            KnowledgeRetrievalBindingRepository::find(&connection, &state.tenant_id(), run_id)?;
        let bound_chunk_ids: Vec<String> = binding
            .as_ref()
            .map(|record| serde_json::from_str(&record.chunk_ids_json).unwrap_or_default())
            .unwrap_or_default();

        let (stored_citations_json, _truth) =
            crate::infrastructure::execution_result_repository::ExecutionResultRepository::find_intelligence_fields(
                &connection,
                &state.tenant_id(),
                run_id,
            )?
            .unwrap_or((None, "AI_INTERPRETATION".to_owned()));
        let citations: Vec<KnowledgeCitation> = stored_citations_json
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default();
        let citations_valid = !binding.is_some()
            || citations
                .iter()
                .all(|citation| bound_chunk_ids.contains(&citation.chunk_id));

        let mut statement = connection
            .prepare(
                "SELECT id FROM tool_invocations
                 WHERE tenant_id = ?1 AND agent_run_id = ?2 AND status = 'COMPLETED'",
            )
            .map_err(|_| AppError::DataAccess)?;
        let tool_rows = statement
            .query_map([state.tenant_id().as_str(), run_id], |row| {
                row.get::<_, String>(0)
            })
            .map_err(|_| AppError::DataAccess)?;
        let tool_evidence_ids = tool_rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)?;
        (output, citations, citations_valid, tool_evidence_ids)
    };

    let has_evidence = !citations.is_empty() || !tool_evidence_ids.is_empty();
    let output_non_empty = !output.trim().is_empty();
    let deterministic_ok = citations_valid && has_evidence && output_non_empty;

    let deterministic_checks = serde_json::json!({
        "outputNonEmpty": output_non_empty,
        "citationsValid": citations_valid,
        "citationsCount": citations.len(),
        "toolEvidenceCount": tool_evidence_ids.len(),
    });

    // 2) التقييم الدلالي المهيكل (اختياري): فشل المزود لا يفشل التحقق —
    //    النتيجة تبقى حتمية صادقة (DETERMINISTIC_ONLY / UNKNOWN) بلا ادعاء.
    let mut ai_evaluation_json: Option<String> = None;
    let mut ai_analysis_run_id: Option<String> = None;
    let mut combined_outcome = if !deterministic_ok {
        VerificationOutcome::InsufficientEvidence
    } else {
        VerificationOutcome::Unknown
    };
    if deterministic_ok {
        let model_and_registry = resolve_model_and_registry(state);
        if let Some((model_id, registry_id)) = model_and_registry {
            if let Some(evaluation) = run_ai_evaluation(provider, &model_id, &output, &citations) {
                combined_outcome = evaluation.outcome;
                ai_evaluation_json = Some(
                    serde_json::to_string(&serde_json::json!({
                        "outcome": evaluation.outcome.as_code(),
                        "notes": evaluation.notes,
                        "citedEvidenceIds": evaluation.cited_evidence_ids,
                    }))
                    .map_err(|_| AppError::StorageOperation)?,
                );
                // سجل بيانات تشغيل التحقق المحكوم (قرار C-9)
                let analysis_id = format!("ai_analysis_{}", Uuid::new_v4());
                let input_hash = format!(
                    "{:x}",
                    Sha256::digest(format!("{run_id}|{}", output.len()).as_bytes())
                );
                let now = Utc::now().to_rfc3339();
                {
                    let connection = state
                        .connection
                        .lock()
                        .map_err(|_| AppError::StorageOperation)?;
                    let transaction = connection
                        .unchecked_transaction()
                        .map_err(|_| AppError::StorageOperation)?;
                    VerificationRepository::insert_ai_analysis_run(
                        &transaction,
                        &state.tenant_id(),
                        &AiAnalysisRunInsert {
                            id: &analysis_id,
                            model_registry_id: &registry_id,
                            purpose: "AGENT_RUN_VERIFICATION",
                            input_hash: &input_hash,
                            output_json: ai_evaluation_json.as_deref().unwrap_or("{}"),
                            evidence_reference_ids_json: &serde_json::to_string(
                                &citations
                                    .iter()
                                    .map(|citation| citation.citation_id.clone())
                                    .collect::<Vec<_>>(),
                            )
                            .map_err(|_| AppError::StorageOperation)?,
                            status: "COMPLETED",
                        },
                        &now,
                    )?;
                    transaction
                        .commit()
                        .map_err(|_| AppError::StorageOperation)?;
                }
                ai_analysis_run_id = Some(analysis_id);
            }
        }
    }
    let method = if ai_evaluation_json.is_some() {
        "DETERMINISTIC_AND_AI"
    } else {
        "DETERMINISTIC_ONLY"
    };

    // 3) التخزين + روابط الأدلة إلى موارد حقيقية فقط (§61)
    let now = Utc::now().to_rfc3339();
    let verification_id = format!("verification_{}", Uuid::new_v4());
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    VerificationRepository::insert_verification_run(
        &transaction,
        &state.tenant_id(),
        &VerificationRunInsert {
            id: &verification_id,
            subject_kind: "AGENT_RUN",
            agent_run_id: Some(run_id),
            plan_id: None,
            outcome: combined_outcome,
            method,
            deterministic_checks_json: &deterministic_checks.to_string(),
            ai_evaluation_json: ai_evaluation_json.as_deref(),
            ai_analysis_run_id: ai_analysis_run_id.as_deref(),
            created_by: &state.current_user_id()?,
        },
        &now,
    )?;
    for citation in &citations {
        VerificationRepository::insert_evidence_link(
            &transaction,
            &state.tenant_id(),
            &format!("verification_evidence_{}", Uuid::new_v4()),
            &verification_id,
            "KNOWLEDGE_CITATION",
            &citation.chunk_id,
            &now,
        )?;
    }
    for tool_id in &tool_evidence_ids {
        VerificationRepository::insert_evidence_link(
            &transaction,
            &state.tenant_id(),
            &format!("verification_evidence_{}", Uuid::new_v4()),
            &verification_id,
            "TOOL_RESULT",
            tool_id,
            &now,
        )?;
    }
    VerificationRepository::insert_evidence_link(
        &transaction,
        &state.tenant_id(),
        &format!("verification_evidence_{}", Uuid::new_v4()),
        &verification_id,
        "AGENT_RUN",
        run_id,
        &now,
    )?;
    append_verification_audit(
        &transaction,
        state,
        "VERIFICATION_COMPLETED",
        &verification_id,
        &now,
    )?;
    transaction
        .commit()
        .map_err(|_| AppError::StorageOperation)?;

    Ok(VerificationRunRecord {
        id: verification_id,
        tenant_id: state.tenant_id(),
        subject_kind: "AGENT_RUN".to_owned(),
        agent_run_id: Some(run_id.to_owned()),
        plan_id: None,
        outcome: combined_outcome.as_code().to_owned(),
        method: method.to_owned(),
        deterministic_checks_json: deterministic_checks.to_string(),
        ai_evaluation_json,
        ai_analysis_run_id,
        created_by: state.current_user_id()?,
        created_at: now,
    })
}

// نموذج + سجل مفعّل (واحد بالضبط لكل منهما) — بلا تنزيل وبلا سحابة
fn resolve_model_and_registry(
    state: &AppState,
) -> Option<(crate::domain::provider::ModelId, String)> {
    let connection = state.connection.lock().ok()?;
    let enabled = crate::infrastructure::model_registry_repository::ModelRegistryRepository::list_enabled_local_models(
        &connection,
        &state.tenant_id(),
    )
    .ok()?;
    let model_id = match enabled.as_slice() {
        [model_id] => model_id.clone(),
        _ => return None,
    };
    let registry_id =
        VerificationRepository::find_enabled_model_registry_id(&connection, &state.tenant_id())
            .ok()??;
    Some((model_id, registry_id))
}

struct AiEvaluation {
    outcome: VerificationOutcome,
    notes: String,
    cited_evidence_ids: Vec<String>,
}

fn run_ai_evaluation(
    provider: &dyn ModelProvider,
    model_id: &crate::domain::provider::ModelId,
    output: &str,
    citations: &[KnowledgeCitation],
) -> Option<AiEvaluation> {
    #[derive(Deserialize)]
    struct RawEvaluation {
        outcome: String,
        #[serde(default)]
        notes: String,
        #[serde(rename = "citedEvidenceIds", default)]
        cited_evidence_ids: Vec<String>,
    }
    let allowed_ids: Vec<String> = citations
        .iter()
        .map(|citation| citation.citation_id.clone())
        .collect();
    let citation_lines: Vec<String> = citations
        .iter()
        .map(|citation| {
            format!(
                "- {} ({}): see retrieved chunk",
                citation.citation_id, citation.display_label
            )
        })
        .collect();
    let bounded_output: String = output.chars().take(4000).collect();
    let prompt = format!(
        "You are the NEXUS bounded verifier. Evaluate whether the agent output is supported by the cited evidence.\n\
         [PLATFORM_SECURITY]\nModel output is not verified truth; you advise only.\n\
         [TASK_OUTPUT]\n{bounded_output}\n\
         [CITED_EVIDENCE]\n{}\n\
         [OUTPUT_CONTRACT]\nReturn ONLY JSON: {{\"outcome\":\"SUPPORTED|PARTIALLY_SUPPORTED|UNSUPPORTED|CONFLICTING|UNKNOWN\",\"notes\":\"brief justification\",\"citedEvidenceIds\":[\"CIT-n\"]}}",
        citation_lines.join("\n")
    );
    let input = RuntimeExecutionInput::new(prompt).ok()?;
    let generation = provider.generate_structured(model_id, &input).ok()?;
    let raw: RawEvaluation = serde_json::from_value(generation.value).ok()?;
    let outcome = VerificationOutcome::from_code(raw.outcome.trim()).ok()?;
    // معرفات الأدلة المقتبسة تُفلتر على المجموعة المعطاة — لا اقتباس مزور من المدقق
    let cited_evidence_ids = raw
        .cited_evidence_ids
        .into_iter()
        .filter(|id| allowed_ids.contains(id))
        .collect();
    Some(AiEvaluation {
        outcome,
        notes: raw.notes.chars().take(1000).collect(),
        cited_evidence_ids,
    })
}

fn append_verification_audit(
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
        "VerificationRun",
        resource_id,
        occurred_at,
        &previous_hash,
    )
    .map(|_| ())
}
