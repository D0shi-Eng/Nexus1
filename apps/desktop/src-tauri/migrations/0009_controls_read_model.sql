PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS controls (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  title TEXT NOT NULL CHECK (length(trim(title)) > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_controls_tenant_updated_id
  ON controls (tenant_id, updated_at DESC, id ASC);
