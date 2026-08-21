use super::*;
use chrono::Duration;
use erpx_integration_contracts::{CurrencyCode, reference::RecordTypeId};
use erpx_kernel::{ActorAttribution, ActorKind, HumanDecisionProof, SessionId};

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}
fn record_id(n: u128) -> Uuid {
    let mut bytes = n.to_be_bytes();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}
fn scope() -> AuthorityScope {
    AuthorityScope::Company {
        holding_id: id(1),
        company_id: id(2),
    }
}
fn command_id(value: u128) -> CommandId {
    CommandId::try_from_uuid(id(value)).unwrap()
}
fn human_decision(principal: Uuid, reference: String, at: DateTime<Utc>) -> HumanDecisionProof {
    HumanDecisionProof::try_new(
        ActorAttribution {
            principal_ref: principal.to_string(),
            kind: ActorKind::HoldingUser,
            session_id: Some(SessionId::try_from_uuid(id(900)).unwrap()),
            represented_principal_ref: None,
            support_capability_id: None,
        },
        scope(),
        reference,
        at,
    )
    .unwrap()
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
    let approve = command_id(101);
    value
        .approve(
            id(7),
            1,
            &human_decision(
                id(7),
                requisition_approve_decision_ref(approve, value.id(), 1),
                at,
            ),
            approve,
            at,
        )
        .unwrap();
    let transition = command_id(102);
    value
        .transition(
            RequisitionStatus::Published,
            2,
            &human_decision(
                id(7),
                requisition_transition_decision_ref(
                    transition,
                    value.id(),
                    2,
                    RequisitionStatus::Published,
                ),
                at,
            ),
            transition,
            at,
        )
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
    let approve = command_id(103);
    offer
        .approve(
            id(14),
            1,
            &human_decision(
                id(14),
                offer_approve_decision_ref(approve, offer.id(), 1),
                at,
            ),
            approve,
            at,
        )
        .unwrap();
    offer.present(2, at).unwrap();
    let stale_decision = command_id(104);
    assert_eq!(
        offer.candidate_decide(
            candidate.principal(),
            true,
            2,
            "receipt",
            &human_decision(
                candidate.principal().principal_id,
                offer_decide_decision_ref(stale_decision, offer.id(), 2, true),
                at,
            ),
            stale_decision,
            at,
        ),
        Err(TalentError::VersionConflict)
    );
    let decide = command_id(105);
    offer
        .candidate_decide(
            candidate.principal(),
            true,
            3,
            "receipt",
            &human_decision(
                candidate.principal().principal_id,
                offer_decide_decision_ref(decide, offer.id(), 3, true),
                at,
            ),
            decide,
            at,
        )
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
    let approve = command_id(106);
    offer
        .approve(
            id(14),
            1,
            &human_decision(
                id(14),
                offer_approve_decision_ref(approve, offer.id(), 1),
                at,
            ),
            approve,
            at,
        )
        .unwrap();
    offer.present(2, at).unwrap();
    let decide = command_id(107);
    offer
        .candidate_decide(
            candidate.principal(),
            true,
            3,
            "receipt",
            &human_decision(
                candidate.principal().principal_id,
                offer_decide_decision_ref(decide, offer.id(), 3, true),
                at,
            ),
            decide,
            at,
        )
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
    let approve = command_id(108);
    proposal
        .approve(
            id(33),
            2,
            &human_decision(
                id(33),
                proposal_approve_decision_ref(approve, proposal.id(), 2),
                at,
            ),
            approve,
            at,
        )
        .unwrap();
    proposal
        .reconcile_owner(true, "payroll-receipt", at)
        .unwrap();
    assert_eq!(proposal.status(), ProposalStatus::Accepted);
}

#[test]
fn requisition_rejects_unbound_human_evidence_before_mutation() {
    let at = Utc::now();
    let mut value = Requisition::draft(
        RequisitionId::try_from_uuid(id(120)).unwrap(),
        scope(),
        "Engineer",
        record(121, ContextId::Organization, "organization.company"),
        1,
        record(122, ContextId::Finance, "finance.close-cycle"),
        1,
        "a".repeat(64),
        1,
        id(123),
        at,
    )
    .unwrap();
    let command = command_id(124);
    assert_eq!(
        value.approve(
            id(125),
            1,
            &human_decision(id(125), "talent.requisition.approve:wrong".into(), at),
            command,
            at,
        ),
        Err(TalentError::HumanDecisionRequired)
    );
    assert_eq!(value.version(), 1);
    assert!(value.human_decisions().is_empty());
}
