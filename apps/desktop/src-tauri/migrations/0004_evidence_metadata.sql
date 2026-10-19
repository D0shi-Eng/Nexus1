PRAGMA foreign_keys = ON;

ALTER TABLE evidence_items ADD COLUMN label TEXT NOT NULL DEFAULT 'Unlabeled evidence';
ALTER TABLE evidence_items ADD COLUMN expires_at TEXT;
CREATE INDEX IF NOT EXISTS idx_evidence_tenant_obligation_status ON evidence_items (tenant_id, obligation_id, status, expires_at);
