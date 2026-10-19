-- =====================================================================
-- 0023 — سجل موفري الذكاء الاصطناعي (AI provider registry)
-- ---------------------------------------------------------------------
-- لا يمس أي هجرة تاريخية. يضيف فقط:
--   1) جدول مثيلات الموفرين ai_provider_registry — معرّف مستقر لكل مثيل
--      (وليس لكل نوع)، بعنوان أساسي وحالة اتصال وعلامة وجود سر.
--   2) تعميم ai_model_registry ليصبح صف النموذج قابلاً للانتماء إلى مثيل
--      موفر محدد عبر provider_id، مع توسيع قيد provider_kind ليشمل أنواع
--      الموفرين الجديدة، وإعادة بناء الجدول بنمط «جدول جديد + نسخ + استبدال»
--      لأن SQLite لا يسمح بتعديل قيد CHECK عبر ALTER TABLE.
--
-- قرارات تصميم ملزمة:
--   * لا يُخزَّن أي سر في القاعدة إطلاقاً. العمود credential_configured
--     علامة منطقية (0/1) فقط، والسر نفسه في مخزن نظام التشغيل المحمي.
--   * connection_state تبدأ UNVERIFIED دائماً: حفظ مفتاح لا يعني اتصالاً.
--     لا مسار في المخطط يسمح بأن يصبح الصف CONNECTED بغير اختبار ناجح فعلي.
--   * بنية ai_model_registry الأصلية محفوظة حرفياً: نفس الأعمدة ونفس القيود
--     ونفس قيد التفرّد (tenant_id, provider_kind, model_id) ونفس المفاتيح
--     الأجنبية على tenants(id)، فلا يتغيّر سلوك أي مسار قائم.
--
-- ملاحظة تنفيذية حاسمة (مُختبرة فعلياً لا مفترضة):
--   إسقاط ai_model_registry يفشل فوراً بـ 1811 (SQLITE_CONSTRAINT_FOREIGNKEY)
--   لأن ai_analysis_runs.model_registry_id يشير إليه، وPRAGMA foreign_keys
--   و PRAGMA defer_foreign_keys **لا أثر لهما داخل معاملة قائمة** (الأول
--   يُتجاهل صامتاً، والثاني يظهر كـ 1 لكن الفحص يبقى فورياً). لذلك يعطّل
--   المشغّل فرض المفاتيح على الاتصال قبل بدء معاملة هذه الهجرة ويعيد تفعيله
--   بعد الالتزام مع فحص سلامة؛ التفاصيل في
--   infrastructure::database::apply_ai_provider_registry_rebuild.
-- =====================================================================

PRAGMA foreign_keys = ON;

-- ===== 1) جدول مثيلات الموفرين =====

CREATE TABLE IF NOT EXISTS ai_provider_registry (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  provider_id TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN (
    'OLLAMA',
    'LM_STUDIO',
    'OPENAI_COMPATIBLE',
    'OPENAI',
    'DEEPSEEK',
    'ANTHROPIC',
    'QWEN',
    'ZAI',
    'XAI',
    'OPENROUTER'
  )),
  display_name TEXT NOT NULL,
  base_url TEXT NOT NULL,
  -- النقل غير المشفّر مسموح فقط للحلقي: القيد في المخطط يمنع كتابة صف
  -- يخالف سياسة الشبكة حتى لو أخطأ مسار تطبيقي (دفاع في العمق).
  transport TEXT NOT NULL CHECK (transport IN ('HTTP_LOOPBACK', 'HTTPS')),
  enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
  connection_state TEXT NOT NULL DEFAULT 'UNVERIFIED' CHECK (connection_state IN (
    'CONNECTED',
    'DISCONNECTED',
    'UNAVAILABLE',
    'UNVERIFIED'
  )),
  connection_checked_at TEXT,
  -- علامة وجود السر فقط. لا عمود ولا جدول ولا فهرس يحمل السر.
  credential_configured INTEGER NOT NULL DEFAULT 0 CHECK (credential_configured IN (0, 1)),
  built_in INTEGER NOT NULL DEFAULT 0 CHECK (built_in IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  -- تفرّد على مستوى المستأجر: نفس معرّف الموفر لا يتكرر داخل مستأجر واحد.
  UNIQUE (tenant_id, provider_id)
);

CREATE INDEX IF NOT EXISTS idx_ai_providers_tenant_enabled
  ON ai_provider_registry (tenant_id, enabled);
CREATE INDEX IF NOT EXISTS idx_ai_providers_tenant_state
  ON ai_provider_registry (tenant_id, connection_state, updated_at DESC);

-- ===== 2) تعميم سجل النماذج: إضافة provider_id وتوسيع provider_kind =====
-- إعادة البناء داخل الهجرة نفسها (المشغّل يلفّ السكربت في معاملة واحدة).
-- legacy_alter_table=ON يمنع SQLite من إعادة كتابة تعريف المفتاح الأجنبي في
-- ai_analysis_runs عند إعادة التسمية، فيبقى مرجعه ai_model_registry حرفياً.
-- الاختبار provider_registry_rebuild_preserves_* يتحقق من ذلك نصّاً ومن نجاح
-- الإدراج في الجدول الابن بعد الترقية.
-- حرس إعادة التشغيل في المسار البرمجي (has provider_id) وليس هنا: هذا السكربت
-- يُنفَّذ مرة واحدة فقط.

-- legacy_alter_table=ON يُضبط على الاتصال قبل بدء المعاملة في
-- apply_ai_provider_registry_rebuild (PRAGMA لا أثر موثوق له داخل معاملة هنا،
-- والإعداد قبلها يمنع SQLite من إعادة كتابة مرجع المفتاح الأجنبي في
-- ai_analysis_runs عند إعادة التسمية).

-- إعادة البناء مشروطة بغياب العمود provider_id. الغرض: السكربت نفسه يُنفَّذ
-- مرة واحدة بالضبط عند تسجيل الإصدار 23، وحينها قد تكون القاعدة مُرقّاة جزئياً
-- (الجدول الجديد موجود) — الشرط يمنع إعادة بناء ثانية تُتلف البيانات.
-- ملاحظة: SQLite يُحلّل السكربت كاملاً حتى مع شرط خاطئ، لذا يجب أن تكون كل
-- مراجع ai_model_registry_new صحيحة نحوياً (وهي كذلك).
CREATE TABLE IF NOT EXISTS ai_model_registry_new (
  id TEXT PRIMARY KEY,
  tenant_id TEXT NOT NULL,
  -- النوع يبقى منسوخاً على الصف للقراءة السريعة، ويُشتق من مثيل الموفر.
  provider_kind TEXT NOT NULL CHECK (provider_kind IN (
    'LOCAL',
    'OPENAI_COMPATIBLE',
    'AZURE_OPENAI',
    'ANTHROPIC',
    'GOOGLE',
    'OLLAMA',
    'LM_STUDIO',
    'OPENAI',
    'DEEPSEEK',
    'QWEN',
    'ZAI',
    'XAI',
    'OPENROUTER'
  )),
  model_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
  data_residency TEXT NOT NULL CHECK (data_residency IN ('LOCAL', 'REGIONAL', 'UNSPECIFIED')),
  allowed_classification_ceiling TEXT NOT NULL CHECK (allowed_classification_ceiling IN ('PUBLIC', 'INTERNAL', 'CONFIDENTIAL', 'RESTRICTED', 'HIGHLY_RESTRICTED')),
  requires_human_approval INTEGER NOT NULL DEFAULT 1 CHECK (requires_human_approval IN (0, 1)),
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  -- مثيل الموفر المالك للنموذج. NULL يعني صفاً تاريخياً سابقاً لـ 0023.
  provider_id TEXT,
  -- اكتمال القدرات: NULL يعني «غير معروف بثقة» ولا يعني «غير مدعوم».
  context_limit INTEGER CHECK (context_limit IS NULL OR context_limit > 0),
  supports_tools INTEGER CHECK (supports_tools IS NULL OR supports_tools IN (0, 1)),
  supports_structured_output INTEGER CHECK (supports_structured_output IS NULL OR supports_structured_output IN (0, 1)),
  supports_vision INTEGER CHECK (supports_vision IS NULL OR supports_vision IN (0, 1)),
  capability_source TEXT NOT NULL DEFAULT 'UNKNOWN' CHECK (capability_source IN (
    'DISCOVERED',
    'DECLARED',
    'UNKNOWN'
  )),
  available INTEGER NOT NULL DEFAULT 0 CHECK (available IN (0, 1)),
  last_discovered_at TEXT,
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE RESTRICT,
  FOREIGN KEY (provider_id) REFERENCES ai_provider_registry(id) ON DELETE RESTRICT,
  UNIQUE (tenant_id, provider_kind, model_id)
);

-- ملاحظة: نسخ الصفوف القديمة إلى الجدول المرشَّح **ليس هنا** بل في
-- apply_ai_provider_registry_rebuild داخل نفس المعاملة، لأن النسخ يحتاج فرعين
-- مختلفين (قاعدة قديمة: عمود provider_id غير موجود ولا يجوز ذكره في SELECT؛
-- قاعدة جديدة: الجدول فارغ أصلاً فلا حاجة إلى نسخ). اختيار الفرع بعمود موجود
-- أو غائب قرار يُتخذ من مخطط القاعدة لا من نص SQL ثابت.

-- ===== ملاحظة إلزامية عن الإسقاط وإعادة التسمية =====
-- SQLite **لا يقبل `WHERE` على جمل DDL** (مُتحقَّق عملياً: `DROP TABLE x WHERE ...`
-- يُنتج `near "WHERE": syntax error`) ولا يملك `IF` على DDL. لذا يكون الترتيب:
--   1) هذا السكربت: إنشاء الجداول والنسخ والبذر والربط — كله بلا DDL مشروط.
--   2) ثم infrastructure::database::apply_ai_provider_registry_rebuild يكمل
--      داخل المعاملة نفسها: إسقاط الجدول القديم، إعادة تسمية المرشَّح، إنشاء
--      الفهارس، وتسجيل الإصدار — كتلة ذرّية واحدة (إمّا كلها أو لا شيء).
--
-- ===== 3) بذر مثيل Ollama المدمج =====
-- الموفر المحلي الوحيد المدعوم أصلاً يُزرع كصف مدمج (built_in = 1) لكل مستأجر
-- قائم كي يبقى المسار المحلي عاملاً بلا أي إعداد من المستخدم. حالته UNVERIFIED
-- ولا علامة سر — لا يدّعي المخطط اتصالاً.
INSERT INTO ai_provider_registry (
  id, tenant_id, provider_id, kind, display_name, base_url, transport,
  enabled, connection_state, connection_checked_at, credential_configured,
  built_in, created_at, updated_at
)
SELECT
  'ai_provider_' || tenants.id || '_ollama_local',
  tenants.id,
  'OLLAMA_LOCAL',
  'OLLAMA',
  'Ollama (Local)',
  'http://127.0.0.1:11434',
  'HTTP_LOOPBACK',
  0,
  'UNVERIFIED',
  NULL,
  0,
  1,
  tenants.created_at,
  tenants.updated_at
FROM tenants
WHERE NOT EXISTS (
  SELECT 1 FROM ai_provider_registry existing
  WHERE existing.tenant_id = tenants.id
    AND existing.provider_id = 'OLLAMA_LOCAL'
);

-- ملاحظة: ربط النماذج المحلية التاريخية بمثيل Ollama المزروع يجري في
-- apply_ai_provider_registry_rebuild **بعد** خطوة نسخ الصفوف إلى المرشَّح.
-- لو نُفِّذ هنا لكان المرشَّح فارغاً بعد (صفر صف مطابق) فلا يُربط شيء.
