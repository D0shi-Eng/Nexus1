// =====================================================================
// اختبارات تحمّل واستقرار **محدودة** + تحقق حي من سلامة قاعدة المستخدم
// ---------------------------------------------------------------------
// كلها `#[ignore]` لأنها تعمل على مدد أو ملفات خارج الجناح السريع:
//   cargo test --lib -- --ignored --nocapture operational_stress
//
// قيود الموارد الملزمة (التزاماً بطلب المالك):
//   * لا استدعاء شبكة واحد هنا. لا نماذج. لا GPU.
//   * كل كتابة على **قواعد معزولة** (ذاكرة أو مجلد حرارة) — لا مساس بقاعدة
//     المستخدم الحية إطلاقاً.
//   * الحدود قصيرة صريحة: 300 قراءة · 6 خيوط × 20 إلحاق · 240 حدثاً مبذوراً.
//
// والاختبار الثالث يقرأ **نسخة** من قاعدة المستخدم الحية بفتح للقراءة فقط،
// فلا يمكنه تعديلها بنيوياً حتى لو أراد.
// =====================================================================

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use chrono::{Duration as ChronoDuration, Utc};
use rusqlite::{params, Connection, OpenFlags, TransactionBehavior};
use sha2::{Digest, Sha256};

use crate::{
    application::{
        audit_service::{list_audit_events, verify_audit_chain, AuditTrailPageInput},
        evidence_service, obligation_service, risk_service,
    },
    domain::models::AuditTrailCursor,
    infrastructure::database::{
        append_audit_event, initialize_demo_database, latest_audit_event_hash, AppState,
        DEMO_OPERATOR_USER_ID, DEMO_TENANT_ID,
    },
};

const SEEDED_AUDIT_EVENTS: usize = 240;
const READ_ITERATIONS: usize = 300;
const WRITER_THREADS: usize = 6;
const APPENDS_PER_THREAD: usize = 20;

fn isolated_memory_state() -> AppState {
    let connection = Connection::open_in_memory().expect("قاعدة الذاكرة يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("تفعيل المفاتيح");
    initialize_demo_database(&connection).expect("تهيئة الديمو");
    AppState::test_instance(
        connection,
        DEMO_TENANT_ID.to_owned(),
        DEMO_OPERATOR_USER_ID.to_owned(),
    )
}

fn isolated_file_directory(label: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("nexus-stress-{label}-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).expect("مجلد الاختبار يجب أن ينشأ");
    directory
}

/// تحقق **مستقل** من سلسلة التدقيق: تُعاد حساب البصمة من الأعمدة المخزَّنة
/// بترتيب الإدراج (`rowid`)، فلا يعتمد على دالة الإنتاج التي يتحقق منها.
fn verify_chain_independently(
    connection: &Connection,
    tenant_id: &str,
) -> Result<(usize, bool, usize), rusqlite::Error> {
    let mut statement = connection.prepare(
        "SELECT id, event_type, actor_name, resource_type, resource_id, occurred_at,
                previous_hash, event_hash
         FROM audit_events WHERE tenant_id = ?1 ORDER BY rowid ASC",
    )?;
    let rows = statement.query_map([tenant_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;

    let mut expected_previous = String::new();
    let mut count = 0usize;
    let mut broken_links = 0usize;
    let mut broken_hashes = 0usize;
    for row in rows {
        let (id, event_type, actor, resource_type, resource_id, occurred_at, previous, hash) = row?;
        if previous != expected_previous {
            broken_links += 1;
        }
        let payload = format!(
            "{expected_previous}|{id}|{event_type}|{actor}|{resource_type}|{resource_id}|{occurred_at}"
        );
        if format!("{:x}", Sha256::digest(payload.as_bytes())) != hash {
            broken_hashes += 1;
        }
        expected_previous = hash;
        count += 1;
    }

    Ok((
        count,
        broken_links == 0 && broken_hashes == 0,
        broken_links + broken_hashes,
    ))
}

// =====================================================================
// 1) قراءات متكررة + ترقيم مفتاحي كامل لسجل التدقيق
// =====================================================================
#[test]
#[ignore = "اختبار تحمّل محدود — يُشغَّل صراحةً"]
fn bounded_stress_repeated_reads_and_full_audit_pagination() {
    let state = isolated_memory_state();
    let tenant_id = DEMO_TENANT_ID.to_owned();

    // بذر أحداث بطوابع زمنية **متباعدة** كي يكون الترتيب التنازلي حتمياً فعلاً.
    let seeded = {
        let mut connection = state.connection.lock().expect("القفل");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("معاملة البذر");
        let base = Utc::now();
        for index in 0..SEEDED_AUDIT_EVENTS {
            let previous = latest_audit_event_hash(&transaction, &tenant_id).expect("ذيل");
            append_audit_event(
                &transaction,
                &tenant_id,
                &format!("audit_stress_{index:04}"),
                "STRESS_EVENT",
                DEMO_OPERATOR_USER_ID,
                "StressProbe",
                &format!("probe-{index}"),
                &(base + ChronoDuration::milliseconds(index as i64)).to_rfc3339(),
                &previous,
            )
            .expect("الإلحاق");
        }
        transaction.commit().expect("التزام البذر");
        SEEDED_AUDIT_EVENTS
    };

    // السلسلة سليمة بعد البذر.
    assert!(
        verify_audit_chain(&state).expect("التحقق يجب أن يُنفَّذ"),
        "السلسلة يجب أن تكون سليمة بعد البذر"
    );
    println!("[STRESS] seeded_audit_events={seeded}");

    // قراءات متكررة على مسارات القراءة الحقيقية.
    let started = Instant::now();
    for _ in 0..READ_ITERATIONS {
        obligation_service::list_obligations(&state).expect("قراءة الالتزامات");
        risk_service::load_risks(&state).expect("قراءة المخاطر");
        evidence_service::load_evidence(&state).expect("قراءة الأدلة");
    }
    let reads_ms = started.elapsed().as_millis();
    println!(
        "[STRESS] repeated_reads={} (3 مسارات/دورة) total_ms={reads_ms} avg_us={}",
        READ_ITERATIONS,
        (reads_ms * 1000) / (READ_ITERATIONS as u128 * 3)
    );

    // ترقيم مفتاحي كامل: كل حدث مرة واحدة بالضبط، والعدد الكلي مطابق.
    let mut seen: Vec<String> = Vec::new();
    let mut cursor: Option<AuditTrailCursor> = None;
    let mut pages = 0usize;
    loop {
        let page = list_audit_events(
            &state,
            AuditTrailPageInput {
                before: cursor.clone(),
            },
        )
        .expect("قراءة صفحة تدقيق");
        pages += 1;
        assert_eq!(page.chain_integrity, "VALID", "سلامة السلسلة في كل صفحة");
        assert!(
            page.items.len() <= 51,
            "حجم الصفحة يجب أن يبقى محدوداً (كان {})",
            page.items.len()
        );
        for item in &page.items {
            seen.push(item.id.clone());
        }
        match (page.has_more, page.next_cursor) {
            (true, Some(next)) => cursor = Some(next),
            _ => break,
        }
        assert!(pages < 200, "حماية من حلقة لا تنتهي");
    }

    let unique: std::collections::HashSet<&String> = seen.iter().collect();
    println!(
        "[STRESS] pagination pages={pages} rows_seen={} unique={}",
        seen.len(),
        unique.len()
    );
    assert_eq!(
        seen.len(),
        unique.len(),
        "الترقيم المفتاحي يجب ألا يكرّر صفاً ولا يُسقط صفاً"
    );

    let total: i64 = {
        let connection = state.connection.lock().expect("القفل");
        connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE tenant_id = ?1",
                params![tenant_id],
                |row| row.get(0),
            )
            .expect("عدّ الأحداث")
    };
    assert_eq!(seen.len() as i64, total, "كل الأحداث تُقرأ مرة واحدة بالضبط");

    // القراءة المتكررة لم تُفسد السلسلة.
    assert!(
        verify_audit_chain(&state).expect("التحقق النهائي"),
        "السلسلة يجب أن تبقى سليمة بعد كل القراءات"
    );
}

// =====================================================================
// 2) كتابات متزامنة معزولة على ملف: هل يبقى للتدقيق ذيل واحد؟
// =====================================================================
#[test]
#[ignore = "اختبار تحمّل محدود — يُشغَّل صراحةً"]
fn bounded_stress_concurrent_audit_appends_keep_a_single_chain() {
    let directory = isolated_file_directory("concurrent");
    let database_path = directory.join("nexus-local.db");
    {
        let connection = Connection::open(&database_path).expect("قاعدة الملف");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("المفاتيح");
        initialize_demo_database(&connection).expect("تهيئة الديمو");
    }

    let tenant_id = DEMO_TENANT_ID.to_owned();
    let path = database_path.clone();
    let started = Instant::now();
    let handles: Vec<_> = (0..WRITER_THREADS)
        .map(|thread_index| {
            let path = path.clone();
            let tenant_id = tenant_id.clone();
            std::thread::spawn(move || {
                let mut connection = Connection::open(&path).expect("اتصال الخيط");
                connection
                    .busy_timeout(std::time::Duration::from_secs(10))
                    .expect("مهلة الانشغال");
                let mut written = 0usize;
                for append_index in 0..APPENDS_PER_THREAD {
                    let transaction = connection
                        .transaction_with_behavior(TransactionBehavior::Immediate)
                        .expect("معاملة فورية");
                    let previous = latest_audit_event_hash(&transaction, &tenant_id)
                        .expect("قراءة الذيل داخل المعاملة");
                    append_audit_event(
                        &transaction,
                        &tenant_id,
                        &format!("audit_conc_{thread_index}_{append_index}"),
                        "CONCURRENT_APPEND",
                        "stress",
                        "StressProbe",
                        &format!("t{thread_index}-{append_index}"),
                        &Utc::now().to_rfc3339(),
                        &previous,
                    )
                    .expect("إلحاق داخل المعاملة");
                    transaction.commit().expect("التزام");
                    written += 1;
                }
                written
            })
        })
        .collect();

    let total_written: usize = handles
        .into_iter()
        .map(|handle| handle.join().expect("الخيط يجب أن ينتهي"))
        .sum();
    let elapsed_ms = started.elapsed().as_millis();

    let connection = Connection::open(&database_path).expect("إعادة الفتح");
    let (count, chain_ok, violations) =
        verify_chain_independently(&connection, &tenant_id).expect("تحقق مستقل");
    println!(
        "[STRESS] concurrent_writers={WRITER_THREADS} x {APPENDS_PER_THREAD} written={total_written} \
         rows={count} chain_ok={chain_ok} violations={violations} elapsed_ms={elapsed_ms}"
    );

    assert_eq!(total_written, WRITER_THREADS * APPENDS_PER_THREAD);
    let concurrent_rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM audit_events WHERE tenant_id = ?1 AND event_type = 'CONCURRENT_APPEND'",
            params![tenant_id],
            |row| row.get(0),
        )
        .expect("عدّ الإلحاقات المتزامنة");
    assert_eq!(
        concurrent_rows as usize,
        WRITER_THREADS * APPENDS_PER_THREAD,
        "لا إلحاق مفقود تحت التزامن"
    );
    assert!(
        chain_ok,
        "سلسلة التدقيق يجب ألا تتفرّع تحت الكتابة المتزامنة (انتهاكات={violations})"
    );

    // تحقق الإنتاج أيضاً على نفس الملف.
    let state = AppState::test_instance(
        Connection::open(&database_path).expect("فتح للحالة"),
        tenant_id,
        DEMO_OPERATOR_USER_ID.to_owned(),
    );
    assert!(
        verify_audit_chain(&state).expect("تحقق الإنتاج"),
        "تحقق الإنتاج يجب أن يوافق التحقق المستقل"
    );

    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("فحص السلامة");
    assert_eq!(integrity, "ok", "قاعدة اختبار التزامن يجب أن تبقى سليمة");

    std::fs::remove_dir_all(&directory).ok();
}

// =====================================================================
// 3) تحقق حي من سلامة **نسخة** قاعدة المستخدم وسلسلة تدقيقها
// ---------------------------------------------------------------------
// يُفتح الملف بعلامة القراءة فقط، فلا يمكن لهذا الاختبار أن يغيّر بايتاً.
// =====================================================================
#[test]
#[ignore = "يقرأ نسخة من قاعدة المستخدم — يُشغَّل صراحةً"]
fn live_user_database_copy_integrity_and_audit_chain() {
    // بلا مسار مطلق مُضمَّن في المصدر: يحدده المستدعي عبر المتغيّر، وإلا فمجلد
    // مؤقت النظام — لا مسار جهاز مطوّر داخل الشجرة المنشورة.
    let path = std::env::var("NEXUS_USER_DB_COPY").unwrap_or_else(|_| {
        std::env::temp_dir()
            .join("nexus-db-integrity-probe.db")
            .to_string_lossy()
            .into_owned()
    });
    let path = PathBuf::from(path);
    assert!(
        path.is_file(),
        "نسخة قاعدة المستخدم غير موجودة: {} — انسخها أولاً",
        path.display()
    );

    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .expect("فتح النسخة للقراءة فقط");
    println!(
        "[LIVE-DB] file={} bytes={}",
        path.display(),
        std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
    );

    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .expect("integrity_check");
    println!("[LIVE-DB] integrity_check={integrity}");
    assert_eq!(integrity, "ok", "سلامة قاعدة المستخدم");

    let foreign_key_violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .expect("foreign_key_check");
    println!("[LIVE-DB] foreign_key_violations={foreign_key_violations}");
    assert_eq!(foreign_key_violations, 0, "لا انتهاك مفاتيح أجنبية");

    let schema_version: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .expect("إصدار المخطط");
    let table_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .expect("عدد الجداول");
    println!("[LIVE-DB] schema_version={schema_version} tables={table_count}");

    let tenants: Vec<String> = {
        let mut statement = connection
            .prepare("SELECT id FROM tenants ORDER BY id ASC")
            .expect("قراءة المستأجرين");
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("تعداد المستأجرين");
        rows.filter_map(Result::ok).collect()
    };
    println!("[LIVE-DB] tenants={}", tenants.join(","));
    assert!(!tenants.is_empty(), "قاعدة المستخدم يجب أن تحوي مستأجراً");

    for tenant_id in &tenants {
        let (events, chain_ok, violations) =
            verify_chain_independently(&connection, tenant_id).expect("تحقق مستقل من السلسلة");
        let reportable: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE tenant_id = ?1",
                params![tenant_id],
                |row| row.get(0),
            )
            .expect("عدّ الأحداث");
        println!(
            "[LIVE-DB] tenant={tenant_id} audit_events={reportable} chain_ok={chain_ok} violations={violations}"
        );
        assert_eq!(events as i64, reportable);
        assert!(
            chain_ok,
            "سلسلة تدقيق المستأجر {tenant_id} يجب أن تكون سليمة (انتهاكات={violations})"
        );
    }

    // جدول الهجرات يجب أن يكون متسقاً مع عدده.
    let applied = {
        let mut statement = connection
            .prepare("SELECT version FROM schema_migrations ORDER BY version ASC")
            .expect("قراءة الهجرات");
        let rows = statement
            .query_map([], |row| row.get::<_, i64>(0))
            .expect("تعداد الهجرات");
        rows.filter_map(Result::ok).collect::<Vec<_>>()
    };
    println!("[LIVE-DB] applied_migrations={:?}", applied);
    assert!(
        applied.windows(2).all(|pair| pair[0] < pair[1]),
        "إصدارات الهجرات يجب أن تكون تصاعدية بلا تكرار"
    );
}

/// يُبقي الأنبوب مستعملاً بلا تحذير موت كود في حال تعطيل الاختبارات.
#[allow(dead_code)]
fn _unused_arc_marker() -> Arc<()> {
    Arc::new(())
}
