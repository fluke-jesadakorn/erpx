//! Owner-local HTTP ingress for the native R3 supplier-bill workspace read.
//!
//! Transport facts only: registered Host/Holding target, exact browser origin,
//! bounded private JSON and business-error mapping. Account session resolution,
//! current Company scope, commercial purchased-package admission and the owner
//! SQL read remain composition responsibilities supplied through the backend
//! trait. The router never mints a session, actor, Company or purchase and never
//! executes SQL itself.
use erpx_platform_runtime_server::public::http_support::{
    header, routing::get, HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response,
    Router, State, StatusCode,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type SupplierBillWorkspaceHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct SupplierBillWorkspaceHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct SupplierBillWorkspaceHttpResponse {
    /// Exact retained `supplierBillWorkspace` data: the projected bill rows.
    /// The router never reshapes or invents any field.
    pub data: serde_json::Value,
}

impl SupplierBillWorkspaceHttpResponse {
    /// The retained route returns only `{data}` (plus `Cache-Control`); no
    /// `context` object is added here so the envelope stays byte-compatible.
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data})
    }
}

pub trait SupplierBillWorkspaceHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: SupplierBillWorkspaceHttpRequest<'a>,
    ) -> SupplierBillWorkspaceHttpFuture<'a, SupplierBillWorkspaceHttpResponse>;
}

pub struct SupplierBillWorkspaceHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn SupplierBillWorkspaceHttpBackend>,
}

struct SupplierBillWorkspaceHttpState {
    targets: BTreeMap<(String, String), SupplierBillWorkspaceHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PROCUREMENT_REQUEST_INVALID",
        "Use one unambiguous Procurement supplier-bill workspace request.",
    )
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
    })
}

pub fn supplier_bill_workspace_router(
    targets: Vec<SupplierBillWorkspaceHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "PROCUREMENT_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/supplier-bills",
            get(read_workspace),
        )
        .with_state(Arc::new(SupplierBillWorkspaceHttpState {
            targets: configured,
        })))
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
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

fn target<'a>(
    state: &'a SupplierBillWorkspaceHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a SupplierBillWorkspaceHttpTarget, BusinessError> {
    let host = unique_header(headers, "host")?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

async fn read_workspace(
    State(state): State<Arc<SupplierBillWorkspaceHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() {
            return Err(request_invalid());
        }
        let target = target(&state, request.headers(), &slug)?;
        target
            .backend
            .read(SupplierBillWorkspaceHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: request.headers(),
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_is_exactly_data() {
        let reply = SupplierBillWorkspaceHttpResponse {
            data: serde_json::json!([{"id": "bill-1"}]),
        };
        assert_eq!(
            reply.value(),
            serde_json::json!({"data": [{"id": "bill-1"}]})
        );
    }

    #[test]
    fn invalid_origin_is_rejected() {
        assert!(!valid_origin("http://acme.jesadakorn.com"));
        assert!(!valid_origin("https://acme.jesadakorn.com/evil"));
        assert!(valid_origin("https://acme.jesadakorn.com"));
    }
}
