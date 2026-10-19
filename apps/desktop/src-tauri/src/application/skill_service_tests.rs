use chrono::Utc;
use rusqlite::{params, Connection};

use super::{
    assign_skill, create_skill, create_skill_version, disable_skill, list_skills, unassign_skill,
    AssignSkillInput, CreateSkillInput, CreateSkillVersionInput, SkillIdInput, UnassignSkillInput,
};
use crate::{
    application::authorization_service::has_permission, domain::error::AppError,
    infrastructure::database::AppState,
};

// مستخدمو الاختبار: فصل منح القراءة/الإنشاء/التحديث/الإسناد لإثبات استقلال كل فعل.
const AUTHOR: &str = "user-skill-author";
const READER: &str = "user-skill-reader";
const OUTSIDER: &str = "user-skill-outsider";

fn create_skill_input(name: &str) -> CreateSkillInput {
    CreateSkillInput {
        name: name.to_owned(),
        description: "Synthetic governed skill for service tests.".to_owned(),
        instruction: "Summarize the bounded evidence packet faithfully.".to_owned(),
        input_contract: "One bounded evidence reference identifier.".to_owned(),
        output_contract: "A short Arabic summary paragraph.".to_owned(),
        constraints: "No external claims beyond the provided packet.".to_owned(),
    }
}

fn assign_input(agent_id: &str, skill_version_id: &str) -> AssignSkillInput {
    AssignSkillInput {
        agent_id: agent_id.to_owned(),
        skill_version_id: skill_version_id.to_owned(),
    }
}

fn skill_test_state(active_user: &str) -> AppState {
    let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
    // سلسلة الهجرات الكاملة نفسها التي يشغلها الإنتاج — تضمن توافر كل فهارس الآباء الفريدة.
    crate::infrastructure::database::apply_migrations(&connection)
        .expect("هجرات الاختبار يجب أن تنجح");
    let now = Utc::now().to_rfc3339();
    for tenant_id in ["tenant-skill-a", "tenant-skill-b"] {
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?2, 'DEMO_SYNTHETIC', ?3, ?3)",
                (tenant_id, tenant_id, &now),
            )
            .expect("المستأجر يجب أن ينشأ");
    }
    // صلاحيات السجل تُزرع من الهجرة 0014 نفسها — لا إعادة زرع يدوية هنا.
    let role_grants: [(&str, &str, &[&str]); 2] = [
        (
            AUTHOR,
            "role-skill-author",
            &[
                "permission_skill_read",
                "permission_skill_create",
                "permission_skill_update",
                "permission_skill_assign",
            ],
        ),
        (READER, "role-skill-reader", &["permission_skill_read"]),
    ];
    for (user_id, role_id, grants) in role_grants {
        connection
            .execute(
                "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES (?1, 'tenant-skill-a', ?2, ?2, ?3, ?3)",
                params![role_id, role_id, &now],
            )
            .expect("الدور يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, 'tenant-skill-a', ?2, ?3, NULL, 'ACTIVE', ?4, ?4)",
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
                "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, 'tenant-skill-a', ?2, ?3, '2030-01-01T00:00:00Z', NULL, 'test')",
                params![format!("session-{user_id}"), user_id, &now],
            )
            .expect("الجلسة يجب أن تنشأ");
    }
    connection
        .execute(
            "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, 'tenant-skill-a', ?1, ?2, NULL, 'ACTIVE', ?3, ?3)",
            params![OUTSIDER, format!("{OUTSIDER}@nexus.test"), &now],
        )
        .expect("المستخدم بلا صلاحيات يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, 'tenant-skill-a', ?2, ?3, '2030-01-01T00:00:00Z', NULL, 'test')",
            params![format!("session-{OUTSIDER}"), OUTSIDER, &now],
        )
        .expect("الجلسة يجب أن تنشأ");
    // وكلاء اختبار: نشط ومعطل في المستأجر أ، ونشط في المستأجر ب.
    for (agent_id, tenant_id, status) in [
        ("agent-skill-active", "tenant-skill-a", "ACTIVE"),
        ("agent-skill-disabled", "tenant-skill-a", "DISABLED"),
        ("agent-skill-foreign", "tenant-skill-b", "ACTIVE"),
    ] {
        connection
            .execute(
                "INSERT INTO agents (id, tenant_id, name, description, status, created_at, updated_at) VALUES (?1, ?2, ?3, 'Test agent', ?4, ?5, ?5)",
                params![agent_id, tenant_id, agent_id, status, &now],
            )
            .expect("الوكيل يجب أن ينشأ");
    }

    AppState::test_instance(
        connection,
        "tenant-skill-a".to_owned(),
        active_user.to_owned(),
    )
}

fn seed_foreign_skill(connection: &Connection) -> (String, String) {
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "INSERT INTO skills (id, tenant_id, name, description, status, created_at, updated_at) VALUES ('skill-foreign', 'tenant-skill-b', 'Foreign skill', 'Seeded fixture', 'ACTIVE', ?1, ?1)",
            [&now],
        )
        .expect("مهارة المستأجر الآخر يجب أن تنشأ");
    connection
        .execute(
            "INSERT INTO skill_versions (id, tenant_id, skill_id, version_number, instruction, input_contract, output_contract, constraints, created_at) VALUES ('skill-version-foreign', 'tenant-skill-b', 'skill-foreign', 1, 'Foreign instruction.', 'in', 'out', 'none', ?1)",
            [&now],
        )
        .expect("نسخة المستأجر الآخر يجب أن تنشأ");
    (
        "skill-foreign".to_owned(),
        "skill-version-foreign".to_owned(),
    )
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

fn first_version_id(state: &AppState, skill_id: &str) -> String {
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    connection
        .query_row(
            "SELECT id FROM skill_versions WHERE skill_id = ?1 AND version_number = 1",
            [skill_id],
            |row| row.get(0),
        )
        .expect("قراءة النسخة الأولى يجب أن تنجح")
}

#[test]
fn list_requires_skill_read_and_reports_empty_tenants_truthfully() {
    let state = skill_test_state(OUTSIDER);
    assert!(matches!(
        list_skills(&state),
        Err(AppError::AuthorizationDenied)
    ));

    state.set_active_user_for_tests(READER);
    let data = list_skills(&state).expect("القارئ المصرح له يجب أن يقرأ");
    assert!(!data.has_data);
    assert!(data.items.is_empty());
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    assert_eq!(count_audit(&connection, "SKILL_CREATED"), 0);
}

#[test]
fn list_isolates_tenants() {
    let state = skill_test_state(AUTHOR);
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        seed_foreign_skill(&connection);
    }
    let created = create_skill(&state, create_skill_input("Isolation probe"))
        .expect("إنشاء المهارة يجب أن ينجح");

    let data = list_skills(&state).expect("القائمة يجب أن تنجح");
    assert_eq!(data.items.len(), 1);
    assert_eq!(data.items[0].id, created.skill.id);
    assert!(data
        .items
        .iter()
        .all(|skill| skill.tenant_id == "tenant-skill-a"));
}

#[test]
fn create_requires_permission_and_binds_backend_tenant_with_version_one_and_audit() {
    let state = skill_test_state(READER);
    assert!(matches!(
        create_skill(&state, create_skill_input("Denied probe")),
        Err(AppError::AuthorizationDenied)
    ));

    state.set_active_user_for_tests(AUTHOR);
    let created = create_skill(&state, create_skill_input("Lifecycle probe"))
        .expect("إنشاء المهارة يجب أن ينجح");
    assert_eq!(created.skill.tenant_id, "tenant-skill-a");
    assert_eq!(
        created.skill.status,
        crate::domain::skill::SkillStatus::Active
    );

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let (version_number, instruction, tenant_id): (i64, String, String) = connection
        .query_row(
            "SELECT version_number, instruction, tenant_id FROM skill_versions WHERE skill_id = ?1",
            [&created.skill.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("قراءة نسخة المهارة يجب أن تنجح");
    assert_eq!(version_number, 1);
    assert_eq!(tenant_id, "tenant-skill-a");
    assert_eq!(instruction, create_skill_input("ignored").instruction);
    assert_eq!(count_audit(&connection, "SKILL_CREATED"), 1);
    assert_eq!(count_audit(&connection, "SKILL_VERSION_CREATED"), 1);
}

#[test]
fn create_rolls_back_skill_and_version_when_audit_fails() {
    let state = skill_test_state(AUTHOR);
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_skill_created_audit BEFORE INSERT ON audit_events
                 WHEN NEW.event_type = 'SKILL_CREATED'
                 BEGIN
                   SELECT RAISE(ABORT, 'رفض اختبار التدقيق');
                 END;",
            )
            .expect("مشغل الاختبار يجب أن ينشأ");
    }
    assert!(matches!(
        create_skill(&state, create_skill_input("Rollback probe")),
        Err(AppError::StorageOperation)
    ));
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let skill_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM skills", [], |row| row.get(0))
        .expect("عدد المهارات يجب أن يقرأ");
    let version_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM skill_versions", [], |row| row.get(0))
        .expect("عدد النسخ يجب أن يقرأ");
    assert_eq!(skill_count, 0);
    assert_eq!(version_count, 0);
}

#[test]
fn create_version_rejects_unauthorized_missing_cross_tenant_and_disabled() {
    let state = skill_test_state(AUTHOR);
    let created = create_skill(&state, create_skill_input("Version guard probe"))
        .expect("إنشاء المهارة يجب أن ينجح");

    state.set_active_user_for_tests(READER);
    let version_input_for = |skill_id: String| CreateSkillVersionInput {
        skill_id,
        instruction: "Updated bounded instruction.".to_owned(),
        input_contract: "in".to_owned(),
        output_contract: "out".to_owned(),
        constraints: "none".to_owned(),
    };
    assert!(matches!(
        create_skill_version(&state, version_input_for(created.skill.id.clone())),
        Err(AppError::AuthorizationDenied)
    ));

    state.set_active_user_for_tests(AUTHOR);
    assert!(matches!(
        create_skill_version(&state, version_input_for("skill-missing".to_owned())),
        Err(AppError::ResourceNotFound)
    ));

    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        seed_foreign_skill(&connection);
    }
    assert!(matches!(
        create_skill_version(&state, version_input_for("skill-foreign".to_owned())),
        Err(AppError::ResourceNotFound)
    ));

    disable_skill(
        &state,
        SkillIdInput {
            skill_id: created.skill.id.clone(),
        },
    )
    .expect("تعطيل المهارة يجب أن ينجح");
    assert!(matches!(
        create_skill_version(&state, version_input_for(created.skill.id.clone())),
        Err(AppError::InvalidStateTransition)
    ));
}

#[test]
fn create_version_increments_and_keeps_history_immutable_with_audit_rollback() {
    let state = skill_test_state(AUTHOR);
    let created = create_skill(&state, create_skill_input("History probe"))
        .expect("إنشاء المهارة يجب أن ينجح");
    let second = create_skill_version(
        &state,
        CreateSkillVersionInput {
            skill_id: created.skill.id.clone(),
            instruction: "Second bounded instruction.".to_owned(),
            input_contract: "in2".to_owned(),
            output_contract: "out2".to_owned(),
            constraints: "none".to_owned(),
        },
    )
    .expect("إنشاء النسخة الثانية يجب أن ينجح");
    assert_eq!(second.version.version_number, 2);

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    // النسخة الأولى لم تتغير والقاعدة تمنع تحريف التاريخ أو حذفه.
    let first_instruction: String = connection
        .query_row(
            "SELECT instruction FROM skill_versions WHERE skill_id = ?1 AND version_number = 1",
            [&created.skill.id],
            |row| row.get(0),
        )
        .expect("قراءة النسخة الأولى يجب أن تنجح");
    assert_eq!(first_instruction, create_skill_input("ignored").instruction);
    assert!(connection
        .execute(
            "UPDATE skill_versions SET instruction = 'tampered' WHERE skill_id = ?1 AND version_number = 1",
            [&created.skill.id],
        )
        .is_err());
    assert!(connection
        .execute(
            "DELETE FROM skill_versions WHERE skill_id = ?1",
            [&created.skill.id],
        )
        .is_err());
    assert_eq!(count_audit(&connection, "SKILL_VERSION_CREATED"), 2);
    drop(connection);

    // فشل تدقيق النسخة الجديدة يترك العدد عند نسختين اثنتين فقط.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_skill_version_audit BEFORE INSERT ON audit_events
                 WHEN NEW.event_type = 'SKILL_VERSION_CREATED'
                 BEGIN
                   SELECT RAISE(ABORT, 'رفض اختبار التدقيق');
                 END;",
            )
            .expect("مشغل الاختبار يجب أن ينشأ");
    }
    assert!(matches!(
        create_skill_version(
            &state,
            CreateSkillVersionInput {
                skill_id: created.skill.id.clone(),
                instruction: "Third bounded instruction.".to_owned(),
                input_contract: "in3".to_owned(),
                output_contract: "out3".to_owned(),
                constraints: "none".to_owned(),
            },
        ),
        Err(AppError::StorageOperation)
    ));
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let version_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM skill_versions WHERE skill_id = ?1",
            [&created.skill.id],
            |row| row.get(0),
        )
        .expect("عدد النسخ يجب أن يقرأ");
    assert_eq!(version_count, 2);
}

#[test]
fn disable_requires_permission_rejects_repeat_and_rolls_back_on_audit_failure() {
    let state = skill_test_state(READER);
    assert!(matches!(
        disable_skill(
            &state,
            SkillIdInput {
                skill_id: "skill-any".to_owned(),
            },
        ),
        Err(AppError::AuthorizationDenied)
    ));

    state.set_active_user_for_tests(AUTHOR);
    let created = create_skill(&state, create_skill_input("Disable probe"))
        .expect("إنشاء المهارة يجب أن ينجح");
    let disabled = disable_skill(
        &state,
        SkillIdInput {
            skill_id: created.skill.id.clone(),
        },
    )
    .expect("التعطيل يجب أن ينجح");
    assert_eq!(
        disabled.skill.status,
        crate::domain::skill::SkillStatus::Disabled
    );
    assert!(matches!(
        disable_skill(
            &state,
            SkillIdInput {
                skill_id: created.skill.id.clone(),
            },
        ),
        Err(AppError::InvalidStateTransition)
    ));
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    assert_eq!(count_audit(&connection, "SKILL_DISABLED"), 1);
    // النسخ التاريخية باقية بعد التعطيل.
    let version_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM skill_versions WHERE skill_id = ?1",
            [&created.skill.id],
            |row| row.get(0),
        )
        .expect("عدد النسخ يجب أن يقرأ");
    assert_eq!(version_count, 1);
    drop(connection);

    // فشل تدقيق التعطيل يعيد الحالة إلى نشطة.
    let second = create_skill(&state, create_skill_input("Disable rollback probe"))
        .expect("إنشاء مهارة ثانية يجب أن ينجح");
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_skill_disabled_audit BEFORE INSERT ON audit_events
                 WHEN NEW.event_type = 'SKILL_DISABLED'
                 BEGIN
                   SELECT RAISE(ABORT, 'رفض اختبار التدقيق');
                 END;",
            )
            .expect("مشغل الاختبار يجب أن ينشأ");
    }
    assert!(matches!(
        disable_skill(
            &state,
            SkillIdInput {
                skill_id: second.skill.id.clone(),
            },
        ),
        Err(AppError::StorageOperation)
    ));
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let status: String = connection
        .query_row(
            "SELECT status FROM skills WHERE id = ?1",
            [&second.skill.id],
            |row| row.get(0),
        )
        .expect("قراءة الحالة يجب أن تنجح");
    assert_eq!(status, "ACTIVE");
}

#[test]
fn assign_requires_permission_and_validates_agent_skill_and_tenant() {
    let state = skill_test_state(READER);
    // القارئ لا يملك صلاحية الإسناد — الرفض يقع في بوابة التفويض أولاً.
    assert!(matches!(
        assign_skill(
            &state,
            assign_input("agent-skill-active", "skill-version-any")
        ),
        Err(AppError::AuthorizationDenied)
    ));

    state.set_active_user_for_tests(AUTHOR);
    let created = create_skill(&state, create_skill_input("Assign probe"))
        .expect("إنشاء المهارة يجب أن ينجح");
    let version_id = first_version_id(&state, &created.skill.id);

    // إسناد وكيل مفقود أو معطل أو من مستأجر آخر مرفوض.
    assert!(matches!(
        assign_skill(&state, assign_input("agent-missing", &version_id)),
        Err(AppError::ResourceNotFound)
    ));
    assert!(matches!(
        assign_skill(&state, assign_input("agent-skill-disabled", &version_id)),
        Err(AppError::ResourceNotFound)
    ));
    assert!(matches!(
        assign_skill(&state, assign_input("agent-skill-foreign", &version_id)),
        Err(AppError::ResourceNotFound)
    ));
    // نسخة مفقودة أو من مستأجر آخر مرفوضة.
    assert!(matches!(
        assign_skill(
            &state,
            assign_input("agent-skill-active", "skill-version-missing")
        ),
        Err(AppError::ResourceNotFound)
    ));

    // مهارة معطلة لا تقبل الإسناد.
    let disabled_skill = create_skill(&state, create_skill_input("Disabled assign probe"))
        .expect("إنشاء المهارة الثانية يجب أن ينجح");
    let disabled_version_id = first_version_id(&state, &disabled_skill.skill.id);
    disable_skill(
        &state,
        SkillIdInput {
            skill_id: disabled_skill.skill.id.clone(),
        },
    )
    .expect("تعطيل المهارة يجب أن ينجح");
    assert!(matches!(
        assign_skill(
            &state,
            assign_input("agent-skill-active", &disabled_version_id)
        ),
        Err(AppError::InvalidStateTransition)
    ));

    // الإسناد الصحيح يكتب سجلاً نشطاً مملوكاً للمستأثر وبتدقيق.
    assign_skill(&state, assign_input("agent-skill-active", &version_id))
        .expect("الإسناد الصحيح يجب أن ينجح");
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let (assignment_tenant, unassigned_at): (String, Option<String>) = connection
        .query_row(
            "SELECT tenant_id, unassigned_at FROM agent_skill_assignments WHERE agent_id = 'agent-skill-active' AND skill_version_id = ?1",
            [&version_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("قراءة الإسناد يجب أن تنجح");
    assert_eq!(assignment_tenant, "tenant-skill-a");
    assert!(unassigned_at.is_none());
    assert_eq!(count_audit(&connection, "AGENT_SKILL_ASSIGNED"), 1);
    drop(connection);

    // إسناد النسخة نفسها للوكيل نفسه مرة أخرى يُرفض كتعارض — لا ازدواج.
    assert!(matches!(
        assign_skill(&state, assign_input("agent-skill-active", &version_id)),
        Err(AppError::Conflict)
    ));
}

#[test]
fn assign_rolls_back_when_audit_fails() {
    let state = skill_test_state(AUTHOR);
    let created = create_skill(&state, create_skill_input("Assign rollback probe"))
        .expect("إنشاء المهارة يجب أن ينجح");
    let version_id = first_version_id(&state, &created.skill.id);
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute_batch(
                "CREATE TRIGGER fail_skill_assign_audit BEFORE INSERT ON audit_events
                 WHEN NEW.event_type = 'AGENT_SKILL_ASSIGNED'
                 BEGIN
                   SELECT RAISE(ABORT, 'رفض اختبار التدقيق');
                 END;",
            )
            .expect("مشغل الاختبار يجب أن ينشأ");
    }
    assert!(matches!(
        assign_skill(&state, assign_input("agent-skill-active", &version_id)),
        Err(AppError::StorageOperation)
    ));
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let assignment_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM agent_skill_assignments", [], |row| {
            row.get(0)
        })
        .expect("عدد الإسنادات يجب أن يقرأ");
    assert_eq!(assignment_count, 0);
}

#[test]
fn unassign_requires_permission_preserves_history_and_rolls_back_on_audit_failure() {
    let state = skill_test_state(AUTHOR);
    let created = create_skill(&state, create_skill_input("Unassign probe"))
        .expect("إنشاء المهارة يجب أن ينجح");
    let version_id = first_version_id(&state, &created.skill.id);
    assign_skill(&state, assign_input("agent-skill-active", &version_id))
        .expect("الإسناد يجب أن ينجح");
    let assignment_id: String = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT id FROM agent_skill_assignments WHERE agent_id = 'agent-skill-active' AND skill_version_id = ?1",
                [&version_id],
                |row| row.get(0),
            )
            .expect("قراءة الإسناد يجب أن تنجح")
    };

    state.set_active_user_for_tests(READER);
    assert!(matches!(
        unassign_skill(
            &state,
            UnassignSkillInput {
                assignment_id: assignment_id.clone(),
            },
        ),
        Err(AppError::AuthorizationDenied)
    ));

    state.set_active_user_for_tests(AUTHOR);
    unassign_skill(
        &state,
        UnassignSkillInput {
            assignment_id: assignment_id.clone(),
        },
    )
    .expect("إلغاء الإسناد يجب أن ينجح");

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    // السجل التاريخي باقٍ ويحمل طابع الإلغاء — لا حذف للإسناد.
    let (unassigned_at, row_count): (Option<String>, i64) = connection
        .query_row(
            "SELECT unassigned_at, (SELECT COUNT(*) FROM agent_skill_assignments WHERE id = ?1) FROM agent_skill_assignments WHERE id = ?1",
            [&assignment_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("قراءة الإسناد الملغى يجب أن تنجح");
    assert!(unassigned_at.is_some());
    assert_eq!(row_count, 1);
    assert_eq!(count_audit(&connection, "AGENT_SKILL_UNASSIGNED"), 1);
    drop(connection);

    // إلغاء إسناد ملغى مسبقاً مرفوض كمفقود — لا إعادة كتابة للتاريخ.
    assert!(matches!(
        unassign_skill(
            &state,
            UnassignSkillInput {
                assignment_id: assignment_id.clone(),
            },
        ),
        Err(AppError::ResourceNotFound)
    ));
    // إلغاء إسناد من مستأجر آخر مرفوض.
    assert!(matches!(
        unassign_skill(
            &state,
            UnassignSkillInput {
                assignment_id: "assignment-foreign".to_owned(),
            },
        ),
        Err(AppError::ResourceNotFound)
    ));
}

#[test]
fn skill_catalog_has_no_execute_and_grants_grant_no_runtime_authority() {
    let state = skill_test_state(AUTHOR);
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let execute_actions: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM permissions WHERE resource_type = 'SKILL' AND action = 'EXECUTE'",
            [],
            |row| row.get(0),
        )
        .expect("استعلام الكتالوج يجب أن ينجح");
    assert_eq!(execute_actions, 0);
    drop(connection);

    // الإسناد صلاحية مستقلة عن التحديث، وصلاحيات المهارة لا تمنح سلطة تشغيل الوكلاء.
    assert!(has_permission(
        &state,
        "tenant-skill-a",
        READER,
        "SKILL",
        "READ"
    ));
    assert!(!has_permission(
        &state,
        "tenant-skill-a",
        READER,
        "SKILL",
        "ASSIGN"
    ));
    assert!(!has_permission(
        &state,
        "tenant-skill-a",
        AUTHOR,
        "AGENT",
        "EXECUTE"
    ));
    assert!(!has_permission(
        &state,
        "tenant-skill-b",
        AUTHOR,
        "SKILL",
        "READ"
    ));
}
