PRAGMA foreign_keys = ON;

-- تمنع تحديث علاقة مورد قائم إلى التزام تابع لمستأجر آخر.
CREATE TRIGGER IF NOT EXISTS trg_evidence_obligation_same_tenant_update
BEFORE UPDATE OF obligation_id, tenant_id ON evidence_items
FOR EACH ROW
WHEN NOT EXISTS (
  SELECT 1 FROM obligations
  WHERE id = NEW.obligation_id AND tenant_id = NEW.tenant_id
)
BEGIN
  SELECT RAISE(ABORT, 'evidence obligation tenant mismatch');
END;

CREATE TRIGGER IF NOT EXISTS trg_tasks_obligation_same_tenant_update
BEFORE UPDATE OF obligation_id, tenant_id ON tasks
FOR EACH ROW
WHEN NOT EXISTS (
  SELECT 1 FROM obligations
  WHERE id = NEW.obligation_id AND tenant_id = NEW.tenant_id
)
BEGIN
  SELECT RAISE(ABORT, 'task obligation tenant mismatch');
END;

CREATE TRIGGER IF NOT EXISTS trg_risks_obligation_same_tenant_update
BEFORE UPDATE OF obligation_id, tenant_id ON risks
FOR EACH ROW
WHEN NOT EXISTS (
  SELECT 1 FROM obligations
  WHERE id = NEW.obligation_id AND tenant_id = NEW.tenant_id
)
BEGIN
  SELECT RAISE(ABORT, 'risk obligation tenant mismatch');
END;
