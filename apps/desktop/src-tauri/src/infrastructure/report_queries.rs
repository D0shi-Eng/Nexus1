use std::collections::BTreeMap;

use rusqlite::{params, params_from_iter, Connection, Rows};

use crate::{
    domain::{
        error::AppError,
        reports::{
            report_scope, ReportColumn, ReportId, ReportProvenance, ReportSection,
            ReportSourceTable, LOCAL_SUMMARY_DISCLAIMER, MAX_ROWS_PER_SECTION,
        },
    },
    infrastructure::database::AppState,
};

// =====================================================================
// استعلامات التقارير — كل صف هنا مقروء من جدول حقيقي داخل نطاق المستأجر
// النشط حصراً. لا رقم مُختلق، ولا صف بديل، ولا درجة امتثال مُصنَّعة.
//
// أربع ضمانات بنيوية في هذا الملف:
//   1) كل جملة SQL تحمل `tenant_id = ?1` صريحاً على الجدول المالك، وكل وسيط
//      مُرقَّم صراحةً بترتيب ظهوره — لا خلط بين `?` و`?N` فينحرف الربط.
//   2) الجداول تُفحص وجودها قبل الاستعلام، فلا ينكسر تقرير على قاعدة
//      طُبِّقت عليها هجرات أقل (المشغّلات والجداول تأتي من 0007 و0022).
//   3) أعمدة كل قسم تُعرَّف بثلاثة أجزاء: تعبير SQL + مفتاح العقد + التسمية
//      العربية. المفتاح السلكي متطابق مع العقد المجمّد، وتعبير SQL هو ما
//      يُنفَّذ فعلاً — لا خلط بين الاسمين.
//   4) الاقتطاع لا يكذب: `total_rows` يُقرأ بعدّ كامل منفصل، والصفوف
//      المُعادة محدودة بالحدّ الأعلى المعلن.
// =====================================================================

/// الحدّ الأعلى المعلن — نقطة تصدير واحدة للكتالوج والتوليد معاً.
pub fn max_rows_per_section() -> usize {
    MAX_ROWS_PER_SECTION
}

/// عمود قسم: تعبير SQL المنفَّذ + مفتاح العقد + التسمية العربية المعروضة.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectColumn {
    expression: &'static str,
    key: &'static str,
    label: &'static str,
}

const fn column(expression: &'static str, key: &'static str, label: &'static str) -> SelectColumn {
    SelectColumn {
        expression,
        key,
        label,
    }
}

/// وضع قراءة الصفوف: صفحة محدودة بعددها الحقيقي المعلن.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowMode {
    MaxRows(usize),
}

/// عقد داخلي لبناء قسم واحد: columns + ORDER BY + WHERE إضافي + وسائط.
struct SectionPlan {
    title: &'static str,
    columns: &'static [SelectColumn],
    from_where: &'static str,
    order_by: &'static str,
    extra_arguments: Vec<String>,
}

impl SectionPlan {
    fn new(
        title: &'static str,
        columns: &'static [SelectColumn],
        from_where: &'static str,
        order_by: &'static str,
    ) -> Self {
        Self {
            title,
            columns,
            from_where,
            order_by,
            extra_arguments: Vec::new(),
        }
    }

    fn with_arguments(mut self, arguments: Vec<String>) -> Self {
        self.extra_arguments = arguments;
        self
    }
}

/// نتيجة قراءة أقسام تقرير واحد مع منشئه.
pub struct LoadedReport {
    pub sections: Vec<ReportSection>,
    pub source_tables: Vec<ReportSourceTable>,
    pub data_mode: String,
}

/// واجهة طبقة التطبيق الوحيدة: تقرير كامل بالأقسام والمنشأ.
pub fn load_report(
    state: &AppState,
    report_id: ReportId,
    max_rows: usize,
) -> Result<LoadedReport, AppError> {
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    load_report_with(&connection, state, report_id, max_rows)
}

/// الواجهة القابلة للاختبار (اتصال مُستعار) — نفس المسار تماماً.
pub fn load_report_with(
    connection: &Connection,
    state: &AppState,
    report_id: ReportId,
    max_rows: usize,
) -> Result<LoadedReport, AppError> {
    let tenant_id = state.tenant_id();
    if tenant_id.trim().is_empty() {
        // بلا مستأجر نشط لا يُقرأ شيء — فشل مغلق قبل أي استعلام.
        return Err(AppError::AuthorizationDenied);
    }

    let source_tables = report_scope(report_id).source_tables;
    verify_source_tables(connection, source_tables)?;

    let sections = build_sections(connection, &tenant_id, report_id, max_rows)?;
    Ok(LoadedReport {
        sections,
        source_tables: source_tables.to_vec(),
        data_mode: read_data_mode(connection, &tenant_id)?,
    })
}

/// منشأ التقرير: الجداول المستعملة فعلاً + اللحظة الحقيقية + وسم صريح.
/// `tenants` تُدرج دائماً لأن وضع البيانات نفسه يُقرأ منها في كل تقرير.
pub fn build_provenance(
    source_tables: &[ReportSourceTable],
    data_mode: &str,
    generated_at: &str,
) -> ReportProvenance {
    let mut tables: Vec<String> = source_tables
        .iter()
        .map(|table| table.as_str().to_owned())
        .collect();
    for metadata_table in [ReportSourceTable::Tenants] {
        let name = metadata_table.as_str().to_owned();
        if !tables.contains(&name) {
            tables.push(name);
        }
    }
    ReportProvenance {
        data_mode: data_mode.to_owned(),
        source_tables: tables,
        generated_at: generated_at.to_owned(),
        disclaimer: LOCAL_SUMMARY_DISCLAIMER.to_owned(),
    }
}

// ===== بناء الأقسام لتقرير بعينه =====

fn build_sections(
    connection: &Connection,
    tenant_id: &str,
    report_id: ReportId,
    max_rows: usize,
) -> Result<Vec<ReportSection>, AppError> {
    match report_id {
        ReportId::ObligationStatus => obligation_status_sections(connection, tenant_id, max_rows),
        ReportId::RiskRegister => risk_register_sections(connection, tenant_id, max_rows),
        ReportId::EvidenceRegister => evidence_register_sections(connection, tenant_id, max_rows),
        ReportId::ControlEffectiveness => {
            control_effectiveness_sections(connection, tenant_id, max_rows)
        }
        ReportId::TaskWorkload => task_workload_sections(connection, tenant_id, max_rows),
        ReportId::AuditActivity => audit_activity_sections(connection, tenant_id, max_rows),
    }
}

// ===== 1) حالة الالتزامات التنظيمية =====

const OBLIGATION_COLUMNS: &[SelectColumn] = &[
    column("obligation.id", "obligationId", "معرّف الالتزام"),
    column("obligation.title", "title", "العنوان"),
    column("obligation.owner", "owner", "المالك"),
    column("obligation.severity", "severity", "الخطورة"),
    column("obligation.status", "status", "الحالة"),
    column("obligation.applicability", "applicability", "الانطباق"),
    column(
        "obligation.evidence_status",
        "evidenceStatus",
        "حالة الدليل",
    ),
    column(
        "obligation.compliance_status",
        "complianceStatus",
        "حالة الامتثال المُخزَّنة",
    ),
    column("source.authority", "sourceAuthority", "الجهة المصدر"),
    column("obligation.deadline", "deadline", "الموعد النهائي"),
];

const OBLIGATION_STATUS_COLUMNS: &[SelectColumn] = &[
    column("obligation.status", "obligationStatus", "حالة الالتزام"),
    column("COUNT(*)", "obligationCount", "عدد الالتزامات"),
];

const OBLIGATION_SEVERITY_COLUMNS: &[SelectColumn] = &[
    column("obligation.severity", "severity", "الخطورة"),
    column("COUNT(*)", "obligationCount", "عدد الالتزامات"),
];

fn obligation_status_sections(
    connection: &Connection,
    tenant_id: &str,
    max_rows: usize,
) -> Result<Vec<ReportSection>, AppError> {
    let obligations = read_section(
        connection,
        &SectionPlan::new(
            "سجل الالتزامات",
            OBLIGATION_COLUMNS,
            "FROM obligations obligation
               LEFT JOIN regulatory_sources source
                 ON source.id = obligation.source_id
                AND source.tenant_id = obligation.tenant_id
              WHERE obligation.tenant_id = ?1",
            "ORDER BY obligation.deadline IS NULL, obligation.deadline ASC, obligation.id ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    let by_status = read_section(
        connection,
        &SectionPlan::new(
            "توزيع حالات الالتزام",
            OBLIGATION_STATUS_COLUMNS,
            "FROM obligations obligation WHERE obligation.tenant_id = ?1",
            "GROUP BY obligation.status ORDER BY obligation.status ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    let by_severity = read_section(
        connection,
        &SectionPlan::new(
            "توزيع خطورة الالتزامات",
            OBLIGATION_SEVERITY_COLUMNS,
            "FROM obligations obligation WHERE obligation.tenant_id = ?1",
            "GROUP BY obligation.severity ORDER BY obligation.severity ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    Ok(vec![obligations, by_status, by_severity])
}

// ===== 2) سجل المخاطر =====

const RISK_COLUMNS: &[SelectColumn] = &[
    column("risk.id", "riskId", "معرّف الخطر"),
    column("risk.title", "title", "العنوان"),
    column("obligation.title", "obligationTitle", "الالتزام المرتبط"),
    column("risk.severity", "severity", "الخطورة"),
    column("risk.likelihood", "likelihood", "الاحتمال"),
    column("risk.impact", "impact", "الأثر"),
    column("risk.status", "status", "الحالة"),
    column("risk.owner", "owner", "المالك"),
];

const RISK_SEVERITY_COLUMNS: &[SelectColumn] = &[
    column("risk.severity", "severity", "الخطورة"),
    column("COUNT(*)", "riskCount", "عدد المخاطر"),
];

const RISK_SCOPE_COLUMNS: &[SelectColumn] = &[
    column("obligation.id", "obligationId", "معرّف الالتزام"),
    column("obligation.title", "obligationTitle", "العنوان"),
    column("obligation.severity", "severity", "الخطورة"),
    column("obligation.status", "status", "حالة الالتزام"),
    column(
        "(SELECT COUNT(*) FROM risks risk WHERE risk.tenant_id = ?1 AND risk.obligation_id = obligation.id)",
        "riskCount",
        "عدد المخاطر المسجلة",
    ),
];

fn risk_register_sections(
    connection: &Connection,
    tenant_id: &str,
    max_rows: usize,
) -> Result<Vec<ReportSection>, AppError> {
    let risks = read_section(
        connection,
        &SectionPlan::new(
            "سجل المخاطر",
            RISK_COLUMNS,
            "FROM risks risk
               INNER JOIN obligations obligation
                 ON obligation.id = risk.obligation_id
                AND obligation.tenant_id = risk.tenant_id
              WHERE risk.tenant_id = ?1",
            "ORDER BY CASE risk.severity
                        WHEN 'CRITICAL' THEN 0
                        WHEN 'HIGH' THEN 1
                        WHEN 'MEDIUM' THEN 2
                        WHEN 'LOW' THEN 3
                        ELSE 4
                      END ASC, risk.id ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    let by_severity = read_section(
        connection,
        &SectionPlan::new(
            "توزيع خطورة المخاطر",
            RISK_SEVERITY_COLUMNS,
            "FROM risks risk WHERE risk.tenant_id = ?1",
            "GROUP BY risk.severity ORDER BY risk.severity ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    let obligation_scope = read_section(
        connection,
        &SectionPlan::new(
            "نطاق الالتزامات المرتبطة بالمخاطر",
            RISK_SCOPE_COLUMNS,
            "FROM obligations obligation WHERE obligation.tenant_id = ?1",
            "ORDER BY (SELECT COUNT(*) FROM risks risk
                       WHERE risk.tenant_id = ?1 AND risk.obligation_id = obligation.id) DESC,
                      obligation.id ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    Ok(vec![risks, by_severity, obligation_scope])
}

// ===== 3) سجل الأدلة =====

const EVIDENCE_COLUMNS: &[SelectColumn] = &[
    column("evidence.id", "evidenceId", "معرّف الدليل"),
    column("evidence.label", "label", "التسمية"),
    column("obligation.title", "obligationTitle", "الالتزام المرتبط"),
    column("evidence.classification", "classification", "التصنيف"),
    column("evidence.status", "status", "الحالة"),
    column("evidence.version", "version", "الإصدار"),
    column("evidence.expires_at", "expiresAt", "تاريخ الانتهاء"),
    column("evidence.updated_at", "updatedAt", "آخر تحديث"),
];

const EVIDENCE_STATUS_COLUMNS: &[SelectColumn] = &[
    column("evidence.status", "evidenceStatus", "حالة الدليل"),
    column("COUNT(*)", "evidenceCount", "عدد بنود الأدلة"),
];

const OBLIGATION_EVIDENCE_COLUMNS: &[SelectColumn] = &[
    column("obligation.id", "obligationId", "معرّف الالتزام"),
    column("obligation.title", "title", "العنوان"),
    column(
        "obligation.evidence_status",
        "evidenceStatus",
        "حالة دليل الالتزام",
    ),
    column(
        "(SELECT COUNT(*) FROM evidence_items evidence WHERE evidence.tenant_id = ?1 AND evidence.obligation_id = obligation.id)",
        "evidenceCount",
        "عدد بنود الأدلة",
    ),
];

fn evidence_register_sections(
    connection: &Connection,
    tenant_id: &str,
    max_rows: usize,
) -> Result<Vec<ReportSection>, AppError> {
    let evidence = read_section(
        connection,
        &SectionPlan::new(
            "سجل الأدلة",
            EVIDENCE_COLUMNS,
            "FROM evidence_items evidence
               INNER JOIN obligations obligation
                 ON obligation.id = evidence.obligation_id
                AND obligation.tenant_id = evidence.tenant_id
              WHERE evidence.tenant_id = ?1",
            "ORDER BY evidence.updated_at DESC, evidence.id ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    let by_status = read_section(
        connection,
        &SectionPlan::new(
            "توزيع حالات الأدلة",
            EVIDENCE_STATUS_COLUMNS,
            "FROM evidence_items evidence WHERE evidence.tenant_id = ?1",
            "GROUP BY evidence.status ORDER BY evidence.status ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    let obligation_coverage = read_section(
        connection,
        &SectionPlan::new(
            "حالة دليل الالتزامات",
            OBLIGATION_EVIDENCE_COLUMNS,
            "FROM obligations obligation WHERE obligation.tenant_id = ?1",
            "ORDER BY (SELECT COUNT(*) FROM evidence_items evidence
                       WHERE evidence.tenant_id = ?1 AND evidence.obligation_id = obligation.id) DESC,
                      obligation.id ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    Ok(vec![evidence, by_status, obligation_coverage])
}

// ===== 4) فعالية الضوابط =====

const CONTROL_COLUMNS: &[SelectColumn] = &[
    column("control.id", "controlId", "معرّف الضابط"),
    column("control.title", "title", "العنوان"),
    column("control.category", "category", "التصنيف"),
    column("control.owner", "owner", "المالك"),
    column("control.status", "status", "حالة الضابط"),
    column(
        "control.effectiveness",
        "effectiveness",
        "فعالية التقييم المُخزَّنة",
    ),
    column(
        "control.last_assessed_at",
        "lastAssessedAt",
        "لحظة آخر تقييم",
    ),
    column(
        "(SELECT COUNT(*) FROM control_obligations link
           WHERE link.control_id = control.id AND link.tenant_id = control.tenant_id)",
        "obligationLinks",
        "روابط الالتزامات",
    ),
    column(
        "(SELECT COUNT(*) FROM control_risks link
           WHERE link.control_id = control.id AND link.tenant_id = control.tenant_id)",
        "riskLinks",
        "روابط المخاطر",
    ),
    column(
        "(SELECT COUNT(*) FROM control_evidence link
           WHERE link.control_id = control.id AND link.tenant_id = control.tenant_id)",
        "evidenceLinks",
        "روابط الأدلة",
    ),
];

const CONTROL_ASSESSMENT_COLUMNS: &[SelectColumn] = &[
    column("control.effectiveness", "effectiveness", "الفعالية"),
    column("COUNT(*)", "controlCount", "عدد الضوابط"),
];

const CONTROL_COVERAGE_COLUMNS: &[SelectColumn] = &[
    column("control.id", "controlId", "معرّف الضابط"),
    column("control.title", "controlTitle", "عنوان الضابط"),
    column("''", "linkedTargets", "الأهداف المرتبطة"),
    column("''", "totalTargets", "إجمالي الأهداف في نطاق المستأجر"),
];

/// أنواع التغطية المدعومة لجدول ربط الضابط — ثوابت داخلية لا تأتي من العميل.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CoverageKind {
    Obligation,
    Risk,
    Evidence,
}

impl CoverageKind {
    fn link_table(self) -> &'static str {
        match self {
            Self::Obligation => "control_obligations",
            Self::Risk => "control_risks",
            Self::Evidence => "control_evidence",
        }
    }

    fn target_column(self) -> &'static str {
        match self {
            Self::Obligation => "obligation_id",
            Self::Risk => "risk_id",
            Self::Evidence => "evidence_id",
        }
    }

    fn target_table(self) -> &'static str {
        match self {
            Self::Obligation => "obligations",
            Self::Risk => "risks",
            Self::Evidence => "evidence_items",
        }
    }
}

fn control_effectiveness_sections(
    connection: &Connection,
    tenant_id: &str,
    max_rows: usize,
) -> Result<Vec<ReportSection>, AppError> {
    let controls = read_section(
        connection,
        &SectionPlan::new(
            "سجل الضوابط وتقييم فعاليتها",
            CONTROL_COLUMNS,
            "FROM controls control WHERE control.tenant_id = ?1",
            "ORDER BY control.updated_at DESC, control.id ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    let assessment_distribution = read_section(
        connection,
        &SectionPlan::new(
            "توزيع فعالية الضوابط",
            CONTROL_ASSESSMENT_COLUMNS,
            "FROM controls control WHERE control.tenant_id = ?1",
            "GROUP BY control.effectiveness ORDER BY control.effectiveness ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    // خرائط الربط الفعلية: كل ضابط → معرّفات الأهداف المرتبطة به داخل المستأجر.
    let obligation_coverage = coverage_section(
        connection,
        tenant_id,
        max_rows,
        "تغطية الضوابط للالتزامات",
        CoverageKind::Obligation,
    )?;
    let risk_coverage = coverage_section(
        connection,
        tenant_id,
        max_rows,
        "تغطية الضوابط للمخاطر",
        CoverageKind::Risk,
    )?;
    let evidence_coverage = coverage_section(
        connection,
        tenant_id,
        max_rows,
        "تغطية الضوابط للأدلة",
        CoverageKind::Evidence,
    )?;

    Ok(vec![
        controls,
        assessment_distribution,
        obligation_coverage,
        risk_coverage,
        evidence_coverage,
    ])
}

/// قسم تغطية: عدد الروابط الفعلية لكل ضابط (من جدول الربط نفسه، بلا ضرب صفوف)
/// مقابل العدد الكامل للأهداف داخل المستأجر.
fn coverage_section(
    connection: &Connection,
    tenant_id: &str,
    max_rows: usize,
    title: &'static str,
    kind: CoverageKind,
) -> Result<ReportSection, AppError> {
    let links = control_links(connection, tenant_id, kind)?;

    // العدد الكامل للأهداف في المستأجر — استعلام عدّ حقيقي على الجدول الهدف.
    let total_targets: i64 = connection
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} target WHERE target.tenant_id = ?1",
                kind.target_table()
            ),
            params![tenant_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)?;

    let mut statement = connection
        .prepare(
            "SELECT control.id, control.title FROM controls control
              WHERE control.tenant_id = ?1
              ORDER BY control.id ASC",
        )
        .map_err(|_| AppError::DataAccess)?;

    let rows = statement
        .query_map(params![tenant_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|_| AppError::DataAccess)?;

    let mut all_rows: Vec<Vec<String>> = Vec::new();
    for row in rows {
        let (control_id, control_title) = row.map_err(|_| AppError::DataAccess)?;
        let linked_targets = links
            .get(&control_id)
            .map(|targets| {
                let mut unique: Vec<&String> = targets.iter().collect();
                unique.sort();
                unique.dedup();
                unique.len()
            })
            .unwrap_or(0);
        all_rows.push(vec![
            control_id,
            control_title,
            linked_targets.to_string(),
            total_targets.to_string(),
        ]);
    }

    Ok(finalize_section(
        title,
        CONTROL_COVERAGE_COLUMNS,
        all_rows,
        max_rows,
    ))
}

/// يقرأ جدول ربط موجوداً فعلاً: (control_id, target_id) مقيّد بالمستأجر.
fn control_links(
    connection: &Connection,
    tenant_id: &str,
    kind: CoverageKind,
) -> Result<BTreeMap<String, Vec<String>>, AppError> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT link.control_id, link.{} FROM {} link
              WHERE link.tenant_id = ?1
              ORDER BY link.control_id ASC, link.{} ASC",
            kind.target_column(),
            kind.link_table(),
            kind.target_column()
        ))
        .map_err(|_| AppError::DataAccess)?;

    let rows = statement
        .query_map(params![tenant_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|_| AppError::DataAccess)?;

    let mut links: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in rows {
        let (control_id, target_id) = row.map_err(|_| AppError::DataAccess)?;
        links.entry(control_id).or_default().push(target_id);
    }
    Ok(links)
}

// ===== 5) عبء المهام =====

const TASK_COLUMNS: &[SelectColumn] = &[
    column("task.id", "taskId", "معرّف المهمة"),
    column("task.title", "title", "العنوان"),
    column("obligation.title", "obligationTitle", "الالتزام المرتبط"),
    column("task.assignee_user_id", "assigneeUserId", "معرّف المُكلَّف"),
    column("assignee.display_name", "assigneeDisplayName", "اسم المُكلَّف"),
    column("task.priority", "priority", "الأولوية"),
    column("task.status", "status", "الحالة"),
    column("task.due_at", "dueAt", "الموعد النهائي"),
];

const TASK_STATUS_COLUMNS: &[SelectColumn] = &[
    column("task.status", "taskStatus", "حالة المهمة"),
    column("COUNT(*)", "taskCount", "عدد المهام"),
];

const TASK_PRIORITY_COLUMNS: &[SelectColumn] = &[
    column("task.priority", "priority", "الأولوية"),
    column("COUNT(*)", "taskCount", "عدد المهام"),
];

const TASK_WORKLOAD_COLUMNS: &[SelectColumn] = &[
    column("task.assignee_user_id", "assigneeUserId", "معرّف المُكلَّف"),
    column("assignee.display_name", "assigneeDisplayName", "اسم المُكلَّف"),
    column("COUNT(*)", "totalTasks", "إجمالي المهام"),
    column(
        "COALESCE(SUM(CASE WHEN task.status = 'OPEN' THEN 1 ELSE 0 END), 0)",
        "openTasks",
        "مهام مفتوحة",
    ),
    column(
        "COALESCE(SUM(CASE WHEN task.status = 'IN_PROGRESS' THEN 1 ELSE 0 END), 0)",
        "inProgressTasks",
        "قيد التنفيذ",
    ),
    column(
        "COALESCE(SUM(CASE WHEN task.status = 'BLOCKED' THEN 1 ELSE 0 END), 0)",
        "blockedTasks",
        "متعطلة",
    ),
    column(
        "COALESCE(SUM(CASE WHEN task.status = 'COMPLETED' THEN 1 ELSE 0 END), 0)",
        "completedTasks",
        "مكتملة",
    ),
    column(
        // الترقيم صريح (?2/?3): الموضع ?1 محجوز لمستأجر الجملة الخارجية، لأن
        // SQLite يرقّم المعاملات بترتيب ظهورها — فيجب أن يطابق ترتيب الوسائط.
        "(SELECT COUNT(*) FROM tasks overdue
           WHERE overdue.tenant_id = ?1
             AND overdue.assignee_user_id = task.assignee_user_id
             AND overdue.status <> 'COMPLETED'
             AND overdue.due_at IS NOT NULL
             AND overdue.due_at < ?2)",
        "overdueTasks",
        "متأخرة",
    ),
];

fn task_workload_sections(
    connection: &Connection,
    tenant_id: &str,
    max_rows: usize,
) -> Result<Vec<ReportSection>, AppError> {
    let tasks = read_section(
        connection,
        &SectionPlan::new(
            "سجل المهام",
            TASK_COLUMNS,
            "FROM tasks task
               INNER JOIN obligations obligation
                 ON obligation.id = task.obligation_id
                AND obligation.tenant_id = task.tenant_id
               LEFT JOIN local_users assignee
                 ON assignee.id = task.assignee_user_id
                AND assignee.tenant_id = task.tenant_id
              WHERE task.tenant_id = ?1",
            "ORDER BY CASE task.priority
                        WHEN 'CRITICAL' THEN 0
                        WHEN 'HIGH' THEN 1
                        WHEN 'MEDIUM' THEN 2
                        WHEN 'LOW' THEN 3
                        ELSE 4
                      END ASC, task.due_at IS NULL, task.due_at ASC, task.id ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    let by_status = read_section(
        connection,
        &SectionPlan::new(
            "توزيع حالات المهام",
            TASK_STATUS_COLUMNS,
            "FROM tasks task WHERE task.tenant_id = ?1",
            "GROUP BY task.status ORDER BY task.status ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    let by_priority = read_section(
        connection,
        &SectionPlan::new(
            "توزيع أولويات المهام",
            TASK_PRIORITY_COLUMNS,
            "FROM tasks task WHERE task.tenant_id = ?1",
            "GROUP BY task.priority ORDER BY task.priority ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    // عبء كل مُكلَّف فعلي: العدّ من صفوف المهام نفسها، والتأخر بمقارنة حقيقية
    // بالموعد واللحظة الحالية — لا تقدير ولا وسم مُصنَّع.
    let now = chrono::Utc::now().to_rfc3339();
    let workload = read_section(
        connection,
        &SectionPlan::new(
            "عبء العمل لكل مُكلَّف",
            TASK_WORKLOAD_COLUMNS,
            "FROM tasks task
               LEFT JOIN local_users assignee
                 ON assignee.id = task.assignee_user_id
                AND assignee.tenant_id = task.tenant_id
              WHERE task.tenant_id = ?1",
            "GROUP BY task.assignee_user_id
             ORDER BY task.assignee_user_id IS NULL, COUNT(task.id) DESC, task.assignee_user_id ASC",
        )
        .with_arguments(vec![now]),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    Ok(vec![tasks, by_status, by_priority, workload])
}

// ===== 6) النشاط التدقيقي =====

const AUDIT_TYPE_COLUMNS: &[SelectColumn] = &[
    column("event.event_type", "eventType", "نوع الحدث"),
    column("COUNT(*)", "eventCount", "عدد الأحداث"),
    column("MIN(event.occurred_at)", "firstOccurredAt", "أول وقوع"),
    column("MAX(event.occurred_at)", "lastOccurredAt", "آخر وقوع"),
];

const AUDIT_EVENT_COLUMNS: &[SelectColumn] = &[
    column("event.id", "eventId", "معرّف الحدث"),
    column("event.event_type", "eventType", "نوع الحدث"),
    column("event.actor_name", "actorName", "الفاعل"),
    column("event.resource_type", "resourceType", "نوع المورد"),
    column("event.resource_id", "resourceId", "معرّف المورد"),
    column("event.occurred_at", "occurredAt", "لحظة الوقوع"),
];

fn audit_activity_sections(
    connection: &Connection,
    tenant_id: &str,
    max_rows: usize,
) -> Result<Vec<ReportSection>, AppError> {
    let by_type = read_section(
        connection,
        &SectionPlan::new(
            "تجميع أحداث التدقيق حسب النوع",
            AUDIT_TYPE_COLUMNS,
            "FROM audit_events event WHERE event.tenant_id = ?1",
            "GROUP BY event.event_type ORDER BY COUNT(event.id) DESC, event.event_type ASC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    let recent = read_section(
        connection,
        &SectionPlan::new(
            "أحدث أحداث التدقيق",
            AUDIT_EVENT_COLUMNS,
            "FROM audit_events event WHERE event.tenant_id = ?1",
            "ORDER BY event.rowid DESC",
        ),
        tenant_id,
        RowMode::MaxRows(max_rows),
    )?;

    Ok(vec![by_type, recent])
}

// ===== محرّك القسم الواحد =====

/// يبني قسماً من خطة: عدّ كامل حقيقي + صفحة محدودة بالحدّ الأعلى.
fn read_section(
    connection: &Connection,
    plan: &SectionPlan,
    tenant_id: &str,
    mode: RowMode,
) -> Result<ReportSection, AppError> {
    // العدّ الكامل دائماً من الاستعلام نفسه — لا من طول الصفحة المُعادة.
    let total_rows = count_rows(connection, plan, tenant_id)?;

    let rows = match mode {
        RowMode::MaxRows(max_rows) => {
            // وسائط الاستعلام بترتيب الظهور: المستأجر (?1) ثم وسائط الخطة (مثل اللحظة).
            let mut arguments: Vec<String> = vec![tenant_id.to_owned()];
            arguments.extend(plan.extra_arguments.iter().cloned());
            let sql = format!(
                "SELECT {} {}{} LIMIT {}",
                select_list(plan.columns),
                plan.from_where,
                plan.order_by,
                max_rows
            );
            let mut statement = connection.prepare(&sql).map_err(|_| AppError::DataAccess)?;
            let mut result = statement
                .query(params_from_iter(arguments.iter()))
                .map_err(|_| AppError::DataAccess)?;
            collect_rows(&mut result, plan.columns.len())?
        }
    };

    Ok(ReportSection {
        title: plan.title.to_owned(),
        columns: columns_of(plan.columns),
        truncated: total_rows > rows.len(),
        rows,
        total_rows,
    })
}

/// العدّ الكامل الحقيقي للقسم — من نفس `FROM/WHERE/GROUP BY` الذي يقرأ منه القسم.
///
/// تقييدان مقصودان:
///   * بلا `ORDER BY`: الترتيب لا معنى له في عدّ كامل، وحذفه يُبقي الوسائط
///     مطابقة لما هو مُستعمل فعلاً.
///   * بلا وسائط الخطة الإضافية: هي تخصّ أعمدة SELECT وحدها (مثل لحظة حساب
///     المتأخرات)، وتمرير وسيط لا يظهر في الجملة يُرفض من SQLite.
///
/// `GROUP BY` يبقى: `COUNT(*)` على استعلام مُجمَّع يعطي عدد **الصفوف الأصلية**
/// لا عدد المجموعات، فيُغلَّف الاستعلام المُجمَّع لجعل العدّ = عدد الأقسام
/// المُجمَّعة فعلاً. بدونه كان الاقتطاع يُقارن صفحة مجموعات بعدد صفوف خام.
fn count_rows(
    connection: &Connection,
    plan: &SectionPlan,
    tenant_id: &str,
) -> Result<usize, AppError> {
    let source = format!("SELECT 1 {}", plan.from_where);
    let sql = format!("SELECT COUNT(*) FROM ({source}) grouped");
    let mut statement = connection.prepare(&sql).map_err(|_| AppError::DataAccess)?;
    let count: i64 = statement
        .query_row(params![tenant_id], |row| row.get(0))
        .map_err(|_| AppError::DataAccess)?;
    usize::try_from(count).map_err(|_| AppError::DataAccess)
}

/// تجميع صفوف جاهزة (بلا اقتطاع) — يُستعمل لبناء قسم مكتمل من كود Rust.
fn finalize_section(
    title: &'static str,
    columns: &'static [SelectColumn],
    rows: Vec<Vec<String>>,
    max_rows: usize,
) -> ReportSection {
    let total_rows = rows.len();
    let shown: Vec<Vec<String>> = rows.into_iter().take(max_rows).collect();
    ReportSection {
        title: title.to_owned(),
        columns: columns_of(columns),
        truncated: total_rows > shown.len(),
        rows: shown,
        total_rows,
    }
}

fn columns_of(columns: &'static [SelectColumn]) -> Vec<ReportColumn> {
    columns
        .iter()
        .map(|column| ReportColumn {
            key: column.key.to_owned(),
            label: column.label.to_owned(),
        })
        .collect()
}

/// قائمة SELECT لأعمدة القسم — كل عمود بتعبيره الحقيقي ومُعرَّف برقم ثابت.
fn select_list(columns: &[SelectColumn]) -> String {
    columns
        .iter()
        .enumerate()
        .map(|(index, column)| format!("{} AS c{index}", column.expression))
        .collect::<Vec<String>>()
        .join(", ")
}

/// استخراج الصفوف كنصوص مطابقة للعقد (`rows: string[][]`).
fn collect_rows(rows: &mut Rows<'_>, column_count: usize) -> Result<Vec<Vec<String>>, AppError> {
    let mut collected = Vec::new();
    while let Some(row) = rows.next().map_err(|_| AppError::DataAccess)? {
        let mut values = Vec::with_capacity(column_count);
        for index in 0..column_count {
            values.push(read_cell(row, index)?);
        }
        collected.push(values);
    }
    Ok(collected)
}

/// قراءة خلية واحدة: نص كما هو، والعدد الصحيح كنص، والغائب كمحتوى فارغ معلن.
fn read_cell(row: &rusqlite::Row<'_>, index: usize) -> Result<String, AppError> {
    let value = row.get_ref(index).map_err(|_| AppError::DataAccess)?;
    Ok(match value {
        rusqlite::types::ValueRef::Null => String::new(),
        rusqlite::types::ValueRef::Integer(number) => number.to_string(),
        rusqlite::types::ValueRef::Real(number) => number.to_string(),
        rusqlite::types::ValueRef::Text(text) => String::from_utf8_lossy(text).into_owned(),
        rusqlite::types::ValueRef::Blob(_) => String::new(),
    })
}

// ===== فحوص بنيوية قبل أي استعلام =====

/// وجود كل جدول مصدر معلن — غيابه فشل مغلق لا استعلام على جدول مفقود.
fn verify_source_tables(
    connection: &Connection,
    source_tables: &[ReportSourceTable],
) -> Result<(), AppError> {
    for table in source_tables {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
                [table.as_str()],
                |row| row.get(0),
            )
            .map_err(|_| AppError::DataAccess)?;
        if !exists {
            return Err(AppError::DataAccess);
        }
    }
    Ok(())
}

fn read_data_mode(connection: &Connection, tenant_id: &str) -> Result<String, AppError> {
    connection
        .query_row(
            "SELECT data_mode FROM tenants WHERE id = ?1",
            [tenant_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => AppError::AuthorizationDenied,
            _ => AppError::DataAccess,
        })
}

#[cfg(test)]
mod tests {
    use super::{build_sections, load_report_with};
    use crate::application::tests_test_fixture::{Fixture, TENANT_EMPTY};
    use crate::domain::reports::{ReportId, MAX_ROWS_PER_SECTION};

    // كل تقرير من الستة يجب أن يُبنى فعلاً على البذور الحقيقية بلا خطأ تخزين،
    // وبعدد الأقسام المعلن في تعريفات التقارير.
    #[test]
    fn every_report_builds_from_real_seeded_rows_with_its_declared_sections() {
        let fixture = Fixture::new();
        fixture.seed_report_data();
        let connection = fixture.state.connection.lock().expect("القفل يجب أن يتاح");

        let expectations = [
            (ReportId::ObligationStatus, 3_usize),
            (ReportId::RiskRegister, 3),
            (ReportId::EvidenceRegister, 3),
            (ReportId::ControlEffectiveness, 5),
            (ReportId::TaskWorkload, 4),
            (ReportId::AuditActivity, 2),
        ];
        for (report_id, expected_sections) in expectations {
            let loaded = load_report_with(&connection, &fixture.state, report_id, 500)
                .unwrap_or_else(|error| panic!("تقرير {:?} يجب أن يُبنى: {error:?}", report_id));
            assert_eq!(
                loaded.sections.len(),
                expected_sections,
                "عدد أقسام {:?} يجب أن يطابق تعريفه",
                report_id
            );
            assert_eq!(loaded.data_mode, "DEMO_SYNTHETIC");
            assert!(!loaded.source_tables.is_empty());
            assert!(loaded
                .sections
                .iter()
                .all(|section| !section.columns.is_empty()));
        }
    }

    // العدّ المُعلن للقسم المُجمَّع = عدد المجموعات لا عدد الصفوف الخام.
    // هذا ما يجعل «اقتُطع» صادقاً على أقسام GROUP BY.
    #[test]
    fn grouped_sections_declare_the_group_count_not_the_raw_row_count() {
        let fixture = Fixture::new();
        fixture.seed_report_data();
        let connection = fixture.state.connection.lock().expect("القفل يجب أن يتاح");
        let sections = build_sections(
            &connection,
            &fixture.state.tenant_id(),
            ReportId::ControlEffectiveness,
            MAX_ROWS_PER_SECTION,
        )
        .expect("أقسام فعالية الضوابط يجب أن تُبنى");

        let distribution = sections
            .iter()
            .find(|section| section.title == "توزيع فعالية الضوابط")
            .expect("قسم التوزيع يجب أن يكون موجوداً");
        // ضابطان بقيمتي فعالية مختلفتين ⇒ مجموعتان، لا صفّان خامّان فقط.
        assert_eq!(distribution.total_rows, 2);
        assert_eq!(distribution.rows.len(), 2);
        assert!(!distribution.truncated);

        // وبسقف صفّ واحد يصبح الاقتطاع معلناً مع بقاء العدد الحقيقي.
        let truncated = build_sections(
            &connection,
            &fixture.state.tenant_id(),
            ReportId::ControlEffectiveness,
            1,
        )
        .expect("البناء بسقف أصغر يجب أن ينجح");
        let truncated_distribution = truncated
            .iter()
            .find(|section| section.title == "توزيع فعالية الضوابط")
            .expect("قسم التوزيع يجب أن يكون موجوداً");
        assert_eq!(truncated_distribution.total_rows, 2);
        assert_eq!(truncated_distribution.rows.len(), 1);
        assert!(truncated_distribution.truncated);
    }

    // مستأجر بلا صفوف: أقسام فارغة وعدد صفر — لا صفوف بديلة ولا رقم مُختلق.
    #[test]
    fn a_tenant_without_rows_reports_zero_without_inventing_rows() {
        let fixture = Fixture::new();
        fixture.seed_report_data();
        // مستأجر فارغ معرّف في الحامل لكنه بلا أي صف أعمال.
        fixture.state.set_active_tenant_for_tests(TENANT_EMPTY);
        let connection = fixture.state.connection.lock().expect("القفل يجب أن يتاح");

        let loaded = load_report_with(&connection, &fixture.state, ReportId::EvidenceRegister, 500)
            .expect("تقرير مستأجر فارغ يجب أن يُبنى");
        for section in &loaded.sections {
            assert_eq!(section.total_rows, 0, "{}", section.title);
            assert!(section.rows.is_empty(), "{}", section.title);
            assert!(!section.truncated);
        }
    }
}
