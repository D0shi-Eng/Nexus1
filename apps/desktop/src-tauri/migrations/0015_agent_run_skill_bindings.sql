PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS agent_run_skill_bindings (
  agent_run_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  agent_run_revision INTEGER NOT NULL CHECK (agent_run_revision > 0),
  agent_id TEXT NOT NULL,
  skill_id TEXT NOT NULL,
  skill_version_id TEXT NOT NULL,
  bound_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id, agent_run_id) REFERENCES agent_runs(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, agent_id) REFERENCES agents(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, skill_id) REFERENCES skills(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, skill_version_id) REFERENCES skill_versions(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, agent_run_id),
  UNIQUE (tenant_id, skill_version_id, agent_run_id)
);

CREATE TRIGGER IF NOT EXISTS prevent_agent_run_skill_binding_update
BEFORE UPDATE ON agent_run_skill_bindings
BEGIN
  SELECT RAISE(ABORT, 'لا يجوز تعديل ربط Skill بعد تثبيته');
END;

CREATE TRIGGER IF NOT EXISTS prevent_agent_run_skill_binding_delete
BEFORE DELETE ON agent_run_skill_bindings
BEGIN
  SELECT RAISE(ABORT, 'لا يجوز حذف provenance التشغيل');
END;

CREATE INDEX IF NOT EXISTS idx_agent_run_skill_bindings_tenant_skill
  ON agent_run_skill_bindings (tenant_id, skill_id, skill_version_id);
