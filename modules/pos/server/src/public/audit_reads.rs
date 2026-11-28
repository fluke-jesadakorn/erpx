//! Native POS audit actor read kinds mounted in the owner statement registry.
//! The TypeScript audit bridge sends these typed kinds with positional record
//! ids; the authoritative SQL is selected here. These re-exports let the
//! integration oracle assert byte equality with the runtime registry.
pub use crate::adapters::audit_reads::{
    supported, sql, AUDIT_ACTOR_CHARGE_TO_ROOM, AUDIT_ACTOR_KITCHEN_START, KINDS,
};
