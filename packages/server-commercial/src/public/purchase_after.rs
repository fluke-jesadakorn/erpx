//! Post-effects purchase SQL decision; actor and transaction composition are separate.
pub use crate::adapters::postgres_purchase_after::{inspect_purchase_after, PurchaseAfterDecision, PurchaseShadowCheckedObservation, write_purchase_shadow_checked_observation};
