PRAGMA foreign_keys = ON;

-- =====================================================================
-- 0025 — دورة حياة المهمة المحكومة
-- ---------------------------------------------------------------------
-- العيب المُثبت قبل هذه الهجرة: جدول tasks كان يقبل أربع حالات فقط
-- (OPEN/IN_PROGRESS/BLOCKED/COMPLETED)، ولا يوجد أي أمر أو خدمة تنقل حالة
-- مهمة، ولا يوجد ربط بين قرار الموافقة (approvals.resource_type = 'TASK')
-- وحالة المهمة. النتيجة: مهمة تُنشأ OPEN وتبقى OPEN إلى الأبد، وقرار
-- الاعتماد يكتب في جدول الموافقات فقط ولا يتحرك العمل.
--
-- هذه الهجرة تُغلق الفجوة البنيوية وحدها: الحالات + جدول سجل الانتقالات.
-- أما قواعد الانتقال ومن يملكها وشروطها المسبقة فمحلُّها الخلف (Rust) لأن
-- SQLite لا يملك state machine، وتُختبر حتمياً هناك.
--
-- ملاحظة إعادة البناء: SQLite لا يقبل تعديل قيد CHECK، فالمسار المتَّبع في
-- هذا المستودع (0012 و0017) هو: جدول جديد ← نسخ ← إسقاط ← إعادة تسمية.
-- لا جدول في المخطط يشير إلى tasks بمفتاح أجنبي، فالإسقاط آمن.
-- =====================================================================

CREATE TABLE tasks_0025 (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  obligation_id TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT NOT NULL,
  assignee_user_id TEXT,
  due_at TEXT,
  priority TEXT NOT NULL CHECK (priority IN ('CRITICAL', 'HIGH', 'MEDIUM', 'LOW')),
  status TEXT NOT NULL CHECK (
    status IN (
      'OPEN',
      'IN_PROGRESS',
      'BLOCKED',
      'READY_FOR_REVIEW',
      'APPROVED',
      'REJECTED',
      'COMPLETED'
    )
  ),
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  -- حماية حالات نهائية: المهمة المكتملة يجب أن تحمل طابع إغلاق صريحاً.
  completed_at TEXT,
  CHECK ((status = 'COMPLETED') = (completed_at IS NOT NULL)),
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (obligation_id) REFERENCES obligations(id) ON DELETE RESTRICT,
  FOREIGN KEY (assignee_user_id) REFERENCES local_users(id) ON DELETE SET NULL
);

-- نسخ الصفوف القائمة حرفياً: لا حالة تُختلق ولا صف يُفقد. المهام المكتملة
-- سابقاً بلا completed_at تأخذ updated_at كطابع إغلاق (أقرب وقت صادق متاح).
INSERT INTO tasks_0025 (
  id,
  tenant_id,
  obligation_id,
  title,
  description,
  assignee_user_id,
  due_at,
  priority,
  status,
  version,
  created_at,
  updated_at,
  completed_at
)
SELECT
  id,
  tenant_id,
  obligation_id,
  title,
  description,
  assignee_user_id,
  due_at,
  priority,
  status,
  version,
  created_at,
  updated_at,
  CASE WHEN status = 'COMPLETED' THEN updated_at ELSE NULL END
FROM tasks;

DROP TABLE tasks;
ALTER TABLE tasks_0025 RENAME TO tasks;

CREATE INDEX IF NOT EXISTS idx_tasks_tenant_status
  ON tasks (tenant_id, status, due_at);

-- مفتاح مركّب فريد: يلزم لربط سجل الانتقالات بالمهمة داخل نفس المستأجر
-- بمفتاح أجنبي مركّب، فلا يمكن تسجيل انتقال لمهمة مستأجر آخر.
CREATE UNIQUE INDEX IF NOT EXISTS idx_tasks_tenant_id
  ON tasks (tenant_id, id);

CREATE INDEX IF NOT EXISTS idx_tasks_tenant_assignee
  ON tasks (tenant_id, assignee_user_id, status, due_at);

-- إعادة إنشاء ضوابط العزل بين المستأجرين التي أُنشئت في 0007 و0008.
-- إسقاط جدول tasks في خطوة إعادة البناء **يُسقط كل مشغّلاته**، فإهمال إعادة
-- إنشائها يحذف ضابطاً أمنياً على مستوى القاعدة بصمت: كان يمكن ربط مهمة
-- مستأجر بالتزام مستأجر آخر مباشرةً بـSQL. هذا الضابط جزء من الهجرة لا زينة.
CREATE TRIGGER IF NOT EXISTS trg_tasks_obligation_same_tenant_insert
BEFORE INSERT ON tasks
FOR EACH ROW
WHEN NOT EXISTS (
  SELECT 1 FROM obligations
  WHERE id = NEW.obligation_id AND tenant_id = NEW.tenant_id
)
BEGIN
  SELECT RAISE(ABORT, 'task obligation tenant mismatch');
END;

CREATE TRIGGER IF NOT EXISTS trg_tasks_obligation_same_tenant_update
BEFORE UPDATE OF obligation_id, tenant_id ON tasks
FOR EACH ROW
WHEN NOT EXISTS (
  SELECT 1 FROM obligations
  WHERE id = NEW.obligation_id AND tenant_id = NEW.tenant_id
)
BEGIN
  SELECT RAISE(ABORT, 'task obligation tenant mismatch');
END;

-- مفتاح مركّب فريد على المستخدمين: يلزم لربط سجل الانتقالات بالفاعل بمفتاح
-- أجنبي مركّب. أُنشئ أصلاً في 0011، ويُعاد إنشاؤه إدراجاً تجاهلياً هنا حتى تكون
-- هذه الهجرة مكتفية بذاتها في كل تركيبة مخطط جزئية (تركيبات الاختبار مثلاً)،
-- فغياب الفهرس يجعل SQLite يرفض الرابط بـ«foreign key mismatch».
CREATE UNIQUE INDEX IF NOT EXISTS idx_local_users_tenant_id
  ON local_users (tenant_id, id);

-- سجل الانتقالات الحقيقي — لكل تغيير حالة صف واحد، لا يُحدَّث ولا يُحذف.
CREATE TABLE task_state_transitions (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  task_id TEXT NOT NULL,
  from_status TEXT NOT NULL,
  to_status TEXT NOT NULL,
  actor_user_id TEXT NOT NULL,
  reason TEXT,
  resource_version INTEGER NOT NULL CHECK (resource_version > 0),
  created_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, task_id) REFERENCES tasks (tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, actor_user_id) REFERENCES local_users (tenant_id, id) ON DELETE RESTRICT,
  CHECK (from_status <> to_status)
);

CREATE INDEX IF NOT EXISTS idx_task_transitions_task
  ON task_state_transitions (tenant_id, task_id, created_at ASC, id ASC);

-- منع تعديل سجل الانتقالات: السجل دليل تدقيقي لا ورقة عمل.
CREATE TRIGGER prevent_task_transition_update
BEFORE UPDATE ON task_state_transitions
BEGIN
  SELECT RAISE(ABORT, 'سجل انتقال حالة المهمة غير قابل للتعديل');
END;

CREATE TRIGGER prevent_task_transition_delete
BEFORE DELETE ON task_state_transitions
BEGIN
  SELECT RAISE(ABORT, 'سجل انتقال حالة المهمة غير قابل للحذف');
END;

-- =====================================================================
-- تصحيح بيانات غير مُتلِف: الموافقات المعلّقة اليتيمة على المهام
-- ---------------------------------------------------------------------
-- العيب: كان بذر الديمو (والإنشاء القديم للمهام) يُنشئ موافقة معلّقة على مهمة
-- حالتها OPEN. وبعد إغلاق دورة الحياة صار قرار هذه الموافقة **مستحيلاً** بحكم
-- التصميم: القرار يشترط أن تكون المهمة في READY_FOR_REVIEW وبإصدار مطابق،
-- فتبقى الصفوف معلّقة إلى الأبد وتُضخّم «الاعتمادات المعلّقة» وقائمة الانتباه
-- ببنود لا يمكن حسمها.
--
-- السياسة المتبعة: **لا حذف صامت**. الصف يبقى كاملاً ويُصنَّف CANCELLED (قيمة
-- مشروعة في القيد الأصلي) مع سبب صريح يذكر المصدر، فيبقى الأثر قابلاً للتدقيق
-- ولا يُقدَّم بعد الآن كعمل معلّق حقيقي.
--
-- الشرط محصور بدقة: لا يُمَس أي صف لا يطابق (مهمة بانتظار المراجعة بإصدار
-- الموافقة نفسه) — أي أن الموافقات الصحيحة القائمة تبقى معلّقة كما هي.
-- =====================================================================
UPDATE approvals
SET decision = 'CANCELLED',
    decision_by = 'NEXUS_MIGRATION_0025',
    decision_reason = 'أُلغيت عند تصحيح دورة حياة المهمة: موافقة معلّقة على مهمة ليست بانتظار المراجعة أو لا يطابق إصدارها إصدار المهمة، فقرارها غير ممكن.',
    decided_at = updated_at
WHERE resource_type = 'TASK'
  AND decision = 'PENDING'
  AND NOT EXISTS (
    SELECT 1
    FROM tasks
    WHERE tasks.id = approvals.resource_id
      AND tasks.tenant_id = approvals.tenant_id
      AND tasks.status = 'READY_FOR_REVIEW'
      AND tasks.version = approvals.resource_version
  );
