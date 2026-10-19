PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS ai_model_registry (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  provider_kind TEXT NOT NULL CHECK (provider_kind IN ('OPENAI_COMPATIBLE', 'AZURE_OPENAI', 'ANTHROPIC', 'GOOGLE', 'LOCAL')),
  model_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
  data_residency TEXT NOT NULL CHECK (data_residency IN ('LOCAL', 'REGIONAL', 'UNSPECIFIED')),
  allowed_classification_ceiling TEXT NOT NULL CHECK (allowed_classification_ceiling IN ('PUBLIC', 'INTERNAL', 'CONFIDENTIAL', 'RESTRICTED', 'HIGHLY_RESTRICTED')),
  requires_human_approval INTEGER NOT NULL DEFAULT 1 CHECK (requires_human_approval IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, provider_kind, model_id)
);

CREATE TABLE IF NOT EXISTS ai_analysis_runs (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  model_registry_id TEXT NOT NULL,
  purpose TEXT NOT NULL,
  input_hash TEXT NOT NULL,
  output_json TEXT,
  confidence REAL,
  human_review_state TEXT NOT NULL CHECK (human_review_state IN ('NOT_REQUESTED', 'PENDING', 'APPROVED', 'REJECTED')),
  status TEXT NOT NULL CHECK (status IN ('QUEUED', 'COMPLETED', 'FAILED', 'BLOCKED')),
  evidence_reference_ids_json TEXT NOT NULL,
  created_at TEXT NOT NULL,
  completed_at TEXT,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (model_registry_id) REFERENCES ai_model_registry(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_ai_models_tenant_enabled ON ai_model_registry (tenant_id, enabled);
CREATE INDEX IF NOT EXISTS idx_ai_runs_tenant_status ON ai_analysis_runs (tenant_id, status, created_at);
