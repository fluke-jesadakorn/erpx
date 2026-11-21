//! Native Finance payment allocation and independent reversal.
pub use crate::adapters::payment_correction::{
    PaymentAllocationInput, PaymentReversalInput, allocate_payment,
    lock_payment_correction_sources, reverse_payment,
};
