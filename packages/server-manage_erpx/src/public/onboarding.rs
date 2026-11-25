//! Native Manage ERPX company-onboarding read API.
//!
//! This is the bounded company-lifecycle read slice. No onboarding command,
//! beta team mutation, public-origin policy or Platform operations are migrated
//! by this module.
pub use crate::adapters::onboarding::{read_onboarding_view, PgOnboardingViewRead};
pub use crate::application::onboarding::{onboarding_view, OnboardingViewReadPort};
pub use crate::domain::onboarding::{
    CompanySlot, CompanySlotState, OnboardingActor, OnboardingView,
};
