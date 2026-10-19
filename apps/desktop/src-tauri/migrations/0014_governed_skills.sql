PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS skills (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 160),
  description TEXT NOT NULL CHECK (length(trim(description)) BETWEEN 1 AND 2000),
  status TEXT NOT NULL CHECK (status IN ('ACTIVE', 'DISABLED')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, name),
  UNIQUE (tenant_id, id)
);

CREATE TABLE IF NOT EXISTS skill_versions (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  skill_id TEXT NOT NULL,
  version_number INTEGER NOT NULL CHECK (version_number > 0),
  instruction TEXT NOT NULL CHECK (length(trim(instruction)) BETWEEN 1 AND 2048),
  input_contract TEXT NOT NULL CHECK (length(trim(input_contract)) BETWEEN 1 AND 2048),
  output_contract TEXT NOT NULL CHECK (length(trim(output_contract)) BETWEEN 1 AND 2048),
  constraints TEXT NOT NULL CHECK (length(trim(constraints)) BETWEEN 1 AND 2048),
  created_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id, skill_id) REFERENCES skills(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, skill_id, version_number),
  UNIQUE (tenant_id, id)
);

CREATE TABLE IF NOT EXISTS agent_skill_assignments (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  agent_id TEXT NOT NULL,
  skill_version_id TEXT NOT NULL,
  assigned_at TEXT NOT NULL,
  unassigned_at TEXT,
  FOREIGN KEY (tenant_id, agent_id) REFERENCES agents(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, skill_version_id) REFERENCES skill_versions(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, agent_id, skill_version_id)
);

CREATE INDEX IF NOT EXISTS idx_skills_tenant_status_updated
  ON skills (tenant_id, status, updated_at DESC, id ASC);
CREATE INDEX IF NOT EXISTS idx_skill_versions_tenant_skill_version
  ON skill_versions (tenant_id, skill_id, version_number DESC);
CREATE INDEX IF NOT EXISTS idx_agent_skill_assignments_tenant_agent
  ON agent_skill_assignments (tenant_id, agent_id, assigned_at DESC, id ASC);

INSERT OR IGNORE INTO permissions (id, code, resource_type, action, classification_ceiling, created_at)
VALUES
  ('permission_skill_read', 'skill:read', 'SKILL', 'READ', 'HIGHLY_RESTRICTED', datetime('now')),
  ('permission_skill_create', 'skill:create', 'SKILL', 'CREATE', 'HIGHLY_RESTRICTED', datetime('now')),
  ('permission_skill_update', 'skill:update', 'SKILL', 'UPDATE', 'HIGHLY_RESTRICTED', datetime('now')),
  ('permission_skill_assign', 'skill:assign', 'SKILL', 'ASSIGN', 'HIGHLY_RESTRICTED', datetime('now'));

CREATE TRIGGER IF NOT EXISTS prevent_skill_version_update
BEFORE UPDATE ON skill_versions
BEGIN
  SELECT RAISE(ABORT, 'نسخة Skill غير قابلة للتعديل');
END;

CREATE TRIGGER IF NOT EXISTS prevent_skill_version_delete
BEFORE DELETE ON skill_versions
BEGIN
  SELECT RAISE(ABORT, 'نسخة Skill غير قابلة للحذف');
END;
