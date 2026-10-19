PRAGMA foreign_keys = ON;

-- توسعة الالتزام من ملخص لوحة القيادة إلى سجل عمل قابل للإدارة مع الحفاظ على الأعمدة القديمة المتوافقة.
ALTER TABLE obligations ADD COLUMN description TEXT NOT NULL DEFAULT '';
ALTER TABLE obligations ADD COLUMN priority TEXT NOT NULL DEFAULT 'MEDIUM' CHECK (priority IN ('CRITICAL', 'HIGH', 'MEDIUM', 'LOW'));
ALTER TABLE obligations ADD COLUMN compliance_status TEXT NOT NULL DEFAULT 'REQUIRES_REVIEW' CHECK (compliance_status IN ('COMPLIANT', 'PARTIALLY_COMPLIANT', 'NON_COMPLIANT', 'NOT_ASSESSED', 'REQUIRES_REVIEW'));
ALTER TABLE obligations ADD COLUMN review_status TEXT NOT NULL DEFAULT 'NOT_REVIEWED' CHECK (review_status IN ('NOT_REVIEWED', 'IN_REVIEW', 'REVIEWED', 'REJECTED'));
ALTER TABLE obligations ADD COLUMN source_type TEXT NOT NULL DEFAULT 'UNKNOWN' CHECK (source_type IN ('OFFICIAL', 'INTERNAL', 'DEMO', 'SYNTHETIC', 'USER_PROVIDED', 'UNKNOWN'));
ALTER TABLE obligations ADD COLUMN archived_at TEXT;
ALTER TABLE obligations ADD COLUMN archived_by TEXT;

-- ترحيل البيانات التجريبية الحالية إلى وصف صريح يمنع التباسها مع مصدر رسمي.
UPDATE obligations
SET priority = severity,
    description = CASE
      WHEN description = '' THEN 'DEMO / SYNTHETIC obligation record for local product verification.'
      ELSE description
    END,
    compliance_status = CASE
      WHEN status = 'COMPLETED' THEN 'COMPLIANT'
      WHEN status = 'AT_RISK' THEN 'NON_COMPLIANT'
      WHEN status = 'REQUIRES_REVIEW' THEN 'REQUIRES_REVIEW'
      ELSE 'NOT_ASSESSED'
    END,
    review_status = CASE
      WHEN verification_state = 'HUMAN_VERIFIED' THEN 'REVIEWED'
      WHEN verification_state = 'REQUIRES_REVIEW' THEN 'IN_REVIEW'
      ELSE 'NOT_REVIEWED'
    END,
    source_type = CASE
      WHEN source_id IN (
        SELECT id FROM regulatory_sources
        WHERE authority LIKE 'DEMO / SYNTHETIC%'
           OR source_url LIKE 'https://example.invalid/demo/%'
      ) THEN 'SYNTHETIC'
      ELSE 'UNKNOWN'
    END;

-- تمنع هذه القيود ربط مصدر أو مورد تابع لمستأجر آخر بالالتزام، حتى لو كان المعرّف موجوداً.
CREATE TRIGGER IF NOT EXISTS trg_obligations_source_same_tenant_insert
BEFORE INSERT ON obligations
FOR EACH ROW
WHEN NOT EXISTS (
  SELECT 1 FROM regulatory_sources
  WHERE id = NEW.source_id AND tenant_id = NEW.tenant_id
)
BEGIN
  SELECT RAISE(ABORT, 'obligation source tenant mismatch');
END;

CREATE TRIGGER IF NOT EXISTS trg_obligations_source_same_tenant_update
BEFORE UPDATE OF source_id, tenant_id ON obligations
FOR EACH ROW
WHEN NOT EXISTS (
  SELECT 1 FROM regulatory_sources
  WHERE id = NEW.source_id AND tenant_id = NEW.tenant_id
)
BEGIN
  SELECT RAISE(ABORT, 'obligation source tenant mismatch');
END;

CREATE TRIGGER IF NOT EXISTS trg_evidence_obligation_same_tenant_insert
BEFORE INSERT ON evidence_items
FOR EACH ROW
WHEN NOT EXISTS (
  SELECT 1 FROM obligations
  WHERE id = NEW.obligation_id AND tenant_id = NEW.tenant_id
)
BEGIN
  SELECT RAISE(ABORT, 'evidence obligation tenant mismatch');
END;

CREATE TRIGGER IF NOT EXISTS trg_tasks_obligation_same_tenant_insert
BEFORE INSERT ON tasks
FOR EACH ROW
WHEN NOT EXISTS (
  SELECT 1 FROM obligations
  WHERE id = NEW.obligation_id AND tenant_id = NEW.tenant_id
)
BEGIN
  SELECT RAISE(ABORT, 'task obligation tenant mismatch');
END;

CREATE TRIGGER IF NOT EXISTS trg_risks_obligation_same_tenant_insert
BEFORE INSERT ON risks
FOR EACH ROW
WHEN NOT EXISTS (
  SELECT 1 FROM obligations
  WHERE id = NEW.obligation_id AND tenant_id = NEW.tenant_id
)
BEGIN
  SELECT RAISE(ABORT, 'risk obligation tenant mismatch');
END;

CREATE INDEX IF NOT EXISTS idx_obligations_tenant_active_priority_due
ON obligations (tenant_id, archived_at, priority, deadline);

CREATE INDEX IF NOT EXISTS idx_obligations_tenant_compliance_review
ON obligations (tenant_id, compliance_status, review_status);

CREATE INDEX IF NOT EXISTS idx_obligations_tenant_owner
ON obligations (tenant_id, owner);
