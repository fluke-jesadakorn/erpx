//! Native POS purchase-boundary read kinds mounted in the owner statement
//! registry. The TypeScript purchase bridge sends these typed kinds with
//! positional values; the authoritative SQL is selected here. These re-exports
//! let the integration oracle assert byte equality with the runtime registry.
pub use crate::adapters::boundary_reads::{
    supported, sql, CASH_APPROVE_MODE, KINDS, PAYMENT_MODE, RECEIPT_REPLAY, REFUND_MODE,
    SALE_ADD_MODE, SALE_CREATE_MODE, SALE_MODE, SHIFT_MODE, SHIFT_OPEN_MODE,
};
