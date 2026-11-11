//! Native payroll run command HTTP ingress and owner-local router.
pub use crate::adapters::payroll_run_http::{
    PayrollCommandHttpBackend, PayrollCommandHttpFuture, PayrollCommandHttpKind,
    PayrollCommandHttpRequest, PayrollCommandHttpResponse, PayrollCommandHttpTarget,
    people_payroll_command_router,
};
