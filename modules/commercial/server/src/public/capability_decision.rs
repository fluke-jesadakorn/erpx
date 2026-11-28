//! Public Commercial entitlement decision port.
//!
//! Consumers obtain one effective decision or grouped purchase access over
//! their own `&mut sqlx::PgConnection`. The reader never opens a transaction,
//! mints authority, or supplies release/record gates.

pub use crate::adapters::capability_decision_postgres::PgCapabilityDecisionReader;
pub use crate::application::capability_decision::{
    CapabilityCheck, CapabilityDecisionObservation, CapabilityDecisionPort, CapabilityScope,
    MAX_CAPABILITIES, MAX_CAPABILITY_BYTES, MAX_GRANTS, MAX_HISTORY_RIGHTS, PurchasedAccess,
    PurchasedAccessCodes, check_purchased_capability, decide_effective_capability,
    purchased_access, purchased_accesses, require_purchased_capability, validate_capability,
    validate_purchase_facts,
};
pub use crate::domain::capability_decision::{
    CapabilityDecisionCode, CapabilityGate, CapabilityRevisions, EffectiveCapabilityDecision,
    EffectiveCapabilityInput, capability_revision, capability_revision_source,
    effective_capability_decision, purchased_decision,
};
