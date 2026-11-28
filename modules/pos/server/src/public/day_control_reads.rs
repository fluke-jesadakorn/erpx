//! Native POS day-control read kinds mounted in the owner statement registry.
//! The TypeScript day-control bridge sends these typed kinds with positional
//! values; the authoritative SQL is selected here. These re-exports let the
//! integration oracle assert byte equality with the runtime registry.
pub use crate::adapters::day_control_reads::{
    supported, sql, FAILED_JOBS, KINDS, MISSING_ROOM_CHARGES, OPEN_BILLS, OPEN_SHIFTS,
    PENDING_PAYMENTS, PENDING_REVERSALS, STOCK_MOVES, TOTALS, UNREVIEWED_VARIANCES,
    UNRESOLVED_REFUNDS,
};
