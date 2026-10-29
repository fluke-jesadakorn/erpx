//! Count review decision evidence; callers provide current Inventory, Commerce,
//! actor and purchase witnesses and retain them in the same transaction.
pub use crate::adapters::service_count_decision_evidence::{
    PlannedServiceCountDecision, ServiceCountDecisionFacts, lock_service_count_decision_evidence,
    service_count_decision_digest,
};
