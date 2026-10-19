-- =====================================================================
-- MP05 — الهوية الإنتاجية ووضع التشغيل الصريح (0021)
-- لا يمس أي هجرة تاريخية؛ يضيف فقط:
-- 1) حالة وضع التشغيل الخلفية (UNPROVISIONED/DEMO/PRODUCTION)
-- 2) أعمدة المصادقة المحلية على المستخدمين (هاش Argon2id + خنق المحاولات)
-- =====================================================================
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS app_runtime_state (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  mode TEXT NOT NULL CHECK (mode IN ('UNPROVISIONED', 'DEMO', 'PRODUCTION')),
  provisioned_at TEXT,
  updated_at TEXT NOT NULL
);

-- ترحيل حالة البيئات القائمة: وجود مستأجر الديمو التاريخي يعني بيئة تطوير/عرض
-- تعمل بالديمو منذ MP01 — تُصنف DEMO لضمان استمرارية عملها دون انقطاع.
-- القاعدة الجديدة تماماً (عميل نظيف) لا تحمل المستأجر فتبقى UNPROVISIONED:
-- لا بذر ديمو صامتاً في بيئة عميل إطلاقاً.
INSERT INTO app_runtime_state (id, mode, provisioned_at, updated_at)
SELECT 1,
       CASE
         WHEN EXISTS (SELECT 1 FROM tenants WHERE id = 'tenant_demo_sa_001') THEN 'DEMO'
         ELSE 'UNPROVISIONED'
       END,
       NULL,
       strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE NOT EXISTS (SELECT 1 FROM app_runtime_state WHERE id = 1);

ALTER TABLE local_users ADD COLUMN password_hash TEXT;
ALTER TABLE local_users ADD COLUMN failed_attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE local_users ADD COLUMN locked_until TEXT;
