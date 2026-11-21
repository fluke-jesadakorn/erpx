//! Native standalone customer-receipt detail, export and issue.
pub use crate::adapters::sales_receipts::{
    SalesReceiptExport, SalesReceiptIssueInput, SalesReceiptIssueResult, SalesReceiptRead,
    export_customer_receipt, issue_customer_receipt, lock_sales_receipt_sources,
    read_customer_receipt,
};
