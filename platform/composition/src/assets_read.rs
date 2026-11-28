//! Composition for the native Assets read-plan route.
//! Standalone module; the lead mounts the router and injects this backend.
use erpx_assets_server::public::native_read_http::{
    NativeReadPlanBackend, NativeReadPlanFuture, NativeReadPlanRequest, NativeReadPlanTarget,
    native_assets_read_router, plan,
};
use erpx_platform_runtime_server::public::http_support::Router;
use erpx_platform_shared_server::public::BusinessError;
use std::sync::Arc;

/// Pure owner backend: a closed owner read kind and bounded caller scope resolve
/// to one exact static statement from the frozen plan registry. No database
/// access, no caller SQL and no TypeScript fallback. The caller executes the
/// returned statement on its own unit with its own snapshot.
pub struct AssetsReadPlanBackend;

impl NativeReadPlanBackend for AssetsReadPlanBackend {
    fn plan<'a>(&'a self, request: NativeReadPlanRequest<'a>) -> NativeReadPlanFuture<'a> {
        let result = plan(request.kind, request.scope);
        Box::pin(async move { result })
    }
}

pub fn assets_read_composed_router(
    host: impl Into<String>,
    holding_slug: impl Into<String>,
    origin: impl Into<String>,
) -> Result<Router, BusinessError> {
    native_assets_read_router(vec![NativeReadPlanTarget {
        host: host.into(),
        holding_slug: holding_slug.into(),
        origin: origin.into(),
        backend: Arc::new(AssetsReadPlanBackend),
    }])
}

/// Mount every registered Holding target on one router. The caller supplies
/// only exact registered host/slug/origin triples; the owner backend is fixed.
pub fn assets_read_all_composed_router(
    targets: Vec<(String, String, String)>,
) -> Result<Router, BusinessError> {
    native_assets_read_router(
        targets
            .into_iter()
            .map(|(host, holding_slug, origin)| NativeReadPlanTarget {
                host,
                holding_slug,
                origin,
                backend: Arc::new(AssetsReadPlanBackend),
            })
            .collect(),
    )
}
