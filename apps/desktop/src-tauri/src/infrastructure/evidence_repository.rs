use chrono::Utc;
use rusqlite::{params, Connection, Transaction};

use crate::domain::{
    error::AppError,
    models::{
        parse_data_mode, parse_evidence_classification, parse_evidence_status, EvidenceData,
        EvidenceRecord, EvidenceSummary,
    },
};

// الحالات التي يُعتبر فيها الدليل "مقبولاً" قبل انقضاء أجله. الدليل المنتهي
// يهبط منها إلى EXPIRED في القراءة المشتقة دون تعديل الصف المخزَّن.
const ACCEPTED_EVIDENCE_STATUSES: [&str; 4] = ["UPLOADED", "PROCESSING", "ANALYZED", "VALID"];

// تصنيف صريح لسجلات ما قبل عمود `registered_by` (0026): لا هوية مقدّم معروفة،
// ولا تُنسب إلى مستخدم مخترع. يُعرض هذا التصنيف في العقود والواجهة.
pub const REGISTERED_BY_RECORDED: &str = "RECORDED";
pub const REGISTERED_BY_UNKNOWN_LEGACY: &str = "UNKNOWN_LEGACY";

pub fn load_evidence(
    connection: &Connection,
    tenant_id: &str,
    generated_at: &str,
) -> Result<EvidenceData, AppError> {
    let data_mode = connection
        .query_row(
            "SELECT data_mode FROM tenants WHERE id = ?1",
            [tenant_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| AppError::DataAccess)?;

    let summary = load_summary(connection, tenant_id, generated_at)?;
    let items = load_items(connection, tenant_id, generated_at)?;

    Ok(EvidenceData {
        has_data: !items.is_empty(),
        items,
        summary,
        data_mode: parse_data_mode(data_mode)?,
        generated_at: generated_at.to_owned(),
    })
}

// تفاصيل دليل واحد داخل المستأجر النشط — نفس صف قائمة الأدلة بنفس بنية الربط،
// لكن مقيّداً بمعرّف واحد؛ غيابه قرار معلن (ResourceNotFound) لا خطأ تخزين.
pub fn load_evidence_detail(
    connection: &Connection,
    tenant_id: &str,
    evidence_id: &str,
) -> Result<EvidenceRecord, AppError> {
    let now = Utc::now().to_rfc3339();
    let mut statement = connection
        .prepare(&format!("{EVIDENCE_SELECT} AND evidence.id = ?3"))
        .map_err(|_| AppError::DataAccess)?;

    let row = statement
        .query_row(params![tenant_id, &now, evidence_id], map_evidence_row)
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => AppError::ResourceNotFound,
            _ => AppError::DataAccess,
        })?;
    build_evidence_record(row, &now)
}

// قاعدة تجميع حالة دليل الالتزام المستخرجة من مسار التسجيل القائم حرفياً:
// أي دليل موجود يعني 'REQUIRES_REVIEW' (لا استنتاج حالة أعلى بالنيابة)،
// وغياب كل الأدلة يعني 'MISSING'. تُستدعى داخل معاملة الكتابة نفسها فلا تتفرّع الحالة.
pub fn obligation_evidence_status(
    transaction: &Transaction<'_>,
    tenant_id: &str,
    obligation_id: &str,
) -> Result<String, AppError> {
    let has_evidence = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM evidence_items WHERE tenant_id = ?1 AND obligation_id = ?2)",
            params![tenant_id, obligation_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| AppError::DataAccess)?;

    Ok(if has_evidence {
        "REQUIRES_REVIEW"
    } else {
        "MISSING"
    }
    .to_owned())
}

// الملخص يحتسب الانتهاء الفعلي: الصف المكتوب EXPIRED أو الصف الذي انقضى
// `expires_at` وهو في حالة مقبولة. الاعتماد على العمود وحده كان يخفي أدلة
// منتهية ما دام لا مسار يكتب EXPIRED تلقائياً.
fn load_summary(
    connection: &Connection,
    tenant_id: &str,
    now: &str,
) -> Result<EvidenceSummary, AppError> {
    let expired_condition = ACCEPTED_EVIDENCE_STATUSES
        .iter()
        .map(|status| format!("'{status}'"))
        .collect::<Vec<_>>()
        .join(", ");
    connection
        .query_row(
            &format!(
                "SELECT
                    COUNT(*),
                    COALESCE(SUM(CASE WHEN status = 'REQUIRES_REVIEW' THEN 1 ELSE 0 END), 0),
                    COALESCE(SUM(CASE
                        WHEN status = 'EXPIRED' THEN 1
                        WHEN expires_at IS NOT NULL AND expires_at <= ?2
                             AND status IN ({expired_condition}) THEN 1
                        ELSE 0 END), 0)
                 FROM evidence_items
                 WHERE tenant_id = ?1"
            ),
            params![tenant_id, now],
            |row| {
                Ok(EvidenceSummary {
                    total: as_count(row.get(0)?),
                    requires_review: as_count(row.get(1)?),
                    expired: as_count(row.get(2)?),
                })
            },
        )
        .map_err(|_| AppError::DataAccess)
}

// القراءة تعيد الحالة المخزَّنة والحالة الفعلية معاً: الواجهة تعرض الفعلية،
// والسجل يبقى شاهداً على ما كتبه المسار لا على ما استنتجته القراءة.
const EVIDENCE_SELECT: &str = "SELECT
                evidence.id,
                evidence.obligation_id,
                obligation.title,
                evidence.label,
                evidence.classification,
                evidence.status,
                evidence.expires_at,
                evidence.registered_by,
                evidence.created_at,
                evidence.updated_at
             FROM evidence_items evidence
             INNER JOIN obligations obligation
                ON obligation.id = evidence.obligation_id
                AND obligation.tenant_id = evidence.tenant_id
             WHERE evidence.tenant_id = ?1";

type EvidenceRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
);

fn map_evidence_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<EvidenceRow> {
    Ok((
        row.get::<_, String>(0)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, String>(3)?,
        row.get::<_, String>(4)?,
        row.get::<_, String>(5)?,
        row.get::<_, Option<String>>(6)?,
        row.get::<_, Option<String>>(7)?,
        row.get::<_, String>(8)?,
        row.get::<_, String>(9)?,
    ))
}

fn build_evidence_record(row: EvidenceRow, now: &str) -> Result<EvidenceRecord, AppError> {
    let (
        id,
        obligation_id,
        obligation_title,
        label,
        classification,
        stored_status,
        expires_at,
        registered_by,
        created_at,
        updated_at,
    ) = row;
    let is_expired = expires_at.as_deref().is_some_and(|expiry| expiry <= now);
    let status = parse_evidence_status(stored_status.clone())?;
    // هبوط مشتق إلى EXPIRED فقط من الحالات المقبولة؛ حالة INVALID أو
    // REQUIRES_REVIEW المخزَّنة تبقى كما هي لأنها أدق من مجرد الانتهاء.
    let effective_status = if is_expired
        && ACCEPTED_EVIDENCE_STATUSES
            .iter()
            .any(|accepted| *accepted == stored_status)
    {
        parse_evidence_status("EXPIRED".to_owned())?
    } else {
        status.clone()
    };
    let registered_by_state = if registered_by.is_some() {
        REGISTERED_BY_RECORDED
    } else {
        REGISTERED_BY_UNKNOWN_LEGACY
    };
    Ok(EvidenceRecord {
        id,
        obligation_id,
        obligation_title,
        label,
        classification: parse_evidence_classification(classification)?,
        status,
        expires_at,
        effective_status,
        is_expired,
        registered_by,
        registered_by_state: registered_by_state.to_owned(),
        created_at,
        updated_at,
    })
}

fn load_items(
    connection: &Connection,
    tenant_id: &str,
    now: &str,
) -> Result<Vec<EvidenceRecord>, AppError> {
    let mut statement = connection
        .prepare(&format!(
            "{EVIDENCE_SELECT} ORDER BY evidence.updated_at DESC, evidence.id ASC"
        ))
        .map_err(|_| AppError::DataAccess)?;

    let rows = statement
        .query_map(params![tenant_id], map_evidence_row)
        .map_err(|_| AppError::DataAccess)?;

    rows.map(|row| {
        let row = row.map_err(|_| AppError::DataAccess)?;
        build_evidence_record(row, now)
    })
    .collect::<Result<Vec<_>, AppError>>()
}

fn as_count(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
