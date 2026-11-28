//! Native standalone sales-invoice edit and void.
pub use crate::adapters::sales_invoice_edit::{
    SalesInvoiceEditInput, SalesInvoiceEditLine, SalesInvoiceEditResult, SalesInvoiceVoidInput,
    SalesInvoiceVoidResult, edit_sales_invoice, lock_sales_invoice_edit_sources,
    void_sales_invoice,
};
