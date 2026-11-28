//! Public surface for the native Manufacturing read-plan HTTP transport.
//! The axum ingress lives in `native_command_http`; this facade exposes the
//! target/backend contract and router builder so composition can mount the
//! exact Holding-target route without importing adapter internals.
pub use crate::adapters::native_command_http::{
    NativeReadPlanBackend, NativeReadPlanFuture, NativeReadPlanRequest, NativeReadPlanScope,
    NativeReadPlanTarget, native_manufacturing_read_router, plan,
};
