//! Native POS attachment source read kinds mounted in the owner statement
//! registry. The TypeScript attachment source bridge sends these typed kinds
//! with positional values; the authoritative SQL is selected here. These
//! re-exports let the integration oracle assert byte equality with the runtime
//! registry.
pub use crate::adapters::attachment_read_sources::{supported, sql, KINDS, POS_REFUND, POS_SALE, POS_SHIFT};
