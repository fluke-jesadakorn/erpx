//! Owner-local HTTP ingress for the native R3 Manufacturing
//! `GET /production/posting-setup/prerequisites` branch. Transport facts only:
//! registered Host/Holding target, bounded private JSON, the retained
//! `Cache-Control` header and business-error mapping. Account session, Company
//! scope, Finance authority, purchased-package admission, locks, the
//! `contextKey` image and the owner SQL read stay behind the backend trait.
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Response, Router, State, StatusCode, header,
    routing::get, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type PrereqHttpFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct PrereqHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
}

/// The retained GET returns only `{data}` plus `Cache-Control`.
#[derive(Clone, Debug)]
pub struct PrereqHttpResponse {
    pub data: Value,
}

impl PrereqHttpResponse {
    pub fn value(self) -> Value {
        serde_json::json!({"data": self.data})
    }
}

pub trait PrereqHttpBackend: Send + Sync {
    fn prerequisites<'a>(&'a self, request: PrereqHttpRequest<'a>) -> PrereqHttpFuture<'a, Value>;
}

pub struct PrereqHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub backend: Arc<dyn PrereqHttpBackend>,
}

struct PrereqHttpState {
    targets: BTreeMap<(String, String), PrereqHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "MANUFACTURING_POSTING_REQUEST_INVALID",
        "Use one unambiguous Manufacturing posting prerequisites request.",
    )
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

pub fn prerequisites_router(targets: Vec<PrereqHttpTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "MANUFACTURING_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "MANUFACTURING_CONFIGURATION_INVALID",
            "Use exact distinct registered Holding targets.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/production/posting-setup/prerequisites",
            get(read_prerequisites),
        )
        .with_state(Arc::new(PrereqHttpState { targets: configured })))
}

fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn error_response(error: BusinessError) -> Response {
    let mut response = (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

fn bounded(value: Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

async fn read_prerequisites(
    State(state): State<Arc<PrereqHttpState>>,
    Path(slug): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let host = unique_header(&headers, "host")?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let target = state.targets.get(&(host, slug)).ok_or_else(missing)?;
        target
            .backend
            .prerequisites(PrereqHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
            })
            .await
    }
    .await;
    match result {
        Ok(data) => bounded(PrereqHttpResponse { data }.value()),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_is_exactly_data() {
        assert_eq!(
            PrereqHttpResponse { data: serde_json::json!({"charts":[]}) }.value(),
            serde_json::json!({"data": {"charts":[]}})
        );
    }

    #[test]
    fn router_requires_exact_distinct_registered_targets() {
        let target = || PrereqHttpTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            backend: Arc::new(Noop),
        };
        assert!(prerequisites_router(vec![target()]).is_ok());
        assert!(prerequisites_router(vec![target(), target()]).is_err());
        assert!(prerequisites_router(vec![]).is_err());
        assert!(prerequisites_router(vec![PrereqHttpTarget {
            host: "".into(),
            ..target()
        }])
        .is_err());
    }

    #[test]
    fn duplicate_headers_are_rejected() {
        let mut headers = HeaderMap::new();
        headers.append("host", HeaderValue::from_static("a.example"));
        headers.append("host", HeaderValue::from_static("b.example"));
        assert_eq!(unique_header(&headers, "host").unwrap_err().code, "MANUFACTURING_POSTING_REQUEST_INVALID");
    }

    struct Noop;

    impl PrereqHttpBackend for Noop {
        fn prerequisites<'a>(&'a self, _: PrereqHttpRequest<'a>) -> PrereqHttpFuture<'a, Value> {
            Box::pin(async { Ok(serde_json::json!({"charts":[],"accounts":[],"periods":[],"rates":[]})) })
        }
    }
}
