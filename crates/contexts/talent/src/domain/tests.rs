use super::*;
use chrono::Duration;
use erpx_integration_contracts::{CurrencyCode, reference::RecordTypeId};

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}
fn record_id(n: u128) -> Uuid {
    let mut bytes = n.to_be_bytes();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
fn scope() -> TenantScope {
    TenantScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}
fn record(n: u128, context: ContextId, kind: &str) -> StableRecordRef {
    StableRecordRef::try_new(
        RecordId::try_from_uuid(record_id(n)).unwrap(),
        context,
        RecordTypeId::try_new(kind).unwrap(),
    )
    .unwrap()
}
fn requisition(at: DateTime<Utc>) -> Requisition {
    let mut value = Requisition::draft(
        RequisitionId::try_from_uuid(id(3)).unwrap(),
        scope(),
        "Engineer",
        record(4, ContextId::Organization, "organization.company"),
        1,
        record(5, ContextId::Finance, "finance.close-cycle"),
        1,
        "a".repeat(64),
        1,
        id(6),
        at,
    )
    .unwrap();
    value.approve(id(7), 1, at).unwrap();
    value
        .transition(RequisitionStatus::Published, 2, at)
        .unwrap();
    value
}
fn candidate(at: DateTime<Utc>) -> CandidateProfile {
    CandidateProfile::create(
        CandidateId::try_from_uuid(id(8)).unwrap(),
        scope(),
        RealmPrincipal::candidate(id(9)).unwrap(),
        "b".repeat(64),
        vec!["c".repeat(64)],
        true,
        false,
        1,
        at,
    )
    .unwrap()
}

#[test]
fn workforce_principal_cannot_own_candidate_profile() {
    let result = CandidateProfile::create(
        CandidateId::try_from_uuid(id(1)).unwrap(),
        scope(),
        RealmPrincipal::workforce(id(2)).unwrap(),
        "a".repeat(64),
        vec![],
        true,
        false,
        1,
        Utc::now(),
    );
    assert_eq!(result.unwrap_err(), TalentError::RealmViolation);
}

#[test]
fn application_pins_profile_and_requisition_versions() {
    let at = Utc::now();
    let req = requisition(at);
    let candidate = candidate(at);
    let application = Application::submit(
        ApplicationId::try_from_uuid(id(10)).unwrap(),
        &candidate,
        &req,
        "submit-1",
        at,
    )
    .unwrap();
    assert_eq!(application.status(), ApplicationStatus::Submitted);
    assert_eq!(application.version(), 1);
}

#[test]
fn selection_requires_complete_exact_interview_evidence() {
    let at = Utc::now();
    let req = requisition(at);
    let candidate = candidate(at);
    let mut application = Application::submit(
        ApplicationId::try_from_uuid(id(10)).unwrap(),
        &candidate,
        &req,
        "submit-1",
        at,
    )
    .unwrap();
    application
        .add_interview(
            InterviewEvidence {
                slot_digest: "d".repeat(64),
                participant_digest: "e".repeat(64),
                outcome_digest: None,
            },
            1,
            at,
        )
        .unwrap();
    let evidence = SelectionEvidence {
        profile_version: candidate.version(),
        requisition_version: req.version(),
        scorecard_digest: "f".repeat(64),
        decided_by: id(11),
    };
    assert_eq!(
        application.select(evidence, 2, at),
        Err(TalentError::IncompleteSelection)
    );
}

#[test]
fn candidate_accepts_only_exact_unexpired_offer() {
    let at = Utc::now();
    let req = requisition(at);
    let candidate = candidate(at);
    let mut application = Application::submit(
        ApplicationId::try_from_uuid(id(10)).unwrap(),
        &candidate,
        &req,
        "submit-1",
        at,
    )
    .unwrap();
    application
        .add_interview(
            InterviewEvidence {
                slot_digest: "d".repeat(64),
                participant_digest: "e".repeat(64),
                outcome_digest: Some("f".repeat(64)),
            },
            1,
            at,
        )
        .unwrap();
    application
        .select(
            SelectionEvidence {
                profile_version: candidate.version(),
                requisition_version: req.version(),
                scorecard_digest: "1".repeat(64),
                decided_by: id(11),
            },
            2,
            at,
        )
        .unwrap();
    let mut offer = Offer::draft(
        OfferId::try_from_uuid(id(12)).unwrap(),
        &application,
        "2".repeat(64),
        1,
        at + Duration::days(7),
        id(13),
        at,
    )
    .unwrap();
    offer.approve(id(14), 1, at).unwrap();
    offer.present(2, at).unwrap();
    assert_eq!(
        offer.candidate_decide(candidate.principal(), true, 2, "receipt", at),
        Err(TalentError::VersionConflict)
    );
    offer
        .candidate_decide(candidate.principal(), true, 3, "receipt", at)
        .unwrap();
    assert_eq!(offer.status(), OfferStatus::Accepted);
}

#[test]
fn hire_handoff_accepts_only_people_owned_record() {
    let at = Utc::now();
    let req = requisition(at);
    let candidate = candidate(at);
    let mut application = Application::submit(
        ApplicationId::try_from_uuid(id(10)).unwrap(),
        &candidate,
        &req,
        "submit-1",
        at,
    )
    .unwrap();
    application
        .add_interview(
            InterviewEvidence {
                slot_digest: "d".repeat(64),
                participant_digest: "e".repeat(64),
                outcome_digest: Some("f".repeat(64)),
            },
            1,
            at,
        )
        .unwrap();
    application
        .select(
            SelectionEvidence {
                profile_version: candidate.version(),
                requisition_version: req.version(),
                scorecard_digest: "1".repeat(64),
                decided_by: id(11),
            },
            2,
            at,
        )
        .unwrap();
    let mut offer = Offer::draft(
        OfferId::try_from_uuid(id(12)).unwrap(),
        &application,
        "2".repeat(64),
        1,
        at + Duration::days(7),
        id(13),
        at,
    )
    .unwrap();
    offer.approve(id(14), 1, at).unwrap();
    offer.present(2, at).unwrap();
    offer
        .candidate_decide(candidate.principal(), true, 3, "receipt", at)
        .unwrap();
    let mut handoff = HireHandoff::request(
        HireHandoffId::try_from_uuid(id(15)).unwrap(),
        &offer,
        "hire-1",
        "3".repeat(64),
        at,
    )
    .unwrap();
    assert_eq!(
        handoff.reconcile(
            HandoffStatus::Accepted,
            "owner",
            Some(record(20, ContextId::Finance, "finance.invoice")),
            at
        ),
        Err(TalentError::OwnerBoundaryViolation)
    );
    handoff
        .reconcile(
            HandoffStatus::Accepted,
            "owner",
            Some(record(21, ContextId::PeoplePayroll, "people.employee")),
            at,
        )
        .unwrap();
    assert_eq!(handoff.status(), HandoffStatus::Accepted);
}

#[test]
fn legal_hold_blocks_candidate_erasure() {
    let at = Utc::now();
    let mut candidate = candidate(at);
    candidate.set_legal_hold(true, 1, at).unwrap();
    let principal = candidate.principal().clone();
    assert_eq!(
        candidate.request_erasure(&principal, 2, at),
        Err(TalentError::RetentionBlocked)
    );
}

#[test]
fn compensation_is_proposal_not_payroll_posting() {
    let at = Utc::now();
    let mut proposal = TalentProposal::draft(
        ProposalId::try_from_uuid(id(30)).unwrap(),
        scope(),
        ProposalKind::Compensation,
        record(31, ContextId::PeoplePayroll, "people.employee"),
        4,
        2,
        "4".repeat(64),
        Some(MoneyAmount::new(
            10_000,
            CurrencyCode::try_new("THB").unwrap(),
        )),
        id(32),
        at,
    )
    .unwrap();
    proposal.submit(1, at).unwrap();
    proposal.approve(id(33), 2, at).unwrap();
    proposal
        .reconcile_owner(true, "payroll-receipt", at)
        .unwrap();
    assert_eq!(proposal.status(), ProposalStatus::Accepted);
}
