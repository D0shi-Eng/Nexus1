// حامل اختبار مشترك لوحدتي الأدلة والضوابط: يبني قاعدة ذاكرة بهجرات 0001→0022
// وثلاثة مستأجرين (a/b/فارغ) وبذور مستقلة قابلة لإعادة الاستخدام.
// لا يمنح أي صلاحية تلقائياً — كل اختبار يمنح ما يختبره صراحةً.
#![cfg(test)]

use chrono::Utc;
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::infrastructure::database::AppState;

pub const TENANT_A: &str = "tenant-fixture-a";
pub const TENANT_B: &str = "tenant-fixture-b";
pub const TENANT_EMPTY: &str = "tenant-fixture-empty";

pub const USER_A: &str = "user-fixture-a";
pub const USER_B: &str = "user-fixture-b";
pub const ROLE_A: &str = "role-fixture-a";
pub const ROLE_B: &str = "role-fixture-b";
pub const SESSION_A: &str = "session-fixture-a";
pub const SESSION_B: &str = "session-fixture-b";
// هوية مراجعة إضافية داخل المستأجر نفسه — لاختبار فصل من يقدّم الدليل عن من يقرره.
pub const USER_A_REVIEWER: &str = "user-fixture-a-reviewer";
pub const ROLE_A_REVIEWER: &str = "role-fixture-a-reviewer";
pub const SESSION_A_REVIEWER: &str = "session-fixture-a-reviewer";

pub const OBLIGATION_A: &str = "obligation-fixture-a";
pub const OBLIGATION_B: &str = "obligation-fixture-b";
pub const RISK_A: &str = "risk-fixture-a";
pub const RISK_B: &str = "risk-fixture-b";
pub const EVIDENCE_A: &str = "evidence-fixture-a";
pub const EVIDENCE_B: &str = "evidence-fixture-b";

// نفس معرّفات كتالوج الإنتاج (permission_* ) حتى تنطبق قيود المفاتيح الأجنبية
// على منح الأدوار في الاختبار كما تنطبق في الإقلاع الحقيقي.
pub const PERMISSION_ROWS: [(&str, &str, &str, &str); 11] = [
    (
        "permission_evidence_read",
        "evidence:read",
        "EVIDENCE",
        "READ",
    ),
    (
        "permission_evidence_create",
        "evidence:create",
        "EVIDENCE",
        "CREATE",
    ),
    (
        "permission_evidence_update",
        "evidence:update",
        "EVIDENCE",
        "UPDATE",
    ),
    ("permission_control_read", "control:read", "CONTROL", "READ"),
    (
        "permission_control_create",
        "control:create",
        "CONTROL",
        "CREATE",
    ),
    (
        "permission_control_update",
        "control:update",
        "CONTROL",
        "UPDATE",
    ),
    (
        "permission_control_disable",
        "control:disable",
        "CONTROL",
        "DISABLE",
    ),
    ("permission_control_link", "control:link", "CONTROL", "LINK"),
    (
        "permission_control_assess",
        "control:assess",
        "CONTROL",
        "ASSESS",
    ),
    ("permission_report_read", "report:read", "REPORT", "READ"),
    (
        "permission_report_export",
        "report:export",
        "REPORT",
        "EXPORT",
    ),
];

// فعالية التقييم صلاحية منفصلة تُمنح صراحةً لمن يختبر التقييم.
pub const PERMISSION_CONTROL_ASSESS: &str = "permission_control_assess";
pub const PERMISSION_REPORT_READ: &str = "permission_report_read";
pub const PERMISSION_REPORT_EXPORT: &str = "permission_report_export";

// معرّفات بذور التقارير: ضابطان لكل مستأجر (أحدهما بلا روابط) ومهمتان
// (إحداهما بلا مُكلَّف) وحدثا تدقيق — كلها صفوف حقيقية في جداول الإنتاج.
pub const CONTROL_A: &str = "control-fixture-a";
pub const TASK_A: &str = "task-fixture-a";

pub struct Fixture {
    pub state: AppState,
}

impl Fixture {
    pub fn new() -> Self {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
        apply_all_migrations(&connection);

        let now = Utc::now().to_rfc3339();
        for tenant_id in [TENANT_A, TENANT_B, TENANT_EMPTY] {
            connection
                .execute(
                    "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?1, 'DEMO_SYNTHETIC', ?2, ?2)",
                    (tenant_id, &now),
                )
                .expect("المستأجر يجب أن ينشأ");
        }

        for (id, code, resource_type, action) in PERMISSION_ROWS {
            connection
                .execute(
                    "INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES (?1, ?2, ?3, ?4, 'HIGHLY_RESTRICTED', ?5)",
                    (id, code, resource_type, action, &now),
                )
                .expect("الصلاحية يجب أن تنشأ");
        }

        for tenant_id in [TENANT_A, TENANT_B] {
            let suffix = tenant_suffix(tenant_id);
            seed_source_and_obligation(&connection, tenant_id, &suffix, &now);
            seed_risk(&connection, tenant_id, &suffix, &now);
            seed_evidence(&connection, tenant_id, &suffix, &now);
        }

        insert_identity(&connection, TENANT_A, USER_A, ROLE_A, SESSION_A, &now);
        insert_identity(&connection, TENANT_B, USER_B, ROLE_B, SESSION_B, &now);

        let state = AppState::test_instance(connection, TENANT_A.to_owned(), USER_A.to_owned());
        let fixture = Self { state };
        // منح كامل معلن سلفاً لكلا الدورين: اختبارات الرفض تسحب الصلاحية صراحةً بـ revoke،
        // فتصبح كل حالة اختبار صريحة بشأن ما هو ممنوع لا بشأن ما هو ناقص سهواً.
        let all_permissions: Vec<&str> = PERMISSION_ROWS.iter().map(|(id, _, _, _)| *id).collect();
        fixture.grant(ROLE_A, &all_permissions);
        fixture.grant(ROLE_B, &all_permissions);
        fixture
    }

    /// بذور التقارير الاختيارية: ضوابط وروابط ومهام وأحداث تدقيق لكل مستأجر.
    /// منفصلة عن البناء الأساسي عمداً: الاختبارات القائمة تبني افتراضاتها على
    /// قاعدة بأربعة كيانات فقط، فلا تُلوَّث بصفوف لا تخصّها. تُستدعى صراحةً
    /// من اختبارات التقارير التي تقيس تقارير مبنية على هذه الجداول.
    pub fn seed_report_data(&self) {
        let now = Utc::now().to_rfc3339();
        let connection = self
            .state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح");
        for tenant_id in [TENANT_A, TENANT_B] {
            let suffix = tenant_suffix(tenant_id);
            seed_controls(&connection, tenant_id, &suffix, &now);
            seed_tasks(&connection, tenant_id, &suffix, &now);
            seed_audit_events(&connection, tenant_id, &suffix, &now);
        }
    }

    pub fn grant(&self, role_id: &str, permission_ids: &[&str]) {
        let connection = self
            .state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح");
        for permission_id in permission_ids {
            connection
                .execute(
                    "INSERT OR IGNORE INTO role_permissions (role_id, permission_id) VALUES (?1, ?2)",
                    (role_id, permission_id),
                )
                .expect("منح الصلاحية يجب أن ينجح");
        }
    }

    pub fn revoke(&self, role_id: &str, permission_id: &str) {
        self.state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح")
            .execute(
                "DELETE FROM role_permissions WHERE role_id = ?1 AND permission_id = ?2",
                (role_id, permission_id),
            )
            .expect("سحب الصلاحية يجب أن ينجح");
    }

    pub fn count(&self, sql: &str) -> i64 {
        self.state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح")
            .query_row(sql, [], |row| row.get(0))
            .expect("الاستعلام يجب أن ينجح")
    }

    pub fn audit_count(&self, event_type: &str, resource_id: &str) -> i64 {
        self.state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح")
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE event_type = ?1 AND resource_id = ?2",
                (event_type, resource_id),
                |row| row.get(0),
            )
            .expect("استعلام التدقيق يجب أن ينجح")
    }

    pub fn evidence_version(&self, evidence_id: &str) -> i64 {
        self.state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح")
            .query_row(
                "SELECT version FROM evidence_items WHERE id = ?1",
                [evidence_id],
                |row| row.get(0),
            )
            .expect("قراءة النسخة يجب أن تنجح")
    }

    pub fn evidence_status(&self, evidence_id: &str) -> String {
        self.state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح")
            .query_row(
                "SELECT status FROM evidence_items WHERE id = ?1",
                [evidence_id],
                |row| row.get(0),
            )
            .expect("قراءة حالة الدليل يجب أن تنجح")
    }

    pub fn obligation_evidence_status(&self, obligation_id: &str) -> String {
        self.state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح")
            .query_row(
                "SELECT evidence_status FROM obligations WHERE id = ?1",
                [obligation_id],
                |row| row.get(0),
            )
            .expect("قراءة حالة دليل الالتزام يجب أن تنجح")
    }

    pub fn control_status(&self, control_id: &str) -> String {
        self.state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح")
            .query_row(
                "SELECT status FROM controls WHERE id = ?1",
                [control_id],
                |row| row.get(0),
            )
            .expect("قراءة حالة الضابط يجب أن تنجح")
    }

    pub fn column_names(&self, table: &str) -> Vec<String> {
        self.state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح")
            .prepare(&format!("SELECT name FROM pragma_table_info('{table}')"))
            .expect("فحص الأعمدة يجب أن ينجح")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("قراءة الأعمدة يجب أن تنجح")
            .collect::<Result<Vec<_>, _>>()
            .expect("تجميع الأعمدة يجب أن ينجح")
    }

    // التبديل إلى مستأجر ب ينتقل إلى هويته الخاصة (USER_B/ROLE_B) الممنوحة كامل الصلاحيات سلفاً،
    // فيصبح الفرق الوحيد المقيس هو عزل المستأجر نفسه لا سحب الصلاحية.
    pub fn switch_to_tenant_b(&self) {
        self.state.set_active_tenant_for_tests(TENANT_B);
        self.state.set_active_user_for_tests(USER_B);
    }

    // ===== 0026: مستخدم ثانٍ داخل المستأجر نفسه =====
    // يفصل «من قدّم الدليل» عن «من يقرر حالته». يُنشأ عند الطلب فقط حتى لا
    // تتغير افتراضات الاختبارات القائمة على مستخدم واحد في المستأجر.
    pub fn add_tenant_a_reviewer(&self) -> String {
        let now = Utc::now().to_rfc3339();
        let connection = self
            .state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح");
        connection
            .execute(
                "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, ?2, 'Reviewer A', 'reviewer-a@test.local', NULL, 'ACTIVE', ?3, ?3)",
                (USER_A_REVIEWER, TENANT_A, &now),
            )
            .expect("مستخدم المراجعة يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES (?1, ?2, 'REVIEWER', 'Reviewer', ?3, ?3)",
                (ROLE_A_REVIEWER, TENANT_A, &now),
            )
            .expect("دور المراجعة يجب أن ينشأ");
        for (permission_id, _, _, _) in PERMISSION_ROWS {
            connection
                .execute(
                    "INSERT OR IGNORE INTO role_permissions (role_id, permission_id) VALUES (?1, ?2)",
                    (ROLE_A_REVIEWER, permission_id),
                )
                .expect("منح المراجعة يجب أن ينجح");
        }
        connection
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, ?2, ?3, 'system')",
                (USER_A_REVIEWER, ROLE_A_REVIEWER, &now),
            )
            .expect("إسناد دور المراجعة يجب أن ينجح");
        connection
            .execute(
                "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, ?2, ?3, ?4, '2030-01-01T00:00:00Z', NULL, 'test')",
                (SESSION_A_REVIEWER, TENANT_A, USER_A_REVIEWER, &now),
            )
            .expect("جلسة المراجعة يجب أن تنشأ");
        drop(connection);
        USER_A_REVIEWER.to_owned()
    }

    pub fn switch_to_tenant_a_reviewer(&self) {
        self.state
            .activate_identity(
                TENANT_A,
                USER_A_REVIEWER,
                Some(SESSION_A_REVIEWER.to_owned()),
            )
            .expect("تفعيل هوية المراجعة يجب أن ينجح");
    }
}

fn tenant_suffix(tenant_id: &str) -> String {
    match tenant_id {
        TENANT_A => "a".to_owned(),
        TENANT_B => "b".to_owned(),
        other => other.to_owned(),
    }
}

fn apply_all_migrations(connection: &Connection) {
    for migration in [
        include_str!("../../migrations/0001_initial.sql"),
        include_str!("../../migrations/0002_identity_and_access.sql"),
        include_str!("../../migrations/0003_workflow_and_risk.sql"),
        include_str!("../../migrations/0004_evidence_metadata.sql"),
        include_str!("../../migrations/0005_ai_governance.sql"),
        include_str!("../../migrations/0006_correct_demo_truth_labels.sql"),
        include_str!("../../migrations/0007_obligation_management.sql"),
        include_str!("../../migrations/0008_obligation_relationship_update_isolation.sql"),
        include_str!("../../migrations/0009_controls_read_model.sql"),
        include_str!("../../migrations/0010_agent_foundation.sql"),
        include_str!("../../migrations/0011_agent_runs.sql"),
        include_str!("../../migrations/0012_agent_run_approval_lifecycle.sql"),
        include_str!("../../migrations/0013_agent_execution_results.sql"),
        include_str!("../../migrations/0014_governed_skills.sql"),
        include_str!("../../migrations/0015_agent_run_skill_bindings.sql"),
        include_str!("../../migrations/0016_governed_tools.sql"),
        include_str!("../../migrations/0017_governed_agent_tool_runtime.sql"),
        include_str!("../../migrations/0018_knowledge_foundation.sql"),
        include_str!("../../migrations/0019_memory_foundation.sql"),
        include_str!("../../migrations/0020_intelligence_runtime.sql"),
        include_str!("../../migrations/0021_production_identity_and_backup.sql"),
        include_str!("../../migrations/0022_controls_authoring.sql"),
        include_str!("../../migrations/0025_task_lifecycle.sql"),
        include_str!("../../migrations/0026_production_user_and_authoring.sql"),
    ] {
        connection
            .execute_batch(migration)
            .expect("هجرة الاختبار يجب أن تنجح");
    }
}

fn seed_source_and_obligation(connection: &Connection, tenant_id: &str, suffix: &str, now: &str) {
    connection
        .execute(
            "INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES (?1, ?2, 'Synthetic Authority', 'Synthetic Source', NULL, '1.0', ?3, 0, 'REQUIRES_REVIEW', ?3, ?3)",
            (format!("source-fixture-{suffix}"), tenant_id, now),
        )
        .expect("المصدر يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, source_type, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'Owner', NULL, 'HIGH', 'OPEN', 'APPLIES', 'MISSING', 'REQUIRES_REVIEW', 'SYNTHETIC', ?5, ?5)",
            (
                format!("obligation-fixture-{suffix}"),
                tenant_id,
                format!("source-fixture-{suffix}"),
                format!("Obligation {suffix}"),
                now,
            ),
        )
        .expect("الالتزام يجب أن ينشأ");
}

fn seed_risk(connection: &Connection, tenant_id: &str, suffix: &str, now: &str) {
    connection
        .execute(
            "INSERT INTO risks (id, tenant_id, obligation_id, title, severity, likelihood, impact, status, owner, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'HIGH', 3, 4, 'OPEN', 'Risk owner', ?5, ?5)",
            (
                format!("risk-fixture-{suffix}"),
                tenant_id,
                format!("obligation-fixture-{suffix}"),
                format!("Risk {suffix}"),
                now,
            ),
        )
        .expect("الخطر يجب أن ينشأ");
}

fn seed_evidence(connection: &Connection, tenant_id: &str, suffix: &str, now: &str) {
    connection
        .execute(
            "INSERT INTO evidence_items (id, tenant_id, obligation_id, classification, status, content_hash, version, created_at, updated_at, label, expires_at) VALUES (?1, ?2, ?3, 'INTERNAL', 'REQUIRES_REVIEW', ?4, 1, ?5, ?5, ?6, NULL)",
            (
                format!("evidence-fixture-{suffix}"),
                tenant_id,
                format!("obligation-fixture-{suffix}"),
                format!("hash-fixture-{suffix}"),
                now,
                format!("Evidence {suffix}"),
            ),
        )
        .expect("الدليل يجب أن ينشأ");
}

// ضابطان لكل مستأجر: أحدهما مرتبط بالالتزام والخطر والدليل، والآخر بلا أي رابط.
// هذا التمييز مقصود: تقرير فعالية الضوابط يجب أن يُظهر تغطية صفرية بصدق.
fn seed_controls(connection: &Connection, tenant_id: &str, suffix: &str, now: &str) {
    let control_id = format!("control-fixture-{suffix}");
    let unlinked_control_id = format!("control-fixture-{suffix}-unlinked");
    for (id, title, effectiveness, assessed_at) in [
        (
            control_id.as_str(),
            format!("Control {suffix}"),
            "EFFECTIVE",
            Some(now),
        ),
        (
            unlinked_control_id.as_str(),
            format!("Control {suffix} (unlinked)"),
            "NOT_ASSESSED",
            None,
        ),
    ] {
        connection
            .execute(
                "INSERT INTO controls (id, tenant_id, title, status, category, owner, effectiveness, last_assessed_at, assessment_note, created_at, updated_at) VALUES (?1, ?2, ?3, 'ACTIVE', 'Synthetic category', 'Control owner', ?4, ?5, NULL, ?6, ?6)",
                (id, tenant_id, title, effectiveness, assessed_at, now),
            )
            .expect("الضابط يجب أن ينشأ");
    }

    // الروابط الفعلية تُدرج بـINSERT عادي كي تُنفَّذ مشغّلات عزل المستأجر.
    connection
        .execute(
            "INSERT INTO control_obligations (control_id, obligation_id, tenant_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            (
                control_id.as_str(),
                format!("obligation-fixture-{suffix}"),
                tenant_id,
                now,
            ),
        )
        .expect("ربط الضابط بالالتزام يجب أن ينجح");
    connection
        .execute(
            "INSERT INTO control_risks (control_id, risk_id, tenant_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            (
                control_id.as_str(),
                format!("risk-fixture-{suffix}"),
                tenant_id,
                now,
            ),
        )
        .expect("ربط الضابط بالخطر يجب أن ينجح");
    connection
        .execute(
            "INSERT INTO control_evidence (control_id, evidence_id, tenant_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            (
                control_id.as_str(),
                format!("evidence-fixture-{suffix}"),
                tenant_id,
                now,
            ),
        )
        .expect("ربط الضابط بالدليل يجب أن ينجح");
}

// مهمتان لكل مستأجر: واحدة مُكلَّفة وواحدة بلا مُكلَّف — يثبت تقرير العبء
// أن الصفّ بلا مُكلَّف يظهر صراحةً ولا يُنسب إلى أحد.
fn seed_tasks(connection: &Connection, tenant_id: &str, suffix: &str, now: &str) {
    connection
        .execute(
            "INSERT INTO tasks (id, tenant_id, obligation_id, title, description, assignee_user_id, due_at, priority, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'Synthetic remediation task', ?5, '2026-01-05T00:00:00Z', 'HIGH', 'OPEN', ?6, ?6)",
            (
                format!("task-fixture-{suffix}"),
                tenant_id,
                format!("obligation-fixture-{suffix}"),
                format!("Task {suffix}"),
                format!("user-fixture-{suffix}"),
                now,
            ),
        )
        .expect("المهمة يجب أن تنشأ");
    connection
        .execute(
            "INSERT INTO tasks (id, tenant_id, obligation_id, title, description, assignee_user_id, due_at, priority, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'Synthetic unassigned task', NULL, '2026-01-06T00:00:00Z', 'MEDIUM', 'IN_PROGRESS', ?5, ?5)",
            (
                format!("task-fixture-{suffix}-unassigned"),
                tenant_id,
                format!("obligation-fixture-{suffix}"),
                format!("Unassigned task {suffix}"),
                now,
            ),
        )
        .expect("المهمة غير المُكلَّفة يجب أن تنشأ");
}

// حدثا تدقيق لكل مستأجر بنفس صيغة الهاش المُسلسَل المستعملة في مسار الإنتاج،
// حتى يكون تقرير النشاط التدقيقي قراءةً لأحداث حقيقية لا لصفوف مُصنَّعة.
fn seed_audit_events(connection: &Connection, tenant_id: &str, suffix: &str, now: &str) {
    let first_id = format!("audit-fixture-{suffix}-1");
    let second_id = format!("audit-fixture-{suffix}-2");
    let resource_id = format!("obligation-fixture-{suffix}");
    for (id, event_type, actor, resource_type, occurred_at, previous_hash) in [
        (
            first_id.as_str(),
            "OBLIGATION_CREATED",
            "NEXUS System",
            "Obligation",
            now,
            "",
        ),
        (
            second_id.as_str(),
            "EVIDENCE_REQUIRES_REVIEW",
            "NEXUS System",
            "Evidence",
            now,
            "seed-previous-hash",
        ),
    ] {
        let payload = format!(
            "{previous_hash}|{id}|{event_type}|{actor}|{resource_type}|{resource_id}|{occurred_at}"
        );
        let event_hash = format!("{:x}", Sha256::digest(payload.as_bytes()));
        connection
            .execute(
                "INSERT INTO audit_events (id, tenant_id, event_type, actor_name, resource_type, resource_id, occurred_at, previous_hash, event_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                (
                    id,
                    tenant_id,
                    event_type,
                    actor,
                    resource_type,
                    resource_id.as_str(),
                    occurred_at,
                    previous_hash,
                    event_hash,
                ),
            )
            .expect("حدث التدقيق يجب أن ينشأ");
    }
}

fn insert_identity(
    connection: &Connection,
    tenant_id: &str,
    user_id: &str,
    role_id: &str,
    session_id: &str,
    now: &str,
) {
    connection
        .execute(
            "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, NULL, 'ACTIVE', ?5, ?5)",
            (user_id, tenant_id, user_id, format!("{user_id}@test.local"), now),
        )
        .expect("المستخدم يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES (?1, ?2, 'OPERATOR', 'Operator', ?3, ?3)",
            (role_id, tenant_id, now),
        )
        .expect("الدور يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, ?2, ?3, 'system')",
            (user_id, role_id, now),
        )
        .expect("تعيين الدور يجب أن ينجح");
    connection
        .execute(
            "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, ?2, ?3, ?4, '2030-01-01T00:00:00Z', NULL, 'test')",
            (session_id, tenant_id, user_id, now),
        )
        .expect("الجلسة يجب أن تنشأ");
}
