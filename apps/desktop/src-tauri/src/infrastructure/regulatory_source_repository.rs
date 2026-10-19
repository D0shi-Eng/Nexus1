use rusqlite::params;

use crate::domain::{
    error::AppError,
    models::{
        DataMode, RegulatorySourceProvenance, RegulatorySourceRecord, RegulatorySourceSummary,
        RegulatorySourcesData,
    },
};

pub struct RegulatorySourceRepository;

impl RegulatorySourceRepository {
    // قراءة المصادر التنظيمية للمستأجر مع عدد الالتزامات النشطة المرتبطة بكل مصدر.
    pub fn load_regulatory_sources(
        connection: &rusqlite::Connection,
        tenant_id: &str,
        generated_at: &str,
    ) -> Result<RegulatorySourcesData, AppError> {
        let data_mode = read_data_mode(connection, tenant_id)?;
        let mut statement = connection
            .prepare(
                "SELECT rs.id, rs.authority, rs.document_title, rs.document_version, rs.source_url,
                        rs.retrieved_at, rs.official, rs.verification_state,
                        (SELECT COUNT(*) FROM obligations o
                          WHERE o.source_id = rs.id AND o.tenant_id = rs.tenant_id
                            AND o.archived_at IS NULL)
                 FROM regulatory_sources rs
                 WHERE rs.tenant_id = ?1
                 ORDER BY rs.updated_at DESC, rs.id ASC",
            )
            .map_err(|_| AppError::DataAccess)?;

        let items = statement
            .query_map(params![tenant_id], |row| {
                Ok(RegulatorySourceRecord {
                    id: row.get(0)?,
                    authority: row.get(1)?,
                    document_title: row.get(2)?,
                    document_version: row.get(3)?,
                    source_url: row.get(4)?,
                    retrieved_at: row.get(5)?,
                    official: row.get::<_, i64>(6)? != 0,
                    verification_state: row.get(7)?,
                    active_obligation_count: row.get::<_, i64>(8)? as u32,
                })
            })
            .map_err(|_| AppError::DataAccess)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| AppError::DataAccess)?;

        let summary = RegulatorySourceSummary {
            total: items.len() as u32,
            official: items.iter().filter(|item| item.official).count() as u32,
            human_verified: items
                .iter()
                .filter(|item| item.verification_state == "HUMAN_VERIFIED")
                .count() as u32,
            requires_review: items
                .iter()
                .filter(|item| item.verification_state == "REQUIRES_REVIEW")
                .count() as u32,
            linked_active_obligations: items.iter().map(|item| item.active_obligation_count).sum(),
        };
        let provenance = derive_provenance(&data_mode, &items);

        Ok(RegulatorySourcesData {
            has_data: !items.is_empty(),
            items,
            summary,
            provenance,
            data_mode,
            generated_at: generated_at.to_owned(),
        })
    }
}

fn read_data_mode(
    connection: &rusqlite::Connection,
    tenant_id: &str,
) -> Result<DataMode, AppError> {
    let raw_mode: String = connection
        .query_row(
            "SELECT data_mode FROM tenants WHERE id = ?1",
            params![tenant_id],
            |row| row.get(0),
        )
        .map_err(|_| AppError::DataAccess)?;
    match raw_mode.as_str() {
        "DEMO_SYNTHETIC" => Ok(DataMode::DemoSynthetic),
        "PRODUCTION" => Ok(DataMode::Production),
        _ => Err(AppError::DataAccess),
    }
}

// تصنيف المنشأ على مستوى البيانات: مستأجر الديمو يوسم تركيبياً دائماً —
// لا يجوز أن تظهر بيانات تركيبية بوصف رسمي مهما كانت قيم الأعمدة.
// وفي الإنتاج: كل مصدر غير رسمي في V1 هو مصدر أدخله المستخدم ولم يُتحقق منه،
// فالتسمية صريحة (`USER_PROVIDED_UNVERIFIED`) بدل «غير معروف» المبهم.
fn derive_provenance(
    data_mode: &DataMode,
    items: &[RegulatorySourceRecord],
) -> RegulatorySourceProvenance {
    match data_mode {
        DataMode::DemoSynthetic => RegulatorySourceProvenance::DemoSynthetic,
        DataMode::Production => {
            if items.is_empty() {
                RegulatorySourceProvenance::Unknown
            } else if items.iter().all(|item| item.official) {
                RegulatorySourceProvenance::Official
            } else {
                RegulatorySourceProvenance::UserProvidedUnverified
            }
        }
    }
}
