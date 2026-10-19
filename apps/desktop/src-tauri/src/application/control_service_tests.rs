use chrono::Utc;
use rusqlite::Connection;

use super::{
    create_control, disable_control, get_control_detail, link_control_evidence,
    link_control_obligation, link_control_risk, load_controls, record_control_assessment,
    unlink_control_evidence, unlink_control_obligation, unlink_control_risk, update_control,
    ControlEvidenceLinkInput, ControlIdInput, ControlObligationLinkInput, ControlRiskLinkInput,
    CreateControlInput, RecordControlAssessmentInput, SetControlStatusInput, UpdateControlInput,
};
use crate::{
    application::tests_test_fixture::{
        Fixture, EVIDENCE_A, EVIDENCE_B, OBLIGATION_A, OBLIGATION_B, PERMISSION_CONTROL_ASSESS,
        RISK_A, RISK_B, ROLE_A, TENANT_A, TENANT_B,
    },
    domain::error::AppError,
    infrastructure::database::AppState,
};

const LINK_PERMISSIONS: [&str; 2] = ["permission_control_read", "permission_control_link"];
const WRITE_PERMISSIONS: [&str; 4] = [
    "permission_control_read",
    "permission_control_create",
    "permission_control_update",
    "permission_control_disable",
];

fn create_input(title: &str) -> CreateControlInput {
    CreateControlInput {
        title: title.to_owned(),
        category: Some("ACCESS".to_owned()),
        owner: Some("Control owner".to_owned()),
    }
}

// مسار سعيد: الضابط يُنشأ ACTIVE بفعالية NOT_ASSESSED مع حدث تدقيق واحد بالضبط.
#[test]
fn creates_control_with_active_status_and_one_audit_event() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &WRITE_PERMISSIONS);

    let created = create_control(&fixture.state, create_input("Access review procedure"))
        .expect("إنشاء الضابط المصرح به يجب أن ينجح");

    assert_eq!(fixture.count("SELECT COUNT(*) FROM controls"), 1);
    assert_eq!(fixture.control_status(&created.control_id), "ACTIVE");
    assert_eq!(
        fixture.audit_count("CONTROL_CREATED", &created.control_id),
        1
    );
    let (status, effectiveness, category, owner): (String, String, Option<String>, Option<String>) =
        fixture
            .state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح")
            .query_row(
                "SELECT status, effectiveness, category, owner FROM controls WHERE id = ?1",
                [&created.control_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("قراءة الضابط يجب أن تنجح");
    assert_eq!(status, "ACTIVE");
    assert_eq!(effectiveness, "NOT_ASSESSED");
    assert_eq!(category.as_deref(), Some("ACCESS"));
    assert_eq!(owner.as_deref(), Some("Control owner"));
}

#[test]
fn rejects_empty_whitespace_and_oversized_control_titles_without_writing() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &WRITE_PERMISSIONS);

    for invalid_title in ["", "   ", "\t\n "] {
        assert!(matches!(
            create_control(&fixture.state, create_input(invalid_title)),
            Err(AppError::ValidationFailed)
        ));
    }
    let oversized = "ع".repeat(201);
    assert!(matches!(
        create_control(&fixture.state, create_input(&oversized)),
        Err(AppError::ValidationFailed)
    ));
    // 200 حرفاً بالضبط مقبولة — الحد أعلاه لا يُرفض.
    let exactly_max = "ع".repeat(200);
    assert!(create_control(&fixture.state, create_input(&exactly_max)).is_ok());

    assert_eq!(fixture.count("SELECT COUNT(*) FROM controls"), 1);
    assert_eq!(fixture.count("SELECT COUNT(*) FROM audit_events"), 1);
}

#[test]
fn rejects_category_or_owner_longer_than_the_declared_bound() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &WRITE_PERMISSIONS);

    let oversized = "x".repeat(121);
    assert!(matches!(
        create_control(
            &fixture.state,
            CreateControlInput {
                title: "Bounded control".to_owned(),
                category: Some(oversized.clone()),
                owner: None,
            }
        ),
        Err(AppError::ValidationFailed)
    ));
    assert!(matches!(
        create_control(
            &fixture.state,
            CreateControlInput {
                title: "Bounded control".to_owned(),
                category: None,
                owner: Some(oversized),
            }
        ),
        Err(AppError::ValidationFailed)
    ));
    assert_eq!(fixture.count("SELECT COUNT(*) FROM controls"), 0);
}

#[test]
fn denies_control_create_without_the_create_permission() {
    let fixture = Fixture::new();
    fixture.revoke(ROLE_A, "permission_control_create");

    assert!(matches!(
        create_control(&fixture.state, create_input("Unauthorized control")),
        Err(AppError::AuthorizationDenied)
    ));
    assert_eq!(fixture.count("SELECT COUNT(*) FROM controls"), 0);
    assert_eq!(fixture.count("SELECT COUNT(*) FROM audit_events"), 0);
}

#[test]
fn denies_control_update_without_the_update_permission() {
    let fixture = Fixture::new();
    fixture.revoke(ROLE_A, "permission_control_update");
    let created = seed_control(&fixture, "control-update-denied");

    let result = update_control(
        &fixture.state,
        UpdateControlInput {
            control_id: created.clone(),
            title: "Renamed without authority".to_owned(),
            category: None,
            owner: None,
        },
    );

    assert!(matches!(result, Err(AppError::AuthorizationDenied)));
    let title: String = fixture
        .state
        .connection
        .lock()
        .expect("قفل الاتصال يجب أن يتاح")
        .query_row(
            "SELECT title FROM controls WHERE id = ?1",
            [&created],
            |row| row.get(0),
        )
        .expect("قراءة العنوان يجب أن تنجح");
    assert_eq!(title, "Seed control");
    assert_eq!(fixture.audit_count("CONTROL_UPDATED", &created), 0);
}

#[test]
fn updates_control_fields_and_audits_once() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &WRITE_PERMISSIONS);
    let created = seed_control(&fixture, "control-update-ok");

    let updated = update_control(
        &fixture.state,
        UpdateControlInput {
            control_id: created.clone(),
            title: "  Renamed control  ".to_owned(),
            category: Some("  ".to_owned()),
            owner: Some("New owner".to_owned()),
        },
    )
    .expect("التحديث المصرح به يجب أن ينجح");

    assert_eq!(updated.control_id, created);
    let (title, category, owner, status): (String, Option<String>, Option<String>, String) =
        fixture
            .state
            .connection
            .lock()
            .expect("قفل الاتصال يجب أن يتاح")
            .query_row(
                "SELECT title, category, owner, status FROM controls WHERE id = ?1",
                [&created],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("قراءة الضابط يجب أن تنجح");
    assert_eq!(title, "Renamed control");
    assert_eq!(category, None, "الفراغ يُطبّع إلى NULL لا سلسلة فارغة");
    assert_eq!(owner.as_deref(), Some("New owner"));
    assert_eq!(status, "ACTIVE", "التحديث الوصفي لا يمس الحالة");
    assert_eq!(fixture.audit_count("CONTROL_UPDATED", &created), 1);
}

#[test]
fn unknown_control_id_is_not_found_and_writes_nothing() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &WRITE_PERMISSIONS);

    assert!(matches!(
        update_control(
            &fixture.state,
            UpdateControlInput {
                control_id: "control-does-not-exist".to_owned(),
                title: "Ghost".to_owned(),
                category: None,
                owner: None,
            }
        ),
        Err(AppError::ResourceNotFound)
    ));
    assert!(matches!(
        disable_control(
            &fixture.state,
            SetControlStatusInput {
                control_id: "control-does-not-exist".to_owned(),
                status: "DISABLED".to_owned(),
            }
        ),
        Err(AppError::ResourceNotFound)
    ));
    assert!(matches!(
        record_control_assessment(
            &fixture.state,
            RecordControlAssessmentInput {
                control_id: "control-does-not-exist".to_owned(),
                effectiveness: "EFFECTIVE".to_owned(),
                assessment_note: None,
            }
        ),
        Err(AppError::ResourceNotFound)
    ));
    assert_eq!(fixture.count("SELECT COUNT(*) FROM audit_events"), 0);
}

#[test]
fn disables_control_and_rejects_statuses_outside_the_closed_list() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &WRITE_PERMISSIONS);
    let created = seed_control(&fixture, "control-disable");

    let disabled = disable_control(
        &fixture.state,
        SetControlStatusInput {
            control_id: created.clone(),
            status: "DISABLED".to_owned(),
        },
    )
    .expect("تعطيل الضابط المصرح به يجب أن ينجح");

    assert_eq!(
        crate::domain::models::control_status_code(&disabled.status),
        "DISABLED"
    );
    assert_eq!(fixture.control_status(&created), "DISABLED");
    assert_eq!(fixture.audit_count("CONTROL_STATUS_CHANGED", &created), 1);

    assert!(matches!(
        disable_control(
            &fixture.state,
            SetControlStatusInput {
                control_id: created.clone(),
                status: "ARCHIVED".to_owned(),
            }
        ),
        Err(AppError::ValidationFailed)
    ));
    assert_eq!(fixture.control_status(&created), "DISABLED");
    assert_eq!(fixture.audit_count("CONTROL_STATUS_CHANGED", &created), 1);
}

#[test]
fn denies_control_disable_without_the_disable_permission() {
    let fixture = Fixture::new();
    fixture.revoke(ROLE_A, "permission_control_disable");
    let created = seed_control(&fixture, "control-disable-denied");

    assert!(matches!(
        disable_control(
            &fixture.state,
            SetControlStatusInput {
                control_id: created.clone(),
                status: "DISABLED".to_owned(),
            }
        ),
        Err(AppError::AuthorizationDenied)
    ));
    assert_eq!(fixture.control_status(&created), "ACTIVE");
}

fn seed_control(fixture: &Fixture, control_id: &str) -> String {
    let now = Utc::now().to_rfc3339();
    fixture
        .state
        .connection
        .lock()
        .expect("قفل الاتصال يجب أن يتاح")
        .execute(
            "INSERT INTO controls (id, tenant_id, title, status, category, owner, effectiveness, last_assessed_at, assessment_note, created_at, updated_at) VALUES (?1, ?2, 'Seed control', 'ACTIVE', NULL, NULL, 'NOT_ASSESSED', NULL, NULL, ?3, ?3)",
            (control_id, TENANT_A, now),
        )
        .expect("ضابط الاختبار يجب أن ينشأ");
    control_id.to_owned()
}

// ===== الروابط =====

#[test]
fn links_and_unlinks_obligation_risk_and_evidence_with_one_audit_event_each() {
    let fixture = Fixture::new();
    let created = seed_control(&fixture, "control-link-happy");

    let obligation_link = link_control_obligation(
        &fixture.state,
        ControlObligationLinkInput {
            control_id: created.clone(),
            obligation_id: OBLIGATION_A.to_owned(),
        },
    )
    .expect("ربط الالتزام يجب أن ينجح");
    assert!(obligation_link.changed);
    assert_eq!(obligation_link.link_count, 1);

    let risk_link = link_control_risk(
        &fixture.state,
        ControlRiskLinkInput {
            control_id: created.clone(),
            risk_id: RISK_A.to_owned(),
        },
    )
    .expect("ربط الخطر يجب أن ينجح");
    assert!(risk_link.changed);

    let evidence_link = link_control_evidence(
        &fixture.state,
        ControlEvidenceLinkInput {
            control_id: created.clone(),
            evidence_id: EVIDENCE_A.to_owned(),
        },
    )
    .expect("ربط الدليل يجب أن ينجح");
    assert!(evidence_link.changed);

    assert_eq!(fixture.count("SELECT COUNT(*) FROM control_obligations"), 1);
    assert_eq!(fixture.count("SELECT COUNT(*) FROM control_risks"), 1);
    assert_eq!(fixture.count("SELECT COUNT(*) FROM control_evidence"), 1);
    assert_eq!(
        fixture
            .count("SELECT COUNT(*) FROM audit_events WHERE event_type = 'CONTROL_LINK_CHANGED'"),
        3
    );

    let unlinked = unlink_control_obligation(
        &fixture.state,
        ControlObligationLinkInput {
            control_id: created.clone(),
            obligation_id: OBLIGATION_A.to_owned(),
        },
    )
    .expect("فك الربط يجب أن ينجح");
    assert!(unlinked.changed);
    assert_eq!(unlinked.link_count, 0);
    assert_eq!(fixture.count("SELECT COUNT(*) FROM control_obligations"), 0);

    unlink_control_risk(
        &fixture.state,
        ControlRiskLinkInput {
            control_id: created.clone(),
            risk_id: RISK_A.to_owned(),
        },
    )
    .expect("فك ربط الخطر يجب أن ينجح");
    unlink_control_evidence(
        &fixture.state,
        ControlEvidenceLinkInput {
            control_id: created.clone(),
            evidence_id: EVIDENCE_A.to_owned(),
        },
    )
    .expect("فك ربط الدليل يجب أن ينجح");
    assert_eq!(fixture.count("SELECT COUNT(*) FROM control_risks"), 0);
    assert_eq!(fixture.count("SELECT COUNT(*) FROM control_evidence"), 0);
}

// التكرار محايد (idempotent) وليس خطأً: نفس الزوج يُعلن changed=false بلا حدث تدقيق مكرر.
#[test]
fn relinking_the_same_pair_is_idempotent_and_never_duplicates_rows() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &LINK_PERMISSIONS);
    let created = seed_control(&fixture, "control-link-idempotent");

    let first = link_control_obligation(
        &fixture.state,
        ControlObligationLinkInput {
            control_id: created.clone(),
            obligation_id: OBLIGATION_A.to_owned(),
        },
    )
    .expect("الربط الأول يجب أن ينجح");
    let second = link_control_obligation(
        &fixture.state,
        ControlObligationLinkInput {
            control_id: created.clone(),
            obligation_id: OBLIGATION_A.to_owned(),
        },
    )
    .expect("إعادة الربط يجب أن تُقبل كمحايدة");

    assert!(first.changed);
    assert!(!second.changed);
    assert_eq!(second.link_count, 1);
    assert_eq!(fixture.count("SELECT COUNT(*) FROM control_obligations"), 1);
    assert_eq!(
        fixture
            .count("SELECT COUNT(*) FROM audit_events WHERE event_type = 'CONTROL_LINK_CHANGED'"),
        1
    );
}

#[test]
fn unlinking_a_missing_row_is_rejected_and_writes_nothing() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &LINK_PERMISSIONS);
    let created = seed_control(&fixture, "control-unlink-missing");

    assert!(matches!(
        unlink_control_obligation(
            &fixture.state,
            ControlObligationLinkInput {
                control_id: created.clone(),
                obligation_id: OBLIGATION_A.to_owned(),
            }
        ),
        Err(AppError::ResourceNotFound)
    ));
    assert_eq!(fixture.count("SELECT COUNT(*) FROM audit_events"), 0);
}

#[test]
fn denies_linking_without_the_link_permission() {
    let fixture = Fixture::new();
    fixture.revoke(ROLE_A, "permission_control_link");
    let created = seed_control(&fixture, "control-link-denied");

    assert!(matches!(
        link_control_obligation(
            &fixture.state,
            ControlObligationLinkInput {
                control_id: created,
                obligation_id: OBLIGATION_A.to_owned(),
            }
        ),
        Err(AppError::AuthorizationDenied)
    ));
    assert_eq!(fixture.count("SELECT COUNT(*) FROM control_obligations"), 0);
}

#[test]
fn cross_tenant_link_targets_are_not_found_and_write_nothing() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &LINK_PERMISSIONS);
    let created = seed_control(&fixture, "control-link-cross-tenant");

    // الأهداف موجودة فعلاً لكنها ملك مستأجر آخر — الرد قرار "غير موجود" لا خطأ تخزين.
    assert!(matches!(
        link_control_obligation(
            &fixture.state,
            ControlObligationLinkInput {
                control_id: created.clone(),
                obligation_id: OBLIGATION_B.to_owned(),
            }
        ),
        Err(AppError::ResourceNotFound)
    ));
    assert!(matches!(
        link_control_risk(
            &fixture.state,
            ControlRiskLinkInput {
                control_id: created.clone(),
                risk_id: RISK_B.to_owned(),
            }
        ),
        Err(AppError::ResourceNotFound)
    ));
    assert!(matches!(
        link_control_evidence(
            &fixture.state,
            ControlEvidenceLinkInput {
                control_id: created.clone(),
                evidence_id: EVIDENCE_B.to_owned(),
            }
        ),
        Err(AppError::ResourceNotFound)
    ));

    assert_eq!(fixture.count("SELECT COUNT(*) FROM control_obligations"), 0);
    assert_eq!(fixture.count("SELECT COUNT(*) FROM control_risks"), 0);
    assert_eq!(fixture.count("SELECT COUNT(*) FROM control_evidence"), 0);
    assert_eq!(fixture.count("SELECT COUNT(*) FROM audit_events"), 0);
}

#[test]
fn cross_tenant_control_id_is_not_found_for_links() {
    let fixture = Fixture::new();
    fixture.switch_to_tenant_b();

    // ضابط المستأجر أ يُطلب من مستأجر ب — لا رؤية ولا كتابة عبر الحدود.
    let now = Utc::now().to_rfc3339();
    fixture
        .state
        .connection
        .lock()
        .expect("قفل الاتصال يجب أن يتاح")
        .execute(
            "INSERT INTO controls (id, tenant_id, title, status, category, owner, effectiveness, last_assessed_at, assessment_note, created_at, updated_at) VALUES ('control-tenant-a-only', 'tenant-fixture-a', 'Tenant A control', 'ACTIVE', NULL, NULL, 'NOT_ASSESSED', NULL, NULL, ?1, ?1)",
            [&now],
        )
        .expect("ضابط المستأجر أ يجب أن ينشأ");

    assert!(matches!(
        link_control_obligation(
            &fixture.state,
            ControlObligationLinkInput {
                control_id: "control-tenant-a-only".to_owned(),
                obligation_id: OBLIGATION_B.to_owned(),
            }
        ),
        Err(AppError::ResourceNotFound)
    ));
    assert_eq!(fixture.count("SELECT COUNT(*) FROM control_obligations"), 0);
}

// فك الربط يحذف صف المستأجر النشط فقط: صف مستأجر آخر بنفس المعرّفات يبقى سليماً.
#[test]
fn unlink_removes_only_the_active_tenants_row() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &LINK_PERMISSIONS);
    let created = seed_control(&fixture, "control-unlink-isolation");

    link_control_obligation(
        &fixture.state,
        ControlObligationLinkInput {
            control_id: created.clone(),
            obligation_id: OBLIGATION_A.to_owned(),
        },
    )
    .expect("ربط المستأجر أ يجب أن ينجح");

    let now = Utc::now().to_rfc3339();
    fixture
        .state
        .connection
        .lock()
        .expect("قفل الاتصال يجب أن يتاح")
        .execute(
            "INSERT INTO controls (id, tenant_id, title, status, category, owner, effectiveness, last_assessed_at, assessment_note, created_at, updated_at) VALUES ('control-tenant-b-same', 'tenant-fixture-b', 'Tenant B control', 'ACTIVE', NULL, NULL, 'NOT_ASSESSED', NULL, NULL, ?1, ?1)",
            [&now],
        )
        .expect("ضابط المستأجر ب يجب أن ينشأ");
    fixture
        .state
        .connection
        .lock()
        .expect("قفل الاتصال يجب أن يتاح")
        .execute(
            "INSERT INTO control_obligations (control_id, obligation_id, tenant_id, created_at) VALUES ('control-tenant-b-same', ?1, ?2, ?3)",
            (OBLIGATION_B, TENANT_B, &now),
        )
        .expect("رابط المستأجر ب يجب أن ينشأ");
    assert_eq!(fixture.count("SELECT COUNT(*) FROM control_obligations"), 2);

    unlink_control_obligation(
        &fixture.state,
        ControlObligationLinkInput {
            control_id: created,
            obligation_id: OBLIGATION_A.to_owned(),
        },
    )
    .expect("فك ربط المستأجر أ يجب أن ينجح");

    assert_eq!(fixture.count("SELECT COUNT(*) FROM control_obligations"), 1);
    assert_eq!(
        fixture
            .count("SELECT COUNT(*) FROM control_obligations WHERE tenant_id = 'tenant-fixture-b'"),
        1
    );
}

// ===== التقييم =====

#[test]
fn records_assessment_effectiveness_and_timestamp_with_one_audit_event() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &WRITE_PERMISSIONS);
    fixture.grant(ROLE_A, &[PERMISSION_CONTROL_ASSESS]);
    let created = seed_control(&fixture, "control-assess");

    let assessed = record_control_assessment(
        &fixture.state,
        RecordControlAssessmentInput {
            control_id: created.clone(),
            effectiveness: "PARTIALLY_EFFECTIVE".to_owned(),
            assessment_note: Some("  ملاحظة تقييم  ".to_owned()),
        },
    )
    .expect("التقييم المصرح به يجب أن ينجح");

    assert_eq!(assessed.control_id, created);
    let (effectiveness, last_assessed_at, note): (String, Option<String>, Option<String>) = fixture
        .state
        .connection
        .lock()
        .expect("قفل الاتصال يجب أن يتاح")
        .query_row(
            "SELECT effectiveness, last_assessed_at, assessment_note FROM controls WHERE id = ?1",
            [&created],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("قراءة التقييم يجب أن تنجح");
    assert_eq!(effectiveness, "PARTIALLY_EFFECTIVE");
    assert_eq!(
        last_assessed_at.as_deref(),
        Some(assessed.last_assessed_at.as_str())
    );
    assert_eq!(note.as_deref(), Some("ملاحظة تقييم"));
    assert_eq!(fixture.audit_count("CONTROL_ASSESSED", &created), 1);
}

#[test]
fn denies_assessment_without_the_assess_permission() {
    let fixture = Fixture::new();
    fixture.revoke(ROLE_A, PERMISSION_CONTROL_ASSESS);
    let created = seed_control(&fixture, "control-assess-denied");

    assert!(matches!(
        record_control_assessment(
            &fixture.state,
            RecordControlAssessmentInput {
                control_id: created.clone(),
                effectiveness: "EFFECTIVE".to_owned(),
                assessment_note: None,
            }
        ),
        Err(AppError::AuthorizationDenied)
    ));
    let effectiveness: String = fixture
        .state
        .connection
        .lock()
        .expect("قفل الاتصال يجب أن يتاح")
        .query_row(
            "SELECT effectiveness FROM controls WHERE id = ?1",
            [&created],
            |row| row.get(0),
        )
        .expect("قراءة الفعالية يجب أن تنجح");
    assert_eq!(effectiveness, "NOT_ASSESSED");
    assert_eq!(fixture.audit_count("CONTROL_ASSESSED", &created), 0);
}

#[test]
fn rejects_effectiveness_values_outside_the_closed_list() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &WRITE_PERMISSIONS);
    fixture.grant(ROLE_A, &[PERMISSION_CONTROL_ASSESS]);
    let created = seed_control(&fixture, "control-assess-invalid");

    assert!(matches!(
        record_control_assessment(
            &fixture.state,
            RecordControlAssessmentInput {
                control_id: created.clone(),
                effectiveness: "EXCELLENT".to_owned(),
                assessment_note: None,
            }
        ),
        Err(AppError::ValidationFailed)
    ));
    assert_eq!(fixture.audit_count("CONTROL_ASSESSED", &created), 0);
}

// ===== القراءة والعزل =====

#[test]
fn returns_authorized_tenant_controls_with_link_counts_and_status_summary() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &LINK_PERMISSIONS);
    let created = seed_control(&fixture, "control-list-a");
    link_control_obligation(
        &fixture.state,
        ControlObligationLinkInput {
            control_id: created.clone(),
            obligation_id: OBLIGATION_A.to_owned(),
        },
    )
    .expect("ربط الالتزام يجب أن ينجح");
    link_control_evidence(
        &fixture.state,
        ControlEvidenceLinkInput {
            control_id: created.clone(),
            evidence_id: EVIDENCE_A.to_owned(),
        },
    )
    .expect("ربط الدليل يجب أن ينجح");
    fixture
        .state
        .connection
        .lock()
        .expect("قفل الاتصال يجب أن يتاح")
        .execute(
            "INSERT INTO controls (id, tenant_id, title, status, category, owner, effectiveness, last_assessed_at, assessment_note, created_at, updated_at) VALUES ('control-list-older', 'tenant-fixture-a', 'Retention procedure', 'INACTIVE', NULL, NULL, 'NOT_ASSESSED', NULL, NULL, '2026-08-18T00:00:00Z', '2026-08-18T00:00:00Z')",
            [],
        )
        .expect("الضابط الأقدم يجب أن ينشأ");

    let data = load_controls(&fixture.state).expect("قراءة الضوابط المصرح بها يجب أن تنجح");

    assert!(data.has_data);
    assert_eq!(data.summary.total, 2);
    assert_eq!(data.summary.active, 1);
    assert_eq!(data.summary.inactive, 1);
    assert_eq!(data.summary.disabled, 0);
    let first = data
        .items
        .iter()
        .find(|item| item.id == created)
        .expect("الضابط المربوط يجب أن يظهر");
    assert_eq!(first.linked_obligation_count, 1);
    assert_eq!(first.linked_risk_count, 0);
    assert_eq!(first.linked_evidence_count, 1);
    assert_eq!(
        crate::domain::models::control_status_code(&first.status),
        "ACTIVE"
    );
    assert!(data
        .items
        .iter()
        .all(|item| item.id != "control-tenant-b-same"));
}

#[test]
fn returns_control_detail_with_ordered_links_and_isolates_tenants() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &LINK_PERMISSIONS);
    let created = seed_control(&fixture, "control-detail");
    link_control_obligation(
        &fixture.state,
        ControlObligationLinkInput {
            control_id: created.clone(),
            obligation_id: OBLIGATION_A.to_owned(),
        },
    )
    .expect("ربط الالتزام يجب أن ينجح");
    link_control_risk(
        &fixture.state,
        ControlRiskLinkInput {
            control_id: created.clone(),
            risk_id: RISK_A.to_owned(),
        },
    )
    .expect("ربط الخطر يجب أن ينجح");
    link_control_evidence(
        &fixture.state,
        ControlEvidenceLinkInput {
            control_id: created.clone(),
            evidence_id: EVIDENCE_A.to_owned(),
        },
    )
    .expect("ربط الدليل يجب أن ينجح");

    let detail = get_control_detail(
        &fixture.state,
        ControlIdInput {
            control_id: created.clone(),
        },
    )
    .expect("تفاصيل الضابط يجب أن تُقرأ");
    assert_eq!(detail.control.id, created);
    assert_eq!(detail.control.linked_obligation_count, 1);
    assert_eq!(
        detail
            .linked_obligations
            .iter()
            .map(|link| link.obligation_id.as_str())
            .collect::<Vec<_>>(),
        vec![OBLIGATION_A]
    );
    assert_eq!(
        detail
            .linked_risks
            .iter()
            .map(|link| link.risk_id.as_str())
            .collect::<Vec<_>>(),
        vec![RISK_A]
    );
    assert_eq!(
        detail
            .linked_evidence
            .iter()
            .map(|link| link.evidence_id.as_str())
            .collect::<Vec<_>>(),
        vec![EVIDENCE_A]
    );
    assert_eq!(detail.linked_obligations[0].title, "Obligation a");
    assert_eq!(detail.linked_risks[0].title, "Risk a");
    assert_eq!(detail.linked_evidence[0].label, "Evidence a");

    // نفس المعرّف من مستأجر آخر: غير موجود.
    fixture.switch_to_tenant_b();
    assert!(matches!(
        get_control_detail(
            &fixture.state,
            ControlIdInput {
                control_id: created.clone(),
            }
        ),
        Err(AppError::ResourceNotFound)
    ));

    // القائمة للمستأجر ب لا تحتوي ضابط المستأجر أ.
    let tenant_b_data = load_controls(&fixture.state).expect("قراءة المستأجر ب يجب أن تنجح");
    assert!(tenant_b_data.items.iter().all(|item| item.id != created));
}

#[test]
fn denies_control_read_without_the_read_permission() {
    let fixture = Fixture::new();
    fixture.revoke(ROLE_A, "permission_control_read");
    assert!(matches!(
        load_controls(&fixture.state),
        Err(AppError::AuthorizationDenied)
    ));
    assert!(matches!(
        get_control_detail(
            &fixture.state,
            ControlIdInput {
                control_id: "control-anything".to_owned(),
            }
        ),
        Err(AppError::AuthorizationDenied)
    ));
}

#[test]
fn rejects_oversized_identifiers_before_touching_the_database() {
    let fixture = Fixture::new();
    fixture.grant(ROLE_A, &WRITE_PERMISSIONS);
    fixture.grant(ROLE_A, &[PERMISSION_CONTROL_ASSESS]);
    let oversized = "c".repeat(129);

    assert!(matches!(
        update_control(
            &fixture.state,
            UpdateControlInput {
                control_id: oversized.clone(),
                title: "Bounded".to_owned(),
                category: None,
                owner: None,
            }
        ),
        Err(AppError::ValidationFailed)
    ));
    assert!(matches!(
        disable_control(
            &fixture.state,
            SetControlStatusInput {
                control_id: String::new(),
                status: "DISABLED".to_owned(),
            }
        ),
        Err(AppError::ValidationFailed)
    ));
    assert!(matches!(
        record_control_assessment(
            &fixture.state,
            RecordControlAssessmentInput {
                control_id: oversized.clone(),
                effectiveness: "EFFECTIVE".to_owned(),
                assessment_note: None,
            }
        ),
        Err(AppError::ValidationFailed)
    ));
    assert!(matches!(
        link_control_obligation(
            &fixture.state,
            ControlObligationLinkInput {
                control_id: "control-x".to_owned(),
                obligation_id: oversized,
            }
        ),
        Err(AppError::ValidationFailed)
    ));
    assert_eq!(fixture.count("SELECT COUNT(*) FROM audit_events"), 0);
}

// تجميع حالة الدليل على الالتزام يعاد حسابه من نفس قاعدة مسار التسجيل القائم:
// أي دليل موجود ⇒ REQUIRES_REVIEW، وغياب كل الأدلة ⇒ MISSING.
#[test]
fn obligation_evidence_aggregate_follows_the_register_path_rule() {
    let fixture = Fixture::new();
    let connection = fixture
        .state
        .connection
        .lock()
        .expect("قفل الاتصال يجب أن يتاح");
    let transaction = connection
        .unchecked_transaction()
        .expect("المعاملة يجب أن تبدأ");
    let with_evidence = crate::infrastructure::evidence_repository::obligation_evidence_status(
        &transaction,
        TENANT_A,
        OBLIGATION_A,
    )
    .expect("التجميع يجب أن ينجح");
    let without_evidence = crate::infrastructure::evidence_repository::obligation_evidence_status(
        &transaction,
        TENANT_A,
        "obligation-without-evidence",
    )
    .expect("التجميع يجب أن ينجح");
    drop(transaction);

    assert_eq!(with_evidence, "REQUIRES_REVIEW");
    assert_eq!(without_evidence, "MISSING");
}

// ===== اختباران قديمان (نفس نمط الحامل السابق) — لا عمود حالة مخترع ولا طفرات في القراءة =====

fn control_read_state() -> AppState {
    let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
    for migration in [
        include_str!("../../migrations/0001_initial.sql"),
        include_str!("../../migrations/0002_identity_and_access.sql"),
        include_str!("../../migrations/0003_workflow_and_risk.sql"),
        include_str!("../../migrations/0004_evidence_metadata.sql"),
        include_str!("../../migrations/0005_ai_governance.sql"),
        include_str!("../../migrations/0006_correct_demo_truth_labels.sql"),
        include_str!("../../migrations/0007_obligation_management.sql"),
        include_str!("../../migrations/0008_obligation_relationship_update_isolation.sql"),
        include_str!("../../migrations/0009_controls_read_model.sql"),
        include_str!("../../migrations/0022_controls_authoring.sql"),
        include_str!("../../migrations/0025_task_lifecycle.sql"),
        include_str!("../../migrations/0026_production_user_and_authoring.sql"),
    ] {
        connection
            .execute_batch(migration)
            .expect("هجرة الاختبار يجب أن تنجح");
    }

    let now = Utc::now().to_rfc3339();
    for tenant_id in [
        "tenant-control-a",
        "tenant-control-b",
        "tenant-control-empty",
    ] {
        connection
            .execute(
                "INSERT INTO tenants (id, name, data_mode, created_at, updated_at) VALUES (?1, ?1, 'DEMO_SYNTHETIC', ?2, ?2)",
                (tenant_id, &now),
            )
            .expect("المستأجر يجب أن ينشأ");
    }
    connection
        .execute(
            "INSERT INTO permissions (id, code, resource_type, action, classification_ceiling, created_at) VALUES ('permission-control-read', 'control:read', 'CONTROL', 'READ', 'HIGHLY_RESTRICTED', ?1)",
            [&now],
        )
        .expect("صلاحية الضوابط يجب أن تنشأ");

    insert_control(
        &connection,
        "control-a-new",
        "tenant-control-a",
        "Access review procedure",
        "2026-08-19T00:00:00Z",
    );
    insert_control(
        &connection,
        "control-a-old",
        "tenant-control-a",
        "Retention procedure",
        "2026-08-18T00:00:00Z",
    );
    insert_control(
        &connection,
        "control-b-only",
        "tenant-control-b",
        "Tenant B procedure",
        "2026-08-20T00:00:00Z",
    );
    create_control_reader(
        &connection,
        "tenant-control-a",
        "user-control-a",
        "role-control-a",
        "session-control-a",
        &now,
    );

    AppState::test_instance(
        connection,
        "tenant-control-a".to_owned(),
        "user-control-a".to_owned(),
    )
}

fn insert_control(
    connection: &Connection,
    id: &str,
    tenant_id: &str,
    title: &str,
    updated_at: &str,
) {
    connection
        .execute(
            "INSERT INTO controls (id, tenant_id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
            (id, tenant_id, title, updated_at),
        )
        .expect("ضابط الاختبار يجب أن ينشأ");
}

fn create_control_reader(
    connection: &Connection,
    tenant_id: &str,
    user_id: &str,
    role_id: &str,
    session_id: &str,
    now: &str,
) {
    connection
        .execute(
            "INSERT INTO local_users (id, tenant_id, display_name, email, department_id, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, NULL, 'ACTIVE', ?5, ?5)",
            (user_id, tenant_id, user_id, format!("{user_id}@test.local"), now),
        )
        .expect("المستخدم يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO roles (id, tenant_id, code, display_name, created_at, updated_at) VALUES (?1, ?2, 'READER', 'Reader', ?3, ?3)",
            (role_id, tenant_id, now),
        )
        .expect("الدور يجب أن ينشأ");
    connection
        .execute(
            "INSERT INTO user_role_assignments (user_id, role_id, assigned_at, assigned_by) VALUES (?1, ?2, ?3, 'system')",
            (user_id, role_id, now),
        )
        .expect("تعيين الدور يجب أن ينجح");
    connection
        .execute(
            "INSERT INTO role_permissions (role_id, permission_id) VALUES (?1, 'permission-control-read')",
            [role_id],
        )
        .expect("تعيين صلاحية الضوابط يجب أن ينجح");
    connection
        .execute(
            "INSERT INTO local_sessions (id, tenant_id, user_id, issued_at, expires_at, revoked_at, device_label) VALUES (?1, ?2, ?3, ?4, '2030-01-01T00:00:00Z', NULL, 'test')",
            (session_id, tenant_id, user_id, now),
        )
        .expect("الجلسة يجب أن تنشأ");
}

#[test]
fn returns_authorized_tenant_controls_with_direct_aggregate_counts() {
    let state = control_read_state();

    let data = load_controls(&state).expect("قراءة الضوابط المصرح بها يجب أن تنجح");

    assert!(data.has_data);
    assert_eq!(data.summary.total, 2);
    assert_eq!(data.items.len(), 2);
    assert_eq!(
        data.items
            .iter()
            .map(|control| control.id.as_str())
            .collect::<Vec<_>>(),
        vec!["control-a-new", "control-a-old"]
    );
    assert!(data
        .items
        .iter()
        .all(|control| !control.id.contains("control-b")));
}

#[test]
fn denies_read_without_control_read_permission() {
    let state = control_read_state();
    state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .execute("DELETE FROM role_permissions", [])
        .expect("إزالة الصلاحية يجب أن تنجح");

    assert!(matches!(
        load_controls(&state),
        Err(AppError::AuthorizationDenied)
    ));
}

#[test]
fn denies_read_for_expired_revoked_and_locked_identity() {
    let state = control_read_state();
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute("UPDATE local_sessions SET expires_at = '2020-01-01T00:00:00Z' WHERE id = 'session-control-a'", [])
            .expect("انتهاء الجلسة يجب أن ينجح");
    }
    assert!(matches!(
        load_controls(&state),
        Err(AppError::AuthorizationDenied)
    ));

    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute("UPDATE local_sessions SET expires_at = '2030-01-01T00:00:00Z', revoked_at = '2026-01-01T00:00:00Z' WHERE id = 'session-control-a'", [])
            .expect("إلغاء الجلسة يجب أن ينجح");
    }
    assert!(matches!(
        load_controls(&state),
        Err(AppError::AuthorizationDenied)
    ));

    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        connection
            .execute(
                "UPDATE local_sessions SET revoked_at = NULL WHERE id = 'session-control-a'",
                [],
            )
            .expect("استعادة الجلسة يجب أن تنجح");
        connection
            .execute(
                "UPDATE local_users SET status = 'LOCKED' WHERE id = 'user-control-a'",
                [],
            )
            .expect("قفل المستخدم يجب أن ينجح");
    }
    assert!(matches!(
        load_controls(&state),
        Err(AppError::AuthorizationDenied)
    ));
}

#[test]
fn isolates_control_list_and_summary_between_authorized_tenants() {
    let state = control_read_state();
    let now = Utc::now().to_rfc3339();
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        create_control_reader(
            &connection,
            "tenant-control-b",
            "user-control-b",
            "role-control-b",
            "session-control-b",
            &now,
        );
    }

    state.set_active_tenant_for_tests("tenant-control-b");
    state.set_active_user_for_tests("user-control-b");
    let data = load_controls(&state).expect("المستأجر ب يجب أن يقرأ ضوابطه");

    assert_eq!(data.summary.total, 1);
    assert_eq!(data.items.len(), 1);
    assert_eq!(data.items[0].id, "control-b-only");
}

#[test]
fn denies_user_when_authenticated_state_is_bound_to_another_tenant() {
    let state = control_read_state();
    state.set_active_tenant_for_tests("tenant-control-b");

    assert!(matches!(
        load_controls(&state),
        Err(AppError::AuthorizationDenied)
    ));
}

#[test]
fn returns_empty_tenant_without_creating_audit_events() {
    let state = control_read_state();
    let now = Utc::now().to_rfc3339();
    {
        let connection = state.connection.lock().expect("القفل يجب أن يتاح");
        create_control_reader(
            &connection,
            "tenant-control-empty",
            "user-control-empty",
            "role-control-empty",
            "session-control-empty",
            &now,
        );
    }
    state.set_active_tenant_for_tests("tenant-control-empty");
    state.set_active_user_for_tests("user-control-empty");

    let before_count: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .expect("عدد التدقيق يجب أن يقرأ");
    let data = load_controls(&state).expect("المستأجر الفارغ يجب أن يقرأ");
    let after_count: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .expect("عدد التدقيق يجب أن يقرأ");

    assert!(!data.has_data);
    assert_eq!(data.summary.total, 0);
    assert!(data.items.is_empty());
    assert_eq!(before_count, after_count);
}

// تحديث مقصود: الجدول صار يحمل دورة حياة تأليف حقيقية (0022)،
// فاختبار "لا عمود حالة مخترع" القديم صار لاغياً بوصفه ويُستبدل بالإقرار الصريح بالمجموعة الجديدة.
#[test]
fn has_the_intentional_control_column_set_after_the_authoring_migration() {
    let fixture = Fixture::new();

    assert_eq!(
        fixture.column_names("controls"),
        vec![
            "id",
            "tenant_id",
            "title",
            "created_at",
            "updated_at",
            "status",
            "category",
            "owner",
            "effectiveness",
            "last_assessed_at",
            "assessment_note"
        ]
    );
}

#[test]
fn returns_controls_in_a_deterministic_order_and_remains_read_only() {
    let state = control_read_state();
    let before_count: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .expect("عدد التدقيق يجب أن يقرأ");

    let first = load_controls(&state).expect("القراءة الأولى يجب أن تنجح");
    let second = load_controls(&state).expect("القراءة الثانية يجب أن تنجح");
    let after_count: i64 = state
        .connection
        .lock()
        .expect("القفل يجب أن يتاح")
        .query_row("SELECT COUNT(*) FROM audit_events", [], |row| row.get(0))
        .expect("عدد التدقيق يجب أن يقرأ");

    assert_eq!(
        first
            .items
            .iter()
            .map(|control| control.id.as_str())
            .collect::<Vec<_>>(),
        second
            .items
            .iter()
            .map(|control| control.id.as_str())
            .collect::<Vec<_>>(),
    );
    assert_eq!(before_count, after_count);
}

// يثبت أن منح الدور الديمو لم يمنح كتابة الضوابط للمراجِع ولا منح control:assess للطالب:
// هذا فحص سياسة إقلاع حقيقي لا فحص وحدة.
#[test]
fn demo_role_grants_split_control_write_from_control_assessment() {
    let connection = Connection::open_in_memory().expect("قاعدة اختبار الذاكرة يجب أن تفتح");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("تفعيل مفاتيح الأجنبة يجب أن ينجح");
    crate::infrastructure::database::initialize_demo_database(&connection)
        .expect("تهيئة الديمو يجب أن تنجح");

    for permission_id in [
        "permission_control_create",
        "permission_control_update",
        "permission_control_disable",
        "permission_control_link",
    ] {
        let requester_has: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM role_permissions WHERE role_id = 'role_demo_requester' AND permission_id = ?1)",
                [permission_id],
                |row| row.get(0),
            )
            .expect("فحص منح الطالب يجب أن ينجح");
        let reviewer_has: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM role_permissions WHERE role_id = 'role_demo_tenant_admin' AND permission_id = ?1)",
                [permission_id],
                |row| row.get(0),
            )
            .expect("فحص منح المراجِع يجب أن ينجح");
        assert!(requester_has, "الطالب يجب أن يملك {permission_id}");
        assert!(!reviewer_has, "المراجِع يجب ألا يملك {permission_id}");
    }

    let requester_assess: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM role_permissions WHERE role_id = 'role_demo_requester' AND permission_id = 'permission_control_assess')",
            [],
            |row| row.get(0),
        )
        .expect("فحص تقييم الطالب يجب أن ينجح");
    let reviewer_assess: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM role_permissions WHERE role_id = 'role_demo_tenant_admin' AND permission_id = 'permission_control_assess')",
            [],
            |row| row.get(0),
        )
        .expect("فحص تقييم المراجِع يجب أن ينجح");
    assert!(!requester_assess, "الطالب يجب ألا يقيّم الفعالية");
    assert!(reviewer_assess, "المراجِع يقيّم الفعالية");

    // 0026 / UAT-04: قرار حالة التحقق من الدليل فعل مراجعة، فانتقلت
    // `evidence:update` إلى المراجِع وسُحبت من الطالب نهائياً.
    let evidence_update_for_requester: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM role_permissions WHERE role_id = 'role_demo_requester' AND permission_id = 'permission_evidence_update')",
            [],
            |row| row.get(0),
        )
        .expect("فحص منح evidence:update للطالب يجب أن ينجح");
    let evidence_update_for_reviewer: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM role_permissions WHERE role_id = 'role_demo_tenant_admin' AND permission_id = 'permission_evidence_update')",
            [],
            |row| row.get(0),
        )
        .expect("فحص منح evidence:update للمراجِع يجب أن ينجح");
    assert!(
        !evidence_update_for_requester,
        "منفّذ العمل يجب ألا يقرر حالة التحقق من دليله"
    );
    assert!(
        evidence_update_for_reviewer,
        "جهة المراجعة هي من تقرر حالة التحقق من الدليل"
    );
    // الديمو لا يزال متسقاً بعد إضافة أعمدة الضوابط.
    let violations: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .expect("فحص المفاتيح يجب أن ينجح");
    assert_eq!(violations, 0);
}
