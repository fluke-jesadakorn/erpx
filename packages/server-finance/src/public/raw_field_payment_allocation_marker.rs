//! Global Finance allocation discriminator; not an authorization grant.
pub use crate::adapters::raw_field_payment_allocation_marker::{
    CurrentDirectFieldPaymentOrigin, inspect_direct_field_payment_origin,
};
pub use crate::adapters::raw_field_payment_allocation_marker::{
    CurrentFieldPaymentOrigin, CurrentRawFieldPaymentAllocationMarker,
    inspect_field_payment_origin, inspect_raw_field_payment_allocation_marker,
    lock_raw_field_payment_allocation_sources,
};
pub use crate::adapters::raw_field_payment_allocation_marker::{
    CurrentFieldPaymentOriginBatch, inspect_field_payment_origin_batch,
};
