//! Public axum ingress for native people-payroll write commands.
pub use crate::adapters::native_command_http::{
    NativeCommandHttpBackend, NativeCommandHttpFuture, NativeCommandHttpRequest,
    NativeCommandHttpResponse, NativeCommandHttpTarget, native_command_company_router,
    native_command_router,
};
