//! Raw Finance invoice discriminator; not a file-read authorization.
pub use crate::adapters::raw_field_invoice_marker::{
    CurrentPaymentReceiptFieldOrigin, CurrentRawFieldInvoiceMarker,
    inspect_payment_receipt_field_origin, inspect_raw_field_invoice_marker,
    lock_raw_field_invoice_marker_sources,
};
