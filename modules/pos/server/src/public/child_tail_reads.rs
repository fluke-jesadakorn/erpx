//! Native POS child-tail read kinds mounted in the owner statement registry.
//! The TypeScript child-tail bridge sends these typed kinds with positional
//! values; the authoritative SQL is selected here. These re-exports let the
//! transport parity oracle assert byte equality with the runtime registry.
pub use crate::adapters::child_tail_reads::{
    supported, sql, KINDS, COMPLETED_LINE, CREATED_BY, LINES_SUM, REFUND_PAYMENT,
};
