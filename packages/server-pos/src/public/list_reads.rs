//! Native POS list-read kinds mounted in the owner statement registry.
//! The TypeScript list-read bridge sends these typed kinds with positional
//! values; the authoritative SQL is selected here. These re-exports let the
//! transport parity oracle assert byte equality with the runtime registry.
pub use crate::adapters::list_reads::{
    supported, sql, KINDS, CONNECTIONS, REFUNDS, SALES, SETTLEMENTS,
};
