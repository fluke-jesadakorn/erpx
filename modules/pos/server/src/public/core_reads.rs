//! Native POS core workspace read kinds mounted in the owner statement
//! registry. The TypeScript shifts/register/table bridge sends these typed
//! kinds with positional values; the authoritative SQL is selected here.
//! These re-exports let the integration oracle assert byte equality with the
//! runtime registry.
pub use crate::adapters::core_reads::{
    supported, sql, CASH_MOVEMENTS, KINDS, OPEN_SHIFT_BY_ID, OPEN_SHIFT_BY_REGISTER,
    REGISTER_BY_NAME, REGISTERS_BY_KIND, RESTAURANT_REGISTER_PROPERTY, SHIFTS, TABLES,
    TABLE_BY_NAME,
};
