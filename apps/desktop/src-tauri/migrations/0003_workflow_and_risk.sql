PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS risks (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  obligation_id TEXT NOT NULL,
  title TEXT NOT NULL,
  severity TEXT NOT NULL CHECK (severity IN ('CRITICAL', 'HIGH', 'MEDIUM', 'LOW')),
  likelihood INTEGER NOT NULL CHECK (likelihood BETWEEN 1 AND 5),
  impact INTEGER NOT NULL CHECK (impact BETWEEN 1 AND 5),
  status TEXT NOT NULL CHECK (status IN ('OPEN', 'MITIGATING', 'ACCEPTED', 'CLOSED')),
  owner TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (obligation_id) REFERENCES obligations(id) ON DELETE RESTRICT
);

CREATE TABLE IF NOT EXISTS tasks (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  obligation_id TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT NOT NULL,
  assignee_user_id TEXT,
  due_at TEXT,
  priority TEXT NOT NULL CHECK (priority IN ('CRITICAL', 'HIGH', 'MEDIUM', 'LOW')),
  status TEXT NOT NULL CHECK (status IN ('OPEN', 'IN_PROGRESS', 'BLOCKED', 'COMPLETED')),
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (obligation_id) REFERENCES obligations(id) ON DELETE RESTRICT,
  FOREIGN KEY (assignee_user_id) REFERENCES local_users(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS approvals (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  resource_type TEXT NOT NULL,
  resource_id TEXT NOT NULL,
  resource_version INTEGER NOT NULL CHECK (resource_version > 0),
  requested_by TEXT NOT NULL,
  required_role_code TEXT NOT NULL,
  decision TEXT NOT NULL CHECK (decision IN ('PENDING', 'APPROVED', 'REJECTED', 'CANCELLED')),
  decision_by TEXT,
  decision_reason TEXT,
  decided_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_risks_tenant_status ON risks (tenant_id, severity, status);
CREATE INDEX IF NOT EXISTS idx_tasks_tenant_status ON tasks (tenant_id, status, due_at);
CREATE INDEX IF NOT EXISTS idx_approvals_tenant_decision ON approvals (tenant_id, decision, created_at);
