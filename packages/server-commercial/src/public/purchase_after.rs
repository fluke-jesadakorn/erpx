//! Post-effects purchase SQL decision; actor and transaction composition are separate.
pub use crate::adapters::postgres_purchase_after::{
    PurchaseAfterDecision, PurchaseShadowCheckedObservation, inspect_purchase_after,
    write_purchase_shadow_checked_observation,
};
