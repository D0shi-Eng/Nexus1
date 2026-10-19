use std::path::PathBuf;

use rusqlite::Connection;

use super::{
    create_backup, list_backups, restore_backup, BackupManifest, CreateBackupInput,
    RestoreBackupInput, BACKUP_FORMAT,
};
use crate::{
    application::authentication_service::{
        bootstrap_production, login, BootstrapProductionInput, LoginInput,
    },
    domain::error::AppError,
    infrastructure::database::{initialize_demo_database, AppState},
};

// مجلد مؤقت نظيف لكل اختبار — لا يمس قاعدة المستخدم الحية أبداً.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "nexus-mp05-backup-{tag}-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&dir).expect("مجلد الاختبار يجب أن ينشأ");
    dir
}

// قاعدة إنتاج حقيقية على ملف مؤقت — كما يراها التطبيق المثبت فعلاً.
fn production_state_on_file(tag: &str) -> (AppState, PathBuf) {
    let dir = temp_dir(tag);
    let db_path = dir.join("nexus-local.db");
    let connection = Connection::open(&db_path).expect("قاعدة الملف يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("مفاتيح الأجنبة يجب أن تعمل");
    crate::infrastructure::database::apply_migrations(&connection).expect("الهجرات يجب أن تنجح");
    crate::infrastructure::database::ensure_permission_catalog(&connection)
        .expect("الكتالوج يجب أن ينجح");
    let mut state = AppState::test_instance(connection, "", "");
    state
        .set_runtime_mode(crate::domain::models::RuntimeMode::Unprovisioned)
        .expect("وضع الاختبار يجب أن يضبط");
    state.set_database_path_for_tests(db_path.clone());
    bootstrap_production(
        &state,
        BootstrapProductionInput {
            organization_name: "Backup Test Org".to_owned(),
            // GAP-05-OPEN: بيانات المؤسسة جزء من العقد — لا تُفترض في الخلف.
            country_code: "SA".to_owned(),
            industry: "Technology Services".to_owned(),
            admin_display_name: "Backup Admin".to_owned(),
            admin_email: "admin@backup.test".to_owned(),
            password: "Str0ng!Passw0rd#2026".to_owned(),
        },
    )
    .expect("إقلاع الإنتاج يجب أن ينجح");
    (state, db_path)
}

fn demo_state_on_file(tag: &str) -> (AppState, PathBuf) {
    let dir = temp_dir(tag);
    let db_path = dir.join("nexus-local.db");
    let connection = Connection::open(&db_path).expect("قاعدة الملف يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("مفاتيح الأجنبة يجب أن تعمل");
    initialize_demo_database(&connection).expect("تهيئة الديمو يجب أن تنجح");
    let mut state = AppState::test_instance(
        connection,
        crate::infrastructure::database::DEMO_TENANT_ID,
        crate::infrastructure::database::DEMO_REVIEWER_USER_ID,
    );
    state.set_database_path_for_tests(db_path.clone());
    (state, db_path)
}

// إعادة فتح القاعدة على القرص = مكافئ إعادة تشغيل التطبيق: الهجرات idempotent،
// وبيانات المستخدم (بيانات فعلية + مستخدم إنتاجي) تبقى سليمة، والوضع يبقى PRODUCTION.
#[test]
fn on_disk_production_database_reopens_with_data_intact_and_idempotent_migrations() {
    let (state, db_path) = production_state_on_file("reopen");
    let backup_dir = temp_dir("reopen-backups");

    // بيانات مستخدم فعلية قبل الإغلاق.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let now = chrono::Utc::now().to_rfc3339();
        let tenant_id = state.tenant_id();
        connection
            .execute(
                "INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES ('source_reopen_001', ?1, 'Reopen Authority', 'Reopen reference', NULL, '1.0', ?2, 0, 'REQUIRES_REVIEW', ?2, ?2)",
                rusqlite::params![tenant_id, now],
            )
            .expect("المصدر يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, source_type, created_at, updated_at) VALUES ('obl_reopen_001', ?1, 'source_reopen_001', 'Obligation surviving restart', 'Owner', '2026-12-01T00:00:00Z', 'HIGH', 'OPEN', 'APPLIES', 'MISSING', 'UNVERIFIED', 'OFFICIAL', ?2, ?2)",
                rusqlite::params![tenant_id, now],
            )
            .expect("الالتزام يجب أن ينشأ");
    }
    let tenant_id = state.tenant_id();
    let users_before: i64 = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT COUNT(*) FROM local_users WHERE tenant_id = ?1",
                [&tenant_id],
                |row| row.get(0),
            )
            .expect("عدّ المستخدمين يجب أن يقرأ")
    };
    assert!(users_before >= 1, "المشرف الأول يجب أن يكون موجوداً");

    // نسخة احتياطية مُتحقق منها قبل الإغلاق — دليل قابل للمقارنة.
    let backup = create_backup(
        &state,
        CreateBackupInput {
            target_directory: backup_dir.to_string_lossy().to_string(),
        },
    )
    .expect("النسخة الاحتياطية يجب أن تنشأ");

    // إغلاق الاتصال الحي فعلاً (إسقاط الحالة) ثم إعادة الفتح من نفس مسار الملف.
    drop(state);
    let reopened = Connection::open(&db_path).expect("القاعدة يجب أن تُعاد فتحها من الملف");
    reopened
        .pragma_update(None, "foreign_keys", "ON")
        .expect("مفاتيح الأجنبة يجب أن تعمل");

    // تشغيل مسار الإقلاع كاملاً على قاعدة قائمة: لا فشل، ولا تقدّم خاطئ في الإصدار.
    let mode_before =
        crate::infrastructure::database::read_runtime_mode(&reopened).expect("الوضع يجب أن يقرأ");
    crate::infrastructure::database::apply_migrations(&reopened)
        .expect("إعادة تشغيل الهجرات على قاعدة قائمة يجب أن تنجح");
    crate::infrastructure::database::ensure_permission_catalog(&reopened)
        .expect("إعادة تشغيل الكتالوج يجب أن تنجح");
    let mode_after = crate::infrastructure::database::read_runtime_mode(&reopened)
        .expect("الوضع بعد الإقلاع يجب أن يقرأ");

    assert_eq!(mode_before, crate::domain::models::RuntimeMode::Production);
    assert_eq!(mode_after, crate::domain::models::RuntimeMode::Production);

    let schema_version: i64 = reopened
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
            row.get(0)
        })
        .expect("إصدار المخطط يجب أن يقرأ");
    assert_eq!(schema_version, 26);

    let obligations_after: i64 = reopened
        .query_row(
            "SELECT COUNT(*) FROM obligations WHERE tenant_id = ?1 AND id = 'obl_reopen_001'",
            [&tenant_id],
            |row| row.get(0),
        )
        .expect("الالتزام يجب أن يقرأ بعد إعادة الفتح");
    assert_eq!(obligations_after, 1, "بيانات المستخدم يجب ألا تُفقد");
    let users_after: i64 = reopened
        .query_row(
            "SELECT COUNT(*) FROM local_users WHERE tenant_id = ?1",
            [&tenant_id],
            |row| row.get(0),
        )
        .expect("المستخدمون يجب أن يقرؤوا بعد إعادة الفتح");
    assert_eq!(users_after, users_before, "المستخدمون يجب أن يبقوا");

    // سلامة القاعدة بعد إعادة الفتح: لا انتهاك مفاتيح ولا تلف.
    let violations: i64 = reopened
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .expect("فحص المفاتيح يجب أن ينجح");
    assert_eq!(violations, 0);
    let integrity: String = reopened
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("فحص السلامة يجب أن ينجح");
    assert_eq!(integrity, "ok");

    // النسخة الاحتياطية المأخوذة قبل إعادة الفتح ما تزال صالحة بنفس الهاش والحجم.
    let manifest_path = backup_dir.join(format!("{}.manifest.json", backup.file_name));
    let manifest: BackupManifest = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).expect("البيان الجانبي يجب أن يقرأ"),
    )
    .expect("البيان يجب أن يُفكّ");
    assert_eq!(manifest.sha256, backup.sha256);
    assert_eq!(manifest.mode, "PRODUCTION");
    assert_eq!(manifest.schema_version, 26);

    drop(reopened);
    let _ = std::fs::remove_dir_all(&backup_dir);
}

#[test]
fn backup_then_mutate_then_restore_roundtrips_data_with_fk_integrity() {
    let (state, db_path) = production_state_on_file("roundtrip");
    let backup_dir = temp_dir("roundtrip-backups");

    // بيانات قبل النسخة: مصدر تنظيمي إنتاجي + التزام واحد يشير إليه.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let now = chrono::Utc::now().to_rfc3339();
        let tenant_id = state.tenant_id();
        connection
            .execute(
                "INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES ('source_prod_001', ?1, 'Production Authority', 'Production reference', 'https://example.invalid/prod', '1.0', ?2, 0, 'REQUIRES_REVIEW', ?2, ?2)",
                rusqlite::params![tenant_id, now],
            )
            .expect("المصدر الإنتاجي يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, source_type, created_at, updated_at) VALUES ('obl_prod_001', ?1, 'source_prod_001', 'Production obligation retained', 'Owner', '2026-12-01T00:00:00Z', 'HIGH', 'OPEN', 'APPLIES', 'MISSING', 'UNVERIFIED', 'OFFICIAL', ?2, ?2)",
                rusqlite::params![tenant_id, now],
            )
            .expect("الالتزام يجب أن ينشأ");
    }

    let result = create_backup(
        &state,
        CreateBackupInput {
            target_directory: backup_dir.to_string_lossy().to_string(),
        },
    )
    .expect("النسخة الاحتياطية يجب أن تنشأ");
    assert!(result.sha256.len() == 64);
    assert_eq!(result.mode, "PRODUCTION");
    assert_eq!(result.schema_version, 26);

    // تحوير البيانات بعد النسخة: حذف الالتزام + تغيير الاسم.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute("DELETE FROM obligations", [])
            .expect("الحذف يجب أن ينجح");
        connection
            .execute(
                "UPDATE local_users SET display_name = 'MUTATED' WHERE id LIKE 'user_prod_%'",
                [],
            )
            .expect("التحوير يجب أن ينجح");
    }

    let backup_file = backup_dir.join(&result.file_name);
    let restore = restore_backup(
        &state,
        RestoreBackupInput {
            backup_file: backup_file.to_string_lossy().to_string(),
            confirm: true,
        },
    )
    .expect("الاستعادة يجب أن تنجح");
    assert_eq!(restore.restored_mode, "PRODUCTION");
    assert!(
        !restore.authenticated,
        "بعد الاستعادة: لا هوية حتى الدخول مجدداً"
    );

    // القاعدة المستعادة: البيانات الأصلية عادت والمحوّر زال.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let obligations: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM obligations WHERE id = 'obl_prod_001'",
                [],
                |row| row.get(0),
            )
            .expect("العد يجب أن يقرأ");
        assert_eq!(obligations, 1);
        let display: String = connection
            .query_row(
                "SELECT display_name FROM local_users WHERE id LIKE 'user_prod_%'",
                [],
                |row| row.get(0),
            )
            .expect("الاسم يجب أن يقرأ");
        assert_eq!(display, "Backup Admin");
        let violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("الفحص يجب أن ينجح");
        assert_eq!(violations, 0);
        // أثر الاستعادة مكتوب داخل القاعدة المستعادة نفسها.
        let restored_event: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE event_type = 'BACKUP_RESTORED'",
                [],
                |row| row.get(0),
            )
            .expect("التدقيق يجب أن يقرأ");
        assert_eq!(restored_event, 1);
    }

    // الدخول من جديد على القاعدة المستعادة يعمل ببيانات الأصل.
    login(
        &state,
        LoginInput {
            email: "admin@backup.test".to_owned(),
            password: "Str0ng!Passw0rd#2026".to_owned(),
        },
    )
    .expect("الدخول بعد الاستعادة يجب أن ينجح");
    std::fs::remove_file(&db_path).ok();
}

#[test]
fn tampered_backup_file_is_rejected_by_hash_mismatch() {
    let (state, db_path) = production_state_on_file("tampered");
    let backup_dir = temp_dir("tampered-backups");

    let result = create_backup(
        &state,
        CreateBackupInput {
            target_directory: backup_dir.to_string_lossy().to_string(),
        },
    )
    .expect("النسخة يجب أن تنشأ");
    let backup_file = backup_dir.join(&result.file_name);

    // تلاعب ببايت واحد داخل ملف النسخة — الهاش يجب أن يكشفه.
    let bytes = std::fs::read(&backup_file).expect("النسخة يجب أن تقرأ");
    let mut tampered = bytes;
    let mid = tampered.len() / 2;
    tampered[mid] = tampered[mid].wrapping_add(1);
    std::fs::write(&backup_file, tampered).expect("النسخة المحرفة يجب أن تكتب");

    assert!(matches!(
        restore_backup(
            &state,
            RestoreBackupInput {
                backup_file: backup_file.to_string_lossy().to_string(),
                confirm: true,
            },
        ),
        Err(AppError::BackupCorrupted)
    ));
    std::fs::remove_file(&db_path).ok();
}

#[test]
fn missing_manifest_wrong_extension_and_unconfirmed_restore_are_rejected() {
    let (state, db_path) = production_state_on_file("invalid");
    let backup_dir = temp_dir("invalid-backups");

    let result = create_backup(
        &state,
        CreateBackupInput {
            target_directory: backup_dir.to_string_lossy().to_string(),
        },
    )
    .expect("النسخة يجب أن تنشأ");
    let backup_file = backup_dir.join(&result.file_name);

    // بلا تأكيد صريح: رفض تحقق.
    assert!(matches!(
        restore_backup(
            &state,
            RestoreBackupInput {
                backup_file: backup_file.to_string_lossy().to_string(),
                confirm: false,
            },
        ),
        Err(AppError::ValidationFailed)
    ));

    // حذف البيان الجانبي: رفض صيغة.
    std::fs::remove_file(super::manifest_path_for_test(&backup_file)).expect("البيان يجب أن يُحذف");
    assert!(matches!(
        restore_backup(
            &state,
            RestoreBackupInput {
                backup_file: backup_file.to_string_lossy().to_string(),
                confirm: true,
            },
        ),
        Err(AppError::BackupInvalid)
    ));

    // امتداد غير nxbackup: رفض مسار.
    assert!(matches!(
        restore_backup(
            &state,
            RestoreBackupInput {
                backup_file: db_path.to_string_lossy().to_string(),
                confirm: true,
            },
        ),
        Err(AppError::BackupInvalid)
    ));

    // بيان بصيغة مجهولة: رفض.
    let foreign = backup_dir.join("foreign.nxbackup");
    std::fs::write(&foreign, b"not a database").expect("الملف الأجنبي يجب أن يكتب");
    std::fs::write(
        super::manifest_path_for_test(&foreign),
        serde_json::to_vec_pretty(&BackupManifest {
            format: "SOMETHING_ELSE".to_owned(),
            app_version: "0.0.0".to_owned(),
            schema_version: 21,
            created_at: chrono::Utc::now().to_rfc3339(),
            mode: "DEMO".to_owned(),
            sha256: "0".repeat(64),
            bytes: 17,
        })
        .unwrap(),
    )
    .expect("بيان أجنبي يجب أن يكتب");
    assert!(matches!(
        restore_backup(
            &state,
            RestoreBackupInput {
                backup_file: foreign.to_string_lossy().to_string(),
                confirm: true,
            },
        ),
        Err(AppError::BackupInvalid)
    ));
    std::fs::remove_file(&db_path).ok();
}

#[test]
fn future_schema_backup_is_rejected_as_incompatible() {
    let (state, db_path) = production_state_on_file("future");
    let backup_dir = temp_dir("future-backups");

    let result = create_backup(
        &state,
        CreateBackupInput {
            target_directory: backup_dir.to_string_lossy().to_string(),
        },
    )
    .expect("النسخة يجب أن تنشأ");
    let backup_file = backup_dir.join(&result.file_name);

    // رفع إصدار المخطط في البيان إلى المستقبل — يجب أن يرفض كعدم توافق.
    let manifest_path = super::manifest_path_for_test(&backup_file);
    let raw = std::fs::read_to_string(&manifest_path).expect("البيان يجب أن يقرأ");
    let mut manifest: BackupManifest = serde_json::from_str(&raw).expect("البيان يجب أن يفهم");
    manifest.schema_version = 99;
    std::fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .expect("البيان المعدل يجب أن يكتب");

    // تعديل الإصدار في البيان يفكك تطابق هاش المحتوى؟ لا — البيان خارج الهاش،
    // لكن فحص القاعدة نفسها سيكشف اختلاف الإصدار الفعلي (21 ≠ 99) → تلف.
    assert!(matches!(
        restore_backup(
            &state,
            RestoreBackupInput {
                backup_file: backup_file.to_string_lossy().to_string(),
                confirm: true,
            },
        ),
        Err(AppError::BackupCorrupted | AppError::BackupIncompatible)
    ));
    std::fs::remove_file(&db_path).ok();
}

#[test]
fn demo_backup_restores_demo_mode_with_active_operator_session() {
    let (state, db_path) = demo_state_on_file("demorestore");
    let backup_dir = temp_dir("demorestore-backups");

    let result = create_backup(
        &state,
        CreateBackupInput {
            target_directory: backup_dir.to_string_lossy().to_string(),
        },
    )
    .expect("نسخة الديمو يجب أن تنشأ");
    assert_eq!(result.mode, "DEMO");

    let backup_file = backup_dir.join(&result.file_name);
    let restore = restore_backup(
        &state,
        RestoreBackupInput {
            backup_file: backup_file.to_string_lossy().to_string(),
            confirm: true,
        },
    )
    .expect("استعادة الديمو يجب أن تنجح");
    assert_eq!(restore.restored_mode, "DEMO");
    assert!(
        restore.authenticated,
        "الديمو يستأنخ بمشغله مباشرة بعد الاستعادة"
    );

    // التفويض يعمل فوراً بعد استعادة الديمو.
    assert!(
        crate::application::authorization_service::require_permission(&state, "OBLIGATION", "READ")
            .is_ok()
    );
    std::fs::remove_file(&db_path).ok();
}

#[test]
fn list_backups_reads_only_valid_manifests_and_requires_permission() {
    let (state, db_path) = production_state_on_file("list");
    let backup_dir = temp_dir("list-backups");

    create_backup(
        &state,
        CreateBackupInput {
            target_directory: backup_dir.to_string_lossy().to_string(),
        },
    )
    .expect("النسخة يجب أن تنشأ");

    let listed = list_backups(&state, backup_dir.to_string_lossy().to_string())
        .expect("القائمة يجب أن تُقرأ");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].format, BACKUP_FORMAT);

    // بلا إذن BACKUP:CREATE لا قائمة — هوية بلا صلاحية تُرفض.
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    connection
        .execute("DELETE FROM role_permissions", [])
        .expect("نزع المنح يجب أن ينجح");
    drop(connection);
    assert!(matches!(
        list_backups(&state, backup_dir.to_string_lossy().to_string()),
        Err(AppError::AuthorizationDenied)
    ));
    std::fs::remove_file(&db_path).ok();
}

#[test]
fn backup_rejects_relative_and_traversal_directories() {
    let (state, db_path) = production_state_on_file("paths");

    assert!(matches!(
        create_backup(
            &state,
            CreateBackupInput {
                target_directory: "relative/dir".to_owned(),
            },
        ),
        Err(AppError::ValidationFailed)
    ));
    let cwd = std::env::current_dir().expect("دليل العمل يجب أن يقرأ");
    let traversal = cwd
        .join("..")
        .join("..")
        .join("somewhere")
        .to_string_lossy()
        .to_string();
    assert!(matches!(
        create_backup(
            &state,
            CreateBackupInput {
                target_directory: traversal,
            },
        ),
        Err(AppError::ValidationFailed)
    ));
    assert!(matches!(
        create_backup(
            &state,
            CreateBackupInput {
                target_directory: cwd
                    .join("definitely-missing-dir-xyz")
                    .to_string_lossy()
                    .to_string(),
            },
        ),
        Err(AppError::BackupStorageError)
    ));
    std::fs::remove_file(&db_path).ok();
}
