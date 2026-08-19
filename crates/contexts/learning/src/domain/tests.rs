use chrono::{NaiveDate, TimeZone, Utc};
use erpx_integration_contracts::{
    RecordVersionRef, StableRecordRef, catalog::ContextId, reference::RecordTypeId,
};
use erpx_kernel::{AuthorityScope, PrincipalId, RecordId};
use uuid::Uuid;

use super::*;
fn uuid(value: u128) -> Uuid {
    let mut bytes = value.to_be_bytes();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
fn at(hour: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 15, hour, 0, 0)
        .single()
        .unwrap()
}
fn scope() -> AuthorityScope {
    AuthorityScope::Branch {
        holding_id: uuid(1),
        company_id: uuid(2),
        branch_id: uuid(3),
    }
}
fn record(id: u128, owner: ContextId, kind: &str) -> StableRecordRef {
    StableRecordRef::try_new(
        RecordId::try_from_uuid(uuid(id)).unwrap(),
        owner,
        RecordTypeId::try_new(kind).unwrap(),
    )
    .unwrap()
}
fn pin(id: u128, owner: ContextId, kind: &str, version: u64) -> RecordVersionRef {
    RecordVersionRef::try_new(record(id, owner, kind), version).unwrap()
}
fn course() -> Course {
    Course::published(
        CourseId::try_from_uuid(uuid(10)).unwrap(),
        scope(),
        "Workplace safety",
        8_000,
        60,
        Some(record(
            11,
            ContextId::PlatformAdministration,
            "platform.provider",
        )),
        at(1),
    )
    .unwrap()
}
fn enrollment(learner: PrincipalId) -> Enrollment {
    Enrollment::assign(
        EnrollmentId::try_from_uuid(uuid(20)).unwrap(),
        AssignmentId::try_from_uuid(uuid(21)).unwrap(),
        scope(),
        pin(22, ContextId::PeoplePayroll, "people.employee", 3),
        learner,
        &course(),
        NaiveDate::from_ymd_opt(2026, 8, 30).unwrap(),
        at(3),
    )
    .unwrap()
}

#[test]
fn learner_cannot_self_edit_attendance_or_self_waive() {
    let learner = PrincipalId::try_from_uuid(uuid(30)).unwrap();
    let mut enrollment = enrollment(learner);
    let attendance = AttendanceFact {
        id: AttendanceFactId::try_from_uuid(uuid(31)).unwrap(),
        session_id: LearningSessionId::try_from_uuid(uuid(32)).unwrap(),
        attended_minutes: 60,
        recorded_by: learner,
        evidence: pin(33, ContextId::Collaboration, "collaboration.thread", 2),
        at: at(4),
    };
    assert_eq!(
        enrollment.record_attendance(learner, attendance),
        Err(LearningDomainError::SelfAttendanceEdit)
    );
    let waiver = Waiver {
        id: WaiverId::try_from_uuid(uuid(34)).unwrap(),
        authorized_by: learner,
        reason: "Prior equivalent learning accepted".into(),
        evidence: pin(35, ContextId::Legal, "legal.contract", 1),
        granted_at: at(4),
    };
    assert_eq!(
        enrollment.waive(learner, waiver),
        Err(LearningDomainError::SelfWaiver)
    );
}

#[test]
fn published_course_is_a_persistable_initial_version() {
    let course = course();
    assert_eq!(course.status(), CourseStatus::Published);
    assert_eq!(course.version(), 1);
}

#[test]
fn pinned_evidence_produces_minimal_scoped_completion_fact() {
    let learner = PrincipalId::try_from_uuid(uuid(40)).unwrap();
    let trainer = PrincipalId::try_from_uuid(uuid(41)).unwrap();
    let mut enrollment = enrollment(learner);
    let course = course();
    enrollment
        .record_attendance(
            trainer,
            AttendanceFact {
                id: AttendanceFactId::try_from_uuid(uuid(42)).unwrap(),
                session_id: LearningSessionId::try_from_uuid(uuid(43)).unwrap(),
                attended_minutes: 60,
                recorded_by: trainer,
                evidence: pin(44, ContextId::Collaboration, "collaboration.thread", 2),
                at: at(4),
            },
        )
        .unwrap();
    assert!(
        enrollment
            .record_provider_result(ProviderResult {
                id: ProviderResultId::try_from_uuid(uuid(45)).unwrap(),
                provider_attempt_id: "provider-attempt-1".into(),
                score_basis_points: 8_500,
                passed: true,
                evidence: pin(46, ContextId::Legal, "legal.contract", 5),
                received_at: at(5)
            })
            .unwrap()
    );
    assert!(
        !enrollment
            .record_provider_result(ProviderResult {
                id: ProviderResultId::try_from_uuid(uuid(45)).unwrap(),
                provider_attempt_id: "provider-attempt-1".into(),
                score_basis_points: 8_500,
                passed: true,
                evidence: pin(46, ContextId::Legal, "legal.contract", 5),
                received_at: at(5)
            })
            .unwrap()
    );
    enrollment
        .complete(
            &course,
            CompletionId::try_from_uuid(uuid(47)).unwrap(),
            at(6),
        )
        .unwrap();
    let fact = enrollment.completion_fact().unwrap();
    assert_eq!(fact.course_version, course.version());
    assert_eq!(fact.enrollment_version, enrollment.version());
    let json = serde_json::to_string(&fact).unwrap();
    assert!(!json.contains("certificate"));
    assert!(!json.contains("provider_attempt"));
}
