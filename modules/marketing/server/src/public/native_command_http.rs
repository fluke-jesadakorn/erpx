//! Public surface for the native marketing command HTTP ingress.
pub use crate::adapters::native_command_http::{
    NativeCommand, NativeCommandHttpBackend, NativeCommandHttpFuture, NativeCommandHttpRequest,
    NativeCommandHttpTarget, native_command_router, native_company_command_router,
};
