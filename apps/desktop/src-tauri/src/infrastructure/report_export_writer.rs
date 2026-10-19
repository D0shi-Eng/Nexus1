use std::{
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::{
    error::AppError,
    reports::{ReportFormat, MAX_EXPORT_BYTES, MAX_EXPORT_PATH_CHARS},
};

// =====================================================================
// كاتب ملف التصدير — هذا مسار كتابة ملفات، لذلك القواعد هنا صارمة عمداً:
//
//   1) مسار مطلق فقط: أي مسار نسبي يُرفض (لا تفسير نسبة إلى مجلد غير معلوم).
//   2) الامتداد إلزامي ومطابق للصيغة: .csv للـCSV و.md للـMARKDOWN،
//      والمقارنة غير حساسة لحالة الأحرف.
//   3) لا كتابة داخل مجلد بيانات التطبيق إطلاقاً: لا استبدال لقاعدة البيانات
//      ولا لمخزن الأسرار ولا لملف سجل — والفحص بـcanonicalize فلا تنطلي
//      الروابط الرمزية ولا مسارات `..`.
//   4) المجلد الأب يجب أن يكون موجوداً مسبقاً: لا mkdir -p على شجرة عشوائية.
//   5) الكتابة ذرّية: ملف مؤقت في المجلد نفسه ثم rename؛ وأي فشل يُنظّف
//      الملف المؤقت فلا يبقى أثر جزئي.
//   6) حدّ حجم صلب قبل الكتابة، والطول الفعلي وهاش SHA-256 يُحسبان من القرص
//      لا من الذاكرة.
// =====================================================================

/// نتيجة كتابة حقيقية: اسم الملف والطول والهاش من القرص بعد الكتابة.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenExport {
    pub file_name: String,
    pub bytes: u64,
    pub sha256: String,
}

/// المسار الأب المُتحقَّق منه + الملف الهدف — لا يُبنى المسار النهائي من نص العميل.
pub struct ExportTarget {
    parent: PathBuf,
    path: PathBuf,
}

impl ExportTarget {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn parent(&self) -> &Path {
        &self.parent
    }
}

/// تحقق المسار المطلوب مقابل الصيغة ومجلد بيانات التطبيق.
pub fn validate_target(
    raw_path: &str,
    format: ReportFormat,
    application_data_directory: Option<&Path>,
) -> Result<ExportTarget, AppError> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_EXPORT_PATH_CHARS {
        return Err(AppError::ReportExportRejected);
    }
    let path = PathBuf::from(trimmed);
    // (1) مسار مطلق فقط.
    if !path.is_absolute() {
        return Err(AppError::ReportExportRejected);
    }
    // (2) لا مكوّن `..` في أي موضع — لا صعود إلى شجرة أخرى.
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(AppError::ReportExportRejected);
    }
    // (2) الامتداد مطابق للصيغة، غير حساس لحالة الأحرف.
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or(AppError::ReportExportRejected)?
        .to_lowercase();
    if extension != format.required_extension() {
        return Err(AppError::ReportExportRejected);
    }
    // (4) المجلد الأب موجود فعلاً — لا إنشاء شجرة مجلدات.
    let parent = path
        .parent()
        .ok_or(AppError::ReportExportRejected)?
        .to_path_buf();
    let parent_metadata = fs::metadata(&parent).map_err(|_| AppError::ReportExportRejected)?;
    if !parent_metadata.is_dir() {
        return Err(AppError::ReportExportRejected);
    }
    let canonical_parent = fs::canonicalize(&parent).map_err(|_| AppError::ReportExportRejected)?;

    // (3) مجلد بيانات التطبيق (القاعدة + مخزن الأسرار + السجلات) محجوز بالكامل.
    if let Some(data_directory) = application_data_directory {
        let canonical_data =
            fs::canonicalize(data_directory).map_err(|_| AppError::ReportExportRejected)?;
        if canonical_parent == canonical_data {
            return Err(AppError::ReportExportRejected);
        }
        for reserved in ["nexus-local.db", "credentials", "logs"] {
            if canonical_parent == canonical_data.join(reserved) {
                return Err(AppError::ReportExportRejected);
            }
        }
    }

    // الملف الهدف إن وُجد: لا نصعد عبر رابط رمزي ولا نستبدل ملفاً ليس ملفاً عادياً.
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(AppError::ReportExportRejected);
        }
    }

    Ok(ExportTarget { parent, path })
}

/// كتابة ذرّية: ملف مؤقت في المجلد نفسه ثم rename فوق الهدف.
pub fn write_atomically(target: &ExportTarget, content: &[u8]) -> Result<WrittenExport, AppError> {
    // (6) حدّ الحجم قبل أي فتح ملف.
    if content.len() > MAX_EXPORT_BYTES {
        return Err(AppError::ReportExportRejected);
    }
    let parent = target.parent();
    let file_name = target
        .path()
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .ok_or(AppError::ReportExportRejected)?;

    let temporary_path = parent.join(format!(".nexus-report-{}.tmp", Uuid::new_v4()));
    // لا تفريع: إمّا تُكتب كل البايتات أو يُرفع الخطأ ويُنظّف المؤقت.
    let write_result = fs::write(&temporary_path, content);
    if write_result.is_err() {
        remove_quietly(&temporary_path);
        return Err(AppError::ReportExportFailed);
    }

    // تحقق أخير: المؤقت ملف عادي (لا رابط رمزي) وأبوه الفعلي هو نفس المجلد
    // المُتحقَّق منه. المقارنة على المسار الأب لا على البادئة النصية، لأن
    // `canonicalize` على ويندوز يعيد بادئة `\\?\` التي لا تظهر في مسارنا.
    //
    // العيب الذي كشفه CI (2026-10-18): كان أحد الطرفين مُطبَّعاً فقط. على
    // أنظمة تُسمّى فيها مجلدات مؤقتة بصيغة 8.3 (مثل `C:\Users\RUNNER~1\...`)
    // يُعيد canonicalize الاسم الطويل بينما يبقى `parent` بالاسم القصير،
    // فتُرفض وجهة تصدير سليمة. الإصلاح: تطبيع الطرفين قبل المقارنة.
    let temporary_is_regular = fs::symlink_metadata(&temporary_path)
        .map(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false);
    let resolved_temporary_parent = fs::canonicalize(&temporary_path)
        .ok()
        .and_then(|canonical| canonical.parent().map(|resolved| resolved.to_path_buf()))
        .map(|resolved_parent| strip_verbatim_prefix(&resolved_parent));
    let resolved_declared_parent = fs::canonicalize(parent)
        .ok()
        .map(|resolved| strip_verbatim_prefix(&resolved));
    let temporary_parent_matches = match (resolved_temporary_parent, resolved_declared_parent) {
        (Some(temporary_parent), Some(declared_parent)) => temporary_parent == declared_parent,
        _ => false,
    };
    if !temporary_is_regular || !temporary_parent_matches {
        remove_quietly(&temporary_path);
        return Err(AppError::ReportExportRejected);
    }

    if fs::rename(&temporary_path, target.path()).is_err() {
        remove_quietly(&temporary_path);
        return Err(AppError::ReportExportFailed);
    }

    // الطول والهاش من الملف المكتوب فعلاً على القرص.
    let filesystem_metadata =
        fs::metadata(target.path()).map_err(|_| AppError::ReportExportFailed)?;
    let sha256 = hash_file(target.path())?;

    Ok(WrittenExport {
        file_name,
        bytes: filesystem_metadata.len(),
        sha256,
    })
}

/// إزالة بادئة `\\?\` / `\\?\UNC\` التي يضيفها canonicalize على ويندوز.
fn strip_verbatim_prefix(path: &Path) -> std::path::PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return std::path::PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return std::path::PathBuf::from(rest.to_string());
    }
    path.to_path_buf()
}

fn remove_quietly(path: &Path) {
    // التنظيف لا يُفشل العملية الأصلية: الفشل هنا يعني بقاء مؤقت فقط.
    let _ = fs::remove_file(path);
}

fn hash_file(path: &Path) -> Result<String, AppError> {
    let mut file = fs::File::open(path).map_err(|_| AppError::ReportExportFailed)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| AppError::ReportExportFailed)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::{strip_verbatim_prefix, validate_target, write_atomically};
    use crate::domain::{error::AppError, reports::ReportFormat};
    use std::path::{Path, PathBuf};
    use uuid::Uuid;

    fn temp_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("nexus-writer-{label}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&root).expect("المجلد المؤقت يجب أن ينشأ");
        root
    }

    // بادئة المسار المُوسَّعة على ويندوز تُطبَّع قبل المقارنة.
    #[test]
    fn strips_the_windows_verbatim_prefix_for_comparison() {
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\D:\data\out")),
            PathBuf::from(r"D:\data\out")
        );
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\UNC\server\share\out")),
            PathBuf::from(r"\\server\share\out")
        );
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"D:\data\out")),
            PathBuf::from(r"D:\data\out")
        );
    }

    #[test]
    fn writes_regularly_renames_and_leaves_no_temporary_behind() {
        let root = temp_root("atomic");
        let target = validate_target(
            &root.join("out.csv").to_string_lossy(),
            ReportFormat::Csv,
            None,
        )
        .expect("المسار السليم يجب أن يُقبل");

        let written =
            write_atomically(&target, b"a,b\r\n1,2\r\n").expect("الكتابة الذرّية يجب أن تنجح");
        assert_eq!(written.bytes, 10);
        assert_eq!(written.file_name, "out.csv");
        assert_eq!(written.sha256.len(), 64);
        assert!(target.path().is_file());
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("قراءة المجلد يجب أن تنجح")
                .flatten()
                .count(),
            1
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    // عيب كشفه CI (2026-10-18): كانت المقارنة بين مسار مُطبَّع ومسار غير
    // مُطبَّع، فتُرفض وجهة تصدير سليمة على أنظمة تُعيد فيها `canonicalize`
    // الاسم الطويل لاسم 8.3 قصير (مثل `C:\Users\RUNNER~1\...`). الاختبار
    // يفرض اختلاف الشكل عمداً (شرطات مائلة أمامية + حالة أحرف مختلفة).
    #[test]
    fn accepts_a_destination_whose_folder_is_spelled_differently() {
        let root = temp_root("spelling");
        let alternate = root.to_string_lossy().replace('\\', "/").to_uppercase();
        let target = validate_target(&format!("{alternate}/out.csv"), ReportFormat::Csv, None)
            .expect("المسار البديل الصالح يجب أن يُقبل");

        let written = write_atomically(&target, b"a,b\r\n1,2\r\n")
            .expect("الكتابة يجب أن تنجح رغم اختلاف شكل المسار");
        assert_eq!(written.bytes, 10);
        assert!(target.path().is_file());

        let _ = std::fs::remove_dir_all(&root);
    }

    // الكتابة على مجلد بنفس اسم الملف تفشل بفشل مغلق ولا تُنشئ ملفاً جزئياً
    // ولا تُبقي ملفاً مؤقتاً في المجلد.
    #[test]
    fn a_failed_rename_leaves_the_target_untouched_and_no_temporary() {
        let root = temp_root("rename-failure");
        let blocked = root.join("blocked.csv");
        std::fs::create_dir_all(&blocked).expect("المجلد المعيق يجب أن ينشأ");
        // الهدف يُبنى مباشرةً (لا عبر التحقق) لقياس مسار الكتابة الذرّية وحده.
        let target = super::ExportTarget {
            parent: root.clone(),
            path: blocked.clone(),
        };

        assert!(matches!(
            write_atomically(&target, b"content"),
            Err(AppError::ReportExportFailed)
        ));
        assert!(blocked.is_dir());
        // لا ملف مؤقت متبقٍّ: العنصر الوحيد هو المجلد المعيق نفسه.
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("قراءة المجلد يجب أن تنجح")
                .flatten()
                .count(),
            1
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_oversized_content_before_touching_the_filesystem() {
        let root = temp_root("oversized");
        let target = validate_target(
            &root.join("big.csv").to_string_lossy(),
            ReportFormat::Csv,
            None,
        )
        .expect("المسار السليم يجب أن يُقبل");

        let oversized = vec![b'x'; crate::domain::reports::MAX_EXPORT_BYTES + 1];
        assert!(matches!(
            write_atomically(&target, &oversized),
            Err(AppError::ReportExportRejected)
        ));
        assert_eq!(
            std::fs::read_dir(&root)
                .expect("قراءة المجلد يجب أن تنجح")
                .flatten()
                .count(),
            0
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
