use std::path::Path;

use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    application::authorization_service,
    domain::{
        error::AppError,
        reports::{
            report_scope, ReportCatalogData, ReportCatalogEntry, ReportData, ReportExportResult,
            ReportFormat, ReportId, ALL_REPORT_IDS,
        },
    },
    infrastructure::{
        database::{append_audit_event, latest_audit_event_hash, AppState},
        report_export_writer::{self, validate_target},
        report_queries::{self, build_provenance},
        report_renderer,
    },
};

// =====================================================================
// خدمة التقارير المحلية — خدمة تطبيقية واحدة بمسؤولية واحدة:
//   * get_report_catalog : كتالوج ثابت + الحدّ الأعلى المعلن (قراءة).
//   * generate_report    : بناء تقرير من استعلامات حقيقية (قراءة).
//   * export_report      : توليد ← تحقق مسار ← كتابة ذرّية ← تدقيق مُسلسَل.
//
// كل مسار كتابة يمر بالترتيب الإلزامي:
//   تفويض ← تحقق ← معاملة واحدة ← تدقيق بسلسلة هاش ← تثبيت.
// =====================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateReportInput {
    pub report_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReportInput {
    pub report_id: String,
    pub format: String,
    pub target_path: String,
}

// الكتالوج يُقرأ من الكتالوج الثابت نفسه — لا نسخة موازية تنحرف عنه.
pub fn get_report_catalog(state: &AppState) -> Result<ReportCatalogData, AppError> {
    authorization_service::require_permission(state, "REPORT", "READ")?;

    let reports = ALL_REPORT_IDS
        .iter()
        .map(|report_id| {
            let scope = report_scope(*report_id);
            ReportCatalogEntry {
                report_id: scope.report_id.as_str().to_owned(),
                title: scope.title.to_owned(),
                description: scope.description.to_owned(),
            }
        })
        .collect();

    Ok(ReportCatalogData {
        reports,
        max_rows_per_section: report_queries::max_rows_per_section(),
        generated_at: Utc::now().to_rfc3339(),
    })
}

pub fn generate_report(
    state: &AppState,
    input: GenerateReportInput,
) -> Result<ReportData, AppError> {
    authorization_service::require_permission(state, "REPORT", "READ")?;
    let report_id: ReportId = input.report_id.parse()?;
    build_report(state, report_id)
}

// تصدير ملف: أثر تدقيقي واحد فقط وفي معاملة واحدة بعد نجاح الكتابة.
pub fn export_report(
    state: &AppState,
    input: ExportReportInput,
) -> Result<ReportExportResult, AppError> {
    authorization_service::require_permission(state, "REPORT", "EXPORT")?;

    let report_id: ReportId = input.report_id.parse()?;
    let format: ReportFormat = input.format.parse()?;
    // التحقق من المسار قبل أي توليد — لا عمل بلا وجهة مشروعة.
    let application_data_directory = application_data_directory(state);
    let target = validate_target(
        &input.target_path,
        format,
        application_data_directory.as_deref(),
    )?;

    let report = build_report(state, report_id)?;
    let content = report_renderer::render(&report, format)?;
    let written = report_export_writer::write_atomically(&target, &content)?;

    audit_export(state, report_id, format, &written)?;

    Ok(ReportExportResult {
        file_name: written.file_name,
        bytes: written.bytes,
        sha256: written.sha256,
        format: format.as_str().to_owned(),
    })
}

/// مجلد بيانات التطبيق يُشتق من مسار القاعدة الحية فقط — ولا يأتي من العميل أبداً.
fn application_data_directory(state: &AppState) -> Option<std::path::PathBuf> {
    state
        .database_path()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

fn build_report(state: &AppState, report_id: ReportId) -> Result<ReportData, AppError> {
    build_report_with_limit(state, report_id, report_queries::max_rows_per_section())
}

/// التوليد بحكم بحدّ صفوف صريح — نقطة واحدة تُستعمل في الإنتاج (الحدّ المعلن)
/// وفي الاختبارات (حدّ أصغر يثبت مساري الاقتطاع وعدمه بلا بيانات ضخمة).
pub fn build_report_with_limit(
    state: &AppState,
    report_id: ReportId,
    max_rows: usize,
) -> Result<ReportData, AppError> {
    let generated_at = Utc::now().to_rfc3339();
    let loaded = report_queries::load_report(state, report_id, max_rows)?;
    let scope = report_scope(report_id);
    Ok(ReportData {
        report_id: report_id.as_str().to_owned(),
        title: scope.title.to_owned(),
        sections: loaded.sections,
        provenance: build_provenance(&loaded.source_tables, &loaded.data_mode, &generated_at),
    })
}

// أثر تصدير واحد: resource_id = معرّف التقرير، ولا محتوى ملف في السجل إطلاقاً.
// مسار الملف مُختار من المستخدم وليس سراً، لكننا نُدوّن اسم الملف فقط احتراماً
// لأضيق نطاق ممكن، مع الصيغة والبايتات والهاش.
fn audit_export(
    state: &AppState,
    report_id: ReportId,
    format: ReportFormat,
    written: &report_export_writer::WrittenExport,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let tenant_id = state.tenant_id();
    let user_id = state.current_user_id()?;
    let audit_id = format!("audit_{}", Uuid::new_v4());
    let resource_id = format!(
        "{}|format={}|fileName={}|bytes={}|sha256={}",
        report_id.as_str(),
        format.as_str(),
        written.file_name,
        written.bytes,
        written.sha256
    );

    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let previous_hash = latest_audit_event_hash(&transaction, &tenant_id)?;
    append_audit_event(
        &transaction,
        &tenant_id,
        &audit_id,
        "REPORT_EXPORTED",
        &user_id,
        "Report",
        &resource_id,
        &now,
        &previous_hash,
    )?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

#[cfg(test)]
#[path = "report_service_tests.rs"]
mod tests;
