//! Manage ERPX company-onboarding command use case.
//!
//! The retained flow is deliberately retriable and split where the deployment
//! changes transaction domain:
//! 1. `reserve_onboarding` claims the verified Account's own reserved slot on
//!    the control plane; the claimed Holding selects the write database.
//! 2. `provision_onboarding` bootstraps the Company graph and retains the
//!    idempotent command receipt in one Holding transaction.
//! 3. `activate_onboarding` writes the owner principal/binding/billing and
//!    activates the slot on the control plane.
//!
//! `onboard_company` sequences the three when the caller already holds both
//! serializable transactions. Callers that must resolve the Holding pool after
//! the claim use the phase functions. This layer never opens, commits or
//! retries a transaction.
use crate::domain::onboarding::CompanySlotState;
use crate::domain::onboarding_command::{
    OnboardingActivationPlan, OnboardingBootstrapOutcome, OnboardingBootstrapPlan,
    OnboardingClaimOutcome, OnboardingCommandActor, OnboardingCommandInput,
    OnboardingCommandLimits, OnboardingCommandReceipt, OnboardingCommandResource,
    OnboardingSlotFacts,
};
use erpx_server_shared::public::{BusinessError, FailureKind, UserId};
use std::future::Future;

/// Reservation result: the owner either waits for a reserved slot or selects
/// the exact slot and beta resource that the Holding phase must use.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OnboardingReservation {
    Waitlisted,
    Reserved {
        slot: OnboardingSlotFacts,
        resource: OnboardingCommandResource,
    },
}

pub trait OnboardingControlPort: Send {
    type Error: From<BusinessError> + Send;

    fn owner_slot(
        &mut self,
        user_id: &UserId,
    ) -> impl Future<Output = Result<Option<OnboardingSlotFacts>, Self::Error>> + Send;

    fn current_owner_slot(
        &mut self,
        user_id: &UserId,
    ) -> impl Future<Output = Result<Option<OnboardingSlotFacts>, Self::Error>> + Send;

    fn slug_binding(
        &mut self,
        slug: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;

    fn claim_slot(
        &mut self,
        user_id: &UserId,
        input: &OnboardingCommandInput,
        now: i64,
    ) -> impl Future<Output = Result<OnboardingClaimOutcome, Self::Error>> + Send;

    fn waitlist(
        &mut self,
        user_id: &UserId,
        now: i64,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    fn activate(
        &mut self,
        plan: &OnboardingActivationPlan,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

pub trait OnboardingBootstrapPort: Send {
    type Error: From<BusinessError> + Send;

    fn bootstrap(
        &mut self,
        plan: &OnboardingBootstrapPlan,
    ) -> impl Future<Output = Result<OnboardingBootstrapOutcome, Self::Error>> + Send;
}

fn beta_closed() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "BETA_CLOSED",
        "Public registration is closed.",
    )
}

fn slug_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SLUG_UNAVAILABLE",
        "Choose another company URL.",
    )
}

fn already_started() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "ONBOARDING_ALREADY_STARTED",
        "Resume using the original company details.",
    )
}

fn slot_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SLOT_UNAVAILABLE",
        "Your reserved company is being prepared. Try again shortly.",
    )
}

fn provisioning_incomplete() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROVISIONING_INCOMPLETE",
        "Company setup is still being prepared. Try again shortly.",
    )
}

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ONBOARDING_SOURCE_INVALID",
        "The reserved company slot is unavailable. Retry the current onboarding state.",
    )
}

fn validate_configuration(
    resources: &[OnboardingCommandResource],
    limits: OnboardingCommandLimits,
) -> Result<(), BusinessError> {
    for resource in resources {
        resource.validate()?;
    }
    limits.validate()
}

/// Phase 1. Claim or recover the verified Account's own slot. The caller commits
/// this control transaction only after activation, so a failed Holding phase
/// cannot publish an owner slot without its receipt.
#[allow(clippy::too_many_arguments)]
pub async fn reserve_onboarding<C: OnboardingControlPort>(
    actor: &OnboardingCommandActor,
    enabled: bool,
    company_name: &str,
    branch_name: &str,
    slug: &str,
    resources: &[OnboardingCommandResource],
    limits: OnboardingCommandLimits,
    now: i64,
    control: &mut C,
) -> Result<OnboardingReservation, C::Error> {
    actor.require_verified()?;
    let input = OnboardingCommandInput::parse(company_name, branch_name, slug)?;
    if !enabled {
        return Err(beta_closed().into());
    }
    validate_configuration(resources, limits)?;

    let mut slot = control.owner_slot(&actor.user_id).await?;
    if slot.is_none() {
        if control.slug_binding(&input.slug).await? {
            return Err(slug_unavailable().into());
        }
        match control.claim_slot(&actor.user_id, &input, now).await? {
            OnboardingClaimOutcome::Claimed(claimed) => slot = Some(claimed),
            OnboardingClaimOutcome::Conflict => {
                // A concurrent claim may already have bound this owner's slot.
                slot = control.current_owner_slot(&actor.user_id).await?;
                if slot.is_none() {
                    return Err(slug_unavailable().into());
                }
            }
            OnboardingClaimOutcome::Unavailable => {
                slot = control.current_owner_slot(&actor.user_id).await?;
                if slot.is_none() {
                    control.waitlist(&actor.user_id, now).await?;
                    return Ok(OnboardingReservation::Waitlisted);
                }
            }
        }
    }
    let slot = slot.ok_or_else(source_invalid)?;
    if !slot.matches(&input) {
        return Err(already_started().into());
    }
    if slot.state == CompanySlotState::Available {
        return Err(source_invalid().into());
    }
    let resource = resources
        .iter()
        .find(|resource| resource.holding_id == slot.holding_id && resource.beta)
        .ok_or_else(slot_unavailable)?
        .clone();
    Ok(OnboardingReservation::Reserved { slot, resource })
}

/// Phase 2. Bootstrap the Company graph and retain the idempotent receipt on
/// the selected Holding write transaction. Returns `active` only after the
/// completed provisioning receipt is visible in that transaction.
pub async fn provision_onboarding<B: OnboardingBootstrapPort>(
    actor: &OnboardingCommandActor,
    slot: &OnboardingSlotFacts,
    limits: OnboardingCommandLimits,
    now: i64,
    holding: &mut B,
) -> Result<OnboardingCommandReceipt, B::Error> {
    actor.require_verified()?;
    limits.validate()?;
    let plan = OnboardingBootstrapPlan::new(slot, actor, limits, now);
    plan.validate()?;
    let outcome = holding.bootstrap(&plan).await?;
    if !outcome.completed {
        return Err(provisioning_incomplete().into());
    }
    Ok(OnboardingCommandReceipt::active(&slot.slug))
}

/// Phase 3. Activate the owner principal, Holding binding, beta billing and
/// slot on the control plane. This is idempotent and must commit last.
pub async fn activate_onboarding<C: OnboardingControlPort>(
    actor: &OnboardingCommandActor,
    slot: &OnboardingSlotFacts,
    resource: &OnboardingCommandResource,
    now: i64,
    control: &mut C,
) -> Result<(), C::Error> {
    actor.require_verified()?;
    let plan = OnboardingActivationPlan::new(slot, actor, resource, now);
    plan.validate()?;
    control.activate(&plan).await
}

/// Convenience sequencing for callers that already hold both serializable
/// transactions. The caller must commit the Holding bootstrap before the
/// control activation. This is a single atomic unit; the deployment flow uses
/// the three phase functions so the control reservation commits before the
/// Holding database is selected, preserving the retained retry point.
#[allow(clippy::too_many_arguments)]
pub async fn onboard_company<C, B>(
    actor: &OnboardingCommandActor,
    enabled: bool,
    company_name: &str,
    branch_name: &str,
    slug: &str,
    resources: &[OnboardingCommandResource],
    limits: OnboardingCommandLimits,
    now: i64,
    control: &mut C,
    holding: &mut B,
) -> Result<OnboardingCommandReceipt, C::Error>
where
    C: OnboardingControlPort,
    B: OnboardingBootstrapPort<Error = C::Error>,
{
    match reserve_onboarding(
        actor,
        enabled,
        company_name,
        branch_name,
        slug,
        resources,
        limits,
        now,
        control,
    )
    .await?
    {
        OnboardingReservation::Waitlisted => Ok(OnboardingCommandReceipt::waitlisted()),
        OnboardingReservation::Reserved { slot, resource } => {
            let receipt = provision_onboarding(actor, &slot, limits, now, holding).await?;
            activate_onboarding(actor, &slot, &resource, now, control).await?;
            Ok(receipt)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::onboarding_command::OnboardingCommandState;

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

    fn resource(beta: bool) -> OnboardingCommandResource {
        OnboardingCommandResource {
            holding_id: "h001".into(),
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
        activation: Option<OnboardingActivationPlan>,
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

        async fn activate(&mut self, plan: &OnboardingActivationPlan) -> Result<(), Self::Error> {
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
                &[resource(true)],
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
                    activate_onboarding(
                        &actor(verified),
                        &slot,
                        &resource,
                        1_700_000_000_000,
                        probe,
                    )
                    .await?;
                    Ok(receipt)
                }
            }
        })
    }

    #[test]
    fn verified_owner_bootstraps_then_activates_the_derived_company() {
        let mut probe = Probe {
            owner: Some(slot(CompanySlotState::Provisioning)),
            completed: true,
            ..Probe::default()
        };
        let receipt = run(&mut probe, true, true).unwrap();
        assert_eq!(receipt.state, OnboardingCommandState::Active);
        assert_eq!(receipt.href.as_deref(), Some("/holdings/acme/home"));
        assert_eq!(probe.calls, ["owner_slot", "bootstrap", "activate"]);
        assert_eq!(
            probe.bootstrap.as_ref().unwrap().command_id,
            "onboarding-bootstrap-h001"
        );
        let activation = probe.activation.as_ref().unwrap();
        assert_eq!(activation.holding_id, "h001");
        assert_eq!(activation.slug, "acme");
        assert_eq!(activation.user_id.as_str(), "owner");
        assert_eq!(activation.write_binding, "H001_DB");
        assert!(!probe.waitlisted);
    }

    #[test]
    fn unverified_or_closed_registration_is_denied_before_the_repository() {
        for (verified, enabled) in [(false, true), (true, false)] {
            let mut probe = Probe::default();
            let error = run(&mut probe, verified, enabled).unwrap_err();
            assert!(
                error.code == "VERIFY_EMAIL" || error.code == "BETA_CLOSED",
                "unexpected {}",
                error.code
            );
            assert!(probe.calls.is_empty());
        }
    }

    #[test]
    fn invalid_body_is_rejected_before_claiming_a_slot() {
        let mut probe = Probe::default();
        let error = sqlx::test_block_on(reserve_onboarding(
            &actor(true),
            true,
            "A",
            "Main",
            "acme",
            &[resource(true)],
            limits(),
            1,
            &mut probe,
        ))
        .unwrap_err();
        assert_eq!(error.code, "VALIDATION_FAILED");
        assert!(probe.calls.is_empty());
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
    fn an_unavailable_slot_recovers_or_waitlists() {
        let mut probe = Probe {
            claim: Some(OnboardingClaimOutcome::Unavailable),
            ..Probe::default()
        };
        let receipt = run(&mut probe, true, true).unwrap();
        assert_eq!(receipt, OnboardingCommandReceipt::waitlisted());
        assert_eq!(
            probe.calls,
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
        assert_eq!(
            recovered.calls,
            [
                "owner_slot",
                "slug_binding",
                "claim_slot",
                "current_owner_slot",
                "bootstrap",
                "activate"
            ]
        );

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
    fn different_company_details_resume_only_the_original_slot() {
        let mut probe = Probe {
            owner: Some(OnboardingSlotFacts {
                company_name: "Other Company".into(),
                ..slot(CompanySlotState::Provisioning)
            }),
            ..Probe::default()
        };
        assert_eq!(
            run(&mut probe, true, true).unwrap_err().code,
            "ONBOARDING_ALREADY_STARTED"
        );
        assert_eq!(probe.calls, ["owner_slot"]);
        assert!(probe.bootstrap.is_none());
    }

    #[test]
    fn a_non_beta_or_missing_resource_never_bootstraps() {
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
                &[resource(false)],
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
        assert_eq!(
            run(&mut probe, true, true).unwrap_err().code,
            "PROVISIONING_INCOMPLETE"
        );
        assert_eq!(probe.calls, ["owner_slot", "bootstrap"]);
        assert!(probe.activation.is_none());
    }

    #[test]
    fn replay_still_activates_without_replanning_the_company() {
        let mut probe = Probe {
            owner: Some(slot(CompanySlotState::Provisioning)),
            completed: true,
            replayed: true,
            ..Probe::default()
        };
        let receipt = run(&mut probe, true, true).unwrap();
        assert_eq!(receipt.state, OnboardingCommandState::Active);
        assert_eq!(probe.calls, ["owner_slot", "bootstrap", "activate"]);
        assert!(probe.bootstrap.unwrap().validate().is_ok());
    }

    #[test]
    fn an_available_source_row_is_never_treated_as_an_owned_company() {
        let mut probe = Probe {
            owner: Some(slot(CompanySlotState::Available)),
            completed: true,
            ..Probe::default()
        };
        assert_eq!(
            run(&mut probe, true, true).unwrap_err().code,
            "ONBOARDING_SOURCE_INVALID"
        );
        assert_eq!(probe.calls, ["owner_slot"]);
    }

    #[test]
    fn only_the_matching_beta_resource_is_activated() {
        let mut probe = Probe {
            owner: Some(slot(CompanySlotState::Provisioning)),
            completed: true,
            ..Probe::default()
        };
        let resources = [
            OnboardingCommandResource {
                holding_id: "h002".into(),
                ..resource(true)
            },
            OnboardingCommandResource {
                holding_id: "h003".into(),
                ..resource(true)
            },
            resource(true),
        ];
        run_with_resources(&mut probe, &resources);
        assert_eq!(probe.activation.unwrap().holding_id, "h001");
    }

    fn run_with_resources(probe: &mut Probe, resources: &[OnboardingCommandResource]) {
        sqlx::test_block_on(async {
            let reservation = reserve_onboarding(
                &actor(true),
                true,
                "Acme Company",
                "Main",
                "acme",
                resources,
                limits(),
                1,
                probe,
            )
            .await
            .unwrap();
            let OnboardingReservation::Reserved { slot, resource } = reservation else {
                panic!("expected a reserved slot");
            };
            let receipt = provision_onboarding(&actor(true), &slot, limits(), 1, probe)
                .await
                .unwrap();
            activate_onboarding(&actor(true), &slot, &resource, 1, probe)
                .await
                .unwrap();
            assert_eq!(receipt.state, OnboardingCommandState::Active);
        })
    }

    #[test]
    fn the_convenience_command_sequences_the_three_phases() {
        struct ControlProbe(Probe);
        impl OnboardingControlPort for ControlProbe {
            type Error = BusinessError;
            async fn owner_slot(
                &mut self,
                user_id: &UserId,
            ) -> Result<Option<OnboardingSlotFacts>, Self::Error> {
                self.0.owner_slot(user_id).await
            }
            async fn current_owner_slot(
                &mut self,
                user_id: &UserId,
            ) -> Result<Option<OnboardingSlotFacts>, Self::Error> {
                self.0.current_owner_slot(user_id).await
            }
            async fn slug_binding(&mut self, slug: &str) -> Result<bool, Self::Error> {
                self.0.slug_binding(slug).await
            }
            async fn claim_slot(
                &mut self,
                user_id: &UserId,
                input: &OnboardingCommandInput,
                now: i64,
            ) -> Result<OnboardingClaimOutcome, Self::Error> {
                self.0.claim_slot(user_id, input, now).await
            }
            async fn waitlist(&mut self, user_id: &UserId, now: i64) -> Result<(), Self::Error> {
                self.0.waitlist(user_id, now).await
            }
            async fn activate(
                &mut self,
                plan: &OnboardingActivationPlan,
            ) -> Result<(), Self::Error> {
                self.0.activate(plan).await
            }
        }
        struct BootstrapProbe(Probe);
        impl OnboardingBootstrapPort for BootstrapProbe {
            type Error = BusinessError;
            async fn bootstrap(
                &mut self,
                plan: &OnboardingBootstrapPlan,
            ) -> Result<OnboardingBootstrapOutcome, Self::Error> {
                self.0.bootstrap(plan).await
            }
        }

        let mut control = ControlProbe(Probe {
            owner: Some(slot(CompanySlotState::Provisioning)),
            ..Probe::default()
        });
        let mut holding = BootstrapProbe(Probe {
            completed: true,
            ..Probe::default()
        });
        let receipt = sqlx::test_block_on(onboard_company(
            &actor(true),
            true,
            "Acme Company",
            "Main",
            "acme",
            &[resource(true)],
            limits(),
            1,
            &mut control,
            &mut holding,
        ))
        .unwrap();
        assert_eq!(receipt.state, OnboardingCommandState::Active);
        assert_eq!(control.0.calls, ["owner_slot", "activate"]);
        assert_eq!(holding.0.calls, ["bootstrap"]);
    }
}
