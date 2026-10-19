use rusqlite::{params, Connection, Transaction};

use crate::domain::{error::AppError, provider::ModelId};

// =====================================================================
// تعريف «النموذج المحلي» لسلطة التنفيذ
// ---------------------------------------------------------------------
// العيبة المُثبتة (كانت HIGH — مانعة للتنفيذ):
//   كانت كل استعلامات هذه الوحدة تشترط `provider_kind = 'LOCAL'` حرفياً. وهذه
//   القيمة لا يكتبها إلا مسار Ollama المدمج (`ensure_local_model_row`). أما أي
//   خادم استنتاج محلي آخر يُسجَّل عبر سجل الموفرين — llama.cpp/Bonsai على
//   127.0.0.1:8080 أو LM Studio على 127.0.0.1:1234 من نوع OPENAI_COMPATIBLE أو
//   LM_STUDIO — فتُكتب صفوف نماذجه بقيمة النوع الحقيقي، فلا يراها المنسق أبداً.
//   الأثر: يمكن إعداد الموفر واستكشافه وانتخاب نموذجه من الواجهة، ثم يفشل كل
//   تنفيذ محكوم بـ `ModelUnavailable` بلا سبب مفهوم. أي أن Bonsai 2 وLM Studio
//   غير قابلين للتنفيذ إطلاقاً رغم أنهما لا يغادران الجهاز.
//
// الإصلاح: التعريف **بالسلوك لا بالاسم**. الصف محلي ما دام نموذجه المُنتخب ينتمي
// إلى إقامة `LOCAL` (تُشتق خلفياً عند الاكتشاف من نقل الموفر المتحقق منه: حلقي ⇒
// لا يغادر الجهاز)، أو كان صفاً تاريخياً بقيمة 'LOCAL' في `provider_kind`.
// المزودون السحابيون (HTTPS) يبقون مستبعدين من سلطة التنفيذ تماماً كما كانوا —
// تغيير سياسة التنفيذ السحابي قرار مالك لا يُتخذ هنا.
// =====================================================================

/// الصف مؤهل كـ«نموذج محلي»: مصنَّف LOCAL تاريخياً، أو **إقامته LOCAL**.
///
/// لماذا `data_residency` لا انضمام إلى `ai_provider_registry`:
/// الإقامة تُشتق خلفياً عند الاكتشاف من **نقل الموفر المتحقق منه** (HTTP حلقي ⇒
/// لا يغادر الجهاز)، فهي تحمل نفس المعلومة بلا انضمام. والانضمام كان يجعل هذه
/// الوحدة تشترط وجود جدول 0023 ووجود عمود `provider_id`، فتنكسر على أي مخطط
/// أقدم — وهو ما حدث فعلاً في اختبارات تستعمل هجرة 0001+0005 فقط.
/// `{alias}` هو اسم الجدول المستعار لـ`ai_model_registry` في الاستعلام.
fn local_execution_row(alias: &str) -> String {
    format!("({alias}.provider_kind = 'LOCAL' OR {alias}.data_residency = 'LOCAL')")
}

pub struct ModelRegistryRepository;

impl ModelRegistryRepository {
    pub fn list_enabled_local_models(
        connection: &Connection,
        tenant_id: &str,
    ) -> Result<Vec<ModelId>, AppError> {
        let predicate = local_execution_row("m");
        let mut statement = connection
            .prepare(&format!(
                "SELECT m.model_id
                 FROM ai_model_registry m
                 WHERE m.tenant_id = ?1
                   AND m.enabled = 1
                   AND {predicate}
                 ORDER BY m.model_id ASC"
            ))
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id], |row| row.get::<_, String>(0))
            .map_err(|_| AppError::DataAccess)?;

        rows.map(|row| {
            let value = row.map_err(|_| AppError::DataAccess)?;
            ModelId::new(value).map_err(AppError::from)
        })
        .collect()
    }

    // إدراج صف النموذج المحلي المكتشف (تجاهلي) — بيانات الاستضافة ثابتة ومحافظة:
    // إقامة LOCAL وسقف تصنيف INTERNAL واعتماد بشري مطلوب.
    pub fn ensure_local_model_row(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        model_id: &str,
        now: &str,
    ) -> Result<(), AppError> {
        transaction
            .execute(
                "INSERT OR IGNORE INTO ai_model_registry (id, tenant_id, provider_kind, model_id, display_name, enabled, data_residency, allowed_classification_ceiling, requires_human_approval, created_at, updated_at) VALUES (?1, ?2, 'LOCAL', ?3, ?3, 0, 'LOCAL', 'INTERNAL', 1, ?4, ?4)",
                params![format!("ai_model_local_{model_id}"), tenant_id, model_id, now],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    // التبديل الأحادي: إلغاء تفعيل كل النماذج المحلية المؤهلة للتنفيذ ثم تفعيل
    // المطلوب — لا يمكن أن يوجد أكثر من نموذج تنفيذي واحد (عقد التشغيل).
    // النطاق صار نفس تعريف `local_execution_row`، وإلا بقي صف حلقي مفعّل خارج
    // النطاق فيتعارض «نموذجان مفعّلان» ويُرفض التنفيذ بلا مخرج من الواجهة.
    pub fn enable_single_local_model(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        model_id: &str,
        now: &str,
    ) -> Result<bool, AppError> {
        let disable_predicate = local_execution_row("ai_model_registry");
        transaction
            .execute(
                &format!(
                    "UPDATE ai_model_registry SET enabled = 0, updated_at = ?2
                     WHERE tenant_id = ?1 AND enabled = 1 AND {disable_predicate}"
                ),
                params![tenant_id, now],
            )
            .map_err(|_| AppError::StorageOperation)?;

        let enable_predicate = local_execution_row("ai_model_registry");
        let changed = transaction
            .execute(
                &format!(
                    "UPDATE ai_model_registry SET enabled = 1, updated_at = ?3
                     WHERE tenant_id = ?1 AND model_id = ?2 AND {enable_predicate}"
                ),
                params![tenant_id, model_id, now],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(changed == 1)
    }

    pub fn disable_all_local_models(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        now: &str,
    ) -> Result<usize, AppError> {
        let predicate = local_execution_row("ai_model_registry");
        let changed = transaction
            .execute(
                &format!(
                    "UPDATE ai_model_registry SET enabled = 0, updated_at = ?2
                     WHERE tenant_id = ?1 AND enabled = 1 AND {predicate}"
                ),
                params![tenant_id, now],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(changed)
    }
}
