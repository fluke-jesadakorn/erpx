//! Native Talent application and proposal lifecycle tests.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use erpx_server_talent::public::requisition::{ReceiptSnapshot, TalentActor};
use erpx_server_talent::public::talent_application::{
    ApplicationAction, ApplicationChange, ApplicationFuture, ApplicationReceipt,
    ApplicationRequest, ApplicationRow, CandidateFact, CreateApplicationPlan, CreateProposalPlan,
    ProposalAction, ProposalChange, ProposalReceipt, ProposalRequest, ProposalRow, Recommendation,
    RequisitionFact, TalentApplicationCommandRepository, change_application, change_proposal,
    create_application, create_proposal,
};
use serde_json::Value;
use std::sync::Mutex;

const NOW: i64 = 1_800_000_000_000;
const TODAY: &str = "2026-11-11";
const DIGEST: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn actor(user: &str, permissions: &[&str]) -> TalentActor {
    TalentActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        assignment_id: AssignmentId::new(format!("assignment-{user}")).unwrap(),
        role: Some("people_manager".into()),
        authority_revision: Some(2),
        assignment_version: Some(1),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        expires_at: NOW + 60_000,
    }
}

fn recruiter() -> TalentActor {
    actor(
        "recruiter",
        &["records.read", "records.write", "records.review"],
    )
}

fn reviewer() -> TalentActor {
    actor(
        "reviewer",
        &["records.read", "records.write", "records.review"],
    )
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn application_request() -> ApplicationRequest {
    ApplicationRequest {
        requisition_id: "talent-req-1".into(),
        requisition_version: 2,
        candidate_id: "talent-candidate-1".into(),
        owner_id: "owner".into(),
    }
}

fn proposal_request() -> ProposalRequest {
    ProposalRequest {
        application_id: "talent-application-talent-application-test-0001".into(),
        application_version: 1,
        recommendation: Recommendation::Hire,
        score_bps: 8_750,
        evidence: "  Strong interview evidence and verified references.  ".into(),
    }
}

fn application_row(status: &str, version: i64, owner: &str) -> ApplicationRow {
    ApplicationRow {
        id: "talent-application-talent-application-test-0001".into(),
        company_id: "company".into(),
        requisition_id: "talent-req-1".into(),
        requisition_version: 2,
        candidate_id: "talent-candidate-1".into(),
        status: status.into(),
        owner_id: owner.into(),
        created_by: if owner == "recruiter" {
            "recruiter".into()
        } else {
            "creator".into()
        },
        version,
        created_at: NOW - 1_000,
        updated_at: NOW - 1_000,
        candidate_status: "active".into(),
        candidate_version: 1,
        retention_until: "2027-01-01".into(),
    }
}

#[derive(Default)]
struct Fake {
    authority: bool,
    requisition: Option<RequisitionFact>,
    candidate: Option<CandidateFact>,
    owner: bool,
    accepted: bool,
    receipt: Option<ReceiptSnapshot>,
    application: Option<ApplicationRow>,
    proposal: Option<ProposalRow>,
    commits: Mutex<Vec<String>>,
    calls: Mutex<Vec<String>>,
}

impl Fake {
    fn ready() -> Self {
        Self {
            authority: true,
            requisition: Some(RequisitionFact {
                id: "talent-req-1".into(),
                company_id: "company".into(),
                branch_id: "branch".into(),
                department_id: None,
                version: 2,
                status: "open".into(),
            }),
            candidate: Some(CandidateFact {
                id: "talent-candidate-1".into(),
                status: "active".into(),
                retention_until: "2027-01-01".into(),
                version: 1,
            }),
            owner: true,
            accepted: true,
            application: Some(application_row("applied", 1, "owner")),
            ..Self::default()
        }
    }
}

impl TalentApplicationCommandRepository for Fake {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: bool,
        _: i64,
    ) -> ApplicationFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.authority) })
    }

    fn digest<'a>(&'a mut self, _: &'a str) -> ApplicationFuture<'a, String, Self::Error> {
        Box::pin(async move { Ok(DIGEST.into()) })
    }

    fn receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> ApplicationFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        Box::pin(async move { Ok(self.receipt.clone()) })
    }

    fn requisition<'a>(
        &'a mut self,
        _: &'a TalentActor,
        id: &'a str,
    ) -> ApplicationFuture<'a, Option<RequisitionFact>, Self::Error> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(format!("requisition:{id}"));
            Ok(self.requisition.clone())
        })
    }

    fn candidate<'a>(
        &'a mut self,
        _: &'a TalentActor,
        id: &'a str,
    ) -> ApplicationFuture<'a, Option<CandidateFact>, Self::Error> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(format!("candidate:{id}"));
            Ok(self.candidate.clone())
        })
    }

    fn owner_eligible<'a>(
        &'a mut self,
        _: &'a TalentActor,
        owner: &'a str,
        _: i64,
    ) -> ApplicationFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(format!("owner:{owner}"));
            Ok(self.owner)
        })
    }

    fn accepted_proposal<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
    ) -> ApplicationFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.accepted) })
    }

    fn commit_application_create<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
        plan: &'a CreateApplicationPlan,
        _: &'a str,
    ) -> ApplicationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.commits.lock().unwrap().push(format!(
                "application:{}:{}:{}",
                plan.id, plan.requisition_version, plan.owner_id
            ));
            Ok(ApplicationReceipt::new(plan.id.clone(), "applied", 1).to_wire())
        })
    }

    fn application_for_update<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
    ) -> ApplicationFuture<'a, Option<ApplicationRow>, Self::Error> {
        Box::pin(async move { Ok(self.application.clone()) })
    }

    fn commit_application_change<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
        plan: &'a erpx_server_talent::public::talent_application::ApplicationChangePlan,
        _: &'a str,
    ) -> ApplicationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.commits.lock().unwrap().push(format!(
                "application-change:{}:{}:{}",
                plan.action.as_str(),
                plan.status,
                plan.owner_id
            ));
            Ok(ApplicationReceipt::new(plan.id.clone(), &plan.status, plan.version).to_wire())
        })
    }

    fn commit_proposal_create<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
        plan: &'a CreateProposalPlan,
        _: &'a str,
    ) -> ApplicationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.commits.lock().unwrap().push(format!(
                "proposal:{}:{}:{}",
                plan.id,
                plan.recommendation.as_str(),
                plan.score_bps
            ));
            Ok(ProposalReceipt::new(plan.id.clone(), "submitted", 1).to_wire())
        })
    }

    fn proposal_for_update<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
    ) -> ApplicationFuture<'a, Option<ProposalRow>, Self::Error> {
        Box::pin(async move { Ok(self.proposal.clone()) })
    }

    fn commit_proposal_change<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
        plan: &'a erpx_server_talent::public::talent_application::ProposalChangePlan,
        _: &'a str,
    ) -> ApplicationFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.commits.lock().unwrap().push(format!(
                "proposal-change:{}:{}",
                plan.action.as_str(),
                plan.status
            ));
            Ok(ProposalReceipt::new(plan.id.clone(), &plan.status, plan.version).to_wire())
        })
    }
}

#[tokio::test]
async fn application_create_pins_open_requisition_current_candidate_and_owner() {
    let mut fake = Fake::ready();
    let result = create_application(
        &mut fake,
        &recruiter(),
        &key("talent-application-test-0001"),
        &application_request(),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "applied");
    assert_eq!(
        fake.commits.lock().unwrap().clone(),
        vec!["application:talent-application-talent-application-test-0001:2:owner".to_owned()]
    );
    assert_eq!(
        fake.calls.lock().unwrap().clone(),
        vec![
            "requisition:talent-req-1".to_owned(),
            "candidate:talent-candidate-1".to_owned(),
            "owner:owner".to_owned()
        ]
    );
}

#[tokio::test]
async fn application_create_rejects_stale_requisition_candidate_and_owner() {
    for (name, fake) in [
        ("closed", {
            let mut fake = Fake::ready();
            fake.requisition.as_mut().unwrap().status = "approved".into();
            fake
        }),
        ("version", {
            let mut fake = Fake::ready();
            fake.requisition.as_mut().unwrap().version = 1;
            fake
        }),
        ("other-branch", {
            let mut fake = Fake::ready();
            fake.requisition.as_mut().unwrap().branch_id = "other".into();
            fake
        }),
        ("expired-consent", {
            let mut fake = Fake::ready();
            fake.candidate.as_mut().unwrap().retention_until = "2026-01-01".into();
            fake
        }),
        ("withdrawn-candidate", {
            let mut fake = Fake::ready();
            fake.candidate.as_mut().unwrap().status = "withdrawn".into();
            fake
        }),
        ("owner", {
            let mut fake = Fake::ready();
            fake.owner = false;
            fake
        }),
    ] {
        let mut fake = fake;
        let error = create_application(
            &mut fake,
            &recruiter(),
            &key(&format!("talent-application-test-{name}")),
            &application_request(),
            TODAY,
            NOW,
        )
        .await
        .unwrap_err();
        let expected = match name {
            "owner" => "TALENT_OWNER_INELIGIBLE",
            "expired-consent" | "withdrawn-candidate" => "TALENT_CANDIDATE_PROCESSING_BLOCKED",
            _ => "TALENT_SOURCE_CHANGED",
        };
        assert_eq!(error.code, expected, "{name}");
        assert_eq!(error.kind, FailureKind::Conflict, "{name}");
        assert!(fake.commits.lock().unwrap().is_empty(), "{name}");
    }

    let mut fake = Fake::ready();
    fake.candidate = None;
    let error = create_application(
        &mut fake,
        &recruiter(),
        &key("talent-application-test-no-candidate"),
        &application_request(),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "TALENT_CANDIDATE_PROCESSING_BLOCKED");
}

fn application_change(action: ApplicationAction, version: i64) -> ApplicationChange {
    ApplicationChange {
        action,
        expected_version: version,
        reason: "Documented recruitment decision".into(),
        owner_id: None,
    }
}

#[tokio::test]
async fn application_transitions_are_cas_and_candidate_bound() {
    let mut fake = Fake::ready();
    let interview = change_application(
        &mut fake,
        &recruiter(),
        &key("talent-application-change-0001"),
        "talent-application-talent-application-test-0001",
        &application_change(ApplicationAction::Interview, 1),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(interview["status"], "interview");

    fake.application = Some(application_row("interview", 2, "owner"));
    let selected = change_application(
        &mut fake,
        &recruiter(),
        &key("talent-application-change-0002"),
        "talent-application-talent-application-test-0001",
        &application_change(ApplicationAction::Select, 2),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(selected["status"], "selected");

    fake.application = Some(application_row("interview", 2, "owner"));
    fake.accepted = false;
    let missing_proposal = change_application(
        &mut fake,
        &recruiter(),
        &key("talent-application-change-0003"),
        "talent-application-talent-application-test-0001",
        &application_change(ApplicationAction::Select, 2),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(missing_proposal.code, "TALENT_PROPOSAL_REQUIRED");

    fake.accepted = true;
    let stale = change_application(
        &mut fake,
        &recruiter(),
        &key("talent-application-change-0004"),
        "talent-application-talent-application-test-0001",
        &application_change(ApplicationAction::Select, 1),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(stale.code, "TALENT_APPLICATION_CHANGED");

    fake.application.as_mut().unwrap().retention_until = "2026-01-01".into();
    let blocked = change_application(
        &mut fake,
        &recruiter(),
        &key("talent-application-change-0005"),
        "talent-application-talent-application-test-0001",
        &application_change(ApplicationAction::Interview, 2),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(blocked.code, "TALENT_CANDIDATE_PROCESSING_BLOCKED");
}

#[tokio::test]
async fn withdraw_reject_and_reassign_follow_owner_and_review_rules() {
    let mut fake = Fake::ready();
    fake.candidate = None;
    let withdrawn = change_application(
        &mut fake,
        &recruiter(),
        &key("talent-application-withdraw-0001"),
        "talent-application-talent-application-test-0001",
        &application_change(ApplicationAction::Withdraw, 1),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(withdrawn["status"], "withdrawn");

    let mut fake = Fake::ready();
    fake.application = Some(application_row("applied", 1, "recruiter"));
    let self_reject = change_application(
        &mut fake,
        &recruiter(),
        &key("talent-application-reject-0001"),
        "talent-application-talent-application-test-0001",
        &application_change(ApplicationAction::Reject, 1),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(self_reject.code, "TALENT_INDEPENDENT_REVIEW_REQUIRED");

    fake.application = Some(application_row("applied", 1, "owner"));
    let rejected = change_application(
        &mut fake,
        &reviewer(),
        &key("talent-application-reject-0002"),
        "talent-application-talent-application-test-0001",
        &application_change(ApplicationAction::Reject, 1),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(rejected["status"], "rejected");

    fake.application = Some(application_row("interview", 3, "owner"));
    let missing_owner = change_application(
        &mut fake,
        &recruiter(),
        &key("talent-application-reassign-0001"),
        "talent-application-talent-application-test-0001",
        &ApplicationChange {
            action: ApplicationAction::Reassign,
            expected_version: 3,
            reason: "Owner left the recruitment team".into(),
            owner_id: None,
        },
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(missing_owner.code, "OWNER_REQUIRED");
    assert_eq!(missing_owner.kind, FailureKind::InvalidInput);

    fake.owner = false;
    let ineligible = change_application(
        &mut fake,
        &recruiter(),
        &key("talent-application-reassign-0002"),
        "talent-application-talent-application-test-0001",
        &ApplicationChange {
            action: ApplicationAction::Reassign,
            expected_version: 3,
            reason: "Owner left the recruitment team".into(),
            owner_id: Some("new-owner".into()),
        },
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(ineligible.code, "TALENT_OWNER_INELIGIBLE");

    fake.owner = true;
    let reassigned = change_application(
        &mut fake,
        &recruiter(),
        &key("talent-application-reassign-0003"),
        "talent-application-talent-application-test-0001",
        &ApplicationChange {
            action: ApplicationAction::Reassign,
            expected_version: 3,
            reason: "Owner left the recruitment team".into(),
            owner_id: Some("new-owner".into()),
        },
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(reassigned["status"], "interview");
    assert_eq!(
        fake.commits.lock().unwrap().last().map(String::as_str),
        Some("application-change:reassign:interview:new-owner")
    );
}

fn proposal_row(status: &str, version: i64, submitted_by: &str) -> ProposalRow {
    ProposalRow {
        id: "talent-proposal-talent-proposal-test-0001".into(),
        company_id: "company".into(),
        application_id: "talent-application-talent-application-test-0001".into(),
        application_version: 1,
        recommendation: "hire".into(),
        score_bps: 8_750,
        evidence: "Strong interview evidence".into(),
        status: status.into(),
        submitted_by: submitted_by.into(),
        reviewed_by: None,
        version,
        created_at: NOW - 1_000,
        updated_at: NOW - 1_000,
    }
}

#[tokio::test]
async fn proposal_create_requires_the_owners_current_interview_application() {
    let mut fake = Fake::ready();
    fake.application = Some(application_row("interview", 1, "recruiter"));
    let result = create_proposal(
        &mut fake,
        &recruiter(),
        &key("talent-proposal-test-0001"),
        &proposal_request(),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "submitted");
    assert_eq!(
        fake.commits.lock().unwrap().clone(),
        vec!["proposal:talent-proposal-talent-proposal-test-0001:hire:8750".to_owned()]
    );

    for (name, setup, expected) in [
        (
            "not-owner",
            Box::new(|fake: &mut Fake| {
                fake.application = Some(application_row("interview", 1, "someone-else"));
            }) as Box<dyn Fn(&mut Fake)>,
            "TALENT_APPLICATION_CHANGED",
        ),
        (
            "status",
            Box::new(|fake: &mut Fake| {
                fake.application = Some(application_row("applied", 1, "recruiter"));
            }),
            "TALENT_APPLICATION_CHANGED",
        ),
        (
            "version",
            Box::new(|fake: &mut Fake| {
                fake.application = Some(application_row("interview", 2, "recruiter"));
            }),
            "TALENT_APPLICATION_CHANGED",
        ),
        (
            "candidate",
            Box::new(|fake: &mut Fake| {
                fake.application = Some(application_row("interview", 1, "recruiter"));
                fake.application.as_mut().unwrap().candidate_status = "withdrawn".into();
            }),
            "TALENT_CANDIDATE_PROCESSING_BLOCKED",
        ),
    ] {
        let mut fake = Fake::ready();
        setup(&mut fake);
        let error = create_proposal(
            &mut fake,
            &recruiter(),
            &key(&format!("talent-proposal-test-{name}")),
            &proposal_request(),
            TODAY,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, expected, "{name}");
    }

    let mut fake = Fake::ready();
    let mut request = proposal_request();
    request.score_bps = 10_001;
    let error = create_proposal(
        &mut fake,
        &recruiter(),
        &key("talent-proposal-test-score"),
        &request,
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "TALENT_APPLICATION_INVALID");
}

fn proposal_change(action: ProposalAction, version: i64) -> ProposalChange {
    ProposalChange {
        action,
        expected_version: version,
        reason: "Independent proposal review".into(),
    }
}

#[tokio::test]
async fn proposal_acceptance_is_independent_cas_and_candidate_bound() {
    let mut fake = Fake::ready();
    fake.proposal = Some(proposal_row("submitted", 1, "recruiter"));
    let accepted = change_proposal(
        &mut fake,
        &reviewer(),
        &key("talent-proposal-accept-0001"),
        "talent-proposal-talent-proposal-test-0001",
        &proposal_change(ProposalAction::AcceptProposal, 1),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(accepted["status"], "accepted");

    let self_accept = change_proposal(
        &mut fake,
        &recruiter(),
        &key("talent-proposal-accept-0002"),
        "talent-proposal-talent-proposal-test-0001",
        &proposal_change(ProposalAction::AcceptProposal, 1),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(self_accept.code, "TALENT_INDEPENDENT_REVIEW_REQUIRED");

    let stale = change_proposal(
        &mut fake,
        &reviewer(),
        &key("talent-proposal-accept-0003"),
        "talent-proposal-talent-proposal-test-0001",
        &proposal_change(ProposalAction::AcceptProposal, 2),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(stale.code, "TALENT_PROPOSAL_CHANGED");

    fake.proposal = Some(proposal_row("accepted", 2, "recruiter"));
    let accepted_again = change_proposal(
        &mut fake,
        &reviewer(),
        &key("talent-proposal-accept-0004"),
        "talent-proposal-talent-proposal-test-0001",
        &proposal_change(ProposalAction::AcceptProposal, 2),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(accepted_again.code, "TALENT_PROPOSAL_CHANGED");

    fake.proposal = Some(proposal_row("submitted", 1, "creator"));
    fake.application = Some(application_row("interview", 1, "owner"));
    fake.application.as_mut().unwrap().candidate_status = "erased".into();
    let blocked = change_proposal(
        &mut fake,
        &reviewer(),
        &key("talent-proposal-accept-0005"),
        "talent-proposal-talent-proposal-test-0001",
        &proposal_change(ProposalAction::AcceptProposal, 1),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(blocked.code, "TALENT_CANDIDATE_PROCESSING_BLOCKED");

    fake.candidate = Some(CandidateFact {
        id: "talent-candidate-1".into(),
        status: "active".into(),
        retention_until: "2027-01-01".into(),
        version: 1,
    });
    fake.proposal = Some(proposal_row("submitted", 1, "creator"));
    let rejected = change_proposal(
        &mut fake,
        &reviewer(),
        &key("talent-proposal-reject-0001"),
        "talent-proposal-talent-proposal-test-0001",
        &proposal_change(ProposalAction::RejectProposal, 1),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(rejected["status"], "rejected");

    let mut missing = Fake::ready();
    missing.proposal = None;
    let not_found = change_proposal(
        &mut missing,
        &reviewer(),
        &key("talent-proposal-reject-0002"),
        "talent-proposal-talent-proposal-test-0001",
        &proposal_change(ProposalAction::RejectProposal, 1),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(not_found.code, "TALENT_RECORD_NOT_FOUND");
}
