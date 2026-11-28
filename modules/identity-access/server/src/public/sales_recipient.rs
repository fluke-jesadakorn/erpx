//! Current scoped Identity facts for a Sales recipient; not an actor grant.
pub use crate::adapters::sales_recipient::{
    CurrentSalesAssignmentJoin, CurrentSalesIamBoundary, CurrentSalesRecipient,
    SalesRecipientLockPreparation, SalesRecipientRequest, SalesRecipientSourceLocks,
    inspect_sales_assignment_join, inspect_sales_iam_boundary, inspect_sales_recipient,
    lock_sales_recipient_sources,
};
