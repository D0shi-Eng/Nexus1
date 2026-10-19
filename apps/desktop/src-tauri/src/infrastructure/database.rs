use std::{fs, sync::Mutex};

use chrono::Utc;
use rusqlite::{params, Connection};
use tauri::{AppHandle, Manager};

use crate::{
    domain::{error::AppError, models::RuntimeMode},
    infrastructure::credential_vault::{CredentialVault, FileCredentialVault},
};

// معرّفات هويات الديمو التركيبية — تُستخدم في البذر وفي تبديل الفاعل النشط فقط.
pub const DEMO_TENANT_ID: &str = "tenant_demo_sa_001";
pub const DEMO_OPERATOR_USER_ID: &str = "user_demo_operator";
pub const DEMO_REVIEWER_USER_ID: &str = "user_demo_reviewer";
pub const DEMO_ADMIN_USER_ID: &str = "user_demo_admin";

// الهوية النشطة كما يرى الخلف — لا يملك الواجهة أي حقول منها ولا يقبلها أي أمر IPC.
#[derive(Debug, Clone)]
pub struct ActiveIdentity {
    pub tenant_id: String,
    pub user_id: String,
    pub session_id: Option<String>,
}

impl ActiveIdentity {
    // هوية مجهولة: كل شيء فارغ وأي تفويض عليها يُرفض فشلاً مغلقاً.
    pub fn anonymous() -> Self {
        Self {
            tenant_id: String::new(),
            user_id: String::new(),
            session_id: None,
        }
    }
}

pub struct AppState {
    pub connection: Mutex<Connection>,
    // السياق النشط: مستأجر + مستخدم + جلسة — يُشتق عند الدخول/تبديل فاعل الديمو فقط.
    active: Mutex<ActiveIdentity>,
    // وضع التشغيل — يُقرأ من app_runtime_state ويُعاد تحميله بعد الاستعادة فقط.
    runtime_mode: Mutex<RuntimeMode>,
    // مسار ملف قاعدة البيانات الحية — تحتاجه الاستعادة للاستبدال الآمن (بلا في الذاكرة).
    live_database_path: Option<std::path::PathBuf>,
    // ===== مخزن أسرار الموفرين (0023) =====
    // لا يمر أي سر عبر قاعدة البيانات أو سجل التدقيق: هذا هو المسار الوحيد.
    // المجلد يُشتق من مسار بيانات التطبيق فقط ولا يأتي من العميل.
    vault: std::sync::RwLock<std::sync::Arc<dyn CredentialVault>>,
}

impl AppState {
    pub fn initialize(app: &AppHandle) -> Result<Self, AppError> {
        let data_directory = app
            .path()
            .app_local_data_dir()
            .map_err(|_| AppError::StorageInitialization)?;
        fs::create_dir_all(&data_directory).map_err(|_| AppError::StorageInitialization)?;
        crate::infrastructure::local_logger::initialize(data_directory.join("logs"));

        let database_path = data_directory.join("nexus-local.db");
        let connection =
            Connection::open(&database_path).map_err(|_| AppError::StorageInitialization)?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|_| AppError::StorageInitialization)?;
        let runtime_mode = initialize_workspace_database(&connection).inspect_err(|_| {
            crate::infrastructure::local_logger::error("DB_INIT_FAILED", "", "");
        })?;
        crate::infrastructure::local_logger::info(
            "APP_START",
            "",
            // إصدار المخطط يُقرأ من القاعدة لا يُكتب ثابتاً في النص: الرقم الثابت
            // كان يتخلّف عن الهجرات (سُجِّل 21 بينما المخطط عند 23)، فيصير السجل
            // مضلِّلاً في التشخيص. المصدر الوحيد هنا هو `schema_migrations`.
            &format!(
                "{{\"appVersion\":\"{}\",\"schemaVersion\":{},\"mode\":\"{}\"}}",
                env!("CARGO_PKG_VERSION"),
                connection
                    .query_row(
                        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                        [],
                        |row| row.get::<_, i64>(0),
                    )
                    .unwrap_or(0),
                runtime_mode.as_str()
            ),
        );

        // في وضع الديمو فقط تبدأ الجلسة بمشغل الديمو؛ ما سواه يبدأ بلا هوية (إعداد أو دخول).
        let active = if runtime_mode == RuntimeMode::Demo {
            let session_id = resolve_active_session(
                &connection,
                DEMO_TENANT_ID,
                DEMO_OPERATOR_USER_ID,
                &Utc::now().to_rfc3339(),
            );
            ActiveIdentity {
                tenant_id: DEMO_TENANT_ID.to_owned(),
                user_id: DEMO_OPERATOR_USER_ID.to_owned(),
                session_id,
            }
        } else {
            ActiveIdentity::anonymous()
        };

        Ok(Self {
            connection: Mutex::new(connection),
            active: Mutex::new(active),
            runtime_mode: Mutex::new(runtime_mode),
            live_database_path: Some(database_path),
            // مجلد الأسرار مجاور لقاعدة البيانات تحت بيانات التطبيق المحلية:
            // لا مسار من العميل ولا مسار نسبي قابل للخروج.
            vault: std::sync::RwLock::new(std::sync::Arc::new(FileCredentialVault::new(
                data_directory.join("credentials"),
            ))),
        })
    }

    // ===== مخزن الأسرار =====

    /// الوصول إلى مخزن الأسرار. لا يُعيد السر نفسه أبداً — العقد يعيد علامات.
    pub fn credential_vault(&self) -> Result<std::sync::Arc<dyn CredentialVault>, AppError> {
        self.vault
            .read()
            .map(|vault| std::sync::Arc::clone(&vault))
            .map_err(|_| AppError::ProviderCredentialStorageUnavailable)
    }

    #[cfg(test)]
    pub fn set_credential_vault_for_tests(&self, vault: std::sync::Arc<dyn CredentialVault>) {
        if let Ok(mut current) = self.vault.write() {
            *current = vault;
        }
    }

    // مسار القاعدة الحية — الاستعادة ترفض العمل على قاعدة بلا ملف (ذاكرة الاختبارات).
    pub fn database_path(&self) -> Option<std::path::PathBuf> {
        self.live_database_path.clone()
    }

    // قراءة المستأجر النشط — قيمة مستنسخة؛ الفارغ يعني بلا هوية ويُرفض في التفويض.
    pub fn tenant_id(&self) -> String {
        self.active
            .lock()
            .map(|active| active.tenant_id.clone())
            .unwrap_or_default()
    }

    // قراءة الهوية النشطة — بلا هوية يعني رفضاً في مسار التفويض (فشل مغلق).
    pub fn current_user_id(&self) -> Result<String, AppError> {
        self.active
            .lock()
            .map(|active| {
                if active.user_id.is_empty() {
                    Err(AppError::AuthorizationDenied)
                } else {
                    Ok(active.user_id.clone())
                }
            })
            .map_err(|_| AppError::DataAccess)?
    }

    // معرف الجلسة النشطة — التفويض يربط بها تحديداً لا بأي جلسة أخرى للمستخدم.
    pub fn current_session_id(&self) -> Result<Option<String>, AppError> {
        self.active
            .lock()
            .map(|active| active.session_id.clone())
            .map_err(|_| AppError::DataAccess)
    }

    pub fn runtime_mode(&self) -> Result<RuntimeMode, AppError> {
        self.runtime_mode
            .lock()
            .map(|mode| *mode)
            .map_err(|_| AppError::DataAccess)
    }

    // تثبيت سياق نشط كامل — يستخدمه دخول الإنتاج وتبديل فاعل الديمو بعد التحقق والتدقيق.
    pub fn activate_identity(
        &self,
        tenant_id: &str,
        user_id: &str,
        session_id: Option<String>,
    ) -> Result<(), AppError> {
        self.active
            .lock()
            .map_err(|_| AppError::DataAccess)?
            .replace_with(tenant_id, user_id, session_id)
    }

    // إبطال السياق النشط — الخروج أو بعد الاستعادة: لا هوية بعدها إطلاقاً.
    pub fn clear_active_identity(&self) -> Result<(), AppError> {
        self.active
            .lock()
            .map(|mut active| {
                *active = ActiveIdentity::anonymous();
            })
            .map_err(|_| AppError::DataAccess)
    }

    // تحديث الوضع في الذاكرة من قاعدة البيانات — بعد الاستعادة أو الانتقال بين الأوضاع.
    pub fn refresh_runtime_mode(&self, connection: &Connection) -> Result<RuntimeMode, AppError> {
        let mode = read_runtime_mode(connection)?;
        if let Ok(mut current) = self.runtime_mode.lock() {
            *current = mode;
        }
        Ok(mode)
    }

    pub fn set_runtime_mode(&self, mode: RuntimeMode) -> Result<(), AppError> {
        self.runtime_mode
            .lock()
            .map(|mut current| {
                *current = mode;
            })
            .map_err(|_| AppError::DataAccess)
    }

    // ===== حوامل الاختبار فقط — لا تستخدم في مسارات الإنتاج =====

    #[cfg(test)]
    pub fn test_instance(
        connection: Connection,
        tenant_id: impl Into<String>,
        user_id: impl Into<String>,
    ) -> Self {
        let tenant_id = tenant_id.into();
        let user_id = user_id.into();
        let now = Utc::now().to_rfc3339();
        let session_id =
            resolve_active_session(&connection, tenant_id.as_str(), user_id.as_str(), &now);
        Self {
            connection: Mutex::new(connection),
            active: Mutex::new(ActiveIdentity {
                tenant_id,
                user_id,
                session_id,
            }),
            runtime_mode: Mutex::new(RuntimeMode::Demo),
            live_database_path: None,
            // حوامل الاختبار تحقن مخزن أسرار بديلاً في الذاكرة عند الحاجة.
            vault: std::sync::RwLock::new(std::sync::Arc::new(FileCredentialVault::default())),
        }
    }

    #[cfg(test)]
    pub fn set_database_path_for_tests(&mut self, path: std::path::PathBuf) {
        self.live_database_path = Some(path);
    }

    #[cfg(test)]
    pub fn set_active_user_for_tests(&self, user_id: &str) {
        let tenant_id = self.tenant_id();
        let now = Utc::now().to_rfc3339();
        let session_id = {
            let connection = self.connection.lock().expect("قفل الاتصال يجب أن يتاح");
            resolve_active_session(&connection, &tenant_id, user_id, &now)
        };
        self.activate_identity(&tenant_id, user_id, session_id)
            .expect("تبديل هوية الاختبار يجب أن ينجح");
    }

    #[cfg(test)]
    pub fn set_active_tenant_for_tests(&self, tenant_id: &str) {
        self.active
            .lock()
            .expect("قفل الهوية يجب أن يتاح")
            .replace_tenant_for_tests(tenant_id)
            .expect("تبديل مستأجر الاختبار يجب أن ينجح");
    }
}

impl ActiveIdentity {
    fn replace_with(
        &mut self,
        tenant_id: &str,
        user_id: &str,
        session_id: Option<String>,
    ) -> Result<(), AppError> {
        self.tenant_id = tenant_id.to_owned();
        self.user_id = user_id.to_owned();
        self.session_id = session_id;
        Ok(())
    }

    #[cfg(test)]
    fn replace_tenant_for_tests(&mut self, tenant_id: &str) -> Result<(), AppError> {
        // تبديل المستأجر يبطل الجلسة الحالية — ربط الجلسة بمستأجرها أمر جوهري.
        self.tenant_id = tenant_id.to_owned();
        self.session_id = None;
        Ok(())
    }
}

// حل جلسة نشطة صالحة لمستخدم داخل مستأجره — غير ملغاة وغير منتهية فقط.
pub fn resolve_active_session(
    connection: &Connection,
    tenant_id: &str,
    user_id: &str,
    now: &str,
) -> Option<String> {
    connection
        .query_row(
            "SELECT id FROM local_sessions
             WHERE tenant_id = ?1 AND user_id = ?2 AND revoked_at IS NULL AND expires_at > ?3
             ORDER BY issued_at DESC LIMIT 1",
            [tenant_id, user_id, now],
            |row| row.get::<_, String>(0),
        )
        .ok()
}

// تهيئة بيئة العمل الحقيقية: هجرات + كتالوج أذونات محايد للوضع + قراءة الوضع.
// البذر التجريبي لا يحدث هنا إطلاقاً — فقط عندما يكون الوضع ديمو فعلاً.
pub fn initialize_workspace_database(connection: &Connection) -> Result<RuntimeMode, AppError> {
    apply_migrations(connection)?;
    ensure_permission_catalog(connection)?;
    // كتالوج أدوار المؤسسة الإنتاجية يُضمن قبل أي قراءة للوحة: قاعدة رُقّيت
    // من إصدار سابق يجب أن تجد دور منفّذ العمل بلا خطوة ترحيل يدوية.
    ensure_production_role_catalog(connection)?;
    // مثيل الموفر المحلي المدمج: يُزار في كل إقلاع لكل مستأجر قائم (إدراج
    // تجاهلي)، لأن الهجرة 0023 تُطبَّق قبل وجود أي مستأجر على قاعدة جديدة —
    // فالبذر وقت الهجرة وحده يترك القاعدة الجديدة بلا مزود محلي.
    seed_built_in_local_providers(connection)?;
    let mode = read_runtime_mode(connection)?;
    if mode == RuntimeMode::Demo {
        seed_demo_tenant(connection)?;
        seed_demo_identity(connection)?;
        seed_demo_workflow(connection)?;
        seed_demo_governed_tool_runtime(connection)?;
        seed_demo_intelligence_runtime(connection)?;
        // بعد إنشاء مستأجر الديمو يصبح البذر ذا معنى له أيضاً — نفس الإدراج
        // التجاهلي فلا ازدواج ولا خطأ.
        seed_built_in_local_providers(connection)?;
    }
    Ok(mode)
}

// قراءة وضع التشغيل — صف وحيد؛ غيابه يعني قاعدة غير مهيأة (فشل مغلق إلى UNPROVISIONED).
pub fn read_runtime_mode(connection: &Connection) -> Result<RuntimeMode, AppError> {
    let mode: Option<String> = connection
        .query_row(
            "SELECT mode FROM app_runtime_state WHERE id = 1",
            [],
            |row| row.get(0),
        )
        .map(Some)
        .map_err(|_| AppError::DataAccess)?;
    Ok(RuntimeMode::from_database(
        mode.as_deref().unwrap_or("UNPROVISIONED"),
    ))
}

// الكتالوج العالمي للأذونات — محايد للوضع، إدراج تجاهلي عند كل إقلاع.
pub fn ensure_permission_catalog(connection: &Connection) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let permissions = [
        // ===== كتالوج المنتج الأصلي (MP01–MP04) =====
        (
            "permission_obligation_read",
            "obligation:read",
            "OBLIGATION",
            "READ",
        ),
        (
            "permission_attention_read",
            "attention:read",
            "ATTENTION",
            "READ",
        ),
        ("permission_risk_read", "risk:read", "RISK", "READ"),
        (
            "permission_evidence_read",
            "evidence:read",
            "EVIDENCE",
            "READ",
        ),
        (
            "permission_regulatory_intelligence_read",
            "regulatory_intelligence:read",
            "REGULATORY_INTELLIGENCE",
            "READ",
        ),
        (
            "permission_obligation_update",
            "obligation:update",
            "OBLIGATION",
            "UPDATE",
        ),
        (
            "permission_obligation_archive",
            "obligation:archive",
            "OBLIGATION",
            "ARCHIVE",
        ),
        ("permission_task_read", "task:read", "TASK", "READ"),
        ("permission_control_read", "control:read", "CONTROL", "READ"),
        ("permission_task_create", "task:create", "TASK", "CREATE"),
        (
            "permission_evidence_create",
            "evidence:create",
            "EVIDENCE",
            "CREATE",
        ),
        (
            "permission_approval_decide",
            "approval:decide",
            "APPROVAL",
            "DECIDE",
        ),
        ("permission_agent_read", "agent:read", "AGENT", "READ"),
        ("permission_agent_create", "agent:create", "AGENT", "CREATE"),
        ("permission_agent_update", "agent:update", "AGENT", "UPDATE"),
        (
            "permission_agent_execute",
            "agent:execute",
            "AGENT",
            "EXECUTE",
        ),
        (
            "permission_organization_read",
            "organization:read",
            "ORGANIZATION",
            "READ",
        ),
        ("permission_audit_read", "audit:read", "AUDIT", "READ"),
        (
            "permission_integration_read",
            "integration:read",
            "INTEGRATION",
            "READ",
        ),
        // أذونات الذكاء MP04 (كانت تُدرج في بذر الديمو فقط — أصبحت كتالوجاً عالمياً)
        (
            "permission_knowledge_read",
            "knowledge:read",
            "KNOWLEDGE",
            "READ",
        ),
        (
            "permission_knowledge_ingest",
            "knowledge:ingest",
            "KNOWLEDGE",
            "INGEST",
        ),
        (
            "permission_knowledge_disable",
            "knowledge:disable",
            "KNOWLEDGE",
            "DISABLE",
        ),
        ("permission_memory_read", "memory:read", "MEMORY", "READ"),
        (
            "permission_memory_manage",
            "memory:manage",
            "MEMORY",
            "MANAGE",
        ),
        ("permission_plan_read", "plan:read", "PLAN", "READ"),
        ("permission_plan_create", "plan:create", "PLAN", "CREATE"),
        ("permission_plan_execute", "plan:execute", "PLAN", "EXECUTE"),
        // ===== أذونات MP05: إدارة الوصول والنسخ الاحتياطي =====
        ("permission_user_read", "user:read", "USER", "READ"),
        ("permission_user_update", "user:update", "USER", "UPDATE"),
        // ===== 0026: إدارة المستخدمين الإنتاجية وتأليف بيانات الأعمال =====
        // إنشاء حساب عضو فعل مستقل عن تعديل عضوية قائم: من يعدّل لا يضيف
        // بالضرورة، ومن يضيف لا يعدّل تلقائياً. والإنشاء في الخلف يبقى مشروطاً
        // بأن لا يتجاوز الدورُ الممنوح صلاحيات المُسنِد.
        ("permission_user_create", "user:create", "USER", "CREATE"),
        // تأليف الالتزام فعل كتابة مستقل عن تحديثه (التحقق البشري منفصل عنهما).
        (
            "permission_obligation_create",
            "obligation:create",
            "OBLIGATION",
            "CREATE",
        ),
        // تسجيل المصدر التنظيمي الذي يقدمه المستخدم — لا يرقّيه النظام إلى رسمي.
        (
            "permission_regulatory_intelligence_create",
            "regulatory_intelligence:create",
            "REGULATORY_INTELLIGENCE",
            "CREATE",
        ),
        (
            "permission_backup_create",
            "backup:create",
            "BACKUP",
            "CREATE",
        ),
        (
            "permission_backup_restore",
            "backup:restore",
            "BACKUP",
            "RESTORE",
        ),
        // ===== أذونات دورة حياة الأدلة وتأليف الضوابط (0022) =====
        (
            "permission_evidence_update",
            "evidence:update",
            "EVIDENCE",
            "UPDATE",
        ),
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
        // ===== أذونات سجل الموفرين (0023) =====
        (
            "permission_integration_update",
            "integration:update",
            "INTEGRATION",
            "UPDATE",
        ),
        // صلاحية منفصلة تماماً لكتابة/حذف الأسرار. سبب الفصل: من يقرأ سجل
        // الموفرين (integration:read) لا يجوز أن يكتسب تلقائياً قدرة كتابة
        // مفتاح سرّي — الفصل هنا هو الضمانة، وتُمنح في بذر الديمو بوعي.
        (
            "permission_integration_credential",
            "integration:credential",
            "INTEGRATION",
            "CREDENTIAL",
        ),
        // ===== 0025: دورة حياة المهمة المحكومة =====
        // منفّذ العمل يحتاج task:update لينقل مهمته (بدء/تعليق/طلب مراجعة/إعادة
        // عمل بعد الرفض)، والإسناد صلاحية منفصلة task:assign لأن نقل مسؤولية
        // العمل إلى شخص آخر فعل إداري لا يورَث من قراءة المهام.
        ("permission_task_update", "task:update", "TASK", "UPDATE"),
        ("permission_task_assign", "task:assign", "TASK", "ASSIGN"),
        // ===== 0025: خطوة التحقق البشري في السلسلة المرجعية (§14) =====
        // صلاحية منفصلة تماماً: التحقق البشري فعل مراجعة لا فعل تنفيذ، ولا يجوز
        // أن يُورَّث من `obligation:update` الذي يملكه منفّذ العمل. وبدون هذا
        // المسار كانت `obligations.verification_state` لا تصل إلى
        // HUMAN_VERIFIED من أي أمر، فتبقى «تغطية تم التحقق منها» صفراً بنيوياً.
        (
            "permission_obligation_verify",
            "obligation:verify",
            "OBLIGATION",
            "VERIFY",
        ),
        // ===== أذونات التقارير المحلية =====
        // قراءة التقارير صلاحية مستقلة، والتصدير صلاحية منفصلة تماماً: التصدير
        // يكتب ملفاً على قرص المستخدم، فلا يجوز أن يُورَّث تلقائياً من القراءة.
        ("permission_report_read", "report:read", "REPORT", "READ"),
        (
            "permission_report_export",
            "report:export",
            "REPORT",
            "EXPORT",
        ),
    ];
    for (id, code, resource_type, action) in permissions {
        connection
            .execute(
                "INSERT OR IGNORE INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES (?1, ?2, ?3, ?4, 'HIGHLY_RESTRICTED', ?5)",
                (id, code, resource_type, action, &now),
            )
            .map_err(|_| AppError::StorageOperation)?;
    }
    Ok(())
}

// =====================================================================
// 0026 — كتالوج أدوار المؤسسة الإنتاجية
// ---------------------------------------------------------------------
// التأسيس كان ينشئ دور TENANT_ADMIN وحده، فلا يوجد في مساحة الإنتاج دور
// لمنفّذ العمل، فيستحيل إنشاء موظف بدور محدود. الكتالوج هنا هو الحد الأدنى
// المعلن: مشرف مستأجر (كل الأذونات، وهو أهلية قرار الموافقة القائمة)، ومنفّذ
// عمل (قراءة + نقل مهمته + تسجيل دليل، بلا أي سلطة اعتماد أو إدارة)، ومراجع
// (تحقق الأدلة والالتزامات وقرار الموافقة، بلا إدارة مستخدمين ولا تأليف).
// الدالة محايدة للمستأجر وتُنادى عند كل إقلاع وعند التأسيس داخل معاملته.
// =====================================================================

// أذونات دور منفّذ العمل: قراءات الوحدات المشتركة + تنفيذ مهمته + تسجيل دليل.
// لا توجد فيها أي سلطة اعتماد أو إدارة أو تأليف بيانات أعمال.
const CONTRIBUTOR_PERMISSION_CODES: &[&str] = &[
    "obligation:read",
    "attention:read",
    "risk:read",
    "evidence:read",
    "evidence:create",
    "regulatory_intelligence:read",
    "task:read",
    "task:update",
    "control:read",
    "agent:read",
    "organization:read",
    "integration:read",
    "knowledge:read",
    "memory:read",
    "plan:read",
    "audit:read",
    "report:read",
];

// أذونات دور المراجع: التحقق من الأدلة وتأكيد الالتزام وقرار الموافقة، بلا أي
// سلطة إدارة مستخدمين وبلا تأليف بيانات أعمال. الفصل مقصود: كان الدور الوحيد
// الذي يملك `evidence:update` هو مشرف المستأجر الذي يملك أيضاً إدارة
// المستخدمين و`user:update`، فكانت صلاحية تعديل الدليل مساوية تلقائياً
// لصلاحية إدارة المؤسسة كلها. هذا الدور يفصل المراجعة عن الإدارة.
const REVIEWER_PERMISSION_CODES: &[&str] = &[
    "obligation:read",
    "attention:read",
    "risk:read",
    "evidence:read",
    // تعديل حالة الدليل = قرار تحقق في المراجعة فقط.
    "evidence:update",
    // التحقق البشري من الالتزام وقرار الموافقة: فعلا مراجعة لا تنفيذ.
    "obligation:verify",
    "approval:decide",
    "regulatory_intelligence:read",
    "task:read",
    "control:read",
    "agent:read",
    "organization:read",
    "integration:read",
    "knowledge:read",
    "memory:read",
    "plan:read",
    "audit:read",
    "report:read",
];

pub(crate) fn ensure_production_role_catalog(connection: &Connection) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    let mut statement = connection
        .prepare("SELECT id FROM tenants WHERE data_mode = 'PRODUCTION' ORDER BY id ASC")
        .map_err(|_| AppError::StorageOperation)?;
    let tenant_ids = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| AppError::StorageOperation)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::StorageOperation)?;
    drop(statement);

    for tenant_id in tenant_ids {
        let contributor_role_id = format!("role_{tenant_id}_contributor");
        // معرّف حتمي مشتق من المستأجر: إعادة التشغيل لا تُنشئ دوراً ثانياً.
        connection
            .execute(
                "INSERT OR IGNORE INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES (?1, ?2, 'CONTRIBUTOR', 'Contributor', ?3, ?3)",
                params![&contributor_role_id, &tenant_id, &now],
            )
            .map_err(|_| AppError::StorageOperation)?;
        for code in CONTRIBUTOR_PERMISSION_CODES {
            connection
                .execute(
                    "INSERT OR IGNORE INTO role_permissions (role_id, permission_id)
                     SELECT ?1, id FROM permissions WHERE code = ?2",
                    params![&contributor_role_id, code],
                )
                .map_err(|_| AppError::StorageOperation)?;
        }

        // دور المراجع: التحقق والقرار بلا إدارة مستخدمين ولا تأليف بيانات.
        // الإدراج تجاهلي فإعادة الإقلاع لا تكرر الأدوار ولا المنح.
        let reviewer_role_id = format!("role_{tenant_id}_reviewer");
        connection
            .execute(
                "INSERT OR IGNORE INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES (?1, ?2, 'REVIEWER', 'Reviewer', ?3, ?3)",
                params![&reviewer_role_id, &tenant_id, &now],
            )
            .map_err(|_| AppError::StorageOperation)?;
        for code in REVIEWER_PERMISSION_CODES {
            connection
                .execute(
                    "INSERT OR IGNORE INTO role_permissions (role_id, permission_id)
                     SELECT ?1, id FROM permissions WHERE code = ?2",
                    params![&reviewer_role_id, code],
                )
                .map_err(|_| AppError::StorageOperation)?;
        }
    }
    Ok(())
}

// تهيئة قاعدة بيانات الديمو كاملة (هجرات + وضع ديمو + بذر) — لحوامل الاختبار حصراً؛
// مسار المنتج يمر عبر initialize_workspace_database أو start_demo_experience.
#[cfg(test)]
pub fn initialize_demo_database(connection: &Connection) -> Result<(), AppError> {
    apply_migrations(connection)?;
    ensure_permission_catalog(connection)?;
    // الانتقال الصريح إلى الديمو — لا يلزم إلا قاعدة غير مهيأة (قواعد الاختبار الطازجة).
    connection
        .execute(
            "UPDATE app_runtime_state SET mode = 'DEMO', updated_at = ?1 WHERE id = 1 AND mode = 'UNPROVISIONED'",
            [Utc::now().to_rfc3339()],
        )
        .map_err(|_| AppError::StorageOperation)?;
    seed_demo_tenant(connection)?;
    seed_demo_identity(connection)?;
    seed_demo_workflow(connection)?;
    seed_demo_governed_tool_runtime(connection)?;
    seed_demo_intelligence_runtime(connection)?;
    seed_built_in_local_providers(connection)?;
    Ok(())
}

// بذر مثيل Ollama المحلي المدمج لكل مستأجر قائم — إدراج تجاهلي (INSERT OR IGNORE
// على قيد التفرّد (tenant_id, provider_id)) فيمكن استدعاؤه في كل إقلاع بأمان.
// الحالة تبقى UNVERIFIED ولا علامة سر: لا يدّعي المخطط اتصالاً لم يُختبر.
pub(crate) fn seed_built_in_local_providers(connection: &Connection) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    connection
        .execute(
            "INSERT OR IGNORE INTO ai_provider_registry
               (id, tenant_id, provider_id, kind, display_name, base_url, transport,
                enabled, connection_state, connection_checked_at, credential_configured,
                built_in, created_at, updated_at)
             SELECT
               'ai_provider_' || tenants.id || '_ollama_local',
               tenants.id,
               'OLLAMA_LOCAL',
               'OLLAMA',
               'Ollama (Local)',
               'http://127.0.0.1:11434',
               'HTTP_LOOPBACK',
               0,
               'UNVERIFIED',
               NULL,
               0,
               1,
               tenants.created_at,
               ?1
             FROM tenants",
            [&now],
        )
        .map_err(|_| AppError::StorageOperation)?;
    Ok(())
}

pub(crate) fn apply_migrations(connection: &Connection) -> Result<(), AppError> {
    let migrations = [
        (1_i32, include_str!("../../migrations/0001_initial.sql")),
        (
            2_i32,
            include_str!("../../migrations/0002_identity_and_access.sql"),
        ),
        (
            3_i32,
            include_str!("../../migrations/0003_workflow_and_risk.sql"),
        ),
        (
            4_i32,
            include_str!("../../migrations/0004_evidence_metadata.sql"),
        ),
        (
            5_i32,
            include_str!("../../migrations/0005_ai_governance.sql"),
        ),
        (
            6_i32,
            include_str!("../../migrations/0006_correct_demo_truth_labels.sql"),
        ),
        (
            7_i32,
            include_str!("../../migrations/0007_obligation_management.sql"),
        ),
        (
            8_i32,
            include_str!("../../migrations/0008_obligation_relationship_update_isolation.sql"),
        ),
        (
            9_i32,
            include_str!("../../migrations/0009_controls_read_model.sql"),
        ),
        (
            10_i32,
            include_str!("../../migrations/0010_agent_foundation.sql"),
        ),
        (11_i32, include_str!("../../migrations/0011_agent_runs.sql")),
        (
            12_i32,
            include_str!("../../migrations/0012_agent_run_approval_lifecycle.sql"),
        ),
        (
            13_i32,
            include_str!("../../migrations/0013_agent_execution_results.sql"),
        ),
        (
            14_i32,
            include_str!("../../migrations/0014_governed_skills.sql"),
        ),
        (
            15_i32,
            include_str!("../../migrations/0015_agent_run_skill_bindings.sql"),
        ),
        (
            16_i32,
            include_str!("../../migrations/0016_governed_tools.sql"),
        ),
        (
            17_i32,
            include_str!("../../migrations/0017_governed_agent_tool_runtime.sql"),
        ),
        (
            18_i32,
            include_str!("../../migrations/0018_knowledge_foundation.sql"),
        ),
        (
            19_i32,
            include_str!("../../migrations/0019_memory_foundation.sql"),
        ),
        (
            20_i32,
            include_str!("../../migrations/0020_intelligence_runtime.sql"),
        ),
        (
            21_i32,
            include_str!("../../migrations/0021_production_identity_and_backup.sql"),
        ),
        (
            22_i32,
            include_str!("../../migrations/0022_controls_authoring.sql"),
        ),
        // 0025 — دورة حياة المهمة المحكومة. إعادة بناء جدول tasks وحده، ولا جدول
        // في المخطط يشير إليه بمفتاح أجنبي، فلا حاجة لتعطيل فرض المفاتيح كما في 0023.
        (
            25_i32,
            include_str!("../../migrations/0025_task_lifecycle.sql"),
        ),
        // 0026 — فصل مقدّم الدليل عن مقرر حالته (عمود واحد بلا إعادة بناء جدول).
        (
            26_i32,
            include_str!("../../migrations/0026_production_user_and_authoring.sql"),
        ),
    ];

    for (version, script) in migrations {
        let migration_exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
                [version],
                |row| row.get::<_, bool>(0),
            )
            .unwrap_or(false);

        if migration_exists {
            continue;
        }

        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        transaction
            .execute_batch(script)
            .map_err(|_| AppError::StorageOperation)?;
        transaction
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                (version, Utc::now().to_rfc3339()),
            )
            .map_err(|_| AppError::StorageOperation)?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
        crate::infrastructure::local_logger::debug(
            "MIGRATION_APPLIED",
            "",
            &format!("{{\"version\":{version}}}"),
        );
    }

    // 0023 خارج الحلقة: تحتاج تعطيل فرض المفاتيح الأجنبية على الاتصال قبل بدء
    // المعاملة (وهو ما لا يمكن فعله داخل معاملة قائمة)، ثم إعادة التفعيل والتحقق.
    let provider_registry_applied = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = 23)",
            [],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false);
    if !provider_registry_applied {
        apply_ai_provider_registry_rebuild(connection)?;
        crate::infrastructure::local_logger::debug("MIGRATION_APPLIED", "", "{\"version\":23}");
    }

    // 0024 — ربط هوية وجهة التنفيذ بالموافقة. هجرة إضافية بحتة (ALTER TABLE على
    // جدول 0017)، فتكفيها معاملة عادية بعد 0023 لأنها لا تمسّ ai_model_registry
    // ولا تحتاج تعطيل فرض المفاتيح الأجنبية.
    let approval_binding_applied = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = 24)",
            [],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false);
    if !approval_binding_applied {
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        transaction
            .execute_batch(include_str!(
                "../../migrations/0024_provider_approval_binding.sql"
            ))
            .map_err(|_| AppError::StorageOperation)?;
        transaction
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                (24_i32, Utc::now().to_rfc3339()),
            )
            .map_err(|_| AppError::StorageOperation)?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
        crate::infrastructure::local_logger::debug("MIGRATION_APPLIED", "", "{\"version\":24}");
    }

    Ok(())
}

// =====================================================================
// هجرة 0023 — سجل موفري الذكاء الاصطناعي
// ---------------------------------------------------------------------
// لماذا لا يكفي تنفيذ السكربت داخل المعاملة العادية مثل بقية الهجرات:
//   إسقاط ai_model_registry (خطوة إعادة البناء) يفشل فوراً بـ 1811
//   (SQLITE_CONSTRAINT_FOREIGNKEY) لأن ai_analysis_runs.model_registry_id
//   يشير إليه. وPRAGMA foreign_keys **لا أثر له داخل معاملة قائمة** (يُتجاهل
//   صامتاً بموجب توثيق SQLite)، وPRAGMA defer_foreign_keys يظهر كـ 1 لكن
//   الفحص يبقى فورياً في هذا المسار على SQLite 3.48.0 المُدمج.
// الحل: تعطيل الفرض على الاتصال، ثم إعادة البناء في معاملة واحدة، ثم إعادة
// التفعيل **والتحقق** — فشل التحقق يعني فشل الهجرة كاملة (فشل مغلق).
//
// PRAGMA legacy_alter_table=ON يمنع SQLite من إعادة كتابة تعريف المفتاح
// الأجنبي في ai_analysis_runs عند إعادة التسمية، فيبقى مرجعه ai_model_registry.
// =====================================================================
fn apply_ai_provider_registry_rebuild(connection: &Connection) -> Result<(), AppError> {
    // لا خروج مبكر هنا: السكربت يحمل بذر الموفر المدمج ويجب أن يُنفَّذ دائماً.
    // الفرعان:
    //   * القاعدة قديمة (بلا provider_id) ⇒ إسقاط الجدول القديم ثم إعادة تسمية
    //     الجديد (المرشَّح) بعد نسخ الصفوف إليه.
    //   * القاعدة مُرقّاة جزئياً (provider_id موجود) ⇒ إسقاط الجدول المرشَّح
    //     الفارغ فقط، بلا أي مساس بالبيانات الحقيقية.
    // السبب في أن الفرع في Rust لا في SQL: SQLite **لا يقبل WHERE على جمل
    // DDL** (`DROP TABLE ... WHERE` يُنتج خطأ نحو)، ولا يملك `IF` على DDL.
    let needs_rebuild = !column_exists(connection, "ai_model_registry", "provider_id")?;

    connection
        .pragma_update(None, "legacy_alter_table", "ON")
        .map_err(|_| AppError::StorageOperation)?;
    // لا أثر لهذا السطر إن كانت معاملة مفتوحة — ولهذا يُستدعى قبل فتح أي معاملة.
    connection
        .pragma_update(None, "foreign_keys", "OFF")
        .map_err(|_| AppError::StorageOperation)?;

    let rebuild = (|| -> Result<(), AppError> {
        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;

        // 1) الجدول المرشَّح بالشكل المعمَّم + جدول الموفرين + بذر الموفر المدمج
        //    + ربط النماذج المحلية التاريخية بالمرشَّح.
        transaction
            .execute_batch(include_str!(
                "../../migrations/0023_ai_provider_registry.sql"
            ))
            .map_err(|_| AppError::StorageOperation)?;

        // 2) نسخ صفوف النماذج إلى المرشَّح — **قبل** أي إسقاط، وإلا فقدنا البيانات.
        //    الفرعان ضروريان: على قاعدة قديمة لا وجود لعمود provider_id فلا
        //    يُذكر في SELECT؛ وعلى قاعدة جديدة الجدول فارغ بلا حاجة إلى نسخ.
        if needs_rebuild {
            transaction
                .execute_batch(
                    "INSERT INTO ai_model_registry_new (
                       id, tenant_id, provider_kind, model_id, display_name, enabled,
                       data_residency, allowed_classification_ceiling, requires_human_approval,
                       created_at, updated_at, provider_id, context_limit, supports_tools,
                       supports_structured_output, supports_vision, capability_source, available,
                       last_discovered_at
                     )
                     SELECT
                       id, tenant_id, provider_kind, model_id, display_name, enabled,
                       data_residency, allowed_classification_ceiling, requires_human_approval,
                       created_at, updated_at, NULL, NULL, NULL,
                       NULL, NULL, 'UNKNOWN', enabled,
                       NULL
                     FROM ai_model_registry;",
                )
                .map_err(|_| AppError::StorageOperation)?;
        }

        // 3) ربط صفوف النماذج المحلية التاريخية بمثيل Ollama المزروع — **بعد**
        //    النسخ (وإلا كان المرشَّح فارغاً). المعرّف يُمرَّر كوسيط لا كنص مُدمج.
        transaction
            .execute(
                "UPDATE ai_model_registry_new
                 SET provider_id = (
                   SELECT providers.id FROM ai_provider_registry providers
                   WHERE providers.tenant_id = ai_model_registry_new.tenant_id
                     AND providers.provider_id = 'OLLAMA_LOCAL'
                 )
                 WHERE provider_id IS NULL
                   AND provider_kind = 'LOCAL'
                   AND EXISTS (
                     SELECT 1 FROM ai_provider_registry providers
                     WHERE providers.tenant_id = ai_model_registry_new.tenant_id
                       AND providers.provider_id = ?1
                   )",
                ["OLLAMA_LOCAL"],
            )
            .map_err(|_| AppError::StorageOperation)?;

        // 4) الاستبدال الفعلي بحسب حالة القاعدة قبل الهجرة.
        if needs_rebuild {
            transaction
                .execute_batch("DROP TABLE ai_model_registry;")
                .map_err(|_| AppError::StorageOperation)?;
        } else {
            // القاعدة مُرقّاة جزئياً: المرشَّح فارغ ولم يُنسخ إليه شيء.
            transaction
                .execute_batch("DROP TABLE IF EXISTS ai_model_registry_new;")
                .map_err(|_| AppError::StorageOperation)?;
        }
        transaction
            .execute_batch("ALTER TABLE ai_model_registry_new RENAME TO ai_model_registry;")
            .map_err(|_| AppError::StorageOperation)?;

        // 3) الفهارس بعد الاستبدال (الفهارس تسقط مع الجدول القديم).
        transaction
            .execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_ai_models_tenant_enabled
                   ON ai_model_registry (tenant_id, enabled);
                 CREATE INDEX IF NOT EXISTS idx_ai_models_provider
                   ON ai_model_registry (tenant_id, provider_id);
                 -- انتخاب واحد فقط: فهرس فريد جزئي يمنع وجود نموذجين مفعّلين.
                 CREATE UNIQUE INDEX IF NOT EXISTS idx_ai_models_single_selection
                   ON ai_model_registry (tenant_id)
                   WHERE enabled = 1;",
            )
            .map_err(|_| AppError::StorageOperation)?;

        transaction
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                (23_i32, Utc::now().to_rfc3339()),
            )
            .map_err(|_| AppError::StorageOperation)?;
        transaction.commit().map_err(|_| AppError::StorageOperation)
    })();

    // إعادة التفعيل في كل الأحوال — لا يجوز أن يبقى الاتصال بلا فرض مفاتيح.
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .map_err(|_| AppError::StorageOperation)?;
    connection
        .pragma_update(None, "legacy_alter_table", "OFF")
        .map_err(|_| AppError::StorageOperation)?;

    rebuild?;

    // تحقق إلزامي بعد الالتزام: أي انتهاك يعني فشل الهجرة لا تمريره بصمت.
    let violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(|_| AppError::StorageOperation)?;
    if violations != 0 {
        crate::infrastructure::local_logger::error("MIGRATION_INTEGRITY_FAILED", "", "");
        return Err(AppError::StorageOperation);
    }

    Ok(())
}

// هل يحمل الجدول هذا العمود؟ قراءة مخطط فقط — لا تلمس البيانات.
fn column_exists(connection: &Connection, table: &str, column: &str) -> Result<bool, AppError> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
            (table, column),
            |row| row.get(0),
        )
        .map_err(|_| AppError::StorageOperation)?;

    Ok(count > 0)
}

pub(crate) fn seed_demo_tenant(connection: &Connection) -> Result<(), AppError> {
    let tenant_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM tenants WHERE id = ?1)",
            ["tenant_demo_sa_001"],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| AppError::StorageOperation)?;

    if tenant_exists {
        return Ok(());
    }

    let now = Utc::now().to_rfc3339();
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    transaction
        .execute(
            "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?2, 'DEMO_SYNTHETIC', ?3, ?3)",
            ("tenant_demo_sa_001", "NEXUS Demo Saudi Tenant", &now),
        )
        .map_err(|_| AppError::StorageOperation)?;
    transaction
        .execute(
            "INSERT INTO organizations (id, tenant_id, legal_name, country_code, industry, created_at, updated_at) VALUES (?1, ?2, ?3, 'SA', ?4, ?5, ?5)",
            ("org_demo_001", "tenant_demo_sa_001", "NEXUS DEMO / SYNTHETIC Organization", "Technology Services", &now),
        )
        .map_err(|_| AppError::StorageOperation)?;

    let sources = [
        (
            "source_demo_pdpl",
            "DEMO / SYNTHETIC — Data Protection Authority",
            "Synthetic PDPL readiness reference",
            "https://example.invalid/demo/pdpl",
            "DEMO-1.0",
            "Section 4",
            false,
        ),
        (
            "source_demo_ecc",
            "DEMO / SYNTHETIC — Cybersecurity Authority",
            "Synthetic cybersecurity control reference",
            "https://example.invalid/demo/ecc",
            "DEMO-1.0",
            "Control 1.2",
            false,
        ),
        (
            "source_demo_zatca",
            "DEMO / SYNTHETIC — E-Invoicing Authority",
            "Synthetic e-invoicing reference",
            "https://example.invalid/demo/zatca",
            "DEMO-1.0",
            "Clause 2",
            false,
        ),
    ];

    for (id, authority, title, url, version, section, official) in sources {
        transaction
            .execute(
                "INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'REQUIRES_REVIEW', ?7, ?7)",
                (id, "tenant_demo_sa_001", authority, title, url, version, &now, official),
            )
            .map_err(|_| AppError::StorageOperation)?;
        let _ = section;
    }

    let obligations = [
        (
            "obl_demo_001",
            "source_demo_pdpl",
            "Review the synthetic data inventory evidence",
            "Privacy Officer",
            "2026-08-20T00:00:00Z",
            "CRITICAL",
            "AT_RISK",
            "APPLIES",
            "MISSING",
            "REQUIRES_REVIEW",
        ),
        (
            "obl_demo_002",
            "source_demo_ecc",
            "Validate synthetic privileged access review evidence",
            "Security Officer",
            "2026-08-24T00:00:00Z",
            "HIGH",
            "IN_PROGRESS",
            "APPLIES",
            "UPLOADED",
            "AI_ANALYZED",
        ),
        (
            "obl_demo_003",
            "source_demo_zatca",
            "Confirm synthetic e-invoicing integration record",
            "Finance Operations",
            "2026-09-01T00:00:00Z",
            "MEDIUM",
            "OPEN",
            "REQUIRES_REVIEW",
            "REQUIRES_REVIEW",
            "UNVERIFIED",
        ),
        (
            "obl_demo_004",
            "source_demo_ecc",
            "Retain synthetic incident response exercise record",
            "Security Officer",
            "2026-09-10T00:00:00Z",
            "LOW",
            "COMPLETED",
            "APPLIES",
            "VALID",
            "HUMAN_VERIFIED",
        ),
    ];

    for (
        id,
        source_id,
        title,
        owner,
        deadline,
        severity,
        status,
        applicability,
        evidence_status,
        verification_state,
    ) in obligations
    {
        transaction
            .execute(
                "INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, source_type, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'SYNTHETIC', ?12, ?12)",
                (id, "tenant_demo_sa_001", source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, &now),
            )
            .map_err(|_| AppError::StorageOperation)?;
    }

    let first_event_hash = append_audit_event(
        &transaction,
        "tenant_demo_sa_001",
        "audit_demo_001",
        "DEMO_DATA_INITIALIZED",
        "NEXUS System",
        "Tenant",
        "tenant_demo_sa_001",
        &now,
        "",
    )?;
    let second_event_hash = append_audit_event(
        &transaction,
        "tenant_demo_sa_001",
        "audit_demo_002",
        "OBLIGATION_CREATED",
        "NEXUS Demo Administrator",
        "Obligation",
        "obl_demo_001",
        &now,
        &first_event_hash,
    )?;
    append_audit_event(
        &transaction,
        "tenant_demo_sa_001",
        "audit_demo_003",
        "EVIDENCE_REQUIRES_REVIEW",
        "NEXUS Demo Administrator",
        "Evidence",
        "obl_demo_003",
        &now,
        &second_event_hash,
    )?;

    transaction.commit().map_err(|_| AppError::StorageOperation)
}

pub(crate) fn seed_demo_identity(connection: &Connection) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();

    // كتالوج الصلاحيات أصبح محايداً للوضع (ensure_permission_catalog) —
    // هذا البذر يقتصر على هويات الديمو وأدوارها وجلساتها.

    // الهويات الديمو التركيبية: مشغّل (طالب) ومراجِع (صاحب قرار) داخل المستأجر الديمو نفسه،
    // مع إبقاء هوية المسؤول القديمة صالحة لأن بيانات سير العمل المزروعة تشير إليها.
    let demo_users = [
        (
            DEMO_ADMIN_USER_ID,
            "NEXUS Demo Administrator",
            "demo.admin@nexus.local",
        ),
        (
            DEMO_OPERATOR_USER_ID,
            "NEXUS Demo Operator (Requester)",
            "demo.operator@nexus.local",
        ),
        (
            DEMO_REVIEWER_USER_ID,
            "NEXUS Demo Reviewer",
            "demo.reviewer@nexus.local",
        ),
    ];
    for (id, display_name, email) in demo_users {
        connection
            .execute(
                "INSERT OR IGNORE INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, NULL, 'ACTIVE', ?5, ?5)",
                (id, DEMO_TENANT_ID, display_name, email, &now),
            )
            .map_err(|_| AppError::StorageOperation)?;
    }

    // الأدوار: دور الطالب يملك بدء سير العمل (بلا أي قدرة اعتماد)،
    // ودور TENANT_ADMIN يمثل أهلية المراجعة المطلوبة في دورة الموافقات الحالية.
    let demo_roles = [
        (
            "role_demo_tenant_admin",
            "TENANT_ADMIN",
            "Demo Approval Reviewer (Tenant Admin)",
        ),
        (
            "role_demo_requester",
            "DEMO_REQUESTER",
            "Demo Workflow Requester",
        ),
    ];
    for (id, code, display_name) in demo_roles {
        connection
            .execute(
                "INSERT OR IGNORE INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                (id, DEMO_TENANT_ID, code, display_name, &now),
            )
            .map_err(|_| AppError::StorageOperation)?;
    }

    let requester_write_permissions = [
        "permission_obligation_update",
        "permission_obligation_archive",
        "permission_task_create",
        // 0025: نقل حالة المهمة فعل تنفيذي يملكه من يحمل العمل. يملكه الطالب
        // هنا، ويملكه المراجِع أيضاً (انظر reviewer_admin_grants) لأنه قد يكون
        // مُسنَداً إليه عمل بنفسه. أما **الإسناد** فلا يملكه الطالب.
        "permission_task_update",
        "permission_evidence_create",
        // صلاحيات دورة حياة الأدلة وتأليف الضوابط (0022) — كتابة محضة بلا أي سلطة اعتماد.
        // ملاحظة 0026: `evidence:update` لم تبقَ هنا. قرار حالة التحقق من الدليل
        // فعل مراجعة، فانتقل إلى دور المراجِع وحده، ويبقى ضابط الخلف رافضاً
        // لمن قدّم الدليل أن يقرر حالته حتى لو ملك الصلاحية.
        "permission_control_create",
        "permission_control_update",
        "permission_control_disable",
        "permission_control_link",
        "permission_agent_create",
        "permission_agent_update",
        "permission_agent_execute",
        // الحد الأدنى لإدارة تكوين تنفيذ الوكلاء (MP03): سجلات المهارات/الأدوات
        // (تعيين أدوات مضاف) وتكوين النموذج المحلي — ولا شيء منها صلاحية اعتماد.
        "permission_skill_read",
        "permission_skill_create",
        "permission_skill_update",
        "permission_skill_assign",
        "permission_tool_read",
        "permission_tool_create",
        "permission_tool_update",
        "permission_tool_assign",
        "permission_integration_update",
        // ===== 0023: صلاحية كتابة أسرار الموفرين =====
        // تُمنح **للطالب وحده** ضمن بذر الديمو (انظر الملاحظة عند منح المراجِع):
        // قراءة سجل الموفرين لا تكفي لكتابة مفتاح سرّي، والقرار هنا صريح.
        "permission_integration_credential",
        // ===== التقارير المحلية: صلاحية تصدير الملفات =====
        // التصدير يكتب ملفاً على قرص المستخدم، وتُمنح **للطالب (المشغّل) وحده**
        // ضمن بذر الديمو بنفس منطق فصل السلطة أعلاه؛ المراجِع يقرأ التقارير
        // ويقرر ولا يكتب ملفات. وهي مسجّلة في قائمة الاستبعاد أدناه صراحةً
        // حتى لا يسحبها تصحيح دور المراجِع سهواً في قواعد البيانات القديمة.
        "permission_report_export",
    ];

    // ملاحظة 0023 (فصل السلطة): صلاحيتا integration:update و integration:credential
    // تُحذفان صراحةً من دور المراجِع role_demo_tenant_admin أدناه، فلا يورثها
    // من قراءة التكاملات. النتيجة النهائية في الديمو:
    //   role_demo_requester      → integration:read + update + credential
    //   role_demo_tenant_admin   → integration:read فقط (بلا update وبلا credential)
    // أي: من يقرأ سجل الموفرين لا يستطيع كتابة سر ولا تعديل إعداد.
    // وينطبق الشيء نفسه على التقارير: report:export تُسحب من المراجِع صراحةً
    // فيبقى له report:read وحده — قرار فصل سلطة معلن لا وراثة صامتة.

    // تسوية قواعد البيانات القديمة: دور TENANT_ADMIN الديمو يُختزل إلى صلاحيات المراجعة فقط
    // حتى لا يرث المراجِع منح كتابة أوسع من دور أصحاب القرار في المصفوفة الحالية.
    for permission_id in requester_write_permissions {
        connection
            .execute(
                "DELETE FROM role_permissions WHERE role_id = 'role_demo_tenant_admin' AND permission_id = ?1",
                [permission_id],
            )
            .map_err(|_| AppError::StorageOperation)?;
    }
    // 0026: `evidence:update` كانت ممنوحة للطالب (وهذا أصل UAT-04). تُسحب منه
    // ومن دور المراجِع أولاً (قواعد قديمة)، ثم تُمنح للمراجِع صراحةً في قائمة
    // منحه أدناه — قرار فصل صلاحية معلن لا وراثة صامتة.
    connection
        .execute(
            "DELETE FROM role_permissions WHERE role_id = 'role_demo_requester' AND permission_id = 'permission_evidence_update'",
            [],
        )
        .map_err(|_| AppError::StorageOperation)?;
    connection
        .execute(
            "DELETE FROM role_permissions WHERE role_id = 'role_demo_tenant_admin' AND permission_id = 'permission_evidence_update'",
            [],
        )
        .map_err(|_| AppError::StorageOperation)?;

    let read_permission_ids = [
        "permission_obligation_read",
        "permission_attention_read",
        "permission_risk_read",
        "permission_evidence_read",
        "permission_regulatory_intelligence_read",
        "permission_task_read",
        "permission_control_read",
        "permission_agent_read",
        // قراءات الوحدات الجديدة تُمنح للفاعلين معاً — كلها مستوى قراءة فقط
        // ولا تتجاوز ما تكشفه اللوحة أصلاً لأي قارئ مأذون.
        "permission_organization_read",
        "permission_audit_read",
        "permission_integration_read",
        // قراءة التقارير المحلية: سطح قراءة محض لا يمنح أي قدرة كتابة،
        // فيُمنح لكلا فاعلي الديمو كبقية القراءات.
        "permission_report_read",
    ];

    // المراجِع: قراءة + قرار الموافقة + إدارة الوصول/النسخ (MP05) لتجربة الإدارة في الديمو.
    let reviewer_admin_grants = [
        "permission_user_read",
        "permission_user_update",
        "permission_backup_create",
        "permission_backup_restore",
        // تقييم فعالية الضابط فعل مراجعة صريح — يُمنح للمراجِع ولا يُمنح للطالب،
        // ولا يمنح أي قدرة تأليف أو ربط أو تعطيل (تلك في requester_write_permissions حصراً).
        "permission_control_assess",
        // 0025: المراجِع مديرُ أعمال في هذا النموذج — يُسنِد المهام وينقل حالتها
        // عندما يكون هو المُسنَد إليه. وtask:update مُدرَجة في قائمة الاستبعاد
        // أعلاه ثم تُعاد هنا صراحةً، فالقرار معلن لا موروث من قراءة المهام.
        "permission_task_update",
        "permission_task_assign",
        // 0025: التحقق البشري من الالتزام — لجهة المراجعة وحدها. منفّذ العمل
        // (الطالب) لا يملكه، فلا يُصدّق على عمله بنفسه.
        "permission_obligation_verify",
        // 0026: قرار حالة التحقق من الدليل فعل مراجعة مستقل عن تقديمه.
        "permission_evidence_update",
    ];
    for permission_id in read_permission_ids
        .iter()
        .chain(std::iter::once(&"permission_approval_decide"))
        .chain(reviewer_admin_grants.iter())
    {
        connection
            .execute(
                "INSERT OR IGNORE INTO role_permissions (role_id, permission_id) VALUES ('role_demo_tenant_admin', ?1)",
                [permission_id],
            )
            .map_err(|_| AppError::StorageOperation)?;
    }

    // الطالب: كل القراءات + ما يلزمه لبدء سير العمل (مهام/أدلة/وكلاء/تنفيذ) وبلا أي صلاحية اعتماد.
    for permission_id in read_permission_ids
        .iter()
        .chain(requester_write_permissions.iter())
    {
        connection
            .execute(
                "INSERT OR IGNORE INTO role_permissions (role_id, permission_id) VALUES ('role_demo_requester', ?1)",
                [permission_id],
            )
            .map_err(|_| AppError::StorageOperation)?;
    }

    // إسنادات الأدوار: المشغّل طالب، والمراجِع والمسؤول القديم صاحبا أهلية القرار.
    for (user_id, role_id) in [
        (DEMO_ADMIN_USER_ID, "role_demo_tenant_admin"),
        (DEMO_OPERATOR_USER_ID, "role_demo_requester"),
        (DEMO_REVIEWER_USER_ID, "role_demo_tenant_admin"),
    ] {
        connection
            .execute(
                "INSERT OR IGNORE INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, ?2, ?3, 'NEXUS System')",
                (user_id, role_id, &now),
            )
            .map_err(|_| AppError::StorageOperation)?;
    }

    // جلسات نشطة لكل فاعل ديمو — مسار التفويض يرفض أي هوية بلا جلسة صالحة.
    for (session_id, user_id) in [
        ("session_demo_local", DEMO_ADMIN_USER_ID),
        ("session_demo_operator", DEMO_OPERATOR_USER_ID),
        ("session_demo_reviewer", DEMO_REVIEWER_USER_ID),
    ] {
        connection
            .execute(
                "INSERT OR IGNORE INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, ?2, ?3, ?4, '2030-01-01T00:00:00Z', NULL, 'NEXUS Local Demo Device')",
                (session_id, DEMO_TENANT_ID, user_id, &now),
            )
            .map_err(|_| AppError::StorageOperation)?;
    }
    Ok(())
}

pub(crate) fn seed_demo_workflow(connection: &Connection) -> Result<(), AppError> {
    let workflow_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id = 'task_demo_001')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| AppError::StorageOperation)?;
    if workflow_exists {
        return Ok(());
    }

    let now = Utc::now().to_rfc3339();
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;
    transaction
        .execute(
            "INSERT INTO risks (id, tenant_id, obligation_id, title, severity, likelihood, impact, status, owner, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'HIGH', 4, 4, 'OPEN', ?5, ?6, ?6)",
            ("risk_demo_001", "tenant_demo_sa_001", "obl_demo_001", "Synthetic evidence gap exposure", "Risk Manager", &now),
        )
        .map_err(|_| AppError::StorageOperation)?;
    transaction
        .execute(
            "INSERT INTO tasks (id, tenant_id, obligation_id, title, description, assignee_user_id, due_at, priority, status, version, completed_at, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'CRITICAL', 'OPEN', 1, NULL, ?8, ?8)",
            ("task_demo_001", "tenant_demo_sa_001", "obl_demo_001", "Prepare synthetic data inventory evidence", "Demonstration task generated from a synthetic evidence gap.", DEMO_OPERATOR_USER_ID, "2026-08-20T00:00:00Z", &now),
        )
        .map_err(|_| AppError::StorageOperation)?;
    // 0025: لا موافقة عند الإنشاء. كانت الهجرة السابقة تبذر موافقة معلّقة على
    // مهمة حالتها OPEN، وهي موافقة لا يمكن أن تُقرَّر أبداً (القرار يستلزم
    // READY_FOR_REVIEW) فتظل معلّقة إلى الأبد وتُضخّم قائمة الاعتمادات المعلّقة
    // وقائمة الانتباه ببند لا معنى له. الموافقة تُنشأ الآن عند طلب المراجعة حصراً.
    let previous_hash = latest_audit_event_hash(&transaction, "tenant_demo_sa_001")?;
    // UAT-05: مهمة الديمو المزروعة كانت تُنشأ بلا حدث إنشاء، فيظهر سجلها ناقصاً
    // عند أول مهمة في مساحة العمل. الحدث هنا ليس تزويراً لتاريخ ماضٍ: البذر نفسه
    // هو لحظة إنشاء الصف، والطابع الزمني وقت البذر الفعلي، والفاعل هوية الديمو
    // المعلنة، والبيانات مصنّفة DEMO_SYNTHETIC في مستأجرها.
    let created_hash = append_audit_event(
        &transaction,
        "tenant_demo_sa_001",
        "audit_demo_task_created",
        "TASK_CREATED",
        "NEXUS Demo Administrator",
        "Task",
        "task_demo_001",
        &now,
        &previous_hash,
    )?;
    append_audit_event(
        &transaction,
        "tenant_demo_sa_001",
        "audit_demo_004",
        "WORKFLOW_SEEDED",
        "NEXUS Demo Administrator",
        "Task",
        "task_demo_001",
        &now,
        &created_hash,
    )?;

    transaction.commit().map_err(|_| AppError::StorageOperation)
}

// بذر أدوات NEXUS المدمجة الموثوقة (MP03): سجل + نسخة v1 + ربط محوّل لكلٍّ.
// هذه الأداة READ-ONLY (NONE/INTERNAL/STANDARD/NOT_APPLICABLE) وقراءة فقط
// عبر مستودعات المنتج — لا تنفّذ أي شيء من واصف المستأجر.
pub(crate) fn seed_demo_governed_tool_runtime(connection: &Connection) -> Result<(), AppError> {
    let tools_seed_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM tools WHERE id = 'tool_builtin_obligations')",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(|_| AppError::StorageOperation)?;
    if tools_seed_exists {
        return Ok(());
    }

    let now = Utc::now().to_rfc3339();
    let transaction = connection
        .unchecked_transaction()
        .map_err(|_| AppError::StorageOperation)?;

    // عقود المدخلات/المخرجات بصيغة ToolContract المقننة (camelCase) — كما يراها الدومين.
    let list_input_contract = r#"{"fields":[{"name":"query","dataType":"string","required":false,"description":"Optional bounded substring filter applied to titles."},{"name":"limit","dataType":"integer","required":false,"description":"Maximum number of returned items between 1 and 10."}],"allowsAdditionalFields":false}"#;
    let obligations_output_contract = r#"{"fields":[{"name":"items","dataType":"string","required":true,"description":"Bounded JSON array of obligation summaries."},{"name":"total","dataType":"integer","required":true,"description":"Total obligations visible to the tenant."}],"allowsAdditionalFields":false}"#;
    let risks_output_contract = r#"{"fields":[{"name":"items","dataType":"string","required":true,"description":"Bounded JSON array of risk summaries."},{"name":"total","dataType":"integer","required":true,"description":"Total risks visible to the tenant."}],"allowsAdditionalFields":false}"#;

    let built_in_tools: [(&str, &str, &str, &str, &str, &str, &str); 2] = [
        (
            "tool_builtin_obligations",
            "tool_version_builtin_obligations_v1",
            "Obligation Bounded List (Built-in)",
            "Reads a bounded list of obligations from NEXUS product data. Read-only, internal, standard privilege.",
            list_input_contract,
            obligations_output_contract,
            "nexus.obligations.bounded_list.v1",
        ),
        (
            "tool_builtin_risks",
            "tool_version_builtin_risks_v1",
            "Risk Bounded List (Built-in)",
            "Reads a bounded list of risks from NEXUS product data. Read-only, internal, standard privilege.",
            list_input_contract,
            risks_output_contract,
            "nexus.risks.bounded_list.v1",
        ),
    ];

    for (tool_id, version_id, name, description, input_contract, output_contract, adapter_key) in
        built_in_tools
    {
        transaction
            .execute(
                "INSERT INTO tools (id, tenant_id, name, description, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'ACTIVE', ?5, ?5)",
                (tool_id, DEMO_TENANT_ID, name, description, &now),
            )
            .map_err(|_| AppError::StorageOperation)?;
        transaction
            .execute(
                "INSERT INTO tool_versions (id, tenant_id, tool_id, version_number, input_contract, output_contract, mutation, externality, privilege, reversibility, created_at) VALUES (?1, ?2, ?3, 1, ?4, ?5, 'NONE', 'INTERNAL', 'STANDARD', 'NOT_APPLICABLE', ?6)",
                (version_id, DEMO_TENANT_ID, tool_id, input_contract, output_contract, &now),
            )
            .map_err(|_| AppError::StorageOperation)?;
        crate::infrastructure::tool_adapter_binding_repository::ToolAdapterBindingRepository::insert(
            &transaction,
            &crate::infrastructure::tool_adapter_binding_repository::ToolAdapterBindingRecord {
                id: format!("tool_adapter_binding_{tool_id}"),
                tenant_id: DEMO_TENANT_ID.to_owned(),
                tool_version_id: version_id.to_owned(),
                adapter_key: adapter_key.to_owned(),
                bound_at: now.clone(),
            },
        )?;
    }

    transaction.commit().map_err(|_| AppError::StorageOperation)
}

// بذر ذكاء المرحلة 04: منح الصلاحيات الضيقة الجديدة + وكلاء ديمو بقدرات محددة
// المسؤوليات واضحة (بحث/التزام/مخاطر) مع بروفايل تنفيذ لكل منهم — سجل الوكلاء
// يظل طفرات منتجية محكومة خارج هذا البذر الأولي.
pub(crate) fn seed_demo_intelligence_runtime(connection: &Connection) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();

    // منح الأدوار: الطالب يملك تدفقات المعرفة/الخطط بلا أي صلاحية اعتماد،
    // والمراجِع (TENANT_ADMIN) يملك القراءة + بوابة ترقية الذاكرة البشرية.
    let requester_grants = [
        "permission_knowledge_read",
        "permission_knowledge_ingest",
        "permission_knowledge_disable",
        "permission_memory_read",
        "permission_plan_read",
        "permission_plan_create",
        "permission_plan_execute",
    ];
    for permission_id in requester_grants {
        connection
            .execute(
                "INSERT OR IGNORE INTO role_permissions (role_id, permission_id) VALUES ('role_demo_requester', ?1)",
                [permission_id],
            )
            .map_err(|_| AppError::StorageOperation)?;
    }
    let reviewer_grants = [
        "permission_knowledge_read",
        "permission_memory_read",
        "permission_memory_manage",
        "permission_plan_read",
    ];
    for permission_id in reviewer_grants {
        connection
            .execute(
                "INSERT OR IGNORE INTO role_permissions (role_id, permission_id) VALUES ('role_demo_tenant_admin', ?1)",
                [permission_id],
            )
            .map_err(|_| AppError::StorageOperation)?;
    }

    // وكلاء ديمو بقدرات محددة — التخطيط متعدد الوكلاء يختار منهم حتمياً.
    let intelligence_agents = [
        (
            "agent_demo_research",
            "DEMO Research Agent",
            "Researches bounded regulatory and organizational context from governed knowledge. Demo agent with clear research responsibility.",
            "RESEARCH",
            "agent_execution_profile_demo_research_v1",
        ),
        (
            "agent_demo_compliance",
            "DEMO Compliance Agent",
            "Analyzes obligation compliance questions from the bounded execution input. Demo agent with clear compliance responsibility.",
            "COMPLIANCE",
            "agent_execution_profile_demo_compliance_v1",
        ),
        (
            "agent_demo_risk",
            "DEMO Risk Agent",
            "Evaluates risk implications from bounded product data. Demo agent with clear risk responsibility.",
            "RISK",
            "agent_execution_profile_demo_risk_v1",
        ),
    ];

    for (agent_id, name, description, capability, profile_id) in intelligence_agents {
        let agent_exists = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM agents WHERE id = ?1)",
                [agent_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|_| AppError::StorageOperation)?;
        if agent_exists {
            continue;
        }

        let transaction = connection
            .unchecked_transaction()
            .map_err(|_| AppError::StorageOperation)?;
        transaction
            .execute(
                "INSERT INTO agents (id, tenant_id, name, description, status, capability, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'ACTIVE', ?5, ?6, ?6)",
                (agent_id, DEMO_TENANT_ID, name, description, capability, &now),
            )
            .map_err(|_| AppError::StorageOperation)?;
        transaction
            .execute(
                "INSERT INTO agent_execution_profiles (id, tenant_id, agent_id, version_number, instruction, created_at) VALUES (?1, ?2, ?3, 1, ?4, ?5)",
                (
                    profile_id,
                    DEMO_TENANT_ID,
                    agent_id,
                    "Answer strictly from the bounded execution input, cite provided citation IDs only, and state uncertainty explicitly.",
                    &now,
                ),
            )
            .map_err(|_| AppError::StorageOperation)?;
        transaction
            .commit()
            .map_err(|_| AppError::StorageOperation)?;
    }

    Ok(())
}

// ترتيب السلسلة هو ترتيب الإدراج الفعلي حصراً (rowid)، لا ترتيب الطابع الزمني والمعرّف:
// عدة أحداث تُضاف في المعاملة نفسها بطابع زمني واحد ومعرّفات UUID عشوائية، فيخالف
// ترتيب (occurred_at, id) ترتيب الإدراج وتُقرأ سلسلة سليمة كأثر مُخترق. يُستدعى داخل
// معاملة الكتابة نفسها حتى لا يتفرّع الذيل بين معاملتين متوازيتين.
pub fn latest_audit_event_hash(
    transaction: &rusqlite::Transaction<'_>,
    tenant_id: &str,
) -> Result<String, AppError> {
    use rusqlite::OptionalExtension;

    transaction
        .query_row(
            "SELECT event_hash FROM audit_events WHERE tenant_id = ?1 ORDER BY rowid DESC LIMIT 1",
            [tenant_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        // غياب أي حدث ليس خطأً (بداية سلسلة جديدة)، أما فشل القراءة فيُرفع فشلاً مغلقاً
        // بدل إسقاطه إلى سلسلة صفرية تُخفي انقطاع الأثر.
        .map_err(|_| AppError::DataAccess)
        .map(|hash| hash.unwrap_or_default())
}

pub fn append_audit_event(
    transaction: &rusqlite::Transaction<'_>,
    tenant_id: &str,
    id: &str,
    event_type: &str,
    actor_name: &str,
    resource_type: &str,
    resource_id: &str,
    occurred_at: &str,
    previous_hash: &str,
) -> Result<String, AppError> {
    use sha2::{Digest, Sha256};

    let payload = format!("{previous_hash}|{id}|{event_type}|{actor_name}|{resource_type}|{resource_id}|{occurred_at}");
    let event_hash = format!("{:x}", Sha256::digest(payload.as_bytes()));
    transaction
        .execute(
            "INSERT INTO audit_events (id, tenant_id, event_type, actor_name, resource_type, resource_id, occurred_at, previous_hash, event_hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            (id, tenant_id, event_type, actor_name, resource_type, resource_id, occurred_at, previous_hash, event_hash.clone()),
        )
        .map_err(|_| AppError::StorageOperation)?;
    Ok(event_hash)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use rusqlite::Connection;

    use super::{
        apply_migrations, ensure_permission_catalog, initialize_demo_database, read_runtime_mode,
    };

    #[test]
    fn migration_chain_applies_cleanly_with_zero_foreign_key_violations() {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
        apply_migrations(&connection).expect("سلسلة الهجرات يجب أن تنجح");

        let versions: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("عدد الهجرات يجب أن يقرأ");
        // 23 هجرة: أُضيفت 0022 لتأليف الضوابط وربطها، و0023 لسجل موفري الذكاء الاصطناعي.
        assert_eq!(versions, 26);

        let violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("فحص المفاتيح يجب أن ينجح");
        assert_eq!(violations, 0);
    }

    // سلامة السلسلة الكاملة 0001→0023 مع أعمدة تأليف الضوابط وجداول الربط وسجل الموفرين:
    // 23 صف هجرة، لا انتهاك مفاتيح أجنبة، و integrity_check = ok.
    #[test]
    fn controls_authoring_migration_lands_with_full_schema_integrity() {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
        apply_migrations(&connection).expect("سلسلة الهجرات يجب أن تنجح");

        let versions: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("عدد الهجرات يجب أن يقرأ");
        assert_eq!(versions, 26, "سلسلة الهجرات يجب أن تحمل 26 إصداراً");

        let latest: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("أحدث إصدار يجب أن يقرأ");
        assert_eq!(latest, 26);

        let violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("فحص المفاتيح يجب أن ينجح");
        assert_eq!(violations, 0, "لا انتهاك مفاتيح أجنبة بعد 0022");

        let integrity: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("فحص السلامة يجب أن ينجح");
        assert_eq!(integrity, "ok");

        let link_tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('control_obligations','control_risks','control_evidence')",
                [],
                |row| row.get(0),
            )
            .expect("جداول الربط يجب أن تُحصى");
        assert_eq!(link_tables, 3);

        let control_columns: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('controls') WHERE name IN ('status','category','owner','effectiveness','last_assessed_at','assessment_note')",
                [],
                |row| row.get(0),
            )
            .expect("أعمدة الضابط يجب أن تُحصى");
        assert_eq!(control_columns, 6);

        // كل جدول ربط يحمل مستأجره صراحةً — عزل إلزامي في المخطط نفسه.
        for table in ["control_obligations", "control_risks", "control_evidence"] {
            let has_tenant: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = 'tenant_id'",
                    [table],
                    |row| row.get(0),
                )
                .expect("فحص عمود المستأجر يجب أن ينجح");
            assert_eq!(has_tenant, 1, "الجدول {table} يجب أن يحمل tenant_id");
        }
    }

    #[test]
    fn upgrade_path_from_0017_live_database_applies_phase_04_migrations_cleanly() {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");

        // محاكاة قاعدة حية عند إغلاق MP03: السلسلة حتى 0017 مُطبقة ومسجلة فقط
        for (version, script) in [
            (1_i32, include_str!("../../migrations/0001_initial.sql")),
            (
                2_i32,
                include_str!("../../migrations/0002_identity_and_access.sql"),
            ),
            (
                3_i32,
                include_str!("../../migrations/0003_workflow_and_risk.sql"),
            ),
            (
                4_i32,
                include_str!("../../migrations/0004_evidence_metadata.sql"),
            ),
            (
                5_i32,
                include_str!("../../migrations/0005_ai_governance.sql"),
            ),
            (
                6_i32,
                include_str!("../../migrations/0006_correct_demo_truth_labels.sql"),
            ),
            (
                7_i32,
                include_str!("../../migrations/0007_obligation_management.sql"),
            ),
            (
                8_i32,
                include_str!("../../migrations/0008_obligation_relationship_update_isolation.sql"),
            ),
            (
                9_i32,
                include_str!("../../migrations/0009_controls_read_model.sql"),
            ),
            (
                10_i32,
                include_str!("../../migrations/0010_agent_foundation.sql"),
            ),
            (11_i32, include_str!("../../migrations/0011_agent_runs.sql")),
            (
                12_i32,
                include_str!("../../migrations/0012_agent_run_approval_lifecycle.sql"),
            ),
            (
                13_i32,
                include_str!("../../migrations/0013_agent_execution_results.sql"),
            ),
            (
                14_i32,
                include_str!("../../migrations/0014_governed_skills.sql"),
            ),
            (
                15_i32,
                include_str!("../../migrations/0015_agent_run_skill_bindings.sql"),
            ),
            (
                16_i32,
                include_str!("../../migrations/0016_governed_tools.sql"),
            ),
            (
                17_i32,
                include_str!("../../migrations/0017_governed_agent_tool_runtime.sql"),
            ),
        ] {
            let transaction = connection
                .unchecked_transaction()
                .expect("معاملة الهجرة يجب أن تبدأ");
            transaction
                .execute_batch(script)
                .expect("هجرة الأساس يجب أن تنجح");
            transaction
                .execute(
                    "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                    (version, "2026-10-05T00:00:00Z"),
                )
                .expect("تسجيل الهجرة يجب أن ينجح");
            transaction.commit().expect("الهجرة يجب أن تُثبّت");
        }

        // الترقية: المُشغِّل يطبق 0018-0020 فقط فوق القاعدة الحية
        apply_migrations(&connection).expect("الترقية يجب أن تنجح");
        let versions: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("عدد الهجرات يجب أن يقرأ");
        let violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("فحص المفاتيح يجب أن ينجح");
        let phase04_tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('knowledge_sources','knowledge_documents','knowledge_chunks','knowledge_chunks_fts','memory_items','plans','plan_steps','agent_delegations','knowledge_retrieval_bindings','verification_runs','verification_evidence_links','plan_approval_links')",
                [],
                |row| row.get(0),
            )
            .expect("جداول المرحلة يجب أن تُحصى");
        let agents_capability: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('agents') WHERE name = 'capability'",
                [],
                |row| row.get(0),
            )
            .expect("عمود القدرة يجب أن يُحصى");

        // الترقية تشمل الآن 0022 و0023 أيضاً — العدد الكلي 23.
        assert_eq!(versions, 26);
        assert_eq!(violations, 0);
        assert_eq!(phase04_tables, 12);
        assert_eq!(agents_capability, 1);
    }

    #[test]
    fn upgrade_from_0020_demo_live_database_transitions_to_demo_mode_cleanly() {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");

        // محاكاة قاعدة MP04 حية عند إغلاق المرحلة: 0001→0020 + مستأجر الديمو موجود.
        for (version, script) in [
            (1_i32, include_str!("../../migrations/0001_initial.sql")),
            (
                2_i32,
                include_str!("../../migrations/0002_identity_and_access.sql"),
            ),
            (
                3_i32,
                include_str!("../../migrations/0003_workflow_and_risk.sql"),
            ),
            (
                4_i32,
                include_str!("../../migrations/0004_evidence_metadata.sql"),
            ),
            (
                5_i32,
                include_str!("../../migrations/0005_ai_governance.sql"),
            ),
            (
                6_i32,
                include_str!("../../migrations/0006_correct_demo_truth_labels.sql"),
            ),
            (
                7_i32,
                include_str!("../../migrations/0007_obligation_management.sql"),
            ),
            (
                8_i32,
                include_str!("../../migrations/0008_obligation_relationship_update_isolation.sql"),
            ),
            (
                9_i32,
                include_str!("../../migrations/0009_controls_read_model.sql"),
            ),
            (
                10_i32,
                include_str!("../../migrations/0010_agent_foundation.sql"),
            ),
            (11_i32, include_str!("../../migrations/0011_agent_runs.sql")),
            (
                12_i32,
                include_str!("../../migrations/0012_agent_run_approval_lifecycle.sql"),
            ),
            (
                13_i32,
                include_str!("../../migrations/0013_agent_execution_results.sql"),
            ),
            (
                14_i32,
                include_str!("../../migrations/0014_governed_skills.sql"),
            ),
            (
                15_i32,
                include_str!("../../migrations/0015_agent_run_skill_bindings.sql"),
            ),
            (
                16_i32,
                include_str!("../../migrations/0016_governed_tools.sql"),
            ),
            (
                17_i32,
                include_str!("../../migrations/0017_governed_agent_tool_runtime.sql"),
            ),
            (
                18_i32,
                include_str!("../../migrations/0018_knowledge_foundation.sql"),
            ),
            (
                19_i32,
                include_str!("../../migrations/0019_memory_foundation.sql"),
            ),
            (
                20_i32,
                include_str!("../../migrations/0020_intelligence_runtime.sql"),
            ),
        ] {
            let transaction = connection
                .unchecked_transaction()
                .expect("معاملة الهجرة يجب أن تبدأ");
            transaction
                .execute_batch(script)
                .expect("هجرة الأساس يجب أن تنجح");
            transaction
                .execute(
                    "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                    (version, "2026-10-08T00:00:00Z"),
                )
                .expect("تسجيل الهجرة يجب أن ينجح");
            transaction.commit().expect("الهجرة يجب أن تُثبّت");
        }
        let now = chrono::Utc::now().to_rfc3339();
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant_demo_sa_001', 'Demo', 'DEMO_SYNTHETIC', ?1, ?1)",
                [&now],
            )
            .expect("مستأجر الديمو يجب أن ينشأ");

        // الترقية إلى 0021: الوضع يُشتق ديمو لأن مستأجر الديمو موجود.
        apply_migrations(&connection).expect("الترقية إلى 0021 يجب أن تنجح");
        let mode = read_runtime_mode(&connection).expect("الوضع يجب أن يقرأ");
        assert_eq!(mode, crate::domain::models::RuntimeMode::Demo);

        let auth_columns: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('local_users') WHERE name IN ('password_hash','failed_attempts','locked_until')",
                [],
                |row| row.get(0),
            )
            .expect("أعمدة المصادقة يجب أن تُحصى");
        assert_eq!(auth_columns, 3);
        let violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("فحص المفاتيح يجب أن ينجح");
        assert_eq!(violations, 0);
    }

    #[test]
    fn fresh_0021_on_empty_database_stays_unprovisioned_with_no_demo_seeding() {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
        apply_migrations(&connection).expect("الهجرات يجب أن تنجح");
        ensure_permission_catalog(&connection).expect("الكتالوج يجب أن ينجح");

        // بيئة عميل نظيفة: لا ديمو، لا هويات، وضع غير مهيأ — الإعداد الصريح هو المدخل الوحيد.
        let mode = read_runtime_mode(&connection).expect("الوضع يجب أن يقرأ");
        assert_eq!(mode, crate::domain::models::RuntimeMode::Unprovisioned);
        let tenants: i64 = connection
            .query_row("SELECT COUNT(*) FROM tenants", [], |row| row.get(0))
            .expect("المستأجرون يجب أن يُحصون");
        assert_eq!(tenants, 0);
        let users: i64 = connection
            .query_row("SELECT COUNT(*) FROM local_users", [], |row| row.get(0))
            .expect("المستخدمون يجب أن يُحصون");
        assert_eq!(users, 0);
        // الكتالوج العالمي موجود رغم ذلك — جاهز لإقلاع الإنتاج.
        let permissions: i64 = connection
            .query_row("SELECT COUNT(*) FROM permissions", [], |row| row.get(0))
            .expect("الأذونات يجب أن تُحصى");
        assert!(permissions >= 33);
    }

    // 0025: الموافقات المعلّقة اليتيمة على المهام. قبل هذه الهجرة كان البذر
    // يُنشئ موافقة معلّقة على مهمة حالتها OPEN، وقرارها مستحيل بالتصميم بعد
    // إغلاق دورة الحياة. السياسة: لا حذف صامت — تصنيفها CANCELLED بسبب صريح.
    #[test]
    fn task_lifecycle_migration_cancels_only_undecidable_pending_task_approvals() {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
        // مخطط ما قبل 0025 كما هو على قرص المستخدم فعلاً.
        for migration in [
            include_str!("../../migrations/0001_initial.sql"),
            include_str!("../../migrations/0002_identity_and_access.sql"),
            include_str!("../../migrations/0003_workflow_and_risk.sql"),
            include_str!("../../migrations/0010_agent_foundation.sql"),
            include_str!("../../migrations/0011_agent_runs.sql"),
            include_str!("../../migrations/0012_agent_run_approval_lifecycle.sql"),
        ] {
            connection
                .execute_batch(migration)
                .expect("هجرة ما قبل الترقية يجب أن تنجح");
        }

        let now = Utc::now().to_rfc3339();
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant-legacy', 'Legacy', 'DEMO_SYNTHETIC', ?1, ?1)",
                [&now],
            )
            .expect("المستأجر يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES ('user-legacy', 'tenant-legacy', 'Legacy User', 'legacy@test.local', NULL, 'ACTIVE', ?1, ?1)",
                [&now],
            )
            .expect("المستخدم يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES ('source-legacy', 'tenant-legacy', 'Synthetic', 'Synthetic source', NULL, '1', ?1, 0, 'UNVERIFIED', ?1, ?1)",
                [&now],
            )
            .expect("المصدر يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, created_at, updated_at) VALUES ('obligation-legacy', 'tenant-legacy', 'source-legacy', 'Legacy obligation', 'Owner', NULL, 'HIGH', 'OPEN', 'APPLIES', 'MISSING', 'UNVERIFIED', ?1, ?1)",
                [&now],
            )
            .expect("الالتزام يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO tasks (id, tenant_id, obligation_id, title, description, assignee_user_id, due_at, priority, status, created_at, updated_at) VALUES ('task-legacy', 'tenant-legacy', 'obligation-legacy', 'Legacy task', 'Seeded under the old lifecycle', 'user-legacy', NULL, 'HIGH', 'OPEN', ?1, ?1)",
                [&now],
            )
            .expect("المهمة القديمة يجب أن تنشأ");
        // الموافقة اليتيمة التي كان البذر القديم يُنشئها.
        connection
            .execute(
                "INSERT INTO approvals (id, tenant_id, resource_type, resource_id, resource_version, requested_by, required_role_code, decision, decision_by, decision_reason, decided_at, created_at, updated_at) VALUES ('approval-orphan', 'tenant-legacy', 'TASK', 'task-legacy', 1, 'user-legacy', 'TENANT_ADMIN', 'PENDING', NULL, NULL, NULL, ?1, ?1)",
                [&now],
            )
            .expect("الموافقة اليتيمة يجب أن تنشأ");
        let before: i64 = connection
            .query_row("SELECT COUNT(*) FROM approvals", [], |row| row.get(0))
            .expect("عدّ الموافقات قبل الترقية");

        connection
            .execute_batch(concat!(
                include_str!("../../migrations/0025_task_lifecycle.sql"),
                include_str!("../../migrations/0026_production_user_and_authoring.sql")
            ))
            .expect("هجرة دورة حياة المهمة وعيوب UAT يجب أن تنجح");

        // لا حذف: الصف باقٍ بعدد الصفوف نفسه، لكنه لم يعد يُقدَّم كعمل معلّق.
        let after: i64 = connection
            .query_row("SELECT COUNT(*) FROM approvals", [], |row| row.get(0))
            .expect("عدّ الموافقات بعد الترقية");
        assert_eq!(before, after, "التصحيح لا يحذف صفاً");
        let (decision, decision_by, reason): (String, Option<String>, Option<String>) = connection
            .query_row(
                "SELECT decision, decision_by, decision_reason FROM approvals WHERE id = 'approval-orphan'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("قراءة الموافقة يجب أن تنجح");
        assert_eq!(decision, "CANCELLED");
        assert_eq!(decision_by.as_deref(), Some("NEXUS_MIGRATION_0025"));
        assert!(
            reason.unwrap_or_default().contains("غير ممكن"),
            "السبب يجب أن يوضح أن القرار غير ممكن"
        );
        let still_pending: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM approvals WHERE decision = 'PENDING'",
                [],
                |row| row.get(0),
            )
            .expect("عدّ المعلّق يجب أن ينجح");
        assert_eq!(still_pending, 0, "لا يبقى اعتماد معلّق لا يمكن حسمه");

        // والمهمة نفسها لم تُمس: حالتها وإصدارها كما كانا.
        let (status, version): (String, i64) = connection
            .query_row(
                "SELECT status, version FROM tasks WHERE id = 'task-legacy'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("قراءة المهمة يجب أن تنجح");
        assert_eq!(status, "OPEN");
        assert_eq!(version, 1);
    }

    #[test]
    fn failed_migration_leaves_version_unadvanced_and_database_recoverable() {
        // سكربت هجرة فاسد داخل معاملة — المحاكاة تثبت أن الإصدار لا يتقدم زوراً.
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
        apply_migrations(&connection).expect("سلسلة الهجرات السليمة يجب أن تنجح");
        let before: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("الإصدار قبل الفشل يجب أن يقرأ");
        assert_eq!(before, 26);

        let broken_script =
            "CREATE TABLE mp05_failure_probe (id TEXT PRIMARY KEY);\nTHIS IS NOT VALID SQL;";
        let transaction = connection
            .unchecked_transaction()
            .expect("معاملة الفشل يجب أن تبدأ");
        let script_failed = transaction.execute_batch(broken_script).is_err();
        assert!(script_failed, "السكربت الفاسد يجب أن يفشل");
        // إفلات المعاملة دون تثبيت — كما يفعل المشغل عند فشل السكربت.
        drop(transaction);

        let after: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("الإصدار بعد الفشل يجب أن يقرأ");
        assert_eq!(after, 26, "فشل الهجرة لا يرفع الإصدار أبداً");
        // الجدول الجزئي من السكربت الفاسد غير موجود — المعاملة ألغت كل شيء.
        let probe: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'mp05_failure_probe'",
                [],
                |row| row.get(0),
            )
            .expect("التحقق من عدم التسرب يجب أن ينجح");
        assert_eq!(probe, 0);
    }

    #[test]
    fn demo_bootstrap_pollutes_neither_schema_nor_foreign_keys() {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
        initialize_demo_database(&connection).expect("تهيئة الديمو يجب أن تنجح");

        // البذر لا يضيف هجرات مخفية ولا يخل بقيد مفاتيح الأجنبة.
        let versions: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("عدد الهجرات يجب أن يقرأ");
        assert_eq!(versions, 26);
        let violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("فحص المفاتيح يجب أن ينجح");
        assert_eq!(violations, 0);

        // الفاعلان التشغيليان مزروعان بجلسات نشطة داخل مستأجر الديمو.
        let active_actors: i64 = connection
            .query_row(
                "SELECT COUNT(DISTINCT u.id) FROM local_users u
                 INNER JOIN local_sessions s ON s.user_id = u.id AND s.tenant_id = u.tenant_id
                 WHERE u.tenant_id = 'tenant_demo_sa_001'
                   AND u.id IN ('user_demo_operator', 'user_demo_reviewer')
                   AND u.status = 'ACTIVE' AND s.revoked_at IS NULL AND s.expires_at > '2029-01-01T00:00:00Z'",
                [],
                |row| row.get(0),
            )
            .expect("عدد الفاعلين يجب أن يقرأ");
        assert_eq!(active_actors, 2);
    }

    // ===== 0023: سجل موفري الذكاء الاصطناعي =====
    // سلامة السلسلة الكاملة 0001→0023 مع سجل الموفرين وprovider_id في سجل النماذج.
    #[test]
    fn ai_provider_registry_migration_lands_with_full_schema_integrity() {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
        apply_migrations(&connection).expect("سلسلة الهجرات يجب أن تنجح");

        let versions: i64 = connection
            .query_row("SELECT COUNT(*) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("عدد الهجرات يجب أن يقرأ");
        assert_eq!(versions, 26, "سلسلة الهجرات يجب أن تحمل 26 إصداراً");

        let latest: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("أحدث إصدار يجب أن يقرأ");
        assert_eq!(latest, 26);

        let violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("فحص المفاتيح يجب أن ينجح");
        assert_eq!(violations, 0, "لا انتهاك مفاتيح أجنبة بعد 0023");

        let integrity: String = connection
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("فحص السلامة يجب أن ينجح");
        assert_eq!(integrity, "ok");

        // سجل الموفرين موجود بعموديه الحرجين: علامة السر وحالة الاتصال.
        let provider_columns: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('ai_provider_registry')
                 WHERE name IN ('provider_id','kind','display_name','base_url','transport',
                                'enabled','connection_state','connection_checked_at',
                                'credential_configured','built_in','tenant_id','created_at','updated_at')",
                [],
                |row| row.get(0),
            )
            .expect("أعمدة سجل الموفرين يجب أن تُحصى");
        assert_eq!(provider_columns, 13);

        // تعميم سجل النماذج: provider_id أُضيف والقدرات تبقى NULL لا صفراً.
        let model_columns: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('ai_model_registry')
                 WHERE name IN ('provider_id','context_limit','supports_tools',
                                'supports_structured_output','supports_vision',
                                'capability_source','available','last_discovered_at')",
                [],
                |row| row.get(0),
            )
            .expect("أعمدة سجل النماذج يجب أن تُحصى");
        assert_eq!(model_columns, 8);

        // لا عمود ولا جدول يحمل سراً — التحقق من المخطط نفسه لا من النية.
        let secret_columns: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('ai_provider_registry')
                 WHERE lower(name) LIKE '%secret%'
                    OR lower(name) LIKE '%token%'
                    OR lower(name) LIKE '%api_key%'
                    OR lower(name) LIKE '%password%'
                    OR lower(name) LIKE '%credential%'",
                [],
                |row| row.get(0),
            )
            .expect("فحص أعمدة الأسرار يجب أن ينجح");
        assert_eq!(
            secret_columns, 1,
            "العمود الوحيد المطابق يجب أن يكون credential_configured (علامة منطقية)"
        );
        let marker: String = connection
            .query_row(
                "SELECT name FROM pragma_table_info('ai_provider_registry')
                 WHERE lower(name) LIKE '%credential%'",
                [],
                |row| row.get(0),
            )
            .expect("اسم عمود العلامة يجب أن يقرأ");
        assert_eq!(marker, "credential_configured");

        // فهرس الانتخاب الواحد: يمنع وجود نموذجين مفعّلين في المستأجر نفسه.
        let single_selection_index: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'index' AND name = 'idx_ai_models_single_selection'",
                [],
                |row| row.get(0),
            )
            .expect("فهرس الانتخاب يجب أن يُحصى");
        assert_eq!(single_selection_index, 1);
    }

    // إعادة بناء ai_model_registry داخل 0023 لا تفقد أي صف ولا تكسر مرجع
    // ai_analysis_runs.model_registry_id (خطر إعادة التسمية في SQLite).
    #[test]
    fn provider_registry_rebuild_preserves_model_rows_and_analysis_references() {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");

        // محاكاة قاعدة حية عند إغلاق 0022: السلسلة كاملة ثم صفوف نماذج وتحليل.
        for (version, script) in [
            (1_i32, include_str!("../../migrations/0001_initial.sql")),
            (
                2_i32,
                include_str!("../../migrations/0002_identity_and_access.sql"),
            ),
            (
                3_i32,
                include_str!("../../migrations/0003_workflow_and_risk.sql"),
            ),
            (
                4_i32,
                include_str!("../../migrations/0004_evidence_metadata.sql"),
            ),
            (
                5_i32,
                include_str!("../../migrations/0005_ai_governance.sql"),
            ),
            (
                6_i32,
                include_str!("../../migrations/0006_correct_demo_truth_labels.sql"),
            ),
            (
                7_i32,
                include_str!("../../migrations/0007_obligation_management.sql"),
            ),
            (
                8_i32,
                include_str!("../../migrations/0008_obligation_relationship_update_isolation.sql"),
            ),
            (
                9_i32,
                include_str!("../../migrations/0009_controls_read_model.sql"),
            ),
            (
                10_i32,
                include_str!("../../migrations/0010_agent_foundation.sql"),
            ),
            (11_i32, include_str!("../../migrations/0011_agent_runs.sql")),
            (
                12_i32,
                include_str!("../../migrations/0012_agent_run_approval_lifecycle.sql"),
            ),
            (
                13_i32,
                include_str!("../../migrations/0013_agent_execution_results.sql"),
            ),
            (
                14_i32,
                include_str!("../../migrations/0014_governed_skills.sql"),
            ),
            (
                15_i32,
                include_str!("../../migrations/0015_agent_run_skill_bindings.sql"),
            ),
            (
                16_i32,
                include_str!("../../migrations/0016_governed_tools.sql"),
            ),
            (
                17_i32,
                include_str!("../../migrations/0017_governed_agent_tool_runtime.sql"),
            ),
            (
                18_i32,
                include_str!("../../migrations/0018_knowledge_foundation.sql"),
            ),
            (
                19_i32,
                include_str!("../../migrations/0019_memory_foundation.sql"),
            ),
            (
                20_i32,
                include_str!("../../migrations/0020_intelligence_runtime.sql"),
            ),
            (
                21_i32,
                include_str!("../../migrations/0021_production_identity_and_backup.sql"),
            ),
            (
                22_i32,
                include_str!("../../migrations/0022_controls_authoring.sql"),
            ),
        ] {
            let transaction = connection
                .unchecked_transaction()
                .expect("معاملة الهجرة يجب أن تبدأ");
            transaction
                .execute_batch(script)
                .expect("هجرة الأساس يجب أن تنجح");
            transaction
                .execute(
                    "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                    (version, "2026-10-10T00:00:00Z"),
                )
                .expect("تسجيل الهجرة يجب أن ينجح");
            transaction.commit().expect("الهجرة يجب أن تُثبّت");
        }

        let now = chrono::Utc::now().to_rfc3339();
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant-rebuild', 'Rebuild', 'DEMO_SYNTHETIC', ?1, ?1)",
                [&now],
            )
            .expect("المستأجر يجب أن ينشأ");
        for (index, model_id) in ["qwen3:4b", "llama3.2:3b"].iter().enumerate() {
            connection
                .execute(
                    "INSERT INTO ai_model_registry (id, tenant_id, provider_kind, model_id, display_name, enabled, data_residency, allowed_classification_ceiling, requires_human_approval, created_at, updated_at) VALUES (?1, 'tenant-rebuild', 'LOCAL', ?2, ?2, 0, 'LOCAL', 'INTERNAL', 1, ?3, ?3)",
                    rusqlite::params![format!("model-{index}"), model_id, &now],
                )
                .expect("صف النموذج يجب أن ينشأ");
        }
        // نموذج واحد مفعّل فقط — كما يفرضه مسار التكوين المحكوم.
        connection
            .execute(
                "UPDATE ai_model_registry SET enabled = 1 WHERE tenant_id = 'tenant-rebuild' AND model_id = 'qwen3:4b'",
                [],
            )
            .expect("تفعيل نموذج واحد يجب أن ينجح");
        connection
            .execute(
                "INSERT INTO ai_analysis_runs (id, tenant_id, model_registry_id, purpose, input_hash, human_review_state, status, evidence_reference_ids_json, created_at) VALUES ('run-1', 'tenant-rebuild', 'model-0', 'DISCOVERY', 'hash', 'NOT_REQUESTED', 'QUEUED', '[]', ?1)",
                [&now],
            )
            .expect("صف التحليل يجب أن ينشأ");

        // الترقية: 0023 وحدها تُطبَّق فوق القاعدة الحية.
        apply_migrations(&connection).expect("الترقية إلى 0023 يجب أن تنجح");

        let model_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_model_registry WHERE tenant_id = 'tenant-rebuild'",
                [],
                |row| row.get(0),
            )
            .expect("عدد صفوف النماذج يجب أن يقرأ");
        assert_eq!(model_rows, 2, "إعادة البناء لا تفقد أي صف نموذج");

        // كل الصفوف التاريخية رُبطت بمثيل Ollama المزروع، ولا واحدة بقيت بلا موفر.
        let linked_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_model_registry WHERE tenant_id = 'tenant-rebuild' AND provider_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .expect("عدد الصفوف المرتبطة يجب أن يقرأ");
        assert_eq!(linked_rows, 2);

        let provider_rows: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ai_provider_registry WHERE tenant_id = 'tenant-rebuild' AND provider_id = 'OLLAMA_LOCAL' AND built_in = 1",
                [],
                |row| row.get(0),
            )
            .expect("صف الموفر المزروع يجب أن يقرأ");
        assert_eq!(provider_rows, 1);

        // مرجع المفتاح الأجنبي في ai_analysis_runs ما زال يشير إلى ai_model_registry:
        // لو أفسدت إعادة التسمية تعريف الجدول لفشل الإدراج أعلاه أو لظهر انتهاك.
        let reference: String = connection
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'ai_analysis_runs'",
                [],
                |row| row.get(0),
            )
            .expect("تعريف جدول التحليل يجب أن يقرأ");
        assert!(
            reference.contains("ai_model_registry"),
            "المفتاح الأجنبي يجب أن يبقى على ai_model_registry"
        );
        assert!(
            !reference.contains("ai_model_registry_new"),
            "لا يجوز أن يبقى أثر الاسم المؤقت في المخطط"
        );

        let violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("فحص المفاتيح يجب أن ينجح");
        assert_eq!(violations, 0);

        // قيد التفرّد الأصلي محفوظ: نوع+نموذج مكرر داخل المستأجر يُرفض كما قبل 0023.
        let duplicated = connection.execute(
            "INSERT INTO ai_model_registry (id, tenant_id, provider_kind, model_id, display_name, enabled, data_residency, allowed_classification_ceiling, requires_human_approval, created_at, updated_at) VALUES ('model-dup', 'tenant-rebuild', 'LOCAL', 'qwen3:4b', 'dup', 0, 'LOCAL', 'INTERNAL', 1, ?1, ?1)",
            [&now],
        );
        assert!(duplicated.is_err(), "قيد التفرّد الأصلي يجب أن يبقى فعّالاً");

        // قيد النموذج المفعّل الواحد: تفعيل صف ثانٍ يُرفض.
        let second_enabled = connection.execute(
            "UPDATE ai_model_registry SET enabled = 1 WHERE tenant_id = 'tenant-rebuild' AND model_id = 'llama3.2:3b'",
            [],
        );
        assert!(
            second_enabled.is_err(),
            "لا يجوز أن يوجد نموذجان مفعّلان في المستأجر نفسه"
        );
    }

    // ===== 0026: هجرة فصل الأدلة وكتالوج أدوار الإنتاج =====
    // الغرض: إثبات أن إضافة العمود لم تُسقط أي مشغّل عزل قائم، وأن كتالوج
    // أدوار الإنتاج يُنشأ لمستأجر إنتاجي قائم بلا تكرار، وأن تعبئة `registered_by`
    // القديمة تستنتج المسجّل من حدث التدقيق نفسه.
    #[test]
    fn uat_closure_migration_keeps_tenant_triggers_and_backfills_registrations() {
        let connection = Connection::open_in_memory().expect("قاعدة الاختبار يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("المفاتيح الأجنبية يجب أن تعمل");
        apply_migrations(&connection).expect("سلسلة الهجرات يجب أن تكتمل");
        ensure_permission_catalog(&connection).expect("كتالوج الأذونات يجب أن يكتمل");
        super::ensure_production_role_catalog(&connection)
            .expect("كتالوج أدوار الإنتاج يجب أن يكتمل");

        let now = Utc::now().to_rfc3339();
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant-uat', 'UAT Org', 'PRODUCTION', ?1, ?1)",
                [&now],
            )
            .expect("المستأجر يجب أن ينشأ");
        super::ensure_production_role_catalog(&connection)
            .expect("إعادة الكتالوج يجب أن تكون عديمة الأثر");
        // دور منفّذ العمل مرة واحدة بالضبط للمستأجر الإنتاجي.
        let contributor_roles: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM roles WHERE tenant_id = 'tenant-uat' AND code = 'CONTRIBUTOR'",
                [],
                |row| row.get(0),
            )
            .expect("عدّ الأدوار يجب أن ينجح");
        assert_eq!(contributor_roles, 1);
        let contributor_grants: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM role_permissions rp
                 INNER JOIN roles r ON r.id = rp.role_id
                 WHERE r.tenant_id = 'tenant-uat' AND r.code = 'CONTRIBUTOR'",
                [],
                |row| row.get(0),
            )
            .expect("عدّ المنح يجب أن ينجح");
        assert!(contributor_grants > 0);

        connection
            .execute(
                "INSERT INTO regulatory_sources (id, tenant_id, authority, document_title, source_url, document_version, retrieved_at, official, verification_state, created_at, updated_at) VALUES ('source-uat', 'tenant-uat', 'Authority', 'Document', NULL, '1', ?1, 0, 'REQUIRES_REVIEW', ?1, ?1)",
                [&now],
            )
            .expect("المصدر يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO obligations (id, tenant_id, source_id, title, owner, deadline, severity, status, applicability, evidence_status, verification_state, source_type, created_at, updated_at) VALUES ('obligation-uat', 'tenant-uat', 'source-uat', 'Obligation', 'Owner', NULL, 'HIGH', 'OPEN', 'APPLIES', 'MISSING', 'UNVERIFIED', 'USER_PROVIDED', ?1, ?1)",
                [&now],
            )
            .expect("الالتزام يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO evidence_items (id, tenant_id, obligation_id, classification, status, content_hash, version, created_at, updated_at, label, expires_at) VALUES ('evidence-uat', 'tenant-uat', 'obligation-uat', 'INTERNAL', 'REQUIRES_REVIEW', 'hash-uat', 1, ?1, ?1, 'Evidence', NULL)",
                [&now],
            )
            .expect("الدليل يجب أن ينشأ");

        // تعبئة السجل القديم من حدث التسجيل نفسه — لا نسبة تخمينية.
        connection
            .execute(
                "INSERT INTO audit_events (id, tenant_id, event_type, actor_name, resource_type, resource_id, occurred_at, previous_hash, event_hash) VALUES ('audit-uat', 'tenant-uat', 'EVIDENCE_REFERENCE_REGISTERED', 'user-registrant', 'Evidence', 'evidence-uat', ?1, '', 'seeded-hash')",
                [&now],
            )
            .expect("حدث التدقيق يجب أن ينشأ");
        connection
            .execute(
                "UPDATE evidence_items SET registered_by = (
                   SELECT ae.actor_name FROM audit_events ae
                   WHERE ae.event_type = 'EVIDENCE_REFERENCE_REGISTERED'
                     AND ae.resource_type = 'Evidence'
                     AND ae.resource_id = evidence_items.id
                     AND ae.tenant_id = evidence_items.tenant_id
                   ORDER BY ae.rowid ASC LIMIT 1
                 ) WHERE registered_by IS NULL",
                [],
            )
            .expect("التعبئة يجب أن تنجح");
        let registrant: Option<String> = connection
            .query_row(
                "SELECT registered_by FROM evidence_items WHERE id = 'evidence-uat'",
                [],
                |row| row.get(0),
            )
            .expect("قراءة المسجّل يجب أن تنجح");
        assert_eq!(registrant.as_deref(), Some("user-registrant"));

        // ضابط العزل القائم ما زال فعّالاً بعد إضافة العمود: ربط دليل بالتزام
        // مستأجر آخر يُرفض بمشغّل قاعدة البيانات لا بتطبيق.
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES ('tenant-other', 'Other', 'PRODUCTION', ?1, ?1)",
                [&now],
            )
            .expect("المستأجر الآخر يجب أن ينشأ");
        let cross_tenant_insert = connection.execute(
            "INSERT INTO evidence_items (id, tenant_id, obligation_id, classification, status, content_hash, version, created_at, updated_at, label, expires_at) VALUES ('evidence-cross', 'tenant-other', 'obligation-uat', 'INTERNAL', 'REQUIRES_REVIEW', 'hash-cross', 1, ?1, ?1, 'Cross', NULL)",
            [&now],
        );
        assert!(
            cross_tenant_insert.is_err(),
            "مشغّل عزل المستأجر على الأدلة يجب أن يبقى فعّالاً بعد الهجرة"
        );

        let violations: i64 = connection
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .expect("فحص المفاتيح يجب أن ينجح");
        assert_eq!(violations, 0);
    }

    // UAT-05: مهمة الديمو المزروعة يجب أن تحمل حدث إنشاء حقيقي الطابع الزمني،
    // فلا يبدأ سجل المهمة الأولى ناقصاً. البيانات تبقى مصنّفة DEMO_SYNTHETIC.
    #[test]
    fn demo_seed_records_a_task_created_event_for_the_seeded_task() {
        let connection = Connection::open_in_memory().expect("قاعدة الاختبار يجب أن تفتح");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
        initialize_demo_database(&connection).expect("تهيئة قاعدة الديمو يجب أن تنجح");

        let creation_events: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE event_type = 'TASK_CREATED' AND resource_id = 'task_demo_001'",
                [],
                |row| row.get(0),
            )
            .expect("عدّ أحداث الإنشاء يجب أن ينجح");
        assert_eq!(
            creation_events, 1,
            "المهمة المزروعة يجب أن تحمل حدث إنشاء واحداً"
        );

        let seeded_events: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE event_type = 'WORKFLOW_SEEDED' AND resource_id = 'task_demo_001'",
                [],
                |row| row.get(0),
            )
            .expect("عدّ أحداث البذر يجب أن ينجح");
        assert_eq!(
            seeded_events, 1,
            "حدث البذر يبقى للدلالة على الطبيعة الاصطناعية"
        );
    }
}
