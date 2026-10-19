use rusqlite::{params, Connection, Transaction};

use crate::domain::{
    error::AppError,
    models::{
        parse_control_effectiveness, parse_control_status, parse_data_mode, ControlDetail,
        ControlEvidenceLink, ControlObligationLink, ControlRecord, ControlRiskLink, ControlStatus,
        ControlSummary, ControlsData,
    },
};

// أنواع روابط الضابط المدعومة. جدول واحد لكل نوع، ودالة واحدة مُعاملة على النوع —
// لا ثلاث دوال شبه متطابقة تتباعد مع الوقت.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlLinkKind {
    Obligation,
    Risk,
    Evidence,
}

impl ControlLinkKind {
    // جدول الرابط + عمود الهدف — ثوابت داخلية لا تصل إليها أي قيمة من الواجهة.
    pub fn link_table(self) -> &'static str {
        match self {
            Self::Obligation => "control_obligations",
            Self::Risk => "control_risks",
            Self::Evidence => "control_evidence",
        }
    }

    pub fn target_column(self) -> &'static str {
        match self {
            Self::Obligation => "obligation_id",
            Self::Risk => "risk_id",
            Self::Evidence => "evidence_id",
        }
    }

    pub fn target_table(self) -> &'static str {
        match self {
            Self::Obligation => "obligations",
            Self::Risk => "risks",
            Self::Evidence => "evidence_items",
        }
    }
}

pub fn load_controls(
    connection: &Connection,
    tenant_id: &str,
    generated_at: &str,
) -> Result<ControlsData, AppError> {
    let data_mode = connection
        .query_row(
            "SELECT data_mode FROM tenants WHERE id = ?1",
            [tenant_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| AppError::DataAccess)?;

    let summary = load_summary(connection, tenant_id)?;
    let items = load_items(connection, tenant_id)?;

    Ok(ControlsData {
        has_data: !items.is_empty(),
        items,
        summary,
        data_mode: parse_data_mode(data_mode)?,
        generated_at: generated_at.to_owned(),
    })
}

fn load_summary(connection: &Connection, tenant_id: &str) -> Result<ControlSummary, AppError> {
    connection
        .query_row(
            "SELECT
                COUNT(*),
                COALESCE(SUM(CASE WHEN status = 'ACTIVE' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'INACTIVE' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN status = 'DISABLED' THEN 1 ELSE 0 END), 0)
             FROM controls
             WHERE tenant_id = ?1",
            [tenant_id],
            |row| {
                Ok(ControlSummary {
                    total: as_count(row.get(0)?),
                    active: as_count(row.get(1)?),
                    inactive: as_count(row.get(2)?),
                    disabled: as_count(row.get(3)?),
                })
            },
        )
        .map_err(|_| AppError::DataAccess)
}

fn load_items(connection: &Connection, tenant_id: &str) -> Result<Vec<ControlRecord>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT
                control.id,
                control.title,
                control.status,
                control.category,
                control.owner,
                control.effectiveness,
                control.last_assessed_at,
                (SELECT COUNT(*) FROM control_obligations link
                   WHERE link.control_id = control.id AND link.tenant_id = control.tenant_id),
                (SELECT COUNT(*) FROM control_risks link
                   WHERE link.control_id = control.id AND link.tenant_id = control.tenant_id),
                (SELECT COUNT(*) FROM control_evidence link
                   WHERE link.control_id = control.id AND link.tenant_id = control.tenant_id),
                control.created_at,
                control.updated_at
             FROM controls control
             WHERE control.tenant_id = ?1
             ORDER BY control.updated_at DESC, control.id ASC",
        )
        .map_err(|_| AppError::DataAccess)?;

    let rows = statement
        .query_map(params![tenant_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, String>(10)?,
                row.get::<_, String>(11)?,
            ))
        })
        .map_err(|_| AppError::DataAccess)?;

    rows.map(|row| {
        let (
            id,
            title,
            status,
            category,
            owner,
            effectiveness,
            last_assessed_at,
            linked_obligations,
            linked_risks,
            linked_evidence,
            created_at,
            updated_at,
        ) = row.map_err(|_| AppError::DataAccess)?;
        Ok(ControlRecord {
            id,
            title,
            status: parse_control_status(status)?,
            category,
            owner,
            effectiveness: parse_control_effectiveness(effectiveness)?,
            last_assessed_at,
            linked_obligation_count: as_count(linked_obligations),
            linked_risk_count: as_count(linked_risks),
            linked_evidence_count: as_count(linked_evidence),
            created_at,
            updated_at,
        })
    })
    .collect::<Result<Vec<_>, AppError>>()
}

// تفاصيل ضابط واحد داخل المستأجر النشط: صف الضابط ثم روابطه الثلاثة بترتيب ثابت،
// وكل استعلام مقيّد بالمستأجر في جملته (لا تصفية بعد القراءة).
pub fn load_control_detail(
    connection: &Connection,
    tenant_id: &str,
    control_id: &str,
) -> Result<ControlDetail, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT
                control.id,
                control.title,
                control.status,
                control.category,
                control.owner,
                control.effectiveness,
                control.last_assessed_at,
                control.assessment_note,
                (SELECT COUNT(*) FROM control_obligations link
                   WHERE link.control_id = control.id AND link.tenant_id = control.tenant_id),
                (SELECT COUNT(*) FROM control_risks link
                   WHERE link.control_id = control.id AND link.tenant_id = control.tenant_id),
                (SELECT COUNT(*) FROM control_evidence link
                   WHERE link.control_id = control.id AND link.tenant_id = control.tenant_id),
                control.created_at,
                control.updated_at
             FROM controls control
             WHERE control.tenant_id = ?1 AND control.id = ?2",
        )
        .map_err(|_| AppError::DataAccess)?;

    let detail = statement
        .query_row(params![tenant_id, control_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
            ))
        })
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => AppError::ResourceNotFound,
            _ => AppError::DataAccess,
        })?;

    let (
        id,
        title,
        status,
        category,
        owner,
        effectiveness,
        last_assessed_at,
        assessment_note,
        linked_obligations,
        linked_risks,
        linked_evidence,
        created_at,
        updated_at,
    ) = detail;

    Ok(ControlDetail {
        control: ControlRecord {
            id,
            title,
            status: parse_control_status(status)?,
            category,
            owner,
            effectiveness: parse_control_effectiveness(effectiveness)?,
            last_assessed_at,
            linked_obligation_count: as_count(linked_obligations),
            linked_risk_count: as_count(linked_risks),
            linked_evidence_count: as_count(linked_evidence),
            created_at,
            updated_at,
        },
        assessment_note,
        linked_obligations: load_obligation_links(connection, tenant_id, control_id)?,
        linked_risks: load_risk_links(connection, tenant_id, control_id)?,
        linked_evidence: load_evidence_links(connection, tenant_id, control_id)?,
    })
}

fn load_obligation_links(
    connection: &Connection,
    tenant_id: &str,
    control_id: &str,
) -> Result<Vec<ControlObligationLink>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT target.id, target.title, target.severity, target.status, link.created_at
             FROM control_obligations link
             INNER JOIN obligations target
                ON target.id = link.obligation_id AND target.tenant_id = link.tenant_id
             WHERE link.tenant_id = ?1 AND link.control_id = ?2
             ORDER BY link.created_at ASC, target.id ASC",
        )
        .map_err(|_| AppError::DataAccess)?;
    let rows = statement
        .query_map(params![tenant_id, control_id], |row| {
            Ok(ControlObligationLink {
                obligation_id: row.get(0)?,
                title: row.get(1)?,
                severity: row.get(2)?,
                status: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|_| AppError::DataAccess)?;
    rows.map(|row| row.map_err(|_| AppError::DataAccess))
        .collect::<Result<Vec<_>, AppError>>()
}

fn load_risk_links(
    connection: &Connection,
    tenant_id: &str,
    control_id: &str,
) -> Result<Vec<ControlRiskLink>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT target.id, target.title, target.severity, target.status, link.created_at
             FROM control_risks link
             INNER JOIN risks target
                ON target.id = link.risk_id AND target.tenant_id = link.tenant_id
             WHERE link.tenant_id = ?1 AND link.control_id = ?2
             ORDER BY link.created_at ASC, target.id ASC",
        )
        .map_err(|_| AppError::DataAccess)?;
    let rows = statement
        .query_map(params![tenant_id, control_id], |row| {
            Ok(ControlRiskLink {
                risk_id: row.get(0)?,
                title: row.get(1)?,
                severity: row.get(2)?,
                status: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|_| AppError::DataAccess)?;
    rows.map(|row| row.map_err(|_| AppError::DataAccess))
        .collect::<Result<Vec<_>, AppError>>()
}

fn load_evidence_links(
    connection: &Connection,
    tenant_id: &str,
    control_id: &str,
) -> Result<Vec<ControlEvidenceLink>, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT target.id, target.label, target.status, target.classification, link.created_at
             FROM control_evidence link
             INNER JOIN evidence_items target
                ON target.id = link.evidence_id AND target.tenant_id = link.tenant_id
             WHERE link.tenant_id = ?1 AND link.control_id = ?2
             ORDER BY link.created_at ASC, target.id ASC",
        )
        .map_err(|_| AppError::DataAccess)?;
    let rows = statement
        .query_map(params![tenant_id, control_id], |row| {
            Ok(ControlEvidenceLink {
                evidence_id: row.get(0)?,
                label: row.get(1)?,
                status: row.get(2)?,
                classification: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|_| AppError::DataAccess)?;
    rows.map(|row| row.map_err(|_| AppError::DataAccess))
        .collect::<Result<Vec<_>, AppError>>()
}

pub fn insert_control(
    transaction: &Transaction<'_>,
    id: &str,
    tenant_id: &str,
    title: &str,
    category: Option<&str>,
    owner: Option<&str>,
    created_at: &str,
) -> Result<(), AppError> {
    transaction
        .execute(
            "INSERT INTO controls (id, tenant_id, title, status, category, owner, effectiveness, last_assessed_at, assessment_note, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'ACTIVE', ?4, ?5, 'NOT_ASSESSED', NULL, NULL, ?6, ?6)",
            params![id, tenant_id, title, category, owner, created_at],
        )
        .map_err(|_| AppError::StorageOperation)?;
    Ok(())
}

// تحديث الحقول الوصفية فقط: الحالة تُدار عبر set_control_status، والتقييم عبر record_assessment.
pub fn update_control(
    transaction: &Transaction<'_>,
    tenant_id: &str,
    control_id: &str,
    title: &str,
    category: Option<&str>,
    owner: Option<&str>,
    updated_at: &str,
) -> Result<usize, AppError> {
    transaction
        .execute(
            "UPDATE controls
                SET title = ?1, category = ?2, owner = ?3, updated_at = ?4
              WHERE id = ?5 AND tenant_id = ?6",
            params![title, category, owner, updated_at, control_id, tenant_id],
        )
        .map_err(|_| AppError::StorageOperation)
}

pub fn set_control_status(
    transaction: &Transaction<'_>,
    tenant_id: &str,
    control_id: &str,
    status: &ControlStatus,
    updated_at: &str,
) -> Result<usize, AppError> {
    transaction
        .execute(
            "UPDATE controls SET status = ?1, updated_at = ?2 WHERE id = ?3 AND tenant_id = ?4",
            params![
                crate::domain::models::control_status_code(status),
                updated_at,
                control_id,
                tenant_id
            ],
        )
        .map_err(|_| AppError::StorageOperation)
}

// تقييم الفعالية: يسجّل القيمة والطابع والملاحظة معاً — لا تقييم بلا طابع زمني.
pub fn record_control_assessment(
    transaction: &Transaction<'_>,
    tenant_id: &str,
    control_id: &str,
    effectiveness: &str,
    assessment_note: Option<&str>,
    assessed_at: &str,
) -> Result<usize, AppError> {
    transaction
        .execute(
            "UPDATE controls
                SET effectiveness = ?1, assessment_note = ?2, last_assessed_at = ?3, updated_at = ?3
              WHERE id = ?4 AND tenant_id = ?5",
            params![
                effectiveness,
                assessment_note,
                assessed_at,
                control_id,
                tenant_id
            ],
        )
        .map_err(|_| AppError::StorageOperation)
}

// إدراج رابط واحد: INSERT OR IGNORE + المفتاح المركّب يجعل تكرار نفس الزوج عملية محايدة (idempotent)،
// والقيمة المُرجعة (عدد الصفوف المتأثرة) هي ما يقرر إن كان الرابط جديداً فعلاً.
pub fn insert_control_link(
    transaction: &Transaction<'_>,
    tenant_id: &str,
    control_id: &str,
    kind: ControlLinkKind,
    target_id: &str,
    created_at: &str,
) -> Result<usize, AppError> {
    let sql = format!(
        "INSERT OR IGNORE INTO {} (control_id, {}, tenant_id, created_at) VALUES (?1, ?2, ?3, ?4)",
        kind.link_table(),
        kind.target_column()
    );
    transaction
        .execute(&sql, params![control_id, target_id, tenant_id, created_at])
        .map_err(|_| AppError::StorageOperation)
}

// حذف رابط واحد داخل المستأجر نفسه — صفر صفوف يعني أن الرابط غير موجود.
pub fn delete_control_link(
    transaction: &Transaction<'_>,
    tenant_id: &str,
    control_id: &str,
    kind: ControlLinkKind,
    target_id: &str,
) -> Result<usize, AppError> {
    let sql = format!(
        "DELETE FROM {} WHERE control_id = ?1 AND {} = ?2 AND tenant_id = ?3",
        kind.link_table(),
        kind.target_column()
    );
    transaction
        .execute(&sql, params![control_id, target_id, tenant_id])
        .map_err(|_| AppError::StorageOperation)
}

// وجود الكيان الهدف داخل مستأجر الطلب — شرط لكل ربط قبل أي إدراج.
pub fn target_exists(
    transaction: &Transaction<'_>,
    tenant_id: &str,
    kind: ControlLinkKind,
    target_id: &str,
) -> Result<bool, AppError> {
    let sql = format!(
        "SELECT EXISTS(SELECT 1 FROM {} WHERE id = ?1 AND tenant_id = ?2)",
        kind.target_table()
    );
    transaction
        .query_row(&sql, params![target_id, tenant_id], |row| {
            row.get::<_, bool>(0)
        })
        .map_err(|_| AppError::DataAccess)
}

pub fn control_exists(
    transaction: &Transaction<'_>,
    tenant_id: &str,
    control_id: &str,
) -> Result<bool, AppError> {
    transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM controls WHERE id = ?1 AND tenant_id = ?2)",
            params![control_id, tenant_id],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| AppError::DataAccess)
}

pub fn link_count(
    transaction: &Transaction<'_>,
    tenant_id: &str,
    control_id: &str,
    kind: ControlLinkKind,
) -> Result<u32, AppError> {
    let sql = format!(
        "SELECT COUNT(*) FROM {} WHERE control_id = ?1 AND tenant_id = ?2",
        kind.link_table()
    );
    transaction
        .query_row(&sql, params![control_id, tenant_id], |row| {
            row.get::<_, i64>(0)
        })
        .map(as_count)
        .map_err(|_| AppError::DataAccess)
}

pub fn touch_control(
    transaction: &Transaction<'_>,
    tenant_id: &str,
    control_id: &str,
    updated_at: &str,
) -> Result<usize, AppError> {
    transaction
        .execute(
            "UPDATE controls SET updated_at = ?1 WHERE id = ?2 AND tenant_id = ?3",
            params![updated_at, control_id, tenant_id],
        )
        .map_err(|_| AppError::StorageOperation)
}

// قراءة الحالة الحالية داخل المعاملة — أساس مقارنة "تغيّرت فعلاً؟" قبل التدقيق.
pub fn current_status(
    transaction: &Transaction<'_>,
    tenant_id: &str,
    control_id: &str,
) -> Result<ControlStatus, AppError> {
    transaction
        .query_row(
            "SELECT status FROM controls WHERE id = ?1 AND tenant_id = ?2",
            params![control_id, tenant_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => AppError::ResourceNotFound,
            _ => AppError::DataAccess,
        })
        .and_then(parse_control_status)
}

pub fn as_count(value: i64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
