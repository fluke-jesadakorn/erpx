//! Native POS Stripe settlement/provider read kinds mounted in the owner
//! statement registry. The TypeScript Stripe bridge sends these typed kinds
//! with positional values; the authoritative SQL is selected here. These
//! re-exports let the integration oracle assert byte equality with the runtime
//! registry.
pub use crate::adapters::stripe_reads::{
    supported, sql, KINDS, PAYMENT_BY_ID, PAYMENT_ID, REFUND_BY_ID, REFUND_ID_BY_PROVIDER,
    SETTLEMENT_BALANCE_ROW, SETTLEMENT_BALANCE_STATE, SETTLEMENT_BY_KEY, SETTLEMENT_PAYOUT_JOURNAL,
};
