//! Stable owner facade for the native Field part-reservation read slice.
//!
//! Composition mounts `field_part_reservation_router` with one
//! `PgFieldPartReservationStore` per registered Holding connection pool. The
//! router performs transport checks only; the store executes the real SQL.
pub use crate::adapters::reservation_part_http::{
    FieldPartReservationTarget, field_part_reservation_router,
};
pub use crate::adapters::reservation_part_source::PgFieldPartReservationStore;
pub use crate::application::reservation_part::{
    FieldPartReservationQuery, FieldPartReservationStore, project,
};
pub use crate::domain::reservation_part::{FieldPartReservation, RESERVATION_KEYS};

pub const CONTRACT_VERSION: &str = "field-service.part-reservation.v1";
