//! Native Manage ERPX company-onboarding command API.
//!
//! This is the bounded POST onboarding/bootstrap slice. It retires the retained
//! TypeScript `onboard()` business path once the composition mount and workplace
//! route selection are integrated. The existing native onboarding GET read is
//! unchanged and not duplicated here.
pub use crate::adapters::onboarding_command::{
    PgOnboardingBootstrap, PgOnboardingCommand, PgOnboardingControl,
};
pub use crate::application::onboarding_command::{
    OnboardingBootstrapPort, OnboardingControlPort, OnboardingReservation, activate_onboarding,
    onboard_company, provision_onboarding, reserve_onboarding,
};
pub use crate::domain::onboarding_command::{
    OnboardingActivationPlan, OnboardingBootstrapOutcome, OnboardingBootstrapPlan,
    OnboardingClaimOutcome, OnboardingCommandActor, OnboardingCommandInput,
    OnboardingCommandLimits, OnboardingCommandReceipt, OnboardingCommandResource,
    OnboardingCommandState, OnboardingSlotFacts,
};
