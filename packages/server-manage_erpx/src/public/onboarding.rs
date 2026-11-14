//! Native Manage ERPX company-onboarding read API.
//!
//! This is the bounded company-lifecycle read slice. No onboarding command,
//! beta team mutation, public-origin policy or Platform operations are migrated
//! by this module.
pub use crate::adapters::onboarding::{PgOnboardingViewRead, read_onboarding_view};
pub use crate::application::onboarding::{OnboardingViewReadPort, onboarding_view};
pub use crate::domain::onboarding::{
    CompanySlot, CompanySlotState, OnboardingActor, OnboardingView,
};
