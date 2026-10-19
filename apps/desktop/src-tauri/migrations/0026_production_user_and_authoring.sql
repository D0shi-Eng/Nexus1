PRAGMA foreign_keys = ON;

-- =====================================================================
-- 0026 — إغلاق عيوب UAT الإنتاجية
-- ---------------------------------------------------------------------
-- 1) فصل من قدّم الدليل عن من يقرر حالة التحقق منه (UAT-04):
--    عمود `registered_by` يحفظ هوية مقدم الدليل وقت التسجيل، فيرفض الخلف
--    على من قدّمه أن يغيّر حالته (ضابط في الخلف لا إخفاء زر في الواجهة).
--
--    لماذا ALTER TABLE ADD COLUMN وحدها: الهجرة **لا تعيد بناء** الجدول،
--    فتبقى كل المشغّلات القائمة (عزل المستأجر على evidence_items) والمفاتيح
--    الأجنبية وسلامة الصفوف كما هي بلا إعادة إنشاء ولا فقدان ضوابط.
-- =====================================================================

ALTER TABLE evidence_items ADD COLUMN registered_by TEXT;

-- تعبئة الأثر القديم من سجل التدقيق نفسه: أول حدث `EVIDENCE_REFERENCE_REGISTERED`
-- لهذا الدليل هو من قدّمه فعلاً. هذا استنتاج موثّق لا تخمين، والسجلات التي لا
-- حدث لها تبقى NULL بوصفها مجهولة المصدر بدل نسبة خاطئة إلى مستخدم.
UPDATE evidence_items
SET registered_by = (
  SELECT ae.actor_name
  FROM audit_events ae
  WHERE ae.event_type = 'EVIDENCE_REFERENCE_REGISTERED'
    AND ae.resource_type = 'Evidence'
    AND ae.resource_id = evidence_items.id
    AND ae.tenant_id = evidence_items.tenant_id
  ORDER BY ae.rowid ASC
  LIMIT 1
)
WHERE registered_by IS NULL;

-- فهرس للتحقق من ملكية التسجيل عند كل نقل حالة — قراءة واحدة على مفتاحين.
CREATE INDEX IF NOT EXISTS idx_evidence_tenant_registered_by
  ON evidence_items (tenant_id, registered_by);

-- =====================================================================
-- 2) كتالوج أدوار المؤسسة الإنتاجية:
--    التأسيس كان يُنشئ دور TENANT_ADMIN وحده، فلا يوجد دور لمنفّذ العمل
--    وتبقى المؤسسة الإنتاجية بمستخدم واحد. البذر هنا **لا يعمل على قاعدة
--    جديدة** لأن الجداول فارغة وقت الترحيل (التهيئة تحدث بعدها)، ولذلك
--    يبقى المصدر الملزم للكتالوج هو `ensure_production_role_catalog` في
--    مسار الإقلاع والتأسيس. هذا السطر إضافي فقط لقواعد رُقّيت ولها مستأجر
--    إنتاجي قائم بلا كتالوج، ويبقى إدراجاً تجاهلياً لا يكرر شيئاً.
-- =====================================================================

INSERT OR IGNORE INTO roles (id, tenant_id, code, display_name, created_at, updated_at)
SELECT 'role_' || t.id || '_contributor', t.id, 'CONTRIBUTOR', 'Contributor',
       strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
FROM tenants t
WHERE t.data_mode = 'PRODUCTION';

INSERT OR IGNORE INTO role_permissions (role_id, permission_id)
SELECT r.id, p.id
FROM roles r
INNER JOIN tenants t ON t.id = r.tenant_id AND t.data_mode = 'PRODUCTION'
INNER JOIN permissions p ON p.code IN (
  'obligation:read',
  'attention:read',
  'risk:read',
  'evidence:read',
  'evidence:create',
  'regulatory_intelligence:read',
  'task:read',
  'task:update',
  'control:read',
  'agent:read',
  'organization:read',
  'integration:read',
  'knowledge:read',
  'memory:read',
  'plan:read',
  'audit:read',
  'report:read'
)
WHERE r.code = 'CONTRIBUTOR';
