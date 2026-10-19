PRAGMA defer_foreign_keys = ON;

-- =====================================================================
-- 0020 — MASTER PHASE 04: INTELLIGENCE RUNTIME
-- قدرة الوكيل + الخطط والخطوات + التفويضات المطبوعة + ربط الاسترجاع +
-- التحقق وروابط الأدلة + موافقة الخطة + توسيع نتائج التنفيذ
-- (اقتباسات/تصنيف حقيقة) — إضافي بالكامل.
-- =====================================================================

-- 1) قدرة الوكيل: اختيار حتمي خلفي لتفويضات الخطط (لا يمنح أي سلطة)
ALTER TABLE agents ADD COLUMN capability TEXT NOT NULL DEFAULT 'GENERAL'
  CHECK (capability IN ('GENERAL', 'RESEARCH', 'COMPLIANCE', 'RISK', 'EVIDENCE', 'CYBERSECURITY', 'SYNTHESIS'));

-- 2) الخطة: هدف محدود + هاش خطوات + موافقة مربوطة بالهوية الدقيقة
CREATE TABLE plans (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  goal TEXT NOT NULL CHECK (length(trim(goal)) BETWEEN 1 AND 500),
  status TEXT NOT NULL CHECK (status IN ('PENDING_APPROVAL', 'ACTIVE', 'RUNNING', 'COMPLETED', 'FAILED', 'REJECTED', 'CANCELLED')),
  steps_hash TEXT NOT NULL CHECK (length(steps_hash) > 0),
  model_id_at_request TEXT CHECK (model_id_at_request IS NULL OR length(model_id_at_request) BETWEEN 1 AND 256),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  requested_by_user_id TEXT NOT NULL,
  synthesis_output TEXT CHECK (synthesis_output IS NULL OR length(synthesis_output) <= 8000),
  conflict_flag INTEGER NOT NULL DEFAULT 0 CHECK (conflict_flag IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, requested_by_user_id) REFERENCES local_users(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, id)
);

CREATE INDEX idx_plans_tenant_status
  ON plans (tenant_id, status, created_at DESC, id ASC);

-- 3) خطوة الخطة: قدرة مطلوبة من تعداد مغلق — المخطط يطلب قدرة ولا يمنحها
CREATE TABLE plan_steps (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  plan_id TEXT NOT NULL,
  step_index INTEGER NOT NULL CHECK (step_index BETWEEN 0 AND 3),
  objective TEXT NOT NULL CHECK (length(trim(objective)) BETWEEN 1 AND 500),
  required_capability TEXT NOT NULL CHECK (required_capability IN ('RESEARCH', 'COMPLIANCE', 'RISK', 'EVIDENCE', 'SYNTHESIS')),
  requires_knowledge INTEGER NOT NULL DEFAULT 0 CHECK (requires_knowledge IN (0, 1)),
  evidence_requirement TEXT NOT NULL CHECK (length(trim(evidence_requirement)) BETWEEN 1 AND 300),
  status TEXT NOT NULL CHECK (status IN ('PENDING', 'RUNNING', 'COMPLETED', 'FAILED', 'CANCELLED')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, plan_id) REFERENCES plans(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, plan_id, step_index),
  UNIQUE (tenant_id, id)
);

CREATE INDEX idx_plan_steps_tenant_plan
  ON plan_steps (tenant_id, plan_id, step_index ASC);

CREATE TRIGGER prevent_plan_step_identity_update
BEFORE UPDATE OF tenant_id, plan_id, step_index, objective, required_capability, requires_knowledge, evidence_requirement, created_at ON plan_steps
BEGIN
  SELECT RAISE(ABORT, 'هوية خطوة الخطة غير قابلة لإعادة الكتابة');
END;

-- 4) التفويض المطبوع: تفويض واحد لكل خطوة (CAS + نهاية حتمية)
CREATE TABLE agent_delegations (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  plan_id TEXT NOT NULL,
  plan_revision INTEGER NOT NULL CHECK (plan_revision > 0),
  step_id TEXT NOT NULL,
  source_kind TEXT NOT NULL CHECK (source_kind = 'SUPERVISOR'),
  target_agent_id TEXT NOT NULL,
  agent_run_id TEXT,
  objective TEXT NOT NULL CHECK (length(trim(objective)) BETWEEN 1 AND 500),
  allowed_context_refs_json TEXT NOT NULL CHECK (length(allowed_context_refs_json) <= 2048),
  allowed_skill_version_id TEXT,
  status TEXT NOT NULL CHECK (status IN ('PENDING', 'RUNNING', 'COMPLETED', 'FAILED', 'CANCELLED')),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  output TEXT CHECK (output IS NULL OR length(output) <= 8000),
  citations_json TEXT CHECK (citations_json IS NULL OR length(citations_json) <= 4096),
  truth_class TEXT CHECK (truth_class IN ('AI_INTERPRETATION', 'UNKNOWN', 'CONFLICTING')),
  failure_code TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  finished_at TEXT,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, plan_id) REFERENCES plans(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, step_id) REFERENCES plan_steps(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, target_agent_id) REFERENCES agents(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, agent_run_id) REFERENCES agent_runs(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, allowed_skill_version_id) REFERENCES skill_versions(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, step_id)
);

CREATE INDEX idx_agent_delegations_tenant_plan
  ON agent_delegations (tenant_id, plan_id, created_at ASC, id ASC);

CREATE TRIGGER prevent_agent_delegation_terminal_revival
BEFORE UPDATE ON agent_delegations
WHEN OLD.status IN ('COMPLETED', 'FAILED', 'CANCELLED')
BEGIN
  SELECT RAISE(ABORT, 'تفويض منتهٍ لا يعود نشطاً');
END;

-- 5) ربط استرجاع المعرفة بهوية التشغيل الدقيقة (قابل للتحقق عند التنفيذ)
CREATE TABLE knowledge_retrieval_bindings (
  agent_run_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  agent_run_revision INTEGER NOT NULL CHECK (agent_run_revision > 0),
  query TEXT NOT NULL CHECK (length(trim(query)) BETWEEN 1 AND 500),
  query_hash TEXT NOT NULL CHECK (length(query_hash) > 0),
  chunk_ids_json TEXT NOT NULL CHECK (length(chunk_ids_json) <= 4096),
  bound_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, agent_run_id) REFERENCES agent_runs(tenant_id, id) ON DELETE RESTRICT
);

CREATE TRIGGER prevent_knowledge_retrieval_binding_update
BEFORE UPDATE ON knowledge_retrieval_bindings
BEGIN
  SELECT RAISE(ABORT, 'ربط استرجاع المعرفة غير قابل للتغيير');
END;

CREATE TRIGGER prevent_knowledge_retrieval_binding_delete
BEFORE DELETE ON knowledge_retrieval_bindings
BEGIN
  SELECT RAISE(ABORT, 'ربط استرجاع المعرفة غير قابل للحذف');
END;

-- 6) التحقق: نتيجة حتمية/مختلطة + روابط أدلة إلى موارد حقيقية فقط
CREATE TABLE verification_runs (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  subject_kind TEXT NOT NULL CHECK (subject_kind IN ('AGENT_RUN', 'PLAN')),
  agent_run_id TEXT,
  plan_id TEXT,
  outcome TEXT NOT NULL CHECK (outcome IN ('SUPPORTED', 'PARTIALLY_SUPPORTED', 'UNSUPPORTED', 'CONFLICTING', 'UNKNOWN', 'INSUFFICIENT_EVIDENCE')),
  method TEXT NOT NULL CHECK (method IN ('DETERMINISTIC_ONLY', 'DETERMINISTIC_AND_AI')),
  deterministic_checks_json TEXT NOT NULL CHECK (length(deterministic_checks_json) <= 4096),
  ai_evaluation_json TEXT CHECK (ai_evaluation_json IS NULL OR length(ai_evaluation_json) <= 4096),
  ai_analysis_run_id TEXT,
  created_by TEXT NOT NULL,
  created_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, agent_run_id) REFERENCES agent_runs(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, plan_id) REFERENCES plans(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (ai_analysis_run_id) REFERENCES ai_analysis_runs(id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, id),
  CHECK (
    (subject_kind = 'AGENT_RUN' AND agent_run_id IS NOT NULL AND plan_id IS NULL)
    OR (subject_kind = 'PLAN' AND plan_id IS NOT NULL AND agent_run_id IS NULL)
  ),
  CHECK (method != 'DETERMINISTIC_AND_AI' OR ai_evaluation_json IS NOT NULL)
);

CREATE INDEX idx_verification_runs_tenant_subject
  ON verification_runs (tenant_id, subject_kind, agent_run_id, plan_id, created_at DESC, id ASC);

CREATE TRIGGER prevent_verification_run_update
BEFORE UPDATE ON verification_runs
BEGIN
  SELECT RAISE(ABORT, 'سجل التحقق غير قابل للتعديل');
END;

CREATE TRIGGER prevent_verification_run_delete
BEFORE DELETE ON verification_runs
BEGIN
  SELECT RAISE(ABORT, 'سجل التحقق غير قابل للحذف');
END;

CREATE TABLE verification_evidence_links (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  verification_run_id TEXT NOT NULL,
  evidence_kind TEXT NOT NULL CHECK (evidence_kind IN ('KNOWLEDGE_CITATION', 'TOOL_RESULT', 'AGENT_RUN', 'AUDIT_EVENT')),
  evidence_ref TEXT NOT NULL CHECK (length(evidence_ref) BETWEEN 1 AND 128),
  created_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, verification_run_id) REFERENCES verification_runs(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, verification_run_id, evidence_kind, evidence_ref)
);

CREATE TRIGGER prevent_verification_evidence_link_update
BEFORE UPDATE ON verification_evidence_links
BEGIN
  SELECT RAISE(ABORT, 'رابط دليل التحقق غير قابل للتعديل');
END;

-- 7) ربط موافقة الخطة (نفس نمط ربط موافقات التشغيل/الأدوات في MP03)
CREATE TABLE plan_approval_links (
  approval_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  plan_id TEXT NOT NULL,
  resource_version INTEGER NOT NULL CHECK (resource_version > 0),
  created_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id, approval_id) REFERENCES approvals(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, plan_id) REFERENCES plans(tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX idx_plan_approval_links_plan
  ON plan_approval_links (tenant_id, plan_id, resource_version, approval_id);

CREATE UNIQUE INDEX idx_approvals_plan_pending_requirement
  ON approvals (
    tenant_id,
    resource_type,
    resource_id,
    resource_version,
    approval_requirement_code
  )
  WHERE resource_type = 'PLAN' AND decision = 'PENDING';

CREATE TRIGGER validate_plan_approval_link_insert
BEFORE INSERT ON plan_approval_links
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM approvals
    WHERE id = NEW.approval_id
      AND tenant_id = NEW.tenant_id
      AND resource_type = 'PLAN'
      AND resource_id = NEW.plan_id
      AND resource_version = NEW.resource_version
      AND approval_requirement_code IS NOT NULL
  ) THEN RAISE(ABORT, 'رابط موافقة الخطة غير متوافق') END;
END;

CREATE TRIGGER prevent_plan_approval_binding_update
BEFORE UPDATE OF tenant_id, resource_type, resource_id, resource_version, approval_requirement_code ON approvals
WHEN EXISTS (
  SELECT 1
  FROM plan_approval_links
  WHERE approval_id = OLD.id
)
BEGIN
  SELECT RAISE(ABORT, 'ربط موافقة الخطة غير قابل للتغيير');
END;

-- 8) توسيع نتائج التنفيذ: اقتباسات متحققة + تصنيف حقيقة افتراضي AI_INTERPRETATION
ALTER TABLE agent_execution_results ADD COLUMN citations_json TEXT
  CHECK (citations_json IS NULL OR length(citations_json) <= 4096);
ALTER TABLE agent_execution_results ADD COLUMN truth_class TEXT NOT NULL DEFAULT 'AI_INTERPRETATION'
  CHECK (truth_class IN ('AI_INTERPRETATION', 'UNKNOWN', 'CONFLICTING'));

-- 9) صلاحيات ضيقة: قراءة/إنشاء/تنفيذ الخطط
INSERT OR IGNORE INTO permissions (id, code, resource_type, action, classification_ceiling, created_at)
VALUES
  ('permission_plan_read', 'plan:read', 'PLAN', 'READ', 'HIGHLY_RESTRICTED', datetime('now')),
  ('permission_plan_create', 'plan:create', 'PLAN', 'CREATE', 'HIGHLY_RESTRICTED', datetime('now')),
  ('permission_plan_execute', 'plan:execute', 'PLAN', 'EXECUTE', 'HIGHLY_RESTRICTED', datetime('now'));
