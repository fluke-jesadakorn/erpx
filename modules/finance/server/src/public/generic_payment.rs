//! Native controlled customer-receipt and supplier-payment preparation.
pub use crate::adapters::generic_payment::{
    GenericPaymentInput, PreparedGenericPayment, RetainedGenericPayment,
    lock_generic_payment_sources, prepare_generic_payment,
};
