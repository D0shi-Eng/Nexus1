PRAGMA foreign_keys = ON;

-- =====================================================================
-- 0022 — تأليف الضوابط وربطها (Controls authoring and linking)
-- لا يمس أي هجرة تاريخية. يضيف فقط:
-- 1) حالة/تصنيف/مالك/فعالية الضابط مع طابع آخر تقييم.
-- 2) جداول ربط صريحة: ضابط↔التزام، ضابط↔خطر، ضابط↔دليل.
-- كل جدول ربط يحمل مستأجره صراحةً (tenant_id) مع مفتاح مركّب يمنع التكرار،
-- ومشغلات تمنع ربط كيان تابع لمستأجر آخر حتى لو وُجد المعرّف — نفس نمط 0007/0008.
-- =====================================================================

-- حالة الضابط دورة حياة صريحة: ACTIVE → INACTIVE → DISABLED.
ALTER TABLE controls ADD COLUMN status TEXT NOT NULL DEFAULT 'ACTIVE' CHECK (status IN ('ACTIVE', 'INACTIVE', 'DISABLED'));
ALTER TABLE controls ADD COLUMN category TEXT;
ALTER TABLE controls ADD COLUMN owner TEXT;
ALTER TABLE controls ADD COLUMN effectiveness TEXT NOT NULL DEFAULT 'NOT_ASSESSED' CHECK (effectiveness IN ('NOT_ASSESSED', 'EFFECTIVE', 'PARTIALLY_EFFECTIVE', 'INEFFECTIVE'));
ALTER TABLE controls ADD COLUMN last_assessed_at TEXT;
ALTER TABLE controls ADD COLUMN assessment_note TEXT;

CREATE INDEX IF NOT EXISTS idx_controls_tenant_status_updated
  ON controls (tenant_id, status, updated_at DESC, id ASC);

-- ===== روابط الضابط بالالتزامات =====
CREATE TABLE IF NOT EXISTS control_obligations (
  control_id TEXT NOT NULL,
  obligation_id TEXT NOT NULL,
  tenant_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (control_id, obligation_id),
  FOREIGN KEY (control_id) REFERENCES controls(id) ON DELETE RESTRICT,
  FOREIGN KEY (obligation_id) REFERENCES obligations(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_control_obligations_tenant_control
  ON control_obligations (tenant_id, control_id, created_at);
CREATE INDEX IF NOT EXISTS idx_control_obligations_tenant_obligation
  ON control_obligations (tenant_id, obligation_id);

-- ===== روابط الضابط بالمخاطر =====
CREATE TABLE IF NOT EXISTS control_risks (
  control_id TEXT NOT NULL,
  risk_id TEXT NOT NULL,
  tenant_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (control_id, risk_id),
  FOREIGN KEY (control_id) REFERENCES controls(id) ON DELETE RESTRICT,
  FOREIGN KEY (risk_id) REFERENCES risks(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_control_risks_tenant_control
  ON control_risks (tenant_id, control_id, created_at);
CREATE INDEX IF NOT EXISTS idx_control_risks_tenant_risk
  ON control_risks (tenant_id, risk_id);

-- ===== روابط الضابط بالأدلة =====
CREATE TABLE IF NOT EXISTS control_evidence (
  control_id TEXT NOT NULL,
  evidence_id TEXT NOT NULL,
  tenant_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (control_id, evidence_id),
  FOREIGN KEY (control_id) REFERENCES controls(id) ON DELETE RESTRICT,
  FOREIGN KEY (evidence_id) REFERENCES evidence_items(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_control_evidence_tenant_control
  ON control_evidence (tenant_id, control_id, created_at);
CREATE INDEX IF NOT EXISTS idx_control_evidence_tenant_evidence
  ON control_evidence (tenant_id, evidence_id);

-- مشغلات الإدراج: تمنع إنشاء رابط يشير إلى ضابط أو هدف تابع لمستأجر آخر.
CREATE TRIGGER IF NOT EXISTS trg_control_obligations_same_tenant_insert
BEFORE INSERT ON control_obligations
FOR EACH ROW
WHEN NOT EXISTS (SELECT 1 FROM controls WHERE id = NEW.control_id AND tenant_id = NEW.tenant_id)
  OR NOT EXISTS (SELECT 1 FROM obligations WHERE id = NEW.obligation_id AND tenant_id = NEW.tenant_id)
BEGIN
  SELECT RAISE(ABORT, 'control obligation tenant mismatch');
END;

CREATE TRIGGER IF NOT EXISTS trg_control_risks_same_tenant_insert
BEFORE INSERT ON control_risks
FOR EACH ROW
WHEN NOT EXISTS (SELECT 1 FROM controls WHERE id = NEW.control_id AND tenant_id = NEW.tenant_id)
  OR NOT EXISTS (SELECT 1 FROM risks WHERE id = NEW.risk_id AND tenant_id = NEW.tenant_id)
BEGIN
  SELECT RAISE(ABORT, 'control risk tenant mismatch');
END;

CREATE TRIGGER IF NOT EXISTS trg_control_evidence_same_tenant_insert
BEFORE INSERT ON control_evidence
FOR EACH ROW
WHEN NOT EXISTS (SELECT 1 FROM controls WHERE id = NEW.control_id AND tenant_id = NEW.tenant_id)
  OR NOT EXISTS (SELECT 1 FROM evidence_items WHERE id = NEW.evidence_id AND tenant_id = NEW.tenant_id)
BEGIN
  SELECT RAISE(ABORT, 'control evidence tenant mismatch');
END;

-- مشغلات التحديث: عزل المستأجر غير قابل للنقل بعد الإنشاء (نفس نمط 0008).
CREATE TRIGGER IF NOT EXISTS trg_control_obligations_same_tenant_update
BEFORE UPDATE OF control_id, obligation_id, tenant_id ON control_obligations
FOR EACH ROW
WHEN NOT EXISTS (SELECT 1 FROM controls WHERE id = NEW.control_id AND tenant_id = NEW.tenant_id)
  OR NOT EXISTS (SELECT 1 FROM obligations WHERE id = NEW.obligation_id AND tenant_id = NEW.tenant_id)
BEGIN
  SELECT RAISE(ABORT, 'control obligation tenant mismatch');
END;

CREATE TRIGGER IF NOT EXISTS trg_control_risks_same_tenant_update
BEFORE UPDATE OF control_id, risk_id, tenant_id ON control_risks
FOR EACH ROW
WHEN NOT EXISTS (SELECT 1 FROM controls WHERE id = NEW.control_id AND tenant_id = NEW.tenant_id)
  OR NOT EXISTS (SELECT 1 FROM risks WHERE id = NEW.risk_id AND tenant_id = NEW.tenant_id)
BEGIN
  SELECT RAISE(ABORT, 'control risk tenant mismatch');
END;

CREATE TRIGGER IF NOT EXISTS trg_control_evidence_same_tenant_update
BEFORE UPDATE OF control_id, evidence_id, tenant_id ON control_evidence
FOR EACH ROW
WHEN NOT EXISTS (SELECT 1 FROM controls WHERE id = NEW.control_id AND tenant_id = NEW.tenant_id)
  OR NOT EXISTS (SELECT 1 FROM evidence_items WHERE id = NEW.evidence_id AND tenant_id = NEW.tenant_id)
BEGIN
  SELECT RAISE(ABORT, 'control evidence tenant mismatch');
END;
