PRAGMA defer_foreign_keys = ON;

-- =====================================================================
-- 0018 — MASTER PHASE 04: KNOWLEDGE FOUNDATION
-- مصادر المعرفة + المستندات + القطع الحتمية + فهرس FTS5 (عربي/إنجليزي/
-- مختلط عبر unicode61) — كلها مقيدة بالمستأجر ومحصنة ضد إعادة كتابة الأصل.
-- =====================================================================

-- 1) مصدر المعرفة: الهوية والمنشأ والحقيقة — لا تعديل للأصل، فقط تغيير status
CREATE TABLE knowledge_sources (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  source_type TEXT NOT NULL CHECK (source_type IN ('USER_PROVIDED_DOCUMENT', 'NEXUS_INTERNAL_RECORD', 'REGULATORY_SOURCE_REFERENCE')),
  title TEXT NOT NULL CHECK (length(trim(title)) BETWEEN 1 AND 200),
  origin TEXT NOT NULL CHECK (length(trim(origin)) BETWEEN 1 AND 200),
  provenance TEXT NOT NULL CHECK (provenance IN ('USER_PROVIDED', 'NEXUS_INTERNAL', 'DEMO_SYNTHETIC', 'EXTERNAL_REFERENCE', 'UNKNOWN')),
  status TEXT NOT NULL CHECK (status IN ('ACTIVE', 'DISABLED', 'FAILED')),
  content_hash TEXT CHECK (content_hash IS NULL OR length(content_hash) > 0),
  origin_ref_resource_type TEXT CHECK (origin_ref_resource_type IN ('OBLIGATION', 'EVIDENCE', 'REGULATORY_SOURCE')),
  origin_ref_resource_id TEXT CHECK (origin_ref_resource_id IS NULL OR length(origin_ref_resource_id) BETWEEN 1 AND 128),
  created_by TEXT NOT NULL,
  created_at TEXT NOT NULL,
  ingested_at TEXT,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, created_by) REFERENCES local_users(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, id),
  UNIQUE (tenant_id, content_hash),
  CHECK (
    (origin_ref_resource_type IS NULL AND origin_ref_resource_id IS NULL)
    OR (origin_ref_resource_type IS NOT NULL AND origin_ref_resource_id IS NOT NULL)
  ),
  CHECK (status != 'ACTIVE' OR ingested_at IS NOT NULL)
);

CREATE INDEX idx_knowledge_sources_tenant_status
  ON knowledge_sources (tenant_id, status, created_at DESC, id ASC);

CREATE TRIGGER prevent_knowledge_source_identity_update
BEFORE UPDATE OF tenant_id, source_type, title, origin, provenance, content_hash, origin_ref_resource_type, origin_ref_resource_id, created_by, created_at, ingested_at ON knowledge_sources
BEGIN
  SELECT RAISE(ABORT, 'أصل مصدر المعرفة غير قابل لإعادة الكتابة');
END;

CREATE TRIGGER prevent_knowledge_source_delete
BEFORE DELETE ON knowledge_sources
BEGIN
  SELECT RAISE(ABORT, 'مصدر المعرفة لا يُحذف — عطّله بدلاً من ذلك');
END;

-- 2) مستند مستخرج واحد لكل عملية إدخال (نص/ماركداون) — غير قابل للتعديل
CREATE TABLE knowledge_documents (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  source_id TEXT NOT NULL,
  format TEXT NOT NULL CHECK (format IN ('TXT', 'MARKDOWN')),
  original_filename TEXT CHECK (original_filename IS NULL OR length(trim(original_filename)) BETWEEN 1 AND 200),
  extracted_text TEXT NOT NULL CHECK (length(extracted_text) > 0),
  char_count INTEGER NOT NULL CHECK (char_count > 0),
  extraction_status TEXT NOT NULL CHECK (extraction_status IN ('EXTRACTED', 'TEXT_EXTRACTION_UNAVAILABLE')),
  created_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, source_id) REFERENCES knowledge_sources(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, id)
);

CREATE INDEX idx_knowledge_documents_tenant_source
  ON knowledge_documents (tenant_id, source_id, created_at DESC, id ASC);

CREATE TRIGGER prevent_knowledge_document_update
BEFORE UPDATE ON knowledge_documents
BEGIN
  SELECT RAISE(ABORT, 'مستند المعرفة غير قابل للتعديل');
END;

CREATE TRIGGER prevent_knowledge_document_delete
BEFORE DELETE ON knowledge_documents
BEGIN
  SELECT RAISE(ABORT, 'مستند المعرفة غير قابل للحذف');
END;

-- 3) قطعة حتمية: نطاق محارف + هاش — إعادة البناء تعطي نفس القطع بترتيبها
CREATE TABLE knowledge_chunks (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  document_id TEXT NOT NULL,
  chunk_index INTEGER NOT NULL CHECK (chunk_index >= 0),
  text TEXT NOT NULL CHECK (length(text) > 0),
  text_hash TEXT NOT NULL CHECK (length(text_hash) > 0),
  char_start INTEGER NOT NULL CHECK (char_start >= 0),
  char_end INTEGER NOT NULL CHECK (char_end > char_start),
  char_count INTEGER NOT NULL CHECK (char_count > 0),
  created_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (tenant_id, document_id) REFERENCES knowledge_documents(tenant_id, id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, document_id, chunk_index),
  UNIQUE (tenant_id, id)
);

CREATE INDEX idx_knowledge_chunks_tenant_document
  ON knowledge_chunks (tenant_id, document_id, chunk_index ASC);

CREATE TRIGGER prevent_knowledge_chunk_update
BEFORE UPDATE ON knowledge_chunks
BEGIN
  SELECT RAISE(ABORT, 'قطعة المعرفة غير قابلة للتعديل');
END;

-- 4) فهرس FTS5 مع مزامنة مشغّلات (إدراج/حذف فقط — القطع غير قابلة للتعديل)
CREATE VIRTUAL TABLE knowledge_chunks_fts USING fts5(
  text,
  chunk_id UNINDEXED,
  tokenize = 'unicode61 remove_diacritics 1'
);

CREATE TRIGGER knowledge_chunk_fts_insert
AFTER INSERT ON knowledge_chunks
BEGIN
  INSERT INTO knowledge_chunks_fts (rowid, text, chunk_id)
  VALUES (NEW.rowid, NEW.text, NEW.id);
END;

CREATE TRIGGER knowledge_chunk_fts_delete
AFTER DELETE ON knowledge_chunks
BEGIN
  INSERT INTO knowledge_chunks_fts (knowledge_chunks_fts, rowid, text, chunk_id)
  VALUES ('delete', OLD.rowid, OLD.text, OLD.id);
END;

-- 5) صلاحيات ضيقة: قراءة / إدخال / تعطيل (نمط 0014/0016/0017)
INSERT OR IGNORE INTO permissions (id, code, resource_type, action, classification_ceiling, created_at)
VALUES
  ('permission_knowledge_read', 'knowledge:read', 'KNOWLEDGE', 'READ', 'HIGHLY_RESTRICTED', datetime('now')),
  ('permission_knowledge_ingest', 'knowledge:ingest', 'KNOWLEDGE', 'INGEST', 'HIGHLY_RESTRICTED', datetime('now')),
  ('permission_knowledge_disable', 'knowledge:disable', 'KNOWLEDGE', 'DISABLE', 'HIGHLY_RESTRICTED', datetime('now'));
