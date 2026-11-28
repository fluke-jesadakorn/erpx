//! Stable owner ports for the native PMS PromptPay payment-status read.
//!
//! Composition mounts the router with exact registered Holding targets and its
//! own Account/IAM authority and provider reconcile; this module mints no
//! authority and issues no SQL outside the sqlx adapter re-exported here.
pub use crate::adapters::payment_status_http::{
    PmsPaymentStatusHttpBackend, PmsPaymentStatusHttpFuture, PmsPaymentStatusHttpTarget,
    PmsPaymentStatusRequest, pms_payment_status_router,
};
pub use crate::adapters::payment_status_postgres::{
    PAYMENT_STATUS_SQL, PgPmsPaymentStatusRead,
};
pub use crate::application::payment_status::{
    PmsPaymentStatusFuture, PmsPaymentStatusRepository, pms_payment_status,
};
pub use crate::domain::payment_status::{
    PmsPaymentStatusFacts, PmsPaymentStatusView, PaymentStatusRow, not_found, project,
    read_invalid, request_invalid, validate,
};
