//! Native POS sale-child detail read kinds mounted in the owner statement
//! registry. The TypeScript sale-child bridge sends these typed kinds with one
//! positional sale id; the authoritative SQL is selected here. These re-exports
//! let the integration oracle assert byte equality with the runtime registry.
pub use crate::adapters::sale_child_reads::{
    supported, sql, DAY_PLANNER_LINES, DAY_PLANNER_PAYMENTS, KINDS, LINES_ALL,
    LINES_FULFILLMENT_DETAIL, LINES_TOTAL, LINES_WITH_FULFILLMENT, PAYMENT_FIRST_ID, PAYMENT_ID,
    PAYMENT_OPEN_ID, PAYMENT_PENDING_ID, PAYMENTS_ORDERED, PAYMENTS_REFUNDABLE,
    PAYMENTS_SUCCEEDED_TOTAL,
};
