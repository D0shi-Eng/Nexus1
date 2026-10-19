use chrono::Utc;
use rusqlite::{params, Connection};

use super::{
    create_tool, create_tool_version, disable_tool, list_tools, CreateToolInput,
    CreateToolVersionInput, ToolIdInput,
};
use crate::{
    application::authorization_service::has_permission,
    domain::{
        error::AppError,
        tool::{
            ToolContract, ToolContractField, ToolEffectProfile, ToolExternality, ToolMutation,
            ToolPrivilege, ToolReversibility,
        },
    },
    infrastructure::database::AppState,
};

// مستخدمو الاختبار: كل هوية تحمل مستوى منح مختلف لإثبات فصل صلاحيات السجل.
const AUTHOR: &str = "user-tool-author";
const READER: &str = "user-tool-reader";
const CREATOR: &str = "user-tool-creator";
const OUTSIDER: &str = "user-tool-outsider";

fn contract_with_field(field_name: &str) -> ToolContract {
    ToolContract {
        fields: vec![ToolContractField {
            name: field_name.to_owned(),
            data_type: "string".to_owned(),
            required: true,
            description: "Bounded contract field".to_owned(),
        }],
        allows_additional_fields: false,
    }
}

fn read_effect() -> ToolEffectProfile {
    ToolEffectProfile {
        mutation: ToolMutation::None,
        externality: ToolExternality::Internal,
        privilege: ToolPrivilege::Standard,
        reversibility: ToolReversibility::NotApplicable,
    }
}

fn state_effect() -> ToolEffectProfile {
    ToolEffectProfile {
        mutation: ToolMutation::StateChange,
        externality: ToolExternality::Internal,
        privilege: ToolPrivilege::Standard,
        reversibility: ToolReversibility::Reversible,
    }
}

fn create_input(name: &str) -> CreateToolInput {
    CreateToolInput {
        name: name.to_owned(),
        description: "Synthetic tool for registry service tests.".to_owned(),
        input_contract: contract_with_field("document_id"),
        output_contract: contract_with_field("summary"),
        effect: read_effect(),
    }
}

fn tool_test_state(active_user: &str) -> AppState {
    let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
    // سلسلة الهجرات الكاملة نفسها التي يشغلها الإنتاج — لا مجموعات فرعية.
    crate::infrastructure::database::apply_migrations(&connection)
        .expect("هجرات الاختبار يجب أن تنجح");
    let now = Utc::now().to_rfc3339();
    for tenant_id in ["tenant-tool-a", "tenant-tool-b"] {
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?2, 'DEMO_SYNTHETIC', ?3, ?3)",
                (tenant_id, tenant_id, &now),
            )
            .expect("المستأجر يجب أن ينشأ");
    }
    for (permission_id, action) in [
        ("permission-tool-read", "READ"),
        ("permission-tool-create", "CREATE"),
        ("permission-tool-update", "UPDATE"),
    ] {
        connection
            .execute(
                "INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES (?1, ?2, 'TOOL', ?3, 'HIGHLY_RESTRICTED', ?4)",
                params![permission_id, format!("tool:{action}"), action, &now],
            )
            .expect("صلاحية السجل يجب أن تنشأ");
    }
    // الأدوار: قارئ فقط، منشئ فقط (قراءة+إنشاء)، مؤلف كامل (قراءة+إنشاء+تحديث) — لإثبات عدم التوريث بين الأفعال.
    let role_grants: [(&str, &str, &[&str]); 3] = [
        (
            "role-tool-author",
            AUTHOR,
            &[
                "permission-tool-read",
                "permission-tool-create",
                "permission-tool-update",
            ],
        ),
        ("role-tool-reader", READER, &["permission-tool-read"]),
        (
            "role-tool-creator",
            CREATOR,
            &["permission-tool-read", "permission-tool-create"],
        ),
    ];
    for (role_id, user_id, grants) in role_grants {
        connection
            .execute(
                "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES (?1, 'tenant-tool-a', ?2, ?2, ?3, ?3)",
                params![role_id, role_id, &now],
            )
            .expect("الدور يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, 'tenant-tool-a', ?2, ?3, NULL, 'ACTIVE', ?4, ?4)",
                params![user_id, user_id, format!("{user_id}@nexus.test"), &now],
            )
            .expect("المستخدم يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, ?2, ?3, 'test')",
                params![user_id, role_id, &now],
            )
            .expect("إسناد الدور يجب أن ينجح");
        for permission_id in grants {
            connection
                .execute(
                    "INSERT INTO role_permissions (role_id, permission_id) VALUES (?1, ?2)",
                    params![role_id, permission_id],
                )
                .expect("منح الصلاحية يجب أن ينجح");
        }
        connection
            .execute(
                "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, 'tenant-tool-a', ?2, ?3, '2030-01-01T00:00:00Z', NULL, 'test')",
                params![format!("session-{user_id}"), user_id, &now],
            )
            .expect("الجلسة يجب أن تنشأ");
    }
    // مستخدم بلا أي منح سجل لإثبات الرفض المغلق.
    connection
        .execute(
            "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, 'tenant-tool-a', ?1, ?2, NULL, 'ACTIVE', ?3, ?3)",
            params![OUTSIDER, format!("{OUTSIDER}@nexus.test"), &now],
        )
        .expect("المستخدم بلا صلاحيات يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, 'tenant-tool-a', ?2, ?3, '2030-01-01T00:00:00Z', NULL, 'test')",
            params![format!("session-{OUTSIDER}"), OUTSIDER, &now],
        )
        .expect("الجلسة يجب أن تنشأ");

    AppState::test_instance(
        connection,
        "tenant-tool-a".to_owned(),
        active_user.to_owned(),
    )
}

fn seed_foreign_tool(connection: &Connection) -> String {
    // أداة في المستأجر الآخر تُزرع كبيئة ثابتة لإثبات العزل — ليست جزءاً من السلوك المختبر.
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "INSERT INTO tools (id, tenant_id, name, description, status, created_at, updated_at) VALUES ('tool-foreign', 'tenant-tool-b', 'Foreign tool', 'Seeded fixture', 'ACTIVE', ?1, ?1)",
            [&now],
        )
        .expect("أداة المستأجر الآخر يجب أن تنشأ");
    "tool-foreign".to_owned()
}

fn count_audit(connection: &Connection, event_type: &str) -> i64 {
    connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE event_type = ?1",
            [event_type],
            |row| row.get(0),
        )
        .expect("استعلام التدقيق يجب أن ينجح")
}

#[test]
fn list_requires_tool_read_and_writes_no_mutation_audit() {
    let state = tool_test_state(OUTSIDER);
    assert!(matches!(
        list_tools(&state),
        Err(AppError::AuthorizationDenied)
    ));

    state.set_active_user_for_tests(READER);
    let data = list_tools(&state).expect("القارئ المصرح له يجب أن يقرأ");
    assert!(!data.has_data);
    assert!(data.items.is_empty());
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    assert_eq!(count_audit(&connection, "TOOL_CREATED"), 0);
    assert_eq!(count_audit(&connection, "TOOL_DISABLED"), 0);
}

#[test]
fn list_isolates_tenants() {
    let state = tool_test_state(AUTHOR);
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        seed_foreign_tool(&connection);
    }
    let created =
        create_tool(&state, create_input("Isolation probe")).expect("إنشاء الأداة يجب أن ينجح");

    let data = list_tools(&state).expect("القائمة يجب أن تنجح");
    assert!(data.has_data);
    assert_eq!(data.items.len(), 1);
    assert_eq!(data.items[0].id, created.tool.id);
    assert_eq!(data.items[0].tenant_id, "tenant-tool-a");
    // أداة المستأجر الآخر لا تظهر إطلاقاً ضمن نتيجة مستأجر الاختبار.
    assert!(data
        .items
        .iter()
        .all(|tool| tool.tenant_id == "tenant-tool-a"));
}

#[test]
fn list_denies_expired_revoked_and_locked_identities() {
    let state = tool_test_state(READER);
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "UPDATE local_sessions SET expires_at = '2000-01-01T00:00:00Z'",
                [],
            )
            .expect("تعديل الجلسة يجب أن ينجح");
    }
    assert!(matches!(
        list_tools(&state),
        Err(AppError::AuthorizationDenied)
    ));

    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "UPDATE local_sessions SET expires_at = '2030-01-01T00:00:00Z', revoked_at = ?1",
                [Utc::now().to_rfc3339()],
            )
            .expect("إلغاء الجلسة يجب أن ينجح");
    }
    assert!(matches!(
        list_tools(&state),
        Err(AppError::AuthorizationDenied)
    ));

    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute("UPDATE local_sessions SET revoked_at = NULL", [])
            .expect("إرجاع الجلسة يجب أن ينجح");
        connection
            .execute(
                "UPDATE local_users SET status = 'LOCKED' WHERE id = ?1",
                [READER],
            )
            .expect("قفل المستخدم يجب أن ينجح");
    }
    assert!(matches!(
        list_tools(&state),
        Err(AppError::AuthorizationDenied)
    ));
}

#[test]
fn create_requires_tool_create_permission() {
    let state = tool_test_state(READER);
    assert!(matches!(
        create_tool(&state, create_input("Denied probe")),
        Err(AppError::AuthorizationDenied)
    ));
}

#[test]
fn create_builds_version_one_bound_to_backend_tenant_with_contracts_and_effect() {
    let state = tool_test_state(AUTHOR);
    let result =
        create_tool(&state, create_input("Versioned probe")).expect("إنشاء الأداة يجب أن ينجح");

    assert_eq!(result.tool.tenant_id, "tenant-tool-a");
    assert_eq!(result.tool.status, crate::domain::tool::ToolStatus::Active);

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let (version_id, tenant_id, tool_id, version_number, input_json, output_json, mutation, externality, privilege, reversibility): (String, String, String, i64, String, String, String, String, String, String) = connection
        .query_row(
            "SELECT id, tenant_id, tool_id, version_number, input_contract, output_contract, mutation, externality, privilege, reversibility FROM tool_versions WHERE tool_id = ?1",
            [&result.tool.id],
            |row| {
                Ok((
                    row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?,
                ))
            },
        )
        .expect("قراءة نسخة الأداة يجب أن تنجح");
    assert_eq!(version_number, 1);
    assert_eq!(tenant_id, "tenant-tool-a");
    assert_eq!(tool_id, result.tool.id);
    assert!(!version_id.is_empty());
    // العقود ونموذج الأثر يُخزنان كما هما دون تحريف.
    assert_eq!(
        input_json,
        create_input("ignored")
            .input_contract
            .to_json()
            .expect("ترميز العقد يجب أن ينجح")
    );
    assert_eq!(
        output_json,
        create_input("ignored")
            .output_contract
            .to_json()
            .expect("ترميز العقد يجب أن ينجح")
    );
    assert_eq!(mutation, "NONE");
    assert_eq!(externality, "INTERNAL");
    assert_eq!(privilege, "STANDARD");
    assert_eq!(reversibility, "NOT_APPLICABLE");

    // حدثا التدقيق مكتوبان داخل نفس المعاملة.
    assert_eq!(count_audit(&connection, "TOOL_CREATED"), 1);
    assert_eq!(count_audit(&connection, "TOOL_VERSION_CREATED"), 1);
}

#[test]
fn create_rolls_back_tool_and_version_when_audit_fails() {
    let state = tool_test_state(AUTHOR);
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_tool_created_audit BEFORE INSERT ON audit_events
                 WHEN NEW.event_type = 'TOOL_CREATED'
                 BEGIN
                   SELECT RAISE(ABORT, 'رفض اختبار التدقيق');
                 END;",
            )
            .expect("مشغل الاختبار يجب أن ينشأ");
    }
    assert!(matches!(
        create_tool(&state, create_input("Rollback probe")),
        Err(AppError::StorageOperation)
    ));
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let tool_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM tools", [], |row| row.get(0))
        .expect("عدد الأدوات يجب أن يقرأ");
    let version_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM tool_versions", [], |row| row.get(0))
        .expect("عدد النسخ يجب أن يقرأ");
    // لا أداة جزئية بلا نسخة ولا نسخة يتيمة بلا أداة.
    assert_eq!(tool_count, 0);
    assert_eq!(version_count, 0);
}

#[test]
fn create_version_requires_tool_update_permission() {
    let state = tool_test_state(CREATOR);
    let created =
        create_tool(&state, create_input("Creator probe")).expect("إنشاء الأداة يجب أن ينجح");
    // المنشئ يملك CREATE دون UPDATE — الإنشاء لا يورث التحديث.
    assert!(matches!(
        create_tool_version(
            &state,
            CreateToolVersionInput {
                tool_id: created.tool.id,
                input_contract: contract_with_field("document_id"),
                output_contract: contract_with_field("summary"),
                effect: state_effect(),
            },
        ),
        Err(AppError::AuthorizationDenied)
    ));
}

#[test]
fn create_version_rejects_missing_cross_tenant_and_disabled_tools() {
    let state = tool_test_state(AUTHOR);
    assert!(matches!(
        create_tool_version(
            &state,
            CreateToolVersionInput {
                tool_id: "tool-missing".to_owned(),
                input_contract: contract_with_field("document_id"),
                output_contract: contract_with_field("summary"),
                effect: read_effect(),
            },
        ),
        Err(AppError::ResourceNotFound)
    ));

    let foreign_id = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        seed_foreign_tool(&connection)
    };
    assert!(matches!(
        create_tool_version(
            &state,
            CreateToolVersionInput {
                tool_id: foreign_id,
                input_contract: contract_with_field("document_id"),
                output_contract: contract_with_field("summary"),
                effect: read_effect(),
            },
        ),
        Err(AppError::ResourceNotFound)
    ));

    let created =
        create_tool(&state, create_input("Disable probe")).expect("إنشاء الأداة يجب أن ينجح");
    disable_tool(
        &state,
        ToolIdInput {
            tool_id: created.tool.id.clone(),
        },
    )
    .expect("تعطيل الأداة يجب أن ينجح");
    assert!(matches!(
        create_tool_version(
            &state,
            CreateToolVersionInput {
                tool_id: created.tool.id,
                input_contract: contract_with_field("document_id"),
                output_contract: contract_with_field("summary"),
                effect: read_effect(),
            },
        ),
        Err(AppError::InvalidStateTransition)
    ));
}

#[test]
fn create_version_increments_and_keeps_previous_version_immutable() {
    let state = tool_test_state(AUTHOR);
    let created =
        create_tool(&state, create_input("Immutability probe")).expect("إنشاء الأداة يجب أن ينجح");
    let second = create_tool_version(
        &state,
        CreateToolVersionInput {
            tool_id: created.tool.id.clone(),
            input_contract: contract_with_field("record_id"),
            output_contract: contract_with_field("digest"),
            effect: state_effect(),
        },
    )
    .expect("إنشاء النسخة الثانية يجب أن ينجح");
    assert_eq!(second.version.version_number, 2);
    assert_eq!(second.version.tool_id, created.tool.id);
    assert_eq!(second.version.tenant_id, "tenant-tool-a");

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    // النسخة الأولى بقيت كما هي حرفياً — لا تحريف للعقد القديم بعد تغير الأثر.
    let first_input: String = connection
        .query_row(
            "SELECT input_contract FROM tool_versions WHERE tool_id = ?1 AND version_number = 1",
            [&created.tool.id],
            |row| row.get(0),
        )
        .expect("قراءة النسخة الأولى يجب أن تنجح");
    assert_eq!(
        first_input,
        create_input("ignored")
            .input_contract
            .to_json()
            .expect("ترميز العقد يجب أن ينجح")
    );
    // قاعدة البيانات تمنع تعديل أي نسخة تاريخية أو حذفها.
    assert!(connection
        .execute(
            "UPDATE tool_versions SET input_contract = 'tampered' WHERE tool_id = ?1 AND version_number = 1",
            [&created.tool.id],
        )
        .is_err());
    assert!(connection
        .execute(
            "DELETE FROM tool_versions WHERE tool_id = ?1 AND version_number = 1",
            [&created.tool.id],
        )
        .is_err());
    assert_eq!(count_audit(&connection, "TOOL_VERSION_CREATED"), 2);
}

#[test]
fn create_version_rolls_back_when_audit_fails() {
    let state = tool_test_state(AUTHOR);
    let created = create_tool(&state, create_input("Version rollback probe"))
        .expect("إنشاء الأداة يجب أن ينجح");
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_tool_version_audit BEFORE INSERT ON audit_events
                 WHEN NEW.event_type = 'TOOL_VERSION_CREATED' AND NEW.resource_id IN (SELECT id FROM tool_versions WHERE version_number > 1)
                 BEGIN
                   SELECT RAISE(ABORT, 'رفض اختبار التدقيق');
                 END;",
            )
            .expect("مشغل الاختبار يجب أن ينشأ");
    }
    assert!(matches!(
        create_tool_version(
            &state,
            CreateToolVersionInput {
                tool_id: created.tool.id.clone(),
                input_contract: contract_with_field("record_id"),
                output_contract: contract_with_field("digest"),
                effect: state_effect(),
            },
        ),
        Err(AppError::StorageOperation)
    ));
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let version_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM tool_versions WHERE tool_id = ?1",
            [&created.tool.id],
            |row| row.get(0),
        )
        .expect("عدد النسخ يجب أن يقرأ");
    assert_eq!(version_count, 1);
}

#[test]
fn disable_requires_update_permission_and_rejects_missing_cross_tenant_and_repeat() {
    let state = tool_test_state(READER);
    assert!(matches!(
        disable_tool(
            &state,
            ToolIdInput {
                tool_id: "tool-any".to_owned(),
            },
        ),
        Err(AppError::AuthorizationDenied)
    ));

    state.set_active_user_for_tests(AUTHOR);
    assert!(matches!(
        disable_tool(
            &state,
            ToolIdInput {
                tool_id: "tool-missing".to_owned(),
            },
        ),
        Err(AppError::ResourceNotFound)
    ));

    let foreign_id = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        seed_foreign_tool(&connection)
    };
    assert!(matches!(
        disable_tool(
            &state,
            ToolIdInput {
                tool_id: foreign_id,
            },
        ),
        Err(AppError::ResourceNotFound)
    ));

    let created =
        create_tool(&state, create_input("Lifecycle probe")).expect("إنشاء الأداة يجب أن ينجح");
    let disabled = disable_tool(
        &state,
        ToolIdInput {
            tool_id: created.tool.id.clone(),
        },
    )
    .expect("التعطيل يجب أن ينجح");
    assert_eq!(
        disabled.tool.status,
        crate::domain::tool::ToolStatus::Disabled
    );
    // العقد الحالي الصريح: التعطيل مرة واحدة — إعادة التعطيل مرفوضة كانتقال حالة غير صالح.
    assert!(matches!(
        disable_tool(
            &state,
            ToolIdInput {
                tool_id: created.tool.id,
            },
        ),
        Err(AppError::InvalidStateTransition)
    ));

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    assert_eq!(count_audit(&connection, "TOOL_DISABLED"), 1);
    // النسخ التاريخية تبقى بعد التعطيل.
    let version_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM tool_versions WHERE tool_id = ?1",
            [&disabled.tool.id],
            |row| row.get(0),
        )
        .expect("عدد النسخ يجب أن يقرأ");
    assert_eq!(version_count, 1);
}

#[test]
fn disable_rolls_back_when_audit_fails() {
    let state = tool_test_state(AUTHOR);
    let created = create_tool(&state, create_input("Disable rollback probe"))
        .expect("إنشاء الأداة يجب أن ينجح");
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_tool_disabled_audit BEFORE INSERT ON audit_events
                 WHEN NEW.event_type = 'TOOL_DISABLED'
                 BEGIN
                   SELECT RAISE(ABORT, 'رفض اختبار التدقيق');
                 END;",
            )
            .expect("مشغل الاختبار يجب أن ينشأ");
    }
    assert!(matches!(
        disable_tool(
            &state,
            ToolIdInput {
                tool_id: created.tool.id.clone(),
            },
        ),
        Err(AppError::StorageOperation)
    ));
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let status: String = connection
        .query_row(
            "SELECT status FROM tools WHERE id = ?1",
            [&created.tool.id],
            |row| row.get(0),
        )
        .expect("قراءة الحالة يجب أن تنجح");
    assert_eq!(status, "ACTIVE");
}

#[test]
fn registry_catalog_has_no_execute_action_and_grants_stay_separated() {
    let state = tool_test_state(AUTHOR);
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    // كتالوج صلاحيات السجل يقتصر على قراءة/إنشاء/تحديث — لا وجود لأي TOOL/EXECUTE.
    let execute_actions: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM permissions WHERE resource_type = 'TOOL' AND action = 'EXECUTE'",
            [],
            |row| row.get(0),
        )
        .expect("استعلام الكتالوج يجب أن ينجح");
    assert_eq!(execute_actions, 0);
    drop(connection);

    // المنح لا تتوراث: القراءة لا تعني الإنشاء، والإنشاء لا يعني التحديث.
    assert!(has_permission(
        &state,
        "tenant-tool-a",
        READER,
        "TOOL",
        "READ"
    ));
    assert!(!has_permission(
        &state,
        "tenant-tool-a",
        READER,
        "TOOL",
        "CREATE"
    ));
    assert!(!has_permission(
        &state,
        "tenant-tool-a",
        READER,
        "TOOL",
        "UPDATE"
    ));
    assert!(has_permission(
        &state,
        "tenant-tool-a",
        CREATOR,
        "TOOL",
        "CREATE"
    ));
    assert!(!has_permission(
        &state,
        "tenant-tool-a",
        CREATOR,
        "TOOL",
        "UPDATE"
    ));

    // صلاحيات السجل لا تخلق أي سلطة تشغيل: أوسع منح سجل لا يمنح AGENT/EXECUTE.
    assert!(!has_permission(
        &state,
        "tenant-tool-a",
        AUTHOR,
        "AGENT",
        "EXECUTE"
    ));
    // صلاحيات السجل معزولة بالمستأجر — لا إعادة استخدام عبر المستأجر الآخر.
    assert!(!has_permission(
        &state,
        "tenant-tool-b",
        AUTHOR,
        "TOOL",
        "READ"
    ));
}
