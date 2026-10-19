use crate::domain::{
    error::AppError,
    reports::{ReportData, ReportFormat},
};

// =====================================================================
// تحويل تقرير إلى محتوى ملف — CSV أو Markdown فقط.
//
// قواعد السلامة:
//   * هروب CSV صحيح: أي حقل يحوي فاصلة أو علامة تنصيص أو سطراً جديداً
//     يُحاط بعلامتي تنصيص وتُضاعف علاماته الداخلية.
//   * جدول Markdown صحيح: الشرطة العمودية داخل الخلية تُهرَّب `\|`،
//     وفواصل الأسطر تُستبدل بمسافة حتى لا ينكسر الجدول.
//   * المحتوى يُبنى بترتيب ثابت (أقسام ثم صفوف) فلا يختلف الملف بين تشغيلين
//     لنفس البيانات، وبذلك يبقى هاش SHA-256 قابلاً للمقارنة.
//   * الملف يحمل منشأه كاملاً: الجداول المصدر ولحظة التوليد والوسم الصريح.
// =====================================================================

/// أعلى طول مقبول لخلية واحدة قبل الرفض — حماية من تضخّم غير محدود للملف.
const MAX_CELL_CHARS: usize = 4_000;

/// تحويل التقرير إلى بايتات الملف المطلوب تنسيقه.
pub fn render(report: &ReportData, format: ReportFormat) -> Result<Vec<u8>, AppError> {
    verify_cells(report)?;
    let text = match format {
        ReportFormat::Csv => render_csv(report),
        ReportFormat::Markdown => render_markdown(report),
    };
    Ok(text.into_bytes())
}

/// الخلايا المرفوضة: محارف تحكم تكسر البنية أو خلية أطول من الحدّ.
fn verify_cells(report: &ReportData) -> Result<(), AppError> {
    for section in &report.sections {
        if section.columns.is_empty() {
            return Err(AppError::ValidationFailed);
        }
        for row in &section.rows {
            // صف بعدد أعمدة مختلف يعني بنية قسم غير متسقة — فشل مغلق لا ملف مشوّه.
            if row.len() != section.columns.len() {
                return Err(AppError::ValidationFailed);
            }
            for cell in row {
                if cell.chars().any(is_forbidden_character) || cell.chars().count() > MAX_CELL_CHARS
                {
                    return Err(AppError::ValidationFailed);
                }
            }
        }
    }
    Ok(())
}

// محارف التحكم الممنوعة: نسمح بالمسافة فقط بين المحارف غير المرئية،
// وأي تبويب/سطر جديد داخل خلية يُرفض بدل إنتاج ملف مشوّه.
fn is_forbidden_character(character: char) -> bool {
    character.is_control()
}

fn render_csv(report: &ReportData) -> String {
    let mut output = String::new();
    push_line(&mut output, &[report.title.clone()]);
    push_line(
        &mut output,
        &[
            report.report_id.clone(),
            format!("generatedAt={}", report.provenance.generated_at),
            format!("dataMode={}", report.provenance.data_mode),
        ],
    );
    push_line(
        &mut output,
        &[format!(
            "sourceTables={}",
            report.provenance.source_tables.join(" | ")
        )],
    );
    push_line(&mut output, &[report.provenance.disclaimer.clone()]);

    for section in &report.sections {
        output.push('\n');
        push_line(&mut output, &[section.title.clone()]);
        let header: Vec<String> = section
            .columns
            .iter()
            .map(|column| column.label.clone())
            .collect();
        push_line(&mut output, &header);
        for row in &section.rows {
            push_line(&mut output, row);
        }
        push_line(
            &mut output,
            &[format!(
                "shownRows={};totalRows={};truncated={}",
                section.rows.len(),
                section.total_rows,
                section.truncated
            )],
        );
    }
    output
}

fn push_line(output: &mut String, fields: &[String]) {
    let escaped: Vec<String> = fields.iter().map(|field| escape_csv(field)).collect();
    output.push_str(&escaped.join(","));
    output.push_str("\r\n");
}

/// هروب CSV قياسي (RFC 4180): التنصيص عند وجود فاصلة أو تنصيص أو سطر جديد،
/// وتُضاعف علامات التنصيص الداخلية.
pub fn escape_csv(field: &str) -> String {
    let needs_quoting =
        field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r');
    if needs_quoting {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_owned()
    }
}

fn render_markdown(report: &ReportData) -> String {
    let mut output = String::new();
    output.push_str(&format!("# {}\n\n", escape_markdown_text(&report.title)));
    output.push_str(&format!("- معرّف التقرير: `{}`\n", report.report_id));
    output.push_str(&format!(
        "- لحظة التوليد: `{}`\n",
        escape_markdown_text(&report.provenance.generated_at)
    ));
    output.push_str(&format!(
        "- وضع البيانات: `{}`\n",
        escape_markdown_text(&report.provenance.data_mode)
    ));
    output.push_str(&format!(
        "- الجداول المصدر: {}\n",
        escape_markdown_text(&report.provenance.source_tables.join(", "))
    ));
    output.push_str(&format!(
        "\n> {}\n",
        escape_markdown_text(&report.provenance.disclaimer)
    ));

    for section in &report.sections {
        output.push_str(&format!(
            "\n## {}\n\n",
            escape_markdown_text(&section.title)
        ));
        let header: Vec<String> = section
            .columns
            .iter()
            .map(|column| escape_markdown_cell(&column.label))
            .collect();
        output.push_str(&format!("| {} |\n", header.join(" | ")));
        let separators: Vec<&str> = section.columns.iter().map(|_| "---").collect();
        output.push_str(&format!("| {} |\n", separators.join(" | ")));
        for row in &section.rows {
            let cells: Vec<String> = row.iter().map(|cell| escape_markdown_cell(cell)).collect();
            output.push_str(&format!("| {} |\n", cells.join(" | ")));
        }
        output.push_str(&format!(
            "\nعدد الصفوف المعروضة: {} من {} — اقتُطع: {}\n",
            section.rows.len(),
            section.total_rows,
            if section.truncated { "نعم" } else { "لا" }
        ));
    }
    output
}

/// هروب خلية Markdown: الشرطة العمودية تُهرَّب، وفواصل الأسطر تُطوى.
pub fn escape_markdown_cell(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace(['\n', '\r'], " ")
}

/// هروب نص Markdown غير الجداول: العناوين والوسوم.
pub fn escape_markdown_text(value: &str) -> String {
    value.replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::{escape_csv, escape_markdown_cell, render, MAX_CELL_CHARS};
    use crate::domain::reports::{
        ReportColumn, ReportData, ReportFormat, ReportProvenance, ReportSection,
    };

    fn report_with_row(row: Vec<&str>) -> ReportData {
        ReportData {
            report_id: "OBLIGATION_STATUS".to_owned(),
            title: "حالة الالتزامات".to_owned(),
            sections: vec![ReportSection {
                title: "سجل الالتزامات".to_owned(),
                columns: vec![ReportColumn {
                    key: "a".to_owned(),
                    label: "A".to_owned(),
                }],
                rows: vec![row.into_iter().map(str::to_owned).collect()],
                total_rows: 1,
                truncated: false,
            }],
            provenance: ReportProvenance {
                data_mode: "DEMO_SYNTHETIC".to_owned(),
                source_tables: vec!["obligations".to_owned()],
                generated_at: "2026-10-15T00:00:00+00:00".to_owned(),
                disclaimer: "ملخص تشغيلي محلي لا إقرار امتثال.".to_owned(),
            },
        }
    }

    // الفاصلة وعلامة التنصيص والسطر الجديد: تنصيص + مضاعفة العلامات — RFC 4180.
    #[test]
    fn csv_escaping_quotes_commas_quotes_and_newlines() {
        assert_eq!(escape_csv("plain"), "plain");
        assert_eq!(escape_csv("a,b"), "\"a,b\"");
        assert_eq!(escape_csv("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(escape_csv("line1\nline2"), "\"line1\nline2\"");
        assert_eq!(escape_csv("carriage\rreturn"), "\"carriage\rreturn\"");
    }

    #[test]
    fn markdown_escaping_neutralises_pipes_and_line_breaks() {
        assert_eq!(escape_markdown_cell("a|b"), "a\\|b");
        assert_eq!(escape_markdown_cell("a\nb"), "a b");
        assert_eq!(escape_markdown_cell("a\\b"), "a\\\\b");
    }

    #[test]
    fn csv_output_carries_provenance_and_real_row_count() {
        let bytes =
            render(&report_with_row(vec!["x"]), ReportFormat::Csv).expect("توليد CSV يجب أن ينجح");
        let text = String::from_utf8(bytes).expect("المحتوى يجب أن يكون نصاً صالحاً");
        assert!(text.contains("DEMO_SYNTHETIC"));
        assert!(text.contains("obligations"));
        assert!(text.contains("shownRows=1;totalRows=1;truncated=false"));
        assert!(text.contains("2026-10-15T00:00:00+00:00"));
    }

    #[test]
    fn markdown_output_is_a_valid_table_with_escaped_pipes() {
        let bytes = render(&report_with_row(vec!["a|b"]), ReportFormat::Markdown)
            .expect("توليد Markdown يجب أن ينجح");
        let text = String::from_utf8(bytes).expect("المحتوى يجب أن يكون نصاً صالحاً");
        assert!(text.contains("| A |"));
        assert!(text.contains("| --- |"));
        assert!(text.contains("| a\\|b |"));
        assert!(text.contains("عدد الصفوف المعروضة: 1 من 1"));
    }

    // خلية بمحارف تحكم أو بحجم مفرط تُرفض قبل أي كتابة ملف.
    #[test]
    fn rejects_control_characters_and_oversized_cells() {
        assert!(render(&report_with_row(vec!["tab\there"]), ReportFormat::Csv).is_err());
        let oversized = "x".repeat(MAX_CELL_CHARS + 1);
        assert!(render(&report_with_row(vec![&oversized]), ReportFormat::Csv).is_err());
    }

    // قسم بأعمدة فارغة أو صف بعدد أعمدة مختلف = بنية غير متسقة ⇒ رفض.
    #[test]
    fn rejects_structurally_inconsistent_sections() {
        let mut report = report_with_row(vec!["x", "y"]);
        assert!(render(&report, ReportFormat::Csv).is_err());

        report.sections[0].rows[0] = vec!["x".to_owned()];
        report.sections[0].columns.clear();
        assert!(render(&report, ReportFormat::Csv).is_err());
    }
}
