//! Opaque actual any-active-hold facts; not an Inventory reservation grant.
pub use crate::adapters::field_service_reservation::{
    CurrentFieldServiceReservationHolds, lock_field_service_reservation_holds,
    read_field_service_reservation_holds,
};
