PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_migrations (
  version INTEGER PRIMARY KEY,
  applied_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS tenants (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  data_mode TEXT NOT NULL CHECK (data_mode IN ('DEMO_SYNTHETIC', 'PRODUCTION')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS organizations (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  legal_name TEXT NOT NULL,
  country_code TEXT NOT NULL,
  industry TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, legal_name)
);

CREATE TABLE IF NOT EXISTS regulatory_sources (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  authority TEXT NOT NULL,
  document_title TEXT NOT NULL,
  source_url TEXT,
  document_version TEXT,
  retrieved_at TEXT NOT NULL,
  official INTEGER NOT NULL CHECK (official IN (0, 1)),
  verification_state TEXT NOT NULL CHECK (verification_state IN ('UNVERIFIED', 'AI_ANALYZED', 'HUMAN_VERIFIED', 'REQUIRES_REVIEW')),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, authority, document_title, document_version)
);

CREATE TABLE IF NOT EXISTS obligations (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  source_id TEXT NOT NULL,
  title TEXT NOT NULL,
  owner TEXT NOT NULL,
  deadline TEXT,
  severity TEXT NOT NULL CHECK (severity IN ('CRITICAL', 'HIGH', 'MEDIUM', 'LOW')),
  status TEXT NOT NULL CHECK (status IN ('OPEN', 'IN_PROGRESS', 'AT_RISK', 'COMPLETED', 'REQUIRES_REVIEW')),
  applicability TEXT NOT NULL CHECK (applicability IN ('APPLIES', 'DOES_NOT_APPLY', 'UNCERTAIN', 'REQUIRES_REVIEW')),
  evidence_status TEXT NOT NULL CHECK (evidence_status IN ('MISSING', 'UPLOADED', 'PROCESSING', 'ANALYZED', 'VALID', 'EXPIRED', 'INVALID', 'REQUIRES_REVIEW')),
  verification_state TEXT NOT NULL CHECK (verification_state IN ('UNVERIFIED', 'AI_ANALYZED', 'HUMAN_VERIFIED', 'REQUIRES_REVIEW')),
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (source_id) REFERENCES regulatory_sources(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_obligations_tenant_status ON obligations (tenant_id, status, severity);
CREATE INDEX IF NOT EXISTS idx_obligations_tenant_deadline ON obligations (tenant_id, deadline);

CREATE TABLE IF NOT EXISTS evidence_items (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  obligation_id TEXT NOT NULL,
  classification TEXT NOT NULL CHECK (classification IN ('PUBLIC', 'INTERNAL', 'CONFIDENTIAL', 'RESTRICTED', 'HIGHLY_RESTRICTED')),
  status TEXT NOT NULL CHECK (status IN ('MISSING', 'UPLOADED', 'PROCESSING', 'ANALYZED', 'VALID', 'EXPIRED', 'INVALID', 'REQUIRES_REVIEW')),
  content_hash TEXT NOT NULL,
  version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (obligation_id) REFERENCES obligations(id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, content_hash, version)
);

CREATE TABLE IF NOT EXISTS audit_events (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  event_type TEXT NOT NULL,
  actor_name TEXT NOT NULL,
  resource_type TEXT NOT NULL,
  resource_id TEXT NOT NULL,
  occurred_at TEXT NOT NULL,
  previous_hash TEXT,
  event_hash TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, event_hash)
);

CREATE INDEX IF NOT EXISTS idx_audit_events_tenant_time ON audit_events (tenant_id, occurred_at DESC);
