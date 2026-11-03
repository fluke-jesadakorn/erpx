//! Scheduled Field reservation sources; no notification writes or personal grant.
pub use crate::adapters::reservation_alerts::{FieldReservationAlert,FieldReservationAlertPhase,CurrentFieldReservationAlerts,lock_field_reservation_alerts,read_field_reservation_alerts};

pub const CONTRACT_VERSION:&str="field-service.reservation-alerts.v1";
