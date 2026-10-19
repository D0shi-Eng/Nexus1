use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::domain::{
    error::AppError,
    provider_registry::{
        AiCapabilitySource, AiProviderConnectionState, AiProviderKind, AiTransportKind,
        DiscoveredCapabilities, ModelRegistryRecord, ProviderInstanceRecord,
    },
};

// =====================================================================
// مستودع مثيلات الموفرين (ai_provider_registry)
// ---------------------------------------------------------------------
// كل قراءة وكل كتابة مقيّدة بالمستأجر صراحةً في جملة SQL نفسها — لا فلترة
// بعد الجلب ولا ثقة بمعرّف قادم من العميل. لا يحمل أي صف سراً: العمود الوحيد
// المتعلق بالأسرار هو علامة منطقية.
// =====================================================================

pub struct AiProviderRepository;

const PROVIDER_COLUMNS: &str =
    "id, tenant_id, provider_id, kind, display_name, base_url, transport, \
     enabled, connection_state, connection_checked_at, credential_configured, built_in, \
     created_at, updated_at";

impl AiProviderRepository {
    fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderInstanceRecord> {
        let kind_text: String = row.get(3)?;
        let transport_text: String = row.get(6)?;
        let state_text: String = row.get(8)?;
        // صف بمخطط غير متوافق فشل مغلق: لا تخمين لنوع أو حالة.
        let kind = AiProviderKind::from_database_code(&kind_text).map_err(|_| {
            rusqlite::Error::InvalidColumnType(3, "kind".to_owned(), rusqlite::types::Type::Text)
        })?;
        let transport = AiTransportKind::parse_database_code(&transport_text).map_err(|_| {
            rusqlite::Error::InvalidColumnType(
                6,
                "transport".to_owned(),
                rusqlite::types::Type::Text,
            )
        })?;
        let connection_state =
            AiProviderConnectionState::parse_code(&state_text).map_err(|_| {
                rusqlite::Error::InvalidColumnType(
                    8,
                    "connection_state".to_owned(),
                    rusqlite::types::Type::Text,
                )
            })?;

        Ok(ProviderInstanceRecord {
            id: row.get(0)?,
            tenant_id: row.get(1)?,
            provider_id: row.get(2)?,
            kind,
            display_name: row.get(4)?,
            base_url: row.get(5)?,
            transport,
            enabled: row.get::<_, i64>(7)? == 1,
            connection_state,
            connection_checked_at: row.get(9)?,
            credential_configured: row.get::<_, i64>(10)? == 1,
            built_in: row.get::<_, i64>(11)? == 1,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        })
    }

    pub fn list_for_tenant(
        connection: &Connection,
        tenant_id: &str,
    ) -> Result<Vec<ProviderInstanceRecord>, AppError> {
        let mut statement = connection
            .prepare(&format!(
                "SELECT {PROVIDER_COLUMNS} FROM ai_provider_registry WHERE tenant_id = ?1 ORDER BY built_in DESC, display_name ASC, provider_id ASC"
            ))
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id], Self::map_row)
            .map_err(|_| AppError::DataAccess)?;

        rows.map(|row| row.map_err(|_| AppError::DataAccess))
            .collect()
    }

    pub fn find_by_provider_id(
        connection: &Connection,
        tenant_id: &str,
        provider_id: &str,
    ) -> Result<Option<ProviderInstanceRecord>, AppError> {
        connection
            .query_row(
                &format!(
                    "SELECT {PROVIDER_COLUMNS} FROM ai_provider_registry WHERE tenant_id = ?1 AND provider_id = ?2"
                ),
                params![tenant_id, provider_id],
                Self::map_row,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    /// إدراج أو تحديث مثيل موفر. القرارات الملزمة هنا:
    ///   * معرّف الصف يُشتق من (المستأجر، معرّف الموفر) فلا يتغيّر بتعديل الإعداد.
    ///   * علامة السر لا تُمس من هنا إطلاقاً: لا يعرفها هذا المسار ولا يغيّرها.
    ///   * تغيير العنوان الأساسي أو النوع يُسقط الحالة إلى UNVERIFIED ويمسح
    ///     طابع الفحص: نتيجة اختبار قديم لا تنطبق على وجهة جديدة.
    pub fn upsert(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        provider_id: &str,
        kind: AiProviderKind,
        display_name: &str,
        base_url: &str,
        transport: AiTransportKind,
        enabled: bool,
        now: &str,
    ) -> Result<ProviderInstanceRecord, AppError> {
        let existing = Self::find_in_transaction(transaction, tenant_id, provider_id)?;
        let row_id = match &existing {
            Some(record) => record.id.clone(),
            None => format!("ai_provider_{tenant_id}_{provider_id}"),
        };
        let endpoint_changed = existing
            .as_ref()
            .map(|record| record.base_url != base_url || record.kind != kind)
            .unwrap_or(true);

        if let Some(record) = &existing {
            let (state, checked_at) = if endpoint_changed {
                (AiProviderConnectionState::Unverified, None)
            } else {
                (
                    record.connection_state,
                    record.connection_checked_at.clone(),
                )
            };
            transaction
                .execute(
                    "UPDATE ai_provider_registry
                     SET kind = ?3, display_name = ?4, base_url = ?5, transport = ?6, enabled = ?7,
                         connection_state = ?8, connection_checked_at = ?9, updated_at = ?10
                     WHERE tenant_id = ?1 AND provider_id = ?2",
                    params![
                        tenant_id,
                        provider_id,
                        kind.as_database_code(),
                        display_name,
                        base_url,
                        transport.as_database_code(),
                        i64::from(enabled),
                        state.as_code(),
                        checked_at,
                        now
                    ],
                )
                .map_err(|_| AppError::StorageOperation)?;
        } else {
            transaction
                .execute(
                    "INSERT INTO ai_provider_registry
                     (id, tenant_id, provider_id, kind, display_name, base_url, transport, enabled,
                      connection_state, connection_checked_at, credential_configured, built_in,
                      created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'UNVERIFIED', NULL, 0, 0, ?9, ?9)",
                    params![
                        row_id,
                        tenant_id,
                        provider_id,
                        kind.as_database_code(),
                        display_name,
                        base_url,
                        transport.as_database_code(),
                        i64::from(enabled),
                        now
                    ],
                )
                .map_err(|_| AppError::StorageOperation)?;
        }

        Self::find_in_transaction(transaction, tenant_id, provider_id)?
            .ok_or(AppError::StorageOperation)
    }

    pub fn find_in_transaction(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        provider_id: &str,
    ) -> Result<Option<ProviderInstanceRecord>, AppError> {
        transaction
            .query_row(
                &format!(
                    "SELECT {PROVIDER_COLUMNS} FROM ai_provider_registry WHERE tenant_id = ?1 AND provider_id = ?2"
                ),
                params![tenant_id, provider_id],
                Self::map_row,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    /// حذف مثيل موفر. المفتاح الأجنبي على ai_model_registry.provider_id مضبوط
    /// على RESTRICT (لا حذف تسلسلي صامت)، لذا تُحذف أولاً صفوف النماذج **غير
    /// المفعّلة** التابعة للموفر، ثم يُرفض الحذف فشلاً مغلقاً إن بقي نموذج مفعّل:
    /// لا يجوز أن يختفي موفر تحت نموذج قيد التشغيل بغير قرار صريح من المستخدم.
    pub fn delete(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        provider_id: &str,
    ) -> Result<usize, AppError> {
        let record = Self::find_in_transaction(transaction, tenant_id, provider_id)?;
        let Some(record) = record else {
            return Ok(0);
        };

        let enabled_models: i64 = transaction
            .query_row(
                "SELECT COUNT(*) FROM ai_model_registry WHERE tenant_id = ?1 AND provider_id = ?2 AND enabled = 1",
                params![tenant_id, record.id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::DataAccess)?;
        if enabled_models > 0 {
            return Err(AppError::Conflict);
        }

        transaction
            .execute(
                "DELETE FROM ai_model_registry WHERE tenant_id = ?1 AND provider_id = ?2",
                params![tenant_id, record.id],
            )
            .map_err(|_| AppError::StorageOperation)?;
        let deleted = transaction
            .execute(
                "DELETE FROM ai_provider_registry WHERE tenant_id = ?1 AND provider_id = ?2",
                params![tenant_id, provider_id],
            )
            .map_err(|_| AppError::StorageOperation)?;

        Ok(deleted)
    }

    /// علامة وجود السر — المفرد الوحيد الذي يُكتب من مسار الأسرار، ولا يحمل السر.
    pub fn set_credential_marker(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        provider_id: &str,
        configured: bool,
        connection_state: AiProviderConnectionState,
        checked_at: Option<&str>,
        now: &str,
    ) -> Result<usize, AppError> {
        let changed = transaction
            .execute(
                "UPDATE ai_provider_registry
                 SET credential_configured = ?3, connection_state = ?4, connection_checked_at = ?5, updated_at = ?6
                 WHERE tenant_id = ?1 AND provider_id = ?2",
                params![
                    tenant_id,
                    provider_id,
                    i64::from(configured),
                    connection_state.as_code(),
                    checked_at,
                    now
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(changed)
    }

    /// تحديث حالة الاتصال وطابع الفحص فقط — لا يمس العنوان ولا السر ولا التمكين.
    pub fn update_connection_state(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        provider_id: &str,
        state: AiProviderConnectionState,
        checked_at: Option<&str>,
        now: &str,
    ) -> Result<usize, AppError> {
        let changed = transaction
            .execute(
                "UPDATE ai_provider_registry
                 SET connection_state = ?3, connection_checked_at = ?4, updated_at = ?5
                 WHERE tenant_id = ?1 AND provider_id = ?2",
                params![tenant_id, provider_id, state.as_code(), checked_at, now],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(changed)
    }
}

// =====================================================================
// مستودع سجل النماذج (ai_model_registry المعمَّم في 0023)
// =====================================================================

pub struct AiModelRepository;

const MODEL_COLUMNS: &str =
    "id, tenant_id, provider_kind, model_id, display_name, enabled, provider_id, \
     context_limit, supports_tools, supports_structured_output, supports_vision, \
     capability_source, available, last_discovered_at, created_at, updated_at";

impl AiModelRepository {
    fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelRegistryRecord> {
        let kind_text: String = row.get(2)?;
        let source_text: String = row.get(11)?;
        let kind = AiProviderKind::from_database_code(&kind_text).map_err(|_| {
            rusqlite::Error::InvalidColumnType(
                2,
                "provider_kind".to_owned(),
                rusqlite::types::Type::Text,
            )
        })?;
        let capability_source = AiCapabilitySource::parse_code(&source_text).map_err(|_| {
            rusqlite::Error::InvalidColumnType(
                11,
                "capability_source".to_owned(),
                rusqlite::types::Type::Text,
            )
        })?;

        Ok(ModelRegistryRecord {
            id: row.get(0)?,
            tenant_id: row.get(1)?,
            provider_kind: kind,
            model_id: row.get(3)?,
            display_name: row.get(4)?,
            enabled: row.get::<_, i64>(5)? == 1,
            provider_id: row.get(6)?,
            capabilities: DiscoveredCapabilities {
                context_limit: row.get(7)?,
                supports_tools: row.get::<_, Option<i64>>(8)?.map(|value| value == 1),
                supports_structured_output: row.get::<_, Option<i64>>(9)?.map(|value| value == 1),
                supports_vision: row.get::<_, Option<i64>>(10)?.map(|value| value == 1),
            },
            capability_source,
            available: row.get::<_, i64>(12)? == 1,
            last_discovered_at: row.get(13)?,
            created_at: row.get(14)?,
            updated_at: row.get(15)?,
        })
    }

    pub fn list_for_tenant(
        connection: &Connection,
        tenant_id: &str,
    ) -> Result<Vec<ModelRegistryRecord>, AppError> {
        let mut statement = connection
            .prepare(&format!(
                "SELECT {MODEL_COLUMNS} FROM ai_model_registry WHERE tenant_id = ?1 ORDER BY model_id ASC"
            ))
            .map_err(|_| AppError::DataAccess)?;
        let rows = statement
            .query_map(params![tenant_id], Self::map_row)
            .map_err(|_| AppError::DataAccess)?;

        rows.map(|row| row.map_err(|_| AppError::DataAccess))
            .collect()
    }

    pub fn find_in_transaction(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> Result<Option<ModelRegistryRecord>, AppError> {
        transaction
            .query_row(
                &format!(
                    "SELECT {MODEL_COLUMNS} FROM ai_model_registry
                     WHERE tenant_id = ?1 AND provider_id = ?2 AND model_id = ?3"
                ),
                params![tenant_id, provider_id, model_id],
                Self::map_row,
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    /// إدراج أو تحديث نموذج مكتشف. القدرات تُكتب كما وصلت: `None` يعني «غير
    /// معروف» ويُخزَّن NULL — لا صفر ولا false. الحدث المكتشف يرفع المصدر إلى
    /// DISCOVERED ويحدّث طابع الاكتشاف، ولا يمس التمكين (الانتخاب قرار منفصل).
    ///
    /// `transport` نقل الموفر **المتحقق منه** لا ادعاء العميل، ومنه تُشتق إقامة
    /// البيانات: النقل الحلقي يعني أن الطلب لا يغادر الجهاز ⇒ `LOCAL`. هذا العمود
    /// هو ما تقرأه سلطة التنفيذ لتحديد «النموذج المحلي»، فلا تعتمد على انضمام جدول
    /// الموفرين (وهو ما كان يكسر المخططات الأقدم).
    pub fn upsert_discovered_model(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        provider_row_id: &str,
        provider_kind: AiProviderKind,
        transport: AiTransportKind,
        model_id: &str,
        display_name: &str,
        capabilities: &DiscoveredCapabilities,
        now: &str,
    ) -> Result<(), AppError> {
        let row_id = format!("ai_model_{provider_row_id}_{model_id}");
        transaction
            .execute(
                "INSERT INTO ai_model_registry
                 (id, tenant_id, provider_kind, model_id, display_name, enabled, data_residency,
                  allowed_classification_ceiling, requires_human_approval, created_at, updated_at,
                  provider_id, context_limit, supports_tools, supports_structured_output,
                  supports_vision, capability_source, available, last_discovered_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, 'INTERNAL', 1, ?7, ?7,
                         ?8, ?9, ?10, ?11, ?12, 'DISCOVERED', 1, ?7)
                 ON CONFLICT (tenant_id, provider_kind, model_id) DO UPDATE SET
                   display_name = excluded.display_name,
                   provider_id = excluded.provider_id,
                   -- الإقامة تُعاد كتابتها أيضاً: صف قديم لموفر محلي كان
                   -- 'UNSPECIFIED' يصبح 'LOCAL' بعد إعادة الاكتشاف (تصحيح ذاتي).
                   data_residency = excluded.data_residency,
                   context_limit = excluded.context_limit,
                   supports_tools = excluded.supports_tools,
                   supports_structured_output = excluded.supports_structured_output,
                   supports_vision = excluded.supports_vision,
                   capability_source = 'DISCOVERED',
                   available = 1,
                   last_discovered_at = excluded.last_discovered_at,
                   updated_at = excluded.updated_at",
                params![
                    row_id,
                    tenant_id,
                    provider_kind.as_database_code(),
                    model_id,
                    display_name,
                    if transport == AiTransportKind::HttpLoopback {
                        "LOCAL"
                    } else {
                        "UNSPECIFIED"
                    },
                    now,
                    provider_row_id,
                    capabilities.context_limit,
                    capabilities.supports_tools.map(i64::from),
                    capabilities.supports_structured_output.map(i64::from),
                    capabilities.supports_vision.map(i64::from),
                ],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(())
    }

    /// إسقاط توفر كل نماذج الموفر قبل إعادة الاكتشاف: نموذج اختفى من المزود
    /// لم يعد متاحاً، لكن صفّه يبقى (تاريخ) ولا يُمحى.
    pub fn mark_provider_models_unavailable(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        provider_row_id: &str,
        now: &str,
    ) -> Result<usize, AppError> {
        let changed = transaction
            .execute(
                "UPDATE ai_model_registry SET available = 0, updated_at = ?3
                 WHERE tenant_id = ?1 AND provider_id = ?2 AND available = 1",
                params![tenant_id, provider_row_id, now],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(changed)
    }

    /// الانتخاب الأحادي: (موفر واحد + نموذج واحد) لكل مستأجر. الفهرس الفريد
    /// الجزئي idx_ai_models_single_selection يفرض ذلك على مستوى المخطط أيضاً.
    pub fn select_single(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        provider_row_id: &str,
        model_id: &str,
        now: &str,
    ) -> Result<bool, AppError> {
        transaction
            .execute(
                "UPDATE ai_model_registry SET enabled = 0, updated_at = ?2 WHERE tenant_id = ?1 AND enabled = 1",
                params![tenant_id, now],
            )
            .map_err(|_| AppError::StorageOperation)?;
        let changed = transaction
            .execute(
                "UPDATE ai_model_registry SET enabled = 1, updated_at = ?4
                 WHERE tenant_id = ?1 AND provider_id = ?2 AND model_id = ?3 AND available = 1",
                params![tenant_id, provider_row_id, model_id, now],
            )
            .map_err(|_| AppError::StorageOperation)?;
        // لا نموذج مختار ⇒ فشل مغلق: نُعيد false ولا نترك انتخابات نصفية.
        Ok(changed == 1)
    }

    pub fn clear_selection(
        transaction: &Transaction<'_>,
        tenant_id: &str,
        now: &str,
    ) -> Result<usize, AppError> {
        let changed = transaction
            .execute(
                "UPDATE ai_model_registry SET enabled = 0, updated_at = ?2 WHERE tenant_id = ?1 AND enabled = 1",
                params![tenant_id, now],
            )
            .map_err(|_| AppError::StorageOperation)?;
        Ok(changed)
    }

    /// الانتخاب الحالي — (معرّف صف الموفر، معرّف النموذج) أو لا شيء.
    pub fn selected_pair(
        connection: &Connection,
        tenant_id: &str,
    ) -> Result<Option<(String, String)>, AppError> {
        connection
            .query_row(
                "SELECT provider_id, model_id FROM ai_model_registry
                 WHERE tenant_id = ?1 AND enabled = 1 AND provider_id IS NOT NULL
                 ORDER BY model_id ASC LIMIT 1",
                params![tenant_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }

    pub fn count_for_provider(
        connection: &Connection,
        tenant_id: &str,
        provider_row_id: &str,
    ) -> Result<i64, AppError> {
        connection
            .query_row(
                "SELECT COUNT(*) FROM ai_model_registry WHERE tenant_id = ?1 AND provider_id = ?2",
                params![tenant_id, provider_row_id],
                |row| row.get(0),
            )
            .map_err(|_| AppError::DataAccess)
    }
}
