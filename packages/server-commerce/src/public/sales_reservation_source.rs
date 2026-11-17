//! Scoped order/line source only; never an actor or entitlement grant.
pub use crate::adapters::legacy_fulfillment::legacy_reservation_workflow_input;
pub use crate::adapters::sales_reservation_source::{
    SalesReservationScope, SalesReservationSource, lock_sales_reservation_source,
};
