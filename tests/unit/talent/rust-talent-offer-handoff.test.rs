//! Native Talent offer, Money-snapshot and hire-handoff tests.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use erpx_server_talent::public::offer::{
    HireHandoffSource, OfferAction, OfferApplicationFact, OfferChange, OfferFuture,
    OfferMoneyRequest, OfferMoneySnapshot, OfferReceipt, OfferRequest, OfferRow, TalentActor,
    TalentHireHandoffReadRepository, TalentOfferCommandRepository, change_offer, create_offer,
    hire_handoff_queue, hire_handoff_source, parse_amount_minor,
};
use erpx_server_talent::public::requisition::ReceiptSnapshot;
use serde_json::Value;
use std::sync::Mutex;

const NOW: i64 = 1_800_000_000_000;
const TODAY: &str = "2026-11-11";
const DIGEST: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
const OFFER: &str = "talent-offer-talent-offer-test-0001";

fn actor(user: &str) -> TalentActor {
    TalentActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: Some(RecordId::new("branch").unwrap()),
        department_id: None,
        assignment_id: AssignmentId::new(format!("assignment-{user}")).unwrap(),
        role: Some("people_manager".into()),
        authority_revision: Some(3),
        assignment_version: Some(1),
        permissions: vec![
            "records.read".into(),
            "records.write".into(),
            "records.review".into(),
        ],
        expires_at: NOW + 60_000,
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn request() -> OfferRequest {
    OfferRequest {
        application_id: "talent-application-1".into(),
        application_version: 3,
        currency: "thb".into(),
        base_amount: "45000.50".into(),
        start_date: "2026-12-01".into(),
        expires_on: "2026-12-15".into(),
        terms: "  Twelve-month contract with standard benefits.  ".into(),
    }
}

fn application(status: &str, version: i64, accepted: bool) -> OfferApplicationFact {
    OfferApplicationFact {
        id: "talent-application-1".into(),
        company_id: "company".into(),
        requisition_id: "talent-req-1".into(),
        branch_id: "branch".into(),
        department_id: None,
        version,
        status: status.into(),
        candidate_id: "talent-candidate-1".into(),
        candidate_status: "active".into(),
        candidate_version: 2,
        retention_until: "2027-01-01".into(),
        has_accepted_proposal: accepted,
    }
}

fn offer_row(status: &str, version: i64, created_by: &str) -> OfferRow {
    OfferRow {
        id: OFFER.into(),
        company_id: "company".into(),
        application_id: "talent-application-1".into(),
        application_version: 3,
        branch_id: "branch".into(),
        department_id: None,
        currency: "THB".into(),
        base_amount_minor: 4_500_050,
        start_date: "2026-12-01".into(),
        expires_on: "2026-12-15".into(),
        terms: "Twelve-month contract with standard benefits.".into(),
        status: status.into(),
        created_by: created_by.into(),
        approved_by: None,
        accepted_evidence: None,
        version,
        created_at: NOW - 1_000,
        updated_at: NOW - 1_000,
        candidate_id: "talent-candidate-1".into(),
        candidate_status: "active".into(),
        candidate_version: 2,
        retention_until: "2027-01-01".into(),
    }
}

fn handoff() -> HireHandoffSource {
    HireHandoffSource {
        id: "talent-hire-1".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        department_id: None,
        offer_id: OFFER.into(),
        offer_version: 4,
        offer_status: "accepted".into(),
        source_offer_version: 4,
        application_id: "talent-application-1".into(),
        candidate_id: "talent-candidate-1".into(),
        candidate_version: 2,
        candidate_status: "active".into(),
        retention_until: "2027-01-01".into(),
        legal_name: "Anong Candidate".into(),
        email: "anong@example.test".into(),
        phone: None,
        consent_evidence: "Signed recruitment consent form".into(),
        accepted_evidence: Some("Signed offer acceptance".into()),
        currency: "THB".into(),
        base_amount_minor: 4_500_050,
        start_date: "2026-12-01".into(),
        job_title: "Service specialist".into(),
        status: "open".into(),
        created_at: NOW - 500,
    }
}

#[derive(Default)]
struct Fake {
    authority: bool,
    application: Option<OfferApplicationFact>,
    row: Option<OfferRow>,
    receipt: Option<ReceiptSnapshot>,
    money: Option<OfferMoneySnapshot>,
    handoff: Option<HireHandoffSource>,
    consumed: bool,
    commits: Mutex<Vec<String>>,
}

impl Fake {
    fn ready() -> Self {
        Self {
            authority: true,
            application: Some(application("selected", 3, true)),
            row: Some(offer_row("draft", 1, "creator")),
            money: Some(OfferMoneySnapshot {
                currency_scale: 2,
                amount_units: 4_500_050,
                usd_units: 125_000,
                rate_id: Some("fx-1".into()),
                used_latest_fallback: false,
                captured_at: NOW - 1,
            }),
            handoff: Some(handoff()),
            ..Self::default()
        }
    }
}

impl TalentOfferCommandRepository for Fake {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: bool,
        _: i64,
    ) -> OfferFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.authority) })
    }

    fn digest<'a>(&'a mut self, _: &'a str) -> OfferFuture<'a, String, Self::Error> {
        Box::pin(async move { Ok(DIGEST.into()) })
    }

    fn receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> OfferFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        Box::pin(async move { Ok(self.receipt.clone()) })
    }

    fn application<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
    ) -> OfferFuture<'a, Option<OfferApplicationFact>, Self::Error> {
        Box::pin(async move { Ok(self.application.clone()) })
    }

    fn money_snapshot<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
        _: &'a OfferMoneyRequest,
        _: i64,
    ) -> OfferFuture<'a, OfferMoneySnapshot, Self::Error> {
        Box::pin(async move {
            self.money
                .clone()
                .ok_or_else(erpx_server_talent::public::offer::money_snapshot_unavailable)
        })
    }

    fn commit_create<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
        plan: &'a erpx_server_talent::public::offer::CreateOfferPlan,
        _: &'a str,
    ) -> OfferFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.commits.lock().unwrap().push(format!(
                "create:{}:{}:{}",
                plan.id, plan.currency, plan.amount_minor
            ));
            Ok(OfferReceipt::new(plan.id.clone(), "draft", 1, None).to_wire())
        })
    }

    fn offer_for_update<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
    ) -> OfferFuture<'a, Option<OfferRow>, Self::Error> {
        Box::pin(async move { Ok(self.row.clone()) })
    }

    fn commit_change<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
        plan: &'a erpx_server_talent::public::offer::OfferChangePlan,
        _: &'a str,
    ) -> OfferFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.commits.lock().unwrap().push(format!(
                "change:{}:{}:{}",
                plan.action.as_str(),
                plan.status,
                plan.handoff_id.is_some()
            ));
            Ok(OfferReceipt::new(
                plan.id.clone(),
                &plan.status,
                plan.version,
                plan.handoff_id.clone(),
            )
            .to_wire())
        })
    }
}

impl TalentHireHandoffReadRepository for Fake {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: bool,
        _: i64,
    ) -> OfferFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.authority) })
    }

    fn handoff<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
    ) -> OfferFuture<'a, Option<HireHandoffSource>, Self::Error> {
        Box::pin(async move { Ok(self.handoff.clone()) })
    }

    fn handoffs<'a>(
        &'a mut self,
        _: &'a TalentActor,
    ) -> OfferFuture<'a, Vec<(HireHandoffSource, bool)>, Self::Error> {
        Box::pin(async move {
            Ok(self
                .handoff
                .clone()
                .map(|value| vec![(value, self.consumed)])
                .unwrap_or_default())
        })
    }
}

#[test]
fn money_parsing_matches_the_decimal_offer_schema() {
    for (value, minor) in [
        ("0", 0),
        ("1", 100),
        ("45000", 4_500_000),
        ("45000.5", 4_500_050),
        ("45000.50", 4_500_050),
        ("0.01", 1),
    ] {
        assert_eq!(parse_amount_minor(value).unwrap(), minor, "{value}");
    }
    for value in [
        "", ".", "1.", ".5", "1.234", "-1", "1e3", "12a", " 1", "1.2.3",
    ] {
        assert_eq!(
            parse_amount_minor(value).unwrap_err().code,
            "TALENT_OFFER_INVALID",
            "{value}"
        );
    }
}

#[tokio::test]
async fn offer_create_fails_closed_without_the_finance_money_producer() {
    let mut fake = Fake::ready();
    fake.money = None;
    let error = create_offer(
        &mut fake,
        &actor("recruiter"),
        &key("talent-offer-test-0001"),
        &request(),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "TALENT_OFFER_MONEY_SNAPSHOT_UNAVAILABLE");
    assert_eq!(error.kind, FailureKind::Unavailable);
    assert!(fake.commits.lock().unwrap().is_empty());
}

#[tokio::test]
async fn offer_create_pins_selected_application_and_snapshots_monthly_compensation() {
    let mut fake = Fake::ready();
    let result = create_offer(
        &mut fake,
        &actor("recruiter"),
        &key("talent-offer-test-0002"),
        &request(),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "draft");
    assert_eq!(
        fake.commits.lock().unwrap().clone(),
        vec!["create:talent-offer-talent-offer-test-0002:THB:4500050".to_owned()]
    );

    for (name, setup, expected) in [
        (
            "status",
            Box::new(|fake: &mut Fake| {
                fake.application = Some(application("interview", 3, true));
            }) as Box<dyn Fn(&mut Fake)>,
            "TALENT_OFFER_SOURCE_CHANGED",
        ),
        (
            "version",
            Box::new(|fake: &mut Fake| {
                fake.application = Some(application("selected", 2, true));
            }),
            "TALENT_OFFER_SOURCE_CHANGED",
        ),
        (
            "proposal",
            Box::new(|fake: &mut Fake| {
                fake.application = Some(application("selected", 3, false));
            }),
            "TALENT_OFFER_SOURCE_CHANGED",
        ),
        (
            "candidate",
            Box::new(|fake: &mut Fake| {
                fake.application = Some(application("selected", 3, true));
                fake.application.as_mut().unwrap().candidate_status = "withdrawn".into();
            }),
            "TALENT_CANDIDATE_PROCESSING_BLOCKED",
        ),
    ] {
        let mut fake = Fake::ready();
        setup(&mut fake);
        let error = create_offer(
            &mut fake,
            &actor("recruiter"),
            &key(&format!("talent-offer-test-{name}")),
            &request(),
            TODAY,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, expected, "{name}");
    }

    let mut fake = Fake::ready();
    let mut input = request();
    input.expires_on = "2026-11-30".into();
    let error = create_offer(
        &mut fake,
        &actor("recruiter"),
        &key("talent-offer-test-dates"),
        &input,
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "INVALID_OFFER_DATES");

    let mut fake = Fake::ready();
    fake.money = Some(OfferMoneySnapshot {
        amount_units: 1,
        ..Fake::ready().money.unwrap()
    });
    let error = create_offer(
        &mut fake,
        &actor("recruiter"),
        &key("talent-offer-test-snapshot"),
        &request(),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "TALENT_OFFER_READBACK_INVALID");
}

fn change(action: OfferAction, version: i64) -> OfferChange {
    OfferChange {
        action,
        expected_version: version,
        reason: "Documented offer decision".into(),
        acceptance_evidence: None,
    }
}

#[tokio::test]
async fn offer_transitions_approve_send_accept_decline_and_withdraw() {
    let mut fake = Fake::ready();
    let approved = change_offer(
        &mut fake,
        &actor("reviewer"),
        &key("talent-offer-approve-0001"),
        OFFER,
        &change(OfferAction::ApproveOffer, 1),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(approved["status"], "approved");

    let self_approve = change_offer(
        &mut fake,
        &actor("creator"),
        &key("talent-offer-approve-0002"),
        OFFER,
        &change(OfferAction::ApproveOffer, 1),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(self_approve.code, "TALENT_INDEPENDENT_REVIEW_REQUIRED");

    fake.row = Some(offer_row("approved", 2, "creator"));
    let sent = change_offer(
        &mut fake,
        &actor("recruiter"),
        &key("talent-offer-send-0001"),
        OFFER,
        &change(OfferAction::Send, 2),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(sent["status"], "sent");

    let missing_evidence = change_offer(
        &mut fake,
        &actor("recruiter"),
        &key("talent-offer-accept-0001"),
        OFFER,
        &change(OfferAction::AcceptOffer, 2),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(missing_evidence.code, "ACCEPTANCE_EVIDENCE_REQUIRED");

    fake.row = Some(offer_row("sent", 3, "creator"));
    let mut accept = change(OfferAction::AcceptOffer, 3);
    accept.acceptance_evidence = Some("  Signed offer acceptance  ".into());
    let accepted = change_offer(
        &mut fake,
        &actor("recruiter"),
        &key("talent-offer-accept-0002"),
        OFFER,
        &accept,
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(accepted["status"], "accepted");
    assert_eq!(
        accepted["handoffId"],
        "talent-hire-talent-offer-accept-0002"
    );
    assert_eq!(
        fake.commits.lock().unwrap().last().map(String::as_str),
        Some("change:accept-offer:accepted:true")
    );

    fake.row = Some(offer_row("sent", 3, "creator"));
    let declined = change_offer(
        &mut fake,
        &actor("recruiter"),
        &key("talent-offer-decline-0001"),
        OFFER,
        &change(OfferAction::Decline, 3),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(declined["status"], "declined");

    fake.row = Some(offer_row("draft", 1, "creator"));
    let withdrawn = change_offer(
        &mut fake,
        &actor("recruiter"),
        &key("talent-offer-withdraw-0001"),
        OFFER,
        &change(OfferAction::Withdraw, 1),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(withdrawn["status"], "withdrawn");

    let invalid = change_offer(
        &mut fake,
        &actor("recruiter"),
        &key("talent-offer-withdraw-0002"),
        OFFER,
        &change(OfferAction::Send, 1),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(invalid.code, "TALENT_OFFER_CHANGED");

    fake.row = Some(offer_row("approved", 2, "creator"));
    fake.row.as_mut().unwrap().candidate_status = "erased".into();
    let blocked = change_offer(
        &mut fake,
        &actor("recruiter"),
        &key("talent-offer-send-0002"),
        OFFER,
        &change(OfferAction::Send, 2),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(blocked.code, "TALENT_CANDIDATE_PROCESSING_BLOCKED");
}

#[tokio::test]
async fn accepted_offer_handoff_contract_is_validated_and_readable() {
    let mut fake = Fake::ready();
    let view = hire_handoff_source(&mut fake, &actor("recruiter"), "talent-hire-1", NOW)
        .await
        .unwrap();
    assert_eq!(view["handoff"]["offer_status"], "accepted");
    assert_eq!(view["handoff"]["source_offer_version"], 4);
    assert_eq!(view["handoff"]["candidate_version"], 2);
    assert_eq!(view["handoff"]["base_amount_minor"], 4_500_050);

    fake.consumed = true;
    let queue = hire_handoff_queue(&mut fake, &actor("recruiter"), NOW)
        .await
        .unwrap();
    assert_eq!(queue["handoffs"][0]["consumed"], true);

    fake.handoff = Some(HireHandoffSource {
        offer_status: "sent".into(),
        ..handoff()
    });
    let not_accepted = hire_handoff_source(&mut fake, &actor("recruiter"), "talent-hire-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(not_accepted.code, "TALENT_CANDIDATE_PROCESSING_BLOCKED");

    fake.handoff = Some(HireHandoffSource {
        accepted_evidence: None,
        ..handoff()
    });
    let no_evidence = hire_handoff_source(&mut fake, &actor("recruiter"), "talent-hire-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(no_evidence.code, "TALENT_CANDIDATE_PROCESSING_BLOCKED");

    fake.handoff = Some(HireHandoffSource {
        source_offer_version: 3,
        ..handoff()
    });
    let stale = hire_handoff_source(&mut fake, &actor("recruiter"), "talent-hire-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(stale.code, "TALENT_CANDIDATE_PROCESSING_BLOCKED");

    fake.handoff = None;
    let missing = hire_handoff_source(&mut fake, &actor("recruiter"), "talent-hire-1", NOW)
        .await
        .unwrap_err();
    assert_eq!(missing.code, "HIRE_HANDOFF_NOT_FOUND");

    fake.handoff = Some(handoff());
    fake.authority = false;
    let stale_authority = hire_handoff_queue(&mut fake, &actor("recruiter"), NOW)
        .await
        .unwrap_err();
    assert_eq!(stale_authority.code, "TALENT_ACCESS_CHANGED");
}
