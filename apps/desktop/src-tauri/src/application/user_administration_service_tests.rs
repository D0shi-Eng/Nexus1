use rusqlite::{params, Connection};

use super::{
    assign_user_role, create_workspace_user, list_workspace_users, revoke_user_role,
    revoke_user_sessions, set_user_status, CreateWorkspaceUserInput, RevokeUserSessionsInput,
    SetUserStatusInput, UserRoleInput,
};
use crate::{
    application::authentication_service::{
        bootstrap_production, login, BootstrapProductionInput, LoginInput,
    },
    application::authorization_service::require_permission,
    domain::error::AppError,
    domain::models::RuntimeMode,
    infrastructure::database::{apply_migrations, ensure_permission_catalog, AppState},
};

// بيئة إنتاج مبذرة + مستخدم ثانٍ داخل المستأجر الإنتاجي نفسه.
fn production_admin_state() -> AppState {
    let connection = Connection::open_in_memory().expect("قاعدة الاختبار يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("مفاتيح الأجنبة يجب أن تعمل");
    apply_migrations(&connection).expect("الهجرات يجب أن تنجح");
    ensure_permission_catalog(&connection).expect("الكتالوج يجب أن ينجح");
    let state = AppState::test_instance(connection, "", "");
    state
        .set_runtime_mode(RuntimeMode::Unprovisioned)
        .expect("ضبط الوضع يجب أن ينجح");
    bootstrap_production(
        &state,
        BootstrapProductionInput {
            organization_name: "Admin Test Org".to_owned(),
            // GAP-05-OPEN: بيانات المؤسسة جزء من عقد الإقلاع لا ثوابت في الخلف.
            country_code: "SA".to_owned(),
            industry: "Technology Services".to_owned(),
            admin_display_name: "Primary Admin".to_owned(),
            admin_email: "admin@users.test".to_owned(),
            password: "Str0ng!Passw0rd#2026".to_owned(),
        },
    )
    .expect("الإقلاع يجب أن ينجح");

    // دور قارئ داخل المستأجر + مستخدم عضو بجلسة نشطة.
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let now = chrono::Utc::now().to_rfc3339();
    let tenant_id = state.tenant_id();
    connection
        .execute(
            "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role_prod_reader', ?1, 'COMPLIANCE_READER', 'Compliance Reader', ?2, ?2)",
            params![tenant_id, now],
        )
        .expect("دور القارئ يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO role_permissions (role_id, permission_id) SELECT 'role_prod_reader', id FROM permissions WHERE code = 'obligation:read'",
            [],
        )
        .expect("منح القارئ يجب أن تنشأ");
    connection
        .execute(
            "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, failed_attempts, created_at, updated_at) VALUES ('user_prod_member', ?1, 'Member User', 'member@users.test', NULL, 'ACTIVE', 0, ?2, ?2)",
            params![tenant_id, now],
        )
        .expect("العضو يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session_member', ?1, 'user_prod_member', ?2, '2030-01-01T00:00:00Z', NULL, 'test')",
            params![tenant_id, now],
        )
        .expect("جلسة العضو يجب أن تنشأ");
    drop(connection);
    state
}

#[test]
fn lists_users_with_roles_and_available_catalog() {
    let state = production_admin_state();

    let data = list_workspace_users(&state).expect("القائمة يجب أن تُقرأ");
    assert_eq!(data.users.len(), 2);
    let member = data
        .users
        .iter()
        .find(|user| user.user_id == "user_prod_member")
        .expect("العضو يجب أن يظهر");
    assert_eq!(member.status, "ACTIVE");
    assert!(member.roles.is_empty());
    assert!(data.available_roles.contains(&"TENANT_ADMIN".to_owned()));
    assert!(data
        .available_roles
        .contains(&"COMPLIANCE_READER".to_owned()));
}

#[test]
fn self_modification_is_forbidden_for_status_and_roles() {
    let state = production_admin_state();
    let admin_id = state.current_user_id().expect("هوية المسؤول");

    assert!(matches!(
        set_user_status(
            &state,
            SetUserStatusInput {
                user_id: admin_id.clone(),
                status: "DISABLED".to_owned(),
            },
        ),
        Err(AppError::SelfModificationForbidden)
    ));
    assert!(matches!(
        assign_user_role(
            &state,
            UserRoleInput {
                user_id: admin_id.clone(),
                role_code: "COMPLIANCE_READER".to_owned(),
            },
        ),
        Err(AppError::SelfModificationForbidden)
    ));
    assert!(matches!(
        revoke_user_role(
            &state,
            UserRoleInput {
                user_id: admin_id,
                role_code: "TENANT_ADMIN".to_owned(),
            },
        ),
        Err(AppError::SelfModificationForbidden)
    ));
}

#[test]
fn assign_and_revoke_role_changes_live_permissions_and_audits() {
    let state = production_admin_state();

    assign_user_role(
        &state,
        UserRoleInput {
            user_id: "user_prod_member".to_owned(),
            role_code: "COMPLIANCE_READER".to_owned(),
        },
    )
    .expect("الإسناد يجب أن ينجح");

    // العضو لديه جلسة صالحة — الصلاحية تسري فوراً على هويته.
    {
        let member_session = {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            crate::infrastructure::database::resolve_active_session(
                &connection,
                &state.tenant_id(),
                "user_prod_member",
                &chrono::Utc::now().to_rfc3339(),
            )
            .expect("جلسة العضو يجب أن تُحل")
        };
        state
            .activate_identity(&state.tenant_id(), "user_prod_member", Some(member_session))
            .expect("تفعيل هوية العضو يجب أن ينجح");
        assert!(require_permission(&state, "OBLIGATION", "READ").is_ok());
        assert!(require_permission(&state, "USER", "UPDATE").is_err());

        // العضو ليس مسؤولاً: لا يستطيع إدارة المستخدمين أصلاً.
        assert!(matches!(
            list_workspace_users(&state),
            Err(AppError::AuthorizationDenied)
        ));
    }

    // عودة المسؤول لنزع الدور — الصلاحية تسقط فوراً.
    let admin_id = login_admin_back(&state);
    let _ = admin_id;
    revoke_user_role(
        &state,
        UserRoleInput {
            user_id: "user_prod_member".to_owned(),
            role_code: "COMPLIANCE_READER".to_owned(),
        },
    )
    .expect("النزع يجب أن ينجح");

    let assigned: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row("SELECT COUNT(*) FROM audit_events WHERE event_type IN ('USER_ROLE_ASSIGNED','USER_ROLE_REVOKED')", [], |row| row.get(0))
        .expect("التدقيق يجب أن يقرأ");
    assert_eq!(assigned, 2);
}

#[test]
fn disabling_user_revokes_sessions_and_blocks_login_immediately() {
    let state = production_admin_state();

    set_user_status(
        &state,
        SetUserStatusInput {
            user_id: "user_prod_member".to_owned(),
            status: "DISABLED".to_owned(),
        },
    )
    .expect("التعطيل يجب أن ينجح");

    let revoked: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row("SELECT COUNT(*) FROM local_sessions WHERE user_id = 'user_prod_member' AND revoked_at IS NOT NULL", [], |row| row.get(0))
        .expect("الجلسات يجب أن تُحصى");
    assert_eq!(revoked, 1);

    // إعادة التفعيل تفتح الحساب من جديد.
    set_user_status(
        &state,
        SetUserStatusInput {
            user_id: "user_prod_member".to_owned(),
            status: "ACTIVE".to_owned(),
        },
    )
    .expect("التنشيط يجب أن ينجح");

    let status_events: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE event_type = 'USER_STATUS_CHANGED'",
            [],
            |row| row.get(0),
        )
        .expect("التدقيق يجب أن يقرأ");
    assert_eq!(status_events, 2);

    let mut unknown = SetUserStatusInput {
        user_id: "user_missing".to_owned(),
        status: "ACTIVE".to_owned(),
    };
    unknown.status = "HACKED".to_owned();
    assert!(matches!(
        set_user_status(&state, unknown),
        Err(AppError::ValidationFailed)
    ));
    assert!(matches!(
        set_user_status(
            &state,
            SetUserStatusInput {
                user_id: "user_missing".to_owned(),
                status: "ACTIVE".to_owned(),
            },
        ),
        Err(AppError::ResourceNotFound)
    ));
}

#[test]
fn role_mutation_stays_inside_tenant_catalog() {
    let state = production_admin_state();

    // دور من مستأجر آخر غير موجود هنا — رفض تحقق لا استثناء.
    assert!(matches!(
        assign_user_role(
            &state,
            UserRoleInput {
                user_id: "user_prod_member".to_owned(),
                role_code: "GHOST_ROLE".to_owned(),
            },
        ),
        Err(AppError::ValidationFailed)
    ));
    // مستخدم من مستأجر آخر — غير موجود ضمن النطاق.
    assert!(matches!(
        assign_user_role(
            &state,
            UserRoleInput {
                user_id: "user_demo_operator".to_owned(),
                role_code: "TENANT_ADMIN".to_owned(),
            },
        ),
        Err(AppError::ResourceNotFound)
    ));
}

fn login_admin_back(state: &AppState) -> String {
    let session = login(
        state,
        LoginInput {
            email: "admin@users.test".to_owned(),
            password: "Str0ng!Passw0rd#2026".to_owned(),
        },
    )
    .expect("عودة المسؤول يجب أن تنجح");
    session.context.actor_id
}

// =====================================================================
// 0026 — إنشاء المستخدمين الإنتاجيين (UAT-01)
// =====================================================================

fn create_input(display_name: &str, email: &str, roles: &[&str]) -> CreateWorkspaceUserInput {
    CreateWorkspaceUserInput {
        display_name: display_name.to_owned(),
        email: email.to_owned(),
        initial_password: "Memb3r!Passw0rd#2026".to_owned(),
        role_codes: roles.iter().map(|role| (*role).to_owned()).collect(),
    }
}

#[test]
fn creates_contributor_and_grants_its_catalogue_with_audit() {
    let state = production_admin_state();

    let created = create_workspace_user(
        &state,
        create_input(
            "Field Contributor",
            "contributor@users.test",
            &["CONTRIBUTOR"],
        ),
    )
    .expect("إنشاء منفّذ العمل يجب أن ينجح");

    assert_eq!(created.status, "ACTIVE");
    assert_eq!(created.roles, vec!["CONTRIBUTOR".to_owned()]);
    assert_eq!(created.email, "contributor@users.test");

    // الصلاحيات الفعلية للدور الممنوح: تنفيذ مهمته وتسجيل دليل، وبلا أي سلطة إدارة.
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let grants: Vec<String> = connection
        .prepare(
            "SELECT p.code FROM user_role_assignments ura
             INNER JOIN role_permissions rp ON rp.role_id = ura.role_id
             INNER JOIN permissions p ON p.id = rp.permission_id
             WHERE ura.user_id = ?1 ORDER BY p.code",
        )
        .expect("استعلام المنح يجب أن ينجح")
        .query_map([&created.user_id], |row| row.get::<_, String>(0))
        .expect("قراءة المنح يجب أن تنجح")
        .collect::<Result<Vec<_>, _>>()
        .expect("تجميع المنح يجب أن ينجح");
    assert!(grants.contains(&"task:update".to_owned()));
    assert!(grants.contains(&"evidence:create".to_owned()));
    assert!(!grants.contains(&"user:create".to_owned()));
    assert!(!grants.contains(&"approval:decide".to_owned()));
    assert!(!grants.contains(&"evidence:update".to_owned()));

    let audit_events: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE event_type = 'USER_PROVISIONED' AND resource_id = ?1",
            [&created.user_id],
            |row| row.get(0),
        )
        .expect("التدقيق يجب أن يُقرأ");
    assert_eq!(audit_events, 1);
}

#[test]
fn provisioned_user_can_login_and_cannot_administer_users() {
    let state = production_admin_state();
    let created = create_workspace_user(
        &state,
        create_input("Reviewed Employee", "employee@users.test", &["CONTRIBUTOR"]),
    )
    .expect("إنشاء الموظف يجب أن ينجح");

    let session = login(
        &state,
        LoginInput {
            email: "employee@users.test".to_owned(),
            password: "Memb3r!Passw0rd#2026".to_owned(),
        },
    )
    .expect("الموظف الجديد يجب أن يدخل بكلمة مروره الأولية");
    assert_eq!(session.context.actor_id, created.user_id);
    // صلاحية التنفيذ موجودة، وسلطة إدارة المستخدمين غير موجودة — من الخلف.
    assert!(require_permission(&state, "TASK", "UPDATE").is_ok());
    assert!(matches!(
        require_permission(&state, "USER", "UPDATE"),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(matches!(
        list_workspace_users(&state),
        Err(AppError::AuthorizationDenied)
    ));
}

#[test]
fn rejects_duplicate_email_unknown_role_and_weak_password_without_partial_rows() {
    let state = production_admin_state();
    create_workspace_user(
        &state,
        create_input("First Employee", "duplicate@users.test", &["CONTRIBUTOR"]),
    )
    .expect("الإنشاء الأول يجب أن ينجح");

    let before: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row("SELECT COUNT(*) FROM local_users", [], |row| row.get(0))
        .expect("العدّ يجب أن ينجح");

    assert!(matches!(
        create_workspace_user(
            &state,
            create_input("Second Employee", "duplicate@users.test", &["CONTRIBUTOR"]),
        ),
        Err(AppError::UserEmailExists)
    ));
    assert!(matches!(
        create_workspace_user(
            &state,
            create_input("Ghost Role User", "ghost@users.test", &["GHOST_ROLE"]),
        ),
        Err(AppError::UserRoleUnknown)
    ));
    assert!(matches!(
        create_workspace_user(
            &state,
            CreateWorkspaceUserInput {
                display_name: "Weak Password".to_owned(),
                email: "weak@users.test".to_owned(),
                initial_password: "short".to_owned(),
                role_codes: vec!["CONTRIBUTOR".to_owned()],
            },
        ),
        Err(AppError::PasswordPolicyViolation)
    ));
    assert!(matches!(
        create_workspace_user(
            &state,
            CreateWorkspaceUserInput {
                display_name: "No Role".to_owned(),
                email: "norole@users.test".to_owned(),
                initial_password: "Memb3r!Passw0rd#2026".to_owned(),
                role_codes: vec![],
            },
        ),
        Err(AppError::ValidationFailed)
    ));

    let after: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row("SELECT COUNT(*) FROM local_users", [], |row| row.get(0))
        .expect("العدّ يجب أن ينجح");
    assert_eq!(before, after, "كل رفض يجب ألا يترك صفوفاً جزئية");
}

#[test]
fn refuses_to_grant_a_role_that_exceeds_the_actor_authority() {
    let state = production_admin_state();
    // عضو بصلاحية إنشاء مستخدمين فقط — لا يملك أدوار المشرف الكاملة.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "INSERT OR IGNORE INTO role_permissions (role_id, permission_id) SELECT 'role_prod_reader', id FROM permissions WHERE code IN ('user:read','user:create')",
                [],
            )
            .expect("منح المُسنِد المحدود يجب أن ينجح");
        connection
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user_prod_member', 'role_prod_reader', ?1, 'system')",
                [chrono::Utc::now().to_rfc3339()],
            )
            .expect("إسناد دور العضو يجب أن ينجح");
    }
    let member_session = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        crate::infrastructure::database::resolve_active_session(
            &connection,
            &state.tenant_id(),
            "user_prod_member",
            &chrono::Utc::now().to_rfc3339(),
        )
        .expect("جلسة العضو يجب أن تُحل")
    };
    state
        .activate_identity(&state.tenant_id(), "user_prod_member", Some(member_session))
        .expect("تفعيل هوية العضو يجب أن ينجح");

    // يملك user:create فعلاً…
    assert!(require_permission(&state, "USER", "CREATE").is_ok());
    // …لكنه لا يستطيع منح TENANT_ADMIN لأنه يتجاوز أذوناته.
    assert!(matches!(
        create_workspace_user(
            &state,
            create_input("Shadow Admin", "shadow@users.test", &["TENANT_ADMIN"]),
        ),
        Err(AppError::UserRoleNotGrantable)
    ));
    let shadow_exists: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row(
            "SELECT COUNT(*) FROM local_users WHERE email = 'shadow@users.test'",
            [],
            |row| row.get(0),
        )
        .expect("العدّ يجب أن ينجح");
    assert_eq!(shadow_exists, 0, "رفع الصلاحيات يجب ألا يترك حساباً");
}

#[test]
fn revoking_sessions_cuts_access_immediately_and_audits() {
    let state = production_admin_state();
    let created = create_workspace_user(
        &state,
        create_input("Session Bound", "sessionbound@users.test", &["CONTRIBUTOR"]),
    )
    .expect("إنشاء الموظف يجب أن ينجح");
    // جلسة حقيقية للموظف الجديد.
    login(
        &state,
        LoginInput {
            email: "sessionbound@users.test".to_owned(),
            password: "Memb3r!Passw0rd#2026".to_owned(),
        },
    )
    .expect("دخول الموظف يجب أن ينجح");
    // عودة المسؤول: تسجيل دخوله يثبت هويته من جديد.
    login_admin_back(&state);

    let revoked = revoke_user_sessions(
        &state,
        RevokeUserSessionsInput {
            user_id: created.user_id.clone(),
        },
    )
    .expect("إلغاء الجلسات يجب أن ينجح");
    assert_eq!(revoked.revoked_sessions, 1);
    // الجلسة الملغاة لم تعد تمنح أي صلاحية.
    state
        .activate_identity(
            &state.tenant_id(),
            &created.user_id,
            Some("session-bound".to_owned()),
        )
        .expect("تفعيل الهوية يجب أن ينجح");
    assert!(matches!(
        require_permission(&state, "TASK", "READ"),
        Err(AppError::AuthorizationDenied)
    ));
    let audit_events: i64 = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE event_type = 'USER_SESSIONS_REVOKED'",
                [],
                |row| row.get(0),
            )
            .expect("التدقيق يجب أن يُقرأ")
    };
    assert_eq!(audit_events, 1);
    // إلغاء جلسات الذات مرفوض — لا يقطع المسؤول جلسته من هذا المسار.
    let (admin_id, admin_session) = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .query_row(
                "SELECT u.id, s.id FROM local_users u
                 INNER JOIN local_sessions s ON s.user_id = u.id AND s.revoked_at IS NULL
                 WHERE u.email = 'admin@users.test'
                 ORDER BY s.rowid ASC LIMIT 1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .expect("جلسة المسؤول يجب أن توجد")
    };
    state
        .activate_identity(&state.tenant_id(), &admin_id, Some(admin_session))
        .expect("تفعيل الهوية يجب أن ينجح");
    assert!(matches!(
        revoke_user_sessions(&state, RevokeUserSessionsInput { user_id: admin_id },),
        Err(AppError::SelfModificationForbidden)
    ));
}

// انحدار أمني: كان فحص «لا تمنح ما لا تملك» موجوداً في الإنشاء وحده، فأمكن
// لمن يملك user:update إسناد دور أعلى من أذوناته إلى حساب قائم — تجاوز غير مباشر.
#[test]
fn refuses_to_assign_a_role_that_exceeds_the_actor_authority() {
    let state = production_admin_state();
    // المسؤول الكامل ينشئ العضو الهدف قبل أن تتحول الهوية إلى المُسنِد المحدود.
    let target = create_workspace_user(
        &state,
        create_input(
            "Assignment Target",
            "assigntarget@users.test",
            &["CONTRIBUTOR"],
        ),
    )
    .expect("إنشاء العضو الهدف يجب أن ينجح");

    // المُسنِد المحدود: قراءة وتحديث المستخدمين فقط، بلا أذونات المشرف الكاملة.
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "INSERT OR IGNORE INTO role_permissions (role_id, permission_id) SELECT 'role_prod_reader', id FROM permissions WHERE code IN ('user:read','user:update')",
                [],
            )
            .expect("منح المُسنِد المحدود يجب أن ينجح");
        connection
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user_prod_member', 'role_prod_reader', ?1, 'system')",
                [chrono::Utc::now().to_rfc3339()],
            )
            .expect("إسناد دور العضو يجب أن ينجح");
    }
    let member_session = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        crate::infrastructure::database::resolve_active_session(
            &connection,
            &state.tenant_id(),
            "user_prod_member",
            &chrono::Utc::now().to_rfc3339(),
        )
        .expect("جلسة العضو يجب أن تُحل")
    };
    state
        .activate_identity(&state.tenant_id(), "user_prod_member", Some(member_session))
        .expect("تفعيل هوية العضو يجب أن ينجح");

    // يملك الإسناد فعلاً…
    assert!(require_permission(&state, "USER", "UPDATE").is_ok());
    // …لكنه لا يمنح TENANT_ADMIN لأنه يتجاوز أذوناته — نفس ضابط الإنشاء.
    assert!(matches!(
        assign_user_role(
            &state,
            UserRoleInput {
                user_id: target.user_id.clone(),
                role_code: "TENANT_ADMIN".to_owned(),
            },
        ),
        Err(AppError::UserRoleNotGrantable)
    ));

    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let admin_assignments: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM user_role_assignments ura
             INNER JOIN roles r ON r.id = ura.role_id
             WHERE ura.user_id = ?1 AND r.code = 'TENANT_ADMIN'",
            [&target.user_id],
            |row| row.get(0),
        )
        .expect("العدّ يجب أن ينجح");
    assert_eq!(admin_assignments, 0, "رفع الصلاحيات يجب ألا يترك إسناداً");
    let role_events: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE event_type = 'USER_ROLE_ASSIGNED'",
            [],
            |row| row.get(0),
        )
        .expect("التدقيق يجب أن يُقرأ");
    assert_eq!(role_events, 0, "الرفض يجب ألا ينتج حدث تدقيق");
}

// =====================================================================
// جولة الإغلاق النهائية — ضابط آخر مسؤول فعال
// ---------------------------------------------------------------------
// الملاحظة المعمارية المعلنة: الجلسة في NEXUS سطح مكتب أحادية الهوية، فطلب
// واحد فقط يُنفَّذ بهوية واحدة في أي لحظة، و`require_permission` يشترط مستخدماً
// نشطاً. لذلك المسار التسلسلي لا يبلغ حالة «صفر مسؤولين» أصلاً؛ الضابط هنا
// دفاع في العمق يبقى صحيحاً لو تعددت العمليات/الاتصالات على ملف القاعدة نفسه.
// =====================================================================
fn count_administrators(state: &AppState) -> i64 {
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let transaction = connection
        .unchecked_transaction()
        .expect("معاملة القراءة يجب أن تبدأ");
    let count = super::count_active_administrators(&transaction, &state.tenant_id())
        .expect("عدّ المسؤولين يجب أن ينجح");
    transaction.rollback().expect("تراجع القراءة يجب أن ينجح");
    count
}

fn create_second_administrator(state: &AppState) -> String {
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let now = chrono::Utc::now().to_rfc3339();
    let tenant_id = state.tenant_id();
    connection
        .execute(
            "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, failed_attempts, created_at, updated_at) VALUES ('user_prod_admin_second', ?1, 'Second Admin', 'second.admin@users.test', NULL, 'ACTIVE', 0, ?2, ?2)",
            params![tenant_id, now],
        )
        .expect("المسؤول الثاني يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role_prod_admin_second', ?1, 'TENANT_ADMIN_SECOND', 'Second Admin Role', ?2, ?2)",
            params![tenant_id, now],
        )
        .expect("دور المسؤول الثاني يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO role_permissions (role_id, permission_id) SELECT 'role_prod_admin_second', id FROM permissions",
            [],
        )
        .expect("منح المسؤول الثاني يجب أن تنشأ");
    connection
        .execute(
            "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user_prod_admin_second', 'role_prod_admin_second', ?1, 'test')",
            params![now],
        )
        .expect("إسناد المسؤول الثاني يجب أن ينجح");
    connection
        .execute(
            "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES ('session_prod_admin_second', ?1, 'user_prod_admin_second', ?2, '2030-01-01T00:00:00Z', NULL, 'test')",
            params![tenant_id, now],
        )
        .expect("جلسة المسؤول الثاني يجب أن تنشأ");
    "user_prod_admin_second".to_owned()
}

#[test]
fn administration_is_measured_by_permission_not_by_role_name() {
    let state = production_admin_state();
    let tenant_id = state.tenant_id();
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let now = chrono::Utc::now().to_rfc3339();
        // دور باسم إداري لكنه لا يمنح أي سلطة على المستخدمين.
        connection
            .execute(
                "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES ('role_prod_reader_only', ?1, 'READER_ONLY', 'Reader Only', ?2, ?2)",
                params![tenant_id, now],
            )
            .expect("الدور يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO role_permissions (role_id, permission_id) SELECT 'role_prod_reader_only', id FROM permissions WHERE code = 'user:read'",
                [],
            )
            .expect("منح القراءة يجب أن تنشأ");
        connection
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES ('user_prod_member', 'role_prod_reader_only', ?1, 'test')",
                params![now],
            )
            .expect("إسناد القارئ يجب أن ينجح");
    }

    let reader_is_administrator = {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        let transaction = connection
            .unchecked_transaction()
            .expect("معاملة القراءة يجب أن تبدأ");
        let holds = super::user_holds_administration(&transaction, &tenant_id, "user_prod_member")
            .expect("الفحص يجب أن ينجح");
        transaction.rollback().expect("التراجع يجب أن ينجح");
        holds
    };
    assert!(
        !reader_is_administrator,
        "قراءة المستخدمين ليست سلطة إدارية"
    );
    // المسؤول الفعلي وحده يُحتسب.
    assert_eq!(count_administrators(&state), 1);
}

#[test]
fn disabling_an_administrator_leaves_a_remaining_administrator_and_never_zero() {
    let state = production_admin_state();
    let actor_id = state.current_user_id().expect("هوية الفاعل");
    let second_admin = create_second_administrator(&state);
    assert_eq!(count_administrators(&state), 2);

    // تعطيل مسؤول آخر مسموح ما دام الفاعل نفسه مسؤولاً نشطاً.
    set_user_status(
        &state,
        SetUserStatusInput {
            user_id: second_admin,
            status: "DISABLED".to_owned(),
        },
    )
    .expect("تعطيل مسؤول ثانٍ يجب أن ينجح");
    assert_eq!(count_administrators(&state), 1, "يبقى مسؤول نشط واحد");

    // المسؤول الأخير لا يستطيع تعطيل نفسه — الضابط القائم يمنع الوصول إلى صفر.
    assert!(matches!(
        set_user_status(
            &state,
            SetUserStatusInput {
                user_id: actor_id,
                status: "DISABLED".to_owned(),
            },
        ),
        Err(AppError::SelfModificationForbidden)
    ));
    assert_eq!(
        count_administrators(&state),
        1,
        "لا يمكن بلوغ حالة صفر مسؤولين"
    );
}

#[test]
fn production_role_catalog_separates_review_authority_from_user_administration() {
    let state = production_admin_state();
    let connection = state.connection.lock().expect("القفل يجب أن يتاح");
    let tenant_id = state.tenant_id();
    let reviewer_evidence_update: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM roles r
             INNER JOIN role_permissions rp ON rp.role_id = r.id
             INNER JOIN permissions p ON p.id = rp.permission_id
             WHERE r.tenant_id = ?1 AND r.code = 'REVIEWER' AND p.code = 'evidence:update'",
            params![tenant_id],
            |row| row.get(0),
        )
        .expect("عدّ صلاحية المراجع يجب أن ينجح");
    let reviewer_user_update: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM roles r
             INNER JOIN role_permissions rp ON rp.role_id = r.id
             INNER JOIN permissions p ON p.id = rp.permission_id
             WHERE r.tenant_id = ?1 AND r.code = 'REVIEWER' AND p.code IN ('user:update', 'user:create')",
            params![tenant_id],
            |row| row.get(0),
        )
        .expect("عدّ صلاحيات الإدارة يجب أن ينجح");
    assert_eq!(reviewer_evidence_update, 1, "المراجع يقرر حالة الدليل");
    assert_eq!(
        reviewer_user_update, 0,
        "المراجع لا يملك إدارة المستخدمين — فصل صريح بين المراجعة والإدارة"
    );
}
