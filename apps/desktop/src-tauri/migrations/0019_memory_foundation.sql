PRAGMA defer_foreign_keys = ON;

-- =====================================================================
-- 0019 — MASTER PHASE 04: MEMORY FOUNDATION
-- طبقات ذاكرة متميزة إلزامياً (عاملة/حدثية/دلالية/تنظيمية/وكيل) في جدول
-- واحد ضيق النطاق: المنشأ والحقيقة محصنان، والتغيير المسموح فقط
-- status/truth_class عبر البوابات المحكومة (ترقية بشرية حصراً).
-- =====================================================================

CREATE TABLE memory_items (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  memory_type TEXT NOT NULL CHECK (memory_type IN ('WORKING', 'EPISODIC', 'SEMANTIC', 'ORGANIZATIONAL', 'AGENT')),
  scope_resource_type TEXT NOT NULL CHECK (scope_resource_type IN ('AGENT_RUN', 'PLAN', 'AGENT', 'TENANT')),
  scope_resource_id TEXT CHECK (scope_resource_id IS NULL OR length(scope_resource_id) BETWEEN 1 AND 128),
  agent_id TEXT,
  content TEXT NOT NULL CHECK (length(trim(content)) BETWEEN 1 AND 2000),
  truth_class TEXT NOT NULL CHECK (truth_class IN ('FACT', 'VERIFIED', 'CLAIM', 'ASSUMPTION', 'INFERENCE', 'OPINION', 'UNKNOWN', 'CONFLICTING', 'AI_INTERPRETATION')),
  provenance_kind TEXT NOT NULL CHECK (provenance_kind IN ('AGENT_RUN', 'PLAN', 'HUMAN', 'SYSTEM')),
  provenance_ref TEXT CHECK (provenance_ref IS NULL OR length(provenance_ref) BETWEEN 1 AND 128),
  status TEXT NOT NULL CHECK (status IN ('ACTIVE', 'PROPOSED', 'DISABLED')),
  created_by TEXT NOT NULL,
  expires_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, agent_id) REFERENCES agents(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, id),
  CHECK ((memory_type = 'AGENT' AND agent_id IS NOT NULL) OR memory_type != 'AGENT'),
  CHECK ((scope_resource_type = 'TENANT' AND scope_resource_id IS NULL) OR scope_resource_id IS NOT NULL),
  CHECK (memory_type != 'WORKING' OR expires_at IS NOT NULL)
);

CREATE INDEX idx_memory_items_tenant_type_status
  ON memory_items (tenant_id, memory_type, status, created_at DESC, id ASC);

CREATE INDEX idx_memory_items_tenant_scope
  ON memory_items (tenant_id, scope_resource_type, scope_resource_id, created_at DESC, id ASC);

CREATE INDEX idx_memory_items_tenant_agent
  ON memory_items (tenant_id, agent_id, status, created_at DESC, id ASC);

-- المنشأ والمحتوى والنطاق محصنة؛ المسموح فقط status/truth_class/updated_at
-- (بوابات الخدمة تحكم من يغيرها — الترقية إلى VERIFIED بشرية حصراً)
CREATE TRIGGER prevent_memory_item_identity_update
BEFORE UPDATE OF tenant_id, memory_type, scope_resource_type, scope_resource_id, agent_id, content, provenance_kind, provenance_ref, created_by, expires_at, created_at ON memory_items
BEGIN
  SELECT RAISE(ABORT, 'هوية عنصر الذاكرة غير قابلة لإعادة الكتابة');
END;

-- 6) صلاحيتان ضيقتان: قراءة / إدارة (تعطيل + ترقية بشرية)
INSERT OR IGNORE INTO permissions (id, code, resource_type, action, classification_ceiling, created_at)
VALUES
  ('permission_memory_read', 'memory:read', 'MEMORY', 'READ', 'HIGHLY_RESTRICTED', datetime('now')),
  ('permission_memory_manage', 'memory:manage', 'MEMORY', 'MANAGE', 'HIGHLY_RESTRICTED', datetime('now'));
