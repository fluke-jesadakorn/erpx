//! Current scoped Identity facts for a Sales recipient; not an actor grant.
pub use crate::adapters::sales_recipient::{
    CurrentSalesRecipient, SalesRecipientLockPreparation, SalesRecipientRequest,
    SalesRecipientSourceLocks, inspect_sales_recipient, lock_sales_recipient_sources,
    CurrentSalesAssignmentJoin, inspect_sales_assignment_join,
    CurrentSalesIamBoundary, inspect_sales_iam_boundary,
};
