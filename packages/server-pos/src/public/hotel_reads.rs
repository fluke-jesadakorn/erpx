//! Native POS hotel day-assertion property kinds mounted in the owner statement
//! registry. The TypeScript hotel-read bridge sends these typed kinds with
//! positional values; the authoritative SQL is selected here. These re-exports
//! let the transport parity oracle assert byte equality with the runtime
//! registry.
pub use crate::adapters::hotel_reads::{
    supported, sql, FULFILLMENT_PROPERTY, KINDS, REGISTER_PROPERTY,
};
