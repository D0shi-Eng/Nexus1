// =====================================================================
// GAP-05-OPEN — كتالوج خيارات الإقلاع
// ---------------------------------------------------------------------
// الواجهة تحتاج قائمة الدول لتعرضها في شاشة تأسيس المؤسسة. مصدرها هنا في
// الخلف لا في ملف ثابت في الواجهة، حتى يبقى كتالوج التحقق واحداً: الرمز الذي
// تعرضه الواجهة هو نفسه الرمز الذي يقبله `bootstrap_production`.
//
// هذا أمر قراءة فقط: لا يغيّر شيئاً ولا يمنح أي سلطة (نطاقه الإقلاع المحلي).
// =====================================================================

use serde::Serialize;

use crate::{
    domain::{error::AppError, organization_profile},
    infrastructure::database::AppState,
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapOptionsData {
    pub country_codes: Vec<&'static str>,
}

pub fn get_bootstrap_options(_state: &AppState) -> Result<BootstrapOptionsData, AppError> {
    Ok(BootstrapOptionsData {
        country_codes: organization_profile::supported_country_codes().to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::get_bootstrap_options;
    use crate::infrastructure::database::AppState;
    use rusqlite::Connection;

    // أمر الكتالوج لا يلمس قاعدة البيانات إطلاقاً: يُختبر بحالة بلا اتصال حقيقي.
    fn state_without_connection() -> AppState {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        AppState::test_instance(connection, "", "")
    }

    #[test]
    fn publishes_the_full_iso_catalog_without_inventing_or_narrowing_it() {
        let options =
            get_bootstrap_options(&state_without_connection()).expect("كتالوج الإقلاع يجب أن يُقرأ");
        assert_eq!(options.country_codes.len(), 249);
        assert!(options.country_codes.contains(&"SA"));
        assert!(options.country_codes.contains(&"AE"));
        assert!(options.country_codes.contains(&"QA"));
        // رموز محجوزة أو غير مخصصة لا تدخل الكتالوج.
        assert!(!options.country_codes.contains(&"XX"));
        assert!(!options.country_codes.contains(&"ZZ"));
    }
}
