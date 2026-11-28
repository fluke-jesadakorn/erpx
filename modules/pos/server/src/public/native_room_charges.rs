//! Public owner port for the native POS room-charge reversal queue read.
//!
//! Stable re-exports for composition and the Next transport. No TypeScript
//! implementation backs this operation; the pool-backed owner reader is the
//! authority and the router is mounted by composition.

pub use crate::adapters::native_room_charges::postgres::Bind;
pub use crate::adapters::native_room_charges::{
    ActorDecision, PackageDecision, PgRoomChargeReader, PosRoomChargeReader, ROOM_CHARGE_SQL,
    RoomChargeFuture, RoomChargeQuery, RoomChargeRow, RoomChargeTarget, ROW_KEYS, read, render,
    router, router_for_holdings, router_with_reader, router_with_readers, valid_row, validate,
};
