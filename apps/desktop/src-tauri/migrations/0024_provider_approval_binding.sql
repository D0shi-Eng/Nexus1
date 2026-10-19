-- =====================================================================
-- 0024 — ربط هوية وجهة التنفيذ بالموافقة (Provider Approval Binding)
-- ---------------------------------------------------------------------
-- العيبة المُثبتة (كانت HIGH مفتوحة ومُعلنة في PROJECT_STATE):
--   كان جدول `agent_run_execution_bindings` يربط **معرّف النموذج وحده**
--   (`model_id_at_request`). والموفر يُحلّ لحظة التنفيذ من **الاختيار الحالي**
--   في `ai_model_registry`، فالموافقة كانت تربط «ماذا يُشغَّل» لا «أين يُشغَّل».
--   ولو غُيّر الموفر المختار بعد الاعتماد إلى وجهة أخرى تعلن النموذج نفسه
--   (مزوّدان متوافقان مع OpenAI كلاهما يعلن `gpt-4o-mini`، أو مزوّد محلي
--   ومزوّد سحابي بنفس المعرّف) لجرى التنفيذ على وجهة **لم تُعتمد** بلا اعتراض.
--
-- الإصلاح: أربعة أعمدة تُثبَّت لحظة الطلب وتُقارن إلزامياً لحظة التنفيذ:
--   provider_id_at_request        — هوية الموفر المنطقية
--   provider_kind_at_request      — عائلة الموفر (تحدد مسارات الواجهة)
--   endpoint_identity_at_request  — بصمة SHA-256 للعنوان الأساسي والنوع والنقل
--   approval_binding_hash         — بصمة تجمع الوجهة + النموذج + نسخة البروفايل
--                                   + هاش المهمة + مراجعة التشغيل
--
-- **فشل مغلق:** الصفوف التاريخية (كتبها إصدار أقدم من 0024) تحمل NULL في هذه
-- الأعمدة. تنفيذ صف بلا هوية وجهة مُثبتة هو **نفس الثغرة**، لذلك يرفضه المنسق
-- بـ StaleApproval ويُطلب تشغيل جديد. لا يُقبل NULL كحالة «توافق».
--
-- القيود تمنع كتابة قيمة فارغة أو بطول مخالف لطول البصمة (64 محرفاً hex).
-- لا تُعدَّل أي هجرة تاريخية (0001–0023): هذا الملف إضافي بحت.
-- =====================================================================

ALTER TABLE agent_run_execution_bindings
  ADD COLUMN provider_id_at_request TEXT
    CHECK (provider_id_at_request IS NULL OR length(provider_id_at_request) BETWEEN 1 AND 128);

ALTER TABLE agent_run_execution_bindings
  ADD COLUMN provider_kind_at_request TEXT
    CHECK (provider_kind_at_request IS NULL OR length(provider_kind_at_request) BETWEEN 1 AND 64);

ALTER TABLE agent_run_execution_bindings
  ADD COLUMN endpoint_identity_at_request TEXT
    CHECK (endpoint_identity_at_request IS NULL OR length(endpoint_identity_at_request) = 64);

ALTER TABLE agent_run_execution_bindings
  ADD COLUMN approval_binding_hash TEXT
    CHECK (approval_binding_hash IS NULL OR length(approval_binding_hash) = 64);

-- ملاحظة: مشغّلا prevent_agent_run_execution_binding_update/_delete (0017) يبقيان
-- فعّالين، فلا يمكن تحريف الهوية بعد كتابتها.
