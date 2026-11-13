//! Native generic stock-count plans. Current IAM/Commercial authority, command
//! replay/audit, Finance journal and Files evidence stay at the composition
//! boundary; these exports never authorize or write by themselves.
pub use crate::domain::counts::{
    CountAllocationInput, CountLineInput, CountLot, CountProduct, CountRecord, CountSaveInput,
    CountSaveLinePlan, CountSaveLotPlan, CountSavePlan, CountSource, CountStock,
    CountTransitionPlan, CountWarehouse, MAX_ALLOCATIONS, MAX_LINES, MAX_REASON, count_action,
    count_status, plan_count_save, plan_count_transition,
};

pub use crate::adapters::counts::{
    apply_count_save, lock_count_sources, read_count_save_source,
};
