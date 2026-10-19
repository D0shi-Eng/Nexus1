PRAGMA foreign_keys = ON;

CREATE UNIQUE INDEX IF NOT EXISTS idx_agents_tenant_id
  ON agents (tenant_id, id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_local_users_tenant_id
  ON local_users (tenant_id, id);

CREATE TABLE IF NOT EXISTS agent_runs (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  agent_id TEXT NOT NULL,
  requested_by_user_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('QUEUED', 'RUNNING', 'WAITING_APPROVAL', 'COMPLETED', 'FAILED', 'CANCELLED')),
  request_summary TEXT NOT NULL CHECK (request_summary = 'GOVERNED_EXECUTION_REQUEST'),
  started_at TEXT,
  completed_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, agent_id) REFERENCES agents(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, requested_by_user_id) REFERENCES local_users(tenant_id, id) ON DELETE RESTRICT,
  CHECK (
    (status = 'QUEUED' AND started_at IS NULL AND completed_at IS NULL)
    OR (status IN ('RUNNING', 'WAITING_APPROVAL') AND started_at IS NOT NULL AND completed_at IS NULL)
    OR (status IN ('COMPLETED', 'FAILED') AND started_at IS NOT NULL AND completed_at IS NOT NULL)
    OR (status = 'CANCELLED' AND completed_at IS NOT NULL)
  )
);

CREATE INDEX IF NOT EXISTS idx_agent_runs_tenant_updated
  ON agent_runs (tenant_id, updated_at DESC, id ASC);

CREATE INDEX IF NOT EXISTS idx_agent_runs_tenant_agent_created
  ON agent_runs (tenant_id, agent_id, created_at DESC, id ASC);
