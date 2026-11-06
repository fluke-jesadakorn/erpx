//! Scoped Sales recipient reference, not a selected actor or command grant.
pub use crate::adapters::recipient_native::{
    CurrentSalesRecipient, SalesRecipientRequest, inspect_sales_recipient,
    prepare_sales_recipient_sources,
};
pub use erpx_server_identity_access::public::sales_recipient::{
    SalesRecipientLockPreparation, SalesRecipientSourceLocks,
};
