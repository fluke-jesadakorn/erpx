//! Field-origin invoice posting decisions. Current authority/source witnesses
//! and transaction ownership must be retained by the native composition.
pub use crate::domain::service_invoice_posting::{
    ServiceInvoicePostFacts, require_service_invoice_poster, service_invoice_post_lines,
    validate_service_invoice_post_request,
};

pub use crate::adapters::service_invoice_posting::{
    CurrentServiceInvoicePostSource, RetainedServiceInvoicePost,
    prepare_service_invoice_post_source, service_invoice_post_runtime,
};

pub use crate::adapters::service_invoice_post_wire::{
    normalize_service_invoice_post, service_invoice_post_physical_input,
};
