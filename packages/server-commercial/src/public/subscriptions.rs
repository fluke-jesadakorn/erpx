//! Public Commercial subscription and grace/recovery reads.
//!
//! The caller supplies its own `&mut sqlx::PgConnection`; the reader opens no
//! transaction and returns only retained Company package facts.

pub use crate::adapters::subscription_sources::PgCompanySubscriptionReader;
pub use crate::application::subscriptions::{
    CompanySubscriptionReadPort, SubscriptionFacts, current_subscriptions,
};
pub use crate::domain::subscriptions::{
    CompanySubscription, CompanySubscriptionRead, GraceWindow, MAX_SUBSCRIPTION_HISTORY,
    MAX_SUBSCRIPTION_TRANSITIONS, MAX_SUBSCRIPTIONS, SubscriptionAction, SubscriptionState,
    SubscriptionTransition, company_subscriptions,
};
