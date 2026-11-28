//! Native migration lane for the POS room-charge reversal queue read.
//!
//! The Rust owner executes the exact retired SQL via sqlx, owns the projection
//! and derives the complete review payload. TypeScript is a strict transport
//! that fails closed. Composition supplies only the authority/package decisions
//! and the Holding-scoped pool; no SQL or business rule stays in TypeScript.

pub mod application;
pub mod domain;
pub mod http;
pub mod postgres;

pub use application::{PosRoomChargeReader, RoomChargeFuture, read};
pub use domain::{
    ActorDecision, PackageDecision, ROW_KEYS, RoomChargeQuery, RoomChargeRow, forbidden,
    valid_row, validate,
};
pub use http::{
    RoomChargeTarget, router, router_for_holdings, router_with_reader, router_with_readers,
};
pub use postgres::{PgRoomChargeReader, ROOM_CHARGE_SQL, render};
