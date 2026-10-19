// =====================================================================
// GAP-05-OPEN — بيانات المؤسسة المعلنة من المستخدم (الدولة والقطاع)
// ---------------------------------------------------------------------
// السياسة الحاكمة لصدق البيانات ومنشئها:
//   * لا تُكتب في جدول `organizations` أي قيمة لم يُدخلها المستخدم.
//   * لا قيمة افتراضية، ولا استنتاج دولة أو قطاع من سياق آخر، ولا استبدال
//     قيمة قديمة بقيمة مختلقة.
//   * كل ما لا يجتاز التحقق هنا يُرفض فشلاً مغلقاً قبل أي كتابة.
//
// حدود V1 المعلنة:
//   * الدولة: كتالوج ISO 3166-1 alpha-2 الرسمي (249 رمزاً مخصصاً)، ولا يُقبل
//     رمز خارج الكتالوج ولا رمز محجوز/غير مخصص.
//   * القطاع: نص يعلنه المستخدم نفسه. لم يُضف تصنيف اقتصادي رسمي (ISIC أو ما
//     يعادله) لأن ذلك يتطلب مصدراً رسمياً وتصميماً مستقلاً — وهذا خارج نطاق
//     إصلاح هذه الفجوة.
// =====================================================================

use crate::domain::error::AppError;

/// كتالوج ISO 3166-1 alpha-2 — الرموز المخصصة رسمياً فقط (249 رمزاً)، مرتبة.
const ISO_3166_1_ALPHA2: [&str; 249] = [
    "AD", "AE", "AF", "AG", "AI", "AL", "AM", "AO", "AQ", "AR", "AS", "AT", "AU", "AW", "AX", "AZ",
    "BA", "BB", "BD", "BE", "BF", "BG", "BH", "BI", "BJ", "BL", "BM", "BN", "BO", "BQ", "BR", "BS",
    "BT", "BV", "BW", "BY", "BZ", "CA", "CC", "CD", "CF", "CG", "CH", "CI", "CK", "CL", "CM", "CN",
    "CO", "CR", "CU", "CV", "CW", "CX", "CY", "CZ", "DE", "DJ", "DK", "DM", "DO", "DZ", "EC", "EE",
    "EG", "EH", "ER", "ES", "ET", "FI", "FJ", "FK", "FM", "FO", "FR", "GA", "GB", "GD", "GE", "GF",
    "GG", "GH", "GI", "GL", "GM", "GN", "GP", "GQ", "GR", "GS", "GT", "GU", "GW", "GY", "HK", "HM",
    "HN", "HR", "HT", "HU", "ID", "IE", "IL", "IM", "IN", "IO", "IQ", "IR", "IS", "IT", "JE", "JM",
    "JO", "JP", "KE", "KG", "KH", "KI", "KM", "KN", "KP", "KR", "KW", "KY", "KZ", "LA", "LB", "LC",
    "LI", "LK", "LR", "LS", "LT", "LU", "LV", "LY", "MA", "MC", "MD", "ME", "MF", "MG", "MH", "MK",
    "ML", "MM", "MN", "MO", "MP", "MQ", "MR", "MS", "MT", "MU", "MV", "MW", "MX", "MY", "MZ", "NA",
    "NC", "NE", "NF", "NG", "NI", "NL", "NO", "NP", "NR", "NU", "NZ", "OM", "PA", "PE", "PF", "PG",
    "PH", "PK", "PL", "PM", "PN", "PR", "PS", "PT", "PW", "PY", "QA", "RE", "RO", "RS", "RU", "RW",
    "SA", "SB", "SC", "SD", "SE", "SG", "SH", "SI", "SJ", "SK", "SL", "SM", "SN", "SO", "SR", "SS",
    "ST", "SV", "SX", "SY", "SZ", "TC", "TD", "TF", "TG", "TH", "TJ", "TK", "TL", "TM", "TN", "TO",
    "TR", "TT", "TV", "TW", "TZ", "UA", "UG", "UM", "US", "UY", "UZ", "VA", "VC", "VE", "VG", "VI",
    "VN", "VU", "WF", "WS", "YE", "YT", "ZA", "ZM", "ZW",
];

/// حدود طول القطاع: نص مقروء قصير يعلنه المستخدم — لا نص طويل حر.
const INDUSTRY_MIN_CHARS: usize = 2;
const INDUSTRY_MAX_CHARS: usize = 80;

/// العنصر النائب التاريخي الذي كان يُزرع تلقائياً قبل هذا الإصلاح.
/// لم يعد قيمة صالحة: النظام لا يعتبره قطاعاً حقيقياً، ووجوده في صف قديم
/// يُقرأ كما هو (بلا تصحيح تلقائي) ولا يُنشأ من جديد.
const LEGACY_UNSPECIFIED_PLACEHOLDER: &str = "UNSPECIFIED";

/// المحارف غير الأبجدية الرقمية المسموح بها داخل اسم القطاع.
/// القائمة ضيقة عمداً: تسمح بعلامات الأسماء التجارية الشائعة وتمنع كل ما
/// يمكن أن يُفهم كوسم (HTML) أو كبنية نصية غير مطلوبة.
const INDUSTRY_ALLOWED_PUNCTUATION: [char; 10] =
    [' ', '&', '\'', ',', '.', '-', '/', '(', ')', '+'];

/// ملف المؤسسة كما أدخله المستخدم بعد التطبيع — القيمة الوحيدة التي تُكتب.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationProfile {
    pub country_code: String,
    pub industry: String,
}

/// كتالوج الدول المتاح للواجهة — يُقرأ من الخلف ليبقى مصدر التحقق واحداً.
pub fn supported_country_codes() -> &'static [&'static str] {
    &ISO_3166_1_ALPHA2
}

/// تطبيع رمز الدولة: قصّ الفراغ ثم رفع الحالة اللاتينية، ثم التحقق من عضوية
/// الكتالوج. أي رمز فارغ أو غير قياسي أو غير مخصص يُرفض برمز خطأ مستقل.
pub fn normalize_country_code(raw: &str) -> Result<String, AppError> {
    let code = raw.trim().to_ascii_uppercase();
    if code.chars().count() != 2 || !code.chars().all(|character| character.is_ascii_uppercase()) {
        return Err(AppError::BootstrapCountryInvalid);
    }
    if !ISO_3166_1_ALPHA2.contains(&code.as_str()) {
        return Err(AppError::BootstrapCountryInvalid);
    }
    Ok(code)
}

/// تطبيع اسم القطاع: توحيد المسافات، ثم حدّ الطول، ثم منع النص غير الصالح.
/// لا تُقبل قيمة فارغة ولا مسافات وحدها ولا العنصر النائب التاريخي.
pub fn normalize_industry(raw: &str) -> Result<String, AppError> {
    // `split_whitespace` يسقط الفراغات الطرفية ويحوّل كل تسلسل مسافات
    // (بما فيها الجدولة والأسطر) إلى فاصل واحد عند إعادة الجمع.
    let normalized = raw.split_whitespace().collect::<Vec<&str>>().join(" ");
    let length = normalized.chars().count();
    if !(INDUSTRY_MIN_CHARS..=INDUSTRY_MAX_CHARS).contains(&length) {
        return Err(AppError::BootstrapIndustryInvalid);
    }
    if normalized.to_ascii_uppercase() == LEGACY_UNSPECIFIED_PLACEHOLDER {
        return Err(AppError::BootstrapIndustryInvalid);
    }
    let mut has_alphanumeric = false;
    for character in normalized.chars() {
        if character.is_alphanumeric() {
            has_alphanumeric = true;
            continue;
        }
        if INDUSTRY_ALLOWED_PUNCTUATION.contains(&character) {
            continue;
        }
        return Err(AppError::BootstrapIndustryInvalid);
    }
    // قطاع من علامات ترقيم فقط ليس قطاعاً.
    if !has_alphanumeric {
        return Err(AppError::BootstrapIndustryInvalid);
    }
    Ok(normalized)
}

/// نقطة التحقق الوحيدة التي يستعملها مسار الإقلاع: لا كتابة قبل نجاحها.
pub fn validate_organization_profile(
    country_code: &str,
    industry: &str,
) -> Result<OrganizationProfile, AppError> {
    Ok(OrganizationProfile {
        country_code: normalize_country_code(country_code)?,
        industry: normalize_industry(industry)?,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_country_code, normalize_industry, supported_country_codes,
        validate_organization_profile,
    };
    use crate::domain::error::AppError;

    #[test]
    fn catalog_is_exactly_the_iso_3166_1_alpha_2_officially_assigned_set() {
        let codes = supported_country_codes();
        assert_eq!(codes.len(), 249, "الكتالوج يجب أن يطابق المجموعة الرسمية");
        assert!(
            codes
                .iter()
                .all(|code| code.len() == 2 && code.chars().all(|c| c.is_ascii_uppercase())),
            "كل رمز يجب أن يكون حرفين لاتينيين كبيرين"
        );
        let unique: std::collections::BTreeSet<&&str> = codes.iter().collect();
        assert_eq!(unique.len(), codes.len(), "لا تكرار في الكتالوج");
        let mut sorted = codes.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, codes.to_vec(), "الكتالوج يجب أن يبقى مرتباً");
    }

    #[test]
    fn country_codes_are_normalized_but_never_invented() {
        assert_eq!(normalize_country_code("sa").expect("الحالة تُرفع"), "SA");
        assert_eq!(normalize_country_code("  ae  ").expect("الفراغ يُقص"), "AE");
        assert_eq!(normalize_country_code("QA").expect("قطر"), "QA");

        // محجوز/غير مخصص، طول خاطئ، رمز رقمي، وحقن نصي — كلها مرفوضة.
        for invalid in [
            "", "   ", "X", "SAU", "1A", "S1", "XX", "ZZ", "S-A", "SA'", "sa;",
        ] {
            assert!(
                matches!(
                    normalize_country_code(invalid),
                    Err(AppError::BootstrapCountryInvalid)
                ),
                "الرمز غير الصالح يجب أن يُرفض: {invalid:?}"
            );
        }
        // رمز عربي غير لاتيني مرفوض أيضاً.
        assert!(matches!(
            normalize_country_code("سع"),
            Err(AppError::BootstrapCountryInvalid)
        ));
    }

    #[test]
    fn industry_is_normalized_and_html_like_text_is_rejected() {
        assert_eq!(
            normalize_industry("  Financial   Services  ").expect("تطبيع المسافات"),
            "Financial Services"
        );
        assert_eq!(
            normalize_industry("تقنية\tالمعلومات").expect("القطاع العربي مقبول"),
            "تقنية المعلومات"
        );
        assert_eq!(
            normalize_industry("Health Care & Social Work").expect("الشرطة والعلامة &"),
            "Health Care & Social Work"
        );

        for invalid in [
            "",
            "   ",
            "\t\n",
            "A",
            "UNSPECIFIED",
            "unspecified",
            " Unspecified ",
            "---",
            "<script>alert(1)</script>",
            "Banking <b>",
            "Generic \"quoted\"",
            "Semi;colon",
            "back\\slash",
            "tick`mark",
            "emoji \u{1F600}",
            "zero\u{200B}width",
        ] {
            assert!(
                matches!(
                    normalize_industry(invalid),
                    Err(AppError::BootstrapIndustryInvalid)
                ),
                "القطاع غير الصالح يجب أن يُرفض: {invalid:?}"
            );
        }

        // الحدّ الأقصى: 80 محرفاً مقبول و81 مرفوض.
        let max_ok = "a".repeat(80);
        assert!(normalize_industry(&max_ok).is_ok());
        let too_long = "a".repeat(81);
        assert!(matches!(
            normalize_industry(&too_long),
            Err(AppError::BootstrapIndustryInvalid)
        ));
    }

    #[test]
    fn profile_validation_returns_the_declared_values_only() {
        let profile =
            validate_organization_profile("  sa ", " Consulting ").expect("ملف المؤسسة يجب أن يمر");
        assert_eq!(profile.country_code, "SA");
        assert_eq!(profile.industry, "Consulting");

        assert!(matches!(
            validate_organization_profile("SA", "UNSPECIFIED"),
            Err(AppError::BootstrapIndustryInvalid)
        ));
        assert!(matches!(
            validate_organization_profile("XX", "Consulting"),
            Err(AppError::BootstrapCountryInvalid)
        ));
    }
}
