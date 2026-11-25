//! Composition for the native private-file denial authority witness.
//!
//! The owner opens one Holding transaction per witness request through
//! [`PgPrivateFileDenial`]; this module owns no SQL and makes no authority
//! decision. The lead mounts the router on exact registered targets.
use erpx_server_identity_access::public::private_file_denial::{
    PgPrivateFileDenial, private_file_denial_witness,
};
use erpx_server_identity_access::public::private_file_denial_http::{
    PrivateFileDenialHttpBackend, PrivateFileDenialHttpFuture, PrivateFileDenialHttpRequest,
    PrivateFileDenialHttpResponse, PrivateFileDenialHttpTarget, private_file_denial_http_router,
};
use erpx_server_runtime::public::http_support::Router;
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::BusinessError;
use std::sync::Arc;

pub struct PrivateFileDenialBackend {
    holding: DatabasePool,
}

impl PrivateFileDenialBackend {
    pub fn new(holding: DatabasePool) -> Self {
        Self { holding }
    }
}

impl PrivateFileDenialHttpBackend for PrivateFileDenialBackend {
    fn execute<'a>(
        &'a self,
        request: PrivateFileDenialHttpRequest<'a>,
    ) -> PrivateFileDenialHttpFuture<'a, PrivateFileDenialHttpResponse> {
        Box::pin(async move {
            let mut repository = PgPrivateFileDenial::new(self.holding.clone());
            let witness = private_file_denial_witness(&mut repository, request.actor).await?;
            Ok(PrivateFileDenialHttpResponse { witness })
        })
    }
}

pub fn private_file_denial_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
    holding: DatabasePool,
) -> Result<Router, BusinessError> {
    private_file_denial_http_router(vec![PrivateFileDenialHttpTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend: Arc::new(PrivateFileDenialBackend::new(holding)),
    }])
}
