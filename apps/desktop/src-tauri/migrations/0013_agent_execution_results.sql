CREATE TABLE agent_execution_results (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  agent_run_id TEXT NOT NULL,
  agent_run_revision INTEGER NOT NULL CHECK (agent_run_revision > 0),
  outcome TEXT NOT NULL CHECK (outcome IN ('COMPLETED', 'FAILED')),
  provider_id TEXT NOT NULL CHECK (length(provider_id) > 0 AND length(provider_id) <= 128),
  model_id TEXT,
  output TEXT,
  failure_code TEXT CHECK (failure_code IN ('PROVIDER_UNAVAILABLE', 'PROVIDER_TIMEOUT', 'MODEL_UNAVAILABLE', 'PROVIDER_RESPONSE_INVALID', 'EXECUTION_INFRASTRUCTURE_ERROR')),
  provider_generated_at TEXT,
  created_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id, agent_run_id) REFERENCES agent_runs(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, agent_run_id),
  CHECK (
    (outcome = 'COMPLETED'
      AND model_id IS NOT NULL
      AND output IS NOT NULL
      AND length(output) > 0
      AND length(output) <= 32768
      AND failure_code IS NULL
      AND provider_generated_at IS NOT NULL)
    OR
    (outcome = 'FAILED'
      AND model_id IS NULL
      AND output IS NULL
      AND failure_code IS NOT NULL
      AND provider_generated_at IS NULL)
  )
);

CREATE INDEX idx_agent_execution_results_tenant_run
  ON agent_execution_results (tenant_id, agent_run_id);

CREATE TRIGGER prevent_agent_execution_result_update
BEFORE UPDATE ON agent_execution_results
BEGIN
  SELECT RAISE(ABORT, 'نتيجة تنفيذ الوكيل غير قابلة للتعديل');
END;

CREATE TRIGGER prevent_agent_execution_result_delete
BEFORE DELETE ON agent_execution_results
BEGIN
  SELECT RAISE(ABORT, 'نتيجة تنفيذ الوكيل غير قابلة للحذف');
END;
