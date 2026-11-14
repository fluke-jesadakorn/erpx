//! Inventory allocation facts; Commerce must separately attest scoped origin.
//!
//! All matching allocations are returned without an artificial row cap. These
//! rows carry no Company or branch; Commerce must retain the scoped actual
//! fulfillment, order, and return sources before using the resulting facts.
pub use crate::adapters::quality_fulfillment_allocation::{
    CurrentQualityFulfillmentAllocations, QualityFulfillmentAllocation,
    QualityFulfillmentAllocationRequest, inspect_quality_fulfillment_allocations,
    lock_quality_fulfillment_allocations,
};
