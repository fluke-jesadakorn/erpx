//! Public surface for the native Operations caller-unit read-plan registry.
//! The command-plan router lives in `operations_http`; this facade exposes the
//! read kinds and resolver so composition can mount or verify the plan route
//! without importing adapter internals.
pub use crate::adapters::native_operations_read_sql::{
    is_read, read_statement, NativeReadStatement, RD_APP_UPLOAD_TASK_STATE,
    RD_TASK_PROJECTION_SOURCE,
};
