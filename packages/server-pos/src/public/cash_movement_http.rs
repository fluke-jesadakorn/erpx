//! Owner-local HTTP ingress for the native POS cash-movement command.
pub use crate::adapters::cash_movement_http::{
    CashMovementHttpBackend, CashMovementHttpFuture, CashMovementHttpRequest,
    CashMovementHttpResponse, CashMovementHttpState, CashMovementHttpTarget, cash_movement_router,
};
