PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS agents (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  name TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 160),
  description TEXT NOT NULL CHECK (length(trim(description)) BETWEEN 1 AND 2000),
  status TEXT NOT NULL CHECK (status IN ('ACTIVE', 'DISABLED')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, name)
);

CREATE INDEX IF NOT EXISTS idx_agents_tenant_status_updated
  ON agents (tenant_id, status, updated_at DESC, id ASC);
