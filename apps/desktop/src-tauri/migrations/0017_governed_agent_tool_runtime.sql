PRAGMA defer_foreign_keys = ON;

-- =====================================================================
-- 0017 — MASTER PHASE 03: GOVERNED AGENT & TOOL RUNTIME
-- إضافية تراكمية بالكامل عدا إعادة بناء جدولين مبررتين:
-- agent_runs (سماح WAITING_APPROVAL مع started_at محفوظ — إيقاف منتصف
-- التنفيذ لموافقة أداة) وagent_execution_results (توسيع رموز الفشل).
-- =====================================================================

-- 1) بروفايل تنفيذ الوكيل: تعليمة تنفيذ إصدارية محصنة، منفصلة عن description
CREATE TABLE agent_execution_profiles (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  agent_id TEXT NOT NULL,
  version_number INTEGER NOT NULL CHECK (version_number > 0),
  instruction TEXT NOT NULL CHECK (length(trim(instruction)) BETWEEN 1 AND 2048),
  created_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, agent_id) REFERENCES agents(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, agent_id, version_number),
  UNIQUE (tenant_id, id)
);

CREATE INDEX idx_agent_execution_profiles_tenant_agent
  ON agent_execution_profiles (tenant_id, agent_id, version_number DESC);

CREATE TRIGGER prevent_agent_execution_profile_update
BEFORE UPDATE ON agent_execution_profiles
BEGIN
  SELECT RAISE(ABORT, 'نسخة بروفايل تنفيذ الوكيل غير قابلة للتعديل');
END;

CREATE TRIGGER prevent_agent_execution_profile_delete
BEFORE DELETE ON agent_execution_profiles
BEGIN
  SELECT RAISE(ABORT, 'نسخة بروفايل تنفيذ الوكيل غير قابلة للحذف');
END;

-- 2) ربط هوية التنفيذ الدقيقة (يبقى عبر إعادة تشغيل التطبيق)
CREATE TABLE agent_run_execution_bindings (
  agent_run_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  agent_run_revision INTEGER NOT NULL CHECK (agent_run_revision > 0),
  agent_id TEXT NOT NULL,
  execution_profile_version_id TEXT NOT NULL,
  task_content TEXT NOT NULL CHECK (length(trim(task_content)) BETWEEN 1 AND 2048),
  task_content_hash TEXT NOT NULL CHECK (length(task_content_hash) > 0),
  model_id_at_request TEXT CHECK (model_id_at_request IS NULL OR length(model_id_at_request) BETWEEN 1 AND 256),
  bound_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, agent_run_id) REFERENCES agent_runs(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, agent_id) REFERENCES agents(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, execution_profile_version_id) REFERENCES agent_execution_profiles(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, agent_run_id)
);

CREATE TRIGGER prevent_agent_run_execution_binding_update
BEFORE UPDATE ON agent_run_execution_bindings
BEGIN
  SELECT RAISE(ABORT, 'ربط هوية تنفيذ التشغيل غير قابل للتغيير');
END;

CREATE TRIGGER prevent_agent_run_execution_binding_delete
BEFORE DELETE ON agent_run_execution_bindings
BEGIN
  SELECT RAISE(ABORT, 'ربط هوية تنفيذ التشغيل غير قابل للحذف');
END;

-- 3) أهلية وكيل ↔ نسخة أداة دقيقة
CREATE TABLE agent_tool_assignments (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  agent_id TEXT NOT NULL,
  tool_version_id TEXT NOT NULL,
  assigned_at TEXT NOT NULL,
  unassigned_at TEXT,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, agent_id) REFERENCES agents(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, tool_version_id) REFERENCES tool_versions(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, agent_id, tool_version_id)
);

CREATE INDEX idx_agent_tool_assignments_tenant_agent
  ON agent_tool_assignments (tenant_id, agent_id, assigned_at DESC, id ASC);

-- 4) سجل ربط المحوّلات الموثوق (خلفي فقط — لا IPC للتحرير)
CREATE TABLE tool_adapter_bindings (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  tool_version_id TEXT NOT NULL,
  adapter_key TEXT NOT NULL CHECK (length(trim(adapter_key)) BETWEEN 1 AND 128),
  bound_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, tool_version_id) REFERENCES tool_versions(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, tool_version_id)
);

CREATE TRIGGER prevent_tool_adapter_binding_update
BEFORE UPDATE ON tool_adapter_bindings
BEGIN
  SELECT RAISE(ABORT, 'ربط محوّل موثوق غير قابل للتغيير');
END;

CREATE TRIGGER prevent_tool_adapter_binding_delete
BEFORE DELETE ON tool_adapter_bindings
BEGIN
  SELECT RAISE(ABORT, 'ربط محوّل موثوق غير قابل للحذف');
END;

-- 5) عقد استدعاء الأداة
CREATE TABLE tool_invocations (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  agent_run_id TEXT NOT NULL,
  agent_run_revision INTEGER NOT NULL CHECK (agent_run_revision > 0),
  tool_id TEXT NOT NULL,
  tool_version_id TEXT NOT NULL,
  adapter_key TEXT,
  input_json TEXT NOT NULL CHECK (length(input_json) BETWEEN 2 AND 2048),
  input_hash TEXT NOT NULL CHECK (length(input_hash) > 0),
  policy_decision TEXT NOT NULL CHECK (policy_decision IN ('ALLOW', 'REQUIRE_APPROVAL', 'DENY')),
  policy_reason TEXT NOT NULL CHECK (length(trim(policy_reason)) BETWEEN 1 AND 128),
  status TEXT NOT NULL CHECK (status IN ('WAITING_APPROVAL', 'RUNNING', 'COMPLETED', 'FAILED', 'CANCELLED')),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  failure_code TEXT,
  created_at TEXT NOT NULL,
  started_at TEXT,
  finished_at TEXT,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, agent_run_id) REFERENCES agent_runs(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, tool_version_id) REFERENCES tool_versions(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, agent_run_id, tool_version_id, input_hash),
  CHECK (
    (status = 'WAITING_APPROVAL' AND started_at IS NULL AND finished_at IS NULL AND failure_code IS NULL)
    OR (status = 'RUNNING' AND started_at IS NOT NULL AND finished_at IS NULL AND failure_code IS NULL)
    OR (status = 'COMPLETED' AND started_at IS NOT NULL AND finished_at IS NOT NULL AND failure_code IS NULL)
    OR (status = 'FAILED' AND started_at IS NOT NULL AND finished_at IS NOT NULL AND failure_code IS NOT NULL)
    OR (status = 'CANCELLED' AND finished_at IS NOT NULL AND failure_code IS NULL)
  )
);

CREATE INDEX idx_tool_invocations_tenant_run
  ON tool_invocations (tenant_id, agent_run_id, created_at DESC, id ASC);

CREATE TRIGGER prevent_tool_invocation_identity_update
BEFORE UPDATE OF tenant_id, agent_run_id, agent_run_revision, tool_id, tool_version_id, adapter_key, input_json, input_hash, policy_decision, policy_reason ON tool_invocations
BEGIN
  SELECT RAISE(ABORT, 'هوية استدعاء الأداة غير قابلة للتغيير');
END;

CREATE TRIGGER prevent_tool_invocation_terminal_revival
BEFORE UPDATE ON tool_invocations
WHEN OLD.status IN ('COMPLETED', 'FAILED', 'CANCELLED')
BEGIN
  SELECT RAISE(ABORT, 'استدعاء أداة منتهٍ لا يعود نشطاً');
END;

-- 6) نتيجة استدعاء الأداة (محصنة تماماً)
CREATE TABLE tool_invocation_results (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  tool_invocation_id TEXT NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('COMPLETED', 'FAILED')),
  output_json TEXT CHECK (output_json IS NULL OR length(output_json) <= 1024),
  failure_code TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, tool_invocation_id) REFERENCES tool_invocations(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, tool_invocation_id),
  CHECK (
    (outcome = 'COMPLETED' AND output_json IS NOT NULL AND failure_code IS NULL)
    OR (outcome = 'FAILED' AND output_json IS NULL AND failure_code IS NOT NULL)
  )
);

CREATE TRIGGER prevent_tool_invocation_result_update
BEFORE UPDATE ON tool_invocation_results
BEGIN
  SELECT RAISE(ABORT, 'نتيجة استدعاء الأداة غير قابلة للتعديل');
END;

CREATE TRIGGER prevent_tool_invocation_result_delete
BEFORE DELETE ON tool_invocation_results
BEGIN
  SELECT RAISE(ABORT, 'نتيجة استدعاء الأداة غير قابلة للحذف');
END;

-- 7) ربط موافقة استدعاء الأداة (يفهرس المفاتيح الأمنية الدقيقة)
CREATE TABLE tool_invocation_approval_links (
  approval_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  tool_invocation_id TEXT NOT NULL,
  resource_version INTEGER NOT NULL CHECK (resource_version > 0),
  created_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id, approval_id) REFERENCES approvals(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, tool_invocation_id) REFERENCES tool_invocations(tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX idx_tool_invocation_approval_links_invocation
  ON tool_invocation_approval_links (tenant_id, tool_invocation_id, resource_version, approval_id);

CREATE UNIQUE INDEX idx_approvals_tool_invocation_pending_requirement
  ON approvals (
    tenant_id,
    resource_type,
    resource_id,
    resource_version,
    approval_requirement_code
  )
  WHERE resource_type = 'TOOL_INVOCATION' AND decision = 'PENDING';

CREATE TRIGGER validate_tool_invocation_approval_link_insert
BEFORE INSERT ON tool_invocation_approval_links
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM approvals
    WHERE id = NEW.approval_id
      AND tenant_id = NEW.tenant_id
      AND resource_type = 'TOOL_INVOCATION'
      AND resource_id = NEW.tool_invocation_id
      AND resource_version = NEW.resource_version
      AND approval_requirement_code IS NOT NULL
  ) THEN RAISE(ABORT, 'رابط موافقة استدعاء الأداة غير متوافق') END;
END;

CREATE TRIGGER prevent_tool_invocation_approval_binding_update
BEFORE UPDATE OF tenant_id, resource_type, resource_id, resource_version, approval_requirement_code ON approvals
WHEN EXISTS (
  SELECT 1
  FROM tool_invocation_approval_links
  WHERE approval_id = OLD.id
)
BEGIN
  SELECT RAISE(ABORT, 'ربط موافقة استدعاء الأداة غير قابل للتغيير');
END;

-- 8) إعادة بناء agent_runs: السماح بإيقاف منتصف التنفيذ
--    (WAITING_APPROVAL/QUEUED بعد استئناف مع started_at محفوظ)
CREATE TABLE agent_runs_0017 (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  agent_id TEXT NOT NULL,
  requested_by_user_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('QUEUED', 'RUNNING', 'WAITING_APPROVAL', 'COMPLETED', 'FAILED', 'CANCELLED')),
  request_summary TEXT NOT NULL CHECK (request_summary = 'GOVERNED_EXECUTION_REQUEST'),
  revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
  started_at TEXT,
  completed_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, agent_id) REFERENCES agents(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, requested_by_user_id) REFERENCES local_users(tenant_id, id) ON DELETE RESTRICT,
  CHECK (
    (status IN ('QUEUED', 'WAITING_APPROVAL') AND completed_at IS NULL)
    OR (status = 'RUNNING' AND started_at IS NOT NULL AND completed_at IS NULL)
    OR (status IN ('COMPLETED', 'FAILED') AND started_at IS NOT NULL AND completed_at IS NOT NULL)
    OR (status = 'CANCELLED' AND completed_at IS NOT NULL)
  )
);

INSERT INTO agent_runs_0017 (
  id, tenant_id, agent_id, requested_by_user_id, status, request_summary,
  revision, started_at, completed_at, created_at, updated_at
)
SELECT
  id, tenant_id, agent_id, requested_by_user_id, status, request_summary,
  revision, started_at, completed_at, created_at, updated_at
FROM agent_runs;

DROP TABLE agent_runs;
ALTER TABLE agent_runs_0017 RENAME TO agent_runs;

CREATE INDEX IF NOT EXISTS idx_agent_runs_tenant_updated
  ON agent_runs (tenant_id, updated_at DESC, id ASC);
CREATE INDEX IF NOT EXISTS idx_agent_runs_tenant_agent_created
  ON agent_runs (tenant_id, agent_id, created_at DESC, id ASC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_runs_tenant_id
  ON agent_runs (tenant_id, id);

-- 9) إعادة بناء agent_execution_results: توسيع رموز فشل أدوات التشغيل
CREATE TABLE agent_execution_results_0017 (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  agent_run_id TEXT NOT NULL,
  agent_run_revision INTEGER NOT NULL CHECK (agent_run_revision > 0),
  outcome TEXT NOT NULL CHECK (outcome IN ('COMPLETED', 'FAILED')),
  provider_id TEXT NOT NULL CHECK (length(provider_id) > 0 AND length(provider_id) <= 128),
  model_id TEXT,
  output TEXT CHECK (output IS NULL OR length(output) <= 32768),
  failure_code TEXT CHECK (failure_code IN (
    'PROVIDER_UNAVAILABLE', 'PROVIDER_TIMEOUT', 'MODEL_UNAVAILABLE', 'PROVIDER_RESPONSE_INVALID',
    'EXECUTION_INFRASTRUCTURE_ERROR', 'TOOL_AUTHORIZATION_DENIED', 'TOOL_INPUT_INVALID',
    'TOOL_CALL_LIMIT_EXCEEDED', 'TOOL_EXECUTION_FAILED', 'TOOL_RESULT_TOO_LARGE', 'EXECUTION_NOT_CONFIGURED'
  )),
  provider_generated_at TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, agent_run_id) REFERENCES agent_runs(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, agent_run_id),
  CHECK (
    (outcome = 'COMPLETED' AND model_id IS NOT NULL AND output IS NOT NULL AND length(trim(output)) > 0 AND failure_code IS NULL AND provider_generated_at IS NOT NULL)
    OR (outcome = 'FAILED' AND model_id IS NULL AND output IS NULL AND failure_code IS NOT NULL AND provider_generated_at IS NULL)
  )
);

INSERT INTO agent_execution_results_0017 (
  id, tenant_id, agent_run_id, agent_run_revision, outcome,
  provider_id, model_id, output, failure_code, provider_generated_at, created_at
)
SELECT
  id, tenant_id, agent_run_id, agent_run_revision, outcome,
  provider_id, model_id, output, failure_code, provider_generated_at, created_at
FROM agent_execution_results;

DROP TABLE agent_execution_results;
ALTER TABLE agent_execution_results_0017 RENAME TO agent_execution_results;

CREATE INDEX IF NOT EXISTS idx_agent_execution_results_tenant_run
  ON agent_execution_results (tenant_id, agent_run_id);

CREATE TRIGGER IF NOT EXISTS prevent_agent_execution_result_update
BEFORE UPDATE ON agent_execution_results
BEGIN
  SELECT RAISE(ABORT, 'نتيجة تنفيذ الوكيل غير قابلة للتعديل');
END;

CREATE TRIGGER IF NOT EXISTS prevent_agent_execution_result_delete
BEFORE DELETE ON agent_execution_results
BEGIN
  SELECT RAISE(ABORT, 'نتيجة تنفيذ الوكيل غير قابلة للحذف');
END;

-- 10) صلاحيتان ضيقتان جديدتان (نمط 0014/0016)
INSERT OR IGNORE INTO permissions (id, code, resource_type, action, classification_ceiling, created_at)
VALUES
  ('permission_tool_assign', 'tool:assign', 'TOOL', 'ASSIGN', 'HIGHLY_RESTRICTED', datetime('now')),
  ('permission_integration_update', 'integration:update', 'INTEGRATION', 'UPDATE', 'HIGHLY_RESTRICTED', datetime('now'));
