//! Native POS day-report business-date list read kinds mounted in the owner
//! statement registry. The TypeScript day-report bridge sends these typed kinds
//! with positional values; the authoritative SQL is selected here. These
//! re-exports let the integration oracle assert byte equality with the runtime
//! registry.
pub use crate::adapters::day_report_reads::{
    supported, sql, CASH_MOVEMENTS, DAY_CLOSES, FULFILLMENTS, KINDS, PAYMENTS, REFUNDS,
    REFUND_PAYMENTS, SALES, SETTLEMENTS, SHIFTS, STOCK_MOVES,
};
