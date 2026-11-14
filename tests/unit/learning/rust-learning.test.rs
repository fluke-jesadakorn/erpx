//! Unit policy for the native Learning owner: current actor scope, reviewed
//! course catalogue, eligible assignment, learner evidence, certification and
//! revocation. No database or HTTP effect is reachable here.
use erpx_server_learning::public::eligibility_facts::{
    BenefitConsumptionActor, BenefitConsumptionPlan, BenefitConsumptionRequest,
    EligibilityCertificateRow, EligibilityConsumptionRow, EligibilityQuery, EligibilityScope,
    MAX_ELIGIBILITY_CERTIFICATES, MAX_ELIGIBILITY_CONSUMPTIONS, PreparedBenefitConsumption,
    benefit_consume_identity, benefit_consumption_plan, certificate_source_json,
    eligibility_facts_json, learning_benefit_continuation, require_certificate_current,
    select_benefit_certificate,
};
use erpx_server_learning::public::benefit_learning::{
    PgBenefitLearning, benefit_consumption_request, benefit_learning_identity,
};
use erpx_server_benefits::public::command::{
    BenefitLearningRequest, BenefitsLearningPort, PreparedBenefitLearning,
};
use erpx_server_learning::public::learning::{
    AssignmentAction, AssignmentChange, AssignmentRequest, AssignmentRow, Attendance,
    CertificateRevocation, CertificateRow, CourseAction, CourseChange, CourseRequest, CourseRow,
    EvidenceManifestItem, HandoffRow, LearningActor, PlacementRow,
    PreparedLearningEvidence, assignment_change_plan, assignment_create_plan,
    assignment_evidence_request, certificate_revoke_plan, course_change_plan, course_create_plan,
    utc_date, valid_date,
};
use erpx_server_shared::public::{
    AssignmentId, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis, UserId,
};

const NOW: i64 = 1_800_000_000_000;

fn actor_with(
    user: &str,
    branch: Option<&str>,
    department: Option<&str>,
    permissions: &[&str],
    modules: &[&str],
) -> LearningActor {
    LearningActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: department.map(|value| RecordId::new(value).unwrap()),
        assignment_id: AssignmentId::new(format!("assignment-{user}")).unwrap(),
        role: Some("role".into()),
        authority_revision: Some(3),
        assignment_version: Some(1),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        modules: modules.iter().map(|value| (*value).to_owned()).collect(),
        expires_at: NOW + 60_000,
    }
}

fn learner() -> LearningActor {
    actor_with(
        "learner",
        None,
        None,
        &["records.read", "records.write"],
        &["learning"],
    )
}

fn reviewer() -> LearningActor {
    actor_with(
        "reviewer",
        None,
        None,
        &["records.read", "records.write", "records.review"],
        &["learning"],
    )
}

fn course(status: &str, required_for: Option<&str>) -> CourseRow {
    CourseRow {
        id: "course-1".into(),
        company_id: "company".into(),
        title: "Safe equipment operation".into(),
        description: "Practical operating and emergency-stop procedure.".into(),
        provider: "Internal academy".into(),
        duration_minutes: 90,
        required_for: required_for.map(str::to_owned),
        status: status.into(),
        created_by: "author".into(),
        reviewed_by: (status != "draft").then(|| "reviewer".into()),
        reviewed_at: (status != "draft").then_some(NOW - 10),
        version: if status == "draft" { 1 } else { 2 },
        created_at: NOW - 100,
        updated_at: NOW - 10,
    }
}

fn assignment(status: &str, eligible: bool) -> AssignmentRow {
    AssignmentRow {
        id: "assignment-1".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        department_id: Some("department".into()),
        course_id: "course-1".into(),
        course_version: 2,
        learner_id: "learner".into(),
        due_date: "2026-11-30".into(),
        status: status.into(),
        attendance: None,
        score_bps: None,
        result_evidence: None,
        completion_reason: None,
        created_by: "reviewer".into(),
        version: if status == "assigned" { 1 } else { 2 },
        created_at: NOW - 100,
        updated_at: NOW - 10,
        course_title: "Safe equipment operation".into(),
        required_for: Some("equipment_operator".into()),
        person_id: Some("person-learner".into()),
        learner_name: Some("Learner".into()),
        learner_eligible: eligible,
    }
}

fn certificate(revocation: Option<&str>) -> CertificateRow {
    CertificateRow {
        id: "learning-certificate-key-1".into(),
        handoff_id: "learning-eligibility-key-1".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        department_id: Some("department".into()),
        person_id: "person-learner".into(),
        assignment_id: "assignment-1".into(),
        source_assignment_version: 3,
        eligibility_code: "equipment_operator".into(),
        valid_from: "2026-11-04".into(),
        expires_on: Some("2028-11-04".into()),
        completion_kind: "completed".into(),
        evidence_ref: "Provider result and observed practical check".into(),
        issued_by: "learner".into(),
        created_at: NOW,
        revocation_id: revocation.map(str::to_owned),
    }
}

fn eligibility_query() -> EligibilityQuery {
    EligibilityQuery {
        person_id: "person-learner".into(),
        eligibility_code: "equipment_operator".into(),
        effective_date: "2026-11-04".into(),
    }
}

fn eligibility_row() -> EligibilityCertificateRow {
    EligibilityCertificateRow {
        certificate: certificate(None),
        handoff: HandoffRow {
            id: "learning-eligibility-key-1".into(),
            assignment_id: "assignment-1".into(),
            company_id: "company".into(),
            person_id: "person-learner".into(),
            eligibility_code: "equipment_operator".into(),
            source_assignment_version: 3,
            status: "open".into(),
            created_at: NOW,
        },
        assignment_version: 3,
        assignment_status: "completed".into(),
        person_status: "active".into(),
        person_merged_into_id: None,
        consumptions: Vec::new(),
    }
}

fn complete_change() -> AssignmentChange {
    AssignmentChange {
        action: AssignmentAction::Complete,
        expected_version: 2,
        reason: "Course and assessment completed".into(),
        attendance: Some(Attendance::Attended),
        score_bps: Some(9_500),
        evidence: Some("Provider result SIM-9500 and observed practical check".into()),
        certificate_valid_from: Some("2026-11-04".into()),
        certificate_expires_on: Some("2028-11-04".into()),
        attachment_set_ids: Vec::new(),
    }
}

#[test]
fn actor_context_module_permissions_and_selected_scope_are_exact() {
    let learner = learner();
    assert!(learner.require_read(NOW).is_ok());
    assert!(learner.require_write(NOW).is_ok());
    assert_eq!(
        learner.require_review(NOW).unwrap_err().code,
        "LEARNING_REVIEW_REQUIRED"
    );
    let expiry = LearningActor {
        expires_at: NOW,
        ..learner.clone()
    };
    assert_eq!(
        expiry.require_read(NOW).unwrap_err().code,
        "WORK_CONTEXT_CHANGED"
    );
    let no_module = LearningActor {
        modules: vec!["sales".into()],
        ..learner.clone()
    };
    assert_eq!(
        no_module.require_read(NOW).unwrap_err().code,
        "MODULE_FORBIDDEN"
    );
    let no_read = LearningActor {
        permissions: vec!["records.write".into()],
        ..learner.clone()
    };
    assert_eq!(
        no_read.require_read(NOW).unwrap_err().kind,
        FailureKind::Forbidden
    );
    assert!(learner.in_scope("company", Some("branch"), Some("department")));
    let branch = reviewer();
    let branch = LearningActor {
        branch_id: Some(RecordId::new("branch").unwrap()),
        ..branch
    };
    assert!(branch.in_scope("company", Some("branch"), Some("department")));
    assert!(!branch.in_scope("company", Some("other"), Some("department")));
    assert!(!branch.in_scope("foreign", Some("branch"), Some("department")));
    assert!(
        branch
            .authority_json()
            .contains("\"modules\":[\"learning\"]")
    );
}

#[test]
fn calendar_dates_and_utc_defaults_are_bounded() {
    assert!(valid_date("2024-02-29"));
    assert!(valid_date("2026-11-04"));
    assert!(!valid_date("2023-02-29"));
    assert!(!valid_date("2026-02-30"));
    assert!(!valid_date("2026-13-01"));
    assert!(!valid_date("2026-1-01"));
    assert!(!valid_date("2026-11-04T00:00:00Z"));
    assert_eq!(utc_date(0), "1970-01-01");
    assert_eq!(utc_date(86_400_000), "1970-01-02");
}

#[test]
fn course_text_limits_count_code_points_for_request_and_utf16_for_readback() {
    let input = CourseRequest {
        title: "ก".repeat(160),
        description: "คำอธิบาย".into(),
        provider: "ทีม".into(),
        duration_minutes: 90,
        required_for: Some("ช่าง".into()),
    };
    let valid = input.validate().unwrap();
    assert_eq!(valid.title, "ก".repeat(160));
    let mut persisted = course("draft", Some("ช่าง"));
    persisted.title = valid.title.clone();
    persisted.description = valid.description.clone();
    persisted.provider = valid.provider.clone();
    persisted.validate_source().unwrap();
    assert!(CourseRequest { title: "😀".repeat(160), ..input.clone() }.validate().is_ok());
    assert!(CourseRequest { title: "😀".repeat(161), ..input.clone() }.validate().is_err());
    assert!(CourseRequest { title: "ก".repeat(161), ..input.clone() }.validate().is_err());
    assert!(CourseRequest { title: "🙂x".into(), ..input.clone() }.validate().is_err());
    let mut utf16_row = course("draft", Some("ช่าง"));
    utf16_row.title = "😀".repeat(80);
    utf16_row.description = valid.description.clone();
    utf16_row.provider = valid.provider.clone();
    utf16_row.validate_source().unwrap();
    utf16_row.title.push('ก');
    assert!(utf16_row.validate_source().is_err());
    utf16_row.title = "😀".repeat(160);
    assert!(utf16_row.validate_source().is_err());
}

#[test]
fn learning_field_boundaries_and_js_whitespace_are_preserved() {
    let base = CourseRequest { title: "\u{feff}หลักสูตร\u{feff}".into(), description: "ก".repeat(3_000), provider: "😀".repeat(80), duration_minutes: 1_000_001, required_for: Some("😀".repeat(60)) };
    let valid = base.validate().unwrap();
    assert_eq!(valid.title, "หลักสูตร");
    assert!(CourseRequest { description: "ก".repeat(3_001), ..base.clone() }.validate().is_err());
    assert!(CourseRequest { description: "😀".repeat(3_000), ..base.clone() }.validate().is_ok());
    assert!(CourseRequest { description: "😀".repeat(3_001), ..base.clone() }.validate().is_err());
    assert!(CourseRequest { provider: "😀".repeat(160), ..base.clone() }.validate().is_ok());
    assert!(CourseRequest { provider: "😀".repeat(161), ..base.clone() }.validate().is_err());
    assert!(CourseRequest { required_for: Some("😀".repeat(120)), ..base.clone() }.validate().is_ok());
    assert!(CourseRequest { required_for: Some("😀".repeat(121)), ..base.clone() }.validate().is_err());
    assert!(CourseRequest { duration_minutes: 9_007_199_254_740_992, ..base.clone() }.validate().is_err());
    assert!(CourseRequest { title: "🙂x".into(), ..base.clone() }.validate().is_err());
    let mut row = course("draft", Some(&"😀".repeat(60)));
    row.title = "😀".repeat(80);
    row.description = valid.description;
    row.provider = valid.provider;
    row.duration_minutes = valid.duration_minutes;
    row.validate_source().unwrap();
    row.title.push('ก');
    assert!(row.validate_source().is_err());
    assert!(erpx_server_learning::public::learning::normalize_reason(&"ก".repeat(3_000)).is_ok());
    assert!(erpx_server_learning::public::learning::normalize_reason(&"ก".repeat(3_001)).is_err());
    assert!(erpx_server_learning::public::learning::normalize_reason(&"😀".repeat(3_000)).is_ok());
    assert!(erpx_server_learning::public::learning::normalize_reason(&"😀".repeat(3_001)).is_err());
    assert!(erpx_server_learning::public::learning::normalize_reason("🙂x").is_err());
    let revoke = CertificateRevocation { expected_assignment_version: 1, reason: format!("\u{feff}{}\u{feff}", "😀".repeat(1_000)) };
    assert_eq!(revoke.validate().unwrap().reason, "😀".repeat(1_000));
    assert!(CertificateRevocation { reason: "😀".repeat(2_000), ..revoke.clone() }.validate().is_ok());
    assert!(CertificateRevocation { reason: "😀".repeat(2_001), ..revoke }.validate().is_err());
    assert_eq!(CourseRequest { title: "\u{0085}abc".into(), ..base.clone() }.validate().unwrap().title, "\u{0085}abc");
    assert!(CourseRequest { title: "ab\0c".into(), ..base }.validate().is_err());
}

#[test]
fn course_request_trims_optional_code_and_rejects_weak_fields() {
    let input = CourseRequest {
        title: "  Safe equipment operation  ".into(),
        description: "  Practical operating and emergency-stop procedure.  ".into(),
        provider: "  Internal academy  ".into(),
        duration_minutes: 90,
        required_for: Some("  equipment_operator  ".into()),
    };
    let valid = input.validate().unwrap();
    assert_eq!(valid.title, "Safe equipment operation");
    assert_eq!(valid.provider, "Internal academy");
    assert_eq!(valid.required_for.as_deref(), Some("equipment_operator"));
    assert!(
        CourseRequest {
            required_for: Some("  ".into()),
            ..input.clone()
        }
        .validate()
        .is_err()
    );
    assert!(
        CourseRequest {
            title: "x".into(),
            ..input.clone()
        }
        .validate()
        .is_err()
    );
    assert!(
        CourseRequest {
            duration_minutes: 0,
            ..input.clone()
        }
        .validate()
        .is_err()
    );
    assert!(
        CourseRequest {
            description: "bad\u{0}description".into(),
            ..input
        }
        .validate()
        .is_err()
    );
}

#[test]
fn course_review_requires_an_independent_reviewer_version_and_status() {
    let draft = course("draft", Some("equipment_operator"));
    let raw_change = CourseChange {
        action: CourseAction::Publish,
        expected_version: 1,
        reason: "Independent reviewer checked content and audience".into(),
    };
    let change = raw_change.clone().validate().unwrap();
    let plan = course_change_plan(&reviewer(), &draft, &change, "course-review-1", NOW).unwrap();
    assert_eq!(plan.version, 2);
    assert_eq!(plan.status, "published");
    assert_eq!(plan.reviewed_by, "reviewer");
    assert_eq!(plan.event_id, "learning-course-event-course-review-1");

    let own = LearningActor {
        user_id: UserId::new("author").unwrap(),
        ..reviewer()
    };
    assert_eq!(
        course_change_plan(&own, &draft, &change, "course-review-2", NOW)
            .unwrap_err()
            .code,
        "LEARNING_REVIEW_REQUIRED"
    );
    let stale = CourseChange {
        expected_version: 2,
        ..raw_change
    }
    .validate()
    .unwrap();
    assert_eq!(
        course_change_plan(&reviewer(), &draft, &stale, "course-review-3", NOW)
            .unwrap_err()
            .code,
        "LEARNING_COURSE_CHANGED"
    );
    let retire = CourseChange {
        action: CourseAction::Retire,
        expected_version: 1,
        reason: "Retire the reviewed course revision".into(),
    }
    .validate()
    .unwrap();
    assert_eq!(
        course_change_plan(&reviewer(), &draft, &retire, "course-review-4", NOW)
            .unwrap_err()
            .code,
        "LEARNING_COURSE_CHANGED"
    );
    let published = course("published", Some("equipment_operator"));
    let retire_published = CourseChange {
        action: CourseAction::Retire,
        expected_version: 2,
        reason: "Retire the reviewed course revision".into(),
    }
    .validate()
    .unwrap();
    assert_eq!(
        course_change_plan(
            &reviewer(),
            &published,
            &retire_published,
            "course-review-5",
            NOW
        )
        .unwrap()
        .status,
        "retired"
    );
    let missing_review = CourseRow {
        reviewed_by: None,
        reviewed_at: None,
        ..published
    };
    assert_eq!(
        missing_review.validate_source().unwrap_err().code,
        "LEARNING_SOURCE_INVALID"
    );
}

#[test]
fn assignment_input_and_action_shape_are_strict() {
    let input = AssignmentRequest {
        course_id: "course-1".into(),
        course_version: 2,
        learner_id: "learner".into(),
        due_date: "2026-11-30".into(),
        reason: "Required before operating production equipment".into(),
    };
    assert_eq!(input.validate().unwrap().due_date, "2026-11-30");
    assert!(AssignmentRequest { due_date: " 2026-11-30 ".into(), ..input.clone() }.validate().is_err());
    assert_eq!(erpx_server_learning::public::learning::normalize_reason("\u{0085}abc\nxyz").unwrap(), "\u{0085}abc\nxyz");
    assert!(erpx_server_learning::public::learning::normalize_reason("abc\0xyz").is_err());
    assert!(
        AssignmentRequest {
            due_date: "2026-02-30".into(),
            ..input.clone()
        }
        .validate()
        .is_err()
    );
    assert!(
        AssignmentRequest {
            course_version: 0,
            ..input
        }
        .validate()
        .is_err()
    );

    let complete = complete_change();
    let valid = complete.validate().unwrap();
    assert_eq!(valid.attendance, Some(Attendance::Attended));
    assert_eq!(valid.score_bps, Some(9_500));
    assert!(complete.attachment_set_ids.is_empty());
    let mut attached = complete.clone();
    attached.attachment_set_ids = vec!["set-1".into()];
    assert_eq!(
        attached.validate().unwrap().attachment_set_ids,
        vec!["set-1".to_owned()]
    );
    let mut duplicated = attached.clone();
    duplicated.attachment_set_ids = vec!["set-1".into(), "set-1".into()];
    assert!(duplicated.validate().is_ok());
    let mut too_many = complete.clone();
    too_many.attachment_set_ids = (0..21).map(|index| format!("set-{index}")).collect();
    assert!(too_many.validate().is_err());
    let mut blank = complete.clone();
    blank.attachment_set_ids = vec![String::new()];
    assert!(blank.validate().is_err());
    let mut missing = complete.clone();
    missing.attendance = None;
    assert!(missing.validate().is_err());
    let mut score = complete.clone();
    score.score_bps = Some(10_001);
    assert!(score.validate().is_err());
    let start = AssignmentChange {
        action: AssignmentAction::Start,
        expected_version: 1,
        reason: "Learner opened the assigned course".into(),
        attendance: None,
        score_bps: None,
        evidence: None,
        certificate_valid_from: None,
        certificate_expires_on: None,
        attachment_set_ids: Vec::new(),
    };
    assert!(start.validate().is_ok());
    let mut weird = start.clone();
    weird.score_bps = Some(10);
    assert!(weird.validate().is_err());
    let mut start_with_evidence = start.clone();
    start_with_evidence.attachment_set_ids = vec!["set-1".into()];
    assert!(start_with_evidence.validate().is_err());
    let waive = AssignmentChange {
        action: AssignmentAction::Waive,
        expected_version: 1,
        reason: "Equivalent external certification verified".into(),
        attendance: None,
        score_bps: None,
        evidence: None,
        certificate_valid_from: None,
        certificate_expires_on: None,
        attachment_set_ids: Vec::new(),
    };
    assert!(waive.validate().is_ok());
}

#[test]
fn assignment_creation_requires_published_course_and_eligible_placement() {
    let published = course("published", Some("equipment_operator"));
    let placement = PlacementRow {
        person_id: "person-learner".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        eligible: true,
    };
    let input = AssignmentRequest {
        course_id: "course-1".into(),
        course_version: 2,
        learner_id: "learner".into(),
        due_date: "2026-11-30".into(),
        reason: "Required before operating production equipment".into(),
    };
    let raw_input = input.clone();
    let input = input.validate().unwrap();
    let plan = assignment_create_plan(
        &reviewer(),
        "assignment-key-1",
        &input,
        &published,
        &placement,
        NOW,
    )
    .unwrap();
    assert_eq!(plan.id, "learning-assignment-assignment-key-1");
    assert_eq!(plan.branch_id, "branch");
    assert_eq!(
        plan.reason,
        "Required before operating production equipment"
    );

    let stale = AssignmentRequest {
        course_version: 3,
        ..raw_input
    }
    .validate()
    .unwrap();
    assert_eq!(
        assignment_create_plan(
            &reviewer(),
            "assignment-key-2",
            &stale,
            &published,
            &placement,
            NOW
        )
        .unwrap_err()
        .code,
        "LEARNING_SOURCE_CHANGED"
    );
    let draft = course("draft", Some("equipment_operator"));
    assert!(
        assignment_create_plan(
            &reviewer(),
            "assignment-key-3",
            &input,
            &draft,
            &placement,
            NOW
        )
        .is_err()
    );
    let ineligible = PlacementRow {
        eligible: false,
        ..placement.clone()
    };
    assert_eq!(
        assignment_create_plan(
            &reviewer(),
            "assignment-key-4",
            &input,
            &published,
            &ineligible,
            NOW
        )
        .unwrap_err()
        .code,
        "LEARNING_LEARNER_INELIGIBLE"
    );
    let no_branch = PlacementRow {
        branch_id: None,
        ..placement.clone()
    };
    assert_eq!(
        assignment_create_plan(
            &reviewer(),
            "assignment-key-5",
            &input,
            &published,
            &no_branch,
            NOW
        )
        .unwrap_err()
        .code,
        "LEARNING_LEARNER_INELIGIBLE"
    );
    let branch = LearningActor {
        branch_id: Some(RecordId::new("other").unwrap()),
        ..reviewer()
    };
    assert_eq!(
        assignment_create_plan(
            &branch,
            "assignment-key-6",
            &input,
            &published,
            &placement,
            NOW
        )
        .unwrap_err()
        .code,
        "LEARNING_LEARNER_INELIGIBLE"
    );
}

#[test]
fn learner_completion_issues_versioned_handoff_and_certificate() {
    let row = assignment("in_progress", true);
    let change = complete_change().validate().unwrap();
    let plan =
        assignment_change_plan(&learner(), &row, &change, None, "assignment-complete-1", NOW).unwrap();
    assert_eq!(plan.status, "completed");
    assert_eq!(plan.attendance.as_deref(), Some("attended"));
    assert_eq!(plan.score_bps, Some(9_500));
    assert_eq!(
        plan.result_evidence.as_deref(),
        Some("Provider result SIM-9500 and observed practical check")
    );
    assert_eq!(
        plan.completion_reason.as_deref(),
        Some("Course and assessment completed")
    );
    let certificate = plan.certificate.unwrap();
    assert_eq!(
        certificate.handoff_id,
        "learning-eligibility-assignment-complete-1"
    );
    assert_eq!(
        certificate.certificate_id,
        "learning-certificate-assignment-complete-1"
    );
    assert_eq!(certificate.person_id, "person-learner");
    assert_eq!(certificate.source_assignment_version, 3);
    assert_eq!(certificate.completion_kind, "completed");
    assert_eq!(certificate.valid_from, "2026-11-04");

    let mut bad_dates = complete_change();
    bad_dates.certificate_expires_on = Some("2025-01-01".into());
    let bad_dates = bad_dates.validate().unwrap();
    assert_eq!(
        assignment_change_plan(&learner(), &row, &bad_dates, None, "assignment-complete-2", NOW)
            .unwrap_err()
            .code,
        "LEARNING_CERTIFICATE_DATES_INVALID"
    );
    let outsider = LearningActor {
        user_id: UserId::new("outsider").unwrap(),
        ..learner()
    };
    assert_eq!(
        assignment_change_plan(&outsider, &row, &change, None, "assignment-complete-3", NOW)
            .unwrap_err()
            .code,
        "LEARNING_LEARNER_REQUIRED"
    );
    let ineligible = assignment("in_progress", false);
    assert_eq!(
        assignment_change_plan(
            &learner(),
            &ineligible,
            &change,
            None,
            "assignment-complete-4",
            NOW
        )
        .unwrap_err()
        .code,
        "LEARNING_LEARNER_REQUIRED"
    );
    let assigned = assignment("assigned", true);
    assert_eq!(
        assignment_change_plan(&learner(), &assigned, &change, None, "assignment-complete-5", NOW)
            .unwrap_err()
            .code,
        "LEARNING_STATE_CHANGED"
    );
}

#[test]
fn reviewer_waiver_and_cancellation_preserve_source_results() {
    let row = AssignmentRow {
        attendance: Some("attended".into()),
        score_bps: Some(8_800),
        result_evidence: Some("Original provider result".into()),
        version: 2,
        ..assignment("in_progress", false)
    };
    let waive = AssignmentChange {
        action: AssignmentAction::Waive,
        expected_version: 2,
        reason: "Equivalent external certification verified".into(),
        attendance: None,
        score_bps: None,
        evidence: None,
        certificate_valid_from: None,
        certificate_expires_on: None,
        attachment_set_ids: Vec::new(),
    }
    .validate()
    .unwrap();
    let plan =
        assignment_change_plan(&reviewer(), &row, &waive, None, "assignment-waive-1", NOW).unwrap();
    assert_eq!(plan.status, "waived");
    assert_eq!(plan.attendance.as_deref(), Some("attended"));
    assert_eq!(plan.score_bps, Some(8_800));
    assert_eq!(
        plan.result_evidence.as_deref(),
        Some("Original provider result")
    );
    assert_eq!(plan.certificate.as_ref().unwrap().completion_kind, "waived");
    assert_eq!(
        plan.certificate.as_ref().unwrap().evidence_ref,
        "Equivalent external certification verified"
    );
    let cancel = AssignmentChange {
        action: AssignmentAction::Cancel,
        expected_version: 2,
        reason: "Learner left the Company before completion".into(),
        attendance: None,
        score_bps: None,
        evidence: None,
        certificate_valid_from: None,
        certificate_expires_on: None,
        attachment_set_ids: Vec::new(),
    }
    .validate()
    .unwrap();
    let plan =
        assignment_change_plan(&reviewer(), &row, &cancel, None, "assignment-cancel-1", NOW).unwrap();
    assert_eq!(plan.status, "cancelled");
    assert!(plan.certificate.is_none());
    assert!(plan.completion_reason.is_none());
    let learner = LearningActor {
        permissions: vec!["records.read".into(), "records.write".into()],
        ..reviewer()
    };
    assert_eq!(
        assignment_change_plan(&learner, &row, &waive, None, "assignment-waive-2", NOW)
            .unwrap_err()
            .code,
        "LEARNING_REVIEW_REQUIRED"
    );
}

#[test]
fn certificate_revocation_requires_scope_current_version_and_no_prior_revocation() {
    let raw_input = CertificateRevocation {
        expected_assignment_version: 3,
        reason: "Independent owner retires this retained certificate".into(),
    };
    let input = raw_input.clone().validate().unwrap();
    let plan = certificate_revoke_plan(
        &reviewer(),
        &certificate(None),
        &input,
        "certificate-revoke-1",
        NOW,
    )
    .unwrap();
    assert_eq!(plan.certificate_id, "learning-certificate-key-1");
    assert_eq!(plan.assignment_id, "assignment-1");
    assert_eq!(plan.expected_assignment_version, 3);
    assert_eq!(plan.id, "certificate-revoke-certificate-revoke-1");
    let stale = CertificateRevocation {
        expected_assignment_version: 2,
        ..raw_input
    }
    .validate()
    .unwrap();
    assert_eq!(
        certificate_revoke_plan(
            &reviewer(),
            &certificate(None),
            &stale,
            "certificate-revoke-2",
            NOW
        )
        .unwrap_err()
        .code,
        "LEARNING_CERTIFICATE_NOT_FOUND"
    );
    assert_eq!(
        certificate_revoke_plan(
            &reviewer(),
            &certificate(Some("certificate-revoke-prior")),
            &input,
            "certificate-revoke-3",
            NOW
        )
        .unwrap_err()
        .code,
        "LEARNING_CERTIFICATE_REVOKED"
    );
    let branch = LearningActor {
        branch_id: Some(RecordId::new("other").unwrap()),
        ..reviewer()
    };
    assert_eq!(
        certificate_revoke_plan(
            &branch,
            &certificate(None),
            &input,
            "certificate-revoke-4",
            NOW
        )
        .unwrap_err()
        .code,
        "LEARNING_CERTIFICATE_NOT_FOUND"
    );
    assert!(
        CertificateRevocation {
            expected_assignment_version: 3,
            reason: "short".into(),
        }
        .validate()
        .is_err()
    );
}

#[test]
fn completion_rows_require_terminal_reason_and_handoff_rows_require_open_status() {
    let completed = AssignmentRow {
        status: "completed".into(),
        completion_reason: None,
        ..assignment("completed", true)
    };
    assert_eq!(
        completed.validate_source().unwrap_err().code,
        "LEARNING_SOURCE_INVALID"
    );
    let mut row = assignment("completed", true);
    row.completion_reason = Some("Completed".into());
    assert!(row.validate_source().is_ok());
    assert_eq!(row.to_json()["learner_eligible"], 1);
    let course_plan = course_create_plan(
        &reviewer(),
        "course-create-1",
        &CourseRequest {
            title: "Native course".into(),
            description: "Native course description".into(),
            provider: "Native academy".into(),
            duration_minutes: 30,
            required_for: None,
        }
        .validate()
        .unwrap(),
        NOW,
    )
    .unwrap();
    assert_eq!(course_plan.id, "learning-course-course-create-1");
    assert_eq!(
        course_plan.event_id,
        "learning-course-event-course-create-1"
    );
}

#[test]
fn eligibility_code_keeps_utf16_and_control_identity_across_handoff() {
    let scope = EligibilityScope::new("company", Some("branch"), Some("department")).unwrap();
    for code in ["ช".repeat(120), "😀".repeat(60), "a\nb\u{0085}c".into()] {
        let query = EligibilityQuery { eligibility_code: code.clone(), ..eligibility_query() };
        let mut row = eligibility_row();
        row.certificate.eligibility_code = code.clone();
        row.handoff.eligibility_code = code.clone();
        let value = eligibility_facts_json(&scope, &query, vec![row.clone()]).unwrap();
        assert_eq!(value["eligible"], true);
        assert_eq!(value["eligibility_code"], code);
        row.handoff.eligibility_code.push('x');
        assert!(eligibility_facts_json(&scope, &query, vec![row]).is_err());
    }
    for code in [format!("{}ก", "😀".repeat(60)), "ab\0c".into(), "\u{feff}ab".into()] {
        assert!(EligibilityQuery { eligibility_code: code, ..eligibility_query() }.validate().is_err());
    }
}

#[test]
fn eligibility_facts_require_current_source_dates_scope_and_consumption_evidence() {
    let query = eligibility_query();
    let consumer_scope =
        EligibilityScope::new("company", Some("branch"), Some("department")).unwrap();
    assert_eq!(
        eligibility_facts_json(&consumer_scope, &query, vec![eligibility_row()]).unwrap()["eligible"],
        true
    );
    assert!(EligibilityScope::new("", None, None).is_err());
    assert!(EligibilityScope::new("company", None, Some("department")).is_err());
    let facts = eligibility_facts_json(
        &EligibilityScope::from_learning_actor(&learner()),
        &query,
        vec![eligibility_row()],
    )
    .unwrap();
    assert_eq!(facts["person_id"], "person-learner");
    assert_eq!(facts["eligibility_code"], "equipment_operator");
    assert_eq!(facts["eligible"], true);
    assert_eq!(facts["certificates"][0]["current"], true);
    assert_eq!(
        facts["certificates"][0]["handoff_source_assignment_version"],
        3
    );
    assert_eq!(facts["certificates"][0]["assignment_status"], "completed");
    assert_eq!(facts["certificates"][0]["person_status"], "active");

    for effective_date in ["2026-11-03", "2028-11-05"] {
        let outside = eligibility_facts_json(
            &EligibilityScope::from_learning_actor(&learner()),
            &EligibilityQuery {
                effective_date: effective_date.into(),
                ..query.clone()
            },
            vec![eligibility_row()],
        )
        .unwrap();
        assert_eq!(outside["eligible"], false, "{effective_date}");
        assert_eq!(outside["certificates"][0]["current"], false);
    }

    let mut revoked = eligibility_row();
    revoked.certificate.revocation_id = Some("revocation-1".into());
    assert_eq!(
        eligibility_facts_json(
            &EligibilityScope::from_learning_actor(&learner()),
            &query,
            vec![revoked]
        )
        .unwrap()["eligible"],
        false
    );

    let mut stale_assignment = eligibility_row();
    stale_assignment.assignment_version = 4;
    stale_assignment.certificate.source_assignment_version = 3;
    stale_assignment.handoff.source_assignment_version = 3;
    assert_eq!(
        eligibility_facts_json(
            &EligibilityScope::from_learning_actor(&learner()),
            &query,
            vec![stale_assignment]
        )
        .unwrap()["eligible"],
        false
    );

    let mut changed_completion = eligibility_row();
    changed_completion.assignment_status = "waived".into();
    assert_eq!(
        eligibility_facts_json(
            &EligibilityScope::from_learning_actor(&learner()),
            &query,
            vec![changed_completion]
        )
        .unwrap()["eligible"],
        false
    );

    let mut merged_person = eligibility_row();
    merged_person.person_status = "merged".into();
    assert_eq!(
        eligibility_facts_json(
            &EligibilityScope::from_learning_actor(&learner()),
            &query,
            vec![merged_person]
        )
        .unwrap()["eligible"],
        false
    );

    let branch = actor_with(
        "branch-reviewer",
        Some("other-branch"),
        Some("outside"),
        &["records.read"],
        &["learning"],
    );
    assert_eq!(
        eligibility_facts_json(
            &EligibilityScope::from_learning_actor(&branch),
            &query,
            vec![eligibility_row()],
        )
        .unwrap_err()
        .code,
        "LEARNING_ELIGIBILITY_NOT_FOUND"
    );

    assert_eq!(
        eligibility_facts_json(
            &EligibilityScope::from_learning_actor(&learner()),
            &query,
            vec![]
        )
        .unwrap_err()
        .code,
        "LEARNING_ELIGIBILITY_NOT_FOUND"
    );

    let duplicate = eligibility_row();
    assert_eq!(
        eligibility_facts_json(
            &EligibilityScope::from_learning_actor(&learner()),
            &query,
            vec![eligibility_row(), duplicate]
        )
        .unwrap_err()
        .code,
        "LEARNING_SOURCE_INVALID"
    );

    let mut foreign = eligibility_row();
    foreign.certificate.company_id = "foreign".into();
    assert_eq!(
        eligibility_facts_json(
            &EligibilityScope::from_learning_actor(&learner()),
            &query,
            vec![foreign]
        )
        .unwrap_err()
        .code,
        "LEARNING_SOURCE_INVALID"
    );

    let mut consumption = EligibilityConsumptionRow {
        id: "learning-benefit-1".into(),
        election_id: "election-1".into(),
        source_assignment_version: 3,
        eligibility_code: "equipment_operator".into(),
        source_digest: "a".repeat(64),
        purpose: "Benefits eligibility at election creation".into(),
        actor_id: "benefits-reviewer".into(),
        command_id: "benefit-election-1".into(),
        created_at: NOW,
    };
    let mut with_consumption = eligibility_row();
    with_consumption.consumptions = vec![consumption.clone()];
    let facts = eligibility_facts_json(
        &EligibilityScope::from_learning_actor(&learner()),
        &query,
        vec![with_consumption],
    )
    .unwrap();
    assert_eq!(
        facts["certificates"][0]["consumptions"][0]["election_id"],
        "election-1"
    );
    assert_eq!(facts["certificates"][0]["current"], true);
    consumption.source_digest = "short".into();
    let mut invalid = eligibility_row();
    invalid.consumptions = vec![consumption];
    assert_eq!(
        eligibility_facts_json(
            &EligibilityScope::from_learning_actor(&learner()),
            &query,
            vec![invalid]
        )
        .unwrap_err()
        .code,
        "LEARNING_SOURCE_INVALID"
    );

    let mut too_many = eligibility_row();
    too_many.consumptions = (0..101)
        .map(|index| EligibilityConsumptionRow {
            id: format!("learning-benefit-{index}"),
            election_id: format!("election-{index}"),
            source_assignment_version: 3,
            eligibility_code: "equipment_operator".into(),
            source_digest: "a".repeat(64),
            purpose: "Benefits eligibility at election creation".into(),
            actor_id: "benefits-reviewer".into(),
            command_id: format!("benefit-election-{index}"),
            created_at: NOW,
        })
        .collect();
    assert_eq!(
        eligibility_facts_json(
            &EligibilityScope::from_learning_actor(&learner()),
            &query,
            vec![too_many]
        )
        .unwrap_err()
        .code,
        "LEARNING_SOURCE_INVALID"
    );

    for invalid in [
        EligibilityQuery {
            person_id: String::new(),
            ..query.clone()
        },
        EligibilityQuery {
            eligibility_code: " equipment_operator".into(),
            ..query.clone()
        },
        EligibilityQuery {
            effective_date: "2026-02-30".into(),
            ..query.clone()
        },
    ] {
        assert_eq!(
            invalid.validate().unwrap_err().code,
            "LEARNING_REQUEST_INVALID"
        );
    }
}

#[test]
fn eligibility_consumption_digest_requires_lowercase_sha256_hex() {
    let scope = EligibilityScope::from_learning_actor(&learner());
    let query = eligibility_query();
    let consumption = |digest: &str| EligibilityConsumptionRow {
        id: "learning-benefit-1".into(),
        election_id: "election-1".into(),
        source_assignment_version: 3,
        eligibility_code: "equipment_operator".into(),
        source_digest: digest.into(),
        purpose: "Benefits eligibility at election creation".into(),
        actor_id: "benefits-reviewer".into(),
        command_id: "benefit-election-1".into(),
        created_at: NOW,
    };
    for digest in [
        "A".repeat(64),
        format!("{}F", "a".repeat(63)),
        format!("{}g", "a".repeat(63)),
        format!("{}A", "a".repeat(63)),
        "a".repeat(63),
        "a".repeat(65),
    ] {
        let mut row = eligibility_row();
        row.consumptions = vec![consumption(&digest)];
        assert_eq!(
            eligibility_facts_json(&scope, &query, vec![row])
                .unwrap_err()
                .code,
            "LEARNING_SOURCE_INVALID",
            "{digest}"
        );
    }
    let mut row = eligibility_row();
    row.consumptions = vec![consumption(&"f".repeat(64))];
    assert!(eligibility_facts_json(&scope, &query, vec![row]).is_ok());
}

#[test]
fn eligibility_facts_bound_certificate_and_consumption_rows() {
    let scope = EligibilityScope::from_learning_actor(&learner());
    let query = eligibility_query();
    let certificates = |count: usize| {
        (0..count)
            .map(|index| {
                let mut row = eligibility_row();
                row.certificate.id = format!("learning-certificate-{index}");
                row
            })
            .collect::<Vec<_>>()
    };
    let exact = eligibility_facts_json(
        &scope,
        &query,
        certificates(MAX_ELIGIBILITY_CERTIFICATES),
    )
    .unwrap();
    assert_eq!(
        exact["certificates"].as_array().unwrap().len(),
        MAX_ELIGIBILITY_CERTIFICATES
    );
    assert_eq!(
        eligibility_facts_json(
            &scope,
            &query,
            certificates(MAX_ELIGIBILITY_CERTIFICATES + 1)
        )
        .unwrap_err()
        .code,
        "LEARNING_SOURCE_INVALID"
    );

    let consumptions = |count: usize| {
        (0..count)
            .map(|index| EligibilityConsumptionRow {
                id: format!("learning-benefit-{index}"),
                election_id: format!("election-{index}"),
                source_assignment_version: 3,
                eligibility_code: "equipment_operator".into(),
                source_digest: "a".repeat(64),
                purpose: "Benefits eligibility at election creation".into(),
                actor_id: "benefits-reviewer".into(),
                command_id: format!("benefit-election-{index}"),
                created_at: NOW,
            })
            .collect::<Vec<_>>()
    };
    let mut row = eligibility_row();
    row.consumptions = consumptions(MAX_ELIGIBILITY_CONSUMPTIONS);
    assert!(eligibility_facts_json(&scope, &query, vec![row]).is_ok());
    let mut row = eligibility_row();
    row.consumptions = consumptions(MAX_ELIGIBILITY_CONSUMPTIONS + 1);
    assert_eq!(
        eligibility_facts_json(&scope, &query, vec![row])
            .unwrap_err()
            .code,
        "LEARNING_SOURCE_INVALID"
    );
}

fn consumption_request() -> BenefitConsumptionRequest {
    BenefitConsumptionRequest {
        company_id: "company".into(),
        person_id: "person-learner".into(),
        required_code: "equipment_operator".into(),
        effective_date: "2026-11-04".into(),
        election_id: "election-1".into(),
        command_id: "benefit-election-1".into(),
        today: "2026-11-04".into(),
        actor_id: "benefits-reviewer".into(),
    }
}

#[test]
fn benefit_consumption_selects_only_current_in_scope_certificates() {
    let scope = EligibilityScope::from_learning_actor(&learner());
    let request = consumption_request();
    let selected = select_benefit_certificate(&scope, &request, vec![eligibility_row()]).unwrap();
    assert_eq!(selected.certificate.id, "learning-certificate-key-1");

    let mut revoked = eligibility_row();
    revoked.certificate.revocation_id = Some("revocation-1".into());
    assert_eq!(
        select_benefit_certificate(&scope, &request, vec![revoked])
            .unwrap_err()
            .code,
        "BENEFIT_LEARNING_ELIGIBILITY_REQUIRED"
    );

    let mut expired = eligibility_row();
    expired.certificate.valid_from = "2025-11-04".into();
    expired.certificate.expires_on = Some("2026-10-01".into());
    assert_eq!(
        select_benefit_certificate(&scope, &request, vec![expired.clone()])
            .unwrap_err()
            .code,
        "BENEFIT_LEARNING_ELIGIBILITY_REQUIRED"
    );
    let expired_id = expired.certificate.id.clone();
    assert_eq!(
        require_certificate_current(&scope, &request, &expired_id, vec![expired])
            .unwrap_err()
            .code,
        "BENEFIT_LEARNING_CERTIFICATE_EXPIRED"
    );

    let mut future = eligibility_row();
    future.certificate.valid_from = "2026-12-01".into();
    assert_eq!(
        select_benefit_certificate(&scope, &request, vec![future])
            .unwrap_err()
            .code,
        "BENEFIT_LEARNING_ELIGIBILITY_REQUIRED"
    );

    let mut late_today = request.clone();
    late_today.today = "2028-11-05".into();
    assert_eq!(
        select_benefit_certificate(&scope, &late_today, vec![eligibility_row()])
            .unwrap_err()
            .code,
        "BENEFIT_LEARNING_ELIGIBILITY_REQUIRED"
    );

    let mut stale_assignment = eligibility_row();
    stale_assignment.assignment_version = 4;
    assert_eq!(
        select_benefit_certificate(&scope, &request, vec![stale_assignment])
            .unwrap_err()
            .code,
        "BENEFIT_LEARNING_ELIGIBILITY_REQUIRED"
    );

    assert_eq!(
        select_benefit_certificate(&scope, &request, Vec::new())
            .unwrap_err()
            .code,
        "BENEFIT_LEARNING_ELIGIBILITY_REQUIRED"
    );
}

#[test]
fn benefit_consumption_plan_binds_exact_certificate_and_digest() {
    let request = consumption_request().validate().unwrap();
    let certificate = certificate(None);
    let digest = "a".repeat(64);
    let plan = benefit_consumption_plan(&request, &certificate, digest.clone(), NOW).unwrap();
    assert_eq!(plan.id, "learning-benefit-benefit-election-1");
    assert_eq!(plan.handoff_id, certificate.handoff_id);
    assert_eq!(plan.certificate_id, certificate.id);
    assert_eq!(plan.election_id, "election-1");
    assert_eq!(plan.company_id, "company");
    assert_eq!(plan.person_id, "person-learner");
    assert_eq!(plan.source_assignment_version, 3);
    assert_eq!(plan.source_digest, digest);
    assert_eq!(plan.command_id, "benefit-election-1");
    assert_eq!(plan.created_at, NOW);
    let receipt = plan.receipt();
    assert!(receipt.matches_source(&plan));
    assert!(receipt.matches_command(&plan));
    assert_eq!(
        receipt.to_json()["sourceAssignmentVersion"],
        serde_json::json!(3)
    );

    assert_eq!(
        benefit_consumption_plan(&request, &certificate, "A".repeat(64), NOW)
            .unwrap_err()
            .code,
        "BENEFIT_LEARNING_CERTIFICATE_EXPIRED"
    );
    let mut revoked = certificate.clone();
    revoked.revocation_id = Some("revocation-1".into());
    assert_eq!(
        benefit_consumption_plan(&request, &revoked, "a".repeat(64), NOW)
            .unwrap_err()
            .code,
        "BENEFIT_LEARNING_CERTIFICATE_EXPIRED"
    );
    let mut foreign = certificate.clone();
    foreign.company_id = "foreign".into();
    assert_eq!(
        benefit_consumption_plan(&request, &foreign, "a".repeat(64), NOW)
            .unwrap_err()
            .code,
        "BENEFIT_LEARNING_CERTIFICATE_EXPIRED"
    );
    assert!(request.validate().is_ok());
    for invalid in [
        BenefitConsumptionRequest {
            person_id: String::new(),
            ..request.clone()
        },
        BenefitConsumptionRequest {
            election_id: String::new(),
            ..request.clone()
        },
        BenefitConsumptionRequest {
            required_code: " equipment_operator".into(),
            ..request.clone()
        },
        BenefitConsumptionRequest {
            effective_date: "2026-02-30".into(),
            ..request.clone()
        },
        BenefitConsumptionRequest {
            command_id: "short".into(),
            ..request.clone()
        },
        BenefitConsumptionRequest {
            today: "not-a-date".into(),
            ..request.clone()
        },
    ] {
        assert_eq!(
            invalid.validate().unwrap_err().code,
            "LEARNING_REQUEST_INVALID"
        );
    }
}

#[test]
fn benefit_consumption_identity_and_source_digest_are_stable() {
    let actor = BenefitConsumptionActor {
        user_id: UserId::new("benefits-reviewer").unwrap(),
        assignment_id: AssignmentId::new("benefits-assignment").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: Some(RecordId::new("department").unwrap()),
        permissions: vec!["records.write".into()],
        modules: vec!["benefits".into()],
        expires_at: NOW + 60_000,
    };
    actor.require_consume(NOW).unwrap();
    let request = consumption_request();
    let identity = benefit_consume_identity(&actor, "benefit-election-1", &request);
    assert!(identity.contains("\"operation\":\"learning.benefit.consume\""));
    assert_eq!(
        identity,
        benefit_consume_identity(&actor, "benefit-election-1", &request)
    );
    assert_eq!(
        certificate_source_json(&certificate(None)),
        certificate_source_json(&certificate(None))
    );
    let mut other = certificate(None);
    other.evidence_ref.push('!');
    assert_ne!(
        certificate_source_json(&certificate(None)),
        certificate_source_json(&other)
    );

    let mut no_scope = actor.clone();
    no_scope.branch_id = None;
    assert!(no_scope.scope().is_err());
    let mut no_module = actor.clone();
    no_module.modules = vec!["learning".into()];
    assert_eq!(
        no_module.require_consume(NOW).unwrap_err().code,
        "MODULE_FORBIDDEN"
    );
    let mut read_only = actor;
    read_only.permissions = vec!["records.read".into()];
    assert_eq!(
        read_only.require_consume(NOW).unwrap_err().code,
        "LEARNING_PERMISSION_REQUIRED"
    );
}

#[test]
fn benefit_continuation_requires_benefits_navigation_authority_and_exact_source() {
    let row = eligibility_row();
    let mut assignment = assignment("completed", true);
    assignment.version = 3;
    assignment.completion_reason = Some("Course and assessment completed".into());
    let modules = vec!["benefits".to_owned()];
    let permissions = vec!["records.read".to_owned()];
    let continuation = || {
        learning_benefit_continuation(
            "learner",
            "company",
            &modules,
            &permissions,
            None,
            &assignment,
            Some(&row.handoff),
            Some(&row.certificate),
        )
    };
    assert_eq!(
        continuation().unwrap()["eligibilityCode"],
        "equipment_operator"
    );
    assert_eq!(
        learning_benefit_continuation(
            "learner",
            "company",
            &modules,
            &permissions,
            Some(true),
            &assignment,
            Some(&row.handoff),
            Some(&row.certificate),
        )
        .unwrap()["eligibilityCode"],
        "equipment_operator"
    );
    assert!(learning_benefit_continuation(
        "learner",
        "company",
        &modules,
        &permissions,
        Some(false),
        &assignment,
        Some(&row.handoff),
        Some(&row.certificate),
    )
    .is_none());
    assert!(learning_benefit_continuation(
        "learner",
        "company",
        &[],
        &permissions,
        None,
        &assignment,
        Some(&row.handoff),
        Some(&row.certificate),
    )
    .is_none());
    assert!(learning_benefit_continuation(
        "learner",
        "company",
        &modules,
        &[],
        None,
        &assignment,
        Some(&row.handoff),
        Some(&row.certificate),
    )
    .is_none());
    assert!(learning_benefit_continuation(
        "peer",
        "company",
        &modules,
        &permissions,
        None,
        &assignment,
        Some(&row.handoff),
        Some(&row.certificate),
    )
    .is_none());
    assert!(learning_benefit_continuation(
        "learner",
        "foreign",
        &modules,
        &permissions,
        None,
        &assignment,
        Some(&row.handoff),
        Some(&row.certificate),
    )
    .is_none());
    let mut stale = assignment.clone();
    stale.version = 4;
    assert!(learning_benefit_continuation(
        "learner",
        "company",
        &modules,
        &permissions,
        None,
        &stale,
        Some(&row.handoff),
        Some(&row.certificate),
    )
    .is_none());
    assert!(learning_benefit_continuation(
        "learner",
        "company",
        &modules,
        &permissions,
        None,
        &assignment,
        None,
        Some(&row.certificate),
    )
    .is_none());
}

#[test]
fn assignment_evidence_request_uses_exact_files_policy_context() {
    let change = complete_change().validate().unwrap();
    assert!(assignment_evidence_request("assignment-1", &change, "command-1")
        .unwrap()
        .is_none());

    let mut attached = change.clone();
    attached.attachment_set_ids = vec!["set-1".into()];
    let request =
        assignment_evidence_request("assignment-1", &attached, "command-1").unwrap().unwrap();
    assert_eq!(request.target_type, "learning_assignment");
    assert_eq!(request.purpose_key, "certificate");
    assert_eq!(request.component_kind, "action");
    assert_eq!(request.component_ref.as_deref(), Some("completion"));
    assert_eq!(request.expected_target_version, 2);
    assert_eq!(request.binding_type, "learning_completion");
    assert_eq!(request.binding_id, "assignment-1");

    let mut waive = AssignmentChange {
        action: AssignmentAction::Waive,
        expected_version: 2,
        reason: "Equivalent external certification verified".into(),
        attendance: None,
        score_bps: None,
        evidence: None,
        certificate_valid_from: None,
        certificate_expires_on: None,
        attachment_set_ids: vec!["set-2".into()],
    }
    .validate()
    .unwrap();
    assert_eq!(
        assignment_evidence_request("assignment-1", &waive, "command-2")
            .unwrap()
            .unwrap()
            .component_ref
            .as_deref(),
        Some("waiver")
    );
    waive.attachment_set_ids = vec![String::new()];
    assert!(assignment_evidence_request("assignment-1", &waive, "command-2").is_err());

    let prepared = PreparedLearningEvidence {
        set_ids: vec!["set-1".into()],
        link_ids: vec!["link-1".into()],
        manifest: vec![EvidenceManifestItem {
            set_id: "set-1".into(),
            link_id: "link-1".into(),
            source_id: "attachment-1".into(),
            purpose_key: "certificate".into(),
            component_kind: "action".into(),
            component_ref: Some("completion".into()),
            checksum_sha256: "a".repeat(64),
            media_type: "application/pdf".into(),
            bytes: 128,
        }],
        target_type: "learning_assignment".into(),
        target_id: "assignment-1".into(),
        expected_target_version: 2,
        policy_version: 1,
        binding_type: "learning_completion".into(),
        binding_id: "assignment-1".into(),
        command_id: "command-1".into(),
    };
    prepared.validate_source(&request).unwrap();
    let mut wrong = prepared.clone();
    wrong.command_id = "another-command".into();
    assert_eq!(
        wrong.validate_source(&request).unwrap_err().code,
        "LEARNING_EVIDENCE_SOURCE_INVALID"
    );

    let mut attached_complete = complete_change();
    attached_complete.attachment_set_ids = vec!["set-1".into()];
    let input = attached_complete.validate().unwrap();
    assert_eq!(
        assignment_change_plan(&learner(), &assignment("in_progress", true), &input, None, "command-1", NOW)
            .unwrap_err()
            .code,
        "LEARNING_EVIDENCE_PORT_REQUIRED"
    );
    let mut plan = assignment_change_plan(
        &learner(),
        &assignment("in_progress", true),
        &input,
        Some(prepared),
        "command-1",
        NOW,
    )
    .unwrap();
    assert_eq!(plan.evidence.as_ref().unwrap().link_ids, vec!["link-1"]);
    plan.evidence = None;
}

fn benefit_learning_request() -> BenefitLearningRequest {
    BenefitLearningRequest {
        company_id: "company".into(),
        person_id: "person-learner".into(),
        required_code: "equipment_operator".into(),
        effective_date: "2026-11-04".into(),
        election_id: "benefit-election-1".into(),
        command_id: "learning-benefit-1".into(),
        today: "2026-11-04".into(),
    }
}

fn prepared_consumption() -> PreparedBenefitConsumption {
    PreparedBenefitConsumption {
        plan: BenefitConsumptionPlan {
            id: "learning-benefit-receipt-1".into(),
            handoff_id: "handoff-1".into(),
            certificate_id: "certificate-1".into(),
            election_id: "benefit-election-1".into(),
            company_id: "company".into(),
            person_id: "person-learner".into(),
            source_assignment_version: 3,
            eligibility_code: "equipment_operator".into(),
            source_digest: "a".repeat(64),
            purpose: "benefit_eligibility".into(),
            actor_id: "learner".into(),
            command_id: "learning-benefit-1".into(),
            created_at: NOW,
        },
        existing: None,
        today: "2026-11-04".into(),
        effective_date: "2026-11-04".into(),
    }
}

#[test]
fn benefit_learning_identity_maps_only_the_prepared_plan_identity() {
    let identity = benefit_learning_identity(&prepared_consumption()).unwrap();
    assert_eq!(
        identity,
        PreparedBenefitLearning {
            certificate_id: "certificate-1".into(),
            election_id: "benefit-election-1".into(),
            command_id: "learning-benefit-1".into(),
        }
    );
    assert!(identity.validate().is_ok());
}

#[test]
fn benefit_learning_identity_refuses_a_malformed_certificate() {
    let mut prepared = prepared_consumption();
    prepared.plan.certificate_id = String::new();
    assert_eq!(
        benefit_learning_identity(&prepared).unwrap_err().code,
        "BENEFITS_SOURCE_INVALID"
    );
}

#[test]
fn benefit_consumption_request_validates_code_person_and_effective_date() {
    let mapped = benefit_consumption_request("learner", &benefit_learning_request()).unwrap();
    assert_eq!(mapped.company_id, "company");
    assert_eq!(mapped.person_id, "person-learner");
    assert_eq!(mapped.required_code, "equipment_operator");
    assert_eq!(mapped.effective_date, "2026-11-04");
    assert_eq!(mapped.election_id, "benefit-election-1");
    assert_eq!(mapped.command_id, "learning-benefit-1");
    assert_eq!(mapped.actor_id, "learner");

    for mutate in [
        (|request: &mut BenefitLearningRequest| request.person_id = String::new())
            as fn(&mut BenefitLearningRequest),
        |request: &mut BenefitLearningRequest| request.required_code = " ".into(),
        |request: &mut BenefitLearningRequest| request.effective_date = "04/11/2026".into(),
        |request: &mut BenefitLearningRequest| request.command_id = String::new(),
    ] {
        let mut request = benefit_learning_request();
        mutate(&mut request);
        let error = benefit_consumption_request("learner", &request).unwrap_err();
        assert_eq!(error.code, "LEARNING_REQUEST_INVALID");
    }
}

#[tokio::test]
async fn benefits_learning_port_refuses_without_the_caller_unit() {
    let mut port = PgBenefitLearning::new("learner".into(), TimestampMillis::new(NOW));
    let error = port
        .prepare(None, benefit_learning_request())
        .await
        .unwrap_err();
    assert_eq!(error.code, "BENEFITS_LEARNING_UNIT_REQUIRED");
    assert_eq!(error.kind, FailureKind::Unavailable);

    let error = port
        .validate(None, benefit_learning_request())
        .await
        .unwrap_err();
    assert_eq!(error.code, "BENEFITS_LEARNING_UNIT_REQUIRED");

    let error = port
        .require_current(
            None,
            &PreparedBenefitLearning {
                certificate_id: "certificate-1".into(),
                election_id: "benefit-election-1".into(),
                command_id: "learning-benefit-1".into(),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, "BENEFITS_LEARNING_UNIT_REQUIRED");
}
