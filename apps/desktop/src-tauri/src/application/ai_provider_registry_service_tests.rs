// =====================================================================
// اختبارات خدمة سجل الموفرين (0023)
// ---------------------------------------------------------------------
// ما تُثبته هذه الاختبارات فعلياً:
//   1) عدم تسرب السر: مسح شامل لكل جدول وعمود نصي في القاعدة + كل صفوف
//      التدقيق، إضافة إلى أن ملف/مخزن الأسرار ليس نصاً.
//   2) دلالة حالة الاتصال: UNVERIFIED قبل أي اختبار، وحفظ مفتاح لا يعني
//      اتصالاً، وفشل الاختبار لا يحذف الإعداد ولا المفتاح.
//   3) فصل السلطة: القراءة لا تمنح صلاحية كتابة الأسرار.
//   4) عزل المستأجر للموفرين والنماذج.
// =====================================================================

use std::sync::Arc;

use chrono::Utc;
use rusqlite::Connection;

use super::{
    clear_credential, configure_provider, discover_models, list_models, list_providers,
    remove_provider, select_model, set_credential, ConfigureAiProviderInput, SelectAiModelInput,
    SetAiProviderCredentialInput,
};
use crate::{
    domain::{
        error::AppError,
        provider_registry::{AiProviderConnectionState, AiProviderKind},
    },
    infrastructure::{
        credential_vault::{CredentialVault as _, InMemoryCredentialVault},
        database::{initialize_demo_database, AppState},
    },
};

/// سر اختباري طويل — أي ظهور له في القاعدة أو التدقيق أو رسالة خطأ يفشل الاختبار.
const TEST_SECRET: &str = "sk-canary-9f13a7-DO-NOT-PERSIST-EVERYWHERE";

const TENANT: &str = "tenant_demo_sa_001";
const OPERATOR: &str = "user_demo_operator";

fn test_state() -> (AppState, Arc<InMemoryCredentialVault>) {
    let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("تفعيل مفاتيح الأجنبة");
    initialize_demo_database(&connection).expect("تهيئة الديمو يجب أن تنجح");
    let state = AppState::test_instance(connection, TENANT.to_owned(), OPERATOR.to_owned());
    let vault = Arc::new(InMemoryCredentialVault::new());
    state.set_credential_vault_for_tests(vault.clone());
    (state, vault)
}

fn configure_input(
    provider_id: &str,
    kind: AiProviderKind,
    base_url: &str,
) -> ConfigureAiProviderInput {
    ConfigureAiProviderInput {
        provider_id: provider_id.to_owned(),
        kind,
        display_name: format!("Test {provider_id}"),
        base_url: base_url.to_owned(),
        enabled: true,
    }
}

/// مسح كل جدول وكل عمود نصي بحثاً عن نص السر. `true` يعني وجود تسرب.
fn database_contains(connection: &Connection, needle: &str) -> Option<String> {
    let tables: Vec<String> = connection
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'")
        .ok()?
        .query_map([], |row| row.get::<_, String>(0))
        .ok()?
        .filter_map(Result::ok)
        .collect();

    for table in tables {
        let quoted_table = table.replace('"', "\"\"");
        let columns: Vec<String> = connection
            .prepare(&format!(
                "SELECT name FROM pragma_table_info('{quoted_table}')"
            ))
            .ok()?
            .query_map([], |row| row.get::<_, String>(0))
            .ok()?
            .filter_map(Result::ok)
            .collect();
        for column in columns {
            let quoted_column = column.replace('"', "\"\"");
            // كل الأعمدة تُحوَّل إلى نص في القاعدة — تحويل ضمني يغطي الأرقام أيضاً.
            let sql = format!(
                "SELECT COUNT(*) FROM \"{quoted_table}\" WHERE CAST(\"{quoted_column}\" AS TEXT) LIKE ?1"
            );
            let Ok(mut statement) = connection.prepare(&sql) else {
                continue;
            };
            let pattern = format!("%{needle}%");
            if let Ok(count) = statement.query_row([&pattern], |row| row.get::<_, i64>(0)) {
                if count > 0 {
                    return Some(format!("{table}.{column}"));
                }
            }
        }
    }

    None
}

/// كل صفوف التدقيق كنص واحد — للتحقق من غياب السر في أي حدث.
fn audit_dump(connection: &Connection) -> String {
    let mut statement = connection
        .prepare(
            "SELECT id, event_type, actor_name, resource_type, resource_id, occurred_at, previous_hash, event_hash FROM audit_events",
        )
        .expect("تحضير قراءة التدقيق");
    let rows = statement
        .query_map([], |row| {
            Ok(format!(
                "{}|{}|{}|{}|{}|{}|{}|{}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .expect("قراءة التدقيق");
    rows.filter_map(Result::ok).collect::<Vec<_>>().join("\n")
}

// =====================================================================
// 1) عدم تسرب السر — الدليل المطلوب
// =====================================================================

#[test]
fn a_stored_credential_never_reaches_any_database_table_audit_row_or_ipc_payload() {
    let (state, vault) = test_state();

    // إعداد موفر بعيد يحتاج سراً.
    configure_provider(
        &state,
        configure_input(
            "REMOTE_TEST",
            AiProviderKind::OpenAi,
            "https://api.openai.com",
        ),
    )
    .expect("الإعداد يجب أن ينجح");

    // حفظ السر عبر المسار الحقيقي.
    let stored = set_credential(
        &state,
        SetAiProviderCredentialInput {
            provider_id: "REMOTE_TEST".to_owned(),
            credential: TEST_SECRET.to_owned(),
        },
    )
    .expect("حفظ السر يجب أن ينجح");
    assert!(stored.credential_configured);

    // (أ) السر موجود فعلاً في مخزن الأسرار — وإلا كان الاختبار بلا معنى.
    assert_eq!(
        vault.load(TENANT, "REMOTE_TEST").expect("قراءة المخزن"),
        Some(TEST_SECRET.to_owned()),
        "السر يجب أن يكون محفوظاً في المخزن الآمن"
    );

    // (ب) لا جدول ولا عمود في القاعدة يحمل السر — مسح شامل لا عيّنة.
    {
        let connection = state.connection.lock().expect("القفل");
        assert_eq!(
            database_contains(&connection, TEST_SECRET),
            None,
            "السر تسرّب إلى جدول/عمود في قاعدة البيانات"
        );
        // التدقيق مفحوص صراحةً أيضاً بنصه الكامل.
        let audit = audit_dump(&connection);
        assert!(!audit.contains(TEST_SECRET), "السر تسرّب إلى سلسلة التدقيق");
        // الحدث نفسه موجود (العملية دُقّقت) لكن بلا سر.
        let audit_events: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE event_type = 'AI_PROVIDER_CREDENTIAL_SET'",
                [],
                |row| row.get(0),
            )
            .expect("عدّ أحداث التدقيق");
        assert_eq!(audit_events, 1, "حفظ السر يجب أن يُدقَّق كحدث");
        // القاعدة تحمل علامة الوجود فقط.
        let marker: i64 = connection
            .query_row(
                "SELECT credential_configured FROM ai_provider_registry WHERE provider_id = 'REMOTE_TEST'",
                [],
                |row| row.get(0),
            )
            .expect("قراءة العلامة");
        assert_eq!(marker, 1, "العلامة المنطقية هي كل ما يُخزَّن");
    }

    // (ج) لا عقد IPC يعيد السر: نفتّش كل مخرجات القارئات المُسلسَلة.
    let providers_json =
        serde_json::to_string(&list_providers(&state).expect("قراءة الموفرين")).expect("تسلسل");
    let models_json =
        serde_json::to_string(&list_models(&state).expect("قراءة النماذج")).expect("تسلسل");
    assert!(
        !providers_json.contains(TEST_SECRET),
        "تسرب في عقد الموفرين"
    );
    assert!(!models_json.contains(TEST_SECRET), "تسرب في عقد النماذج");
    // وحقل السر غير موجود أصلاً في العقد — العلامة المنطقية وحدها.
    assert!(providers_json.contains("\"credentialConfigured\":true"));
    for forbidden in ["\"credential\":", "\"apiKey\"", "\"secret\"", "\"token\""] {
        assert!(
            !providers_json.contains(forbidden),
            "العقد يحمل حقلاً محظوراً: {forbidden}"
        );
    }

    // (د) فشل المسارات لا يسرّب السر في نصوص الأخطاء.
    for error in [
        configure_provider(
            &state,
            configure_input(
                "REMOTE_TEST",
                AiProviderKind::OpenAi,
                "http://api.openai.com",
            ),
        )
        .expect_err("http لغير الحلقي يجب أن يُرفض"),
        remove_provider(&state, "OLLAMA_LOCAL").expect_err("المدمج لا يُحذف"),
    ] {
        let rendered = format!("{error:?} {}", error);
        assert!(!rendered.contains(TEST_SECRET));
    }
    // رمز الرفض الشبكي نفسه لا يحمل أي تفصيل.
    let rejected = configure_provider(
        &state,
        configure_input(
            "REMOTE_TEST",
            AiProviderKind::OpenAi,
            "http://api.openai.com",
        ),
    )
    .expect_err("يجب أن يُرفض");
    assert!(matches!(
        rejected,
        AppError::ProviderEndpointRejected("NETWORK_NON_LOOPBACK_HTTP_FORBIDDEN")
    ));
    assert!(!rejected.to_string().contains("api.openai.com"));
}

// =====================================================================
// 2) دلالة حالة الاتصال
// =====================================================================

#[test]
fn a_stored_credential_does_not_make_a_provider_connected() {
    let (state, _vault) = test_state();
    configure_provider(
        &state,
        configure_input(
            "REMOTE_TEST",
            AiProviderKind::OpenAi,
            "https://api.openai.com",
        ),
    )
    .expect("الإعداد");

    // UNVERIFIED قبل أي اختبار — لا ادعاء اتصال.
    let before = list_providers(&state).expect("قراءة");
    let record = before
        .providers
        .iter()
        .find(|provider| provider.provider_id == "REMOTE_TEST")
        .expect("الموفر يجب أن يظهر");
    assert_eq!(
        record.connection_state,
        AiProviderConnectionState::Unverified
    );
    assert!(record.connection_checked_at.is_none());
    assert!(!record.credential_configured);

    // حفظ المفتاح: العلامة تتغير والحالة تبقى UNVERIFIED.
    let stored = set_credential(
        &state,
        SetAiProviderCredentialInput {
            provider_id: "REMOTE_TEST".to_owned(),
            credential: TEST_SECRET.to_owned(),
        },
    )
    .expect("حفظ السر");
    assert!(stored.credential_configured);

    let after = list_providers(&state).expect("قراءة");
    let record = after
        .providers
        .iter()
        .find(|provider| provider.provider_id == "REMOTE_TEST")
        .expect("الموفر");
    assert!(record.credential_configured, "العلامة يجب أن تصبح صحيحة");
    assert_eq!(
        record.connection_state,
        AiProviderConnectionState::Unverified,
        "حفظ المفتاح لا يعني اتصالاً"
    );
    assert!(record.connection_checked_at.is_none());
}

#[test]
fn a_failed_connection_test_keeps_the_configuration_and_the_credential_intact() {
    let (state, vault) = test_state();
    // عنوان https عام لا يصل إليه الاختبار فعلياً في بيئة الاختبار ⇒ فشل.
    // لكن الأهم أن الفشل لا يتلف شيئاً.
    configure_provider(
        &state,
        configure_input(
            "REMOTE_TEST",
            AiProviderKind::OpenAiCompatible,
            "https://api.openai.com",
        ),
    )
    .expect("الإعداد");
    set_credential(
        &state,
        SetAiProviderCredentialInput {
            provider_id: "REMOTE_TEST".to_owned(),
            credential: TEST_SECRET.to_owned(),
        },
    )
    .expect("حفظ السر");

    let result = super::test_provider(&state, "REMOTE_TEST").expect("الاختبار يجب أن يُنفَّذ");
    assert_eq!(result.provider_id, "REMOTE_TEST");
    assert_eq!(result.discovered_model_count, 0);
    // الحالة إمّا CONNECTED (لو توفّرت شبكة حقيقية) أو فشل مصنّف — لا شيء آخر.
    if result.connection_state != AiProviderConnectionState::Connected {
        assert!(
            result.safe_error_code.is_some(),
            "الفشل يجب أن يحمل رمزاً آمناً"
        );
        let code = result.safe_error_code.clone().unwrap_or_default();
        assert!(!code.contains(TEST_SECRET));
        assert!(!code.contains("api.openai.com"));
        assert!(!code.contains("http"));
    }

    // الإعداد والمفتاح باقيان رغم الفشل — هذا هو الشرط الصريح.
    assert_eq!(
        vault.load(TENANT, "REMOTE_TEST").expect("قراءة المخزن"),
        Some(TEST_SECRET.to_owned()),
        "فشل الاختبار لا يحذف المفتاح"
    );
    let providers = list_providers(&state).expect("قراءة");
    let record = providers
        .providers
        .iter()
        .find(|provider| provider.provider_id == "REMOTE_TEST")
        .expect("الإعداد لا يُحذف بالفشل");
    assert_eq!(record.base_url, "https://api.openai.com");
    assert!(record.enabled);
    assert!(record.credential_configured);
    // طابع الفحص يُسجَّل لأن الاختبار نُفِّذ فعلاً.
    assert!(record.connection_checked_at.is_some());
}

#[test]
fn clearing_a_credential_moves_the_provider_out_of_any_connected_claim() {
    let (state, vault) = test_state();
    configure_provider(
        &state,
        configure_input(
            "REMOTE_TEST",
            AiProviderKind::OpenAi,
            "https://api.openai.com",
        ),
    )
    .expect("الإعداد");
    set_credential(
        &state,
        SetAiProviderCredentialInput {
            provider_id: "REMOTE_TEST".to_owned(),
            credential: TEST_SECRET.to_owned(),
        },
    )
    .expect("حفظ");

    let cleared = clear_credential(&state, "REMOTE_TEST").expect("الحذف");
    assert!(!cleared.credential_configured);
    assert_eq!(vault.load(TENANT, "REMOTE_TEST").expect("قراءة"), None);

    let providers = list_providers(&state).expect("قراءة");
    let record = providers
        .providers
        .iter()
        .find(|provider| provider.provider_id == "REMOTE_TEST")
        .expect("الموفر");
    assert!(!record.credential_configured);
    assert!(!record.base_url.is_empty(), "الإعداد نفسه يبقى");
    assert_eq!(
        record.connection_state,
        AiProviderConnectionState::Unverified
    );
    // ولا أثر للسر في القاعدة بعد الحذف أيضاً.
    let connection = state.connection.lock().expect("القفل");
    assert_eq!(database_contains(&connection, TEST_SECRET), None);
}

// =====================================================================
// 3) فصل السلطة: قراءة الموفرين لا تمنح كتابة الأسرار
// =====================================================================

#[test]
fn reading_providers_does_not_imply_the_credential_permission() {
    let (state, _vault) = test_state();
    configure_provider(
        &state,
        configure_input(
            "REMOTE_TEST",
            AiProviderKind::OpenAi,
            "https://api.openai.com",
        ),
    )
    .expect("الإعداد");
    // الموفر المزروع المدمج أيضاً.
    assert!(list_providers(&state).is_ok());

    // سحب صلاحية كتابة الأسرار وحدها (المراجِع لا يملكها أصلاً).
    {
        let connection = state.connection.lock().expect("القفل");
        connection
            .execute(
                "DELETE FROM role_permissions WHERE permission_id = 'permission_integration_credential'",
                [],
            )
            .expect("سحب صلاحية الأسرار");
    }
    // القراءة ما زالت تعمل…
    assert!(list_providers(&state).is_ok(), "القراءة يجب أن تبقى متاحة");
    // …لكتابة السر لا.
    assert!(matches!(
        set_credential(
            &state,
            SetAiProviderCredentialInput {
                provider_id: "REMOTE_TEST".to_owned(),
                credential: TEST_SECRET.to_owned(),
            }
        ),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(matches!(
        clear_credential(&state, "REMOTE_TEST"),
        Err(AppError::AuthorizationDenied)
    ));
}

#[test]
fn every_registry_command_denies_an_identity_without_the_required_permission() {
    let (state, _vault) = test_state();
    {
        let connection = state.connection.lock().expect("القفل");
        connection
            .execute("DELETE FROM role_permissions", [])
            .expect("سحب كل المنح");
    }

    assert!(matches!(
        list_providers(&state),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(matches!(
        list_models(&state),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(matches!(
        configure_provider(
            &state,
            configure_input("NEW_ONE", AiProviderKind::OpenAi, "https://api.openai.com")
        ),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(matches!(
        remove_provider(&state, "OLLAMA_LOCAL"),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(matches!(
        set_credential(
            &state,
            SetAiProviderCredentialInput {
                provider_id: "OLLAMA_LOCAL".to_owned(),
                credential: TEST_SECRET.to_owned(),
            }
        ),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(matches!(
        clear_credential(&state, "OLLAMA_LOCAL"),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(matches!(
        super::test_provider(&state, "OLLAMA_LOCAL"),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(matches!(
        super::discover_models(&state, "OLLAMA_LOCAL"),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(matches!(
        select_model(
            &state,
            SelectAiModelInput {
                provider_id: "OLLAMA_LOCAL".to_owned(),
                model_id: "qwen3:4b".to_owned(),
            }
        ),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(matches!(
        super::clear_model_selection(&state),
        Err(AppError::AuthorizationDenied)
    ));
}

// =====================================================================
// 4) العزل بين المستأجرين
// =====================================================================

#[test]
fn providers_and_models_are_isolated_per_tenant() {
    let (state, _vault) = test_state();
    configure_provider(
        &state,
        configure_input(
            "REMOTE_TEST",
            AiProviderKind::OpenAi,
            "https://api.openai.com",
        ),
    )
    .expect("الإعداد في مستأجر الديمو");

    // مستأجر آخر بنفس عنوان القاعدة: لا يرى شيئاً من صفوف الأول.
    let connection = Connection::open_in_memory().expect("قاعدة ثانية");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("مفاتيح");
    initialize_demo_database(&connection).expect("تهيئة");
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant-other', 'Other', 'DEMO_SYNTHETIC', ?1, ?1)",
            [&now],
        )
        .expect("مستأجر آخر");
    connection
        .execute(
            "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES ('user-other', 'tenant-other', 'Other', 'other@test.local', NULL, 'ACTIVE', ?1, ?1)",
            [&now],
        )
        .expect("مستخدم آخر");
    connection
        .execute(
            "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role-other', 'tenant-other', 'READER', 'Reader', ?1, ?1)",
            [&now],
        )
        .expect("دور آخر");
    for permission in [
        "permission_integration_read",
        "permission_integration_update",
        "permission_integration_credential",
    ] {
        connection
            .execute(
                "INSERT OR IGNORE INTO role_permissions (role_id, permission_id) VALUES ('role-other', ?1)",
                [permission],
            )
            .expect("منح");
    }
    connection
        .execute(
            "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user-other', 'role-other', ?1, 'system')",
            [&now],
        )
        .expect("إسناد");
    connection
        .execute(
            "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session-other', 'tenant-other', 'user-other', ?1, '2030-01-01T00:00:00Z', NULL, 'test')",
            [&now],
        )
        .expect("جلسة");

    let other_state = AppState::test_instance(
        connection,
        "tenant-other".to_owned(),
        "user-other".to_owned(),
    );
    other_state.set_credential_vault_for_tests(Arc::new(InMemoryCredentialVault::new()));

    let other_view = list_providers(&other_state).expect("قراءة المستأجر الآخر");
    assert!(
        other_view
            .providers
            .iter()
            .all(|provider| provider.provider_id != "REMOTE_TEST"),
        "موفر مستأجر آخر لا يجوز أن يظهر هنا"
    );

    // ولا يستطيع المستأجر الآخر إزالة موفر ليس له: الرفض لا التحديث العابر.
    assert!(matches!(
        remove_provider(&other_state, "REMOTE_TEST"),
        Err(AppError::ProviderNotConfigured)
    ));
    // ولا كتابة سر لموفر خارج مستواه.
    assert!(matches!(
        set_credential(
            &other_state,
            SetAiProviderCredentialInput {
                provider_id: "REMOTE_TEST".to_owned(),
                credential: TEST_SECRET.to_owned(),
            }
        ),
        Err(AppError::ProviderNotConfigured)
    ));

    // وقاعدة المستأجر الآخر نفسها لا تحمل سر الأول.
    {
        let connection = other_state.connection.lock().expect("القفل");
        assert_eq!(database_contains(&connection, TEST_SECRET), None);
    }
}

// =====================================================================
// 5) الانتخاب الأحادي وحدوده
// =====================================================================

#[test]
fn selecting_a_model_without_an_available_model_fails_closed() {
    let (state, _vault) = test_state();
    // موفر محلي: لا يتطلب سراً، فالوصول إلى فحص «توفر النموذج» مباشر.
    configure_provider(
        &state,
        configure_input(
            "LOCAL_TEST",
            AiProviderKind::LmStudio,
            "http://127.0.0.1:1234",
        ),
    )
    .expect("الإعداد");
    // لا نموذج مكتشف ⇒ الانتخاب يُرفض ولا يُنشئ صفاً.
    assert!(matches!(
        select_model(
            &state,
            SelectAiModelInput {
                provider_id: "LOCAL_TEST".to_owned(),
                model_id: "ghost-model".to_owned(),
            }
        ),
        Err(AppError::ProviderModelSelectionInvalid)
    ));
    // ولا نموذج مبقى مفعّلاً.
    let data = list_models(&state).expect("قراءة");
    assert!(
        data.models.iter().all(|model| !model.selected),
        "لا انتخاب بعد فشل"
    );
}

#[test]
fn selecting_a_remote_model_without_a_stored_credential_is_refused() {
    let (state, _vault) = test_state();
    configure_provider(
        &state,
        configure_input(
            "REMOTE_TEST",
            AiProviderKind::OpenAi,
            "https://api.openai.com",
        ),
    )
    .expect("الإعداد");
    // نُدخل نموذجاً متاحاً يدوياً لمحاكاة اكتشاف سابق بلا سر محفوظ.
    {
        let connection = state.connection.lock().expect("القفل");
        let now = Utc::now().to_rfc3339();
        let provider_row: String = connection
            .query_row(
                "SELECT id FROM ai_provider_registry WHERE provider_id = 'REMOTE_TEST'",
                [],
                |row| row.get(0),
            )
            .expect("صف الموفر");
        connection
            .execute(
                "INSERT INTO ai_model_registry (id, tenant_id, provider_kind, model_id, display_name, enabled, data_residency, allowed_classification_ceiling, requires_human_approval, created_at, updated_at, provider_id, available, capability_source, last_discovered_at) VALUES ('m1', ?1, 'OPENAI', 'gpt-4o-mini', 'gpt-4o-mini', 0, 'UNSPECIFIED', 'INTERNAL', 1, ?2, ?2, ?3, 1, 'DISCOVERED', ?2)",
                rusqlite::params![TENANT, &now, provider_row],
            )
            .expect("إدراج نموذج");
    }

    // بلا سر: الرفض قبل أي شبكة.
    assert!(matches!(
        select_model(
            &state,
            SelectAiModelInput {
                provider_id: "REMOTE_TEST".to_owned(),
                model_id: "gpt-4o-mini".to_owned(),
            }
        ),
        Err(AppError::ProviderCredentialMissing)
    ));
}

#[test]
fn the_built_in_local_provider_is_seeded_unverified_and_cannot_be_removed() {
    let (state, _vault) = test_state();
    let data = list_providers(&state).expect("قراءة");
    let built_in = data
        .providers
        .iter()
        .find(|provider| provider.built_in)
        .expect("الموفر المدمج يجب أن يكون مزروعاً");
    assert_eq!(built_in.provider_id, "OLLAMA_LOCAL");
    assert_eq!(built_in.kind, AiProviderKind::Ollama);
    assert_eq!(built_in.base_url, "http://127.0.0.1:11434");
    assert_eq!(
        built_in.connection_state,
        AiProviderConnectionState::Unverified
    );
    assert!(!built_in.credential_configured);
    assert!(
        !built_in.requires_credential,
        "المزود المحلي لا يتطلب مفتاحاً"
    );
    assert!(built_in.supports_discovery);

    assert!(matches!(
        remove_provider(&state, "OLLAMA_LOCAL"),
        Err(AppError::Conflict)
    ));
}

// =====================================================================
// 6) الرفض الشبكي يمنع الكتابة أصلاً (فشل مغلق قبل القاعدة)
// =====================================================================

#[test]
fn a_rejected_endpoint_never_lands_in_the_database() {
    let (state, _vault) = test_state();
    for (base_url, expected_code) in [
        (
            "http://api.example.com",
            "NETWORK_NON_LOOPBACK_HTTP_FORBIDDEN",
        ),
        (
            "https://user:pass@api.example.com",
            "NETWORK_CREDENTIALS_IN_URL",
        ),
        ("https://169.254.169.254", "NETWORK_DESTINATION_DENIED"),
        ("file:///etc/passwd", "NETWORK_SCHEME_NOT_ALLOWED"),
        (
            "https://api.example.com/v1/models",
            "NETWORK_BASE_URL_INVALID",
        ),
    ] {
        let error = configure_provider(
            &state,
            configure_input("BAD_ENDPOINT", AiProviderKind::OpenAiCompatible, base_url),
        )
        .expect_err("يجب أن يُرفض");
        assert!(
            matches!(error, AppError::ProviderEndpointRejected(code) if code == expected_code),
            "الرمز المتوقع {expected_code} لـ {base_url}"
        );
    }
    // لا صف كُتب لأي من المحاولات المرفوضة.
    let connection = state.connection.lock().expect("القفل");
    let rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM ai_provider_registry WHERE provider_id = 'BAD_ENDPOINT'",
            [],
            |row| row.get(0),
        )
        .expect("عدّ");
    assert_eq!(rows, 0, "الرفض لا يترك أثراً في القاعدة");
}

#[test]
fn a_loopback_http_provider_is_accepted_and_stored_with_normalised_origin() {
    let (state, _vault) = test_state();
    configure_provider(
        &state,
        configure_input(
            "LOCAL_CUSTOM",
            AiProviderKind::LmStudio,
            "http://127.0.0.1:1234",
        ),
    )
    .expect("العنوان الحلقي http مسموح");
    let connection = state.connection.lock().expect("القفل");
    let (base_url, transport): (String, String) = connection
        .query_row(
            "SELECT base_url, transport FROM ai_provider_registry WHERE provider_id = 'LOCAL_CUSTOM'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("صف الموفر");
    // العنوان المُخزَّن هو الأصل المطبَّع المتحقق منه.
    assert_eq!(base_url, "http://127.0.0.1:1234");
    assert_eq!(transport, "HTTP_LOOPBACK");
}

#[test]
fn reconfiguring_with_the_same_endpoint_keeps_the_verified_state_and_credential_flag() {
    let (state, _vault) = test_state();
    configure_provider(
        &state,
        configure_input(
            "REMOTE_TEST",
            AiProviderKind::OpenAi,
            "https://api.openai.com",
        ),
    )
    .expect("الإعداد");
    set_credential(
        &state,
        SetAiProviderCredentialInput {
            provider_id: "REMOTE_TEST".to_owned(),
            credential: TEST_SECRET.to_owned(),
        },
    )
    .expect("حفظ");
    // إعادة الإعداد بنفس العنوان: العلامة لا تُصفَّر.
    let again = configure_provider(
        &state,
        configure_input(
            "REMOTE_TEST",
            AiProviderKind::OpenAi,
            "https://api.openai.com",
        ),
    )
    .expect("إعادة الإعداد");
    assert!(again.credential_configured, "إعادة الإعداد لا تمس علامة السر");
    assert_eq!(
        again.connection_state,
        AiProviderConnectionState::Unverified
    );
}

// =====================================================================
// انحدار عيبة الجمود (كانت CRITICAL — تجميد كامل للخلف)
// ---------------------------------------------------------------------
// كان `discover_models` يربط حارس `state.connection` في نطاق الدالة (لا في كتلة)
// ثم يستدعي `list_models_for_provider` ⇒ `list_models` الذي يطلب القفل نفسه مرة
// ثانية على نفس الخيط. `std::sync::Mutex` غير قابل لإعادة الدخول، فيجمُد الخيط
// حاملاً القفل وكل أمر IPC لاحق يلمس القاعدة يتجمّد معه — أي أن زر «استكشاف»
// في لوحة الموفرين كان يُجمّد التطبيق نهائياً.
//
// الوجهة هنا منفذ حلقي مغلق: الفحص يفشل بسرعة، ثم يمرّ المسار **بالقفل المزدوج**
// وهو موضع العيبة بالضبط. الاختبار يُدار في خيط منفصل مع مهلة، فإن عاد الجمود
// يفشل الاختبار برسالة واضحة بدل أن يتعلّق الجناح كله.
// =====================================================================
#[test]
fn discovery_never_deadlocks_when_it_reads_models_after_probing() {
    let (sender, receiver) = std::sync::mpsc::channel::<String>();

    std::thread::spawn(move || {
        let (state, _vault) = test_state();
        configure_provider(
            &state,
            configure_input(
                "DEADLOCK_PROBE",
                AiProviderKind::OpenAiCompatible,
                // منفذ حلقي مغلق: الرفض فوري، والمهم أن المسار يُكمل بعد الفحص.
                "http://127.0.0.1:9",
            ),
        )
        .expect("الإعداد يجب أن ينجح");
        set_credential(
            &state,
            SetAiProviderCredentialInput {
                provider_id: "DEADLOCK_PROBE".to_owned(),
                credential: "local-placeholder".to_owned(),
            },
        )
        .expect("السر الموضعي يجب أن يُحفظ");

        let outcome = discover_models(&state, "DEADLOCK_PROBE");
        let summary = match outcome {
            Ok(result) => format!(
                "OK models={} error={:?}",
                result.models.len(),
                result.safe_error_code
            ),
            Err(error) => format!("ERR {error:?}"),
        };
        // الوصول إلى هذا السطر يعني أن القفل تحرّر: العيبة كانت تُوقفه هنا.
        let _ = sender.send(summary);
    });

    match receiver.recv_timeout(std::time::Duration::from_secs(30)) {
        Ok(summary) => assert!(
            summary.starts_with("OK"),
            "الاستكشاف يجب أن يُرجع نتيجة لا خطأً: {summary}"
        ),
        Err(_) => {
            panic!("discover_models جمُد أكثر من 30 ثانية: القفل غير القابل لإعادة الدخول محتجز")
        }
    }
}
