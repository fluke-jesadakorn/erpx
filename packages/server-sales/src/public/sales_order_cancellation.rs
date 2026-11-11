//! Native Sales confirmed sales-order cancellation.
pub use crate::adapters::sales_order_cancellation::PgSalesCancellation;
pub use crate::application::sales_order_cancellation::{
    CancellationAuditEntry, CancellationOutboxEntry, CancellationReceiptEntry,
    SalesCancellationFuture, SalesCancellationRepository, cancel_confirmed_sales_order,
};
pub use crate::domain::sales_order_cancellation::{
    CancellationReceiptRecord, OPERATION, SalesCancellationActor, SalesCancellationCommand,
    SalesCancellationPlan, SalesCancellationReceipt, SalesCancellationRow,
    authorize_confirmed_cancellation, confirmed_required, request_invalid, sale_required,
    validate_readback, validate_receipt_shape,
};
