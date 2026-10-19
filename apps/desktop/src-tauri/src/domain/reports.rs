use crate::domain::error::AppError;

// =====================================================================
// نموذج نطاق التقارير المحلية — مطابق للعقد المجمّد في packages/contracts
// (serde rename_all = "camelCase") حرفياً.
//
// قواعد ملزمة يعبّر عنها هذا النموذج:
//   * كل قسم يُبنى من صفوف حقيقية داخل نطاق المستأجر؛ لا رقم مُختلق.
//   * `total_rows` هو العدد الكامل قبل الحدّ الأعلى، و`truncated` يعلن
//     صراحةً أن هناك ما لم يُدرج بدل التلميح.
//   * كل تقرير يحمل منشأه: جدولاته المصدر + لحظة التوليد + وسم صريح
//     بأنه ملخص تشغيلي محلي وليس إقرار امتثال قانوني.
// =====================================================================

/// الحدّ الأعلى المعلن لصفوف أي قسم — يُعرض للمستخدم في الكتالوج بصدق.
pub const MAX_ROWS_PER_SECTION: usize = 500;

/// أعلى حجم مقبول لملف تصدير واحد — حدّ صلب يمنع كتابة غير محدودة.
pub const MAX_EXPORT_BYTES: usize = 8 * 1024 * 1024;

/// أقصى طول مقبول لمسار ملف التصدير (حماية من سلاسل مسارات ضخمة).
pub const MAX_EXPORT_PATH_CHARS: usize = 600;

/// تسمية المنشأ الصريحة: ملخص تشغيلي محلي لا إقرار امتثال قانوني.
pub const LOCAL_SUMMARY_DISCLAIMER: &str =
    "هذا تقرير تشغيلي محلي مُشتق من بيانات مساحة العمل الحقيقية داخل نطاق المستأجر النشط. \
     لا يمثل إقرار امتثال قانونياً ولا شهادة تدقيق، ولا يجوز تقديمه لجهة تنظيمية كدليل امتثال. \
     الصفوف محدودة بحدّ أعلى مُعلن، والقيم مقروءة كما هي مُخزَّنة بلا تفسير أو تقدير.";

/// معرّفات التقارير المحلية الستة — نفس تعداد العقد المجمّد.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportId {
    ObligationStatus,
    RiskRegister,
    EvidenceRegister,
    ControlEffectiveness,
    TaskWorkload,
    AuditActivity,
}

/// كل التقارير بترتيب الكتالوج المعلن — ترتيب ثابت لا يعتمد على أي مُدخل.
pub const ALL_REPORT_IDS: [ReportId; 6] = [
    ReportId::ObligationStatus,
    ReportId::RiskRegister,
    ReportId::EvidenceRegister,
    ReportId::ControlEffectiveness,
    ReportId::TaskWorkload,
    ReportId::AuditActivity,
];

impl ReportId {
    /// المعرّف السلكي المطابق للعقد المجمّد (upper snake).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ObligationStatus => "OBLIGATION_STATUS",
            Self::RiskRegister => "RISK_REGISTER",
            Self::EvidenceRegister => "EVIDENCE_REGISTER",
            Self::ControlEffectiveness => "CONTROL_EFFECTIVENESS",
            Self::TaskWorkload => "TASK_WORKLOAD",
            Self::AuditActivity => "AUDIT_ACTIVITY",
        }
    }
}

// أي قيمة خارج التعداد تُرفض بنفس خطأ التحقق (فشل مغلق) — لا تخمين ولا تقريب.
impl std::str::FromStr for ReportId {
    type Err = AppError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim() {
            "OBLIGATION_STATUS" => Ok(Self::ObligationStatus),
            "RISK_REGISTER" => Ok(Self::RiskRegister),
            "EVIDENCE_REGISTER" => Ok(Self::EvidenceRegister),
            "CONTROL_EFFECTIVENESS" => Ok(Self::ControlEffectiveness),
            "TASK_WORKLOAD" => Ok(Self::TaskWorkload),
            "AUDIT_ACTIVITY" => Ok(Self::AuditActivity),
            _ => Err(AppError::ValidationFailed),
        }
    }
}

/// صيغة التصدير المدعومة — CSV أو MARKDOWN فقط، حرفياً كما في العقد.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportFormat {
    Csv,
    Markdown,
}

impl ReportFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Markdown => "MARKDOWN",
        }
    }

    /// الامتداد الإلزامي (بحروف صغيرة) — المقارنة لاحقاً غير حساسة لحالة الأحرف.
    pub fn required_extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Markdown => "md",
        }
    }
}

impl std::str::FromStr for ReportFormat {
    type Err = AppError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw.trim() {
            "CSV" => Ok(Self::Csv),
            "MARKDOWN" => Ok(Self::Markdown),
            _ => Err(AppError::ValidationFailed),
        }
    }
}

/// جدول مصدر واحد استُعلم فعلاً — يُبنى في طبقة الاستعلام لا في الواجهة.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportSourceTable {
    Tenants,
    Obligations,
    RegulatorySources,
    Risks,
    EvidenceItems,
    Controls,
    ControlObligations,
    ControlRisks,
    ControlEvidence,
    Tasks,
    LocalUsers,
    AuditEvents,
}

impl ReportSourceTable {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tenants => "tenants",
            Self::Obligations => "obligations",
            Self::RegulatorySources => "regulatory_sources",
            Self::Risks => "risks",
            Self::EvidenceItems => "evidence_items",
            Self::Controls => "controls",
            Self::ControlObligations => "control_obligations",
            Self::ControlRisks => "control_risks",
            Self::ControlEvidence => "control_evidence",
            Self::Tasks => "tasks",
            Self::LocalUsers => "local_users",
            Self::AuditEvents => "audit_events",
        }
    }
}

/// نطاق تقرير واحد: جدوله المصدر المعلن + وصفه.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportScope {
    pub report_id: ReportId,
    pub title: &'static str,
    pub description: &'static str,
    pub source_tables: &'static [ReportSourceTable],
}

/// الكتالوج الثابت للتقارير الستة — لا يُشتق من أي قيمة قادمة من العميل.
pub const REPORT_CATALOG: [ReportScope; 6] = [
    ReportScope {
        report_id: ReportId::ObligationStatus,
        title: "حالة الالتزامات التنظيمية",
        description:
            "سجل الالتزامات الفعلي داخل مساحة العمل: الحالة والخطورة والمالك والموعد وحالة الدليل، \
             مع توزيع الحالات والخطورة.",
        source_tables: &[
            ReportSourceTable::Obligations,
            ReportSourceTable::RegulatorySources,
        ],
    },
    ReportScope {
        report_id: ReportId::RiskRegister,
        title: "سجل المخاطر",
        description: "المخاطر المسجلة مع ارتباطها بالالتزام المالك، وخطورتها واحتمالها وأثرها، \
             مع تجميع حسب الخطورة.",
        source_tables: &[ReportSourceTable::Risks, ReportSourceTable::Obligations],
    },
    ReportScope {
        report_id: ReportId::EvidenceRegister,
        title: "سجل الأدلة",
        description: "بنود الأدلة المُسجَّلة مع تصنيفها وحالتها وتاريخ انتهائها، \
             وتوزيع الحالات، وحالة دليل كل التزام.",
        source_tables: &[
            ReportSourceTable::EvidenceItems,
            ReportSourceTable::Obligations,
        ],
    },
    ReportScope {
        report_id: ReportId::ControlEffectiveness,
        title: "فعالية الضوابط",
        description: "الضوابط المُؤلَّفة مع تصنيفها ومالكها وحالة تقييم فعاليتها ولحظته، \
             ونسبة تغطية كل ضابط للالتزامات والمخاطر والأدلة الفعلية.",
        source_tables: &[
            ReportSourceTable::Controls,
            ReportSourceTable::ControlObligations,
            ReportSourceTable::Obligations,
            ReportSourceTable::ControlRisks,
            ReportSourceTable::Risks,
            ReportSourceTable::ControlEvidence,
            ReportSourceTable::EvidenceItems,
        ],
    },
    ReportScope {
        report_id: ReportId::TaskWorkload,
        title: "عبء المهام",
        description: "المهام الفعلية مع أولويتها وحالتها وموعدها، وتجميع العبء لكل مُكلَّف \
             بعدد المهام المتأخرة والمفتوحة.",
        source_tables: &[
            ReportSourceTable::Tasks,
            ReportSourceTable::Obligations,
            ReportSourceTable::LocalUsers,
        ],
    },
    ReportScope {
        report_id: ReportId::AuditActivity,
        title: "النشاط التدقيقي",
        description: "أحداث سجل التدقيق المُخزَّنة فعلاً: تجميع حسب نوع الحدث، وأحدث الأحداث \
             بالفاعل والمورد واللحظة.",
        source_tables: &[ReportSourceTable::AuditEvents],
    },
];

/// نطاق تقرير بمعرّفه — غيابه مستحيل بنيوياً لأن التعداد يطابق الكتالوج كاملاً.
pub fn report_scope(report_id: ReportId) -> ReportScope {
    REPORT_CATALOG
        .iter()
        .copied()
        .find(|scope| scope.report_id == report_id)
        .unwrap_or(REPORT_CATALOG[0])
}

// ===== أنواع الخرج المطابقة للعقد المجمّد =====

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportColumn {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportSection {
    pub title: String,
    pub columns: Vec<ReportColumn>,
    pub rows: Vec<Vec<String>>,
    /// العدد الكامل قبل الحدّ الأعلى — يميّز «لا بيانات» من «اقتُطع».
    pub total_rows: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportProvenance {
    pub data_mode: String,
    pub source_tables: Vec<String>,
    pub generated_at: String,
    pub disclaimer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportCatalogEntry {
    pub report_id: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportCatalogData {
    pub reports: Vec<ReportCatalogEntry>,
    pub max_rows_per_section: usize,
    pub generated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportData {
    pub report_id: String,
    pub title: String,
    pub sections: Vec<ReportSection>,
    pub provenance: ReportProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportExportResult {
    pub file_name: String,
    pub bytes: u64,
    pub sha256: String,
    pub format: String,
}
#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{
        report_scope, ReportColumn, ReportFormat, ReportId, ReportSection, ReportSourceTable,
        ALL_REPORT_IDS, MAX_ROWS_PER_SECTION, REPORT_CATALOG,
    };

    // التعداد السلكي يجب أن يطابق عقد TypeScript حرفياً وبالترتيب نفسه.
    #[test]
    fn report_identifiers_match_the_frozen_contract_exactly() {
        let ids: Vec<&str> = ALL_REPORT_IDS.iter().map(|id| id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "OBLIGATION_STATUS",
                "RISK_REGISTER",
                "EVIDENCE_REGISTER",
                "CONTROL_EFFECTIVENESS",
                "TASK_WORKLOAD",
                "AUDIT_ACTIVITY",
            ]
        );
        assert_eq!(REPORT_CATALOG.len(), ALL_REPORT_IDS.len());
    }

    #[test]
    fn parsing_round_trips_and_rejects_unknown_values() {
        for id in ALL_REPORT_IDS {
            assert_eq!(ReportId::from_str(id.as_str()).ok(), Some(id));
            assert_eq!(report_scope(id).report_id, id);
        }
        assert!(
            ReportId::from_str("NOT_A_REPORT").is_err(),
            "معرّف غير معرّف يجب أن يُرفض"
        );
        assert!(ReportId::from_str("obligation_status").is_err());
        assert!(ReportId::from_str("").is_err());
    }

    #[test]
    fn export_formats_are_limited_to_csv_and_markdown_with_fixed_extensions() {
        assert_eq!(ReportFormat::from_str("CSV").ok(), Some(ReportFormat::Csv));
        assert_eq!(
            ReportFormat::from_str("MARKDOWN").ok(),
            Some(ReportFormat::Markdown)
        );
        assert_eq!(ReportFormat::Csv.required_extension(), "csv");
        assert_eq!(ReportFormat::Markdown.required_extension(), "md");
        assert!(ReportFormat::from_str("PDF").is_err());
        assert!(ReportFormat::from_str("csv").is_err());
    }

    // كل تقرير يعلن جداوله المصدر صراحةً، والحدّ الأعلى ثابت مُعلن.
    #[test]
    fn every_report_declares_at_least_one_real_source_table() {
        for scope in REPORT_CATALOG {
            assert!(!scope.source_tables.is_empty());
            assert!(!scope.title.trim().is_empty());
            assert!(!scope.description.trim().is_empty());
            assert!(scope
                .source_tables
                .iter()
                .all(|table| !table.as_str().is_empty()));
        }
        assert_eq!(MAX_ROWS_PER_SECTION, 500);
        assert_eq!(ReportSourceTable::AuditEvents.as_str(), "audit_events");
    }

    // سياسة الاقتطاع: تسلسل العدد الكامل + العدد المُدرج + راية الاقتطاع.
    #[test]
    fn truncation_reports_the_true_total_and_the_shown_count() {
        let rows: Vec<Vec<String>> = (0..5).map(|index| vec![index.to_string()]).collect();
        let section = ReportSection {
            title: "قسم".to_owned(),
            columns: vec![ReportColumn {
                key: "index".to_owned(),
                label: "Index".to_owned(),
            }],
            rows: rows.into_iter().take(2).collect(),
            total_rows: 5,
            truncated: true,
        };
        assert_eq!(section.total_rows, 5);
        assert_eq!(section.rows.len(), 2);
        assert!(section.truncated);
    }
}
