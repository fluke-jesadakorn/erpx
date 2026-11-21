//! Scoped Sales recipient reference, not a selected actor or command grant.
//!
//! This port composes Identity authority facts with the People
//! `workforce_reference` port. Callers must lock sources before the first
//! snapshot; see `prepare_sales_recipient_sources`.
pub use crate::adapters::recipient_native::{
    CurrentSalesRecipient, SalesRecipientRequest, inspect_sales_recipient,
    prepare_sales_recipient_sources,
};
pub use erpx_server_identity_access::public::sales_recipient::{
    SalesRecipientLockPreparation, SalesRecipientSourceLocks,
};
