//! Composition for the native Commercial owner-read route.
//! Standalone module; the lead mounts the router and injects this backend.
use erpx_server_commercial::public::owner_read::{invalid, statement};
use erpx_server_commercial::public::owner_read_http::{
    OwnerReadBackend, OwnerReadFuture, OwnerReadResponse, OwnerReadTarget,
    commercial_owner_read_router,
};
use erpx_server_runtime::public::http_support::Router;
use erpx_server_shared::public::BusinessError;
use std::sync::Arc;

/// Pure owner backend: a closed read kind resolves to one exact static SELECT.
/// No database access, no caller SQL and no TypeScript fallback. The caller
/// executes the returned SELECT on its own unit with its own locks.
pub struct CommercialOwnerReadBackend;

impl OwnerReadBackend for CommercialOwnerReadBackend {
    fn read<'a>(&'a self, kind: &'a str) -> OwnerReadFuture<'a, OwnerReadResponse> {
        Box::pin(async move {
            let sql = statement(kind).ok_or_else(invalid)?;
            Ok(OwnerReadResponse {
                data: serde_json::json!({"sql": sql}),
            })
        })
    }
}

pub fn commercial_owner_read_composed_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
) -> Result<Router, BusinessError> {
    commercial_owner_read_router(vec![OwnerReadTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend: Arc::new(CommercialOwnerReadBackend),
    }])
}
