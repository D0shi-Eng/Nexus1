// قياسات أداء MP05 (§63-§65) — أرقام فعلية على الجهاز الحالي، مؤتمتة وقابلة للإعادة.
// تُشغَّل صراحةً بـ`--ignored --nocapture` لأنها قياس لا بوابة (لا تفشل ببطء عابر):
//   cargo test --lib -- --ignored --nocapture --test-threads=1 mp05_performance
//
// القاعدة المنهجية: تُفصل كلفة العتاد/التشفير عن كلفة التطبيق، ولا تُقاس كلفة النموذج
// هنا إطلاقاً (توليد Ollama يُقاس حياً ومفصولاً في تقرير الإصدار).

use std::time::Instant;

use rusqlite::Connection;

use crate::{
    application::authentication_service::{
        bootstrap_production, login, BootstrapProductionInput, LoginInput,
    },
    domain::models::RuntimeMode,
    infrastructure::database::{apply_migrations, ensure_permission_catalog, AppState},
};

fn millis(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

#[test]
#[ignore = "قياس أداء: يُشغَّل صراحةً بـ--ignored --nocapture"]
fn mp05_performance_measurements_are_reported() {
    // 1) فتح قاعدة جديدة + تطبيق سلسلة الهجرات كاملةً (0001→0021).
    let started = Instant::now();
    let connection = Connection::open_in_memory().expect("قاعدة القياس يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("مفاتيح الأجنبة يجب أن تعمل");
    apply_migrations(&connection).expect("الهجرات يجب أن تنجح");
    let migrate_ms = millis(started);

    // 2) كتالوج الأذونات (idempotent عند كل إقلاع).
    let started = Instant::now();
    ensure_permission_catalog(&connection).expect("الكتالوج يجب أن ينجح");
    let catalog_ms = millis(started);
    let permissions: i64 = connection
        .query_row("SELECT COUNT(*) FROM permissions", [], |row| row.get(0))
        .expect("عدّ الأذونات يجب أن يقرأ");

    // 3) إقلاع إنتاجي كامل: مستأجر + مستخدم + دور + منح + جلسة + تدقيق (يتضمن Argon2id).
    let state = AppState::test_instance(connection, "", "");
    state
        .set_runtime_mode(RuntimeMode::Unprovisioned)
        .expect("ضبط الوضع يجب أن ينجح");
    let started = Instant::now();
    bootstrap_production(
        &state,
        BootstrapProductionInput {
            organization_name: "Performance Measurement Org".to_owned(),
            // GAP-05-OPEN: الدولة والقطاع مُدخلان صريحان في قياس كلفة الإقلاع.
            country_code: "SA".to_owned(),
            industry: "Technology Services".to_owned(),
            admin_display_name: "Perf Admin".to_owned(),
            admin_email: "perf@admin.test".to_owned(),
            password: "Str0ng!Passw0rd#2026".to_owned(),
        },
    )
    .expect("الإقلاع الإنتاجي يجب أن ينجح");
    let bootstrap_ms = millis(started);

    // 4) دخول إنتاجي (Argon2id verify + جلسة + تدقيق).
    state.clear_active_identity().expect("محو الهوية");
    let started = Instant::now();
    login(
        &state,
        LoginInput {
            email: "perf@admin.test".to_owned(),
            password: "Str0ng!Passw0rd#2026".to_owned(),
        },
    )
    .expect("الدخول يجب أن ينجح");
    let login_ms = millis(started);

    // 5) كلفة Argon2id وحدها (هاش) — لفصل كلفة التشفير عن كلفة التطبيق.
    let started = Instant::now();
    let _hash = crate::application::authentication_service::hash_password("Str0ng!Passw0rd#2026")
        .expect("الهاش يجب أن ينجح");
    let hash_ms = millis(started);

    // 6) استعلام معتمد على الفهارس: قراءة قائمة الالتزامات (خدمة حقيقية ببوابة تفاعل).
    let started = Instant::now();
    let obligations =
        crate::application::obligation_service::list_obligations(&state).expect("القائمة تقرأ");
    let obligations_ms = millis(started);

    // 7) قراءة صفحة التدقيق (مقيدة بحد أعلى وصفحات مفتاحية).
    let started = Instant::now();
    let audit = crate::application::audit_service::list_audit_events(
        &state,
        crate::application::audit_service::AuditTrailPageInput { before: None },
    )
    .expect("التدقيق يُقرأ");
    let audit_ms = millis(started);

    // 8) فحص الصحة الكامل (استعلامات محلية فقط، بلا شبكة).
    let started = Instant::now();
    let health = crate::application::health_service::get_health(&state).expect("الصحة تُقرأ");
    let health_ms = millis(started);

    // 9) استرجاع معرفي معجمي FTS5 على قاعدة بلا مستندات (قياس مسار الاسترجاع نفسه).
    let started = Instant::now();
    let search = crate::application::knowledge_service::search(
        &state,
        crate::application::knowledge_service::SearchKnowledgeInput {
            query: "retention policy".to_owned(),
        },
    )
    .expect("البحث يجب أن ينجح");
    let search_ms = millis(started);

    println!("MP05-PERF migrate_chain_0001_0021_ms={migrate_ms:.1} (permissions={permissions})");
    println!("MP05-PERF permission_catalog_ms={catalog_ms:.1}");
    println!("MP05-PERF production_bootstrap_ms={bootstrap_ms:.1}");
    println!("MP05-PERF production_login_ms={login_ms:.1}");
    println!("MP05-PERF argon2id_hash_only_ms={hash_ms:.1}");
    println!(
        "MP05-PERF obligations_list_ms={obligations_ms:.1} (items={})",
        obligations.items.len()
    );
    println!(
        "MP05-PERF audit_page_50_ms={audit_ms:.1} (events={})",
        audit.items.len()
    );
    println!(
        "MP05-PERF system_health_ms={health_ms:.1} (schema={}, mode={})",
        health.schema_version, health.runtime_mode
    );
    println!(
        "MP05-PERF knowledge_fts_search_ms={search_ms:.1} (items={})",
        search.items.len()
    );

    // بوابات صحة معقولة فقط — لا تفشل القياسات ببطء عابر على جهاز مشغول.
    assert!(
        migrate_ms < 10_000.0,
        "سلسلة الهجرات بطيئة بشكل غير معقول: {migrate_ms:.1}ms"
    );
    assert!(
        login_ms < 10_000.0,
        "الدخول بطيء بشكل غير معقول: {login_ms:.1}ms"
    );
    assert!(health.schema_version == 26);
    assert_eq!(health.runtime_mode, RuntimeMode::Production.as_str());
}
