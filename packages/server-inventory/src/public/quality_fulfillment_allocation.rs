//! Inventory allocation facts; Commerce must separately attest scoped origin.
pub use crate::adapters::quality_fulfillment_allocation::{
    CurrentQualityFulfillmentAllocations, QualityFulfillmentAllocation,
    QualityFulfillmentAllocationRequest, inspect_quality_fulfillment_allocations,
    lock_quality_fulfillment_allocations,
};
