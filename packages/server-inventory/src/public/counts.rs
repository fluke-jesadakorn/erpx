//! Native generic stock-count plans. Current IAM/Commercial authority, command
//! replay/audit, Finance journal and Files evidence stay at the composition
//! boundary; these exports never authorize or write by themselves.
pub use crate::domain::counts::{
    CountAllocationInput, CountLineInput, CountLot, CountLotEffect, CountProduct, CountRecord,
    CountSaveInput, CountSaveLinePlan, CountSaveLotPlan, CountSavePlan, CountSource, CountStock,
    CountStockEffect, CountTransitionEffects, CountTransitionLine, CountTransitionLot,
    CountTransitionPlan, CountWarehouse, MAX_ALLOCATIONS, MAX_LINES, MAX_REASON, count_action,
    count_status, plan_count_save, plan_count_transition, plan_count_transition_effects,
};

pub use crate::adapters::counts::{
    CountSeedKind, CountSeedPlan, CountSeedSource, apply_count_save, apply_count_transition,
    insert_count_seed_source, lock_count_sources, plan_count_seed, read_count_save_source,
    read_count_transition_source,
};
