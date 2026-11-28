//! Owner-local Records registry HTTP ingress.
pub use crate::adapters::record_registry_http::{
    RecordsRegistryHttpBackend, RecordsRegistryHttpCommand, RecordsRegistryHttpFuture,
    RecordsRegistryHttpRequest, RecordsRegistryHttpResponse, RecordsRegistryHttpTarget,
    records_registry_http_router,
};
