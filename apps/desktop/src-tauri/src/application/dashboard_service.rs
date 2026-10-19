use chrono::Utc;
use rusqlite::params;

use crate::{
    application::{attention_service, audit_service, authorization_service},
    domain::{
        error::AppError,
        models::{
            AuditEventSummary, DashboardData, ObligationSummary, ReadinessSnapshot, SourceReference,
        },
    },
    infrastructure::database::AppState,
};

pub fn load_dashboard(state: &AppState) -> Result<DashboardData, AppError> {
    authorization_service::require_permission(state, "OBLIGATION", "READ")?;
    let attention = attention_service::load_attention(state)?;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let tenant_id = state.tenant_id();
    let obligations = load_obligations(&connection, &tenant_id)?;
    // ظˆط¶ط¹ ط§ظ„ط¨ظٹط§ظ†ط§طھ ظٹظڈظ‚ط±ط£ ظ…ظ† طµظپ ط§ظ„ظ…ط³طھط£ط¬ط± ظ†ظپط³ظ‡ â€” ظ„ط§ ظ…ظ† ظƒظٹط§ظ† ط¹ط§ظ… ظˆظ„ط§ ظ…ظ† طھط®ظ…ظٹظ†.
    let data_mode: String = connection
        .query_row(
            "SELECT data_mode FROM tenants WHERE id = ?1",
            [&tenant_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::ResourceNotFound)?;
    let now = Utc::now().to_rfc3339();
    let evidence_covered = load_evidence_covered(&connection, &tenant_id, &now)?;
    let archived_obligations = load_archived_obligation_count(&connection, &tenant_id)?;
    let readiness = calculate_readiness(
        &tenant_id,
        &obligations,
        &data_mode,
        &evidence_covered,
        archived_obligations,
    );
    // ط³ظ„ط§ظ…ط© ط³ظ„ط³ظ„ط© ط§ظ„طھط¯ظ‚ظٹظ‚ طھظڈط­ط³ط¨ ظپط¹ظ„ظٹط§ظ‹ ظ‡ظ†ط§ ط¨ط¯ظ„ ط®طھظ… ظƒظ„ ط­ط¯ط« ط¨ظ€آ«VALIDآ» ط¨ظ„ط§ ظپط­طµ.
    let chain = audit_service::verify_chain_report(&connection, &tenant_id)?;
    let recent_audit_events = load_audit_events(&connection, &tenant_id, &chain)?;

    Ok(DashboardData {
        attention_items: attention.items,
        obligations,
        readiness,
        recent_audit_events,
        audit_chain_integrity: if chain.valid {
            "VALID".to_owned()
        } else {
            "INVALID".to_owned()
        },
        data_mode,
    })
}

// ط§ظ„ط§ظ„طھط²ط§ظ…ط§طھ ط§ظ„طھظٹ طھط­ظ…ظ„ ط¯ظ„ظٹظ„ط§ظ‹ ظپط¹ظ„ظٹط§ظ‹ ط¨ط­ط§ظ„ط© ظ…ظ‚ط¨ظˆظ„ط©. ط§ظ„ظ…طµط¯ط± ظ‡ظˆ ط¬ط¯ظˆظ„ ط§ظ„ط£ط¯ظ„ط© ظ†ظپط³ظ‡طŒ
// **ظ„ط§** `obligations.evidence_status` ط§ظ„ظ…ط´طھظ‚.
//
// ط§ظ„ط¹ظٹط¨ ط§ظ„ط°ظٹ ط£ظڈط؛ظ„ظ‚ ظ‡ظ†ط§: ظƒط§ظ†طھ ط¯ط±ط¬ط© ط§ظƒطھظ…ط§ظ„ ط§ظ„ط£ط¯ظ„ط© طھظڈط­ط³ط¨ ظ…ظ†
// `obligations.evidence_status = 'VALID'`طŒ ظˆ`VALID` ظ‚ظٹظ…ط© **ظ„ط§ ظٹظƒطھط¨ظ‡ط§ ط£ظٹ ظ…ط³ط§ط±
// طھط·ط¨ظٹظ‚ظٹ** ظپظٹ ط°ط§ظƒ ط§ظ„ط¹ظ…ظˆط¯: `evidence_repository::obligation_evidence_status`
// طھظڈظ†طھط¬ `MISSING` ط£ظˆ `REQUIRES_REVIEW` ط­طµط±ط§ظ‹ (ط¨طھطµظ…ظٹظ… ظ…ط¹ظ„ظ†: آ«ظ„ط§ ط§ط³طھظ†طھط§ط¬ ط­ط§ظ„ط©
// ط£ط¹ظ„ظ‰ ط¨ط§ظ„ظ†ظٹط§ط¨ط©آ»). ظپط§ظ„ظ†طھظٹط¬ط© ط£ظ† ط§ظ„ظ…ط¤ط´ط± ظƒط§ظ† **طµظپط±ط§ظ‹ ط¨ظ†ظٹظˆظٹط§ظ‹ ظ„ظƒظ„ ظ…ط³طھط£ط¬ط± ط­ظ‚ظٹظ‚ظٹ** ظ…ظ‡ظ…ط§
// ط³ط¬ظ‘ظ„ ظ…ظ† ط£ط¯ظ„ط© â€” طµظپط± ظٹظˆظ‡ظ… ط¨ط§ظ„ظپط´ظ„ ط¨ط¯ظ„ ط£ظ† ظٹظ‚ظٹط³ ط´ظٹط¦ط§ظ‹.
fn load_evidence_covered(
    connection: &rusqlite::Connection,
    tenant_id: &str,
    now: &str,
) -> Result<std::collections::HashSet<String>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT obligation_id FROM evidence_items
             WHERE tenant_id = ?1
               AND status IN ('UPLOADED', 'PROCESSING', 'ANALYZED', 'VALID')
               -- الدليل المنتهي لا يغطي التزاماً: الانتهاء حالة مقروءة من الوقت
               -- الحالي لا عمود يُكتب، فالاستبعاد هنا شرط صريح على الأجل.
               AND (expires_at IS NULL OR expires_at > ?2)",
        )
        .map_err(|_| AppError::DataAccess)?;
    let rows = statement
        .query_map(params![tenant_id, now], |row| row.get::<_, String>(0))
        .map_err(|_| AppError::DataAccess)?;
    let mut covered = std::collections::HashSet::new();
    for row in rows {
        covered.insert(row.map_err(|_| AppError::DataAccess)?);
    }
    Ok(covered)
}

// سياسة العد المعلنة: الالتزامات المؤرشفة لا تدخل في أي مؤشر حالة نشط
// (الجاهزية، التغطية، البنود الحرجة المفتوحة، المقام). تُحتسب هنا صراحة
// وتُعرض في اللقطة كي يفهم المستخدم الفرق بينها وبين سجل الالتزامات الكامل.
fn load_archived_obligation_count(
    connection: &rusqlite::Connection,
    tenant_id: &str,
) -> Result<u32, AppError> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM obligations WHERE tenant_id = ?1 AND archived_at IS NOT NULL",
            params![tenant_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| u32::try_from(count).unwrap_or(u32::MAX))
        .map_err(|_| AppError::DataAccess)
}

fn load_obligations(
    connection: &rusqlite::Connection,
    tenant_id: &str,
) -> Result<Vec<ObligationSummary>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT o.id, o.tenant_id, o.title, rs.authority, o.owner, o.deadline, o.severity, o.status, o.applicability, o.evidence_status, o.verification_state, rs.document_title, rs.source_url, rs.document_version, rs.retrieved_at, rs.official, o.updated_at
             FROM obligations o
             INNER JOIN regulatory_sources rs ON rs.id = o.source_id AND rs.tenant_id = o.tenant_id
             WHERE o.tenant_id = ?1
               AND o.archived_at IS NULL
             ORDER BY CASE o.severity WHEN 'CRITICAL' THEN 0 WHEN 'HIGH' THEN 1 WHEN 'MEDIUM' THEN 2 ELSE 3 END, o.deadline ASC",
        )
        .map_err(|_| AppError::DataAccess)?;

    let mapped_rows = statement
        .query_map(params![tenant_id], |row| {
            Ok(ObligationSummary {
                id: row.get(0)?,
                tenant_id: row.get(1)?,
                title: row.get(2)?,
                authority: row.get(3)?,
                owner: row.get(4)?,
                deadline: row.get(5)?,
                severity: row.get(6)?,
                status: row.get(7)?,
                applicability: row.get(8)?,
                evidence_status: row.get(9)?,
                verification_state: row.get(10)?,
                source: SourceReference {
                    authority: row.get(3)?,
                    document_title: row.get(11)?,
                    source_url: row.get(12)?,
                    version: row.get(13)?,
                    section: None,
                    retrieved_at: row.get(14)?,
                    official: row.get(15)?,
                },
                updated_at: row.get(16)?,
            })
        })
        .map_err(|_| AppError::DataAccess)?;

    mapped_rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::DataAccess)
}

// =====================================================================
// طھظ‚ط¯ظٹط± ط§ظ„ط¬ط§ظ‡ط²ظٹط© â€” ط§ظ„ظ‚ط§ط¹ط¯ط© ظ…ظˆط«ظ‚ط© ظˆظ…ظڈط®طھط¨ط±ط©طŒ ظ„ط§ ط±ظ‚ظ… ظ…ط®طھظ„ظ‚
// ---------------------------------------------------------------------
// ط§ظ„ظ…ظ‚ط§ظ…: ط§ظ„ط§ظ„طھط²ط§ظ…ط§طھ **ط§ظ„ظ…ط¤ظ‡ظ„ط©** ظپظ‚ط· (applicability = 'APPLIES').
//   ط§ظ„ط§ظ„طھط²ط§ظ…ط§طھ DOES_NOT_APPLY ظˆUNCERTAIN ظˆREQUIRES_REVIEW ظ…ط³طھط¨ط¹ط¯ط© طµط±ط§ط­ط©ظ‹:
//   ط¥ط¯ط±ط§ط¬ظ‡ط§ ظپظٹ ط§ظ„ظ…ظ‚ط§ظ… ظٹط®ظپط¶ ط§ظ„ظ†ط³ط¨ط© ط¨ط³ط¨ط¨ ط³ط¬ظ„ط§طھ ظ„ظ… ظٹظ†ط·ط¨ظ‚ ط¹ظ„ظٹظ‡ط§ ط§ظ„ط§ظ„طھط²ط§ظ… ط£طµظ„ط§ظ‹.
// ط§ظ„ط¨ط³ط·: طھط؛ط·ظٹط© طھظ… ط§ظ„طھط­ظ‚ظ‚ ظ…ظ†ظ‡ط§ = HUMAN_VERIFIED أ· ط§ظ„ظ…ط¤ظ‡ظ„ط©.
//        ط§ظƒطھظ…ط§ظ„ ط§ظ„ط£ط¯ظ„ط©       = ط§ظ„ظ…ط¤ظ‡ظ„ط© ط§ظ„طھظٹ طھط­ظ…ظ„ ط¯ظ„ظٹظ„ط§ظ‹ ظپط¹ظ„ظٹط§ظ‹ ط¨ط­ط§ظ„ط© ظ…ظ‚ط¨ظˆظ„ط©
//                              (UPLOADED/PROCESSING/ANALYZED/VALID) أ· ط§ظ„ظ…ط¤ظ‡ظ„ط©.
//   ظ…طµط¯ط± ط§ظ„ط£ط¯ظ„ط© ظ‡ظˆ ط¬ط¯ظˆظ„ evidence_items ظ„ط§ ط¹ظ…ظˆط¯ obligations.evidence_status
//   ط§ظ„ظ…ط´طھظ‚ â€” ظ„ظ„ط£ط³ط¨ط§ط¨ ط§ظ„ظ…ط´ط±ظˆط­ط© ط¹ظ†ط¯ load_evidence_covered.
// ط§ظ„ط¯ط±ط¬ط©: (ط§ظ„طھط؛ط·ظٹط© أ— 60 + ط§ظ„ط§ظƒطھظ…ط§ظ„ أ— 40) أ· 100 â€” ظˆط²ظ† ط§ظ„طھط­ظ‚ظ‚ ط§ظ„ط¨ط´ط±ظٹ ط£ط¹ظ„ظ‰
//   ظ„ط£ظ† ط§ظ„ط£ط¯ظ„ط© ظˆط­ط¯ظ‡ط§ ظ„ط§ طھط¹ظ†ظٹ ظ…ط±ط§ط¬ط¹ط© ط¨ط´ط±ظٹط©.
// ط¹ظ†ط¯ ط؛ظٹط§ط¨ ط§ظ„طھط²ط§ظ… ظ…ط¤ظ‡ظ„: **ظ„ط§ ظ†ط³ط¨ط© ط¥ط·ظ„ط§ظ‚ط§ظ‹** (sufficient_data = false).
//   ط¹ط±ط¶ آ«0%آ» ظپظٹ ظ‡ط°ظ‡ ط§ظ„ط­ط§ظ„ط© ط§ط¯ط¹ط§ط، ظƒط§ط°ط¨ ط¨ط£ظ† ط§ظ„ط¹ظ…ظ„ ط¬ظڈط±ظگظ‘ط¨ ظˆظپط´ظ„طŒ ظˆط§ظ„ط­ظ‚ظٹظ‚ط© ط£ظ†
//   ط§ظ„ظ‚ظٹط§ط³ ط؛ظٹط± ظ…ظ…ظƒظ†. ظ‡ط°ط§ ظ…ط§ ظƒط§ظ† ظٹط­ط¯ط« ظ‚ط¨ظ„ ط§ظ„ط¥طµظ„ط§ط­ ط¨ط³ط¨ط¨ `len().max(1)`.
// ط§ظ„ط¹ظ†ط§طµط± ط§ظ„ط­ط±ط¬ط© ط§ظ„ظ…ظپطھظˆط­ط©: ظƒظ„ ط§ظ„طھط²ط§ظ… ظ…ط¤ظ‡ظ„ ط´ط¯طھظ‡ CRITICAL ظˆط­ط§ظ„طھظ‡ ظ„ظٹط³طھ COMPLETED.
// =====================================================================
fn calculate_readiness(
    tenant_id: &str,
    obligations: &[ObligationSummary],
    data_mode: &str,
    evidence_covered: &std::collections::HashSet<String>,
    archived_obligations: u32,
) -> ReadinessSnapshot {
    let eligible: Vec<&ObligationSummary> = obligations
        .iter()
        .filter(|obligation| obligation.applicability == "APPLIES")
        .collect();
    let eligible_count = u32::try_from(eligible.len()).unwrap_or(u32::MAX);

    let open_critical_items = eligible
        .iter()
        .filter(|obligation| obligation.severity == "CRITICAL" && obligation.status != "COMPLETED")
        .count() as u32;

    let (readiness_score, verified_coverage, evidence_completeness) = if eligible.is_empty() {
        (None, None, None)
    } else {
        let human_verified = eligible
            .iter()
            .filter(|obligation| obligation.verification_state == "HUMAN_VERIFIED")
            .count() as u32;
        let valid_evidence = eligible
            .iter()
            .filter(|obligation| evidence_covered.contains(&obligation.id))
            .count() as u32;
        let coverage = ((human_verified * 100) / eligible_count) as u8;
        let completeness = ((valid_evidence * 100) / eligible_count) as u8;
        let score = (((coverage as u16 * 60) + (completeness as u16 * 40)) / 100) as u8;
        (Some(score), Some(coverage), Some(completeness))
    };

    // ط§ظ„ط¥ظپطµط§ط­ ظٹطھط¨ط¹ ظˆط¶ط¹ ط§ظ„ط¨ظٹط§ظ†ط§طھ ط§ظ„ظپط¹ظ„ظٹ: طھط³ظ…ظٹط© ط¨ظٹط§ظ†ط§طھ ط§ظ„ط¥ظ†طھط§ط¬ آ«DEMO / SYNTHETICآ»
    // ظƒط§ظ†طھ ط®ط·ط£ ظ…ظ†ط´ط£ ظپظٹ ط§ظ„ط§طھط¬ط§ظ‡ ط§ظ„ظ…ط¹ط§ظƒط³ â€” طھظڈطµط¯ظ‘ظ‚ ط¹ظ„ظ‰ ط¨ظٹط§ظ†ط§طھ ط­ظ‚ظٹظ‚ظٹط© ط£ظ†ظ‡ط§ ط§طµط·ظ†ط§ط¹ظٹط©.
    let disclaimer = if data_mode == "DEMO_SYNTHETIC" {
        "DEMO / SYNTHETIC readiness estimate based on locally available evidence. Human verification is required."
    } else {
        "Readiness is a local operational estimate computed only from obligations applicable to this organization. It is not a legal compliance assertion."
    };

    ReadinessSnapshot {
        tenant_id: tenant_id.to_owned(),
        readiness_score,
        verified_coverage,
        evidence_completeness,
        open_critical_items,
        eligible_obligations: eligible_count,
        archived_obligations,
        sufficient_data: !eligible.is_empty(),
        generated_at: Utc::now().to_rfc3339(),
        disclaimer: disclaimer.to_owned(),
    }
}

fn load_audit_events(
    connection: &rusqlite::Connection,
    tenant_id: &str,
    chain: &audit_service::AuditChainReport,
) -> Result<Vec<AuditEventSummary>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT id, event_type, actor_name, resource_type, resource_id, occurred_at
             FROM audit_events
             WHERE tenant_id = ?1
             ORDER BY occurred_at DESC
             LIMIT 6",
        )
        .map_err(|_| AppError::DataAccess)?;

    let mapped_rows = statement
        .query_map(params![tenant_id], |row| {
            Ok(AuditEventSummary {
                id: row.get(0)?,
                event_type: row.get(1)?,
                actor_name: row.get(2)?,
                resource_type: row.get(3)?,
                resource_id: row.get(4)?,
                occurred_at: row.get(5)?,
                integrity_status: String::new(),
            })
        })
        .map_err(|_| AppError::DataAccess)?;

    let mut events = mapped_rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::DataAccess)?;
    // ط§ظ„ط­ط§ظ„ط© طھظڈط´طھظ‚ ظ…ظ† ط§ظ„طھط­ظ‚ظ‚ ط§ظ„ظپط¹ظ„ظٹ ظ„ظƒظ„ ط­ط¯ط«طŒ ظ„ط§ ظ…ظ† ظ‚ظٹظ…ط© ط«ط§ط¨طھط©.
    for event in &mut events {
        event.integrity_status = if chain.invalid_event_ids.contains(&event.id) {
            "INVALID".to_owned()
        } else {
            "VALID".to_owned()
        };
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::{calculate_readiness, load_dashboard};
    use crate::{
        application::authentication_service::{bootstrap_production, BootstrapProductionInput},
        domain::models::{ObligationSummary, SourceReference},
        infrastructure::database::{
            initialize_demo_database, AppState, DEMO_OPERATOR_USER_ID, DEMO_TENANT_ID,
        },
    };
    use rusqlite::Connection;

    // مجموعة الالتزامات التي تحمل دليلاً فعلياً — تُبنى في الاختبار بدل قاعدة.
    fn covered(ids: &[&str]) -> std::collections::HashSet<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    fn demo_state() -> AppState {
        let connection = Connection::open_in_memory()
            .expect("ظ‚ط§ط¹ط¯ط© ط§ط®طھط¨ط§ط± ط§ظ„ط°ط§ظƒط±ط© ظٹط¬ط¨ ط£ظ† طھظپطھط­");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("طھظپط¹ظٹظ„ ظ…ظپط§طھظٹط­ ط§ظ„ط£ط¬ظ†ط¨ط© ظٹط¬ط¨ ط£ظ† ظٹظ†ط¬ط­");
        initialize_demo_database(&connection)
            .expect("طھظ‡ظٹط¦ط© ظ‚ط§ط¹ط¯ط© ط§ظ„ط¯ظٹظ…ظˆ ظٹط¬ط¨ ط£ظ† طھظ†ط¬ط­");
        AppState::test_instance(connection, DEMO_TENANT_ID, DEMO_OPERATOR_USER_ID)
    }

    fn obligation(
        id: &str,
        severity: &str,
        status: &str,
        applicability: &str,
        evidence_status: &str,
        verification_state: &str,
    ) -> ObligationSummary {
        ObligationSummary {
            id: id.to_owned(),
            tenant_id: "tenant-test".to_owned(),
            title: "Test obligation".to_owned(),
            authority: "Test authority".to_owned(),
            owner: "Test owner".to_owned(),
            deadline: None,
            severity: severity.to_owned(),
            status: status.to_owned(),
            applicability: applicability.to_owned(),
            evidence_status: evidence_status.to_owned(),
            verification_state: verification_state.to_owned(),
            source: SourceReference {
                authority: "Test authority".to_owned(),
                document_title: "Test document".to_owned(),
                source_url: None,
                version: None,
                section: None,
                retrieved_at: "2026-08-16T00:00:00Z".to_owned(),
                official: false,
            },
            updated_at: "2026-08-16T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn readiness_is_derived_from_verified_coverage_and_valid_evidence() {
        let obligations = vec![
            obligation(
                "one",
                "CRITICAL",
                "OPEN",
                "APPLIES",
                "VALID",
                "HUMAN_VERIFIED",
            ),
            obligation(
                "two",
                "MEDIUM",
                "COMPLETED",
                "APPLIES",
                "MISSING",
                "UNVERIFIED",
            ),
        ];

        let readiness = calculate_readiness(
            "tenant-test",
            &obligations,
            "PRODUCTION",
            &covered(&["one"]),
            0,
        );

        assert!(readiness.sufficient_data);
        assert_eq!(readiness.eligible_obligations, 2);
        assert_eq!(readiness.verified_coverage, Some(50));
        assert_eq!(readiness.evidence_completeness, Some(50));
        assert_eq!(readiness.readiness_score, Some(50));
        assert_eq!(readiness.open_critical_items, 1);
        assert!(readiness
            .disclaimer
            .contains("not a legal compliance assertion"));
    }

    #[test]
    fn an_empty_portfolio_reports_insufficient_data_instead_of_a_fake_zero() {
        let readiness = calculate_readiness("tenant-test", &[], "PRODUCTION", &covered(&[]), 0);
        // ط§ظ„ط¹ظٹط¨ ط§ظ„ظ‚ط¯ظٹظ…: `len().max(1)` ظƒط§ظ† ظٹظ†طھط¬ 0% ظˆظƒط£ظ† ط§ظ„ظ‚ظٹط§ط³ ط¬ط±ظ‰ ظپط¹ظ„ط§ظ‹.
        assert!(!readiness.sufficient_data);
        assert_eq!(readiness.readiness_score, None);
        assert_eq!(readiness.verified_coverage, None);
        assert_eq!(readiness.evidence_completeness, None);
        assert_eq!(readiness.eligible_obligations, 0);
    }

    #[test]
    fn non_applicable_records_are_excluded_from_the_denominator() {
        let obligations = vec![
            obligation(
                "applies",
                "HIGH",
                "OPEN",
                "APPLIES",
                "VALID",
                "HUMAN_VERIFIED",
            ),
            obligation(
                "na",
                "HIGH",
                "OPEN",
                "DOES_NOT_APPLY",
                "MISSING",
                "UNVERIFIED",
            ),
            obligation(
                "uncertain",
                "HIGH",
                "OPEN",
                "UNCERTAIN",
                "MISSING",
                "UNVERIFIED",
            ),
        ];

        let readiness = calculate_readiness(
            "tenant-test",
            &obligations,
            "PRODUCTION",
            &covered(&["applies"]),
            0,
        );

        assert_eq!(readiness.eligible_obligations, 1);
        assert_eq!(readiness.verified_coverage, Some(100));
        assert_eq!(readiness.evidence_completeness, Some(100));
        assert_eq!(readiness.readiness_score, Some(100));
    }

    #[test]
    fn the_demo_disclaimer_never_labels_production_data_as_synthetic() {
        let obligations = vec![obligation(
            "one",
            "HIGH",
            "OPEN",
            "APPLIES",
            "VALID",
            "HUMAN_VERIFIED",
        )];
        let demo = calculate_readiness(
            "tenant-test",
            &obligations,
            "DEMO_SYNTHETIC",
            &covered(&["one"]),
            0,
        );
        let production = calculate_readiness(
            "tenant-test",
            &obligations,
            "PRODUCTION",
            &covered(&["one"]),
            0,
        );
        assert!(demo.disclaimer.starts_with("DEMO / SYNTHETIC"));
        assert!(!production.disclaimer.contains("SYNTHETIC"));
    }

    // ط§ظ„ط¹ظٹط¨ ط§ظ„ط°ظٹ ط£ظڈط؛ظ„ظ‚: ظƒط§ظ† ط§ظ„ط®ظ„ظپ ظٹط®طھظ… ظƒظ„ ط­ط¯ط« طھط¯ظ‚ظٹظ‚ ط¨ظ€`integrity_status = "VALID"`
    // ط¨ظ„ط§ ط£ظٹ ظپط­طµ ظ„ظ„ط³ظ„ط³ظ„ط©طŒ ظپطھط¹ط±ط¶ ط§ظ„ظ„ظˆط­ط© ط¶ظ…ط§ظ†ط§ظ‹ ظ„ظ… ظٹظڈط®طھط¨ط±. ظ‡ط°ط§ ط§ظ„ط§ط®طھط¨ط§ط± ظٹط¹ط¯ظ‘ظ„ ط­ط¯ط«ط§ظ‹
    // ظپط¹ظ„ط§ظ‹ ظˆظٹط«ط¨طھ ط£ظ† ط§ظ„ط­ط§ظ„ط© طھطھط¨ط¹ ط§ظ„ط­ظ‚ظٹظ‚ط© ظ„ط§ ط«ط§ط¨طھط§ظ‹ ظپظٹ ط§ظ„ظ…طµط¯ط±.
    #[test]
    fn dashboard_audit_integrity_is_measured_and_flips_when_the_chain_is_tampered() {
        let state = demo_state();

        let intact = load_dashboard(&state).expect("ظ‚ط±ط§ط،ط© ط§ظ„ظ„ظˆط­ط© ظٹط¬ط¨ ط£ظ† طھظ†ط¬ط­");
        assert_eq!(intact.audit_chain_integrity, "VALID");
        assert!(!intact.recent_audit_events.is_empty());
        assert!(intact
            .recent_audit_events
            .iter()
            .all(|event| event.integrity_status == "VALID"));

        {
            let connection = state
                .connection
                .lock()
                .expect("ط§ظ„ظ‚ظپظ„ ظٹط¬ط¨ ط£ظ† ظٹطھط§ط­");
            connection
                .execute("UPDATE audit_events SET event_type = 'TAMPERED'", [])
                .expect("ط§ظ„طھط¹ط¯ظٹظ„ ظٹط¬ط¨ ط£ظ† ظٹظ†ط¬ط­");
        }

        let tampered = load_dashboard(&state)
            .expect("ظ‚ط±ط§ط،ط© ط§ظ„ظ„ظˆط­ط© ط¨ط¹ط¯ ط§ظ„طھط¹ط¯ظٹظ„ ظٹط¬ط¨ ط£ظ† طھظ†ط¬ط­");
        assert_eq!(tampered.audit_chain_integrity, "INVALID");
        assert!(tampered
            .recent_audit_events
            .iter()
            .all(|event| event.integrity_status == "INVALID"));
    }

    // ط§ظ„ظ„ظˆط­ط© ظپظٹ ط¨ظٹط¦ط© ط¥ظ†طھط§ط¬ ظ†ط¸ظٹظپط©: ظ„ط§ ط§ظ„طھط²ط§ظ…ط§طھ â‡’ ظ„ط§ ظ†ط³ط¨ط© ظ…ط®طھظ„ظ‚ط©طŒ ظˆظ„ط§ ظˆط³ظ… ط§طµط·ظ†ط§ط¹ظٹ.
    #[test]
    fn a_fresh_production_dashboard_reports_no_measured_readiness() {
        let connection = Connection::open_in_memory()
            .expect("ظ‚ط§ط¹ط¯ط© ط§ط®طھط¨ط§ط± ط§ظ„ط°ط§ظƒط±ط© ظٹط¬ط¨ ط£ظ† طھظپطھط­");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("طھظپط¹ظٹظ„ ظ…ظپط§طھظٹط­ ط§ظ„ط£ط¬ظ†ط¨ط© ظٹط¬ط¨ ط£ظ† ظٹظ†ط¬ط­");
        let state = AppState::test_instance(connection, "tenant-unprovisioned".to_owned(), "");
        crate::infrastructure::database::initialize_workspace_database(
            &state.connection.lock().expect("ط§ظ„ظ‚ظپظ„"),
        )
        .expect("طھظ‡ظٹط¦ط© ظ…ط³ط§ط­ط© ط§ظ„ط¹ظ…ظ„ ظٹط¬ط¨ ط£ظ† طھظ†ط¬ط­");
        bootstrap_production(
            &state,
            BootstrapProductionInput {
                organization_name: "Acme Industrial Group".to_owned(),
                // GAP-05-OPEN: بيانات المؤسسة تُعلن في الطلب لا في الخلف.
                country_code: "SA".to_owned(),
                industry: "Industrial Manufacturing".to_owned(),
                admin_display_name: "Acme Administrator".to_owned(),
                admin_email: "admin@acme.example".to_owned(),
                password: "Correct!Horse#Battery7".to_owned(),
            },
        )
        .expect("ط¥ظ‚ظ„ط§ط¹ ط§ظ„ط¥ظ†طھط§ط¬ ظٹط¬ط¨ ط£ظ† ظٹظ†ط¬ط­");

        let dashboard = load_dashboard(&state).expect("ظ‚ط±ط§ط،ط© ط§ظ„ظ„ظˆط­ط© ظٹط¬ط¨ ط£ظ† طھظ†ط¬ط­");
        assert!(!dashboard.readiness.sufficient_data);
        assert_eq!(dashboard.readiness.readiness_score, None);
        assert_eq!(dashboard.readiness.eligible_obligations, 0);
        assert!(dashboard.obligations.is_empty());
        // ظˆط³ظˆظ… ط§ظ„ظ…ظ†ط´ط£ طھطھط¨ط¹ ط§ظ„ظˆط¶ط¹ ط§ظ„ظپط¹ظ„ظٹ: ظ„ط§ ظˆط³ظ… آ«طھط±ظƒظٹط¨ظٹآ» ط¹ظ„ظ‰ ط¨ظٹط¦ط© ط¥ظ†طھط§ط¬.
        assert!(!dashboard.readiness.disclaimer.contains("SYNTHETIC"));
        assert_eq!(dashboard.data_mode, "PRODUCTION");
    }
}

// =====================================================================
// جولة الإغلاق النهائية — سياسة عد الالتزامات المؤرشفة والأدلة المنتهية
// =====================================================================
#[cfg(test)]
mod closure_tests {
    use chrono::Utc;
    use rusqlite::Connection;

    use super::load_dashboard;
    use crate::infrastructure::database::{
        initialize_demo_database, AppState, DEMO_OPERATOR_USER_ID, DEMO_TENANT_ID,
    };

    fn demo_state() -> AppState {
        let connection = Connection::open_in_memory().expect("قاعدة الاختبار يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
        initialize_demo_database(&connection).expect("تهيئة قاعدة الديمو يجب أن تنجح");
        AppState::test_instance(connection, DEMO_TENANT_ID, DEMO_OPERATOR_USER_ID)
    }

    // الالتزامات المؤرشفة كانت تدخل في المقام والبنود الحرجة وقائمة اللوحة،
    // فينخفض مؤشر الجاهزية بسبب سجلات أغلقها المستخدم بنفسه. السياسة المعلنة
    // الآن: الأرشيف خارج كل مؤشر حالة نشط، ويُحتسب ويُعرض صراحةً.
    #[test]
    fn archived_obligations_leave_active_indicators_and_are_reported() {
        let state = demo_state();
        let active_total: i64 = {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .query_row(
                    "SELECT COUNT(*) FROM obligations WHERE tenant_id = ?1 AND archived_at IS NULL",
                    [DEMO_TENANT_ID],
                    |row| row.get(0),
                )
                .expect("العدّ يجب أن ينجح")
        };
        assert!(active_total > 0, "الديمو المبذور يجب أن يحمل التزامات نشطة");

        let before = load_dashboard(&state).expect("قراءة اللوحة قبل الأرشفة");
        assert_eq!(
            before.readiness.archived_obligations, 0,
            "لا التزامات مؤرشفة قبل الأرشفة"
        );

        let now = Utc::now().to_rfc3339();
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute(
                    "UPDATE obligations SET archived_at = ?1, archived_by = 'test', version = version + 1 WHERE tenant_id = ?2 AND archived_at IS NULL",
                    (&now, DEMO_TENANT_ID),
                )
                .expect("أرشفة الاختبار يجب أن تنجح");
        }

        let after = load_dashboard(&state).expect("قراءة اللوحة بعد الأرشفة");
        assert_eq!(after.readiness.archived_obligations as i64, active_total);
        assert_eq!(
            after.readiness.eligible_obligations, 0,
            "الأرشيف لا يبقى في مقام الجاهزية"
        );
        assert_eq!(after.readiness.readiness_score, None);
        assert!(!after.readiness.sufficient_data);
        assert!(after.obligations.is_empty());
        assert_eq!(after.readiness.open_critical_items, 0);
    }

    // الدليل المنتهي لا يغطي التزاماً: كان `load_evidence_covered` يقرأ الحالة
    // المخزَّنة وحدها، فدليل VALID انقضى أجله يبقى «تغطية» كاملة إلى الأبد.
    #[test]
    fn expired_evidence_stops_counting_towards_evidence_completeness() {
        let state = demo_state();
        let obligation_id: String = {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .query_row(
                    "SELECT id FROM obligations WHERE tenant_id = ?1 AND archived_at IS NULL AND applicability = 'APPLIES' LIMIT 1",
                    [DEMO_TENANT_ID],
                    |row| row.get(0),
                )
                .expect("يجب أن يوجد التزام منطبق")
        };
        let baseline = load_dashboard(&state).expect("القراءة الأساسية");
        let baseline_completeness = baseline.readiness.evidence_completeness;

        let now = Utc::now().to_rfc3339();
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute(
                    "INSERT INTO evidence_items (id, tenant_id, obligation_id, classification, status, content_hash, version, created_at, updated_at, label, expires_at) VALUES ('evidence-closure-future', ?1, ?2, 'INTERNAL', 'VALID', 'hash-closure-future', 1, ?3, ?3, 'Future evidence', '2999-01-01T00:00:00Z')",
                    (DEMO_TENANT_ID, &obligation_id, &now),
                )
                .expect("إدراج دليل ساري يجب أن ينجح");
        }
        let with_valid = load_dashboard(&state).expect("قراءة بعد الدليل الساري");
        assert!(
            with_valid.readiness.evidence_completeness.unwrap_or(0)
                > baseline_completeness.unwrap_or(0),
            "الدليل الساري يجب أن يرفع الاكتمال"
        );

        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute(
                    "UPDATE evidence_items SET expires_at = '2000-01-01T00:00:00Z' WHERE id = 'evidence-closure-future'",
                    [],
                )
                .expect("تعديل الأجل يجب أن ينجح");
        }
        let after_expiry = load_dashboard(&state).expect("قراءة بعد الانتهاء");
        assert_eq!(
            after_expiry.readiness.evidence_completeness, baseline_completeness,
            "الدليل المنتهي يجب ألا يبقى تغطية"
        );
    }
}
