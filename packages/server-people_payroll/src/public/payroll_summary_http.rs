//! Owner-local HTTP ingress for native Payroll summary, run-list and Work-row reads.
pub use crate::adapters::payroll_summary_http::{
    PayrollReadHttpBackend, PayrollReadHttpFuture, PayrollReadHttpKind, PayrollReadHttpRequest,
    PayrollReadHttpResponse, PayrollReadHttpTarget, people_payroll_router,
};
