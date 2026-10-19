use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use chrono::Utc;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    application::authorization_service::require_permission,
    domain::{error::AppError, models::RuntimeMode},
    infrastructure::database::{
        append_audit_event, apply_migrations, ensure_permission_catalog, latest_audit_event_hash,
        read_runtime_mode, resolve_active_session, AppState, DEMO_OPERATOR_USER_ID, DEMO_TENANT_ID,
    },
};

// صيغة النسخة الاحتياطية: ملف قاعدة SQLite واحد (VACUUM INTO = لقطة متسقة معاملاتياً)
// + بيان جانبي بالهاش — لا مسارات داخل البيان إطلاقاً.
pub const BACKUP_FORMAT: &str = "NEXUS_BACKUP_V1";
pub const BACKUP_EXTENSION: &str = "nxbackup";
const MANIFEST_SUFFIX: &str = "manifest.json";
// أعلى إصدار مخطط تقبله هذه النسخة من التطبيق — نسخة أحدث تُرفض.
// يرتفع مع كل هجرة جديدة: 0026 (فصل الأدلة + كتالوج أدوار الإنتاج).
pub const SUPPORTED_SCHEMA_VERSION: i32 = 26;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBackupInput {
    pub target_directory: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub file_name: String,
    pub sha256: String,
    pub bytes: u64,
    pub schema_version: i32,
    pub mode: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupManifest {
    pub format: String,
    pub app_version: String,
    pub schema_version: i32,
    pub created_at: String,
    pub mode: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBackupInput {
    pub backup_file: String,
    pub confirm: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreResult {
    pub restored_mode: String,
    pub restored_schema_version: i32,
    pub safety_copy_file: String,
    pub authenticated: bool,
}

// إنشاء نسخة احتياطية متسقة: VACUUM INTO على الاتصال الحي + بيان جانبي بهاش SHA-256.
pub fn create_backup(state: &AppState, input: CreateBackupInput) -> Result<BackupResult, AppError> {
    require_permission(state, "BACKUP", "CREATE")?;

    let target_directory = validate_directory(&input.target_directory)?;
    let created_at = Utc::now();
    let stamp = created_at.format("%Y%m%d-%H%M%S");
    let mut backup_path = target_directory.join(format!("nexus-backup-{stamp}.{BACKUP_EXTENSION}"));
    // تفادي التصادم داخل الثانية الواحدة — لوحة لاحقة للطابع.
    let mut counter = 1;
    while backup_path.exists() {
        backup_path =
            target_directory.join(format!("nexus-backup-{stamp}-{counter}.{BACKUP_EXTENSION}"));
        counter += 1;
    }

    let (mode, schema_version) = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        connection
            .execute("VACUUM INTO ?1", [&backup_path.to_string_lossy()])
            .map_err(|_| AppError::BackupStorageError)?;
        (
            read_runtime_mode(&connection)?,
            current_schema_version(&connection)?,
        )
    };

    let bytes = fs::metadata(&backup_path)
        .map_err(|_| AppError::BackupStorageError)?
        .len();
    let sha256 = hash_file(&backup_path)?;
    let manifest = BackupManifest {
        format: BACKUP_FORMAT.to_owned(),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        schema_version,
        created_at: created_at.to_rfc3339(),
        mode: mode.as_str().to_owned(),
        sha256: sha256.clone(),
        bytes,
    };
    let manifest_path = manifest_path_for(&backup_path);
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).map_err(|_| AppError::BackupStorageError)?,
    )
    .map_err(|_| AppError::BackupStorageError)?;

    audit_backup_event(state, "BACKUP_CREATED", &backup_path)?;
    crate::infrastructure::local_logger::info(
        "BACKUP_CREATED",
        &state.tenant_id(),
        &format!("{{\"bytes\":{bytes},\"schemaVersion\":{schema_version}}}"),
    );

    Ok(BackupResult {
        file_name: backup_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .ok_or(AppError::BackupStorageError)?,
        sha256,
        bytes,
        schema_version,
        mode: mode.as_str().to_owned(),
        created_at: manifest.created_at,
    })
}

// استعادة مُتحقق منها: بيان + هاش + سلامة + توافق مخطط، ثم نسخة أمان قبل الاستبدال.
pub fn restore_backup(
    state: &AppState,
    input: RestoreBackupInput,
) -> Result<RestoreResult, AppError> {
    require_permission(state, "BACKUP", "RESTORE")?;
    if !input.confirm {
        return Err(AppError::ValidationFailed);
    }

    let backup_path = validate_backup_file(&input.backup_file)?;
    let manifest = load_and_verify_manifest(&backup_path)?;
    verify_backup_database(&backup_path, &manifest)?;

    let live_path = state.database_path().ok_or(AppError::BackupInvalid)?;

    // نسخة أمان للقاعدة الحالية قبل أي استبدال — نفس صيغة النسخ الاحتياطي الموثوقة.
    let safety_dir = live_path
        .parent()
        .ok_or(AppError::BackupStorageError)?
        .to_path_buf();
    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let mut safety_path = safety_dir.join(format!("nexus-pre-restore-{stamp}.{BACKUP_EXTENSION}"));
    let mut counter = 1;
    while safety_path.exists() {
        safety_path = safety_dir.join(format!(
            "nexus-pre-restore-{stamp}-{counter}.{BACKUP_EXTENSION}"
        ));
        counter += 1;
    }
    {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        connection
            .execute("VACUUM INTO ?1", [&safety_path.to_string_lossy()])
            .map_err(|_| AppError::BackupStorageError)?;
    }
    let safety_bytes = fs::metadata(&safety_path)
        .map_err(|_| AppError::BackupStorageError)?
        .len();
    let safety_manifest = BackupManifest {
        format: BACKUP_FORMAT.to_owned(),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        schema_version: current_schema_version_from(&state)?,
        created_at: Utc::now().to_rfc3339(),
        mode: state.runtime_mode()?.as_str().to_owned(),
        sha256: hash_file(&safety_path)?,
        bytes: safety_bytes,
    };
    fs::write(
        manifest_path_for(&safety_path),
        serde_json::to_vec_pretty(&safety_manifest).map_err(|_| AppError::BackupStorageError)?,
    )
    .map_err(|_| AppError::BackupStorageError)?;

    // الاستبدال: إغلاق الاتصال الحي ثم نسخ الملف ثم إعادة الفتح والتحقق.
    {
        let mut guard = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        let placeholder = Connection::open_in_memory().map_err(|_| AppError::BackupStorageError)?;
        let old = std::mem::replace(&mut *guard, placeholder);
        drop(old);
        fs::copy(&backup_path, &live_path).map_err(|_| AppError::BackupStorageError)?;
        let reopened = Connection::open(&live_path).map_err(|_| AppError::BackupStorageError)?;
        reopened
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|_| AppError::BackupStorageError)?;
        *guard = reopened;
    }

    // ترقية القاعدة المستعادة إلى مخطط هذا التطبيق إن كانت أقدم (نفس مسار الإقلاع).
    let (restored_mode, restored_schema_version, authenticated) = {
        let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
        apply_migrations(&connection)?;
        ensure_permission_catalog(&connection)?;
        let mode = read_runtime_mode(&connection)?;
        audit_restore_in_restored_database(&connection, &backup_path)?;
        if mode == RuntimeMode::Demo {
            let now = Utc::now().to_rfc3339();
            let session_id =
                resolve_active_session(&connection, DEMO_TENANT_ID, DEMO_OPERATOR_USER_ID, &now)
                    .ok_or(AppError::AuthorizationDenied)?;
            drop(connection);
            state.set_runtime_mode(mode)?;
            state.activate_identity(DEMO_TENANT_ID, DEMO_OPERATOR_USER_ID, Some(session_id))?;
            (mode, current_schema_version_from(state)?, true)
        } else {
            drop(connection);
            state.set_runtime_mode(mode)?;
            state.clear_active_identity()?;
            (mode, current_schema_version_from(state)?, false)
        }
    };

    crate::infrastructure::local_logger::info(
        "BACKUP_RESTORED",
        "",
        &format!("{{\"restoredMode\":\"{}\"}}", restored_mode.as_str()),
    );
    // مرساة سلسلة التدقيق تُبطل بعد الاستعادة: القاعدة المستعادة قد تكون أقدم
    // شرعاً من آخر تحقق، فمقارنتها بالمرساة القديمة تُنتج «اقتطاعاً» كاذباً.
    // تُثبَّت من جديد تلقائياً عند أول تحقق ناجح على القاعدة المستعادة.
    crate::application::audit_anchor::reset(state);
    Ok(RestoreResult {
        restored_mode: restored_mode.as_str().to_owned(),
        restored_schema_version: restored_schema_version,
        safety_copy_file: safety_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .ok_or(AppError::BackupStorageError)?,
        authenticated,
    })
}

// قائمة النسخ الاحتياطية داخل مجلد — تُقرأ البيانات الجانبية فقط بلا فتح قواعد.
pub fn list_backups(state: &AppState, directory: String) -> Result<Vec<BackupManifest>, AppError> {
    require_permission(state, "BACKUP", "CREATE")?;
    let directory = validate_directory(&directory)?;
    let mut backups = Vec::new();
    let entries = fs::read_dir(&directory).map_err(|_| AppError::BackupStorageError)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some(BACKUP_EXTENSION) {
            continue;
        }
        if let Some(manifest) = load_manifest(&path).ok() {
            backups.push(manifest);
        }
    }
    backups.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(backups)
}

// ===== التحقق والتحقق المساعد =====

fn validate_directory(raw: &str) -> Result<PathBuf, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 600 {
        return Err(AppError::ValidationFailed);
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(AppError::ValidationFailed);
    }
    let metadata = fs::metadata(&path).map_err(|_| AppError::BackupStorageError)?;
    if !metadata.is_dir() {
        return Err(AppError::BackupStorageError);
    }
    Ok(path)
}

fn validate_backup_file(raw: &str) -> Result<PathBuf, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 600 {
        return Err(AppError::BackupInvalid);
    }
    let path = PathBuf::from(trimmed);
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        || path.extension().and_then(|e| e.to_str()) != Some(BACKUP_EXTENSION)
    {
        return Err(AppError::BackupInvalid);
    }
    if !path.is_file() {
        return Err(AppError::BackupInvalid);
    }
    Ok(path)
}

fn manifest_path_for(backup_path: &Path) -> PathBuf {
    let mut name = backup_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    name.push('.');
    name.push_str(MANIFEST_SUFFIX);
    backup_path.with_file_name(name)
}

fn load_manifest(backup_path: &Path) -> Result<BackupManifest, AppError> {
    let manifest_path = manifest_path_for(backup_path);
    let raw = fs::read_to_string(&manifest_path).map_err(|_| AppError::BackupInvalid)?;
    let manifest: BackupManifest =
        serde_json::from_str(&raw).map_err(|_| AppError::BackupInvalid)?;
    if manifest.format != BACKUP_FORMAT {
        return Err(AppError::BackupInvalid);
    }
    Ok(manifest)
}

fn load_and_verify_manifest(backup_path: &Path) -> Result<BackupManifest, AppError> {
    let manifest = load_manifest(backup_path)?;
    let metadata = fs::metadata(backup_path).map_err(|_| AppError::BackupInvalid)?;
    if metadata.len() != manifest.bytes {
        return Err(AppError::BackupCorrupted);
    }
    let actual_hash = hash_file(backup_path)?;
    if !constant_time_eq(&actual_hash, &manifest.sha256) {
        return Err(AppError::BackupCorrupted);
    }
    Ok(manifest)
}

// فتح النسخة للقراءة فقط: سلامة بنيوية + توافق إصدار + سلمة مفاتيح الأجنبة.
fn verify_backup_database(backup_path: &Path, manifest: &BackupManifest) -> Result<(), AppError> {
    if manifest.schema_version > SUPPORTED_SCHEMA_VERSION {
        return Err(AppError::BackupIncompatible);
    }
    let connection = Connection::open_with_flags(backup_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| AppError::BackupCorrupted)?;
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .map_err(|_| AppError::BackupCorrupted)?;
    if integrity != "ok" {
        return Err(AppError::BackupCorrupted);
    }
    let version: i32 = connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .map_err(|_| AppError::BackupInvalid)?;
    if version != manifest.schema_version {
        return Err(AppError::BackupCorrupted);
    }
    if version > SUPPORTED_SCHEMA_VERSION {
        return Err(AppError::BackupIncompatible);
    }
    let mode: String = connection
        .query_row(
            "SELECT mode FROM app_runtime_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| AppError::BackupInvalid)?;
    if !matches!(mode.as_str(), "UNPROVISIONED" | "DEMO" | "PRODUCTION") {
        return Err(AppError::BackupInvalid);
    }
    let violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(|_| AppError::BackupCorrupted)?;
    if violations != 0 {
        return Err(AppError::BackupCorrupted);
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<String, AppError> {
    let mut file = fs::File::open(path).map_err(|_| AppError::BackupStorageError)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| AppError::BackupStorageError)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

// مقارنة سلاسل هاش بطول ثابت — توقيت مستقل عن موضع أول اختلاف.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    let mut difference = 0_u8;
    for (x, y) in a.iter().zip(b.iter()) {
        difference |= x ^ y;
    }
    difference == 0
}

fn current_schema_version(connection: &Connection) -> Result<i32, AppError> {
    connection
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .map_err(|_| AppError::DataAccess)
}

fn current_schema_version_from(state: &AppState) -> Result<i32, AppError> {
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    current_schema_version(&connection)
}

fn audit_backup_event(state: &AppState, event_type: &str, path: &Path) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let tenant_id = state.tenant_id();
    let user_id = state.current_user_id()?;
    // يُدوّن اسم الملف فقط — لا مسارات كاملة في سجل التدقيق.
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let connection = state
        .connection
        .lock()
        .map_err(|_| AppError::StorageOperation)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    let previous_hash = latest_audit_event_hash(&transaction, &state.tenant_id())?;
    append_audit_event(
        &transaction,
        &tenant_id,
        &format!("audit_{}", uuid::Uuid::new_v4()),
        event_type,
        &user_id,
        "Backup",
        &file_name,
        &now,
        &previous_hash,
    )?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

// أثر الاستعادة يكتب داخل القاعدة المستعادة نفسها — بفاعل نظامي بلا مسارات.
fn audit_restore_in_restored_database(
    connection: &Connection,
    backup_path: &Path,
) -> Result<(), AppError> {
    let tenant_id: String = connection
        .query_row(
            "SELECT id FROM tenants ORDER BY created_at ASC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)?;
    let now = Utc::now().to_rfc3339();
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    // القاعدة المستعادة قد تحمل مستأجراً مختلفاً عن الحالة الحية — نقرأ السلسلة بمستأجر القاعدة نفسها.
    let previous_hash = latest_audit_event_hash(&transaction, &tenant_id)?;
    let file_name = backup_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    append_audit_event(
        &transaction,
        &tenant_id,
        &format!("audit_{}", uuid::Uuid::new_v4()),
        "BACKUP_RESTORED",
        "NEXUS System",
        "Backup",
        &file_name,
        &now,
        &previous_hash,
    )?;
    transaction.commit().map_err(|_| AppError::StorageOperation)
}

#[cfg(test)]
pub fn manifest_path_for_test(backup_path: &Path) -> PathBuf {
    manifest_path_for(backup_path)
}

#[cfg(test)]
#[path = "backup_service_tests.rs"]
mod tests;
