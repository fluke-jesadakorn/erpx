//! Procurement-owned RFQ target snapshot; it grants no command permission.
pub use crate::adapters::rfq_target_source::{
    CurrentRfqTarget, RfqTargetFacts, lock_rfq_target_sources, observe_rfq_target,
};
