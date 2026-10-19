use chrono::Utc;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{
    application::{audit_anchor, authorization_service},
    domain::{
        error::AppError,
        models::{AuditTrailCursor, AuditTrailData, AuditTrailItem},
    },
    infrastructure::database::AppState,
};

// حد الصفحة لقارئ التدقيق — استعلام محدود دائماً لأن جدول التدقيق ينمو بلا سقف منتجي.
const AUDIT_TRAIL_PAGE_SIZE: usize = 50;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditTrailPageInput {
    pub before: Option<AuditTrailCursor>,
}

// تجزئة الحدث في موضع إدراجي واحد (1-based) — تُستخدم لمقارنة البادئة المؤرشفة
// مع الحالة الفعلية دون إعادة قراءة السلسلة كاملة.
fn anchor_hash_at<'a>(
    connection: &'a rusqlite::Connection,
    tenant_id: &'a str,
) -> impl Fn(u32) -> Result<Option<String>, AppError> + 'a {
    move |position: u32| {
        if position == 0 {
            return Ok(None);
        }
        connection
            .query_row(
                "SELECT event_hash FROM audit_events WHERE tenant_id = ?1 ORDER BY rowid ASC LIMIT 1 OFFSET ?2",
                params![tenant_id, i64::from(position) - 1],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| AppError::DataAccess)
    }
}

fn last_event_hash(
    connection: &rusqlite::Connection,
    tenant_id: &str,
) -> Result<Option<String>, AppError> {
    connection
        .query_row(
            "SELECT event_hash FROM audit_events WHERE tenant_id = ?1 ORDER BY rowid DESC LIMIT 1",
            [tenant_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|_| AppError::DataAccess)
}

fn evaluate_anchor(
    state: &AppState,
    connection: &rusqlite::Connection,
    events: u32,
) -> Result<audit_anchor::AnchorEvaluation, AppError> {
    let tenant_id = state.tenant_id();
    let path = audit_anchor::anchor_path(state);
    audit_anchor::evaluate(path.as_deref(), &tenant_id, events, |position| {
        anchor_hash_at(connection, &tenant_id)(position)
    })
}

// التحقق الكامل = سلامة السلسلة الداخلية **و**عدم تناقضها مع مرساة الذيل.
// المرساة لا تُبطل سلامة سلسلة سليمة عند غيابها؛ تُبطلها فقط عند اقتطاع
// أو إعادة كتابة بادئة مؤرشفة.
pub fn verify_audit_chain(state: &AppState) -> Result<bool, AppError> {
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let report = verify_chain_report(&connection, &state.tenant_id())?;
    let evaluation = evaluate_anchor(state, &connection, report.events)?;
    Ok(report.valid && !evaluation.state.contradicts_chain())
}

// قارئ أحداث التدقيق للوحدة 12 — قراءة فقط، مأذونة، محصورة بالمستأجر،
// مرتبة ترتيباً حتمياً (occurred_at تنازلياً ثم id تنازلياً) بترقيم مفتاحي.
pub fn list_audit_events(
    state: &AppState,
    input: AuditTrailPageInput,
) -> Result<AuditTrailData, AppError> {
    authorization_service::require_permission(state, "AUDIT", "READ")?;

    let fetch_limit = (AUDIT_TRAIL_PAGE_SIZE + 1) as i64;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let map_item = |row: &rusqlite::Row<'_>| -> rusqlite::Result<AuditTrailItem> {
        Ok(AuditTrailItem {
            id: row.get(0)?,
            event_type: row.get(1)?,
            actor_name: row.get(2)?,
            resource_type: row.get(3)?,
            resource_id: row.get(4)?,
            occurred_at: row.get(5)?,
        })
    };

    let mut items: Vec<AuditTrailItem> = match input.before {
        None => {
            let mut statement = connection
                .prepare(
                    "SELECT id, event_type, actor_name, resource_type, resource_id, occurred_at
                     FROM audit_events
                     WHERE tenant_id = ?1
                     ORDER BY occurred_at DESC, id DESC
                     LIMIT ?2",
                )
                .map_err(|_| AppError::DataAccess)?;
            let rows = statement
                .query_map(params![&state.tenant_id(), fetch_limit], map_item)
                .map_err(|_| AppError::DataAccess)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| AppError::DataAccess)?
        }
        Some(before) => {
            // ترقيم مفتاحي: كل ما سبق المفتاح (occurred_at, id) في الترتيب التنازلي الحتمي.
            let mut statement = connection
                .prepare(
                    "SELECT id, event_type, actor_name, resource_type, resource_id, occurred_at
                     FROM audit_events
                     WHERE tenant_id = ?1
                       AND (occurred_at < ?2 OR (occurred_at = ?2 AND id < ?3))
                     ORDER BY occurred_at DESC, id DESC
                     LIMIT ?4",
                )
                .map_err(|_| AppError::DataAccess)?;
            let rows = statement
                .query_map(
                    params![
                        &state.tenant_id(),
                        &before.occurred_at,
                        &before.id,
                        fetch_limit
                    ],
                    map_item,
                )
                .map_err(|_| AppError::DataAccess)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| AppError::DataAccess)?
        }
    };

    let has_more = items.len() > AUDIT_TRAIL_PAGE_SIZE;
    if has_more {
        items.truncate(AUDIT_TRAIL_PAGE_SIZE);
    }
    let next_cursor = if has_more {
        items.last().map(|last| AuditTrailCursor {
            occurred_at: last.occurred_at.clone(),
            id: last.id.clone(),
        })
    } else {
        None
    };

    // سلامة السلسلة فعلية من إعادة تشغيل التجزئة لا قيمة ثابتة، والمرساة
    // تُقارن أيضاً كي لا يُعرض «سليم» بعد اقتطاع ذيل السلسلة.
    let (chain_integrity, chain_anchor_state) =
        match verify_chain_report(&connection, &state.tenant_id()) {
            Ok(report) => {
                let evaluation = evaluate_anchor(state, &connection, report.events)?;
                let contradicts = evaluation.state.contradicts_chain();
                let integrity = if report.valid && !contradicts {
                    "VALID"
                } else {
                    "INVALID"
                };
                (integrity.to_owned(), evaluation.state.as_str().to_owned())
            }
            Err(_) => ("UNKNOWN".to_owned(), "NO_REFERENCE".to_owned()),
        };

    Ok(AuditTrailData {
        generated_at: Utc::now().to_rfc3339(),
        items,
        chain_integrity,
        chain_anchor_state,
        has_more,
        next_cursor,
    })
}

// نتيجة التحقق التفصيلية: لا قيمة إجمالية فقط، بل معرّفات الأحداث التي فشلت
// فعلاً. السبب: لوحة القيادة تعرض حالة لكل حدث، فلا يجوز أن تُختم كل الأحداث
// بـ«سليم» بناءً على قيمة إجمالية محسوبة مرة واحدة لغيرها.
#[derive(Debug, Clone)]
pub(crate) struct AuditChainReport {
    pub valid: bool,
    pub invalid_event_ids: Vec<String>,
    pub events: u32,
}

// تحقق صريح قابل للاستدعاء من الواجهة: يعيد تشغيل السلسلة كاملة ويُعيد
// عدد الأحداث وعدد الروابط المكسورة — لا علامة «VALID» ثابتة.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditChainVerificationResult {
    pub valid: bool,
    pub events: u32,
    pub broken_links: u32,
    pub checked_at: String,
    /// حالة مرساة الذيل: MATCH / NO_REFERENCE / TRUNCATED / PREFIX_MISMATCH.
    pub anchor_state: String,
    /// عدد الأحداث التي غطتها المرساة وقت تثبيتها.
    pub anchored_events: u32,
}

pub fn verify_audit_chain_report(
    state: &AppState,
) -> Result<AuditChainVerificationResult, AppError> {
    authorization_service::require_permission(state, "AUDIT", "READ")?;
    let connection = state.connection.lock().map_err(|_| AppError::DataAccess)?;
    let report = verify_chain_report(&connection, &state.tenant_id())?;
    let evaluation = evaluate_anchor(state, &connection, report.events)?;
    let valid = report.valid && !evaluation.state.contradicts_chain();
    // تثبيت المرساة عند التحقق الناجح فقط: تثبيتها على سلسلة متناقضة يجعل
    // التلاعب مرجعاً معتمداً في المرة التالية.
    if valid {
        if let (Some(path), Some(head)) = (
            audit_anchor::anchor_path(state),
            last_event_hash(&connection, &state.tenant_id())?,
        ) {
            if audit_anchor::persist(&path, &state.tenant_id(), report.events, &head).is_err() {
                crate::infrastructure::local_logger::warn("AUDIT_ANCHOR_WRITE_FAILED", "", "");
            }
        }
    }
    Ok(AuditChainVerificationResult {
        valid,
        events: report.events,
        broken_links: report.invalid_event_ids.len() as u32,
        checked_at: Utc::now().to_rfc3339(),
        anchor_state: evaluation.state.as_str().to_owned(),
        anchored_events: evaluation.anchored_events,
    })
}

// تُقرأ السلسلة بترتيب الإدراج الفعلي (rowid) لتطابق ترتيب الربط عند الكتابة:
// الاعتماد على (occurred_at, id) يقلب ترتيب الأحداث المتقاسمة طابعاً زمنياً واحداً.
pub(crate) fn verify_chain_report(
    connection: &rusqlite::Connection,
    tenant_id: &str,
) -> Result<AuditChainReport, AppError> {
    let mut statement = connection
        .prepare(
            "SELECT id, event_type, actor_name, resource_type, resource_id, occurred_at, previous_hash, event_hash
             FROM audit_events
             WHERE tenant_id = ?1
             ORDER BY rowid ASC",
        )
        .map_err(|_| AppError::DataAccess)?;
    let rows = statement
        .query_map(params![tenant_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(|_| AppError::DataAccess)?;

    let mut expected_previous_hash = String::new();
    let mut invalid_event_ids = Vec::new();
    let mut events = 0_u32;
    for row in rows {
        let (
            id,
            event_type,
            actor_name,
            resource_type,
            resource_id,
            occurred_at,
            previous_hash,
            event_hash,
        ) = row.map_err(|_| AppError::DataAccess)?;
        events += 1;
        let stored_previous_hash = previous_hash.unwrap_or_default();
        let linked = stored_previous_hash == expected_previous_hash;
        let payload = format!(
            "{stored_previous_hash}|{id}|{event_type}|{actor_name}|{resource_type}|{resource_id}|{occurred_at}"
        );
        let calculated_hash = format!("{:x}", Sha256::digest(payload.as_bytes()));
        let intact = calculated_hash == event_hash;
        if !linked || !intact {
            invalid_event_ids.push(id);
        }
        // المتابعة من التجزئة المخزَّنة: حدث واحد معدَّل لا يجعل كل ما بعده
        // «تالفاً» زوراً — كل حدث يُحاسب على سلامته وسلامة رابطه وحدهما.
        expected_previous_hash = event_hash;
    }

    Ok(AuditChainReport {
        valid: invalid_event_ids.is_empty(),
        invalid_event_ids,
        events,
    })
}

#[cfg(test)]
mod tests {

    use chrono::Utc;
    use rusqlite::{params, Connection};
    use sha2::{Digest, Sha256};

    use super::{list_audit_events, verify_audit_chain, AuditTrailPageInput};
    use crate::infrastructure::database::{append_audit_event, latest_audit_event_hash, AppState};

    // حدثان بنفس الطابع الزمني ومعرّفان بترتيب معاكس لترتيب الإدراج: يجب أن تبقى
    // السلسلة صالحة لأن الربط الفعلي يتبع الإدراج، لا ترتيب المعرّف النصي.
    #[test]
    fn chain_stays_valid_when_tied_timestamps_do_not_match_id_order() {
        let state = state_with_chained_events();
        let occurred_at = "2026-01-05T00:00:00Z";
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            let transaction = connection
                .unchecked_transaction()
                .expect("معاملة الاختبار يجب أن تبدأ");
            let previous = latest_audit_event_hash(&transaction, "tenant-audit")
                .expect("قراءة آخر تجزئة يجب أن تنجح");
            let first = append_audit_event(
                &transaction,
                "tenant-audit",
                "audit_zzz_first",
                "TIED_FIRST",
                "System",
                "Tenant",
                "tenant-audit",
                occurred_at,
                &previous,
            )
            .expect("الحدث الأول يجب أن يُضاف");
            let previous_second = latest_audit_event_hash(&transaction, "tenant-audit")
                .expect("قراءة آخر تجزئة يجب أن تنجح");
            assert_eq!(
                previous_second, first,
                "أحدث حدث يجب أن يكون المُدرج أخيراً لا الأكبر معرّفاً"
            );
            append_audit_event(
                &transaction,
                "tenant-audit",
                "audit_aaa_second",
                "TIED_SECOND",
                "System",
                "Tenant",
                "tenant-audit",
                occurred_at,
                &previous_second,
            )
            .expect("الحدث الثاني يجب أن يُضاف");
            transaction.commit().expect("تثبيت الاختبار يجب أن ينجح");
        }

        assert!(verify_audit_chain(&state).expect("التحقق يجب أن ينجح"));
        let data =
            list_audit_events(&state, AuditTrailPageInput::default()).expect("القراءة يجب أن تنجح");
        assert_eq!(data.chain_integrity, "VALID");
    }

    fn append_test_event(
        connection: &Connection,
        tenant_id: &str,
        id: &str,
        event_type: &str,
        occurred_at: &str,
        previous_hash: &str,
    ) -> String {
        let payload =
            format!("{previous_hash}|{id}|{event_type}|System|Tenant|{tenant_id}|{occurred_at}");
        let event_hash = format!("{:x}", Sha256::digest(payload.as_bytes()));
        connection
            .execute(
                "INSERT INTO audit_events (id, tenant_id, event_type, actor_name, resource_type, resource_id, occurred_at, previous_hash, event_hash) VALUES (?1, ?2, ?3, 'System', 'Tenant', ?2, ?4, ?5, ?6)",
                params![id, tenant_id, event_type, occurred_at, previous_hash, &event_hash],
            )
            .expect("حدث الاختبار يجب أن ينشأ");
        event_hash
    }

    fn authorize(connection: &Connection, tenant_id: &str, user_id: &str, now: &str) {
        connection
            .execute(
                "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, NULL, 'ACTIVE', ?5, ?5)",
                params![user_id, tenant_id, user_id, format!("{user_id}@test.local"), now],
            )
            .expect("المستخدم يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES ('perm-audit-read', 'audit:read', 'AUDIT', 'READ', 'HIGHLY_RESTRICTED', ?1)",
                [now],
            )
            .expect("الصلاحية يجب أن تنشأ");
        connection
            .execute(
                "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES (?1, ?2, 'READER', 'قارئ', ?3, ?3)",
                params![format!("role-{user_id}"), tenant_id, now],
            )
            .expect("الدور يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO role_permissions (role_id, permission_id) VALUES (?1, 'perm-audit-read')",
                params![format!("role-{user_id}")],
            )
            .expect("المنح يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, ?2, ?3, 'system')",
                params![user_id, format!("role-{user_id}"), now],
            )
            .expect("الإسناد يجب أن ينشأ");
        connection
            .execute(
                "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, ?2, ?3, ?4, '2030-01-01T00:00:00Z', NULL, 'test')",
                params![format!("session-{user_id}"), tenant_id, user_id, now],
            )
            .expect("الجلسة يجب أن تنشأ");
    }

    fn state_with_chained_events() -> AppState {
        let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
        connection
            .execute_batch(include_str!("../../migrations/0001_initial.sql"))
            .expect("هجرة الأساس يجب أن تنجح");
        connection
            .execute_batch(include_str!(
                "../../migrations/0002_identity_and_access.sql"
            ))
            .expect("هجرة الهوية يجب أن تنجح");
        let now = Utc::now().to_rfc3339();

        for (tenant_id, data_mode) in [
            ("tenant-audit", "DEMO_SYNTHETIC"),
            ("tenant-other", "DEMO_SYNTHETIC"),
        ] {
            connection
                .execute(
                    "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?1, ?2, ?3, ?3)",
                    params![tenant_id, data_mode, &now],
                )
                .expect("المستأجر يجب أن ينشأ");
        }
        authorize(&connection, "tenant-audit", "audit-user", &now);

        // سلسلة من 3 أحداث متتابعة التجزئة داخل tenant-audit + حدث في مستأجر آخر.
        let first = append_test_event(
            &connection,
            "tenant-audit",
            "event-1",
            "CREATED",
            "2026-01-01T00:00:00Z",
            "",
        );
        let second = append_test_event(
            &connection,
            "tenant-audit",
            "event-2",
            "UPDATED",
            "2026-01-02T00:00:00Z",
            &first,
        );
        append_test_event(
            &connection,
            "tenant-audit",
            "event-3",
            "ARCHIVED",
            "2026-01-03T00:00:00Z",
            &second,
        );
        append_test_event(
            &connection,
            "tenant-other",
            "event-other",
            "CREATED",
            "2026-01-04T00:00:00Z",
            "",
        );

        AppState::test_instance(
            connection,
            "tenant-audit".to_owned(),
            "audit-user".to_owned(),
        )
    }

    #[test]
    fn lists_events_newest_first_with_real_chain_integrity_and_tenant_scope() {
        let state = state_with_chained_events();
        let data =
            list_audit_events(&state, AuditTrailPageInput::default()).expect("القراءة يجب أن تنجح");

        assert_eq!(data.chain_integrity, "VALID");
        assert!(!data.has_more);
        assert!(data.next_cursor.is_none());
        let ids: Vec<&str> = data.items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, vec!["event-3", "event-2", "event-1"]);
        assert!(!ids.contains(&"event-other"));
    }

    #[test]
    fn reports_invalid_chain_integrity_after_tampering() {
        let state = state_with_chained_events();
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute(
                    "UPDATE audit_events SET event_type = 'TAMPERED' WHERE id = 'event-2'",
                    [],
                )
                .expect("تعديل الاختبار يجب أن ينجح");
        }
        let data =
            list_audit_events(&state, AuditTrailPageInput::default()).expect("القراءة يجب أن تنجح");
        assert_eq!(data.chain_integrity, "INVALID");
        assert!(verify_audit_chain(&state).is_ok_and(|valid| !valid));
    }

    #[test]
    fn paginates_with_a_keyset_cursor_without_overlap_or_loss() {
        let state = state_with_chained_events();
        // نزرع 55 حدثاً حتمي الطوابع لتجاوز حد الصفحة (50) واختبار الترقيم المفتاحي فعلياً.
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            let mut previous_hash = String::new();
            for index in 0..55 {
                let id = format!("event-bulk-{index:03}");
                let occurred_at = format!("2026-02-01T00:00:{index:02}Z");
                previous_hash = append_test_event(
                    &connection,
                    "tenant-audit",
                    &id,
                    "BULK",
                    &occurred_at,
                    &previous_hash,
                );
            }
        }

        let first = list_audit_events(&state, AuditTrailPageInput { before: None })
            .expect("الصفحة الأولى يجب أن تنجح");
        assert_eq!(first.items.len(), 50);
        assert!(first.has_more);
        let cursor = first
            .next_cursor
            .clone()
            .expect("مؤشر الصفحة التالية يجب أن يتوفر");

        let second = list_audit_events(
            &state,
            AuditTrailPageInput {
                before: Some(cursor),
            },
        )
        .expect("الصفحة الثانية يجب أن تنجح");
        assert_eq!(second.items.len(), 8);
        assert!(!second.has_more);
        assert!(second.next_cursor.is_none());

        // لا تداخل ولا فقد: المجموع = كل أحداث المستأجر، والترتيب داخل كل صفحة حتمي تنازلياً.
        let mut all_ids: Vec<String> = first
            .items
            .iter()
            .chain(second.items.iter())
            .map(|item| item.id.clone())
            .collect();
        let total_in_tenant: i64 = {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .query_row(
                    "SELECT COUNT(*) FROM audit_events WHERE tenant_id = 'tenant-audit'",
                    [],
                    |row| row.get(0),
                )
                .expect("العدد الكلي يجب أن يقرأ")
        };
        assert_eq!(all_ids.len() as i64, total_in_tenant);
        let mut sorted_desc = all_ids.clone();
        sorted_desc.sort_by(|left, right| right.cmp(left));
        assert_eq!(all_ids, sorted_desc);
        all_ids.dedup();
        assert_eq!(all_ids.len() as i64, total_in_tenant);
    }

    #[test]
    fn denies_read_without_audit_permission() {
        let state = state_with_chained_events();
        {
            let connection = state.connection.lock().expect("القفل يجب أن يتاح");
            connection
                .execute("DELETE FROM role_permissions", [])
                .expect("حذف المنح يجب أن ينجح");
        }
        assert!(list_audit_events(&state, AuditTrailPageInput::default()).is_err());
    }

    #[test]
    fn audit_chain_detects_tampered_event_payloads() {
        let state = state_with_chained_events();
        assert!(verify_audit_chain(&state).expect("التحقق يجب أن ينجح"));

        state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .execute(
                "UPDATE audit_events SET event_type = 'TAMPERED' WHERE id = 'event-3'",
                [],
            )
            .expect("تعديل الاختبار يجب أن ينجح");

        assert!(!verify_audit_chain(&state).expect("التحقق يجب أن ينجح"));
    }

    // =================================================================
    // مرساة الذيل (جولة الإغلاق النهائية)
    // -----------------------------------------------------------------
    // سلسلة SHA-256 وحدها لا تكشف حذف آخر الأحداث؛ المرساة المحفوظة عند
    // آخر تحقق ناجح هي التي تكشفه. هذه الاختبارات تُثبت الكشف فعلياً.
    // =================================================================
    fn state_with_anchor_file(label: &str) -> (AppState, std::path::PathBuf) {
        let mut state = state_with_chained_events();
        let directory =
            std::env::temp_dir().join(format!("nexus-anchor-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).expect("مجلد المرساة يجب أن ينشأ");
        state.set_database_path_for_tests(directory.join("nexus-local.db"));
        (state, directory)
    }

    #[test]
    fn tail_truncation_is_detected_by_the_anchor_even_though_links_stay_intact() {
        let (state, directory) = state_with_anchor_file("truncate");
        let first = super::verify_audit_chain_report(&state).expect("أول تحقق");
        assert!(first.valid);
        assert_eq!(
            first.anchor_state, "NO_REFERENCE",
            "أول تحقق لا يملك مرجعاً سابقاً"
        );

        let second = super::verify_audit_chain_report(&state).expect("ثاني تحقق");
        assert!(second.valid);
        assert_eq!(second.anchor_state, "MATCH");
        assert_eq!(second.anchored_events, 3);

        // حذف آخر حدث: كل الروابط الداخلية تبقى سليمة لأن لا شيء بعد الذيل.
        state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .execute("DELETE FROM audit_events WHERE id = 'event-3'", [])
            .expect("حذف الذيل يجب أن ينجح");

        let truncated = super::verify_audit_chain_report(&state).expect("التحقق بعد الاقتطاع");
        assert!(!truncated.valid, "الاقتطاع يجب أن يُبطل التحقق");
        assert_eq!(truncated.anchor_state, "TRUNCATED");
        assert_eq!(
            truncated.broken_links, 0,
            "الروابط الداخلية سليمة — المرساة وحدها هي التي كشفت الحذف"
        );

        // القارئ اليومي أيضاً لا يعرض VALID بعد الاقتطاع.
        let trail = list_audit_events(&state, AuditTrailPageInput::default())
            .expect("قراءة المسار يجب أن تنجح");
        assert_eq!(trail.chain_integrity, "INVALID");
        assert_eq!(trail.chain_anchor_state, "TRUNCATED");

        // المرساة لا تُثبَّت فوق سلسلة مشكوك فيها.
        let again = super::verify_audit_chain_report(&state).expect("تحقق ثالث");
        assert_eq!(again.anchor_state, "TRUNCATED");
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn removing_an_event_from_the_middle_of_the_chain_is_detected() {
        let state = state_with_chained_events();
        let intact = super::verify_audit_chain_report(&state).expect("التحقق قبل الحذف");
        assert!(intact.valid);

        state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .execute("DELETE FROM audit_events WHERE id = 'event-2'", [])
            .expect("حذف الحدث الأوسط يجب أن ينجح");

        let report = super::verify_audit_chain_report(&state).expect("التحقق بعد الحذف");
        assert!(!report.valid);
        assert!(report.broken_links >= 1, "الرابط المكسور يجب أن يُعدّ");
        assert_eq!(report.events, 2);
    }

    #[test]
    fn changing_the_actor_binding_of_a_stored_event_is_detected() {
        let state = state_with_chained_events();
        assert!(
            super::verify_audit_chain_report(&state)
                .expect("التحقق قبل التعديل")
                .valid
        );

        state
            .connection
            .lock()
            .expect("القفل يجب أن يتاح")
            .execute(
                "UPDATE audit_events SET actor_name = 'Someone Else' WHERE id = 'event-2'",
                [],
            )
            .expect("تعديل الفاعل يجب أن ينجح");

        let report = super::verify_audit_chain_report(&state).expect("التحقق بعد التعديل");
        assert!(!report.valid, "تغيير الفاعل يجب أن يكسر التجزئة");
        assert!(report.broken_links >= 1);
    }
}
