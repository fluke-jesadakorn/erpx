//! Unit policy for the native Learning owner: current actor scope, reviewed
//! course catalogue, eligible assignment, learner evidence, certification and
//! revocation. No database or HTTP effect is reachable here.
use erpx_server_learning::public::learning::{
    AssignmentAction, AssignmentChange, AssignmentRequest, AssignmentRow, Attendance,
    CertificateRevocation, CertificateRow, CourseAction, CourseChange, CourseRequest, CourseRow,
    LearningActor, PlacementRow, assignment_change_plan, assignment_create_plan,
    certificate_revoke_plan, course_change_plan, course_create_plan, utc_date, valid_date,
};
use erpx_server_shared::public::{
    AssignmentId, CompanyId, FailureKind, HoldingId, RecordId, UserId,
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
        .unwrap()
        .required_for
        .is_none()
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
        attached.validate().unwrap_err().code,
        "LEARNING_ATTACHMENTS_RETAINED"
    );
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
        assignment_change_plan(&learner(), &row, &change, "assignment-complete-1", NOW).unwrap();
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
        assignment_change_plan(&learner(), &row, &bad_dates, "assignment-complete-2", NOW)
            .unwrap_err()
            .code,
        "LEARNING_CERTIFICATE_DATES_INVALID"
    );
    let outsider = LearningActor {
        user_id: UserId::new("outsider").unwrap(),
        ..learner()
    };
    assert_eq!(
        assignment_change_plan(&outsider, &row, &change, "assignment-complete-3", NOW)
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
            "assignment-complete-4",
            NOW
        )
        .unwrap_err()
        .code,
        "LEARNING_LEARNER_REQUIRED"
    );
    let assigned = assignment("assigned", true);
    assert_eq!(
        assignment_change_plan(&learner(), &assigned, &change, "assignment-complete-5", NOW)
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
        assignment_change_plan(&reviewer(), &row, &waive, "assignment-waive-1", NOW).unwrap();
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
        assignment_change_plan(&reviewer(), &row, &cancel, "assignment-cancel-1", NOW).unwrap();
    assert_eq!(plan.status, "cancelled");
    assert!(plan.certificate.is_none());
    assert!(plan.completion_reason.is_none());
    let learner = LearningActor {
        permissions: vec!["records.read".into(), "records.write".into()],
        ..reviewer()
    };
    assert_eq!(
        assignment_change_plan(&learner, &row, &waive, "assignment-waive-2", NOW)
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
