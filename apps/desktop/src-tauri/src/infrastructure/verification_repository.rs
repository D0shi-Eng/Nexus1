use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::domain::{error::AppError, truth::VerificationOutcome};

// مستودع التحقق: verification_runs + روابط الأدلة + كتابة ai_analysis_runs
// (قرار C-9: الجدول المحجوز من MP02 يصبح سجل بيانات تشغيل التحقق المحكوم).
pub struct VerificationRepository;

pub struct VerificationRunInsert<'a> {
    pub id: &'a str,
    pub subject_kind: &'a str,
    pub agent_run_id: Option<&'a str>,
    pub plan_id: Option<&'a str>,
    pub outcome: VerificationOutcome,
    pub method: &'a str,
    pub deterministic_checks_json: &'a str,
    pub ai_evaluation_json: Option<&'a str>,
    pub ai_analysis_run_id: Option<&'a str>,
    pub created_by: &'a str,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationRunRecord {
    pub id: String,
    pub tenant_id: String,
    pub subject_kind: String,
    pub agent_run_id: Option<String>,
    pub plan_id: Option<String>,
    pub outcome: String,
    pub method: String,
    pub deterministic_checks_json: String,
    pub ai_evaluation_json: Option<String>,
    pub ai_analysis_run_id: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

pub struct AiAnalysisRunInsert<'a> {
    pub id: &'a str,
    pub model_registry_id: &'a str,
    pub purpose: &'a str,
    pub input_hash: &'a str,
    pub output_json: &'a str,
    pub evidence_reference_ids_json: &'a str,
    pub status: &'a str,
}

impl VerificationRepository {
    pub fn insert_verification_run(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        insert: &VerificationRunInsert<'_>,
        now: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO verification_runs (
                    id, tenant_id, subject_kind, agent_run_id, plan_id, outcome, method,
                    deterministic_checks_json, ai_evaluation_json, ai_analysis_run_id,
                    created_by, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    insert.id,
                    tenant_id,
                    insert.subject_kind,
                    insert.agent_run_id,
                    insert.plan_id,
                    insert.outcome.as_code(),
                    insert.method,
                    insert.deterministic_checks_json,
                    insert.ai_evaluation_json,
                    insert.ai_analysis_run_id,
                    insert.created_by,
                    now,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn insert_evidence_link(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        link_id: &str,
        verification_run_id: &str,
        evidence_kind: &str,
        evidence_ref: &str,
        now: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO verification_evidence_links (
                    id, tenant_id, verification_run_id, evidence_kind, evidence_ref, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    link_id,
                    tenant_id,
                    verification_run_id,
                    evidence_kind,
                    evidence_ref,
                    now
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    pub fn list_latest_for_agent_run(
        connection: &Connection,
        tenant_id: &str,
        agent_run_id: &str,
    ) -> Result<Option<VerificationRunRecord>, AppError> {
        connection
            .query_row(
                "SELECT id, tenant_id, subject_kind, agent_run_id, plan_id, outcome, method,
                        deterministic_checks_json, ai_evaluation_json, ai_analysis_run_id,
                        created_by, created_at
                 FROM verification_runs
                 WHERE tenant_id = ?1 AND agent_run_id = ?2
                 ORDER BY created_at DESC, id ASC
                 LIMIT 1",
                params![tenant_id, agent_run_id],
                map_verification_run,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    // كتابة ai_analysis_runs — سجل بيانات تشغيل التحقق (C-9): بلا confidence
    // مجردة وبلا محتوى خام — هاش المدخل + مخرج مهيكل + معرفات أدلة فقط.
    pub fn insert_ai_analysis_run(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        insert: &AiAnalysisRunInsert<'_>,
        now: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT INTO ai_analysis_runs (
                    id, tenant_id, model_registry_id, purpose, input_hash, output_json,
                    confidence, human_review_state, status, evidence_reference_ids_json,
                    created_at, completed_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, 'NOT_REQUESTED', ?7, ?8, ?9, ?9)",
                params![
                    insert.id,
                    tenant_id,
                    insert.model_registry_id,
                    insert.purpose,
                    insert.input_hash,
                    insert.output_json,
                    insert.status,
                    insert.evidence_reference_ids_json,
                    now,
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    // سجل النموذج المفعّل الوحيد — نفس قاعدة حل النموذج في MP03 (واحد بالضبط)
    pub fn find_enabled_model_registry_id(
        connection: &Connection,
        tenant_id: &str,
    ) -> Result<Option<String>, AppError> {
        connection
            .query_row(
                "SELECT id FROM ai_model_registry WHERE tenant_id = ?1 AND enabled = 1",
                params![tenant_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }
}

fn map_verification_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<VerificationRunRecord> {
    Ok(VerificationRunRecord {
        id: row.get(0)?,
        tenant_id: row.get(1)?,
        subject_kind: row.get(2)?,
        agent_run_id: row.get(3)?,
        plan_id: row.get(4)?,
        outcome: row.get(5)?,
        method: row.get(6)?,
        deterministic_checks_json: row.get(7)?,
        ai_evaluation_json: row.get(8)?,
        ai_analysis_run_id: row.get(9)?,
        created_by: row.get(10)?,
        created_at: row.get(11)?,
    })
}
