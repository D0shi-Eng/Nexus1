PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS tools (
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

CREATE TABLE IF NOT EXISTS tool_versions (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  tool_id TEXT NOT NULL,
  version_number INTEGER NOT NULL CHECK (version_number > 0),
  input_contract TEXT NOT NULL CHECK (length(trim(input_contract)) BETWEEN 1 AND 2048),
  output_contract TEXT NOT NULL CHECK (length(trim(output_contract)) BETWEEN 1 AND 2048),
  mutation TEXT NOT NULL CHECK (mutation IN ('NONE', 'STATE_CHANGE')),
  externality TEXT NOT NULL CHECK (externality IN ('INTERNAL', 'EXTERNAL')),
  privilege TEXT NOT NULL CHECK (privilege IN ('STANDARD', 'PRIVILEGED')),
  reversibility TEXT NOT NULL CHECK (reversibility IN ('NOT_APPLICABLE', 'REVERSIBLE', 'CONDITIONAL', 'IRREVERSIBLE')),
  created_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id, tool_id) REFERENCES tools(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, tool_id, version_number),
  UNIQUE (tenant_id, id),
  CHECK ((mutation = 'NONE' AND reversibility = 'NOT_APPLICABLE') OR (mutation = 'STATE_CHANGE' AND reversibility <> 'NOT_APPLICABLE'))
);

CREATE INDEX IF NOT EXISTS idx_tools_tenant_status_updated
  ON tools (tenant_id, status, updated_at DESC, id ASC);
CREATE INDEX IF NOT EXISTS idx_tool_versions_tenant_tool_version
  ON tool_versions (tenant_id, tool_id, version_number DESC);

INSERT OR IGNORE INTO permissions (id, code, resource_type, action, classification_ceiling, created_at)
VALUES
  ('permission_tool_read', 'tool:read', 'TOOL', 'READ', 'HIGHLY_RESTRICTED', datetime('now')),
  ('permission_tool_create', 'tool:create', 'TOOL', 'CREATE', 'HIGHLY_RESTRICTED', datetime('now')),
  ('permission_tool_update', 'tool:update', 'TOOL', 'UPDATE', 'HIGHLY_RESTRICTED', datetime('now'));

CREATE TRIGGER IF NOT EXISTS prevent_tool_version_update
BEFORE UPDATE ON tool_versions
BEGIN
  SELECT RAISE(ABORT, 'نسخة Tool غير قابلة للتعديل');
END;

CREATE TRIGGER IF NOT EXISTS prevent_tool_version_delete
BEFORE DELETE ON tool_versions
BEGIN
  SELECT RAISE(ABORT, 'نسخة Tool غير قابلة للحذف');
END;
