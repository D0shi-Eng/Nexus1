// =====================================================================
// مخزن أسرار الموفرين (Credential vault)
// ---------------------------------------------------------------------
// المسار الوحيد المسموح لتخزين مفاتيح API. الخصائص الملزمة:
//   - التشفير عند السكون بحماية نظام التشغيل: DPAPI بمستوى المستخدم الحالي
//     (CryptProtectData/CryptUnprotectData) — لا مفتاح سري خاص بنا ولا كلمة مرور
//     رئيسية يمكن تخمينها، والمفتاح الجذري يبقى في خزانة المستخدم في النظام.
//   - الربط بالموفر: يُمرَّر معرّف الموفر كـ «عشوائية إضافية» (entropy) إلى DPAPI،
//     فنقل الملف إلى موفر آخر يفشل في فك التشفير (لا انتحال بين الموفرين).
//   - لا يمر السر في أي معاملة SQLite ولا في أي حدث تدقيق ولا في أي سجل.
//     القاعدة تحمل علامة وجود فقط (credential_configured).
//   - لا تُعاد الأسرار عبر IPC إطلاقاً: الواجهة البرمجية تعيد bool فقط.
//   - لا يُسجَّل أي خطأ يحمل السر: كل الأخطاء رموز ثابتة (VaultError).
// =====================================================================

use std::fs;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

/// رمز خطأ ثابت للمخزن — لا يحمل أبداً أي جزء من السر أو مسار ملف حقيقي.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultError {
    /// تعذّر تهيئة مجلد المفاتيح.
    StorageUnavailable,
    /// تعذّرت الحماية أو رفع الحماية عبر نظام التشغيل.
    OsProtectionFailed,
    /// المدخل غير صالح (معرّف موفر فارغ أو سر فارغ).
    InvalidInput,
    /// السر المحفوظ تالف أو لا ينتمي إلى هذا الموفر.
    Corrupted,
}

impl VaultError {
    pub fn code(self) -> &'static str {
        match self {
            Self::StorageUnavailable => "CREDENTIAL_STORAGE_UNAVAILABLE",
            Self::OsProtectionFailed => "CREDENTIAL_PROTECTION_FAILED",
            Self::InvalidInput => "CREDENTIAL_INPUT_INVALID",
            Self::Corrupted => "CREDENTIAL_CORRUPTED",
        }
    }
}

/// عقد التخزين الآمن — يسمح بحقن مخزن بديل في الاختبارات بلا تغيير أي مسار إنتاجي.
///
/// `tenant_id` جزء **إلزامي** من مفتاح التخزين. العيبة المُغلقة (كانت HIGH):
/// كان المفتاح معرّف الموفر وحده، فأي مستأجرين يستعملان المعرّف نفسه (مثل
/// `OPENAI`) يتشاركان **ملف السر نفسه**: مستأجر يقرأ مفتاح مستأجر آخر ويرسله إلى
/// وجهته، وحذف موفر عند أحدهما يحذف سر الآخر. المسار العملي للإصابة قائم:
/// `restore_backup` يستبدل قاعدة البيانات بمستأجر آخر ولا يمسّ مجلد الأسرار.
pub trait CredentialVault: Send + Sync {
    fn store(&self, tenant_id: &str, provider_id: &str, credential: &str)
        -> Result<(), VaultError>;
    /// يُستدعى فقط لبناء ترويسة الطلب في الذاكرة لحظة الاستعمال — لا يُخزَّن ولا يُطبع.
    fn load(&self, tenant_id: &str, provider_id: &str) -> Result<Option<String>, VaultError>;
    fn delete(&self, tenant_id: &str, provider_id: &str) -> Result<(), VaultError>;
    fn exists(&self, tenant_id: &str, provider_id: &str) -> bool;
}

/// معرّف الموفر المسموح: أحرف كبيرة/أرقام/شرطة سفلية فقط — يمنع أي تسرّب مسار
/// (لا `..` ولا فاصل مسار) لأن المعرّف يُستعمل في اسم الملف.
pub fn is_valid_provider_secret_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

/// نطاق المستأجر في اسم الملف: **بصمة** لا الاسم الخام.
///
/// لماذا بصمة: معرّفات المستأجرين ليست مغلقة أبجدياً (أحرف صغيرة و`_`)، وإدخال
/// قواعد أبجدية جديدة في اسم ملف طريقٌ إلى تسرّب المسار. البصمة تجعل أي معرّف
/// مستأجر — بأي محارف — آمناً بالبناء، ولا تكشف اسم المستأجر في أسماء الملفات.
fn vault_scope(tenant_id: &str) -> Result<String, VaultError> {
    if tenant_id.is_empty() || tenant_id.len() > 128 {
        return Err(VaultError::InvalidInput);
    }
    let digest = Sha256::digest(tenant_id.as_bytes());
    let hex = format!("{digest:x}");
    Ok(hex[..16].to_owned())
}

/// مخزن الملفات: ملف واحد لكل (مستأجر، موفر) تحت مجلد مفاتيح مخصّص.
#[derive(Default)]
pub struct FileCredentialVault {
    directory: PathBuf,
}

impl FileCredentialVault {
    /// الإنتاج: المجلد يُشتق من مسار بيانات التطبيق — لا يأتي من العميل إطلاقاً.
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    fn ensure_directory(&self) -> Result<(), VaultError> {
        fs::create_dir_all(&self.directory).map_err(|_| VaultError::StorageUnavailable)
    }

    fn file_path(&self, tenant_id: &str, provider_id: &str) -> Result<PathBuf, VaultError> {
        if !is_valid_provider_secret_id(provider_id) {
            return Err(VaultError::InvalidInput);
        }
        let scope = vault_scope(tenant_id)?;

        Ok(self
            .directory
            .join(format!("provider-{provider_id}-{scope}.secret")))
    }

    /// عشوائية DPAPI: تربط النص المشفَّر بالمستأجر **وبالموفر** معاً، فنقل الملف
    /// إلى مستأجر آخر أو موفر آخر يفشل في فك التشفير.
    fn entropy(tenant_id: &str, provider_id: &str) -> Result<Vec<u8>, VaultError> {
        let scope = vault_scope(tenant_id)?;
        Ok(format!("{scope}:{provider_id}").into_bytes())
    }
}

impl CredentialVault for FileCredentialVault {
    fn store(
        &self,
        tenant_id: &str,
        provider_id: &str,
        credential: &str,
    ) -> Result<(), VaultError> {
        if credential.is_empty() || credential.len() > 8 * 1024 {
            return Err(VaultError::InvalidInput);
        }
        let path = self.file_path(tenant_id, provider_id)?;
        self.ensure_directory()?;
        let entropy = Self::entropy(tenant_id, provider_id)?;
        let protected = protect(&entropy, credential.as_bytes())?;
        // كتابة ذرّية: ملف مؤقت ثم استبدال — لا يبقى نصف ملف عند انقطاع.
        let temporary = path.with_extension("secret.tmp");
        fs::write(&temporary, &protected).map_err(|_| VaultError::StorageUnavailable)?;
        fs::rename(&temporary, &path).map_err(|_| VaultError::StorageUnavailable)?;
        Ok(())
    }

    fn load(&self, tenant_id: &str, provider_id: &str) -> Result<Option<String>, VaultError> {
        let path = self.file_path(tenant_id, provider_id)?;
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(VaultError::StorageUnavailable),
        };
        let entropy = Self::entropy(tenant_id, provider_id)?;
        let plaintext = unprotect(&entropy, &bytes)?;
        let text = String::from_utf8(plaintext).map_err(|_| VaultError::Corrupted)?;
        if text.is_empty() {
            return Err(VaultError::Corrupted);
        }

        Ok(Some(text))
    }

    fn delete(&self, tenant_id: &str, provider_id: &str) -> Result<(), VaultError> {
        let path = self.file_path(tenant_id, provider_id)?;
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(VaultError::StorageUnavailable),
        }
    }

    fn exists(&self, tenant_id: &str, provider_id: &str) -> bool {
        self.file_path(tenant_id, provider_id)
            .map(|path| path.is_file())
            .unwrap_or(false)
    }
}

// ===== حماية نظام التشغيل =====

#[cfg(windows)]
fn protect(entropy: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, VaultError> {
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let mut input = CRYPT_INTEGER_BLOB {
        cbData: plaintext.len() as u32,
        pbData: plaintext.as_ptr() as *mut u8,
    };
    let mut extra = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // UI_FORBIDDEN: لا نافذة نظام تظهر أبداً — الفشل يبقى صامتاً ومغلقاً.
    let succeeded = unsafe {
        CryptProtectData(
            &mut input,
            std::ptr::null(),
            &mut extra,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if succeeded == 0 || output.pbData.is_null() {
        return Err(VaultError::OsProtectionFailed);
    }

    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) };
    let protected = bytes.to_vec();
    unsafe { free_local_blob(output.pbData) };

    Ok(protected)
}

#[cfg(windows)]
fn unprotect(entropy: &[u8], protected: &[u8]) -> Result<Vec<u8>, VaultError> {
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let mut input = CRYPT_INTEGER_BLOB {
        cbData: protected.len() as u32,
        pbData: protected.as_ptr() as *mut u8,
    };
    let mut extra = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let succeeded = unsafe {
        CryptUnprotectData(
            &mut input,
            std::ptr::null_mut(),
            &mut extra,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    // فشل فك التشفير يعني: ملف منقول من موفر آخر، أو مستخدم آخر، أو تلاعب.
    if succeeded == 0 || output.pbData.is_null() {
        return Err(VaultError::Corrupted);
    }

    let bytes = unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) };
    let plaintext = bytes.to_vec();
    unsafe { free_local_blob(output.pbData) };

    Ok(plaintext)
}

#[cfg(windows)]
unsafe fn free_local_blob(pointer: *mut u8) {
    use windows_sys::Win32::Foundation::LocalFree;

    // تحرير ذاكرة DPAPI المخصّصة بـ LocalAlloc — التسريب هنا يعني بقاء السر في الذاكرة.
    unsafe {
        LocalFree(pointer as *mut core::ffi::c_void);
    }
}

/// منصات غير ويندوز: فشل مغلق صريح بدل صمت يوهم بتخزين آمن.
#[cfg(not(windows))]
fn protect(_entropy: &[u8], _plaintext: &[u8]) -> Result<Vec<u8>, VaultError> {
    Err(VaultError::OsProtectionFailed)
}

#[cfg(not(windows))]
fn unprotect(_entropy: &[u8], _protected: &[u8]) -> Result<Vec<u8>, VaultError> {
    Err(VaultError::OsProtectionFailed)
}

// ===== مخزن اختبارات في الذاكرة =====
// لا يحاكي DPAPI ولا يُستعمل في الإنتاج: يسمح لاختبارات التكامل بالتحكم بالوجود
// والغياب بلا كتابة أسرار حقيقية على القرص.
#[cfg(test)]
#[derive(Default)]
pub struct InMemoryCredentialVault {
    entries: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

#[cfg(test)]
impl InMemoryCredentialVault {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
impl CredentialVault for InMemoryCredentialVault {
    fn store(
        &self,
        tenant_id: &str,
        provider_id: &str,
        credential: &str,
    ) -> Result<(), VaultError> {
        if !is_valid_provider_secret_id(provider_id) || credential.is_empty() {
            return Err(VaultError::InvalidInput);
        }
        let scope = vault_scope(tenant_id)?;
        self.entries
            .lock()
            .map_err(|_| VaultError::StorageUnavailable)?
            .insert(format!("{scope}:{provider_id}"), credential.to_owned());
        Ok(())
    }

    fn load(&self, tenant_id: &str, provider_id: &str) -> Result<Option<String>, VaultError> {
        let scope = vault_scope(tenant_id)?;
        Ok(self
            .entries
            .lock()
            .map_err(|_| VaultError::StorageUnavailable)?
            .get(&format!("{scope}:{provider_id}"))
            .cloned())
    }

    fn delete(&self, tenant_id: &str, provider_id: &str) -> Result<(), VaultError> {
        let scope = vault_scope(tenant_id)?;
        self.entries
            .lock()
            .map_err(|_| VaultError::StorageUnavailable)?
            .remove(&format!("{scope}:{provider_id}"));
        Ok(())
    }

    fn exists(&self, tenant_id: &str, provider_id: &str) -> bool {
        let Ok(scope) = vault_scope(tenant_id) else {
            return false;
        };
        self.entries
            .lock()
            .map(|entries| entries.contains_key(&format!("{scope}:{provider_id}")))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        is_valid_provider_secret_id, CredentialVault, FileCredentialVault, InMemoryCredentialVault,
        VaultError,
    };

    // سر اختباري طويل وعشوائي الشكل — يُستخدم في كل تأكيدات عدم التسرب.
    const TEST_SECRET: &str = "sk-test-8b1f4c9d2e7a3f6051bd-CANARY-DO-NOT-LEAK";
    /// نطاق مستأجر الاختبار — المفتاح صار (مستأجر، موفر).
    const TENANT: &str = "tenant_demo";
    /// مستأجر ثانٍ لإثبات عدم المشاركة.
    const OTHER_TENANT: &str = "tenant_foreign";

    /// مسار ملف السر المتوقع — يُشتق من نفس دالة النطاق الإنتاجية لا من نص مكرر.
    fn secret_path(
        directory: &std::path::Path,
        tenant: &str,
        provider_id: &str,
    ) -> std::path::PathBuf {
        directory.join(format!(
            "provider-{provider_id}-{}.secret",
            super::vault_scope(tenant).expect("نطاق المستأجر يجب أن يُشتق")
        ))
    }

    fn temporary_directory(label: &str) -> std::path::PathBuf {
        let directory =
            std::env::temp_dir().join(format!("nexus-vault-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("مجلد الاختبار يجب أن ينشأ");
        directory
    }

    fn vault(label: &str) -> (FileCredentialVault, std::path::PathBuf) {
        let directory = temporary_directory(label);
        (FileCredentialVault::new(directory.clone()), directory)
    }

    #[test]
    fn round_trips_a_credential_through_the_operating_system_protection() {
        let (vault, directory) = vault("roundtrip");
        assert!(!vault.exists(TENANT, "PROVIDER_ONE"));

        vault
            .store(TENANT, "PROVIDER_ONE", TEST_SECRET)
            .expect("الحفظ يجب أن ينجح");
        assert!(vault.exists(TENANT, "PROVIDER_ONE"));
        assert_eq!(
            vault
                .load(TENANT, "PROVIDER_ONE")
                .expect("القراءة يجب أن تنجح"),
            Some(TEST_SECRET.to_owned())
        );

        // الملف موجود لكن محتواه ليس السر: هذا جوهر «التشفير عند السكون».
        let path = secret_path(&directory, TENANT, "PROVIDER_ONE");
        assert!(path.is_file(), "ملف السر يجب أن يوجد");
        let raw = std::fs::read(&path).expect("قراءة الملف الخام يجب أن تنجح");
        assert!(
            !contains_subsequence(&raw, TEST_SECRET.as_bytes()),
            "السر يجب ألا يظهر نصاً في الملف المحفوظ"
        );
        assert!(!raw.is_empty());

        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn overwrites_an_existing_credential_and_keeps_only_the_new_value() {
        let (vault, directory) = vault("overwrite");
        vault
            .store(TENANT, "PROVIDER_ONE", "first-secret")
            .expect("الحفظ الأول");
        vault
            .store(TENANT, "PROVIDER_ONE", TEST_SECRET)
            .expect("الاستبدال");

        assert_eq!(
            vault.load(TENANT, "PROVIDER_ONE").expect("القراءة"),
            Some(TEST_SECRET.to_owned())
        );
        let raw = std::fs::read(secret_path(&directory, TENANT, "PROVIDER_ONE")).expect("قراءة");
        assert!(
            !contains_subsequence(&raw, b"first-secret"),
            "القيمة القديمة يجب ألا تبقى في الملف"
        );

        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn delete_removes_the_credential_and_is_idempotent() {
        let (vault, directory) = vault("delete");
        vault
            .store(TENANT, "PROVIDER_ONE", TEST_SECRET)
            .expect("الحفظ");
        vault
            .delete(TENANT, "PROVIDER_ONE")
            .expect("الحذف يجب أن ينجح");
        assert!(!vault.exists(TENANT, "PROVIDER_ONE"));
        assert_eq!(vault.load(TENANT, "PROVIDER_ONE").expect("القراءة"), None);
        // الحذف المتكرر ليس خطأً.
        vault.delete(TENANT, "PROVIDER_ONE").expect("الحذف المتكرر");

        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn reports_absence_instead_of_failing_when_no_credential_exists() {
        let (vault, directory) = vault("absent");
        assert_eq!(vault.load(TENANT, "PROVIDER_ONE").expect("القراءة"), None);
        assert!(!vault.exists(TENANT, "PROVIDER_ONE"));
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn rejects_empty_credentials_and_unsafe_provider_identifiers() {
        let (vault, directory) = vault("invalid");
        assert_eq!(
            vault.store(TENANT, "PROVIDER_ONE", ""),
            Err(VaultError::InvalidInput)
        );
        for unsafe_id in [
            "",
            "../../etc/passwd",
            "provider/one",
            "provider one",
            "provider-one",
            "provider.one",
        ] {
            assert!(
                !is_valid_provider_secret_id(unsafe_id),
                "المعرّف {unsafe_id} يجب أن يُرفض"
            );
            assert_eq!(
                vault.store(TENANT, unsafe_id, TEST_SECRET),
                Err(VaultError::InvalidInput)
            );
            assert_eq!(vault.load(TENANT, unsafe_id), Err(VaultError::InvalidInput));
            assert!(!vault.exists(TENANT, unsafe_id));
        }
        assert!(is_valid_provider_secret_id("OPENAI_PRIMARY_1"));
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_blob_moved_to_another_provider_fails_to_decrypt() {
        // الربط بالموفر والمستأجر عبر عشوائية DPAPI: النسخ لا يمنح وصولاً.
        let (vault, directory) = vault("binding");
        vault
            .store(TENANT, "PROVIDER_ONE", TEST_SECRET)
            .expect("الحفظ");
        std::fs::copy(
            secret_path(&directory, TENANT, "PROVIDER_ONE"),
            secret_path(&directory, TENANT, "PROVIDER_TWO"),
        )
        .expect("النسخ يجب أن ينجح");

        assert_eq!(
            vault.load(TENANT, "PROVIDER_TWO"),
            Err(VaultError::Corrupted),
            "ملف منقول من موفر آخر يجب أن يفشل في فك التشفير"
        );

        std::fs::remove_dir_all(&directory).ok();
    }

    // انحدار العيبة (كانت HIGH): كان مفتاح المخزن معرّف الموفر وحده، فمستأجران
    // يستعملان المعرّف نفسه يتشاركان الملف نفسه. هذا الاختبار يثبت الفصل التام:
    // لا مشاركة ملف، ولا قراءة عابرة للمستأجرين، ولا حذف عابر.
    #[test]
    fn different_tenants_never_share_or_cross_read_a_secret_file() {
        let (vault, directory) = vault("tenant-scope");
        vault
            .store(TENANT, "OPENAI", "secret-for-first-tenant")
            .expect("حفظ سر المستأجر الأول");
        vault
            .store(OTHER_TENANT, "OPENAI", "secret-for-second-tenant")
            .expect("حفظ سر المستأجر الثاني");

        // ملفان مختلفان على القرص للمعرّف نفسه.
        let first = secret_path(&directory, TENANT, "OPENAI");
        let second = secret_path(&directory, OTHER_TENANT, "OPENAI");
        assert!(first.is_file() && second.is_file());
        assert_ne!(first, second, "المستأجران يجب ألا يتشاركا مساراً");

        // كل مستأجر يقرأ سرّه هو فقط.
        assert_eq!(
            vault.load(TENANT, "OPENAI").expect("قراءة الأول"),
            Some("secret-for-first-tenant".to_owned())
        );
        assert_eq!(
            vault.load(OTHER_TENANT, "OPENAI").expect("قراءة الثاني"),
            Some("secret-for-second-tenant".to_owned())
        );

        // حذف سر مستأجر لا يمسّ سر الآخر.
        vault.delete(TENANT, "OPENAI").expect("الحذف");
        assert!(!vault.exists(TENANT, "OPENAI"));
        assert!(vault.exists(OTHER_TENANT, "OPENAI"));
        assert_eq!(
            vault.load(OTHER_TENANT, "OPENAI").expect("قراءة الثاني"),
            Some("secret-for-second-tenant".to_owned())
        );

        std::fs::remove_dir_all(&directory).ok();
    }

    // ملف مشفَّر لمستأجر لا يُفكّ عند نسبته إلى مستأجر آخر (ربط العشوائية).
    #[test]
    fn a_blob_attributed_to_another_tenant_fails_to_decrypt() {
        let (vault, directory) = vault("tenant-binding");
        vault.store(TENANT, "OPENAI", TEST_SECRET).expect("الحفظ");
        std::fs::copy(
            secret_path(&directory, TENANT, "OPENAI"),
            secret_path(&directory, OTHER_TENANT, "OPENAI"),
        )
        .expect("النسخ يجب أن ينجح");

        assert_eq!(
            vault.load(OTHER_TENANT, "OPENAI"),
            Err(VaultError::Corrupted),
            "ملف منقول من مستأجر آخر يجب أن يفشل في فك التشفير"
        );

        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_tampered_blob_is_reported_as_corrupted_never_as_plaintext() {
        let (vault, directory) = vault("tampered");
        vault
            .store(TENANT, "PROVIDER_ONE", TEST_SECRET)
            .expect("الحفظ");
        let path = secret_path(&directory, TENANT, "PROVIDER_ONE");
        let mut raw = std::fs::read(&path).expect("قراءة");
        for byte in raw.iter_mut() {
            *byte ^= 0x5a;
        }
        std::fs::write(&path, &raw).expect("كتابة التلاعب");

        assert_eq!(
            vault.load(TENANT, "PROVIDER_ONE"),
            Err(VaultError::Corrupted)
        );
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn vault_error_codes_are_stable_and_carry_no_secret_material() {
        for (error, expected) in [
            (
                VaultError::StorageUnavailable,
                "CREDENTIAL_STORAGE_UNAVAILABLE",
            ),
            (
                VaultError::OsProtectionFailed,
                "CREDENTIAL_PROTECTION_FAILED",
            ),
            (VaultError::InvalidInput, "CREDENTIAL_INPUT_INVALID"),
            (VaultError::Corrupted, "CREDENTIAL_CORRUPTED"),
        ] {
            assert_eq!(error.code(), expected);
            assert!(!expected.contains(TEST_SECRET));
        }
        // التصحيح لا يُسرّب السر ولا اسم الملف.
        let formatted = format!("{:?}", VaultError::OsProtectionFailed);
        assert!(!formatted.contains(TEST_SECRET));
        assert!(!formatted.contains(".secret"));
    }

    #[test]
    fn in_memory_vault_matches_the_contract_for_tests() {
        let vault = InMemoryCredentialVault::new();
        assert!(!vault.exists(TENANT, "PROVIDER_ONE"));
        vault
            .store(TENANT, "PROVIDER_ONE", TEST_SECRET)
            .expect("الحفظ");
        assert!(vault.exists(TENANT, "PROVIDER_ONE"));
        assert_eq!(
            vault.load(TENANT, "PROVIDER_ONE").expect("القراءة"),
            Some(TEST_SECRET.to_owned())
        );
        vault.delete(TENANT, "PROVIDER_ONE").expect("الحذف");
        assert!(!vault.exists(TENANT, "PROVIDER_ONE"));
    }

    /// بحث بايت خام — يُستخدم للتأكد من غياب السر نصاً في أي ملف.
    fn contains_subsequence(haystack: &[u8], needle: &[u8]) -> bool {
        if needle.is_empty() || haystack.len() < needle.len() {
            return false;
        }
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }
}
