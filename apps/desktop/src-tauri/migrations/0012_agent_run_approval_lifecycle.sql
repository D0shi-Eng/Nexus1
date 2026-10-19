PRAGMA foreign_keys = ON;

CREATE TABLE agent_runs_0012 (
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
    (status IN ('QUEUED', 'WAITING_APPROVAL') AND started_at IS NULL AND completed_at IS NULL)
    OR (status = 'RUNNING' AND started_at IS NOT NULL AND completed_at IS NULL)
    OR (status IN ('COMPLETED', 'FAILED') AND started_at IS NOT NULL AND completed_at IS NOT NULL)
    OR (status = 'CANCELLED' AND completed_at IS NOT NULL)
  )
);

INSERT INTO agent_runs_0012 (
  id,
  tenant_id,
  agent_id,
  requested_by_user_id,
  status,
  request_summary,
  revision,
  started_at,
  completed_at,
  created_at,
  updated_at
)
SELECT
  id,
  tenant_id,
  agent_id,
  requested_by_user_id,
  status,
  request_summary,
  1,
  CASE WHEN status = 'WAITING_APPROVAL' THEN NULL ELSE started_at END,
  completed_at,
  created_at,
  updated_at
FROM agent_runs;

DROP TABLE agent_runs;
ALTER TABLE agent_runs_0012 RENAME TO agent_runs;

CREATE INDEX IF NOT EXISTS idx_agent_runs_tenant_updated
  ON agent_runs (tenant_id, updated_at DESC, id ASC);

CREATE INDEX IF NOT EXISTS idx_agent_runs_tenant_agent_created
  ON agent_runs (tenant_id, agent_id, created_at DESC, id ASC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_runs_tenant_id
  ON agent_runs (tenant_id, id);

ALTER TABLE approvals ADD COLUMN approval_requirement_code TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_approvals_tenant_id
  ON approvals (tenant_id, id);

CREATE TABLE agent_run_approval_links (
  approval_id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  agent_run_id TEXT NOT NULL,
  resource_version INTEGER NOT NULL CHECK (resource_version > 0),
  created_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id, approval_id) REFERENCES approvals(tenant_id, id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, agent_run_id) REFERENCES agent_runs(tenant_id, id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_agent_run_approval_links_run
  ON agent_run_approval_links (tenant_id, agent_run_id, resource_version, approval_id);

CREATE UNIQUE INDEX IF NOT EXISTS idx_approvals_agent_run_pending_requirement
  ON approvals (
    tenant_id,
    resource_type,
    resource_id,
    resource_version,
    approval_requirement_code
  )
  WHERE resource_type = 'AGENT_RUN' AND decision = 'PENDING';

CREATE TRIGGER validate_agent_run_approval_link_insert
BEFORE INSERT ON agent_run_approval_links
BEGIN
  SELECT CASE WHEN NOT EXISTS (
    SELECT 1
    FROM approvals
    WHERE id = NEW.approval_id
      AND tenant_id = NEW.tenant_id
      AND resource_type = 'AGENT_RUN'
      AND resource_id = NEW.agent_run_id
      AND resource_version = NEW.resource_version
      AND approval_requirement_code IS NOT NULL
  ) THEN RAISE(ABORT, 'رابط موافقة التشغيل غير متوافق') END;
END;

CREATE TRIGGER prevent_agent_run_approval_binding_update
BEFORE UPDATE OF tenant_id, resource_type, resource_id, resource_version, approval_requirement_code ON approvals
WHEN EXISTS (
  SELECT 1
  FROM agent_run_approval_links
  WHERE approval_id = OLD.id
)
BEGIN
  SELECT RAISE(ABORT, 'ربط موافقة التشغيل غير قابل للتغيير');
END;
