//! Native Support SLA workspace HTTP ingress.
//!
//! Transport facts only: registered Host/Holding targets, no-selector GET and
//! business-error mapping. Account proof, selected Support scope, `support.sla`
//! purchase gates and the serializable owner transaction belong to the
//! composition backend. No SQL or composition import lives here.
use crate::adapters::support_http::{
    SupportHttpFuture, SupportHttpTargetSpec, configure_targets, error_response, no_query, ok,
    selected_target,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, Path, Request, Response, Router, State, routing::get, serde_json::Value,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{collections::BTreeMap, sync::Arc};

pub struct SlaWorkspaceHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

/// Composition-owned session/scope/transaction adapter. The returned value is
/// the complete SLA workspace payload, produced by the owner SQL adapter.
pub trait SlaWorkspaceHttpBackend: Send + Sync {
    fn workspace<'a>(&'a self, request: SlaWorkspaceHttpRequest<'a>)
        -> SupportHttpFuture<'a, Value>;
}

pub struct SlaWorkspaceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn SlaWorkspaceHttpBackend>,
}

impl SupportHttpTargetSpec for SlaWorkspaceHttpTarget {
    fn host(&self) -> &str {
        &self.host
    }
    fn holding_slug(&self) -> &str {
        &self.holding_slug
    }
    fn origin(&self) -> &str {
        &self.origin
    }
}

struct SlaWorkspaceHttpState {
    targets: BTreeMap<(String, String), SlaWorkspaceHttpTarget>,
}

/// Router for `GET /api/v1/holdings/{holding_slug}/support/sla`. Composition
/// mounts this on the private native listener; the public Next route is retired
/// only after the browser reaches this handler directly.
pub fn sla_workspace_http_router(
    targets: Vec<SlaWorkspaceHttpTarget>,
) -> Result<Router, BusinessError> {
    let configured = configure_targets(targets, "SUPPORT_SLA_HTTP_CONFIGURATION_INVALID")?;
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/support/sla",
            get(workspace),
        )
        .with_state(Arc::new(SlaWorkspaceHttpState {
            targets: configured,
        })))
}

async fn workspace(
    State(state): State<Arc<SlaWorkspaceHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let target = match selected_target(&state.targets, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    if let Err(error) = no_query(request.uri().query()) {
        return error_response(error);
    }
    match target
        .backend
        .workspace(SlaWorkspaceHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: request.headers(),
        })
        .await
    {
        Ok(value) => ok(value),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::support_http::block_on;
    use erpx_platform_runtime_server::public::http_support::body::Body;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct Backend {
        calls: AtomicUsize,
    }

    impl SlaWorkspaceHttpBackend for Backend {
        fn workspace<'a>(
            &'a self,
            _request: SlaWorkspaceHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, Value> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(serde_json::json!({"alerts":[],"items":[],"canManage":true})) })
        }
    }

    fn state(backend: Arc<Backend>) -> Arc<SlaWorkspaceHttpState> {
        Arc::new(SlaWorkspaceHttpState {
            targets: BTreeMap::from([(
                ("acme.example.test".to_owned(), "acme".to_owned()),
                SlaWorkspaceHttpTarget {
                    host: "acme.example.test".into(),
                    holding_slug: "acme".into(),
                    origin: "https://acme.example.test".into(),
                    backend,
                },
            )]),
        })
    }

    fn get(uri: &str) -> Request {
        Request::builder()
            .uri(uri)
            .header("host", "acme.example.test")
            .body(Body::empty())
            .unwrap()
    }

    #[test]
    fn sla_workspace_is_a_no_selector_read() {
        let backend = Arc::new(Backend::default());
        let state = state(backend.clone());
        let response = block_on(workspace(
            State(state.clone()),
            Path("acme".into()),
            get("/support/sla?ticketId=ticket-1"),
        ));
        assert_eq!(response.status(), 422);
        assert_eq!(backend.calls.load(Ordering::SeqCst), 0);
        let response = block_on(workspace(
            State(state),
            Path("acme".into()),
            get("/support/sla"),
        ));
        assert_eq!(response.status(), 200);
        assert_eq!(backend.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn target_config_is_distinct_and_authoritative() {
        let target = SlaWorkspaceHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend: Arc::new(Backend::default()),
        };
        assert!(sla_workspace_http_router(vec![target]).is_ok());
        let bad = SlaWorkspaceHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "http://acme.example.test".into(),
            backend: Arc::new(Backend::default()),
        };
        assert!(sla_workspace_http_router(vec![bad]).is_err());
    }
}
