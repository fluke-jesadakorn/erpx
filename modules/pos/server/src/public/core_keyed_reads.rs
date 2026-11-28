//! Native POS keyed core read kinds mounted in the owner statement registry.
//! The TypeScript settings/receipt/day-close/reversal bridges send these typed
//! kinds with positional values; the authoritative SQL is selected here. These
//! re-exports let the integration oracle assert byte equality with the runtime
//! registry.
pub use crate::adapters::core_keyed_reads::{
    supported, sql, KINDS, PENDING_REVERSAL_REQUEST, PREPARED_DAY_CLOSE, RECEIPT_BY_ID,
    REVERSAL_BY_SALE, SALE_DOCUMENT, SETTINGS, SETTINGS_CURRENCY_ACCOUNTS,
    SETTINGS_PREPARED_STATE,
};
