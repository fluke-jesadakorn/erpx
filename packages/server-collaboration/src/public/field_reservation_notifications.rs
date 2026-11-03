//! Finite native Field scheduled notification contract.
pub use crate::adapters::field_reservation_notifications::{FieldReservationNotice,PgFieldReservationNoticeSource,write_field_reservation_notices,RetainedFieldReservationNotices};

pub const CONTRACT_VERSION:&str="collaboration.field-reservation-notices.v1";
