//! Owner-local command tests for the Manage ERPX onboarding bootstrap.
//!
//! These tests exercise the public application contract without a database:
//! validation, verified-Account admission, slot reservation/waitlist recovery,
//! current-owner matching, beta resource selection, idempotent replay,
//! incomplete provisioning and activation. The PostgreSQL adapter tests cover
//! the actual writes and transaction boundary.
use erpx_server_manage_erpx::public::onboarding::CompanySlotState;
use erpx_server_manage_erpx::public::onboarding_command::{
    OnboardingBootstrapOutcome, OnboardingBootstrapPlan, OnboardingBootstrapPort,
    OnboardingClaimOutcome, OnboardingCommandActor, OnboardingCommandInput,
    OnboardingCommandLimits, OnboardingCommandReceipt, OnboardingCommandResource,
    OnboardingCommandState, OnboardingControlPort, OnboardingReservation, OnboardingSlotFacts,
    activate_onboarding, provision_onboarding, reserve_onboarding,
};
use erpx_server_shared::public::{BusinessError, FailureKind, UserId};

fn actor(verified: bool) -> OnboardingCommandActor {
    OnboardingCommandActor {
        user_id: UserId::new("owner").unwrap(),
        display_name: "Owner".into(),
        email: "owner@example.test".into(),
        email_verified: verified,
    }
}

fn slot(state: CompanySlotState) -> OnboardingSlotFacts {
    OnboardingSlotFacts {
        holding_id: "h001".into(),
        slug: "acme".into(),
        company_name: "Acme Company".into(),
        branch_name: "Main".into(),
        state,
    }
}

fn resource(holding_id: &str, beta: bool) -> OnboardingCommandResource {
    OnboardingCommandResource {
        holding_id: holding_id.into(),
        write_binding: "H001_DB".into(),
        read_binding: "H001_READ_DB".into(),
        files_binding: "H001_FILES".into(),
        beta,
    }
}

fn limits() -> OnboardingCommandLimits {
    OnboardingCommandLimits {
        members: 20,
        branches: 5,
        ai: 100,
        storage: 1 << 30,
    }
}

#[derive(Default)]
struct Probe {
    owner: Option<OnboardingSlotFacts>,
    current: Option<OnboardingSlotFacts>,
    binding: bool,
    claim: Option<OnboardingClaimOutcome>,
    completed: bool,
    replayed: bool,
    calls: Vec<&'static str>,
    waitlisted: bool,
    bootstrap: Option<OnboardingBootstrapPlan>,
    activation:
        Option<erpx_server_manage_erpx::public::onboarding_command::OnboardingActivationPlan>,
}

impl OnboardingControlPort for Probe {
    type Error = BusinessError;

    async fn owner_slot(
        &mut self,
        user_id: &UserId,
    ) -> Result<Option<OnboardingSlotFacts>, Self::Error> {
        self.calls.push("owner_slot");
        assert_eq!(user_id.as_str(), "owner");
        Ok(self.owner.clone())
    }

    async fn current_owner_slot(
        &mut self,
        user_id: &UserId,
    ) -> Result<Option<OnboardingSlotFacts>, Self::Error> {
        self.calls.push("current_owner_slot");
        assert_eq!(user_id.as_str(), "owner");
        Ok(self.current.clone())
    }

    async fn slug_binding(&mut self, slug: &str) -> Result<bool, Self::Error> {
        self.calls.push("slug_binding");
        assert_eq!(slug, "acme");
        Ok(self.binding)
    }

    async fn claim_slot(
        &mut self,
        user_id: &UserId,
        input: &OnboardingCommandInput,
        now: i64,
    ) -> Result<OnboardingClaimOutcome, Self::Error> {
        self.calls.push("claim_slot");
        assert_eq!(user_id.as_str(), "owner");
        assert_eq!(input.company_name, "Acme Company");
        assert_eq!(input.slug, "acme");
        assert!(now > 0);
        Ok(self
            .claim
            .clone()
            .unwrap_or(OnboardingClaimOutcome::Unavailable))
    }

    async fn waitlist(&mut self, user_id: &UserId, now: i64) -> Result<(), Self::Error> {
        self.calls.push("waitlist");
        assert_eq!(user_id.as_str(), "owner");
        assert!(now > 0);
        self.waitlisted = true;
        Ok(())
    }

    async fn activate(
        &mut self,
        plan: &erpx_server_manage_erpx::public::onboarding_command::OnboardingActivationPlan,
    ) -> Result<(), Self::Error> {
        self.calls.push("activate");
        self.activation = Some(plan.clone());
        Ok(())
    }
}

impl OnboardingBootstrapPort for Probe {
    type Error = BusinessError;

    async fn bootstrap(
        &mut self,
        plan: &OnboardingBootstrapPlan,
    ) -> Result<OnboardingBootstrapOutcome, Self::Error> {
        self.calls.push("bootstrap");
        self.bootstrap = Some(plan.clone());
        Ok(OnboardingBootstrapOutcome {
            completed: self.completed,
            replayed: self.replayed,
        })
    }
}

fn run(
    probe: &mut Probe,
    verified: bool,
    enabled: bool,
) -> Result<OnboardingCommandReceipt, BusinessError> {
    sqlx::test_block_on(async {
        let reservation = reserve_onboarding(
            &actor(verified),
            enabled,
            "  Acme Company ",
            "Main",
            "acme",
            &[resource("h001", true)],
            limits(),
            1_700_000_000_000,
            probe,
        )
        .await?;
        match reservation {
            OnboardingReservation::Waitlisted => Ok(OnboardingCommandReceipt::waitlisted()),
            OnboardingReservation::Reserved { slot, resource } => {
                let receipt = provision_onboarding(
                    &actor(verified),
                    &slot,
                    limits(),
                    1_700_000_000_000,
                    probe,
                )
                .await?;
                activate_onboarding(&actor(verified), &slot, &resource, 1_700_000_000_000, probe)
                    .await?;
                Ok(receipt)
            }
        }
    })
}

#[test]
fn verified_owner_provisions_then_activates_only_its_derived_company() {
    let mut probe = Probe {
        owner: Some(slot(CompanySlotState::Provisioning)),
        completed: true,
        ..Probe::default()
    };
    let receipt = run(&mut probe, true, true).unwrap();
    assert_eq!(receipt.state, OnboardingCommandState::Active);
    assert_eq!(receipt.href.as_deref(), Some("/holdings/acme/home"));
    assert_eq!(receipt.value()["state"], "active");
    assert_eq!(probe.calls, ["owner_slot", "bootstrap", "activate"]);

    let plan = probe.bootstrap.unwrap();
    assert_eq!(plan.command_id, "onboarding-bootstrap-h001");
    assert_eq!(plan.company_id, "company-h001");
    assert_eq!(plan.branch_id, "branch-h001");
    assert_eq!(plan.department_id, "department-branch-h001");
    assert_eq!(plan.bootstrap_key, "bootstrap-h001");
    assert_eq!(plan.principal_id, "principal-owner");
    assert_eq!(plan.completed_at, plan.requested_at + 1);
    assert_eq!(plan.href, "/holdings/acme/home");
    assert!(plan.request_hash().contains("\"holdingId\":\"h001\""));
    assert!(
        plan.authority_json
            .contains("\"scope\":\"platform_account\"")
    );

    let activation = probe.activation.unwrap();
    assert_eq!(activation.holding_id, "h001");
    assert_eq!(activation.slug, "acme");
    assert_eq!(activation.company_name, "Acme Company");
    assert_eq!(activation.user_id.as_str(), "owner");
    assert_eq!(activation.write_binding, "H001_DB");
    assert_eq!(activation.read_binding, "H001_READ_DB");
    assert_eq!(activation.files_binding, "H001_FILES");
    assert!(!probe.waitlisted);
}

#[test]
fn unverified_or_closed_registration_is_denied_before_the_repository() {
    for (verified, enabled, code) in [(false, true, "VERIFY_EMAIL"), (true, false, "BETA_CLOSED")] {
        let mut probe = Probe::default();
        let error = run(&mut probe, verified, enabled).unwrap_err();
        assert_eq!(error.code, code);
        assert!(probe.calls.is_empty());
    }
}

#[test]
fn invalid_company_details_are_rejected_before_any_slot_read() {
    for (company, branch, slug) in [
        ("A", "Main", "acme"),
        ("Acme", "M", "acme"),
        ("Acme", "Main", "Ac"),
        ("Acme", "Main", "1acme"),
        ("Acme", "Main", "Acme"),
        ("Acme", "Main", "acme_"),
        ("Acme", "Main", &"a".repeat(41)),
    ] {
        let mut probe = Probe::default();
        let error = sqlx::test_block_on(reserve_onboarding(
            &actor(true),
            true,
            company,
            branch,
            slug,
            &[resource("h001", true)],
            limits(),
            1,
            &mut probe,
        ))
        .unwrap_err();
        assert_eq!(error.code, "VALIDATION_FAILED", "{company:?}/{slug:?}");
        assert_eq!(error.kind, FailureKind::InvalidInput);
        assert!(probe.calls.is_empty());
    }
    // Exactly 40 slug characters and 100 code-point names remain valid.
    assert!(
        OnboardingCommandInput::parse(
            &"ก".repeat(100),
            &"😀".repeat(100),
            &format!("a{}", "b".repeat(39)),
        )
        .is_ok()
    );
}

#[test]
fn a_taken_slug_never_claims_or_waitlists() {
    let mut probe = Probe {
        binding: true,
        ..Probe::default()
    };
    let error = run(&mut probe, true, true).unwrap_err();
    assert_eq!(error.code, "SLUG_UNAVAILABLE");
    assert_eq!(probe.calls, ["owner_slot", "slug_binding"]);
    assert!(!probe.waitlisted);
}

#[test]
fn no_available_slot_waitlists_while_a_claim_race_recovers_the_owner() {
    let mut waitlisted = Probe {
        claim: Some(OnboardingClaimOutcome::Unavailable),
        ..Probe::default()
    };
    assert_eq!(
        run(&mut waitlisted, true, true).unwrap(),
        OnboardingCommandReceipt::waitlisted()
    );
    assert!(waitlisted.waitlisted);
    assert_eq!(
        waitlisted.calls,
        [
            "owner_slot",
            "slug_binding",
            "claim_slot",
            "current_owner_slot",
            "waitlist"
        ]
    );

    let mut recovered = Probe {
        claim: Some(OnboardingClaimOutcome::Conflict),
        current: Some(slot(CompanySlotState::Provisioning)),
        completed: true,
        ..Probe::default()
    };
    assert_eq!(
        run(&mut recovered, true, true).unwrap().state,
        OnboardingCommandState::Active
    );
    assert!(!recovered.waitlisted);

    let mut conflicted = Probe {
        claim: Some(OnboardingClaimOutcome::Conflict),
        ..Probe::default()
    };
    assert_eq!(
        run(&mut conflicted, true, true).unwrap_err().code,
        "SLUG_UNAVAILABLE"
    );
    assert!(!conflicted.waitlisted);
}

#[test]
fn resuming_with_different_details_never_bootstraps_or_activates() {
    let mut probe = Probe {
        owner: Some(OnboardingSlotFacts {
            company_name: "Other Company".into(),
            ..slot(CompanySlotState::Provisioning)
        }),
        completed: true,
        ..Probe::default()
    };
    assert_eq!(
        run(&mut probe, true, true).unwrap_err().code,
        "ONBOARDING_ALREADY_STARTED"
    );
    assert_eq!(probe.calls, ["owner_slot"]);
    assert!(probe.bootstrap.is_none());
    assert!(probe.activation.is_none());
}

#[test]
fn only_a_matching_beta_resource_is_eligible() {
    let mut missing = Probe {
        owner: Some(slot(CompanySlotState::Provisioning)),
        completed: true,
        ..Probe::default()
    };
    assert_eq!(
        sqlx::test_block_on(reserve_onboarding(
            &actor(true),
            true,
            "Acme Company",
            "Main",
            "acme",
            &[],
            limits(),
            1,
            &mut missing,
        ))
        .unwrap_err()
        .code,
        "SLOT_UNAVAILABLE"
    );
    assert_eq!(missing.calls, ["owner_slot"]);

    let mut non_beta = Probe {
        owner: Some(slot(CompanySlotState::Provisioning)),
        completed: true,
        ..Probe::default()
    };
    assert_eq!(
        sqlx::test_block_on(reserve_onboarding(
            &actor(true),
            true,
            "Acme Company",
            "Main",
            "acme",
            &[resource("h002", true), resource("h001", false)],
            limits(),
            1,
            &mut non_beta,
        ))
        .unwrap_err()
        .code,
        "SLOT_UNAVAILABLE"
    );
    assert_eq!(non_beta.calls, ["owner_slot"]);
}

#[test]
fn an_incomplete_bootstrap_receipt_never_activates_the_slot() {
    let mut probe = Probe {
        owner: Some(slot(CompanySlotState::Provisioning)),
        completed: false,
        ..Probe::default()
    };
    let error = run(&mut probe, true, true).unwrap_err();
    assert_eq!(error.code, "PROVISIONING_INCOMPLETE");
    assert_eq!(error.kind, FailureKind::Unavailable);
    assert_eq!(probe.calls, ["owner_slot", "bootstrap"]);
    assert!(probe.activation.is_none());
}

#[test]
fn receipt_replay_is_idempotent_but_still_activates_the_control_plane() {
    let mut probe = Probe {
        owner: Some(slot(CompanySlotState::Provisioning)),
        completed: true,
        replayed: true,
        ..Probe::default()
    };
    let receipt = run(&mut probe, true, true).unwrap();
    assert_eq!(receipt.state, OnboardingCommandState::Active);
    assert_eq!(probe.calls, ["owner_slot", "bootstrap", "activate"]);
    assert!(probe.activation.is_some());
}

#[test]
fn an_available_or_malformed_source_slot_fails_closed() {
    let mut available = Probe {
        owner: Some(slot(CompanySlotState::Available)),
        completed: true,
        ..Probe::default()
    };
    assert_eq!(
        run(&mut available, true, true).unwrap_err().code,
        "ONBOARDING_SOURCE_INVALID"
    );
    assert_eq!(available.calls, ["owner_slot"]);

    assert!(
        OnboardingSlotFacts::from_row(
            "H001".into(),
            "acme".into(),
            "Acme".into(),
            "Main".into(),
            "provisioning"
        )
        .is_err()
    );
    assert!(
        OnboardingCommandResource {
            holding_id: "h001".into(),
            write_binding: "h001_db".into(),
            read_binding: "H001_READ_DB".into(),
            files_binding: "H001_FILES".into(),
            beta: true,
        }
        .validate()
        .is_err()
    );
}
