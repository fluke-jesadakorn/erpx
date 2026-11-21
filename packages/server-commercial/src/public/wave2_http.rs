//! Native Commercial wave2 grant-save axum command ingress.
pub use crate::adapters::wave2_http::{
    GrantSaveHttpBackend, GrantSaveHttpFuture, GrantSaveHttpRequest, GrantSaveHttpResponse,
    GrantSaveHttpTarget, commercial_grant_save_router,
};
