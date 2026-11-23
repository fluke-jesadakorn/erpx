//! Native POS core workspace read kinds mounted in the owner statement
//! registry. The TypeScript shifts/tables/cash-movement bridge sends these
//! typed kinds with positional values; the authoritative SQL is selected here.
//! These re-exports let the integration oracle assert byte equality with the
//! runtime registry.
pub use crate::adapters::core_reads::{
    supported, sql, CASH_MOVEMENTS, KINDS, SHIFTS, TABLES,
};
