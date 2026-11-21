//! Public surface for the native crm command HTTP ingress.
pub use crate::adapters::native_command_http::{
    NativeCommand, NativeCommandHttpBackend, NativeCommandHttpFuture, NativeCommandHttpRequest,
    NativeCommandHttpTarget, native_command_router,
};
