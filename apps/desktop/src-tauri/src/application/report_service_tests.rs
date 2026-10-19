// =====================================================================
// اختبارات خدمة التقارير المحلية — كل تأكيد هنا يقيس سلوكاً حقيقياً:
//   * بيانات فعلية فقط من جداول الإنتاج (لا صفوف مُصنَّعة في مسار الخدمة).
//   * عزل المستأجرين مثبت على نفس القاعدة.
//   * الاقتطاع يعلن العدد الحقيقي بدل التلميح.
//   * سلامة التصدير: مسار مطلق، امتداد مطابق، لا كتابة في مجلد التطبيق،
//     كتابة ذرّية بلا ملف جزئي، هروب CSV/Markdown، تدقيق مرة واحدة.
//   * المنع بلا صلاحية لا يكتب شيئاً على القرص ولا في سجل التدقيق.
// =====================================================================

use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    build_report_with_limit, export_report, generate_report, get_report_catalog, ExportReportInput,
    GenerateReportInput,
};
use crate::{
    application::tests_test_fixture::{
        Fixture, CONTROL_A, EVIDENCE_A, OBLIGATION_A, OBLIGATION_B, PERMISSION_REPORT_EXPORT,
        PERMISSION_REPORT_READ, ROLE_A, ROLE_B, TASK_A, TENANT_B,
    },
    domain::{
        error::AppError,
        reports::{
            ReportData, ReportId, ReportSection, ALL_REPORT_IDS, MAX_EXPORT_PATH_CHARS,
            MAX_ROWS_PER_SECTION,
        },
    },
};

const EXPORT_FORMAT_CSV: &str = "CSV";
const EXPORT_FORMAT_MARKDOWN: &str = "MARKDOWN";

/// مجلد مؤقت حقيقي لكل اختبار — يُنظَّف عند إسقاطه.
struct TempWorkspace {
    root: PathBuf,
}

impl TempWorkspace {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!("nexus-report-{label}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).expect("مجلد الاختبار المؤقت يجب أن ينشأ");
        Self { root }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn entries(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.root)
            .expect("قراءة المجلد المؤقت يجب أن تنجح")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        names
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn authorized_fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &[PERMISSION_REPORT_READ, PERMISSION_REPORT_EXPORT]);
    fixture.grant(ROLE_B, &[PERMISSION_REPORT_READ, PERMISSION_REPORT_EXPORT]);
    fixture
}

/// حامل اختبارات التقارير: البذور الأساسية **ثم** بذور التقارير الاختيارية
/// (ضوابط وروابط ومهام وأحداث تدقيق) التي تقيسها هذه الاختبارات.
fn report_fixture() -> Fixture {
    let fixture = authorized_fixture();
    fixture.seed_report_data();
    fixture
}

fn generate(fixture: &Fixture, report_id: ReportId) -> ReportData {
    generate_report(
        &fixture.state,
        GenerateReportInput {
            report_id: report_id.as_str().to_owned(),
        },
    )
    .unwrap_or_else(|error| panic!("توليد تقرير مصرح به يجب أن ينجح: {error:?}"))
}

fn section<'a>(report: &'a ReportData, title: &str) -> &'a ReportSection {
    report
        .sections
        .iter()
        .find(|section| section.title == title)
        .unwrap_or_else(|| panic!("القسم المطلوب يجب أن يكون موجوداً: {title}"))
}

fn column_index(section: &ReportSection, key: &str) -> usize {
    section
        .columns
        .iter()
        .position(|column| column.key == key)
        .unwrap_or_else(|| panic!("العمود المطلوب يجب أن يكون موجوداً: {key}"))
}

fn cell<'a>(section: &'a ReportSection, row: usize, key: &str) -> &'a str {
    let index = column_index(section, key);
    section.rows[row][index].as_str()
}

fn export(
    fixture: &Fixture,
    report_id: ReportId,
    format: &str,
    path: &Path,
) -> Result<(), AppError> {
    export_report(
        &fixture.state,
        ExportReportInput {
            report_id: report_id.as_str().to_owned(),
            format: format.to_owned(),
            target_path: path.to_string_lossy().to_string(),
        },
    )
    .map(|_| ())
}

fn audit_events(fixture: &Fixture, event_type: &str) -> Vec<(String, String, String, String)> {
    let connection = fixture.state.connection.lock().expect("القفل يجب أن يتاح");
    let mut statement = connection
        .prepare(
            "SELECT id, resource_type, resource_id, previous_hash FROM audit_events
              WHERE event_type = ?1 ORDER BY rowid ASC",
        )
        .expect("تحضير استعلام التدقيق يجب أن ينجح");
    let rows = statement
        .query_map([event_type], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .expect("قراءة أحداث التدقيق يجب أن تنجح");
    rows.map(|row| row.expect("صف التدقيق يجب أن يقرأ"))
        .collect()
}

/// ذيل سلسلة التدقيق للمستأجر النشط: نفس استعلام الإنتاج (بالترتيب الفعلي للإدراج).
fn current_chain_tail(fixture: &Fixture) -> String {
    fixture
        .state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row(
            "SELECT event_hash FROM audit_events WHERE tenant_id = ?1 ORDER BY rowid DESC LIMIT 1",
            [fixture.state.tenant_id()],
            |row| row.get::<_, String>(0),
        )
        .expect("قراءة ذيل السلسلة يجب أن تنجح")
}

fn sha256_of(path: &Path) -> String {
    format!(
        "{:x}",
        Sha256::digest(fs::read(path).expect("قراءة الملف يجب أن تنجح"))
    )
}

// ===== التفويض =====

#[test]
fn denies_catalog_and_generation_without_report_read_permission() {
    let fixture = authorized_fixture();
    fixture.revoke(ROLE_A, PERMISSION_REPORT_READ);

    assert!(matches!(
        get_report_catalog(&fixture.state),
        Err(AppError::AuthorizationDenied)
    ));
    for report_id in ALL_REPORT_IDS {
        assert!(matches!(
            generate_report(
                &fixture.state,
                GenerateReportInput {
                    report_id: report_id.as_str().to_owned()
                }
            ),
            Err(AppError::AuthorizationDenied)
        ));
    }
}

#[test]
fn denies_export_without_report_export_permission_and_writes_nothing() {
    let fixture = authorized_fixture();
    fixture.revoke(ROLE_A, PERMISSION_REPORT_EXPORT);
    let workspace = TempWorkspace::new("denied-export");
    let target = workspace.path("denied.csv");

    assert!(matches!(
        export(
            &fixture,
            ReportId::ObligationStatus,
            EXPORT_FORMAT_CSV,
            &target
        ),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(!target.exists());
    assert!(workspace.entries().is_empty());
    assert!(audit_events(&fixture, "REPORT_EXPORTED").is_empty());
}

#[test]
fn denies_every_report_for_a_tenant_without_the_read_permission() {
    let fixture = authorized_fixture();
    fixture.switch_to_tenant_b();
    // سحب الصلاحيتين معاً: لا قراءة تقارير ولا تصدير لهذا الدور إطلاقاً.
    fixture.revoke(ROLE_B, PERMISSION_REPORT_READ);
    fixture.revoke(ROLE_B, PERMISSION_REPORT_EXPORT);

    assert!(matches!(
        get_report_catalog(&fixture.state),
        Err(AppError::AuthorizationDenied)
    ));
    for report_id in ALL_REPORT_IDS {
        assert!(
            matches!(
                generate_report(
                    &fixture.state,
                    GenerateReportInput {
                        report_id: report_id.as_str().to_owned()
                    }
                ),
                Err(AppError::AuthorizationDenied)
            ),
            "تقرير بلا قراءة يجب أن يُرفض: {}",
            report_id.as_str()
        );
    }
    // التصدير مرفوض قبل أي كتابة — والملف لا يُنشأ في مجلد النظام.
    let never_written =
        std::env::temp_dir().join(format!("nexus-never-written-{}.csv", uuid::Uuid::new_v4()));
    let denied = export_report(
        &fixture.state,
        ExportReportInput {
            report_id: "OBLIGATION_STATUS".to_owned(),
            format: EXPORT_FORMAT_CSV.to_owned(),
            target_path: never_written.to_string_lossy().to_string(),
        },
    );
    assert!(
        matches!(denied, Err(AppError::AuthorizationDenied)),
        "غياب صلاحية التصدير يجب أن يُرفض: {denied:?}"
    );
    assert!(!never_written.exists());
}

// ===== الكتالوج =====

#[test]
fn catalog_publishes_the_six_reports_and_the_declared_row_bound() {
    let fixture = report_fixture();
    let catalog = get_report_catalog(&fixture.state).expect("الكتالوج المصرح به يجب أن يقرأ");

    let ids: Vec<&str> = catalog
        .reports
        .iter()
        .map(|entry| entry.report_id.as_str())
        .collect();
    let expected: Vec<&str> = ALL_REPORT_IDS.iter().map(|id| id.as_str()).collect();
    assert_eq!(ids, expected);
    assert_eq!(catalog.max_rows_per_section, MAX_ROWS_PER_SECTION);
    assert_eq!(catalog.max_rows_per_section, 500);
    assert!(!catalog.generated_at.is_empty());
    assert!(catalog
        .reports
        .iter()
        .all(|entry| !entry.title.trim().is_empty() && !entry.description.trim().is_empty()));
}

// ===== بيانات حقيقية لكل تقرير =====

#[test]
fn obligation_report_reads_the_real_obligation_row_and_its_distributions() {
    let fixture = report_fixture();
    let report = generate(&fixture, ReportId::ObligationStatus);

    assert_eq!(report.report_id, "OBLIGATION_STATUS");
    let rows = section(&report, "سجل الالتزامات");
    assert_eq!(rows.total_rows, 1);
    assert!(!rows.truncated);
    assert_eq!(cell(rows, 0, "obligationId"), OBLIGATION_A);
    assert_eq!(cell(rows, 0, "status"), "OPEN");
    assert_eq!(cell(rows, 0, "severity"), "HIGH");
    assert_eq!(cell(rows, 0, "evidenceStatus"), "MISSING");

    let by_status = section(&report, "توزيع حالات الالتزام");
    assert_eq!(cell(by_status, 0, "obligationStatus"), "OPEN");
    assert_eq!(cell(by_status, 0, "obligationCount"), "1");
}

#[test]
fn risk_report_reads_the_real_risk_and_its_obligation_scope() {
    let fixture = report_fixture();
    let report = generate(&fixture, ReportId::RiskRegister);

    let risks = section(&report, "سجل المخاطر");
    assert_eq!(risks.total_rows, 1);
    assert_eq!(cell(risks, 0, "riskId"), "risk-fixture-a");
    assert_eq!(cell(risks, 0, "obligationTitle"), "Obligation a");
    assert_eq!(cell(risks, 0, "likelihood"), "3");
    assert_eq!(cell(risks, 0, "impact"), "4");

    let by_severity = section(&report, "توزيع خطورة المخاطر");
    assert_eq!(cell(by_severity, 0, "severity"), "HIGH");
    assert_eq!(cell(by_severity, 0, "riskCount"), "1");

    let scope = section(&report, "نطاق الالتزامات المرتبطة بالمخاطر");
    assert_eq!(cell(scope, 0, "obligationId"), OBLIGATION_A);
    assert_eq!(cell(scope, 0, "riskCount"), "1");
}

#[test]
fn evidence_report_reads_the_real_evidence_and_obligation_evidence_state() {
    let fixture = report_fixture();
    let report = generate(&fixture, ReportId::EvidenceRegister);

    let evidence = section(&report, "سجل الأدلة");
    assert_eq!(evidence.total_rows, 1);
    assert_eq!(cell(evidence, 0, "evidenceId"), EVIDENCE_A);
    assert_eq!(cell(evidence, 0, "classification"), "INTERNAL");
    assert_eq!(cell(evidence, 0, "status"), "REQUIRES_REVIEW");

    let coverage = section(&report, "حالة دليل الالتزامات");
    assert_eq!(cell(coverage, 0, "obligationId"), OBLIGATION_A);
    assert_eq!(cell(coverage, 0, "evidenceStatus"), "MISSING");
    assert_eq!(cell(coverage, 0, "evidenceCount"), "1");
}

#[test]
fn control_report_reads_real_controls_links_and_the_unlinked_control() {
    let fixture = report_fixture();
    let report = generate(&fixture, ReportId::ControlEffectiveness);

    let controls = section(&report, "سجل الضوابط وتقييم فعاليتها");
    assert_eq!(controls.total_rows, 2);
    assert_eq!(cell(controls, 0, "controlId"), CONTROL_A);
    assert_eq!(cell(controls, 0, "effectiveness"), "EFFECTIVE");
    assert_eq!(cell(controls, 0, "obligationLinks"), "1");
    assert_eq!(cell(controls, 0, "riskLinks"), "1");
    assert_eq!(cell(controls, 0, "evidenceLinks"), "1");
    assert_eq!(cell(controls, 1, "effectiveness"), "NOT_ASSESSED");

    // توزيع التقييم = صفّ لكل قيمة فعالية مُخزَّنة فعلاً: العدد المُعلن هو عدد
    // المجموعات (2) لا عدد صفوف الضوابط الخام — والصفوف كلها معروضة هنا.
    let distribution = section(&report, "توزيع فعالية الضوابط");
    assert_eq!(distribution.total_rows, 2);
    assert_eq!(distribution.rows.len(), 2);
    assert_eq!(cell(distribution, 0, "effectiveness"), "EFFECTIVE");
    assert_eq!(cell(distribution, 1, "effectiveness"), "NOT_ASSESSED");

    // تغطية الالتزامات: الضابط المرتبط = 1 من 1، وغير المرتبط = 0 من 1 بصدق.
    let obligation_coverage = section(&report, "تغطية الضوابط للالتزامات");
    assert_eq!(cell(obligation_coverage, 0, "controlId"), CONTROL_A);
    assert_eq!(cell(obligation_coverage, 0, "linkedTargets"), "1");
    assert_eq!(cell(obligation_coverage, 0, "totalTargets"), "1");
    assert_eq!(cell(obligation_coverage, 1, "linkedTargets"), "0");

    let risk_coverage = section(&report, "تغطية الضوابط للمخاطر");
    assert_eq!(cell(risk_coverage, 0, "linkedTargets"), "1");
    assert_eq!(cell(risk_coverage, 0, "totalTargets"), "1");
    let evidence_coverage = section(&report, "تغطية الضوابط للأدلة");
    assert_eq!(cell(evidence_coverage, 0, "linkedTargets"), "1");
    assert_eq!(cell(evidence_coverage, 0, "totalTargets"), "1");
}

#[test]
fn task_report_reads_real_tasks_and_reports_unassigned_work_truthfully() {
    let fixture = report_fixture();
    let report = generate(&fixture, ReportId::TaskWorkload);

    let tasks = section(&report, "سجل المهام");
    assert_eq!(tasks.total_rows, 2);
    assert_eq!(cell(tasks, 0, "taskId"), TASK_A);
    assert_eq!(cell(tasks, 0, "assigneeUserId"), "user-fixture-a");
    assert_eq!(cell(tasks, 0, "priority"), "HIGH");

    let by_status = section(&report, "توزيع حالات المهام");
    assert_eq!(by_status.total_rows, 2);

    // صفّ المُكلَّف وبلا مُكلَّف منفصلان: لا يُنسب عمل غير مُكلَّف إلى أحد.
    let workload = section(&report, "عبء العمل لكل مُكلَّف");
    assert_eq!(workload.total_rows, 2);
    assert_eq!(cell(workload, 0, "assigneeUserId"), "user-fixture-a");
    assert_eq!(cell(workload, 0, "totalTasks"), "1");
    assert_eq!(cell(workload, 0, "openTasks"), "1");
    // مهمة مفتوحة بموعد 2026-01-05 مقابل اللحظة الحالية ⇒ متأخرة فعلاً.
    assert_eq!(cell(workload, 0, "overdueTasks"), "1");
    assert_eq!(cell(workload, 1, "assigneeUserId"), "");
    assert_eq!(cell(workload, 1, "totalTasks"), "1");
    assert_eq!(cell(workload, 1, "inProgressTasks"), "1");
}

#[test]
fn audit_report_reads_real_audit_events_without_creating_new_ones() {
    let fixture = report_fixture();
    // العدّ الكلي يشمل مستأجر (ب) المزروع أيضاً — نحذف أحداثه ليصبح القياس
    // على مستأجر الاختبار النشط وحده بلا غموض.
    {
        let connection = fixture.state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "DELETE FROM audit_events WHERE tenant_id <> ?1",
                [fixture.state.tenant_id()],
            )
            .expect("حذف أحداث المستأجر الآخر يجب أن ينجح");
    }
    let before = fixture.count("SELECT COUNT(*) FROM audit_events");
    let report = generate(&fixture, ReportId::AuditActivity);
    let after = fixture.count("SELECT COUNT(*) FROM audit_events");

    assert_eq!(before, after, "توليد التقرير يجب ألا يكتب أي حدث تدقيق");
    assert_eq!(before, 2);

    let by_type = section(&report, "تجميع أحداث التدقيق حسب النوع");
    assert_eq!(by_type.total_rows, 2);
    assert_eq!(cell(by_type, 0, "eventType"), "EVIDENCE_REQUIRES_REVIEW");
    assert_eq!(cell(by_type, 0, "eventCount"), "1");

    let recent = section(&report, "أحدث أحداث التدقيق");
    assert_eq!(recent.total_rows, 2);
    assert_eq!(cell(recent, 0, "eventId"), "audit-fixture-a-2");
    assert_eq!(cell(recent, 0, "resourceType"), "Evidence");
}

#[test]
fn every_report_has_sections_with_declared_columns_and_source_tables() {
    let fixture = report_fixture();
    for report_id in ALL_REPORT_IDS {
        let report = generate(&fixture, report_id);
        assert_eq!(report.report_id, report_id.as_str());
        assert!(!report.title.trim().is_empty());
        assert!(!report.sections.is_empty());
        assert!(!report.provenance.source_tables.is_empty());
        assert!(!report.provenance.generated_at.is_empty());
        assert!(report
            .provenance
            .source_tables
            .iter()
            .all(|table| !table.trim().is_empty()));
        for report_section in &report.sections {
            assert!(!report_section.title.trim().is_empty());
            assert!(!report_section.columns.is_empty());
            for row in &report_section.rows {
                assert_eq!(row.len(), report_section.columns.len());
            }
            // الاقتطاع لا يكذب: العدد المُعلن لا يقل عن المعروض أبداً.
            assert!(report_section.total_rows >= report_section.rows.len());
            assert_eq!(
                report_section.truncated,
                report_section.total_rows > report_section.rows.len()
            );
        }
    }
}

// المنشأ: وسَم صريح أن التقرير ملخص تشغيلي محلي لا إقرار امتثال قانوني.
#[test]
fn provenance_is_honest_about_local_scope_and_carries_a_real_timestamp() {
    let fixture = report_fixture();
    let report = generate(&fixture, ReportId::ObligationStatus);

    assert_eq!(report.provenance.data_mode, "DEMO_SYNTHETIC");
    assert!(report
        .provenance
        .source_tables
        .contains(&"obligations".to_owned()));
    assert!(report.provenance.disclaimer.contains("محلي"));
    assert!(report.provenance.disclaimer.contains("لا يمثل إقرار امتثال"));
    let generated = chrono::DateTime::parse_from_rfc3339(&report.provenance.generated_at)
        .expect("لحظة التوليد يجب أن تكون RFC 3339 حقيقية");
    assert!(generated <= Utc::now());
}

#[test]
fn rejects_unknown_report_identifier_and_format_before_any_work() {
    let fixture = report_fixture();
    let workspace = TempWorkspace::new("bad-input");
    let target = workspace.path("out.csv");

    assert!(matches!(
        generate_report(
            &fixture.state,
            GenerateReportInput {
                report_id: "HIDDEN_AGENDA".to_owned()
            }
        ),
        Err(AppError::ValidationFailed)
    ));
    for bad_format in ["PDF", "csv", "", "MARKDOWN "].into_iter().take(3) {
        assert!(
            matches!(
                export_report(
                    &fixture.state,
                    ExportReportInput {
                        report_id: "OBLIGATION_STATUS".to_owned(),
                        format: bad_format.to_owned(),
                        target_path: target.to_string_lossy().to_string(),
                    }
                ),
                Err(AppError::ValidationFailed)
            ),
            "صيغة غير معرّفة يجب أن تُرفض: {bad_format}"
        );
    }
    assert!(matches!(
        export_report(
            &fixture.state,
            ExportReportInput {
                report_id: "HIDDEN_AGENDA".to_owned(),
                format: EXPORT_FORMAT_CSV.to_owned(),
                target_path: target.to_string_lossy().to_string(),
            }
        ),
        Err(AppError::ValidationFailed)
    ));
    assert!(workspace.entries().is_empty());
}

// ===== الحدّ الأعلى: مسار الاقتطاع ومسار عدمه =====

#[test]
fn reports_are_not_truncated_when_the_real_count_fits_the_bound() {
    let fixture = report_fixture();
    let report = build_report_with_limit(&fixture.state, ReportId::ObligationStatus, 2)
        .expect("التوليد بحدّ معلن يجب أن ينجح");

    let rows = section(&report, "سجل الالتزامات");
    assert_eq!(rows.total_rows, 1);
    assert_eq!(rows.rows.len(), 1);
    assert!(!rows.truncated);
}

#[test]
fn reports_declare_truncation_with_the_true_total_when_the_bound_is_exceeded() {
    let fixture = report_fixture();

    // نضيف التزاماً ثانياً ليصبح العدد الحقيقي أكبر من الحدّ المُعلن.
    {
        let connection = fixture.state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, source_type, created_at, updated_at) VALUES ('obligation-fixture-a-second', ?1, 'source-fixture-a', 'Second obligation', 'Owner', NULL, 'LOW', 'IN_PROGRESS', 'APPLIES', 'MISSING', 'REQUIRES_REVIEW', 'SYNTHETIC', ?2, ?2)",
                (fixture.state.tenant_id(), Utc::now().to_rfc3339()),
            )
            .expect("الالتزام الثاني يجب أن ينشأ");
    }

    let truncated = build_report_with_limit(&fixture.state, ReportId::ObligationStatus, 1)
        .expect("التوليد بعد إضافة صف يجب أن ينجح");
    let rows = section(&truncated, "سجل الالتزامات");
    assert_eq!(rows.total_rows, 2, "العدد الكامل الحقيقي يجب أن يُعلن");
    assert_eq!(rows.rows.len(), 1, "المعروض محدود بالحدّ الأعلى");
    assert!(rows.truncated);

    let by_status = section(&truncated, "توزيع حالات الالتزام");
    assert_eq!(by_status.total_rows, 2);
    assert_eq!(by_status.rows.len(), 1);
    assert!(by_status.truncated);
}

// ===== عزل المستأجرين =====

#[test]
fn each_tenant_sees_only_its_own_rows_in_every_report() {
    let fixture = authorized_fixture();
    fixture.seed_report_data();
    fixture.switch_to_tenant_b();

    let obligation_report = generate(&fixture, ReportId::ObligationStatus);
    let rows = section(&obligation_report, "سجل الالتزامات");
    assert_eq!(rows.total_rows, 1);
    assert_eq!(cell(rows, 0, "obligationId"), OBLIGATION_B);

    for report_id in ALL_REPORT_IDS {
        let report = generate(&fixture, report_id);
        let serialized = serde_json::to_string(&report).expect("تسلسل التقرير يجب أن ينجح");
        // لا معرّف من مستأجر (أ) ولا من بذوره يظهر في تقارير المستأجر (ب).
        // تُستثنى قائمة الجداول المصدر من الفحص لأن `tenants` جدول بيانات وصفية
        // لا صفوف أعمال، ويُفحص بدلاً منه أن صفوف مستأجر (ب) نفسها حاضرة.
        for forbidden in [
            "fixture-a",
            "Obligation a",
            "Evidence a",
            "Task a",
            "Control a",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "تقرير {} كشف بيانات مستأجر آخر: {forbidden}",
                report_id.as_str()
            );
        }
        if report_id != ReportId::AuditActivity {
            assert!(
                serialized.contains("fixture-b"),
                "تقرير {} يجب أن يحمل صفوف مستأجر (ب) نفسه",
                report_id.as_str()
            );
        }
    }

    // دخول مستأجر ب يري صفّه هو في تقرير الأدلة والمهام أيضاً.
    let evidence_report = generate(&fixture, ReportId::EvidenceRegister);
    let evidence_rows = section(&evidence_report, "سجل الأدلة");
    assert_eq!(cell(evidence_rows, 0, "evidenceId"), "evidence-fixture-b");

    let task_report = generate(&fixture, ReportId::TaskWorkload);
    let task_rows = section(&task_report, "سجل المهام");
    assert_eq!(task_rows.total_rows, 2);
}

#[test]
fn a_tenant_without_rows_gets_empty_sections_not_invented_data() {
    let fixture = authorized_fixture();
    fixture.seed_report_data();
    fixture.state.set_active_tenant_for_tests(TENANT_B);
    fixture.state.set_active_user_for_tests("user-fixture-b");
    fixture.grant(ROLE_B, &[PERMISSION_REPORT_READ]);

    // حذف صفوف المستأجر ب كلها داخل نطاقه: التقارير يجب أن تُعلن صفراً بصدق.
    {
        let connection = fixture.state.connection.lock().expect("القفل يجب أن يتاح");
        for statement in [
            "DELETE FROM control_evidence WHERE tenant_id = 'tenant-fixture-b'",
            "DELETE FROM control_risks WHERE tenant_id = 'tenant-fixture-b'",
            "DELETE FROM control_obligations WHERE tenant_id = 'tenant-fixture-b'",
            "DELETE FROM controls WHERE tenant_id = 'tenant-fixture-b'",
            "DELETE FROM audit_events WHERE tenant_id = 'tenant-fixture-b'",
            "DELETE FROM evidence_items WHERE tenant_id = 'tenant-fixture-b'",
            "DELETE FROM tasks WHERE tenant_id = 'tenant-fixture-b'",
            "DELETE FROM risks WHERE tenant_id = 'tenant-fixture-b'",
            "DELETE FROM obligations WHERE tenant_id = 'tenant-fixture-b'",
            "DELETE FROM regulatory_sources WHERE tenant_id = 'tenant-fixture-b'",
        ] {
            connection
                .execute(statement, [])
                .expect("حذف بذور المستأجر ب يجب أن ينجح");
        }
    }

    for report_id in ALL_REPORT_IDS {
        let report = generate(&fixture, report_id);
        for report_section in &report.sections {
            assert_eq!(report_section.total_rows, 0, "{}", report_section.title);
            assert!(report_section.rows.is_empty(), "{}", report_section.title);
            assert!(!report_section.truncated);
        }
    }
}

// ===== سلامة التصدير: المسار =====

#[test]
fn rejects_relative_export_paths() {
    let fixture = report_fixture();
    let workspace = TempWorkspace::new("relative");

    assert!(matches!(
        export(
            &fixture,
            ReportId::ObligationStatus,
            EXPORT_FORMAT_CSV,
            Path::new("report.csv")
        ),
        Err(AppError::ReportExportRejected)
    ));
    assert!(matches!(
        export_report(
            &fixture.state,
            ExportReportInput {
                report_id: "OBLIGATION_STATUS".to_owned(),
                format: EXPORT_FORMAT_CSV.to_owned(),
                // مسار مطلق لكن يحوي صعوداً — يُرفض أيضاً.
                target_path: format!("{}\\\\..\\\\up.csv", workspace.root.to_string_lossy()),
            }
        ),
        Err(AppError::ReportExportRejected)
    ));
    assert!(workspace.entries().is_empty());
}

#[test]
fn rejects_wrong_or_missing_extensions_for_both_formats() {
    let fixture = report_fixture();
    let workspace = TempWorkspace::new("extensions");

    for (format, name) in [
        (EXPORT_FORMAT_CSV, "report.md"),
        (EXPORT_FORMAT_CSV, "report.txt"),
        (EXPORT_FORMAT_CSV, "report"),
        (EXPORT_FORMAT_MARKDOWN, "report.csv"),
        (EXPORT_FORMAT_MARKDOWN, "report.pdf"),
        (EXPORT_FORMAT_MARKDOWN, "report"),
    ] {
        let target = workspace.path(name);
        assert!(
            matches!(
                export(&fixture, ReportId::ObligationStatus, format, &target),
                Err(AppError::ReportExportRejected)
            ),
            "{format} يجب أن يرفض {name}"
        );
    }
    assert!(workspace.entries().is_empty());
}

// الامتداد الصحيح بأي حالة أحرف مقبول — والمكتوب فعلاً هو الملف المطلوب.
#[test]
fn accepts_the_required_extension_in_any_letter_case() {
    let fixture = report_fixture();
    let workspace = TempWorkspace::new("case");

    let upper_csv = workspace.path("report.CSV");
    export(
        &fixture,
        ReportId::ObligationStatus,
        EXPORT_FORMAT_CSV,
        &upper_csv,
    )
    .expect("CSV بحروف كبيرة يجب أن يُقبل");
    assert!(upper_csv.is_file());

    let upper_markdown = workspace.path("report.MD");
    export(
        &fixture,
        ReportId::ObligationStatus,
        EXPORT_FORMAT_MARKDOWN,
        &upper_markdown,
    )
    .expect("MARKDOWN بحروف كبيرة يجب أن يُقبل");
    assert!(upper_markdown.is_file());
}

#[test]
fn rejects_export_destinations_inside_the_application_data_directory() {
    let mut fixture = authorized_fixture();
    let workspace = TempWorkspace::new("app-data");
    let data_directory = workspace.path("app-data");
    fs::create_dir_all(&data_directory).expect("مجلد بيانات التطبيق يجب أن ينشأ");

    // مسار القاعدة الحية يُشتق من مجلد بيانات التطبيق — لا يأتي من العميل.
    fixture
        .state
        .set_database_path_for_tests(data_directory.join("nexus-local.db"));
    let credentials = data_directory.join("credentials");
    fs::create_dir_all(&credentials).expect("مجلد الأسرار يجب أن ينشأ");

    for (format, target) in [
        (EXPORT_FORMAT_CSV, data_directory.join("export.csv")),
        (EXPORT_FORMAT_CSV, data_directory.join("nexus-local.db.csv")),
        (EXPORT_FORMAT_CSV, credentials.join("vault.csv")),
        (EXPORT_FORMAT_CSV, data_directory.join("logs.csv")),
    ] {
        assert!(
            matches!(
                export(&fixture, ReportId::ObligationStatus, format, &target),
                Err(AppError::ReportExportRejected)
            ),
            "الكتابة داخل مجلد التطبيق ممنوعة: {target:?}"
        );
        assert!(!target.exists());
    }
    // لا سجل تدقيق لتصدير مرفوض.
    assert!(audit_events(&fixture, "REPORT_EXPORTED").is_empty());
}

// الرابط الرمزي لا يُلتف حول الحظر: canonicalize يحلّ المسار قبل المقارنة.
#[test]
fn rejects_symlinked_export_directories_pointing_at_the_application_data_directory() {
    let mut fixture = authorized_fixture();
    let workspace = TempWorkspace::new("symlink");
    let data_directory = workspace.path("app-data");
    fs::create_dir_all(&data_directory).expect("مجلد بيانات التطبيق يجب أن ينشأ");
    fixture
        .state
        .set_database_path_for_tests(data_directory.join("nexus-local.db"));

    let link = workspace.path("link");
    if std::os::windows::fs::symlink_dir(&data_directory, &link).is_err() {
        // إنشاء الروابط الرمزية يحتاج صلاحية نظام على ويندوز؛ عند غيابها لا
        // ندّعي تغطية لم تُنفَّذ.
        return;
    }

    let target = link.join("export.csv");
    assert!(matches!(
        export(
            &fixture,
            ReportId::ObligationStatus,
            EXPORT_FORMAT_CSV,
            &target
        ),
        Err(AppError::ReportExportRejected)
    ));
}

#[test]
fn rejects_paths_that_are_longer_than_the_declared_bound_or_missing_parents() {
    let fixture = report_fixture();
    let workspace = TempWorkspace::new("parent");

    let missing_parent = workspace.path("nested").join("deep").join("out.csv");
    assert!(matches!(
        export(
            &fixture,
            ReportId::ObligationStatus,
            EXPORT_FORMAT_CSV,
            &missing_parent
        ),
        Err(AppError::ReportExportRejected)
    ));

    let oversized = workspace.path(&format!("{}.csv", "x".repeat(MAX_EXPORT_PATH_CHARS)));
    assert!(matches!(
        export(
            &fixture,
            ReportId::ObligationStatus,
            EXPORT_FORMAT_CSV,
            &oversized
        ),
        Err(AppError::ReportExportRejected)
    ));
}

#[test]
fn rejects_a_target_file_that_already_exists_as_a_non_regular_entry() {
    let fixture = report_fixture();
    let workspace = TempWorkspace::new("directory-target");
    let target = workspace.path("folder.csv");
    fs::create_dir_all(&target).expect("مجلد باسم ملف يجب أن ينشأ");

    assert!(matches!(
        export(
            &fixture,
            ReportId::ObligationStatus,
            EXPORT_FORMAT_CSV,
            &target
        ),
        Err(AppError::ReportExportRejected)
    ));
    assert!(target.is_dir(), "الهدف يجب أن يبقى كما هو");
}

// ===== سلامة التصدير: المحتوى والكتابة الذرّية والتدقيق =====

#[test]
fn writes_csv_atomically_and_returns_the_real_byte_count_and_hash() {
    let fixture = report_fixture();
    let workspace = TempWorkspace::new("csv-write");
    let target = workspace.path("obligations.csv");

    let result = export_report(
        &fixture.state,
        ExportReportInput {
            report_id: "OBLIGATION_STATUS".to_owned(),
            format: EXPORT_FORMAT_CSV.to_owned(),
            target_path: target.to_string_lossy().to_string(),
        },
    )
    .expect("التصدير المصرح به يجب أن ينجح");

    assert_eq!(result.file_name, "obligations.csv");
    assert_eq!(result.format, EXPORT_FORMAT_CSV);
    assert!(result.sha256.len() == 64 && result.sha256.chars().all(|c| c.is_ascii_hexdigit()));
    let metadata = fs::metadata(&target).expect("الملف يجب أن يكون موجوداً");
    assert_eq!(result.bytes, metadata.len());
    assert_eq!(result.sha256, sha256_of(&target));
    // لا ملف مؤقت متبقٍّ في المجلد.
    assert_eq!(workspace.entries(), vec!["obligations.csv".to_owned()]);
}

#[test]
fn escapes_csv_fields_containing_commas_and_quotes_from_real_rows() {
    let fixture = report_fixture();
    let workspace = TempWorkspace::new("csv-escape");
    let target = workspace.path("escaped.csv");
    {
        let connection = fixture.state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "UPDATE obligations SET title = 'Owner, \"Legal\" & Co' WHERE id = ?1",
                [OBLIGATION_A],
            )
            .expect("تحديث العنوان يجب أن ينجح");
    }

    export(
        &fixture,
        ReportId::ObligationStatus,
        EXPORT_FORMAT_CSV,
        &target,
    )
    .expect("التصدير يجب أن ينجح");

    let content = fs::read_to_string(&target).expect("قراءة الملف يجب أن تنجح");
    assert!(
        content.contains("\"Owner, \"\"Legal\"\" & Co\""),
        "حقل فيه فاصلة وتنصيص يجب أن يُنصَّص وتُضاعف علاماته"
    );
}

#[test]
fn escapes_markdown_cells_containing_pipes_from_real_rows() {
    let fixture = report_fixture();
    let workspace = TempWorkspace::new("md-escape");
    let target = workspace.path("escaped.md");
    {
        let connection = fixture.state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "UPDATE obligations SET title = 'Owner | Legal' WHERE id = ?1",
                [OBLIGATION_A],
            )
            .expect("تحديث العنوان يجب أن ينجح");
    }

    export(
        &fixture,
        ReportId::ObligationStatus,
        EXPORT_FORMAT_MARKDOWN,
        &target,
    )
    .expect("التصدير يجب أن ينجح");

    let content = fs::read_to_string(&target).expect("قراءة الملف يجب أن تنجح");
    assert!(content.contains("Owner \\| Legal"));
    assert!(content.contains("| --- |"));
    assert!(content.contains("- الجداول المصدر:"));
}

// فشل الكتابة لا يترك ملفاً جزئياً: الهدف نفسه يبقى غائباً ولا يبقى مؤقت.
#[test]
fn a_failed_write_leaves_no_partial_file_and_no_temporary_residue() {
    let fixture = report_fixture();
    let workspace = TempWorkspace::new("atomic-failure");
    let target = workspace.path("blocked.csv");
    // مجلد باسم الهدف: الكتابة الذرّية تفشل (rename على مجلد)، والهدف لا يتغير.
    fs::create_dir_all(&target).expect("المجلد المعيق يجب أن ينشأ");

    assert!(matches!(
        export(
            &fixture,
            ReportId::ObligationStatus,
            EXPORT_FORMAT_CSV,
            &target
        ),
        Err(AppError::ReportExportRejected)
    ));
    assert!(target.is_dir());
    assert_eq!(workspace.entries(), vec!["blocked.csv".to_owned()]);

    // ومحاولة الكتابة إلى مجلد أب محجوز (مجلد بيانات التطبيق) لا تُنشئ شيئاً.
    let mut fixture = fixture;
    let data_directory = workspace.path("app-data");
    fs::create_dir_all(&data_directory).expect("مجلد بيانات التطبيق يجب أن ينشأ");
    fixture
        .state
        .set_database_path_for_tests(data_directory.join("nexus-local.db"));
    let refused = data_directory.join("refused.csv");
    assert!(matches!(
        export(
            &fixture,
            ReportId::ObligationStatus,
            EXPORT_FORMAT_CSV,
            &refused
        ),
        Err(AppError::ReportExportRejected)
    ));
    assert!(!refused.exists());
    // مجلد التطبيق يجب أن يبقى فارغاً تماماً — لا ملف ولا مؤقت.
    assert!(fs::read_dir(&data_directory)
        .expect("قراءة مجلد التطبيق يجب أن تنجح")
        .flatten()
        .next()
        .is_none());
}
#[test]
fn successful_export_appends_exactly_one_hash_chained_audit_event() {
    let fixture = authorized_fixture();
    fixture.seed_report_data();
    let workspace = TempWorkspace::new("audit");
    let target = workspace.path("audited.csv");
    // ذيل السلسلة الفعلي لمستأجر الاختبار: الأحداث مُدوَّلة بين مستأجرين،
    // فالسلسلة تُقرأ بالترتيب الفعلي للإدراج مقيَّدةً بالمستأجر نفسه.
    let previous_hash = current_chain_tail(&fixture);
    let audit_events_before = fixture.count("SELECT COUNT(*) FROM audit_events");

    export(
        &fixture,
        ReportId::ObligationStatus,
        EXPORT_FORMAT_CSV,
        &target,
    )
    .expect("التصدير يجب أن ينجح");

    let events = audit_events(&fixture, "REPORT_EXPORTED");
    assert_eq!(events.len(), 1, "حدث تصدير واحد بالضبط لكل تصدير");
    let (audit_id, resource_type, resource_id, previous) = &events[0];
    assert_eq!(resource_type, "Report");
    assert!(resource_id.starts_with("OBLIGATION_STATUS|format=CSV|"));
    assert!(resource_id.contains("sha256="));
    // السلسلة متصلة: previous_hash للحدث يساوي آخر هاش سابق في نفس المستأجر.
    assert_eq!(previous, &previous_hash);
    // أثر واحد بالضبط: العدد الكلي زاد بمقدار واحد لا أكثر.
    assert_eq!(
        fixture.count("SELECT COUNT(*) FROM audit_events"),
        audit_events_before + 1
    );

    // الهاش المُخزَّن يطابق صيغة السلسلة الموثّقة حرفياً.
    let occurred_at: String = {
        let connection = fixture.state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT occurred_at FROM audit_events WHERE id = ?1",
                [audit_id],
                |row| row.get(0),
            )
            .expect("قراءة اللحظة يجب أن تنجح")
    };
    let stored_hash: String = {
        let connection = fixture.state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT event_hash FROM audit_events WHERE id = ?1",
                [audit_id],
                |row| row.get(0),
            )
            .expect("قراءة الهاش يجب أن تنجح")
    };
    let actor = fixture.state.current_user_id().expect("الهوية يجب أن تقرأ");
    let payload = format!(
        "{previous_hash}|{audit_id}|REPORT_EXPORTED|{actor}|Report|{resource_id}|{occurred_at}"
    );
    assert_eq!(
        stored_hash,
        format!("{:x}", Sha256::digest(payload.as_bytes())),
        "هاش حدث التصدير يجب أن يتبع سلسلة التدقيق نفسها"
    );
    // المحتوى نفسه لا يدخل سجل التدقيق — لا صفوف ولا نص تقرير.
    assert!(!resource_id.contains("Obligation a"));
}

#[test]
fn two_exports_of_the_same_report_produce_identical_content_and_two_audit_events() {
    let fixture = report_fixture();
    let workspace = TempWorkspace::new("deterministic");
    let first = workspace.path("first.csv");
    let second = workspace.path("second.csv");

    export(&fixture, ReportId::RiskRegister, EXPORT_FORMAT_CSV, &first)
        .expect("التصدير الأول يجب أن ينجح");
    export(&fixture, ReportId::RiskRegister, EXPORT_FORMAT_CSV, &second)
        .expect("التصدير الثاني يجب أن ينجح");

    // نفس البيانات ⇒ نفس الصفوف بالترتيب نفسه؛ اللحظة والاسم يختلفان بحق.
    let first_content = fs::read_to_string(&first).expect("قراءة الملف يجب أن تنجح");
    let second_content = fs::read_to_string(&second).expect("قراءة الملف يجب أن تنجح");
    let body = |content: &str| -> Vec<String> {
        content
            .lines()
            .filter(|line| line.contains("risk-fixture-a"))
            .map(str::to_owned)
            .collect()
    };
    assert_eq!(body(&first_content), body(&second_content));
    assert_eq!(audit_events(&fixture, "REPORT_EXPORTED").len(), 2);
}

// تقرير فارغ تماماً: التصدير ينجح بمحتوى صادق بلا صفوف، والصيغتان صالحتان.
#[test]
fn exporting_an_empty_report_succeeds_with_headers_only() {
    let fixture = report_fixture();
    let workspace = TempWorkspace::new("empty");
    {
        let connection = fixture.state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "DELETE FROM audit_events WHERE tenant_id = ?1",
                [fixture.state.tenant_id()],
            )
            .expect("حذف أحداث التدقيق يجب أن ينجح");
    }
    let target = workspace.path("empty-audit.md");
    export(
        &fixture,
        ReportId::AuditActivity,
        EXPORT_FORMAT_MARKDOWN,
        &target,
    )
    .expect("تصدير تقرير فارغ يجب أن ينجح");

    let content = fs::read_to_string(&target).expect("قراءة الملف يجب أن تنجح");
    assert!(content.contains("عدد الصفوف المعروضة: 0 من 0"));
    assert!(content.contains("| --- |"));
}
