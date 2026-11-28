//! Native Support SLA policy point/work HTTP ingress.
//!
//! Transport facts only: registered Host/Holding targets, bounded policy
//! selectors, no-selector GETs and business-error mapping. Account proof,
//! selected Support scope, `support.sla` purchase gates and the serializable
//! owner transaction belong to the composition backend. No SQL lives here.
use crate::adapters::support_http::{
    SupportHttpFuture, SupportHttpTargetSpec, configure_targets, error_response, no_query, ok,
    selected_target, validation_failed,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, Path, Request, Response, Router, State, routing::get, serde_json::Value,
};
use erpx_platform_shared_server::public::BusinessError;
use std::{collections::BTreeMap, sync::Arc};

pub struct SlaPolicyPointHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub id: &'a str,
}

pub struct SlaPolicyWorkHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

/// Composition-owned session/scope/transaction adapter. Both values are full
/// payloads produced by the owner SQL adapter; the router cannot shape them.
pub trait SlaPolicyReadHttpBackend: Send + Sync {
    fn point<'a>(
        &'a self,
        request: SlaPolicyPointHttpRequest<'a>,
    ) -> SupportHttpFuture<'a, Value>;

    fn work<'a>(&'a self, request: SlaPolicyWorkHttpRequest<'a>) -> SupportHttpFuture<'a, Value>;
}

pub struct SlaPolicyReadHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn SlaPolicyReadHttpBackend>,
}

impl SupportHttpTargetSpec for SlaPolicyReadHttpTarget {
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

struct SlaPolicyReadHttpState {
    targets: BTreeMap<(String, String), SlaPolicyReadHttpTarget>,
}

/// Router for the Support SLA policy point and policy-work reads. Composition
/// mounts this on the private native listener; the TypeScript authority is
/// retired only after the browser reaches these handlers directly.
pub fn sla_policy_read_http_router(
    targets: Vec<SlaPolicyReadHttpTarget>,
) -> Result<Router, BusinessError> {
    let configured = configure_targets(targets, "SUPPORT_SLA_POLICY_HTTP_CONFIGURATION_INVALID")?;
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/support/sla/policies/work",
            get(work),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/support/sla/policies/{id}",
            get(point),
        )
        .with_state(Arc::new(SlaPolicyReadHttpState {
            targets: configured,
        })))
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 200 && !value.chars().any(char::is_control)
}

async fn work(
    State(state): State<Arc<SlaPolicyReadHttpState>>,
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
        .work(SlaPolicyWorkHttpRequest {
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

async fn point(
    State(state): State<Arc<SlaPolicyReadHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let target = match selected_target(&state.targets, request.headers(), &slug) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    if let Err(error) = no_query(request.uri().query()) {
        return error_response(error);
    }
    if !valid_id(&id) {
        return error_response(validation_failed());
    }
    match target
        .backend
        .point(SlaPolicyPointHttpRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: request.headers(),
            id: &id,
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
        points: AtomicUsize,
        works: AtomicUsize,
    }

    impl SlaPolicyReadHttpBackend for Backend {
        fn point<'a>(
            &'a self,
            _request: SlaPolicyPointHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, Value> {
            self.points.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                Ok(serde_json::json!({"id":"support-policy-1","status":"draft","version":1}))
            })
        }

        fn work<'a>(
            &'a self,
            _request: SlaPolicyWorkHttpRequest<'a>,
        ) -> SupportHttpFuture<'a, Value> {
            self.works.fetch_add(1, Ordering::SeqCst);
            Box::pin(async { Ok(serde_json::json!([{"id":"support-policy-1","queue":"todo"}])) })
        }
    }

    fn target(backend: Arc<Backend>) -> SlaPolicyReadHttpTarget {
        SlaPolicyReadHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }
    }

    fn state(backend: Arc<Backend>) -> Arc<SlaPolicyReadHttpState> {
        Arc::new(SlaPolicyReadHttpState {
            targets: BTreeMap::from([(
                ("acme.example.test".to_owned(), "acme".to_owned()),
                target(backend),
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
    fn policy_reads_are_no_selector_gets() {
        let backend = Arc::new(Backend::default());
        let state = state(backend.clone());
        let response = block_on(work(
            State(state.clone()),
            Path("acme".into()),
            get("/support/sla/policies/work?id=support-policy-1"),
        ));
        assert_eq!(response.status(), 422);
        assert_eq!(backend.works.load(Ordering::SeqCst), 0);
        let response = block_on(work(
            State(state.clone()),
            Path("acme".into()),
            get("/support/sla/policies/work"),
        ));
        assert_eq!(response.status(), 200);
        assert_eq!(backend.works.load(Ordering::SeqCst), 1);
        let response = block_on(point(
            State(state.clone()),
            Path(("acme".into(), "support-policy-1".into())),
            get("/support/sla/policies/support-policy-1"),
        ));
        assert_eq!(response.status(), 200);
        assert_eq!(backend.points.load(Ordering::SeqCst), 1);
        let response = block_on(point(
            State(state),
            Path(("acme".into(), "support-policy-1".into())),
            get("/support/sla/policies/support-policy-1?id=other"),
        ));
        assert_eq!(response.status(), 422);
        assert_eq!(backend.points.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn duplicate_targets_are_rejected() {
        let backend = Arc::new(Backend::default());
        let error = match sla_policy_read_http_router(vec![
            target(backend.clone()),
            target(backend),
        ]) {
            Ok(_) => panic!("duplicate target accepted"),
            Err(error) => error,
        };
        assert_eq!(error.code, "SUPPORT_SLA_POLICY_HTTP_CONFIGURATION_INVALID");
    }
}
