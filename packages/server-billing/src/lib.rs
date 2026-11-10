//! Billing owner. The qualified native slice reads the current Holding's
//! billing account, subscription, plans, metered costs, invoices, audit and
//! pending jobs, and performs the bounded platform cost-review commands plus
//! the local invoice lifecycle. Stripe subscription/checkout commands and
//! background jobs remain in the retained TypeScript owner until their native
//! provider and worker slices are implemented.
mod adapters;
mod application;
mod domain;
pub mod public;
