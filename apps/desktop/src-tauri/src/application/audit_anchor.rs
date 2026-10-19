use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{domain::error::AppError, infrastructure::database::AppState};

// =====================================================================
// مرساة سلسلة التدقيق — كشف اقتطاع ذيل السلسلة
// ---------------------------------------------------------------------
// سلسلة SHA-256 الحالية تكشف تعديل حدث، وحذفاً من منتصف السلسلة، وإعادة
// الترتيب، وتغيير المستأجر/الفاعل — لأن كل حدث يحمل تجزئة سابقه. لكنها لا
// تكشف **حذف آخر الأحداث**: لا شيء بعد الذيل يشير إليه.
//
// المرساة هنا مرجع موثوق نسبياً خارج الجداول: ملف صغير في مجلد بيانات
// التطبيق نفسه يحفظ (عدد الأحداث، تجزئة الذيل) عند كل تحقق ناجح. في التحقق
// التالي:
//   * عدد أقل من المرساة            ⇒ اقتطاع ذيل (TRUNCATED)
//   * عدد أكبر/مساوٍ وتجزئة الموضع  ⇒ بادئة مُعادة الكتابة (PREFIX_MISMATCH)
//   * تطابق                         ⇒ MATCH
//   * لا مرساة (أول تشغيل/استعادة)  ⇒ NO_REFERENCE (ليست سلامة ولا تلفاً)
//
// حدود نموذج الثقة المعلنة — لا يُدَّعى أكثر من هذا:
//   * من يملك ملف القاعدة **و**ملف المرساة معاً يستطيع تعديلهما معاً.
//   * الحماية الفعلية ضد مستخدم النظام المالك تتطلب مرساة خارجية (نسخ احتياطي
//     خارجي/ختم HMAC) وهي خارج نطاق V1 ومؤجلة لقرار المالك.
//   * الاستعادة من نسخة احتياطية تُبطل المرساة صراحةً (`reset`) لأن القاعدة
//     المستعادة قد تكون أقدم شرعاً، فلا يجوز اعتبارها اقتطاعاً.
// =====================================================================

const ANCHOR_FILE_NAME: &str = "audit-anchor.json";
// نسخة الصيغة: قراءة ملف بصيغة غير معروفة تُعامل كغياب مرجع لا كاقتطاع.
const ANCHOR_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AnchorDocument {
    version: u32,
    tenant_id: String,
    event_count: u32,
    head_hash: String,
    verified_at: String,
}

// حالات المرساة كما تُعرض للواجهة — نصوص ثابتة لا تُترجم في الخلف.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnchorState {
    Match,
    NoReference,
    Truncated,
    PrefixMismatch,
}

impl AnchorState {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Match => "MATCH",
            Self::NoReference => "NO_REFERENCE",
            Self::Truncated => "TRUNCATED",
            Self::PrefixMismatch => "PREFIX_MISMATCH",
        }
    }

    // هل تُبطل هذه الحالة اعتماد سلامة السلسلة؟ NO_REFERENCE لا تُبطل: غياب
    // المرجع ليس دليلاً على تلاعب، وإعلانه تلاعباً ادعاء كاذب.
    pub(crate) fn contradicts_chain(self) -> bool {
        matches!(self, Self::Truncated | Self::PrefixMismatch)
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AnchorEvaluation {
    pub(crate) state: AnchorState,
    pub(crate) anchored_events: u32,
}

// مسار المرساة يُشتق من مسار قاعدة البيانات الحية فقط — لا يأتي من العميل.
pub(crate) fn anchor_path(state: &AppState) -> Option<PathBuf> {
    state.database_path().and_then(|path| {
        path.parent()
            .map(|directory| directory.join(ANCHOR_FILE_NAME))
    })
}

fn read_document(path: &Path) -> Option<AnchorDocument> {
    let raw = std::fs::read_to_string(path).ok()?;
    let document: AnchorDocument = serde_json::from_str(&raw).ok()?;
    (document.version == ANCHOR_VERSION).then_some(document)
}

fn write_document(path: &Path, document: &AnchorDocument) -> Result<(), AppError> {
    let raw = serde_json::to_string_pretty(document).map_err(|_| AppError::StorageOperation)?;
    // كتابة ذرية: ملف مؤقت ثم إحلال، فلا تبقى مرساة نصف مكتوبة بعد انقطاع.
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, raw).map_err(|_| AppError::StorageOperation)?;
    std::fs::rename(&temporary, path).map_err(|_| AppError::StorageOperation)
}

// تقييم المرساة مقابل الحالة الفعلية. `hash_at` يُرجع تجزئة الحدث في موضع
// إدراجي واحد (1-based) فتتحقق البادئة المؤرشفة دون إعادة قراءة السلسلة كلها.
pub(crate) fn evaluate(
    path: Option<&Path>,
    tenant_id: &str,
    current_count: u32,
    hash_at: impl Fn(u32) -> Result<Option<String>, AppError>,
) -> Result<AnchorEvaluation, AppError> {
    let Some(path) = path else {
        return Ok(AnchorEvaluation {
            state: AnchorState::NoReference,
            anchored_events: 0,
        });
    };
    let Some(document) = read_document(path) else {
        return Ok(AnchorEvaluation {
            state: AnchorState::NoReference,
            anchored_events: 0,
        });
    };
    // مرساة مستأجر آخر (نسخة قاعدة استُبدلت) لا تُقارن بهذا المستأجر.
    if document.tenant_id != tenant_id {
        return Ok(AnchorEvaluation {
            state: AnchorState::NoReference,
            anchored_events: 0,
        });
    }

    if current_count < document.event_count {
        return Ok(AnchorEvaluation {
            state: AnchorState::Truncated,
            anchored_events: document.event_count,
        });
    }

    let anchored_hash = hash_at(document.event_count)?;
    let state = if anchored_hash.as_deref() == Some(document.head_hash.as_str()) {
        AnchorState::Match
    } else {
        AnchorState::PrefixMismatch
    };
    Ok(AnchorEvaluation {
        state,
        anchored_events: document.event_count,
    })
}

// تثبيت المرساة بعد تحقق ناجح — لا يُستدعى أبداً على سلسلة مشكوك فيها،
// وإلا صار التلاعب مرجعاً معتمداً في المرة التالية.
pub(crate) fn persist(
    path: &Path,
    tenant_id: &str,
    event_count: u32,
    head_hash: &str,
) -> Result<(), AppError> {
    let document = AnchorDocument {
        version: ANCHOR_VERSION,
        tenant_id: tenant_id.to_owned(),
        event_count,
        head_hash: head_hash.to_owned(),
        verified_at: chrono::Utc::now().to_rfc3339(),
    };
    write_document(path, &document)
}

// إبطال المرساة صراحةً (استعادة نسخة احتياطية): القاعدة المستعادة قد تكون
// أقدم شرعاً، فمقارنتها بمرساة أحدث تُنتج اقتطاعاً كاذباً.
pub fn reset(state: &AppState) {
    if let Some(path) = anchor_path(state) {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{evaluate, persist, read_document, AnchorState, ANCHOR_VERSION};

    fn temporary_directory(label: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("nexus-anchor-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("مجلد الاختبار يجب أن ينشأ");
        directory
    }

    #[test]
    fn anchor_round_trips_through_the_file() {
        let directory = temporary_directory("roundtrip");
        let path = directory.join("audit-anchor.json");
        persist(&path, "tenant-a", 7, "hash-7").expect("المرساة يجب أن تُكتب");
        let document = read_document(&path).expect("المرساة يجب أن تُقرأ");
        assert_eq!(document.version, ANCHOR_VERSION);
        assert_eq!(document.tenant_id, "tenant-a");
        assert_eq!(document.event_count, 7);
        assert_eq!(document.head_hash, "hash-7");
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn missing_or_foreign_anchor_is_no_reference_not_tampering() {
        let directory = temporary_directory("noref");
        let path = directory.join("audit-anchor.json");
        let absent = evaluate(Some(&path), "tenant-a", 3, |_| Ok(None)).expect("التقييم");
        assert_eq!(absent.state, AnchorState::NoReference);
        persist(&path, "tenant-other", 3, "hash-3").expect("المرساة يجب أن تُكتب");
        let foreign = evaluate(Some(&path), "tenant-a", 3, |_| Ok(None)).expect("التقييم");
        assert_eq!(foreign.state, AnchorState::NoReference);
        assert!(!foreign.state.contradicts_chain());
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn fewer_events_than_the_anchor_is_a_truncated_tail() {
        let directory = temporary_directory("truncated");
        let path = directory.join("audit-anchor.json");
        persist(&path, "tenant-a", 5, "hash-5").expect("المرساة يجب أن تُكتب");
        let evaluation = evaluate(Some(&path), "tenant-a", 4, |position| {
            Ok(Some(format!("hash-{position}")))
        })
        .expect("التقييم");
        assert_eq!(evaluation.state, AnchorState::Truncated);
        assert!(evaluation.state.contradicts_chain());
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn identical_prefix_is_a_match_and_a_rewritten_prefix_is_a_mismatch() {
        let directory = temporary_directory("prefix");
        let path = directory.join("audit-anchor.json");
        persist(&path, "tenant-a", 5, "hash-5").expect("المرساة يجب أن تُكتب");
        let matched = evaluate(Some(&path), "tenant-a", 9, |position| {
            Ok((position == 5).then(|| "hash-5".to_owned()))
        })
        .expect("التقييم");
        assert_eq!(matched.state, AnchorState::Match);
        let rewritten = evaluate(Some(&path), "tenant-a", 5, |_| {
            Ok(Some("hash-x".to_owned()))
        })
        .expect("التقييم");
        assert_eq!(rewritten.state, AnchorState::PrefixMismatch);
        std::fs::remove_dir_all(&directory).ok();
    }
}
